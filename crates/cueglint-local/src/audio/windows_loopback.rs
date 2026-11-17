//! A distinct render-endpoint source; the native worker owns every COM object.
use super::{AudioPacket, PACKET_FRAMES, Source};
use anyhow::{Context, Result, bail, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{SyncSender, TrySendError},
};

#[cfg(target_os = "windows")]
pub(super) use native::SystemCapture;

#[derive(Clone, Copy)]
enum Encoding {
    Float32,
    Pcm16,
    Pcm24,
    Pcm32,
}

impl Encoding {
    fn bytes(self) -> usize {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Float32 | Self::Pcm32 => 4,
        }
    }

    fn sample(self, bytes: &[u8]) -> Result<f32> {
        let value = match self {
            Self::Float32 => f32::from_le_bytes(bytes.try_into()?),
            Self::Pcm16 => i16::from_le_bytes(bytes.try_into()?) as f32 / 32768.0,
            Self::Pcm24 => {
                let value = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
                value as f32 / 8_388_608.0
            }
            Self::Pcm32 => i32::from_le_bytes(bytes.try_into()?) as f32 / 2_147_483_648.0,
        };
        ensure!(value.is_finite(), "nonfinite Windows system audio sample");
        Ok(value)
    }
}

#[derive(Clone, Copy)]
struct MixFormat {
    encoding: Encoding,
    channels: usize,
    sample_rate: u32,
    block_align: usize,
}

impl MixFormat {
    fn new(
        float: bool,
        bits: u16,
        valid_bits: u16,
        channels: u16,
        sample_rate: u32,
        block_align: u16,
        byte_rate: u32,
    ) -> Result<Self> {
        let encoding = match (float, bits) {
            (true, 32) => Encoding::Float32,
            (false, 16) => Encoding::Pcm16,
            (false, 24) => Encoding::Pcm24,
            (false, 32) => Encoding::Pcm32,
            _ => bail!("unsupported Windows system audio format: float={float}, bits={bits}"),
        };
        ensure!(
            valid_bits > 0 && valid_bits <= bits && (!float || valid_bits == bits),
            "invalid Windows system audio precision"
        );
        ensure!(
            (1..=32).contains(&channels) && (8_000..=192_000).contains(&sample_rate),
            "unsupported Windows system audio channels or sample rate"
        );
        ensure!(
            block_align as usize == channels as usize * encoding.bytes()
                && byte_rate == sample_rate * u32::from(block_align),
            "invalid Windows system audio block alignment or byte rate"
        );
        Ok(Self {
            encoding,
            channels: channels as usize,
            sample_rate,
            block_align: block_align as usize,
        })
    }

    fn packet_bytes(self, frames: u32, buffer_frames: u32) -> Result<usize> {
        ensure!(
            buffer_frames > 0
                && buffer_frames <= self.sample_rate * 2
                && frames > 0
                && frames <= buffer_frames,
            "invalid Windows system audio buffer frame count"
        );
        (frames as usize)
            .checked_mul(self.block_align)
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .context("Windows system audio buffer size overflow")
    }
}

/// # Safety
/// A nonsilent pointer must hold `len` readable bytes for the returned slice's lifetime.
unsafe fn packet_data<'a>(data: *const u8, len: usize, silent: bool) -> Result<Option<&'a [u8]>> {
    ensure!(
        len > 0 && len <= isize::MAX as usize,
        "invalid audio byte count"
    );
    if silent {
        return Ok(None);
    }
    ensure!(!data.is_null(), "null Windows system audio buffer");
    // SILENT buffers are deliberately never read, even when Windows supplies a pointer.
    Ok(Some(unsafe { std::slice::from_raw_parts(data, len) }))
}

