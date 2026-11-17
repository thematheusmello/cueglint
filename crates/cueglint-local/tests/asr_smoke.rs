use cueglint_local::{asr::Transcriber, audio::load_wav, models};
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
#[ignore = "requires the verified Whisper profile in CUEGLINT_TEST_MODELS_DIR; never downloads models"]
fn original_clean_english_fixture_measures_actual_warm_asr() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("parity/fixtures/clean-english.json"),
    )?)?;
    let audio_path = root.join(fixture["audio"].as_str().unwrap());
    let bytes = std::fs::read(&audio_path)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        fixture["sha256"].as_str().unwrap()
    );
    let samples = load_wav(&audio_path)?;
    let duration_seconds = samples.len() as f64 / 16_000.0;
    let directory =
        PathBuf::from(std::env::var_os("CUEGLINT_TEST_MODELS_DIR").ok_or_else(|| {
            anyhow::anyhow!("set CUEGLINT_TEST_MODELS_DIR to the installed model root")
        })?);
    let installed = models::verify(&directory, models::TRANSCRIPTION_PROFILE)?;
    let mut transcriber = Transcriber::load(&installed.directory)?;
    let cancel = AtomicBool::new(false);
    let cold = Instant::now();
    let cold_text = transcriber.transcribe(&samples, &cancel)?;
    let cold_ms = cold.elapsed().as_secs_f64() * 1_000.0;
    let warm = Instant::now();
    let text = transcriber.transcribe(&samples, &cancel)?;
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
            "duration_seconds": duration_seconds, "cold_text": cold_text, "text": text, "word_errors": errors,
            "reference_words": reference.len(), "word_error_rate": wer, "cold_decode_ms": cold_ms,
            "warm_decode_ms": warm_ms, "warm_real_time_factor": rtf, "backend": "candle-cpu-0.11.1",
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
