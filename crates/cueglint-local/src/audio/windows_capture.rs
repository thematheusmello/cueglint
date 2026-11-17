//! One bounded native bridge for Windows input and render-loopback capture.
use super::{AudioPacket, PACKET_FRAMES, Source};
use anyhow::{Context, Result, bail, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{SyncSender, TrySendError},
};

#[cfg(target_os = "windows")]
pub(super) use native::WasapiCapture;

// shortcut: cap endpoint IDs at 16,384 UTF-16 units; raise only for a verified endpoint that needs it.
const MAX_ENDPOINT_ID_UNITS: usize = 16_384;

fn endpoint_id_units(id: &str) -> Result<Vec<u16>> {
    ensure!(!id.is_empty(), "Windows Microphone endpoint ID is empty");
    ensure!(
        !id.contains('\0'),
        "Windows Microphone endpoint ID contains NUL"
    );
    let mut units: Vec<_> = id.encode_utf16().take(MAX_ENDPOINT_ID_UNITS + 1).collect();
    ensure!(
        units.len() <= MAX_ENDPOINT_ID_UNITS,
        "Windows Microphone endpoint ID is too long"
    );
    units.push(0);
    Ok(units)
}

/// # Safety
/// A nonnull pointer must hold a terminated string or at least MAX_ENDPOINT_ID_UNITS + 1 units.
unsafe fn read_endpoint_id(data: *const u16) -> Result<Vec<u16>> {
    ensure!(!data.is_null(), "null Windows audio endpoint ID");
    let mut units = Vec::new();
    for index in 0..=MAX_ENDPOINT_ID_UNITS {
        let unit = unsafe { data.add(index).read() };
        if unit == 0 {
            ensure!(!units.is_empty(), "empty Windows audio endpoint ID");
            units.push(0);
            return Ok(units);
        }
        units.push(unit);
    }
    bail!("Windows audio endpoint ID is too long")
}

#[cfg(target_os = "windows")]
fn selected_microphone_id(serialized: &str) -> Result<Vec<u16>> {
    let bound = MAX_ENDPOINT_ID_UNITS + "wasapi:".len();
    ensure!(
        serialized.encode_utf16().take(bound + 1).count() <= bound,
        "Windows Microphone device ID is too long"
    );
    let id: cpal::DeviceId = serialized.parse()?;
    ensure!(
        id.host() == cpal::HostId::Wasapi,
        "Windows Microphone requires a WASAPI device ID"
    );
    endpoint_id_units(id.id())
}

struct MicrophoneRoute {
    id: Vec<u16>,
    default: bool,
}

impl MicrophoneRoute {
    fn new(
        selected: Option<&[u16]>,
        id: Vec<u16>,
        capture_endpoint: bool,
        active: bool,
    ) -> Result<Self> {
        ensure!(
            capture_endpoint && active,
            "Windows Microphone requires an active capture endpoint"
        );
        ensure!(
            selected.is_none_or(|selected| selected == id.as_slice()),
            "Windows Microphone resolved a different endpoint ID"
        );
        Ok(Self {
            id,
            default: selected.is_none(),
        })
    }

    fn check(&self, active: bool, current_default: Option<&[u16]>) -> Result<()> {
        ensure!(
            active,
            "Windows Microphone endpoint is unavailable; reconnect it and resume"
        );
        if self.default {
            ensure!(
                current_default == Some(self.id.as_slice()),
                "Windows Microphone default endpoint changed or is missing; resume to select it again"
            );
        }
        Ok(())
    }
}

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
        ensure!(value.is_finite(), "nonfinite Windows audio sample");
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
            _ => bail!("unsupported Windows audio format: float={float}, bits={bits}"),
        };
        ensure!(
            valid_bits > 0 && valid_bits <= bits && (!float || valid_bits == bits),
            "invalid Windows audio precision"
        );
        ensure!(
            (1..=32).contains(&channels) && (8_000..=192_000).contains(&sample_rate),
            "unsupported Windows audio channels or sample rate"
        );
        ensure!(
            block_align as usize == channels as usize * encoding.bytes()
                && byte_rate == sample_rate * u32::from(block_align),
            "invalid Windows audio block alignment or byte rate"
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
            "invalid Windows audio buffer frame count"
        );
        (frames as usize)
            .checked_mul(self.block_align)
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .context("Windows audio buffer size overflow")
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
    ensure!(!data.is_null(), "null Windows audio buffer");
    // SILENT buffers are deliberately never read, even when Windows supplies a pointer.
    Ok(Some(unsafe { std::slice::from_raw_parts(data, len) }))
}

