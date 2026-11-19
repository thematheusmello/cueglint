//! Queued macOS PCM is projected onto the capture's host clock on the consumer.
use super::{AudioPacket, PACKET_FRAMES, PACKET_QUEUE_CAPACITY};
use anyhow::{Context, Result, ensure};
use screencapturekit::cm::{CMClock, CMTime};
use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::Receiver,
    },
    time::{Duration, Instant},
};

const NS_PER_SECOND: i128 = 1_000_000_000;
// shortcut: use 250 ms of presentation lead until native route timing is measured.
const MAX_CLOCK_LEAD_NS: i128 = 250_000_000;
const STOP_WAIT: Duration = Duration::from_millis(250);

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSyncConvertTime(time: CMTime, from: *const c_void, to: *const c_void) -> CMTime;
}

pub(super) enum QueuedPacket {
    Microphone(AudioPacket),
    System {
        packet: AudioPacket,
        pts: CMTime,
        buffer_sequence: u64,
        frame_offset_ns: u64,
    },
}

impl From<AudioPacket> for QueuedPacket {
    fn from(packet: AudioPacket) -> Self {
        Self::Microphone(packet)
    }
}

pub(super) struct Timing {
    pub(super) arrival_origin: Instant,
    pub(super) callbacks: Arc<CallbackGate>,
    pub(super) system_clock: Option<CMClock>,
    host_clock: CMClock,
    host_origin_ns: i128,
    system_buffer: Option<(u64, i128)>,
    last_microphone_start: Option<u64>,
    last_system_start: Option<u64>,
    pub(super) final_packets: Vec<Result<AudioPacket>>,
}

impl Timing {
    pub(super) fn new() -> Result<Self> {
        let host_clock = CMClock::host_time_clock();
        let before = Instant::now();
        let host_origin_ns =
            numeric_ns(host_clock.time()).context("reading macOS capture clock")?;
        ensure!(host_origin_ns >= 0, "macOS host clock preceded its epoch");
        let arrival_origin = before + before.elapsed() / 2;
        Ok(Self {
            arrival_origin,
            callbacks: Arc::default(),
            system_clock: None,
            host_clock,
            host_origin_ns,
            system_buffer: None,
            last_microphone_start: None,
            last_system_start: None,
            final_packets: Vec::new(),
        })
    }

    pub(super) fn normalize(&mut self, queued: QueuedPacket) -> Result<Option<AudioPacket>> {
        let (packet, host_ns, previous) = match queued {
            QueuedPacket::Microphone(packet) => {
                let host_ns = i128::from(packet.start_ns);
                (packet, host_ns, &mut self.last_microphone_start)
            }
            QueuedPacket::System {
                packet,
                pts,
                buffer_sequence,
                frame_offset_ns,
            } => {
                let Some(clock) = &self.system_clock else {
                    return Ok(Some(packet));
                };
                let buffer_start = match self.system_buffer {
                    Some((sequence, start)) if sequence == buffer_sequence => start,
                    _ => {
                        let start = convert(pts, clock, &self.host_clock).with_context(|| {
                            format!(
                                "converting ScreenCaptureKit audio packet {} PTS to host time",
                                packet.sequence,
                            )
                        })?;
                        self.system_buffer = Some((buffer_sequence, start));
                        start
                    }
                };
                (
                    packet,
                    buffer_start + i128::from(frame_offset_ns),
                    &mut self.last_system_start,
                )
            }
        };
        let source = packet.source;
        let sequence = packet.sequence;
        project(
            packet,
            host_ns,
            self.host_origin_ns,
            numeric_ns(self.host_clock.time()).context("reading current macOS host time")?,
            previous,
        )
        .with_context(|| format!("timing {source:?} audio packet {sequence}"))
    }

    pub(super) fn prepare_stop(
        &mut self,
        packets: &Receiver<QueuedPacket>,
        muted: &AtomicBool,
    ) -> Result<()> {
        if !self.callbacks.close() {
            return Ok(());
        }
        let quiet = self.callbacks.wait(STOP_WAIT);
        muted.store(true, Ordering::Relaxed);
        // The stream still owns its clock. The worker releases it before processing utterances.
        for packet in packets.try_iter().take(PACKET_QUEUE_CAPACITY) {
            match self.normalize(packet) {
                Ok(Some(packet)) => self.final_packets.push(Ok(packet)),
                Ok(None) => {}
                Err(error) => self.final_packets.push(Err(error)),
            }
        }
        quiet?;
        ensure!(
            packets.try_recv().is_err(),
            "macOS audio continued after callbacks closed; {PACKET_QUEUE_CAPACITY}-packet drain exhausted"
        );
        Ok(())
    }
}

fn numeric_ns(time: CMTime) -> Result<i128> {
    ensure!(
        time.is_numeric() && time.timescale > 0 && time.epoch == 0,
        "macOS audio clock returned an invalid timestamp"
    );
    Ok(i128::from(time.value) * NS_PER_SECOND / i128::from(time.timescale))
}

