// SPDX-License-Identifier: MIT OR Apache-2.0
//! Original bounded Cueglint Whisper feature assembly.
//!
//! FFT helpers are separately attributed in asr_frontend/fft.rs. The associated
//! upstream license texts and modification notice are retained under
//! assets/licenses/asr-frontend/. See that NOTICE.md for pinned source identities.
//! Behavior follows standard Whisper preprocessing; no reference implementation
//! source was copied into this feature-assembly module.
mod fft;
use fft::fft;

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
/// Frontend behavior revision, independent of the model profile revision.
pub const REVISION: &str = "whisper-standard-v1";
/// Canonical configuration bytes for reproducible benchmark identities.
pub const CONFIGURATION: &str = r#"{"sample_rate":16000,"maximum_samples":480000,"output_shape":[80,3000],"fft_size":400,"hop_length":160,"window":"periodic-hann","centered":true,"padding":"reflect-logical-right-zero-padded-30s","spectrum":"one-sided-unscaled-complex-power","discard_final_stft_frame":true,"log10_floor":1e-10,"dynamic_range_log10":8,"normalization":"(clamped_log10_power+4)/4","fft_and_mel_accumulation":"float64","output":"float32","mel_filter_sha256":"85818f156f7e189453901a515e4726d270d307f976e161cf9403e8caab405498"}"#;

/// Hash the exact canonical configuration bytes used in runtime provenance.
pub fn configuration_sha256() -> String {
    format!("{:x}", Sha256::digest(CONFIGURATION.as_bytes()))
}

pub const SAMPLES: usize = 480_000;
pub const FRAMES: usize = 3_000;
const FFT: usize = 400;
const HOP: usize = 160;
const BINS: usize = FFT / 2 + 1;
const MELS: usize = 80;

/// Fixed-shape 80x3000 row-major features; bounded memory, no model or I/O.
pub fn pcm_to_mel(samples: &[f32], filters: &[f32], cancelled: &AtomicBool) -> Result<Vec<f32>> {
    if cancelled.load(Ordering::Relaxed) {
        bail!("transcription cancelled");
    }
    if samples.is_empty() || samples.len() > SAMPLES || samples.iter().any(|v| !v.is_finite()) {
        bail!("Whisper requires finite mono 16kHz samples, 0–30 seconds");
    }
    if filters.len() != MELS * BINS || filters.iter().any(|v| !v.is_finite() || *v < 0.0) {
        bail!("Whisper requires 80x201 finite nonnegative mel filters");
    }
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / FFT as f64).cos()))
        .collect();
    let mut log_mel = vec![-10.0f64; MELS * FRAMES];
    let mut frame = [0.0f64; FFT];
    let mut power = [0.0f64; BINS];
    for t in 0..FRAMES {
        if cancelled.load(Ordering::Relaxed) {
            bail!("transcription cancelled");
        }
        let mut nonzero = false;
        for j in 0..FFT {
            // Reflect the already right-zero-padded 30s input, not the short clip.
            let index = t as isize * HOP as isize + j as isize - (FFT / 2) as isize;
            let index = reflected_index(index, SAMPLES);
            frame[j] = f64::from(samples.get(index).copied().unwrap_or(0.0)) * window[j];
            nonzero |= frame[j] != 0.0;
        }
        if !nonzero {
            continue;
        }
        let spectrum = fft(&frame);
        for (k, value) in power.iter_mut().enumerate() {
            *value = spectrum[2 * k].powi(2) + spectrum[2 * k + 1].powi(2);
        }
        // One-sided complex power: do not add the negative-frequency mirror.
        for mel in 0..MELS {
            let sum: f64 = filters[mel * BINS..(mel + 1) * BINS]
                .iter()
                .zip(&power)
                .map(|(weight, value)| f64::from(*weight) * value)
                .sum();
            log_mel[mel * FRAMES + t] = sum.max(1e-10).log10();
        }
    }
    // Exactly 3000 frames: the centered STFT frame at 30s is never computed and
    // cannot affect the global dynamic-range floor.
    let floor = log_mel.iter().copied().fold(f64::NEG_INFINITY, f64::max) - 8.0;
    Ok(log_mel
        .into_iter()
        .map(|value| ((value.max(floor) + 4.0) / 4.0) as f32)
        .collect())
}

