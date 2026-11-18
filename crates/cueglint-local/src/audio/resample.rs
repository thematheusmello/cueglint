//! Bounded mono resampling on the capture consumer, never in a native callback.
use super::{PACKET_FRAMES, SAMPLE_RATE};
use anyhow::{Context, Result, ensure};
use std::collections::VecDeque;

const PHASES: usize = 64;

pub(super) struct Resampler {
    rate: u32,
    half: usize,
    coefficients: Vec<f32>,
    samples: VecDeque<f32>,
    first_input: u64,
    inputs: u64,
    outputs: u64,
}

impl Resampler {
    pub(super) fn new(rate: u32) -> Result<Self> {
        ensure!((8_000..=192_000).contains(&rate), "invalid audio rate");
        let half = if rate == SAMPLE_RATE {
            0
        } else {
            (32 * rate.max(SAMPLE_RATE)).div_ceil(SAMPLE_RATE) as usize
        };
        let mut coefficients = Vec::with_capacity((PHASES + 1) * (2 * half + 1));
        if half != 0 {
            let cutoff = 0.45 * f64::from(SAMPLE_RATE.min(rate)) / f64::from(rate);
            for phase in 0..=PHASES {
                let fraction = phase as f64 / PHASES as f64;
                let mut row = Vec::with_capacity(2 * half + 1);
                for tap in 0..=2 * half {
                    let distance = tap as f64 - half as f64 - fraction;
                    let window = if distance.abs() > half as f64 {
                        0.0
                    } else {
                        let angle = std::f64::consts::PI * distance / half as f64;
                        0.42 + 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos()
                    };
                    let angle = 2.0 * std::f64::consts::PI * cutoff * distance;
                    let sinc = if angle == 0.0 {
                        1.0
                    } else {
                        angle.sin() / angle
                    };
                    row.push(2.0 * cutoff * sinc * window);
                }
                let sum: f64 = row.iter().sum();
                coefficients.extend(row.into_iter().map(|value| (value / sum) as f32));
            }
        }
        Ok(Self {
            rate,
            half,
            coefficients,
            samples: VecDeque::with_capacity(
                PACKET_FRAMES + 2 * half + rate.div_ceil(SAMPLE_RATE) as usize + 1,
            ),
            first_input: 0,
            inputs: 0,
            outputs: 0,
        })
    }

    pub(super) fn input_count(&self) -> u64 {
        self.inputs
    }

    pub(super) fn output_count(&self) -> u64 {
        self.outputs
    }