fn convert(pts: CMTime, stream: &CMClock, host: &CMClock) -> Result<i128> {
    numeric_ns(pts)?;
    // Both retained references outlive the call, and the wrapper's CMTime has the SDK's C layout.
    numeric_ns(unsafe { CMSyncConvertTime(pts, stream.as_ptr(), host.as_ptr()) })
}

fn project(
    mut packet: AudioPacket,
    host_ns: i128,
    origin_ns: i128,
    now_ns: i128,
    previous: &mut Option<u64>,
) -> Result<Option<AudioPacket>> {
    ensure!(
        packet.len > 0 && packet.len <= PACKET_FRAMES && packet.sample_rate > 0,
        "invalid native audio packet"
    );
    ensure!(
        host_ns >= 0,
        "macOS audio timestamp preceded the host epoch"
    );
    ensure!(
        host_ns <= now_ns + MAX_CLOCK_LEAD_NS,
        "macOS audio timestamp is in the future"
    );
    let mut start = host_ns - origin_ns;
    let rate = i128::from(packet.sample_rate);
    if start < 0 {
        ensure!(previous.is_none(), "macOS audio clock moved backwards");
        // Keep the sampled portion that falls inside this capture.
        let skip = (-start * rate + NS_PER_SECOND - 1) / NS_PER_SECOND;
        if skip >= packet.len as i128 {
            return Ok(None);
        }
        packet.samples.copy_within(skip as usize..packet.len, 0);
        packet.len -= skip as usize;
        start += skip * NS_PER_SECOND / rate;
    }

    let start_ns = u64::try_from(start).context("macOS audio timestamp overflow")?;
    u64::try_from(start + packet.len as i128 * NS_PER_SECOND / rate)
        .context("macOS audio packet end overflow")?;
    ensure!(
        previous.is_none_or(|last| start_ns > last),
        "macOS audio clock repeated or moved backwards"
    );
    *previous = Some(start_ns);
    packet.start_ns = start_ns;
    Ok(Some(packet))
}

const CLOSED: usize = 1;
const ACTIVE: usize = 2;

#[derive(Default)]
pub(super) struct CallbackGate(AtomicUsize);

impl CallbackGate {
    pub(super) fn enter(&self) -> Option<CallbackGuard<'_>> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                if state & CLOSED == 0 {
                    state.checked_add(ACTIVE)
                } else {
                    None
                }
            })
            .ok()?;
        Some(CallbackGuard(self))
    }

    fn close(&self) -> bool {
        self.0.fetch_or(CLOSED, Ordering::AcqRel) & CLOSED == 0
    }

    fn wait(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        while self.0.load(Ordering::Acquire) != CLOSED {
            let left = timeout.saturating_sub(start.elapsed());
            ensure!(
                !left.is_zero(),
                "macOS audio callbacks did not finish before stop; queued tail may be incomplete"
            );
            std::thread::sleep(left.min(Duration::from_millis(1)));
        }
        Ok(())
    }
}

pub(super) struct CallbackGuard<'a>(&'a CallbackGate);