fn reflected_index(index: isize, length: usize) -> usize {
    if index < 0 {
        (-index) as usize
    } else if index >= length as isize {
        (2 * length as isize - 2 - index) as usize
    } else {
        index as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_configuration_pins_the_production_filter_asset() {
        let config: serde_json::Value = serde_json::from_str(CONFIGURATION).unwrap();
        assert_eq!(
            config["mel_filter_sha256"],
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../assets/melfilters.bytes"))
            )
        );
    }

    fn one_bin_filters(bin: usize) -> Vec<f32> {
        let mut filters = vec![0.0; 80 * 201];
        for mel in 0..80 {
            filters[mel * 201 + bin] = 1.0;
        }
        filters
    }
    fn hann(index: usize) -> f64 {
        0.5 * (1.0 - (2.0 * std::f64::consts::PI * index as f64 / 400.0).cos())
    }
    fn check_close(actual: f32, expected: f64) {
        assert!(
            (f64::from(actual) - expected).abs() < 0.000001,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn silence_has_fixed_shape_and_exact_log_power_floor() {
        let actual =
            pcm_to_mel(&[0.0; 16000], &one_bin_filters(25), &AtomicBool::new(false)).unwrap();
        assert_eq!(actual.len(), 80 * FRAMES);
        assert!(actual.iter().all(|v| *v == -1.5));
    }

    #[test]
    fn leading_impulse_detects_centering_and_rejects_mirrored_power_doubling() {
        let mut signal = vec![0.0; 400];
        signal[0] = 1.0;
        let actual = pcm_to_mel(&signal, &one_bin_filters(25), &AtomicBool::new(false)).unwrap();
        // At frame0 the impulse is under the Hann peak, so one-sided power is1.
        // Forward framing would see zero; mirror doubling would produce1.0752575.
        check_close(actual[0], 1.0);
        check_close(actual[1], (hann(40).powi(2).log10() + 4.0) / 4.0);
        check_close(actual[2], -1.0);
    }

    #[test]
    fn left_edge_reflection_uses_the_zero_padded_logical_input() {
        let mut signal = vec![0.0; 2];
        signal[1] = 1.0;
        let actual = pcm_to_mel(&signal, &one_bin_filters(0), &AtomicBool::new(false)).unwrap();
        let expected_power = (hann(199) + hann(201)).powi(2);
        check_close(actual[0], (expected_power.log10() + 4.0) / 4.0);
    }

    #[test]
    fn full_thirty_second_right_reflection_and_excluded_final_frame() {
        let mut signal = vec![0.0; SAMPLES];
        signal[SAMPLES - 2] = 1.0;
        let actual = pcm_to_mel(&signal, &one_bin_filters(0), &AtomicBool::new(false)).unwrap();
        assert_eq!(actual.len(), 80 * 3000);
        // Last kept window spans original indices479640..480039. The impulse
        // at479998 appears at positions358 and360 after reflection across479999.
        let expected_power = (hann(358) + hann(360)).powi(2);
        let maximum_log_power = expected_power.log10();
        check_close(actual[2999], (maximum_log_power + 4.0) / 4.0);
        // Computing discarded frame3000 would create a larger peak and change
        // this global floor even if that frame were cropped from the final tensor.
        check_close(
            actual[0],
            ((maximum_log_power - 8.0).max(-10.0) + 4.0) / 4.0,
        );
    }

    #[test]
    fn invalid_values_lengths_filters_and_pre_cancel_are_rejected() {
        let filters = one_bin_filters(25);
        let cancel = AtomicBool::new(false);
        assert!(pcm_to_mel(&[], &filters, &cancel).is_err());
        assert!(pcm_to_mel(&vec![0.0; SAMPLES + 1], &filters, &cancel).is_err());
        assert!(pcm_to_mel(&[f32::NAN], &filters, &cancel).is_err());
        assert!(pcm_to_mel(&[0.0], &[], &cancel).is_err());
        let mut invalid = filters.clone();
        invalid[0] = -1.0;
        assert!(pcm_to_mel(&[0.0], &invalid, &cancel).is_err());
        invalid[0] = f32::INFINITY;
        assert!(pcm_to_mel(&[0.0], &invalid, &cancel).is_err());
        assert_eq!(
            pcm_to_mel(&[0.0], &filters, &AtomicBool::new(true))
                .unwrap_err()
                .to_string(),
            "transcription cancelled"
        );
    }
}
