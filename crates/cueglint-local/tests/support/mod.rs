//! Bounded immutable evidence inputs, shared by feature-only and ASR checks.
use anyhow::{Result, ensure};
use cueglint_local::audio::load_wav_bytes;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

// Thirty seconds of mono PCM16 is <=960,000 bytes; allow bounded container metadata.
const MAX_WAV_BYTES: usize = 2 * 1024 * 1024;

pub struct WavInput {
    pub samples: Vec<f32>,
    pub wav_sha256: String,
    pub decoded_f32_le_sha256: String,
}

pub fn frozen_wav(path: &Path) -> Result<WavInput> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_WAV_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_WAV_BYTES, "evidence WAV exceeds 2 MiB");
    let wav_sha256 = format!("{:x}", Sha256::digest(&bytes));
    let samples = load_wav_bytes(&bytes)?;
    let mut decoded_hash = Sha256::new();
    for sample in &samples {
        decoded_hash.update(sample.to_le_bytes());
    }
    Ok(WavInput {
        samples,
        wav_sha256,
        decoded_f32_le_sha256: format!("{:x}", decoded_hash.finalize()),
    })
}