impl Drop for CallbackGuard<'_> {
    fn drop(&mut self) {
        self.0.0.fetch_sub(ACTIVE, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::super::Source;
    use super::*;
    use std::sync::mpsc;

    fn pcm(source: Source, sequence: u64, start_ns: u64) -> AudioPacket {
        AudioPacket {
            source,
            sequence,
            start_ns,
            sample_rate: 48_000,
            len: PACKET_FRAMES,
            samples: [0.25; PACKET_FRAMES],
        }
    }

    fn system(pts: CMTime, sequence: u64, offset: u64) -> QueuedPacket {
        QueuedPacket::System {
            packet: pcm(Source::SystemAudio, sequence, 999_000_000 + offset),
            pts,
            buffer_sequence: 0,
            frame_offset_ns: offset,
        }
    }

    #[test]
    fn core_media_host_clock_conversion_preserves_numeric_identity() {
        let host = CMClock::host_time_clock();
        let time = host.time();
        assert_eq!(
            convert(time, &host, &host).unwrap(),
            numeric_ns(time).unwrap()
        );
        for invalid in [
            CMTime::INVALID,
            CMTime::new(2, 0),
            CMTime::indefinite(),
            CMTime::positive_infinity(),
            CMTime { epoch: 1, ..time },
        ] {
            assert!(convert(invalid, &host, &host).is_err());
        }
    }

    #[test]
    fn queued_startup_audio_and_microphone_share_one_host_origin() {
        let mut timing = Timing::new().unwrap();
        let sample_start = timing.host_origin_ns;
        timing.host_origin_ns -= 2_000_000_000;
        let (tx, rx) = mpsc::sync_channel(PACKET_QUEUE_CAPACITY);
        let pts = CMTime::new(i64::try_from(sample_start).unwrap(), 1_000_000_000);
        for (index, offset) in [0, 21_333_333, 42_666_666].into_iter().enumerate() {
            tx.try_send(system(pts, index as u64, offset)).unwrap();
        }
        let microphone = pcm(Source::Microphone, 0, u64::try_from(sample_start).unwrap());
        tx.try_send(QueuedPacket::Microphone(microphone)).unwrap();
        timing.system_clock = Some(timing.host_clock.clone());

        let first = timing.normalize(rx.try_recv().unwrap()).unwrap().unwrap();
        let muted = AtomicBool::new(false);
        timing.prepare_stop(&rx, &muted).unwrap();
        assert!(muted.load(Ordering::Relaxed));
        timing.system_clock.take();
        let packets: Vec<_> = std::iter::once(first)
            .chain(timing.final_packets.drain(..).map(Result::unwrap))
            .collect();
        assert_eq!(
            packets
                .iter()
                .map(|packet| packet.start_ns)
                .collect::<Vec<_>>(),
            [2_000_000_000, 2_021_333_333, 2_042_666_666, 2_000_000_000]
        );
        assert!(packets.iter().all(|packet| packet.samples[0] == 0.25));
        assert_eq!(packets[0].source, Source::SystemAudio);
        assert_eq!(packets[3].source, Source::Microphone);
    }

    #[test]
    fn missing_system_clock_keeps_arrival_time() {
        let mut timing = Timing::new().unwrap();
        for sequence in [0, 5] {
            let packet = timing
                .normalize(system(CMTime::INVALID, sequence, 0))
                .unwrap()
                .unwrap();
            assert_eq!(packet.sequence, sequence);
            assert_eq!(packet.start_ns, 999_000_000);
        }
    }

    #[test]
    fn projection_trims_pre_origin_frames_and_rejects_invalid_clock_positions() {
        let mut previous = None;
        let mut first = pcm(Source::Microphone, 0, 0);
        first.sample_rate = 16_000;
        first.len = 4;
        first.samples[..4].copy_from_slice(&[0.25, 0.5, 0.75, 1.0]);
        assert!(
            project(first.clone(), 100_000, 1_000_000, 1_000_000, &mut previous)
                .unwrap()
                .is_none()
        );
        let inside = project(first.clone(), 875_000, 1_000_000, 1_000_000, &mut previous)
            .unwrap()
            .unwrap();
        assert_eq!(inside.start_ns, 0);
        assert_eq!(&inside.samples[..inside.len], &[0.75, 1.0]);
        for (host_ns, now_ns) in [
            (-1, 1_000_000),
            (999_999, 1_000_000),
            (1_000_000, 1_000_000),
            (1_000_000 + MAX_CLOCK_LEAD_NS + 1, 1_000_000),
            (
                i128::from(u64::MAX) + 1_000_000,
                i128::from(u64::MAX) + 1_000_000,
            ),
        ] {
            assert!(project(first.clone(), host_ns, 1_000_000, now_ns, &mut previous).is_err());
        }
        assert_eq!(previous, Some(0));
    }

    #[test]
    fn stop_prepares_tail_once_and_reports_in_flight_timeout() {
        let mut timing = Timing::new().unwrap();
        let time = timing.host_clock.time();
        timing.system_clock = Some(timing.host_clock.clone());
        let (tx, rx) = mpsc::sync_channel(PACKET_QUEUE_CAPACITY);
        let callbacks = Arc::clone(&timing.callbacks);
        let admitted = callbacks.enter().unwrap();
        let muted = AtomicBool::new(false);
        tx.try_send(system(time, 0, 0)).unwrap();
        let mut malformed = system(CMTime::INVALID, 1, 0);
        if let QueuedPacket::System {
            buffer_sequence, ..
        } = &mut malformed
        {
            *buffer_sequence = 1;
        }
        tx.try_send(malformed).unwrap();
        tx.try_send(QueuedPacket::Microphone(pcm(
            Source::Microphone,
            0,
            u64::try_from(numeric_ns(time).unwrap()).unwrap(),
        )))
        .unwrap();
        let failure = timing.prepare_stop(&rx, &muted).unwrap_err();
        assert!(failure.to_string().contains("callbacks did not finish"));
        assert!(muted.load(Ordering::Relaxed));
        assert!(callbacks.enter().is_none());
        drop(admitted);
        callbacks.wait(Duration::ZERO).unwrap();
        timing.system_clock = None;
        timing.prepare_stop(&rx, &muted).unwrap();
        assert_eq!(timing.final_packets.len(), 3);
        let first = timing.final_packets[0].as_ref().unwrap();
        let last = timing.final_packets[2].as_ref().unwrap();
        assert_eq!(first.source, Source::SystemAudio);
        assert_eq!(last.source, Source::Microphone);
        assert_eq!(first.start_ns, last.start_ns);
        assert!(timing.final_packets[1].is_err());
        assert!(rx.try_recv().is_err());
    }
}