    pub(super) fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        ensure!(samples.len() <= PACKET_FRAMES, "audio block is too large");
        ensure!(
            samples.iter().all(|value| value.is_finite()),
            "invalid audio sample"
        );
        let inputs = self
            .inputs
            .checked_add(samples.len() as u64)
            .context("audio sample count overflow")?;
        let outputs =
            (u128::from(inputs) * u128::from(SAMPLE_RATE)).div_ceil(u128::from(self.rate));
        ensure!(
            outputs <= u128::from(u64::MAX),
            "audio output count overflow"
        );
        self.inputs = inputs;
        if self.half == 0 {
            self.outputs = inputs;
            return Ok(samples.to_vec());
        }
        self.samples.extend(samples.iter().copied());
        Ok(self.render(false))
    }

    pub(super) fn finish(mut self) -> Vec<f32> {
        if self.half == 0 {
            return Vec::new();
        }
        self.render(true)
    }

    fn render(&mut self, finalizing: bool) -> Vec<f32> {
        let available = if finalizing {
            self.inputs
        } else {
            self.inputs.saturating_sub(self.half as u64)
        };
        let output_limit = (u128::from(available) * u128::from(SAMPLE_RATE))
            .div_ceil(u128::from(self.rate)) as u64;
        let mut result = Vec::with_capacity((output_limit - self.outputs) as usize);
        let taps = 2 * self.half + 1;
        while self.outputs < output_limit {
            let position = u128::from(self.outputs) * u128::from(self.rate);
            let center = (position / u128::from(SAMPLE_RATE)) as u64;
            let phase = (position % u128::from(SAMPLE_RATE)) as usize * PHASES;
            let row = phase / SAMPLE_RATE as usize;
            let mix = (phase % SAMPLE_RATE as usize) as f64 / f64::from(SAMPLE_RATE);
            let mut value = 0.0;
            for tap in 0..taps {
                let input = i128::from(center) + tap as i128 - self.half as i128;
                if input < 0 || input >= i128::from(self.inputs) {
                    continue;
                }
                let index = (input as u64 - self.first_input) as usize;
                let left = f64::from(self.coefficients[row * taps + tap]);
                let right = f64::from(self.coefficients[(row + 1) * taps + tap]);
                value += f64::from(self.samples[index]) * (left + (right - left) * mix);
            }
            // Filter overshoot can exceed f32's representable range for extreme finite inputs.
            result.push(value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32);
            self.outputs += 1;
        }
        let next_center =
            u128::from(self.outputs) * u128::from(self.rate) / u128::from(SAMPLE_RATE);
        let keep_from = next_center
            .saturating_sub(self.half as u128)
            .min(u128::from(self.inputs)) as u64;
        let discard = (keep_from - self.first_input) as usize;
        self.samples.drain(..discard);
        self.first_input = keep_from;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 9] = [
        8_000, 11_025, 16_000, 22_050, 24_000, 44_100, 48_000, 96_000, 192_000,
    ];

    fn convert(rate: u32, input: &[f32], blocks: &[usize]) -> Vec<f32> {
        let mut resampler = Resampler::new(rate).unwrap();
        let mut result = resampler.push(&[]).unwrap();
        let mut offset = 0;
        let mut block = 0;
        while offset < input.len() {
            let end = (offset + blocks[block % blocks.len()]).min(input.len());
            result.extend(resampler.push(&input[offset..end]).unwrap());
            offset = end;
            block += 1;
        }
        result.extend(resampler.finish());
        result
    }

    fn original_signal(len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| (((index * 197 + 31) % 1_024) as f32 - 512.0) / 1_024.0)
            .collect()
    }

    #[test]
    fn exact_counts_and_samples_do_not_depend_on_packet_partitioning() {
        for rate in RATES {
            for len in [
                0, 1, 2, 15, 31, 32, 63, 64, 65, 127, 128, 255, 256, 257, 1_023, 1_024, 1_025,
                4_099,
            ] {
                let input = original_signal(len);
                let expected = convert(rate, &input, &[PACKET_FRAMES]);
                assert_eq!(
                    expected.len(),
                    (len as u64 * u64::from(SAMPLE_RATE)).div_ceil(u64::from(rate)) as usize,
                    "rate{rate},len{len}"
                );
                let expected: Vec<_> = expected.iter().map(|sample| sample.to_bits()).collect();
                for blocks in [&[1][..], &[1, 7, 257, 1_024][..]] {
                    let actual: Vec<_> = convert(rate, &input, blocks)
                        .iter()
                        .map(|sample| sample.to_bits())
                        .collect();
                    assert_eq!(actual, expected, "rate{rate},len{len},blocks{blocks:?}");
                }
            }
        }
    }

    #[test]
    fn sixteen_khz_preserves_every_sample_bit_including_first_last_and_signed_zero() {
        let mut input = original_signal(4_099);
        input.splice(0..0, [f32::MIN, -0.0, 0.0, f32::MIN_POSITIVE, f32::MAX]);
        for blocks in [&[1][..], &[1, 7, 257, 1_024][..]] {
            let actual = convert(SAMPLE_RATE, &input, blocks);
            assert_eq!(
                actual
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>(),
                input
                    .iter()
                    .map(|sample| sample.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    fn tone(rate: u32, frequency: f64) -> Vec<f32> {
        (0..rate / 2)
            .map(|index| {
                (0.5 * (2.0 * std::f64::consts::PI * frequency * f64::from(index)
                    / f64::from(rate))
                .sin()) as f32
            })
            .collect()
    }

    fn stable_output(rate: u32, frequency: f64) -> Vec<f32> {
        let output = convert(rate, &tone(rate, frequency), &[1, 7, 257, 1_024]);
        // Fixed before execution: exclude exactly the approved source filter support at each edge.
        let edge = match rate {
            8_000 => 64,
            11_025 => 47,
            16_000 => 0,
            22_050 | 44_100 => 33,
            24_000 | 48_000 | 96_000 | 192_000 => 32,
            _ => panic!("spectral fixture rate was not predeclared"),
        };
        output[edge..output.len() - edge].to_vec()
    }

    fn amplitude(samples: &[f32], frequency: f64) -> f64 {
        let mut real = 0.0;
        let mut imaginary = 0.0;
        for (index, sample) in samples.iter().enumerate() {
            let angle =
                2.0 * std::f64::consts::PI * frequency * index as f64 / f64::from(SAMPLE_RATE);
            real += f64::from(*sample) * angle.cos();
            imaginary += f64::from(*sample) * angle.sin();
        }
        2.0 * real.hypot(imaginary) / samples.len() as f64
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt()
    }

    #[test]
    fn original_passband_tones_stay_within_quarter_decibel() {
        for rate in RATES {
            for frequency in [250.0, 1_000.0, 3_000.0, 6_000.0] {
                if frequency > 0.375 * f64::from(rate) {
                    continue;
                }
                let deviation_db =
                    20.0 * (amplitude(&stable_output(rate, frequency), frequency) / 0.5).log10();
                println!(
                    "resampler passband rate={rate} frequency={frequency} deviation_db={deviation_db:.6}"
                );
                assert!(
                    deviation_db.abs() <= 0.25,
                    "rate{rate},frequency{frequency},deviation{deviation_db}dB"
                );
            }
        }
    }

    #[test]
    fn original_alias_tones_are_rejected_by_at_least_forty_decibels() {
        for rate in [24_000, 44_100, 48_000, 96_000, 192_000] {
            for frequency in [8_500.0, 10_000.0, 12_000.0, 15_000.0, 20_000.0] {
                if frequency >= f64::from(rate) / 2.0 {
                    continue;
                }
                let ratio = rms(&stable_output(rate, frequency)) / (0.5 / 2.0_f64.sqrt());
                println!(
                    "resampler alias rate={rate} frequency={frequency} attenuation_db={:.6}",
                    20.0 * ratio.log10()
                );
                assert!(
                    ratio <= 0.01,
                    "rate{rate},frequency{frequency},ratio{ratio}"
                );
            }
        }
    }

    #[test]
    fn upsampling_images_are_rejected_by_at_least_forty_decibels() {
        for rate in [8_000, 11_025] {
            for frequency in [1_000.0, 3_000.0] {
                let output = stable_output(rate, frequency);
                let image = (f64::from(rate) - frequency) % f64::from(SAMPLE_RATE);
                let image = image.min(f64::from(SAMPLE_RATE) - image);
                let ratio = amplitude(&output, image) / amplitude(&output, frequency);
                println!(
                    "resampler image rate={rate} frequency={frequency} image={image} rejection_db={:.6}",
                    20.0 * ratio.log10()
                );
                assert!(
                    ratio <= 0.01,
                    "rate{rate},frequency{frequency},image{image},ratio{ratio}"
                );
            }
        }
    }

    #[test]
    fn history_and_tables_stay_bounded_during_continuous_input() {
        for rate in RATES {
            let mut resampler = Resampler::new(rate).unwrap();
            let half = if rate == SAMPLE_RATE {
                0
            } else {
                (32 * rate.max(SAMPLE_RATE)).div_ceil(SAMPLE_RATE) as usize
            };
            assert_eq!(resampler.half, half);
            assert!(resampler.coefficients.len() <= 49_985);
            let input = original_signal(PACKET_FRAMES);
            let mut output_count = 0;
            for _ in 0..500 {
                output_count += resampler.push(&input).unwrap().len();
                assert!(
                    resampler.samples.len()
                        <= PACKET_FRAMES + 2 * half + rate.div_ceil(SAMPLE_RATE) as usize + 1
                );
            }
            output_count += resampler.finish().len();
            assert_eq!(
                output_count as u64,
                (500 * PACKET_FRAMES as u64 * u64::from(SAMPLE_RATE)).div_ceil(u64::from(rate))
            );
        }
    }

    #[test]
    fn invalid_inputs_fail_before_changing_state() {
        for rate in [0, 7_999, 192_001, u32::MAX] {
            assert!(Resampler::new(rate).is_err());
        }
        let mut resampler = Resampler::new(48_000).unwrap();
        assert!(resampler.push(&[0.0; PACKET_FRAMES + 1]).is_err());
        assert!(resampler.push(&[f32::NAN]).is_err());
        assert!(resampler.push(&[f32::INFINITY]).is_err());
        assert_eq!(
            (resampler.inputs, resampler.outputs, resampler.samples.len()),
            (0, 0, 0)
        );
        resampler.inputs = u64::MAX;
        assert!(resampler.push(&[0.1]).is_err());
        assert_eq!(resampler.inputs, u64::MAX);
        assert!(resampler.samples.is_empty());
    }
}
