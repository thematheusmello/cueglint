//! Bounded source-labelled capture; transcription runs on a separate consumer.
use anyhow::{Context, Result, bail, ensure};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::Instant;

pub const SAMPLE_RATE: u32 = 16_000;
pub const PACKET_FRAMES: usize = 1024;

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

#[derive(Clone, Debug, Serialize)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
}

pub fn input_devices() -> Result<Vec<InputDevice>> {
    cpal::default_host()
        .input_devices()?
        .map(|d| {
            Ok(InputDevice {
                id: d.id()?.to_string(),
                name: d.description()?.name().to_owned(),
            })
        })
        .collect()
}

/// The fixture/import boundary is deliberately narrow and bounded, never guessed from extension.
pub fn load_wav(path: &std::path::Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
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
    microphone: Option<cpal::Stream>,
    #[cfg(target_os = "macos")]
    system: Option<screencapturekit::stream::SCStream>,
    pub packets: Receiver<AudioPacket>,
    pub dropped_packets: Arc<AtomicU64>,
    pub device_errors: Arc<AtomicU64>,
    pub muted: Arc<std::sync::atomic::AtomicBool>,
}

impl Capture {
    /// Source initialization is all-or-error; a missing system source is never disguised as mic capture.
    pub fn start(
        microphone: bool,
        system_audio: bool,
        device_id: Option<&str>,
        display_id: Option<u32>,
    ) -> Result<Self> {
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
        let (tx, packets) = mpsc::sync_channel(128);
        let dropped_packets = Arc::new(AtomicU64::new(0));
        let device_errors = Arc::new(AtomicU64::new(0));
        let muted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let origin = Instant::now();
        let mic = if microphone {
            Some(start_microphone(
                device_id,
                tx.clone(),
                dropped_packets.clone(),
                device_errors.clone(),
                muted.clone(),
                origin,
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
            )?)
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        if system_audio {
            let _ = display_id;
            bail!("system audio is not implemented for this platform yet");
        }
        Ok(Self {
            microphone: mic,
            #[cfg(target_os = "macos")]
            system,
            packets,
            dropped_packets,
            device_errors,
            muted,
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(mic) = self.microphone.take() {
            mic.pause().context("stopping microphone")?;
        }
        #[cfg(target_os = "macos")]
        if let Some(system) = self.system.take() {
            system
                .stop_capture()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Ok(())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn start_microphone(
    device_id: Option<&str>,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    muted: Arc<std::sync::atomic::AtomicBool>,
    origin: Instant,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => host.device_by_id(&id.parse()?),
        None => host.default_input_device(),
    }
    .context("no microphone available; choose a connected input device")?;
    let config = device
        .default_input_config()
        .context("microphone unavailable or permission denied")?;
    let channels = config.channels() as usize;
    ensure!(
        (1..=32).contains(&channels),
        "unsupported microphone channel count"
    );
    let rate = config.sample_rate();
    let err_fn = move |_error: cpal::Error| {
        errors.fetch_add(1, Ordering::Relaxed);
    };
    let mut sequence = 0;
    let mut capture = move |data: &[f32], info: &cpal::InputCallbackInfo| {
        let latency = info
            .timestamp()
            .callback
            .duration_since(info.timestamp().capture)
            .as_nanos() as u64;
        let start = (origin.elapsed().as_nanos() as u64).saturating_sub(latency);
        for (index, block) in data.chunks(channels * PACKET_FRAMES).enumerate() {
            let mut packet = AudioPacket {
                source: Source::Microphone,
                sequence,
                start_ns: start + index as u64 * PACKET_FRAMES as u64 * 1_000_000_000 / rate as u64,
                sample_rate: rate,
                len: block.len() / channels,
                samples: [0.0; PACKET_FRAMES],
            };
            sequence += 1;
            if muted.load(Ordering::Relaxed) {
                continue;
            }
            for (out, frame) in packet.samples.iter_mut().zip(block.chunks_exact(channels)) {
                *out = frame.iter().copied().sum::<f32>() / channels as f32;
            }
            if tx.try_send(packet).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    };
    // CoreAudio defaults to f32. Other formats fail visibly rather than reinterpret sample bytes.
    if config.sample_format() != cpal::SampleFormat::F32 {
        bail!(
            "microphone format {:?} is not supported yet; select a float32 device",
            config.sample_format()
        );
    }
    let stream = device.build_input_stream(
        config.config(),
        move |data: &[f32], info| capture(data, info),
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use screencapturekit::prelude::*;
    struct Output {
        tx: SyncSender<AudioPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        sequence: AtomicU64,
        muted: Arc<std::sync::atomic::AtomicBool>,
        origin: Instant,
    }
    impl SCStreamOutputTrait for Output {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Audio || self.muted.load(Ordering::Relaxed) {
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
            let rate = format.audio_sample_rate().unwrap_or(0.0) as u32;
            let channels = format.audio_channel_count().unwrap_or(0) as usize;
            // ScreenCaptureKit is configured for mono float32 PCM; reject any renegotiated layout.
            if rate != 48_000 || channels != 1 {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let start = self.origin.elapsed().as_nanos() as u64;
            for buffer in &list {
                let data = buffer.data();
                if data.len() % 4 != 0 {
                    self.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                for (index, block) in data.chunks(PACKET_FRAMES * 4).enumerate() {
                    let mut packet = AudioPacket {
                        source: Source::SystemAudio,
                        sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
                        start_ns: start
                            + index as u64 * PACKET_FRAMES as u64 * 1_000_000_000 / rate as u64,
                        sample_rate: rate,
                        len: block.len() / 4,
                        samples: [0.0; PACKET_FRAMES],
                    };
                    for (out, bytes) in packet.samples.iter_mut().zip(block.chunks_exact(4)) {
                        *out = f32::from_le_bytes(bytes.try_into().unwrap());
                    }
                    if self.tx.try_send(packet).is_err() {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
    pub fn start_system(
        display_id: Option<u32>,
        tx: SyncSender<AudioPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        muted: Arc<std::sync::atomic::AtomicBool>,
        origin: Instant,
    ) -> Result<SCStream> {
        crate::permissions::ensure_screen()?;
        let content = SCShareableContent::get().map_err(|e| {
            anyhow::anyhow!("system audio requires Screen Recording permission: {e}")
        })?;
        let display = content
            .displays()
            .into_iter()
            .find(|d| display_id.is_none_or(|id| d.display_id() == id))
            .context("selected display unavailable")?;
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
        let mut stream =
            SCStream::new(&filter, &config).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        stream
            .add_output_handler(
                Output {
                    tx,
                    dropped,
                    errors,
                    sequence: AtomicU64::new(0),
                    muted,
                    origin,
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
    phase: f64,
    previous: f32,
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
            phase: 0.0,
            previous: 0.0,
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
        let mut result = vec![];
        if self.next_sequence.is_some_and(|s| s != packet.sequence)
            || self.rate != 0 && self.rate != packet.sample_rate
            || self
                .next_start_ns
                .is_some_and(|t| t.abs_diff(packet.start_ns) > 250_000_000)
        {
            self.gaps += 1;
            if let Some(finalized) = self.finish() {
                result.push(finalized);
            }
            self.phase = 0.0;
            self.previous = 0.0;
        }
        self.next_sequence = Some(packet.sequence + 1);
        self.rate = packet.sample_rate;
        self.next_start_ns =
            Some(packet.start_ns + packet.len as u64 * 1_000_000_000 / packet.sample_rate as u64);
        if packet.len == 0 {
            return Ok(result);
        }
        let rms = (packet.samples[..packet.len]
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            / packet.len as f32)
            .sqrt();
        if self.pcm.is_empty() && rms < 0.008 {
            self.previous = packet.samples[packet.len - 1];
            return Ok(result);
        }
        if self.pcm.is_empty() {
            self.start_ns = packet.start_ns;
        }
        let ratio = packet.sample_rate as f64 / SAMPLE_RATE as f64;
        let mut resampled = Vec::with_capacity(PACKET_FRAMES * 2);
        // shortcut: linear resampling; replace with a bandlimited filter before advertising high-fidelity ASR across rates.
        while self.phase < packet.len as f64 {
            let index = self.phase.floor() as usize;
            let frac = (self.phase - index as f64) as f32;
            let left = if index == 0 {
                self.previous
            } else {
                packet.samples[index - 1]
            };
            let right = packet.samples[index];
            resampled.push(left + (right - left) * frac);
            self.phase += ratio;
        }
        self.phase -= packet.len as f64;
        self.previous = packet.samples[packet.len - 1];
        self.silent_samples = if rms < 0.008 {
            self.silent_samples + resampled.len()
        } else {
            0
        };
        self.pcm.extend(resampled);
        self.end_ns =
            packet.start_ns + packet.len as u64 * 1_000_000_000 / packet.sample_rate as u64;
        if self.silent_samples >= 10_400 || self.pcm.len() >= SAMPLE_RATE as usize * 12 {
            if let Some(finalized) = self.finish() {
                result.push(finalized);
            }
        } else if self.pcm.len() >= self.last_partial + SAMPLE_RATE as usize * 2 {
            self.last_partial = self.pcm.len();
            result.push(self.snapshot(false));
        }
        Ok(result)
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
        std::fs::remove_file(path).unwrap();
    }
}