struct PacketWriter {
    source: Source,
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
        let bytes = self
            .format
            .packet_bytes(frames, self.buffer_frames)
            .with_context(|| format!("validating Windows {:?} audio buffer", self.source))?;
        ensure!(
            data.is_none_or(|data| data.len() == bytes),
            "truncated Windows {:?} audio buffer",
            self.source
        );
        start_ns
            .checked_add(u64::from(frames) * 1_000_000_000 / u64::from(self.format.sample_rate))
            .with_context(|| format!("Windows {:?} audio timestamp overflow", self.source))?;
        for offset in (0..frames as usize).step_by(PACKET_FRAMES) {
            if self.stop.load(Ordering::Relaxed) || self.muted.load(Ordering::Relaxed) {
                break;
            }
            let len = PACKET_FRAMES.min(frames as usize - offset);
            let mut packet = AudioPacket {
                source: self.source,
                sequence: self.sequence,
                start_ns: start_ns + offset as u64 * 1_000_000_000 / self.format.sample_rate as u64,
                sample_rate: self.format.sample_rate,
                len,
                samples: [0.0; PACKET_FRAMES],
            };
            self.sequence = self
                .sequence
                .checked_add(1)
                .with_context(|| format!("Windows {:?} audio sequence overflow", self.source))?;
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
                        sum +=
                            f64::from(self.format.encoding.sample(sample).with_context(|| {
                                format!("invalid Windows {:?} audio sample", self.source)
                            })?);
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
                    bail!(
                        "Windows {:?} audio packet receiver disconnected",
                        self.source
                    )
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
        ensure!(!timestamp_error, "Windows audio timestamp is uncertain");
        // The first discontinuity flag is not meaningful; later gaps stop rather than join audio.
        ensure!(
            self.next_device_position
                .is_none_or(|next| !discontinuity && next == device_position),
            "Windows audio discontinuity; resume to reconnect the source"
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
        ffi::c_void,
        sync::mpsc,
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Media::{
                Audio::{
                    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
                    AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow,
                    IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, IMMEndpoint,
                    MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
                    eCapture, eConsole, eRender,
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
        },
        core::{Interface, PCWSTR},
    };

    // shortcut: poll microphone identity; add notifications if measured latency/power requires them.
    const MICROPHONE_ROUTE_CHECK_INTERVAL: Duration = Duration::from_millis(250);

    pub(crate) struct WasapiCapture {
        source: Source,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<Result<()>>>,
    }

    impl WasapiCapture {
        pub(crate) fn start(
            source: Source,
            device_id: Option<&str>,
            tx: SyncSender<AudioPacket>,
            dropped: Arc<AtomicU64>,
            errors: Arc<AtomicU64>,
            muted: Arc<AtomicBool>,
            origin: Instant,
        ) -> Result<Self> {
            let selected = match (source, device_id) {
                (Source::Microphone, Some(id)) => {
                    Some(selected_microphone_id(id).context("selecting Windows Microphone audio")?)
                }
                (Source::SystemAudio, Some(_)) => {
                    bail!("Windows SystemAudio does not accept a microphone device ID")
                }
                _ => None,
            };
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let (ready, started) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name(format!("cueglint-wasapi-{source:?}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(source, selected, tx, dropped, muted, stopped, origin, ready)
                            .with_context(|| format!("capturing Windows {source:?} audio"))
                    }))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!("Windows {source:?} audio worker panicked"))
                    });
                    if result.is_err() {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                    result
                })
                .with_context(|| format!("starting Windows {source:?} audio worker"))?;
            let mut capture = Self {
                source,
                stop,
                worker: Some(worker),
            };
            if started.recv().is_err() {
                capture
                    .stop()
                    .with_context(|| format!("initializing Windows {source:?} audio"))?;
                bail!("Windows {source:?} audio worker exited before startup");
            }
            Ok(capture)
        }

        pub(crate) fn stop(&mut self) -> Result<()> {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                worker.thread().unpark();
                // Native calls cannot be interrupted; capture shutdown stays off the UI thread.
                return worker
                    .join()
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "Windows {:?} audio worker panicked",
                            self.source
                        ))
                    })
                    .with_context(|| format!("stopping Windows {:?} audio", self.source));
            }
            Ok(())
        }
    }

    impl Drop for WasapiCapture {
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

    struct TaskMemory(*mut c_void);
    impl Drop for TaskMemory {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }

    fn endpoint_id(device: &IMMDevice) -> Result<Vec<u16>> {
        let id = unsafe { device.GetId() }.context("reading Windows audio endpoint ID")?;
        let _allocation = TaskMemory(id.0.cast());
        unsafe { read_endpoint_id(id.0) }
    }

    fn microphone_route(
        selected: Option<&[u16]>,
        id: Vec<u16>,
        flow: EDataFlow,
        state: DEVICE_STATE,
    ) -> Result<MicrophoneRoute> {
        MicrophoneRoute::new(selected, id, flow == eCapture, state == DEVICE_STATE_ACTIVE)
    }

    struct Buffer<'a>(&'a IAudioCaptureClient, u32);
    impl Buffer<'_> {
        fn release(mut self) -> Result<()> {
            let frames = std::mem::replace(&mut self.1, 0);
            unsafe { self.0.ReleaseBuffer(frames) }.context("releasing Windows audio buffer")
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
            unsafe { self.0.Stop() }.context("stopping Windows audio client")
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
        ensure!(!data.is_null(), "null Windows audio mix format");
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
                bail!("unsupported Windows audio subformat");
            };
        }
        ensure!(
            tag == WAVE_FORMAT_PCM || tag == WAVE_FORMAT_IEEE_FLOAT,
            "unsupported Windows audio format tag"
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
        source: Source,
        selected: Option<Vec<u16>>,
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
        let device = match source {
            Source::Microphone => match &selected {
                Some(id) => unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) }
                    .context("selected Windows Microphone endpoint is unavailable; reconnect it or select another input")?,
                None => unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
                    .context("no default Windows Microphone endpoint; connect an input device and resume")?,
            },
            // shortcut: pin the default render route; add selection/recovery after native Windows checks.
            Source::SystemAudio => unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
                .context("no default Windows playback endpoint; connect a render device and resume")?,
        };
        let microphone_route = if source == Source::Microphone {
            let endpoint: IMMEndpoint = device
                .cast()
                .context("querying Windows Microphone endpoint direction")?;
            Some(microphone_route(
                selected.as_deref(),
                endpoint_id(&device)?,
                unsafe { endpoint.GetDataFlow() }
                    .context("reading Windows Microphone endpoint direction")?,
                unsafe { device.GetState() }
                    .context("reading Windows Microphone endpoint state")?,
            )?)
        } else {
            None
        };
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .context("activating Windows audio endpoint")?;
        let allocation = TaskMemory(
            unsafe { client.GetMixFormat() }
                .context("reading Windows audio mix format")?
                .cast(),
        );
        let format = unsafe { mix_format(allocation.0.cast()) }?;
        let flags = if source == Source::SystemAudio {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                1_000_000,
                0,
                allocation.0.cast(),
                None,
            )
        }
        .context("initializing shared Windows audio stream")?;
        let buffer_frames =
            unsafe { client.GetBufferSize() }.context("reading Windows audio buffer capacity")?;
        format.packet_bytes(1, buffer_frames)?;
        let capture: IAudioCaptureClient =
            unsafe { client.GetService() }.context("opening Windows audio capture service")?;
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
            source,
            format,
            buffer_frames,
            sequence: 0,
            tx,
            dropped,
            muted,
            stop: stop.clone(),
        };
        unsafe { client.Start() }.context("starting Windows audio client")?;
        let started = StartedClient(&client, true);
        let mut checked_at = Instant::now();
        let running = (|| -> Result<()> {
            ready
                .send(())
                .context("Windows audio startup receiver disconnected")?;
            while !stop.load(Ordering::Relaxed) {
                if let Some(route) = &microphone_route {
                    if checked_at.elapsed() >= MICROPHONE_ROUTE_CHECK_INTERVAL {
                        let active = unsafe { device.GetState() }
                            .context("checking Windows Microphone endpoint state")?
                            == DEVICE_STATE_ACTIVE;
                        let default = if active && route.default {
                            let default = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
                                .context("Windows Microphone default endpoint is unavailable; reconnect it and resume")?;
                            Some(endpoint_id(&default)?)
                        } else {
                            None
                        };
                        route.check(active, default.as_deref())?;
                        checked_at = Instant::now();
                    }
                }
                let pending = unsafe { capture.GetNextPacketSize() }
                    .context("reading Windows audio packet size")?;
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
                .context("acquiring Windows audio buffer")?;
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
        fn persisted_cpal_ids_map_to_the_opaque_native_id_without_opening_audio() {
            for id in ["synthetic:A:B", "é:😀", " raw ", "default"] {
                assert_eq!(
                    selected_microphone_id(&format!("WaSaPi:{id}")).unwrap(),
                    endpoint_id_units(id).unwrap()
                );
            }
            for id in [
                "no-colon",
                "wasapi:",
                "wasapi:a\0b",
                "asio:synthetic",
                "coreaudio:synthetic",
            ] {
                assert!(selected_microphone_id(id).is_err(), "{id:?}");
            }
            assert!(
                selected_microphone_id(&format!(
                    "wasapi:{}",
                    "x".repeat(MAX_ENDPOINT_ID_UNITS + 1)
                ))
                .is_err()
            );
            assert!(
                selected_microphone_id(&format!(
                    "wasapi:{}",
                    "😀".repeat(MAX_ENDPOINT_ID_UNITS / 2)
                ))
                .is_ok()
            );
        }

        #[test]
        fn native_microphone_direction_and_state_schema_fail_closed() {
            use windows::Win32::Media::Audio::{
                DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED,
            };
            for (state, flow, allowed) in [
                (DEVICE_STATE_ACTIVE, eCapture, true),
                (DEVICE_STATE_ACTIVE, eRender, false),
                (DEVICE_STATE_DISABLED, eCapture, false),
                (DEVICE_STATE_NOTPRESENT, eCapture, false),
                (DEVICE_STATE_UNPLUGGED, eCapture, false),
                (DEVICE_STATE(0), eCapture, false),
            ] {
                let route =
                    microphone_route(None, endpoint_id_units("synthetic").unwrap(), flow, state);
                assert_eq!(route.is_ok(), allowed);
            }
        }

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
            for source in [Source::Microphone, Source::SystemAudio] {
                let stop = Arc::new(AtomicBool::new(false));
                let stopped = stop.clone();
                let worker = thread::spawn(move || -> Result<()> {
                    while !stopped.load(Ordering::Relaxed) {
                        thread::park_timeout(Duration::from_millis(10));
                    }
                    Err(std::io::Error::other("synthetic worker failure").into())
                });
                let mut capture = WasapiCapture {
                    source,
                    stop,
                    worker: Some(worker),
                };
                let error = capture.stop().unwrap_err();
                assert!(format!("{error:#}").contains(&format!("{source:?}")));
                assert!(error.downcast_ref::<std::io::Error>().is_some());
                assert!(capture.worker.is_none());
                assert!(capture.stop().is_ok());
            }
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
                source: Source::SystemAudio,
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
    fn endpoint_ids_preserve_unicode_and_bound_every_native_read() {
        assert_eq!(
            endpoint_id_units("é:😀").unwrap(),
            [233, 58, 0xd83d, 0xde00, 0]
        );
        assert!(endpoint_id_units("").is_err());
        assert!(endpoint_id_units("a\0b").is_err());
        let maximum = "😀".repeat(MAX_ENDPOINT_ID_UNITS / 2);
        let units = endpoint_id_units(&maximum).unwrap();
        assert_eq!(units.len(), MAX_ENDPOINT_ID_UNITS + 1);
        assert!(endpoint_id_units(&(maximum + "a")).is_err());
        assert_eq!(unsafe { read_endpoint_id(units.as_ptr()) }.unwrap(), units);
        assert!(unsafe { read_endpoint_id(std::ptr::null()) }.is_err());
        assert!(unsafe { read_endpoint_id([0].as_ptr()) }.is_err());
        let oversized = vec![1; MAX_ENDPOINT_ID_UNITS + 1];
        assert!(unsafe { read_endpoint_id(oversized.as_ptr()) }.is_err());
    }

    #[test]
    fn microphone_selection_never_substitutes_another_endpoint() {
        let chosen = endpoint_id_units("chosen").unwrap();
        let other = endpoint_id_units("other").unwrap();
        assert!(MicrophoneRoute::new(Some(&chosen), other.clone(), true, true).is_err());
        assert!(MicrophoneRoute::new(None, chosen.clone(), false, true).is_err());
        assert!(MicrophoneRoute::new(None, chosen.clone(), true, false).is_err());
        let explicit = MicrophoneRoute::new(Some(&chosen), chosen.clone(), true, true).unwrap();
        assert!(explicit.check(true, None).is_ok());
        assert!(explicit.check(true, Some(&other)).is_ok());
        assert!(explicit.check(false, Some(&other)).is_err());
        let default = MicrophoneRoute::new(None, chosen.clone(), true, true).unwrap();
        assert!(default.check(true, Some(&chosen)).is_ok());
        assert!(default.check(true, Some(&other)).is_err());
        assert!(default.check(true, None).is_err());
        assert!(default.check(false, Some(&chosen)).is_err());
    }

    #[test]
    fn shared_packet_queue_preserves_both_source_identities_and_sequences() {
        let (mut microphone, packets) = writer(format(true, 32, 1), 3);
        microphone.source = Source::Microphone;
        let (mut system, _unused) = writer(format(true, 32, 1), 1);
        system.tx = microphone.tx.clone();
        microphone.write(None, 1, 0).unwrap();
        system.write(None, 1, 0).unwrap();
        microphone.write(None, 1, 21_000).unwrap();
        let delivered: Vec<_> = packets
            .try_iter()
            .map(|packet| (packet.source, packet.sequence))
            .collect();
        assert_eq!(
            delivered,
            [
                (Source::Microphone, 0),
                (Source::SystemAudio, 0),
                (Source::Microphone, 1)
            ]
        );
    }

    #[test]
    fn both_source_gates_close_and_errors_keep_the_actual_source() {
        for source in [Source::Microphone, Source::SystemAudio] {
            let (mut output, packets) = writer(format(true, 32, 1), 1);
            output.source = source;
            output.muted.store(true, Ordering::Relaxed);
            output.write(None, 1, 0).unwrap();
            output.muted.store(false, Ordering::Relaxed);
            output.stop.store(true, Ordering::Relaxed);
            output.write(None, 1, 0).unwrap();
            assert!(packets.try_recv().is_err());
            output.stop.store(false, Ordering::Relaxed);
            let error = output
                .write(Some(&f32::NAN.to_le_bytes()), 1, 0)
                .unwrap_err();
            assert!(format!("{error:#}").contains(&format!("{source:?}")));
            drop(packets);
            let error = output.write(None, 1, 0).unwrap_err();
            assert!(format!("{error:#}").contains(&format!("{source:?}")));
        }
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
