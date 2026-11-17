mod support;

use cueglint_local::{asr::Transcriber, asr_frontend, models};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn word_errors(reference: &[String], actual: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=actual.len()).collect();
    for (index, expected) in reference.iter().enumerate() {
        let mut current = vec![index + 1];
        for (position, observed) in actual.iter().enumerate() {
            current.push(
                (current[position] + 1)
                    .min(previous[position + 1] + 1)
                    .min(previous[position] + usize::from(expected != observed)),
            );
        }
        previous = current;
    }
    previous[actual.len()]
}

#[test]
fn word_error_metric_counts_substitutions_insertions_and_deletions() {
    assert_eq!(
        word_errors(&words("one two three"), &words("one two three.")),
        0
    );
    assert_eq!(
        word_errors(&words("one two three"), &words("one too three")),
        1
    );
    assert_eq!(word_errors(&words("one two three"), &words("one three")), 1);
    assert_eq!(
        word_errors(&words("one two three"), &words("one two and three")),
        1
    );
}

#[test]
fn replacement_fixture_is_pinned_with_unchanged_text_thresholds_and_separate_license()
-> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("parity/fixtures/clean-english-v2.json"),
    )?)?;
    let retired: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("parity/fixtures/clean-english-v1.retired.json"),
    )?)?;
    assert_eq!(fixture["fixture_version"], 2);
    assert_eq!(fixture["license"], "GPL-3.0-or-later");
    assert_eq!(fixture["ground_truth"], retired["ground_truth"]);
    assert_eq!(
        fixture["acceptance_declared_before_run"],
        retired["acceptance_declared_before_run"]
    );
    assert!(
        !root.join(retired["audio"].as_str().unwrap()).exists(),
        "retired System Voice waveform must not return to the current tree"
    );
    let audio = root.join(fixture["audio"].as_str().unwrap());
    let input = support::frozen_wav(&audio)?;
    assert_eq!(input.wav_sha256, fixture["sha256"].as_str().unwrap());
    assert_eq!(
        input.samples.len(),
        (fixture["duration_seconds"].as_f64().unwrap() * 16_000.0).round() as usize
    );
    let recipe = root.join("parity/fixtures/generate-clean-english-v2.py");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(recipe)?)),
        fixture["generation"]["recipe_sha256"].as_str().unwrap()
    );
    assert!(
        root.join(fixture["license_notice"].as_str().unwrap())
            .is_file()
    );
    assert!(
        root.join(fixture["license_text"].as_str().unwrap())
            .is_file()
    );
    Ok(())
}

#[test]
#[ignore = "requires the verified Whisper profile in CUEGLINT_TEST_MODELS_DIR; never downloads models"]
fn original_clean_english_fixture_measures_actual_warm_asr() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("parity/fixtures/clean-english-v2.json"),
    )?)?;
    let audio_path = root.join(fixture["audio"].as_str().unwrap());
    let input = support::frozen_wav(&audio_path)?;
    assert_eq!(input.wav_sha256, fixture["sha256"].as_str().unwrap());
    let samples = &input.samples;
    let duration_seconds = samples.len() as f64 / 16_000.0;
    let directory =
        PathBuf::from(std::env::var_os("CUEGLINT_TEST_MODELS_DIR").ok_or_else(|| {
            anyhow::anyhow!("set CUEGLINT_TEST_MODELS_DIR to the installed model root")
        })?);
    let profile = std::env::var("CUEGLINT_TEST_ASR_PROFILE")
        .unwrap_or_else(|_| models::TRANSCRIPTION_PROFILE.to_owned());
    let installed = models::verify(&directory, &profile)?;
    let mut transcriber = Transcriber::load(&installed.directory)?;
    let backend = transcriber.backend();
    let frontend_configuration: serde_json::Value =
        serde_json::from_str(asr_frontend::CONFIGURATION)?;
    let cancel = AtomicBool::new(false);
    let cold = Instant::now();
    let cold_text = transcriber.transcribe(samples, &cancel)?;
    let cold_ms = cold.elapsed().as_secs_f64() * 1_000.0;
    let warm = Instant::now();
    let text = transcriber.transcribe(samples, &cancel)?;
    let warm_ms = warm.elapsed().as_secs_f64() * 1_000.0;
    let reference = words(fixture["ground_truth"].as_str().unwrap());
    let errors = word_errors(&reference, &words(&text));
    let wer = errors as f64 / reference.len() as f64;
    let rtf = warm_ms / 1_000.0 / duration_seconds;
    let acceptance = &fixture["acceptance_declared_before_run"];
    println!(
        "CUEGLINT_ASR_EVIDENCE {}",
        serde_json::to_string(&serde_json::json!({
            "fixture_id": fixture["id"], "fixture_sha256": fixture["sha256"], "ground_truth": fixture["ground_truth"],
            "wav_sha256": input.wav_sha256, "decoded_input_f32_le_sha256": input.decoded_f32_le_sha256,
            "input_sample_count": samples.len(), "input_binding": "bounded frozen WAV bytes decoded once",
            "duration_seconds": duration_seconds, "cold_text": cold_text, "text": text, "word_errors": errors,
            "reference_words": reference.len(), "word_error_rate": wer, "cold_decode_ms": cold_ms,
            "warm_decode_ms": warm_ms, "warm_real_time_factor": rtf, "backend": backend,
            "frontend": {"revision": asr_frontend::REVISION, "configuration": frontend_configuration,
                "configuration_sha256": asr_frontend::configuration_sha256(),
                "source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../src/asr_frontend.rs"))),
                "fft_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../src/asr_frontend/fft.rs"))),
                "filter_sha256": format!("{:x}", Sha256::digest(include_bytes!("../assets/melfilters.bytes")))},
            "model": installed.profile, "architecture": std::env::consts::ARCH, "os": std::env::consts::OS,
            "acceptance": acceptance, "status": if wer <= acceptance["normalized_word_error_rate_max"].as_f64().unwrap() && rtf <= acceptance["real_time_factor_warm_max"].as_f64().unwrap() { "passed" } else { "failed" },
        }))?
    );
    assert!(
        wer <= acceptance["normalized_word_error_rate_max"]
            .as_f64()
            .unwrap(),
        "single-fixture WER {wer} exceeds the predeclared threshold"
    );
    assert!(
        rtf <= acceptance["real_time_factor_warm_max"].as_f64().unwrap(),
        "warm real-time factor {rtf} exceeds the predeclared threshold"
    );
    Ok(())
}
