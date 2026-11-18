//! Typed CPAL PCM input; conversion and packet delivery never wait in a callback.
use super::{AudioPacket, PACKET_FRAMES, Source};
use anyhow::{Result, bail, ensure};
use cpal::{FromSample, Sample, SampleFormat};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::SyncSender,
};

macro_rules! pcm_type {
    ($format:expr, $sample:ident, $body:block) => {{
        match $format {
            SampleFormat::I8 => {
                type $sample = i8;
                $body
            }
            SampleFormat::I16 => {
                type $sample = i16;
                $body
            }
            SampleFormat::I24 => {
                type $sample = cpal::I24;
                $body
            }
            SampleFormat::I32 => {
                type $sample = i32;
                $body
            }
            SampleFormat::I64 => {
                type $sample = i64;
                $body
            }
            SampleFormat::U8 => {
                type $sample = u8;
                $body
            }
            SampleFormat::U16 => {
                type $sample = u16;
                $body
            }
            SampleFormat::U24 => {
                type $sample = cpal::U24;
                $body
            }
            SampleFormat::U32 => {
                type $sample = u32;
                $body
            }
            SampleFormat::U64 => {
                type $sample = u64;
                $body
            }
            SampleFormat::F32 => {
                type $sample = f32;
                $body
            }
            SampleFormat::F64 => {
                type $sample = f64;
                $body
            }
            format => bail!("microphone format {format:?} is not supported PCM"),
        }
    }};
}

#[cfg(not(target_os = "windows"))]
pub(super) fn build(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
    origin: std::time::Instant,
) -> Result<cpal::Stream> {
    use anyhow::Context;
    use cpal::traits::DeviceTrait;

    let mut packets = Packets::new(
        usize::from(config.channels()),
        config.sample_rate(),
        tx,
        dropped,
        errors.clone(),
        muted,
    )?;
    pcm_type!(config.sample_format(), Pcm, {
        device
            .build_input_stream(
                config.config(),
                move |data: &[Pcm], info: &cpal::InputCallbackInfo| {
                    let timestamp = info.timestamp();
                    let latency = timestamp
                        .callback
                        .duration_since(timestamp.capture)
                        .as_nanos();
                    let Ok(start) =
                        u64::try_from(origin.elapsed().as_nanos().saturating_sub(latency))
                    else {
                        packets.error();
                        return;
                    };
                    packets.push(data, start);
                },
                move |_error: cpal::Error| {
                    errors.fetch_add(1, Ordering::Relaxed);
                },
                None,
            )
            .context("building microphone PCM stream")
    })
}

struct Packets {
    channels: usize,
    rate: u32,
    sequence: u64,
    tx: SyncSender<AudioPacket>,
    dropped: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
}

impl Packets {
    fn new(
        channels: usize,
        rate: u32,
        tx: SyncSender<AudioPacket>,
        dropped: Arc<AtomicU64>,
        errors: Arc<AtomicU64>,
        muted: Arc<AtomicBool>,
    ) -> Result<Self> {
        ensure!(
            (1..=32).contains(&channels),
            "unsupported microphone channel count"
        );
        ensure!(
            (8_000..=192_000).contains(&rate),
            "unsupported microphone sample rate"
        );
        Ok(Self {
            channels,
            rate,
            sequence: 0,
            tx,
            dropped,
            errors,
            muted,
        })
    }

    fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn push<T: Sample>(&mut self, data: &[T], start_ns: u64)
    where
        f32: FromSample<T>,
    {
        if data.len() % self.channels != 0 {
            self.error();
            return;
        }
        for (index, block) in data.chunks(self.channels * PACKET_FRAMES).enumerate() {
            let start = u128::from(start_ns)
                + index as u128 * PACKET_FRAMES as u128 * 1_000_000_000 / u128::from(self.rate);
            let frames = block.len() / self.channels;
            let end = start + frames as u128 * 1_000_000_000 / u128::from(self.rate);
            let Some(next_sequence) = self.sequence.checked_add(1) else {
                self.error();
                return;
            };
            if end > u128::from(u64::MAX) {
                self.error();
                return;
            }
            let sequence = self.sequence;
            self.sequence = next_sequence;
            if self.muted.load(Ordering::Relaxed) {
                continue;
            }
            let mut packet = AudioPacket {
                source: Source::Microphone,
                sequence,
                start_ns: start as u64,
                sample_rate: self.rate,
                len: frames,
                samples: [0.0; PACKET_FRAMES],
            };
            for (out, frame) in packet
                .samples
                .iter_mut()
                .zip(block.chunks_exact(self.channels))
            {
                *out = frame
                    .iter()
                    .map(|sample| (*sample).to_sample::<f32>())
                    .sum::<f32>()
                    / self.channels as f32;
            }
            if self.tx.try_send(packet).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{Endpoint, PACKET_QUEUE_CAPACITY};
    use std::sync::mpsc::{self, Receiver};

    fn writer(channels: usize, rate: u32, capacity: usize) -> (Packets, Receiver<AudioPacket>) {
        let (tx, received) = mpsc::sync_channel(capacity);
        let packets = Packets::new(
            channels,
            rate,
            tx,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        (packets, received)
    }

    fn capture<T: Sample>(
        input: &[T],
        channels: usize,
        rate: u32,
        blocks: &[usize],
    ) -> Vec<AudioPacket>
    where
        f32: FromSample<T>,
    {
        assert_eq!(input.len() % channels, 0);
        let frames = input.len() / channels;
        let (mut writer, received) = writer(channels, rate, PACKET_QUEUE_CAPACITY);
        let mut packets = Vec::new();
        let mut offset = 0;
        let mut block = 0;
        while offset < frames {
            let end = (offset + blocks[block % blocks.len()]).min(frames);
            let start = 123_000_000 + offset as u64 * 1_000_000_000 / u64::from(rate);
            writer.push(&input[offset * channels..end * channels], start);
            packets.extend(received.try_iter());
            offset = end;
            block += 1;
        }
        assert_eq!(writer.errors.load(Ordering::Relaxed), 0);
        assert_eq!(writer.dropped.load(Ordering::Relaxed), 0);
        packets
    }

    fn flatten(packets: &[AudioPacket]) -> Vec<f32> {
        packets
            .iter()
            .flat_map(|packet| packet.samples[..packet.len].iter().copied())
            .collect()
    }

    fn dispatched(
        format: SampleFormat,
        entered: &std::cell::Cell<bool>,
    ) -> Result<(SampleFormat, usize)> {
        pcm_type!(format, Pcm, {
            entered.set(true);
            Ok((
                <Pcm as cpal::SizedSample>::FORMAT,
                std::mem::size_of::<Pcm>(),
            ))
        })
    }

    const PCM: [(SampleFormat, usize); 12] = [
        (SampleFormat::I8, 1),
        (SampleFormat::I16, 2),
        (SampleFormat::I24, 4),
        (SampleFormat::I32, 4),
        (SampleFormat::I64, 8),
        (SampleFormat::U8, 1),
        (SampleFormat::U16, 2),
        (SampleFormat::U24, 4),
        (SampleFormat::U32, 4),
        (SampleFormat::U64, 8),
        (SampleFormat::F32, 4),
        (SampleFormat::F64, 8),
    ];

    #[test]
    fn actual_pcm_types_are_dispatched_and_dsd_never_enters_the_builder() {
        for (format, size) in PCM {
            let entered = std::cell::Cell::new(false);
            assert_eq!(dispatched(format, &entered).unwrap(), (format, size));
            assert!(entered.get());
        }
        for format in [
            SampleFormat::DsdU8,
            SampleFormat::DsdU16,
            SampleFormat::DsdU32,
        ] {
            let entered = std::cell::Cell::new(false);
            assert!(dispatched(format, &entered).is_err());
            assert!(!entered.get());
        }
    }

    fn normalized<T: Sample>(input: &[T], centered: &[i128], bits: u32)
    where
        f32: FromSample<T>,
    {
        let actual = flatten(&capture(input, 1, 16_000, &[PACKET_FRAMES]));
        let scale = (1_u128 << (bits - 1)) as f32;
        let expected: Vec<_> = centered.iter().map(|value| *value as f32 / scale).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_integer_type_is_centered_and_normalized_with_exact_f32_rounding() {
        macro_rules! signed {
            ($sample:ty, $bits:expr) => {{
                let input = [
                    <$sample>::MIN,
                    -1,
                    0,
                    1,
                    (1_i128 << ($bits - 3)) as $sample,
                    <$sample>::MAX,
                ];
                normalized(&input, &input.map(|value| value as i128), $bits);
            }};
        }
        macro_rules! unsigned {
            ($sample:ty, $bits:expr) => {{
                let midpoint = 1_u128 << ($bits - 1);
                let input = [
                    0 as $sample,
                    (midpoint - 1) as $sample,
                    midpoint as $sample,
                    (midpoint + 1) as $sample,
                    (midpoint + (1 << ($bits - 3))) as $sample,
                    <$sample>::MAX,
                ];
                normalized(
                    &input,
                    &input.map(|value| value as i128 - midpoint as i128),
                    $bits,
                );
            }};
        }
        signed!(i8, 8);
        signed!(i16, 16);
        signed!(i32, 32);
        signed!(i64, 64);
        unsigned!(u8, 8);
        unsigned!(u16, 16);
        unsigned!(u32, 32);
        unsigned!(u64, 64);
        let centered = [-8_388_608, -1, 0, 1, 2_097_152, 8_388_607];
        normalized(
            &centered.map(|value| cpal::I24::new(value).unwrap()),
            &centered.map(i128::from),
            24,
        );
        let unsigned = [0, 8_388_607, 8_388_608, 8_388_609, 10_485_760, 16_777_215];
        normalized(
            &unsigned.map(|value| cpal::U24::new(value).unwrap()),
            &centered.map(i128::from),
            24,
        );
    }

    fn original_float_frames(frames: usize, channels: usize) -> Vec<f32> {
        const VALUES: [f32; 8] = [-1.0, -0.7, -0.0, 0.0, 0.2, 0.5, 0.875, 1.0];
        (0..frames * channels)
            .map(|index| {
                let frame = index / channels;
                VALUES[(index + 2 * frame) % VALUES.len()] * ((frame % 7 + 1) as f32 / 8.0)
            })
            .collect()
    }

    #[test]
    fn float32_mixing_is_bit_identical_to_the_previous_callback() {
        for channels in [1, 2, 32] {
            let input = original_float_frames(2_065, channels);
            let expected: Vec<_> = input
                .chunks_exact(channels)
                .map(|frame| (frame.iter().copied().sum::<f32>() / channels as f32).to_bits())
                .collect();
            let actual: Vec<_> = flatten(&capture(&input, channels, 44_100, &[1, 7, 257, 1_024]))
                .iter()
                .map(|sample| sample.to_bits())
                .collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn float64_is_converted_per_sample_before_the_same_channel_mix() {
        let values = [
            -1.0,
            -0.5,
            -0.0,
            0.0,
            0.5,
            1.0,
            f64::MIN_POSITIVE,
            f64::from(f32::MIN_POSITIVE),
            f64::from(f32::MAX),
            f64::from(f32::MIN),
        ];
        for channels in [1, 2] {
            let expected: Vec<_> = values
                .chunks_exact(channels)
                .map(|frame| frame.iter().map(|value| *value as f32).sum::<f32>() / channels as f32)
                .collect();
            let actual = flatten(&capture(&values, channels, 16_000, &[PACKET_FRAMES]));
            assert_eq!(
                actual
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    fn framing<T: Sample>(input: &[T], channels: usize, rate: u32) -> Result<()>
    where
        f32: FromSample<T>,
    {
        let frames = input.len() / channels;
        let whole = capture(input, channels, rate, &[frames]);
        for blocks in [&[1][..], &[1, 7, 257, 1_024][..]] {
            let actual = capture(input, channels, rate, blocks);
            assert_eq!(
                flatten(&actual)
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>(),
                flatten(&whole)
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>()
            );
            let mut offset = 0;
            for (sequence, packet) in actual.iter().enumerate() {
                assert_eq!(packet.source, Source::Microphone);
                assert_eq!(packet.sample_rate, rate);
                assert_eq!(packet.sequence, sequence as u64);
                assert!((1..=PACKET_FRAMES).contains(&packet.len));
                let expected = 123_000_000 + offset as u64 * 1_000_000_000 / u64::from(rate);
                assert!(packet.start_ns.abs_diff(expected) <= 1_000_000_000 / u64::from(rate));
                assert!(
                    packet.samples[packet.len..]
                        .iter()
                        .all(|sample| sample.to_bits() == 0)
                );
                offset += packet.len;
            }
            assert_eq!(offset, frames);
        }
        Ok(())
    }

    #[test]
    fn all_pcm_formats_keep_first_last_frames_and_packet_partitions() -> Result<()> {
        for (format, _) in PCM {
            for channels in [1, 2, 32] {
                for frames in [1, 1_023, 1_024, 1_025, 2_065] {
                    pcm_type!(format, Pcm, {
                        let input: Vec<Pcm> = original_float_frames(frames, channels)
                            .into_iter()
                            .map(Pcm::from_sample)
                            .collect();
                        framing(&input, channels, 44_100)
                    })?;
                }
            }
        }
        Ok(())
    }

    #[test]
    fn nonfinite_samples_reach_the_shared_rejection_boundary_without_zero_fill() {
        let mut invalid = Vec::new();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            invalid.extend(capture(&[value], 1, 16_000, &[1]));
        }
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            f64::MIN,
        ] {
            invalid.extend(capture(&[value], 1, 16_000, &[1]));
        }
        for packet in invalid {
            assert!(!packet.samples[0].is_finite());
            let mut endpoint = Endpoint::new(Source::Microphone);
            let valid = capture(&[0.5_f32; 3], 1, 16_000, &[3]).pop().unwrap();
            endpoint.push(&valid).unwrap();
            let before = endpoint.snapshot(false);
            assert!(endpoint.push(&packet).is_err());
            let after = endpoint.finish().unwrap();
            assert_eq!(after.id, before.id);
            assert_eq!(after.samples, before.samples);
            assert_eq!(
                (after.start_ns, after.end_ns),
                (before.start_ns, before.end_ns)
            );
        }
    }

    #[test]
    fn full_and_disconnected_queues_return_and_count_each_undelivered_packet() {
        let (mut writer, received) = writer(1, 16_000, 1);
        writer.push(&[0.5_f32; PACKET_FRAMES * 2 + 3], 0);
        assert_eq!(writer.sequence, 3);
        assert_eq!(writer.dropped.load(Ordering::Relaxed), 2);
        assert_eq!(writer.errors.load(Ordering::Relaxed), 0);
        let first = received.try_recv().unwrap();
        assert_eq!((first.sequence, first.len), (0, PACKET_FRAMES));
        writer.push(&[0.25_f32], 200_000_000);
        assert_eq!(received.try_recv().unwrap().sequence, 3);
        drop(received);
        writer.push(&[0.5_f32; PACKET_FRAMES + 1], 300_000_000);
        assert_eq!(writer.sequence, 6);
        assert_eq!(writer.dropped.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn mute_and_stop_gate_publish_nothing_and_preserve_sequence_gaps() {
        let (mut writer, received) = writer(1, 16_000, 4);
        writer.push(&[0.5_f32; 3], 0);
        let before = received.try_recv().unwrap();
        let mut endpoint = Endpoint::new(Source::Microphone);
        endpoint.push(&before).unwrap();
        writer.muted.store(true, Ordering::Relaxed);
        writer.push(&[0.5_f32; PACKET_FRAMES * 2 + 1], 64_000_000);
        assert!(received.try_recv().is_err());
        assert_eq!(writer.sequence, 4);
        writer.muted.store(false, Ordering::Relaxed);
        writer.push(&[0.25_f32], 192_187_500);
        let after = received.try_recv().unwrap();
        assert_eq!(after.sequence, 4);
        let old = endpoint.push(&after).unwrap();
        assert_eq!(old.len(), 1);
        assert!(old[0].finalized);
        assert_eq!(endpoint.gaps, 1);
        assert_ne!(old[0].id, endpoint.finish().unwrap().id);
        writer.muted.store(true, Ordering::Relaxed);
        writer.push(&[0.5_f32], 400_000_000);
        assert!(received.try_recv().is_err());
        assert_eq!(writer.dropped.load(Ordering::Relaxed), 0);
        assert_eq!(writer.errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn malformed_frames_and_invalid_configuration_fail_without_partial_publication() {
        for (channels, rate) in [
            (0, 16_000),
            (33, 16_000),
            (usize::MAX, 16_000),
            (1, 0),
            (1, 7_999),
            (1, 192_001),
            (1, u32::MAX),
        ] {
            let (tx, _) = mpsc::sync_channel(1);
            assert!(
                Packets::new(
                    channels,
                    rate,
                    tx,
                    Arc::new(AtomicU64::new(0)),
                    Arc::new(AtomicU64::new(0)),
                    Arc::new(AtomicBool::new(false))
                )
                .is_err()
            );
        }
        let (mut writer, received) = writer(2, 16_000, 4);
        writer.push(&[0.5_f32; 3], 0);
        assert!(received.try_recv().is_err());
        assert_eq!(writer.sequence, 0);
        assert_eq!(writer.errors.load(Ordering::Relaxed), 1);
        writer.push(&[] as &[f32], 0);
        assert_eq!(writer.sequence, 0);
        assert_eq!(writer.errors.load(Ordering::Relaxed), 1);
        writer.push(&[0.5_f32; 2], 0);
        assert_eq!(received.try_recv().unwrap().len, 1);
    }

    #[test]
    fn sequence_and_timestamp_overflow_keep_earlier_packets_and_report_the_error() {
        let (mut sequence, received) = writer(1, 16_000, 4);
        sequence.sequence = u64::MAX - 1;
        sequence.push(&[0.5_f32; PACKET_FRAMES + 1], 0);
        let retained = received.try_recv().unwrap();
        assert_eq!(retained.sequence, u64::MAX - 1);
        assert_eq!(retained.len, PACKET_FRAMES);
        assert_eq!(sequence.sequence, u64::MAX);
        assert_eq!(sequence.errors.load(Ordering::Relaxed), 1);
        assert!(received.try_recv().is_err());
        let (mut timestamp, received) = writer(1, 16_000, 4);
        timestamp.push(&[0.5_f32; PACKET_FRAMES + 1], u64::MAX - 64_000_000);
        assert_eq!(received.try_recv().unwrap().sequence, 0);
        assert_eq!(timestamp.sequence, 1);
        assert_eq!(timestamp.errors.load(Ordering::Relaxed), 1);
        assert!(received.try_recv().is_err());
    }
}
