//! Bounded source-labelled capture; transcription runs on a separate consumer.
use anyhow::{Context, Result, bail, ensure};
#[cfg(not(target_os = "windows"))]
use cpal::traits::StreamTrait;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod linux_monitor;
#[cfg(target_os = "macos")]
mod mac_timing;
#[cfg(any(not(target_os = "windows"), test))]
mod microphone;
mod resample;
#[cfg(any(target_os = "windows", test))]
mod windows_capture;

#[cfg(target_os = "macos")]
use mac_timing::QueuedPacket;
#[cfg(not(target_os = "macos"))]
type QueuedPacket = AudioPacket;

pub const SAMPLE_RATE: u32 = 16_000;
pub const PACKET_FRAMES: usize = 1024;
pub const PACKET_QUEUE_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Microphone,
    SystemAudio,
}

#[derive(Clone)]
pub struct AudioPacket {
    pub source: Source,
    pub sequence: u64,
    pub start_ns: u64,
    pub sample_rate: u32,
    pub len: usize,
    pub samples: [f32; PACKET_FRAMES],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
}

pub fn input_devices() -> Result<Vec<InputDevice>> {
    // shortcut: cap the native catalog at 128 inputs; raise for a verified larger setup.
    let devices: Vec<_> = cpal::default_host()
        .input_devices()?
        .take(129)
        .map(|device| {
            let id = device.id()?.to_string();
            cueglint_core::validate_microphone_device_id(&id)?;
            let name = device.description()?.name().to_owned();
            ensure!(
                name.len() <= 1_024 && !name.contains('\0'),
                "microphone label is invalid or exceeds 1024 bytes"
            );
            Ok(InputDevice { id, name })
        })
        .collect::<Result<_>>()?;
    ensure!(
        devices.len() <= 128,
        "microphone catalog exceeds 128 inputs"
    );
    Ok(devices)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureConfig {
    pub microphone: bool,
    pub system_audio: bool,
    pub microphone_device_id: Option<String>,
    pub display_id: Option<u32>,
}

impl From<&cueglint_core::Settings> for CaptureConfig {
    fn from(settings: &cueglint_core::Settings) -> Self {
        Self {
            microphone: settings.microphone_enabled,
            system_audio: settings.system_audio_enabled,
            microphone_device_id: settings.microphone_device_id.clone(),
            display_id: settings.display_id,
        }
    }
}

/// The fixture/import boundary is deliberately narrow and bounded, never guessed from extension.
pub fn load_wav(path: &std::path::Path) -> Result<Vec<f32>> {
    decode_wav(hound::WavReader::open(path)?)
}

/// Decode the exact immutable WAV bytes used by an evidence/import caller.
pub fn load_wav_bytes(bytes: &[u8]) -> Result<Vec<f32>> {
    decode_wav(hound::WavReader::new(std::io::Cursor::new(bytes))?)
}

fn decode_wav<R: std::io::Read>(mut reader: hound::WavReader<R>) -> Result<Vec<f32>> {
    let spec = reader.spec();
    ensure!(
        spec.channels == 1
            && spec.sample_rate == SAMPLE_RATE
            && spec.bits_per_sample == 16
            && spec.sample_format == hound::SampleFormat::Int,
        "audio must be mono 16kHz PCM16 WAV"
    );
    ensure!(
        reader.len() <= SAMPLE_RATE * 30,
        "audio fixture exceeds 30 seconds"
    );
    let samples = reader
        .samples::<i16>()
        .take(SAMPLE_RATE as usize * 30 + 1)
        .map(|s| s.map(|v| v as f32 / 32768.0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        !samples.is_empty() && samples.len() <= SAMPLE_RATE as usize * 30,
        "audio must contain 0–30 seconds of samples"
    );
    Ok(samples)
}

pub struct Capture {
    #[cfg(not(target_os = "windows"))]
    microphone: Option<cpal::Stream>,
    #[cfg(target_os = "windows")]
    microphone: Option<windows_capture::WasapiCapture>,
    #[cfg(target_os = "macos")]
    system: Option<screencapturekit::stream::SCStream>,
    #[cfg(target_os = "windows")]
    system: Option<windows_capture::WasapiCapture>,
    #[cfg(target_os = "linux")]
    system: Option<linux_monitor::Monitor>,
    packets: Receiver<QueuedPacket>,
    #[cfg(target_os = "macos")]
    timing: mac_timing::Timing,
    pub dropped_packets: Arc<AtomicU64>,
    pub device_errors: Arc<AtomicU64>,
    pub muted: Arc<std::sync::atomic::AtomicBool>,
}

impl Capture {
    /// Source initialization is all-or-error; a missing system source is never disguised as mic capture.
    pub fn start(config: &CaptureConfig) -> Result<Self> {
        let microphone = config.microphone;
        let system_audio = config.system_audio;
        let device_id = config.microphone_device_id.as_deref();
        let display_id = config.display_id;
        if let Some(id) = device_id {
            cueglint_core::validate_microphone_device_id(id)?;
        }
        ensure!(display_id != Some(0), "capture display ID must be nonzero");
        #[cfg(target_os = "macos")]
        if microphone {
            ensure!(
                crate::permissions::status().microphone == crate::permissions::Status::Granted,
                "Microphone permission is required before capture can start"
            );
        }
        ensure!(
            microphone || system_audio,
            "enable at least one audio source"
        );
        let (tx, packets) = mpsc::sync_channel(PACKET_QUEUE_CAPACITY);
        let dropped_packets = Arc::new(AtomicU64::new(0));
        let device_errors = Arc::new(AtomicU64::new(0));
        let muted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(target_os = "macos")]
        let mut timing = mac_timing::Timing::new()?;
        #[cfg(target_os = "macos")]
        let origin = timing.arrival_origin;
        #[cfg(not(target_os = "macos"))]
        let origin = Instant::now();
        let mic = if microphone {
            Some(start_microphone(
                device_id,
                tx.clone(),
                dropped_packets.clone(),
                device_errors.clone(),
                muted.clone(),
                #[cfg(not(target_os = "macos"))]
                origin,
                #[cfg(target_os = "macos")]
                timing.callbacks.clone(),
            )?)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        let system = if system_audio {
            Some(mac::start_system(
                display_id,
                tx,
                dropped_packets.clone(),
                device_errors.clone(),
                muted.clone(),
                origin,
                timing.callbacks.clone(),
            )?)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        {
            timing.system_clock = system
                .as_ref()
                .and_then(|stream| stream.synchronization_clock());
        }
        #[cfg(target_os = "windows")]
        let system = if system_audio {
            Some(windows_capture::WasapiCapture::start(
                Source::SystemAudio,
                None,
                tx,
                dropped_packets.clone(),
                device_errors.clone(),
                muted.clone(),
                origin,
            )?)
        } else {
            None
        };
        #[cfg(target_os = "windows")]
        let _ = display_id;
        #[cfg(target_os = "linux")]
        let system = if system_audio {
            Some(linux_monitor::Monitor::start(
                tx,
                dropped_packets.clone(),
                device_errors.clone(),
                muted.clone(),
                origin,
            )?)
        } else {
            None
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        if system_audio {
            let _ = display_id;
            bail!("system audio is not implemented for this platform yet");
        }
        Ok(Self {
            microphone: mic,
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            system,
            packets,
            #[cfg(target_os = "macos")]
            timing,
            dropped_packets,
            device_errors,
            muted,
        })
    }

    pub fn timing_notice(&self) -> Option<&'static str> {
        #[cfg(target_os = "macos")]
        if self.system.is_some() && self.timing.system_clock.is_none() {
            return Some(
                "System audio is using estimated arrival times because the stream clock was unavailable.",
            );
        }
        #[cfg(target_os = "linux")]
        if self.system.is_some() {
            return Some(
                "System audio uses the playback clock when available and estimates timestamps from buffered audio while the clock initializes.",
            );
        }
        None
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<AudioPacket>> {
        if self.muted.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Ok(packet) = self.packets.recv_timeout(timeout) else {
            return Ok(None);
        };
        #[cfg(target_os = "macos")]
        {
            self.timing.normalize(packet)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(Some(packet))
        }
    }

    /// Call after stop, including when stop returned an error.
    pub fn stopped_packets(&mut self) -> impl Iterator<Item = Result<AudioPacket>> + '_ {
        #[cfg(target_os = "macos")]
        {
            self.timing.final_packets.drain(..)
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.packets.try_iter().map(Ok)
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        #[cfg(target_os = "macos")]
        let prepared = self.timing.prepare_stop(&self.packets, &self.muted);
        #[cfg(not(target_os = "macos"))]
        self.muted.store(true, Ordering::Relaxed);
        let microphone = self.microphone.take();
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        let system = self.system.take();
        let stopped = stop_sources(
            || {
                #[cfg(not(target_os = "windows"))]
                if let Some(mic) = microphone {
                    mic.pause().context("stopping microphone")?;
                }
                #[cfg(target_os = "windows")]
                if let Some(mut mic) = microphone {
                    mic.stop().context("stopping microphone")?;
                }
                Ok(())
            },
            || {
                #[cfg(target_os = "macos")]
                if let Some(system) = system {
                    system
                        .stop_capture()
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                        .context("stopping system audio")?;
                }
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                if let Some(mut system) = system {
                    system.stop().context("stopping system audio")?;
                }
                Ok(())
            },
        );
        #[cfg(target_os = "macos")]
        if let Err(error) = prepared {
            return Err(match stopped {
                Ok(()) => error,
                Err(stop_error) => {
                    error.context(format!("stopping audio also failed: {stop_error:#}"))
                }
            });
        }
        stopped
    }
}

fn stop_sources(
    microphone: impl FnOnce() -> Result<()>,
    system_audio: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let microphone = microphone();
    let system_audio = system_audio();
    match (microphone, system_audio) {
        (Err(mic), Err(system)) => {
            Err(mic.context(format!("system audio stop also failed: {system:#}")))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(target_os = "windows")]
fn start_microphone(
    device_id: Option<&str>,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    muted: Arc<std::sync::atomic::AtomicBool>,
    origin: Instant,
) -> Result<windows_capture::WasapiCapture> {
    windows_capture::WasapiCapture::start(
        Source::Microphone,
        device_id,
        tx,
        dropped,
        errors,
        muted,
        origin,
    )
}

#[cfg(not(target_os = "windows"))]
fn start_microphone(
    device_id: Option<&str>,
    tx: SyncSender<QueuedPacket>,
    dropped: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    muted: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(not(target_os = "macos"))] origin: Instant,
    #[cfg(target_os = "macos")] callbacks: Arc<mac_timing::CallbackGate>,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => {
            let id: cpal::DeviceId = id.parse().context("invalid selected microphone ID")?;
            ensure!(
                id.host() == host.id(),
                "selected microphone belongs to another audio host; select an input for this platform"
            );
            let device = host.device_by_id(&id)
                .context("selected microphone is unavailable; reconnect it or explicitly choose another input")?;
            ensure!(
                device.id()? == id,
                "selected microphone resolved to a different device"
            );
            device
        }
        None => host
            .default_input_device()
            .context("no default microphone is available; connect or select an input")?,
    };
    let config = device
        .default_input_config()
        .context("microphone unavailable or permission denied")?;
    let stream = microphone::build(
        &device,
        &config,
        tx,
        dropped,
        errors,
        muted,
        #[cfg(not(target_os = "macos"))]
        origin,
        #[cfg(target_os = "macos")]
        callbacks,
    )?;
    stream.play()?;
    Ok(stream)
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use screencapturekit::{prelude::*, stream::ErrorHandler};
    struct Output {
        tx: SyncSender<QueuedPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        sequence: AtomicU64,
        muted: Arc<std::sync::atomic::AtomicBool>,
        origin: Instant,
        callbacks: Arc<mac_timing::CallbackGate>,
    }
    impl SCStreamOutputTrait for Output {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Audio || self.muted.load(Ordering::Relaxed) {
                return;
            }
            let Some(_callback) = self.callbacks.enter() else {
                return;
            };
            let frame_count = sample.num_samples();
            if frame_count == 0 {
                return;
            }
            let Ok(list) = sample.audio_buffer_list() else {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let Some(format) = sample.format_description() else {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let rate = 48_000;
            // This stream is configured for a single mono float32 plane.
            if format.audio_sample_rate() != Some(f64::from(rate))
                || format.audio_channel_count() != Some(1)
                || list.num_buffers() != 1
            {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let Ok(arrival) = u64::try_from(self.origin.elapsed().as_nanos()) else {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return;
            };
            let pts = sample.presentation_timestamp();
            let buffer_sequence = self.sequence.load(Ordering::Relaxed);
            for buffer in &list {
                let data = buffer.data();
                if data.len() % 4 != 0 || usize::try_from(frame_count) != Ok(data.len() / 4) {
                    self.errors.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                for (index, block) in data.chunks(PACKET_FRAMES * 4).enumerate() {
                    if self.muted.load(Ordering::Relaxed) {
                        return;
                    }
                    let offset =
                        index as u128 * PACKET_FRAMES as u128 * 1_000_000_000 / u128::from(rate);
                    let Ok(frame_offset_ns) = u64::try_from(offset) else {
                        self.errors.fetch_add(1, Ordering::Relaxed);
                        return;
                    };
                    let Some(start_ns) = arrival.checked_add(frame_offset_ns) else {
                        self.errors.fetch_add(1, Ordering::Relaxed);
                        return;
                    };
                    let Ok(sequence) = self.sequence.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |sequence| sequence.checked_add(1),
                    ) else {
                        self.errors.fetch_add(1, Ordering::Relaxed);
                        return;
                    };
                    let mut packet = AudioPacket {
                        source: Source::SystemAudio,
                        sequence,
                        start_ns,
                        sample_rate: rate,
                        len: block.len() / 4,
                        samples: [0.0; PACKET_FRAMES],
                    };
                    for (out, bytes) in packet.samples.iter_mut().zip(block.chunks_exact(4)) {
                        *out = f32::from_le_bytes(bytes.try_into().unwrap());
                    }
                    if self
                        .tx
                        .try_send(QueuedPacket::System {
                            packet,
                            pts,
                            buffer_sequence,
                            frame_offset_ns,
                        })
                        .is_err()
                    {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
    pub fn start_system(
        display_id: Option<u32>,
        tx: SyncSender<QueuedPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        muted: Arc<std::sync::atomic::AtomicBool>,
        origin: Instant,
        callbacks: Arc<mac_timing::CallbackGate>,
    ) -> Result<SCStream> {
        crate::permissions::ensure_screen()?;
        let content = SCShareableContent::get().map_err(|e| {
            anyhow::anyhow!("system audio requires Screen Recording permission: {e}")
        })?;
        let display = crate::screen::select_display(
            content
                .displays()
                .into_iter()
                .map(|display| (display.display_id(), display)),
            display_id,
            objc2_core_graphics::CGMainDisplayID(),
        )?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_captures_audio(true)
            .with_sample_rate(48_000)
            .with_channel_count(1)
            .with_excludes_current_process_audio(true);
        let stopped_errors = errors.clone();
        let stopped_muted = muted.clone();
        let delegate = ErrorHandler::new(move |_| {
            if !stopped_muted.load(Ordering::Relaxed) {
                stopped_errors.fetch_add(1, Ordering::Relaxed);
            }
        });
        let mut stream = SCStream::new_with_delegate(&filter, &config, delegate)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        stream
            .add_output_handler(
                Output {
                    tx,
                    dropped,
                    errors,
                    sequence: AtomicU64::new(0),
                    muted,
                    origin,
                    callbacks,
                },
                SCStreamOutputType::Audio,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        stream
            .start_capture()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(stream)
    }
}

/// One bounded utterance. Partial updates keep the same identity until finalized.
#[derive(Clone)]
pub struct Utterance {
    pub id: String,
    pub source: Source,
    pub start_ns: u64,
    pub end_ns: u64,
    pub samples: Vec<f32>,
    pub finalized: bool,
}

pub struct Endpoint {
    source: Source,
    id: String,
    pcm: Vec<f32>,
    start_ns: u64,
    end_ns: u64,
    silent_samples: usize,
    last_partial: usize,
    rate: u32,
    resampler: Option<resample::Resampler>,
    stream_origin_ns: u64,
    next_sequence: Option<u64>,
    next_start_ns: Option<u64>,
    pub gaps: u64,
}

impl Endpoint {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            id: uuid::Uuid::new_v4().to_string(),
            pcm: Vec::new(),
            start_ns: 0,
            end_ns: 0,
            silent_samples: 0,
            last_partial: 0,
            rate: 0,
            resampler: None,
            stream_origin_ns: 0,
            next_sequence: None,
            next_start_ns: None,
            gaps: 0,
        }
    }
    pub fn push(&mut self, packet: &AudioPacket) -> Result<Vec<Utterance>> {
        ensure!(packet.source == self.source, "source mismatch");
        ensure!(
            packet.len <= PACKET_FRAMES && (8_000..=192_000).contains(&packet.sample_rate),
            "invalid audio packet"
        );
        ensure!(
            packet.samples[..packet.len].iter().all(|f| f.is_finite()),
            "invalid audio sample"
        );
        let next_sequence = packet
            .sequence
            .checked_add(1)
            .context("audio sequence overflow")?;
        let next_start_ns = packet
            .start_ns
            .checked_add(packet.len as u64 * 1_000_000_000 / u64::from(packet.sample_rate))
            .context("audio timestamp overflow")?;
        let gap = self.next_sequence.is_some_and(|s| s != packet.sequence)
            || self.rate != 0 && self.rate != packet.sample_rate
            || self
                .next_start_ns
                .is_some_and(|t| t.abs_diff(packet.start_ns) > 250_000_000);
        let inputs = if gap {
            0
        } else {
            self.resampler
                .as_ref()
                .map_or(0, |resampler| resampler.input_count())
        };
        let inputs = inputs
            .checked_add(packet.len as u64)
            .context("audio sample count overflow")?;
        let origin = if gap || self.resampler.is_none() {
            packet.start_ns
        } else {
            self.stream_origin_ns
        };
        let stream_end_ns = u128::from(origin)
            + u128::from(inputs) * 1_000_000_000 / u128::from(packet.sample_rate);
        ensure!(
            stream_end_ns <= u128::from(u64::MAX),
            "audio timeline overflow"
        );
        ensure!(
            (u128::from(inputs) * u128::from(SAMPLE_RATE)).div_ceil(u128::from(packet.sample_rate))
                <= u128::from(u64::MAX),
            "audio output count overflow"
        );
        let gaps = self
            .gaps
            .checked_add(u64::from(gap))
            .context("audio gap count overflow")?;
        let mut result = vec![];
        if gap {
            if let Some(finalized) = self.finish() {
                result.push(finalized);
            }
        }
        self.gaps = gaps;
        self.next_sequence = Some(next_sequence);
        self.rate = packet.sample_rate;
        self.next_start_ns = Some(next_start_ns);
        if packet.len == 0 {
            return Ok(result);
        }
        if self.resampler.is_none() {
            self.resampler = Some(resample::Resampler::new(packet.sample_rate)?);
            self.stream_origin_ns = packet.start_ns;
        }
        let resampler = self.resampler.as_mut().unwrap();
        let first_output = resampler.output_count();
        let samples = resampler.push(&packet.samples[..packet.len])?;
        self.append_samples(first_output, samples, stream_end_ns as u64);
        if self.silent_samples >= 10_400 || self.pcm.len() >= SAMPLE_RATE as usize * 12 {
            if let Some(finalized) = self.finish_utterance() {
                result.push(finalized);
            }
        } else if self.pcm.len() >= self.last_partial + SAMPLE_RATE as usize * 2 {
            self.last_partial = self.pcm.len();
            result.push(self.snapshot(false));
        }
        Ok(result)
    }
    fn append_samples(&mut self, first_output: u64, samples: Vec<f32>, stream_end_ns: u64) {
        if samples.is_empty() {
            return;
        }
        let rms = (samples
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        if self.pcm.is_empty() && rms < 0.008 {
            return;
        }
        // shortcut: use the capture anchor and nominal rate; add drift correction after synchronized device-clock measurements.
        if self.pcm.is_empty() {
            self.start_ns = (u128::from(self.stream_origin_ns)
                + u128::from(first_output) * 1_000_000_000 / u128::from(SAMPLE_RATE))
                as u64;
        }
        self.end_ns = (u128::from(self.stream_origin_ns)
            + (u128::from(first_output) + samples.len() as u128) * 1_000_000_000
                / u128::from(SAMPLE_RATE))
        .min(u128::from(stream_end_ns)) as u64;
        self.silent_samples = if rms < 0.008 {
            self.silent_samples + samples.len()
        } else {
            0
        };
        self.pcm.extend(samples);
    }
    fn snapshot(&self, finalized: bool) -> Utterance {
        Utterance {
            id: self.id.clone(),
            source: self.source,
            start_ns: self.start_ns,
            end_ns: self.end_ns,
            samples: self.pcm.clone(),
            finalized,
        }
    }
    pub fn finish(&mut self) -> Option<Utterance> {
        if let Some(resampler) = self.resampler.take() {
            let stream_end_ns = (u128::from(self.stream_origin_ns)
                + u128::from(resampler.input_count()) * 1_000_000_000 / u128::from(self.rate))
                as u64;
            let first_output = resampler.output_count();
            self.append_samples(first_output, resampler.finish(), stream_end_ns);
        }
        self.rate = 0;
        self.next_sequence = None;
        self.next_start_ns = None;
        self.stream_origin_ns = 0;
        self.finish_utterance()
    }
    fn finish_utterance(&mut self) -> Option<Utterance> {
        if self.pcm.is_empty() {
            return None;
        }
        let result = self.snapshot(true);
        self.pcm.clear();
        self.silent_samples = 0;
        self.last_partial = 0;
        self.id = uuid::Uuid::new_v4().to_string();
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_saved_capture_selection_fails_before_any_native_device_access() {
        for (device, display) in [
            (Some("invalid-id"), None),
            (Some("coreaudio:"), None),
            (None, Some(0)),
        ] {
            let mut settings = cueglint_core::Settings::default();
            settings.microphone_device_id = device.map(str::to_owned);
            settings.display_id = display;
            let config = super::CaptureConfig::from(&settings);
            assert!(super::Capture::start(&config).is_err());
        }
    }

    use super::*;
    fn packet(sequence: u64) -> AudioPacket {
        AudioPacket {
            source: Source::Microphone,
            sequence,
            start_ns: sequence * 64_000_000,
            sample_rate: 16_000,
            len: 1024,
            samples: [0.05; 1024],
        }
    }
    #[test]
    fn stopping_attempts_both_sources_and_preserves_both_errors() {
        let stopped = std::cell::RefCell::new(Vec::new());
        let error = stop_sources(
            || {
                stopped.borrow_mut().push(Source::Microphone);
                Err(std::io::Error::other("microphone pause failed").into())
            },
            || {
                stopped.borrow_mut().push(Source::SystemAudio);
                bail!("system capture stop failed")
            },
        )
        .unwrap_err();
        assert_eq!(*stopped.borrow(), [Source::Microphone, Source::SystemAudio]);
        let message = format!("{error:#}");
        assert!(message.contains("microphone pause failed"));
        assert!(message.contains("system capture stop failed"));
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert!(stop_sources(|| Ok(()), || Ok(())).is_ok());
        let system_error = stop_sources(|| Ok(()), || bail!("system only")).unwrap_err();
        assert!(system_error.to_string().contains("system only"));
    }
    #[test]
    fn stopping_closes_the_callback_gate_and_is_idempotent() {
        let (_, packets) = mpsc::sync_channel(1);
        let mut capture = Capture {
            microphone: None,
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            system: None,
            packets,
            #[cfg(target_os = "macos")]
            timing: super::mac_timing::Timing::new().unwrap(),
            dropped_packets: Arc::new(AtomicU64::new(0)),
            device_errors: Arc::new(AtomicU64::new(0)),
            muted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        capture.stop().unwrap();
        capture.stop().unwrap();
        assert!(capture.muted.load(Ordering::Relaxed));
    }
    #[test]
    fn transcript_partials_keep_identity_and_stop_flushes_last_utterance() {
        let mut e = Endpoint::new(Source::Microphone);
        let mut updates = vec![];
        for i in 0..40 {
            updates.extend(e.push(&packet(i)).unwrap());
        }
        let final_update = e.finish().unwrap();
        assert!(!updates.is_empty());
        assert_eq!(updates[0].id, final_update.id);
        assert!(final_update.finalized);
        assert_eq!(final_update.samples.len(), 40960);
        assert!(e.finish().is_none());
    }
    #[test]
    fn gap_finalizes_instead_of_joining_unrelated_audio() {
        let mut e = Endpoint::new(Source::Microphone);
        e.push(&packet(0)).unwrap();
        let old = e.push(&packet(2)).unwrap();
        assert_eq!(e.gaps, 1);
        assert_eq!(old.len(), 1);
        assert!(old[0].finalized);
        assert_ne!(old[0].id, e.finish().unwrap().id);
    }
    #[test]
    fn invalid_samples_and_source_do_not_enter_the_pipeline() {
        let mut e = Endpoint::new(Source::Microphone);
        let mut p = packet(0);
        p.samples[0] = f32::NAN;
        assert!(e.push(&p).is_err());
        p.samples[0] = 0.0;
        p.source = Source::SystemAudio;
        assert!(e.push(&p).is_err());
    }
    #[test]
    fn sleep_gap_does_not_create_an_overlong_utterance_and_silence_is_not_a_gap() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        for sequence in 0..10 {
            let mut p = packet(sequence);
            p.samples.fill(0.0);
            endpoint.push(&p).unwrap();
        }
        assert_eq!(endpoint.gaps, 0);
        endpoint.push(&packet(10)).unwrap();
        let mut resumed = packet(11);
        resumed.start_ns = 60_000_000_000;
        let before_sleep = endpoint.push(&resumed).unwrap();
        assert_eq!(endpoint.gaps, 1);
        assert_eq!(before_sleep.len(), 1);
        assert!(before_sleep[0].finalized);
        assert!(before_sleep[0].end_ns < resumed.start_ns);
        assert!(endpoint.finish().unwrap().end_ns - resumed.start_ns < 1_000_000_000);
    }

    fn original_block(
        rate: u32,
        sequence: u64,
        start_ns: u64,
        len: usize,
        value: f32,
    ) -> AudioPacket {
        AudioPacket {
            source: Source::Microphone,
            sequence,
            start_ns,
            sample_rate: rate,
            len,
            samples: [value; PACKET_FRAMES],
        }
    }

    #[test]
    fn endpoint_keeps_the_first_and_last_sixteen_khz_samples_and_exact_times() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        let mut input = original_block(16_000, 0, 123_000, 3, 0.0);
        input.samples[..3].copy_from_slice(&[0.3, -0.7, 0.5]);
        assert!(endpoint.push(&input).unwrap().is_empty());
        let finalized = endpoint.finish().unwrap();
        assert_eq!(finalized.samples, input.samples[..3]);
        assert_eq!(finalized.start_ns, 123_000);
        assert_eq!(finalized.end_ns, 123_000 + 3 * 62_500);
        assert!(endpoint.finish().is_none());
    }

    #[test]
    fn stopping_before_filter_lookahead_flushes_the_complete_short_source_once() {
        for rate in [8_000, 11_025, 22_050, 44_100, 48_000, 96_000, 192_000] {
            for len in [1, 17, 257] {
                let mut endpoint = Endpoint::new(Source::Microphone);
                let input = original_block(rate, 0, 1_234_567_890, len, 0.5);
                assert!(endpoint.push(&input).unwrap().is_empty());
                let finalized = endpoint.finish().unwrap();
                assert_eq!(
                    finalized.samples.len(),
                    (len as u64 * u64::from(SAMPLE_RATE)).div_ceil(u64::from(rate)) as usize,
                    "rate{rate},len{len}"
                );
                assert_eq!(finalized.start_ns, input.start_ns);
                assert_eq!(
                    finalized.end_ns,
                    input.start_ns + len as u64 * 1_000_000_000 / u64::from(rate)
                );
                assert!(finalized.samples.iter().all(|sample| sample.is_finite()));
                assert!(endpoint.finish().is_none());
                assert!(endpoint.resampler.is_none());
            }
        }
    }

    #[test]
    fn sequence_rate_and_timestamp_gaps_flush_old_audio_and_reset_filter_history() {
        for (sequence, rate, start_ns) in [
            (2, 48_000, 2_000_354_166),
            (1, 44_100, 2_000_354_166),
            (1, 48_000, 60_000_000_000),
        ] {
            let mut endpoint = Endpoint::new(Source::Microphone);
            let old_input = original_block(48_000, 0, 2_000_000_000, 17, 0.5);
            endpoint.push(&old_input).unwrap();
            let new_input = original_block(rate, sequence, start_ns, 17, -0.5);
            let old = endpoint.push(&new_input).unwrap();
            assert_eq!(endpoint.gaps, 1);
            assert_eq!(old.len(), 1);
            assert!(old[0].finalized);
            assert_eq!(old[0].start_ns, old_input.start_ns);
            assert_eq!(
                old[0].end_ns,
                old_input.start_ns + 17 * 1_000_000_000 / 48_000
            );
            assert_eq!(old[0].samples.len(), 6);
            let current = endpoint.finish().unwrap();
            let mut independent = Endpoint::new(Source::Microphone);
            independent.push(&new_input).unwrap();
            let independent = independent.finish().unwrap();
            assert_eq!(current.samples, independent.samples);
            assert_eq!(
                (current.start_ns, current.end_ns),
                (independent.start_ns, independent.end_ns)
            );
            assert_ne!(old[0].id, current.id);
            assert!(endpoint.finish().is_none());
        }
    }

    #[test]
    fn empty_gap_packets_flush_the_old_tail_without_inventing_new_audio() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        let input = original_block(48_000, 0, 0, 17, 0.5);
        endpoint.push(&input).unwrap();
        let empty = original_block(48_000, 2, 354_166, 0, 0.0);
        let old = endpoint.push(&empty).unwrap();
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].samples.len(), 6);
        assert_eq!(endpoint.gaps, 1);
        assert!(endpoint.finish().is_none());
        assert!(endpoint.resampler.is_none());
    }

    #[test]
    fn twelve_second_utterance_cuts_keep_continuous_filter_samples_and_partial_ids() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        let rate = 8_000;
        let frames = rate as usize * 13;
        let mut finals = Vec::new();
        let mut partial_ids = Vec::new();
        for (sequence, offset) in (0..frames).step_by(PACKET_FRAMES).enumerate() {
            let input = original_block(
                rate,
                sequence as u64,
                offset as u64 * 1_000_000_000 / u64::from(rate),
                (frames - offset).min(PACKET_FRAMES),
                0.5,
            );
            for update in endpoint.push(&input).unwrap() {
                if update.finalized {
                    finals.push(update);
                } else {
                    partial_ids.push(update.id);
                }
            }
        }
        finals.push(endpoint.finish().unwrap());
        assert_eq!(finals.len(), 2);
        assert_ne!(finals[0].id, finals[1].id);
        assert!(
            partial_ids
                .iter()
                .all(|id| finals.iter().any(|finalized| &finalized.id == id))
        );
        assert_eq!(
            finals
                .iter()
                .map(|finalized| finalized.samples.len())
                .sum::<usize>(),
            13 * SAMPLE_RATE as usize
        );
        assert_eq!(finals[0].start_ns, 0);
        assert_eq!(finals[0].end_ns, finals[1].start_ns);
        assert_eq!(finals[1].end_ns, 13_000_000_000);
        let mut reference = resample::Resampler::new(rate).unwrap();
        let mut expected = Vec::new();
        for offset in (0..frames).step_by(PACKET_FRAMES) {
            expected.extend(
                reference
                    .push(&vec![0.5; (frames - offset).min(PACKET_FRAMES)])
                    .unwrap(),
            );
        }
        expected.extend(reference.finish());
        assert_eq!(
            finals
                .into_iter()
                .flat_map(|finalized| finalized.samples)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(endpoint.finish().is_none());
    }

    #[test]
    fn silence_cuts_do_not_reset_the_continuous_input_clock_or_filter() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        let rate = 44_100;
        let frames = rate as usize * 2;
        let mut finalized = Vec::new();
        for (sequence, offset) in (0..frames).step_by(PACKET_FRAMES).enumerate() {
            let mut input = original_block(
                rate,
                sequence as u64,
                offset as u64 * 1_000_000_000 / u64::from(rate),
                (frames - offset).min(PACKET_FRAMES),
                0.0,
            );
            for (index, sample) in input.samples[..input.len].iter_mut().enumerate() {
                if offset + index < rate as usize {
                    *sample = 0.5;
                }
            }
            finalized.extend(
                endpoint
                    .push(&input)
                    .unwrap()
                    .into_iter()
                    .filter(|update| update.finalized),
            );
        }
        assert_eq!(finalized.len(), 1);
        assert!(endpoint.pcm.is_empty());
        let resampler = endpoint.resampler.as_ref().unwrap();
        assert_eq!(resampler.input_count(), u64::from(rate) * 2);
        assert_eq!(
            resampler.output_count(),
            ((u64::from(rate) * 2 - 89) * u64::from(SAMPLE_RATE)).div_ceil(u64::from(rate))
        );
        assert_eq!(endpoint.gaps, 0);
        assert!(endpoint.finish().is_none());
    }

    #[test]
    fn invalid_sequence_and_timestamps_leave_the_pending_utterance_unchanged() {
        let mut endpoint = Endpoint::new(Source::Microphone);
        endpoint.push(&packet(0)).unwrap();
        let before = endpoint.snapshot(false);
        let mut invalid = packet(1);
        invalid.sequence = u64::MAX;
        assert!(endpoint.push(&invalid).is_err());
        invalid.sequence = 1;
        invalid.start_ns = u64::MAX;
        assert!(endpoint.push(&invalid).is_err());
        let after = endpoint.finish().unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.samples, before.samples);
        assert_eq!(
            (after.start_ns, after.end_ns),
            (before.start_ns, before.end_ns)
        );
        assert_eq!(endpoint.gaps, 0);
    }

    #[test]
    fn wav_boundary_rejects_the_wrong_rate_and_normalizes_pcm16() {
        let path = std::env::temp_dir().join(format!("cueglint-wav-{}.wav", uuid::Uuid::new_v4()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut w = hound::WavWriter::create(&path, spec).unwrap();
            w.write_sample(i16::MIN).unwrap();
            w.write_sample(i16::MAX).unwrap();
            w.finalize().unwrap();
        }
        let pcm = load_wav(&path).unwrap();
        let frozen_bytes = std::fs::read(&path).unwrap();
        assert_eq!(load_wav_bytes(&frozen_bytes).unwrap(), pcm);
        assert_eq!(pcm[0], -1.0);
        assert!(pcm[1] < 1.0);
        {
            let mut w = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    sample_rate: 48_000,
                    ..spec
                },
            )
            .unwrap();
            w.write_sample(1i16).unwrap();
            w.finalize().unwrap();
        }
        assert!(load_wav(&path).is_err());
        assert!(load_wav_bytes(&std::fs::read(&path).unwrap()).is_err());
        // A later pathname replacement cannot change the already frozen input.
        assert_eq!(load_wav_bytes(&frozen_bytes).unwrap(), pcm);
        std::fs::remove_file(path).unwrap();
    }
}