struct PacketWriter {
    format: MixFormat,
    buffer_frames: u32,
    sequence: u64,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl PacketWriter {
    fn write(&mut self, data: Option<&[u8]>, frames: u32, start_ns: u64) -> Result<()> {
        let bytes = self.format.packet_bytes(frames, self.buffer_frames)?;
        ensure!(
            data.is_none_or(|data| data.len() == bytes),
            "truncated Windows audio buffer"
        );
        start_ns
            .checked_add(u64::from(frames) * 1_000_000_000 / u64::from(self.format.sample_rate))
            .context("Windows audio timestamp overflow")?;
        for offset in (0..frames as usize).step_by(PACKET_FRAMES) {
            if self.stop.load(Ordering::Relaxed) || self.muted.load(Ordering::Relaxed) {
                break;
            }
            let len = PACKET_FRAMES.min(frames as usize - offset);
            let mut packet = AudioPacket {
                source: Source::SystemAudio,
                sequence: self.sequence,
                start_ns: start_ns + offset as u64 * 1_000_000_000 / self.format.sample_rate as u64,
                sample_rate: self.format.sample_rate,
                len,
                samples: [0.0; PACKET_FRAMES],
            };
            self.sequence = self
                .sequence
                .checked_add(1)
                .context("Windows audio sequence overflow")?;
            if let Some(data) = data {
                let first = offset * self.format.block_align;
                let data = &data[first..first + len * self.format.block_align];
                for (out, frame) in packet
                    .samples
                    .iter_mut()
                    .zip(data.chunks_exact(self.format.block_align))
                {
                    let mut sum = 0.0_f64;
                    for sample in frame.chunks_exact(self.format.encoding.bytes()) {
                        sum += f64::from(self.format.encoding.sample(sample)?);
                    }
                    *out = (sum / self.format.channels as f64) as f32;
                }
            }
            match self.tx.try_send(packet) {
                Ok(()) => (),
                Err(TrySendError::Full(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {
                    bail!("Windows audio packet receiver disconnected")
                }
            }
        }
        Ok(())
    }
}

struct PacketClock {
    offset_ns: i128,
    next_device_position: Option<u64>,
    last_qpc: Option<u64>,
}

impl PacketClock {
    fn new(before: i64, after: i64, frequency: i64, elapsed_ns: u64) -> Result<Self> {
        ensure!(
            before >= 0 && after >= before && frequency > 0,
            "invalid Windows performance clock"
        );
        ensure!(
            i128::from(after - before) * 1_000_000_000 / i128::from(frequency) <= 1_000_000,
            "Windows audio clock calibration was interrupted; resume to retry"
        );
        let counter = before + (after - before) / 2;
        Ok(Self {
            offset_ns: i128::from(elapsed_ns)
                - i128::from(counter) * 1_000_000_000 / i128::from(frequency),
            next_device_position: None,
            last_qpc: None,
        })
    }

    fn start_ns(
        &mut self,
        device_position: u64,
        qpc_100ns: u64,
        frames: u32,
        discontinuity: bool,
        timestamp_error: bool,
        elapsed_ns: u64,
    ) -> Result<u64> {
        ensure!(
            !timestamp_error,
            "Windows system audio timestamp is uncertain"
        );
        // The first discontinuity flag is not meaningful; later gaps stop rather than join audio.
        ensure!(
            self.next_device_position
                .is_none_or(|next| !discontinuity && next == device_position),
            "Windows system audio discontinuity; resume to reconnect the source"
        );
        ensure!(
            self.last_qpc.is_none_or(|last| qpc_100ns > last),
            "Windows audio clock moved backwards"
        );
        let start_ns = u64::try_from(i128::from(qpc_100ns) * 100 + self.offset_ns)
            .context("Windows audio timestamp precedes capture or overflows")?;
        ensure!(
            start_ns <= elapsed_ns.saturating_add(1_000_000),
            "Windows audio timestamp is in the future"
        );
        self.next_device_position = Some(
            device_position
                .checked_add(u64::from(frames))
                .context("Windows audio device position overflow")?,
        );
        self.last_qpc = Some(qpc_100ns);
        Ok(start_ns)
    }
}

fn with_cleanup(result: Result<()>, cleanup: Result<()>) -> Result<()> {
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("cleanup also failed: {cleanup:#}")))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use std::{
        sync::mpsc,
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Media::{
            Audio::{
                AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
                AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient,
                IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX,
                WAVEFORMATEXTENSIBLE, eConsole, eRender,
            },
            KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE},
            Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT},
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
    };

