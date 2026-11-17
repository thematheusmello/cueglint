//! Explicit feature-only development check; never loads or downloads models.
mod support;

use anyhow::{Context, Result, ensure};
use cueglint_local::asr_frontend;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const MAX_ABSOLUTE_ERROR: f64 = 0.0005;
const MAX_RMSE: f64 = 0.00001;
const ORACLE_BYTES: usize = 80 * asr_frontend::FRAMES * 4;

fn required_path(variable: &str) -> Result<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .with_context(|| format!("set {variable} to an existing external input file"))
}

fn read_oracle(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(ORACLE_BYTES + 1);
    File::open(path)?
        .take((ORACLE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() == ORACLE_BYTES,
        "oracle must contain exactly 80x3000 little-endian float32 values"
    );
    Ok(bytes)
}

fn write_new_output(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(path)
        .context("feature output must be a new file; existing evidence is never replaced")?;
    output.write_all(bytes)?;
    output.sync_all()?;
    Ok(())
}

#[test]
fn explicit_feature_output_preserves_existing_evidence() -> Result<()> {
    let directory =
        std::env::temp_dir().join(format!("cueglint-frontend-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory)?;
    let path = directory.join("features.f32");
    let result = (|| -> Result<()> {
        write_new_output(&path, b"original")?;
        ensure!(write_new_output(&path, b"replacement").is_err());
        ensure!(std::fs::read(&path)? == b"original");
        Ok(())
    })();
    std::fs::remove_dir_all(directory)?;
    result
}

#[test]
#[ignore = "feature-only development check; requires explicit external WAV/oracle/hash paths, never models"]
fn supplied_standard_whisper_oracle_matches_frozen_numerical_tolerances() -> Result<()> {
    let wav = required_path("CUEGLINT_TEST_FRONTEND_WAV")?;
    let oracle_path = required_path("CUEGLINT_TEST_FRONTEND_ORACLE")?;
    let expected_hash = std::env::var("CUEGLINT_TEST_FRONTEND_ORACLE_SHA256").context(
        "set CUEGLINT_TEST_FRONTEND_ORACLE_SHA256 to the independently recorded oracle hash",
    )?;
    ensure!(
        expected_hash.len() == 64 && expected_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "oracle hash must be 64 hexadecimal characters"
    );
    let oracle_bytes = read_oracle(&oracle_path)?;
    let oracle_hash = format!("{:x}", Sha256::digest(&oracle_bytes));
    ensure!(
        oracle_hash.eq_ignore_ascii_case(&expected_hash),
        "independent oracle hash mismatch"
    );
    let oracle: Vec<f32> = oracle_bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    ensure!(oracle.iter().all(|v| v.is_finite()), "nonfinite oracle");
    let input = support::frozen_wav(&wav)?;
    let filters: Vec<f32> = include_bytes!("../assets/melfilters.bytes")
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let actual = asr_frontend::pcm_to_mel(&input.samples, &filters, &AtomicBool::new(false))?;
    ensure!(actual.len() == oracle.len());
    let mut maximum = 0.0f64;
    let mut squared_sum = 0.0f64;
    let mut actual_bytes = Vec::with_capacity(ORACLE_BYTES);
    for (actual, reference) in actual.iter().zip(&oracle) {
        let error = f64::from(*actual) - f64::from(*reference);
        maximum = maximum.max(error.abs());
        squared_sum += error * error;
        actual_bytes.extend_from_slice(&actual.to_le_bytes());
    }
    let rmse = (squared_sum / actual.len() as f64).sqrt();
    let output = std::env::var_os("CUEGLINT_TEST_FRONTEND_OUTPUT");
    if let Some(path) = &output {
        // Preserve explicitly requested output even when the numerical check fails.
        write_new_output(Path::new(path), &actual_bytes)?;
    }
    let passed = maximum <= MAX_ABSOLUTE_ERROR && rmse <= MAX_RMSE;
    println!(
        "CUEGLINT_FRONTEND_EVIDENCE {}",
        serde_json::to_string(&serde_json::json!({
            "status": if passed { "passed" } else { "failed" },
            "scope": "feature-only numerical agreement; no ASR accuracy or speed claim",
            "backend": "rust-cpu-feature-only", "model_loads": 0, "model_inference_calls": 0,
            "wav_sha256": input.wav_sha256,
            "decoded_input_f32_le_sha256": input.decoded_f32_le_sha256,
            "input_sample_count": input.samples.len(), "input_binding": "bounded frozen WAV bytes decoded once",
            "oracle_sha256": oracle_hash, "actual_sha256": format!("{:x}", Sha256::digest(&actual_bytes)),
            "output_written": output.is_some(), "shape": [80, asr_frontend::FRAMES],
            "max_absolute_error": maximum, "root_mean_square_error": rmse,
            "acceptance_declared_before_run": {"max_absolute_error": MAX_ABSOLUTE_ERROR, "root_mean_square_error": MAX_RMSE},
            "frontend": {"revision": asr_frontend::REVISION,
                "configuration": serde_json::from_str::<serde_json::Value>(asr_frontend::CONFIGURATION)?,
                "configuration_sha256": asr_frontend::configuration_sha256(),
                "source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../src/asr_frontend.rs"))),
                "fft_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../src/asr_frontend/fft.rs"))),
                "filter_sha256": format!("{:x}", Sha256::digest(include_bytes!("../assets/melfilters.bytes")))},
        }))?
    );
    ensure!(passed, "frontend exceeds the frozen numerical tolerances");
    Ok(())
}