    pub(crate) struct SystemCapture {
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<Result<()>>>,
    }

    impl SystemCapture {
        pub(crate) fn start(
            tx: SyncSender<AudioPacket>,
            dropped: Arc<AtomicU64>,
            errors: Arc<AtomicU64>,
            muted: Arc<AtomicBool>,
            origin: Instant,
        ) -> Result<Self> {
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let (ready, started) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("cueglint-wasapi-loopback".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(tx, dropped, muted, stopped, origin, ready)
                    }))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!("Windows system audio worker panicked"))
                    });
                    if result.is_err() {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                    result
                })
                .context("starting Windows system audio worker")?;
            let mut capture = Self {
                stop,
                worker: Some(worker),
            };
            if started.recv().is_err() {
                capture
                    .stop()
                    .context("initializing Windows system audio")?;
                bail!("Windows system audio worker exited before startup");
            }
            Ok(capture)
        }

        pub(crate) fn stop(&mut self) -> Result<()> {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                worker.thread().unpark();
                // Native calls cannot be interrupted; capture shutdown stays off the UI thread.
                return worker.join().unwrap_or_else(|_| {
                    Err(anyhow::anyhow!("Windows system audio worker panicked"))
                });
            }
            Ok(())
        }
    }

    impl Drop for SystemCapture {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    struct MixAllocation(*mut WAVEFORMATEX);
    impl Drop for MixAllocation {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }

    struct Buffer<'a>(&'a IAudioCaptureClient, u32);
    impl Buffer<'_> {
        fn release(mut self) -> Result<()> {
            let frames = std::mem::replace(&mut self.1, 0);
            unsafe { self.0.ReleaseBuffer(frames) }.context("releasing Windows system audio buffer")
        }
    }
    impl Drop for Buffer<'_> {
        fn drop(&mut self) {
            if self.1 != 0 {
                let _ = unsafe { self.0.ReleaseBuffer(self.1) };
            }
        }
    }

    struct StartedClient<'a>(&'a IAudioClient, bool);
    impl StartedClient<'_> {
        fn stop(mut self) -> Result<()> {
            self.1 = false;
            unsafe { self.0.Stop() }.context("stopping Windows system audio client")
        }
    }
    impl Drop for StartedClient<'_> {
        fn drop(&mut self) {
            if self.1 {
                let _ = unsafe { self.0.Stop() };
            }
        }
    }

    /// # Safety
    /// A nonnull pointer must hold WAVEFORMATEX and its declared extension.
    unsafe fn mix_format(data: *const WAVEFORMATEX) -> Result<MixFormat> {
        ensure!(!data.is_null(), "null Windows system audio mix format");
        let wave = unsafe { data.read_unaligned() };
        let mut tag = u32::from(wave.wFormatTag);
        let mut valid_bits = wave.wBitsPerSample;
        if tag == WAVE_FORMAT_EXTENSIBLE {
            ensure!(
                wave.cbSize >= 22,
                "truncated Windows extensible audio format"
            );
            let extended = unsafe { data.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            let subtype = extended.SubFormat;
            let mask = extended.dwChannelMask;
            ensure!(
                mask == 0 || mask.count_ones() == u32::from(wave.nChannels),
                "invalid Windows audio channel mask"
            );
            valid_bits = unsafe { extended.Samples.wValidBitsPerSample };
            tag = if subtype == KSDATAFORMAT_SUBTYPE_PCM {
                WAVE_FORMAT_PCM
            } else if subtype == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                WAVE_FORMAT_IEEE_FLOAT
            } else {
                bail!("unsupported Windows system audio subformat");
            };
        }
        ensure!(
            tag == WAVE_FORMAT_PCM || tag == WAVE_FORMAT_IEEE_FLOAT,
            "unsupported Windows system audio format tag"
        );
        MixFormat::new(
            tag == WAVE_FORMAT_IEEE_FLOAT,
            wave.wBitsPerSample,
            valid_bits,
            wave.nChannels,
            wave.nSamplesPerSec,
            wave.nBlockAlign,
            wave.nAvgBytesPerSec,
        )
    }

    fn elapsed_ns(origin: Instant) -> Result<u64> {
        origin
            .elapsed()
            .as_nanos()
            .try_into()
            .context("Windows capture duration overflow")
    }

    fn run(
        tx: SyncSender<AudioPacket>,
        dropped: Arc<AtomicU64>,
        muted: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        origin: Instant,
        ready: SyncSender<()>,
    ) -> Result<()> {
        // IAudioClient's first device access must also respect the Windows 8 STA boundary.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .context("initializing Windows audio COM apartment")?;
        let _apartment = Apartment;
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .context("creating Windows audio endpoint enumerator")?;
        // shortcut: pin the default render route; add selection/recovery after native Windows checks.
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .context("no default Windows playback endpoint; connect a render device and resume")?;
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .context("activating Windows playback endpoint for loopback")?;
        let allocation = MixAllocation(
            unsafe { client.GetMixFormat() }.context("reading Windows loopback mix format")?,
        );
        let format = unsafe { mix_format(allocation.0) }?;
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                1_000_000,
                0,
                allocation.0,
                None,
            )
        }
        .context("initializing shared Windows loopback stream")?;
        let buffer_frames = unsafe { client.GetBufferSize() }
            .context("reading Windows loopback buffer capacity")?;
        format.packet_bytes(1, buffer_frames)?;
        let capture: IAudioCaptureClient =
            unsafe { client.GetService() }.context("opening Windows loopback capture service")?;
        let mut frequency = 0;
        let mut before = 0;
        let mut after = 0;
        unsafe { QueryPerformanceFrequency(&mut frequency) }
            .context("reading Windows performance clock frequency")?;
        unsafe { QueryPerformanceCounter(&mut before) }
            .context("reading Windows performance clock")?;
        let elapsed = elapsed_ns(origin)?;
        unsafe { QueryPerformanceCounter(&mut after) }
            .context("reading Windows performance clock")?;
        let mut clock = PacketClock::new(before, after, frequency, elapsed)?;
        let mut writer = PacketWriter {
            format,
            buffer_frames,
            sequence: 0,
            tx,
            dropped,
            muted,
            stop: stop.clone(),
        };
        unsafe { client.Start() }.context("starting Windows loopback client")?;
        let started = StartedClient(&client, true);
        let running = (|| -> Result<()> {
            ready
                .send(())
                .context("Windows loopback startup receiver disconnected")?;
            while !stop.load(Ordering::Relaxed) {
                let pending = unsafe { capture.GetNextPacketSize() }
                    .context("reading Windows loopback packet size")?;
                if pending == 0 {
                    // shortcut: 10ms polling; measure power/latency before choosing event-driven scheduling.
                    thread::park_timeout(Duration::from_millis(10));
                    continue;
                }
                format.packet_bytes(pending, buffer_frames)?;
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                let mut device_position = 0;
                let mut qpc = 0;
                unsafe {
                    capture.GetBuffer(
                        &mut data,
                        &mut frames,
                        &mut flags,
                        Some(&mut device_position),
                        Some(&mut qpc),
                    )
                }
                .context("acquiring Windows loopback buffer")?;
                if frames == 0 {
                    continue;
                }
                let buffer = Buffer(&capture, frames);
                let consumed = (|| -> Result<()> {
                    let bytes = format.packet_bytes(frames, buffer_frames)?;
                    let known_flags = (AUDCLNT_BUFFERFLAGS_SILENT.0
                        | AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0
                        | AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0)
                        as u32;
                    ensure!(
                        flags & !known_flags == 0,
                        "unknown Windows audio buffer flags"
                    );
                    let start_ns = clock.start_ns(
                        device_position,
                        qpc,
                        frames,
                        flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0,
                        flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0,
                        elapsed_ns(origin)?,
                    )?;
                    let data = unsafe {
                        packet_data(
                            data,
                            bytes,
                            flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0,
                        )
                    }?;
                    writer.write(data, frames, start_ns)
                })();
                with_cleanup(consumed, buffer.release())?;
            }
            Ok(())
        })();
        with_cleanup(running, started.stop())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn installed_wave_schema_validates_extensions_without_calling_windows() {
            let parse = |wave: &WAVEFORMATEXTENSIBLE| unsafe {
                mix_format(std::ptr::from_ref(wave).cast())
            };
            let mut wave = WAVEFORMATEXTENSIBLE::default();
            wave.Format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 384_000,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 22,
            };
            wave.Samples.wValidBitsPerSample = 32;
            wave.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
            wave.dwChannelMask = 3;
            assert!(parse(&wave).is_ok());
            assert!(unsafe { mix_format(std::ptr::null()) }.is_err());
            wave.Format.cbSize = 21;
            assert!(parse(&wave).is_err());
            wave.Format.cbSize = 22;
            wave.SubFormat = windows::core::GUID::zeroed();
            assert!(parse(&wave).is_err());
            wave.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
            wave.dwChannelMask = 1;
            assert!(parse(&wave).is_err());
            wave.dwChannelMask = 0;
            wave.SubFormat = KSDATAFORMAT_SUBTYPE_PCM;
            wave.Samples.wValidBitsPerSample = 24;
            assert!(parse(&wave).is_ok());
            wave.Format.wFormatTag = 42;
            assert!(parse(&wave).is_err());
        }

        #[test]
        fn stop_joins_once_and_keeps_the_worker_error_without_opening_audio() {
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let worker = thread::spawn(move || -> Result<()> {
                while !stopped.load(Ordering::Relaxed) {
                    thread::park_timeout(Duration::from_millis(10));
                }
                Err(std::io::Error::other("synthetic worker failure").into())
            });
            let mut capture = SystemCapture {
                stop,
                worker: Some(worker),
            };
            let error = capture.stop().unwrap_err();
            assert!(error.downcast_ref::<std::io::Error>().is_some());
            assert!(capture.worker.is_none());
            assert!(capture.stop().is_ok());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver};

    fn format(float: bool, bits: u16, channels: u16) -> MixFormat {
        let align = channels * bits / 8;
        MixFormat::new(
            float,
            bits,
            bits,
            channels,
            48_000,
            align,
            48_000 * u32::from(align),
        )
        .unwrap()
    }

    fn writer(format: MixFormat, capacity: usize) -> (PacketWriter, Receiver<AudioPacket>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        (
            PacketWriter {
                format,
                buffer_frames: 4_096,
                sequence: 0,
                tx,
                dropped: Arc::new(AtomicU64::new(0)),
                muted: Arc::new(AtomicBool::new(false)),
                stop: Arc::new(AtomicBool::new(false)),
            },
            rx,
        )
    }

    #[test]
    fn malformed_mix_formats_fail_closed() {
        for (float, bits, valid, channels, rate, align, byte_rate) in [
            (true, 64, 64, 2, 48_000, 16, 768_000),
            (true, 32, 16, 2, 48_000, 8, 384_000),
            (false, 16, 0, 2, 48_000, 4, 192_000),
            (false, 16, 17, 2, 48_000, 4, 192_000),
            (false, 16, 16, 0, 48_000, 0, 0),
            (false, 16, 16, 33, 48_000, 66, 3_168_000),
            (false, 16, 16, 2, 7_999, 4, 31_996),
            (false, 16, 16, 2, 192_001, 4, 768_004),
            (false, 16, 16, 2, 48_000, 3, 144_000),
            (false, 16, 16, 2, 48_000, 4, 192_001),
            (false, 32, 32, 32, 192_000, u16::MAX, u32::MAX),
        ] {
            assert!(MixFormat::new(float, bits, valid, channels, rate, align, byte_rate).is_err());
        }
        assert!(MixFormat::new(false, 32, 24, 2, 48_000, 8, 384_000).is_ok());
    }

    #[test]
    fn signed_pcm_and_float_downmix_preserve_source_and_rate() {
        for (bits, bytes) in [
            (16, vec![0, 128, 0, 64]),
            (24, vec![0, 0, 128, 0, 0, 64]),
            (32, vec![0, 0, 0, 128, 0, 0, 0, 64]),
        ] {
            let (mut output, packets) = writer(format(false, bits, 2), 1);
            output.write(Some(&bytes), 1, 123).unwrap();
            let packet = packets.recv().unwrap();
            assert_eq!(packet.source, Source::SystemAudio);
            assert_eq!(
                (packet.sample_rate, packet.start_ns, packet.len),
                (48_000, 123, 1)
            );
            assert_eq!(packet.samples[0], -0.25);
        }
        let bytes: Vec<_> = [-0.5_f32, 0.25]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let (mut output, packets) = writer(format(true, 32, 2), 1);
        output.write(Some(&bytes), 1, 0).unwrap();
        assert_eq!(packets.recv().unwrap().samples[0], -0.125);
    }

    #[test]
    fn silence_never_reads_a_null_pointer_and_bounds_are_checked_first() {
        assert!(unsafe { packet_data(std::ptr::null(), 4, false) }.is_err());
        assert!(unsafe { packet_data(std::ptr::null(), 0, true) }.is_err());
        assert!(unsafe { packet_data(std::ptr::null(), usize::MAX, true) }.is_err());
        let silent = unsafe { packet_data(std::ptr::null(), 8, true) }.unwrap();
        let (mut output, packets) = writer(format(true, 32, 2), 1);
        output.write(silent, 1, 0).unwrap();
        let packet = packets.recv().unwrap();
        assert_eq!(packet.len, 1);
        assert!(packet.samples.iter().all(|sample| *sample == 0.0));
        assert!(output.write(None, 4_097, 0).is_err());
        assert!(output.write(None, 0, 0).is_err());
        assert!(output.format.packet_bytes(u32::MAX, 4_096).is_err());
        assert!(output.format.packet_bytes(1, 0).is_err());
        assert!(output.format.packet_bytes(1, 96_001).is_err());
    }

    #[test]
    fn truncated_and_nonfinite_buffers_do_not_enter_the_queue() {
        let (mut output, packets) = writer(format(true, 32, 1), 1);
        assert!(output.write(Some(&[0; 3]), 1, 0).is_err());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(output.write(Some(&value.to_le_bytes()), 1, 0).is_err());
        }
        assert!(packets.try_recv().is_err());
    }

    #[test]
    fn packet_queue_is_bounded_and_dropped_sequences_remain_visible() {
        let (mut output, packets) = writer(format(true, 32, 1), 1);
        output
            .write(None, (PACKET_FRAMES * 2 + 1) as u32, 0)
            .unwrap();
        let first = packets.recv().unwrap();
        assert_eq!((first.sequence, first.len), (0, PACKET_FRAMES));
        assert_eq!(output.dropped.load(Ordering::Relaxed), 2);
        output.write(None, 1, 50_000_000).unwrap();
        let next = packets.recv().unwrap();
        assert_eq!((next.sequence, next.start_ns), (3, 50_000_000));
        drop(packets);
        assert!(output.write(None, 1, 60_000_000).is_err());
    }

    #[test]
    fn block_timestamps_and_partial_lengths_are_exact_and_both_stop_gates_close() {
        let (mut output, packets) = writer(format(true, 32, 1), 2);
        output.write(None, (PACKET_FRAMES + 1) as u32, 100).unwrap();
        assert_eq!(packets.recv().unwrap().start_ns, 100);
        let last = packets.recv().unwrap();
        assert_eq!((last.sequence, last.len, last.start_ns), (1, 1, 21_333_433));
        output.muted.store(true, Ordering::Relaxed);
        output.write(None, 1, 30_000_000).unwrap();
        output.muted.store(false, Ordering::Relaxed);
        output.stop.store(true, Ordering::Relaxed);
        output.write(None, 1, 40_000_000).unwrap();
        assert!(packets.try_recv().is_err());
    }

    #[test]
    fn packet_identity_and_timestamp_overflow_fail_visibly() {
        let (mut output, packets) = writer(format(true, 32, 1), 2);
        output.sequence = u64::MAX;
        assert!(output.write(None, 1, 0).is_err());
        assert!(packets.try_recv().is_err());
        output.sequence = 0;
        assert!(
            output
                .write(None, (PACKET_FRAMES + 1) as u32, u64::MAX)
                .is_err()
        );
        assert!(packets.try_recv().is_err());
    }

    #[test]
    fn native_clock_mapping_rejects_uncertainty_gaps_and_backward_time() {
        let mut clock = PacketClock::new(10_000_000, 10_000_000, 10_000_000, 1_000).unwrap();
        assert_eq!(
            clock
                .start_ns(0, 10_000_000, 128, true, false, 1_000)
                .unwrap(),
            1_000
        );
        assert!(
            clock
                .start_ns(128, 10_026_667, 128, false, true, 3_000_000)
                .is_err()
        );
        assert!(
            clock
                .start_ns(128, 10_026_667, 128, true, false, 3_000_000)
                .is_err()
        );
        assert!(
            clock
                .start_ns(129, 10_026_667, 128, false, false, 3_000_000)
                .is_err()
        );
        assert!(
            clock
                .start_ns(128, 9_999_999, 128, false, false, 3_000_000)
                .is_err()
        );
        assert_eq!(
            clock
                .start_ns(128, 10_026_667, 128, false, false, 3_000_000)
                .unwrap(),
            2_667_700
        );
        assert!(PacketClock::new(0, 0, 0, 0).is_err());
        assert!(PacketClock::new(-1, 0, 1, 0).is_err());
        assert!(PacketClock::new(1, 0, 1, 0).is_err());
        assert!(PacketClock::new(0, 10_001, 10_000_000, 0).is_err());
        let mut clock = PacketClock::new(0, 0, 1, 0).unwrap();
        assert!(
            clock
                .start_ns(0, u64::MAX, 1, false, false, u64::MAX)
                .is_err()
        );
        assert!(clock.start_ns(0, 20_001, 1, false, false, 0).is_err());
        assert!(clock.start_ns(u64::MAX, 0, 1, false, false, 0).is_err());
    }

    #[test]
    fn cleanup_preserves_the_primary_error_and_native_cleanup_failure() {
        let error = with_cleanup(
            with_cleanup(
                Err(std::io::Error::other("decode failed").into()),
                Err(anyhow::anyhow!("release failed")),
            ),
            Err(anyhow::anyhow!("stop failed")),
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("decode failed")
                && message.contains("release failed")
                && message.contains("stop failed")
        );
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert!(
            with_cleanup(Ok(()), Err(anyhow::anyhow!("stop failed")))
                .unwrap_err()
                .to_string()
                .contains("stop failed")
        );
    }
}
