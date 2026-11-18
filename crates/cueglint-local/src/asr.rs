//! Local Candle Whisper inference. No network is opened by model loading or decoding.
use anyhow::{Context, Result, ensure};
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, Config, model::Whisper};
use cueglint_core::TranscriptionBackend;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tokenizers::Tokenizer;

/// Build support only; this does not initialize or check an actual Metal device.
pub const fn backend_compiled(backend: TranscriptionBackend) -> bool {
    match backend {
        TranscriptionBackend::Cpu => true,
        TranscriptionBackend::Metal => cfg!(feature = "metal"),
    }
}

fn select_device(backend: TranscriptionBackend) -> Result<Device> {
    match backend {
        TranscriptionBackend::Cpu => Ok(Device::Cpu),
        TranscriptionBackend::Metal => {
            #[cfg(feature = "metal")]
            {
                Device::new_metal(0)
                    .context("Metal ASR initialization failed; select CPU explicitly")
            }
            #[cfg(not(feature = "metal"))]
            {
                anyhow::bail!(
                    "Metal ASR is unavailable in this build; select CPU or build with cueglint-local/metal"
                )
            }
        }
    }
}

/// Validate the explicit selection without reading model files or initializing a device.
pub fn validate_language(profile_id: &str, language: &str) -> Result<()> {
    cueglint_core::validate_transcription_language(language)?;
    match profile_id {
        crate::models::TRANSCRIPTION_PROFILE | crate::models::SMALL_TRANSCRIPTION_PROFILE => {
            ensure!(
                language == "en",
                "{profile_id} is English-only; explicitly select whisper-base for {language} transcription"
            );
        }
        crate::models::MULTILINGUAL_TRANSCRIPTION_PROFILE => {}
        _ => anyhow::bail!("selected profile is not a supported transcription model"),
    }
    Ok(())
}

fn decoder_prefix(
    language: &str,
    multilingual: bool,
    mut token: impl FnMut(&str) -> Result<u32>,
) -> Result<Vec<u32>> {
    cueglint_core::validate_transcription_language(language)?;
    ensure!(multilingual || language == "en", "English-only ASR profile");
    let mut prefix = vec![token(whisper::SOT_TOKEN)?];
    if multilingual {
        prefix.push(token(&format!("<|{language}|>"))?);
        prefix.push(token(whisper::TRANSCRIBE_TOKEN)?);
    }
    prefix.push(token(whisper::NO_TIMESTAMPS_TOKEN)?);
    Ok(prefix)
}

pub struct Transcriber {
    model: Whisper,
    tokenizer: Tokenizer,
    filters: Vec<f32>,
    device: Device,
    language: String,
    decoder_prefix: Vec<u32>,
}

impl Transcriber {
    /// Selected input language frozen at model load; this is not detected-language metadata.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Actual selected inference backend; feature extraction remains CPU Rust.
    pub fn backend(&self) -> &'static str {
        match self.device {
            Device::Cpu => "candle-cpu-0.11.1",
            Device::Metal(_) => "candle-metal-0.11.1",
            Device::Cuda(_) => "candle-cuda-0.11.1",
        }
    }

    /// Legacy build-selected backend: Metal with the `metal` feature, otherwise CPU.
    /// Managed app callers should use `load_profile_with_backend` with their saved selection.
    pub fn load(directory: &Path) -> Result<Self> {
        Self::load_with_backend(
            directory,
            if cfg!(feature = "metal") {
                TranscriptionBackend::Metal
            } else {
                TranscriptionBackend::Cpu
            },
        )
    }

    /// Legacy English decoding. Device failures never fall back to CPU or a service.
    pub fn load_with_backend(directory: &Path, backend: TranscriptionBackend) -> Result<Self> {
        let device = select_device(backend)?;
        Self::load_from(device, |name| Ok(std::fs::read(directory.join(name))?))
    }

    /// Load catalog-pinned artifacts through their verified handles, returning their bound profile.
    pub fn load_profile_with_backend(
        root: &Path,
        profile_id: &str,
        backend: TranscriptionBackend,
        cancel: &AtomicBool,
    ) -> Result<(Self, crate::models::ModelProfile)> {
        Self::load_profile_with_language(root, profile_id, backend, "en", cancel)
    }

    /// Load one explicitly selected input language; no language detection or translation.
    pub fn load_profile_with_language(
        root: &Path,
        profile_id: &str,
        backend: TranscriptionBackend,
        language: &str,
        cancel: &AtomicBool,
    ) -> Result<(Self, crate::models::ModelProfile)> {
        ensure!(!cancel.load(Ordering::Relaxed), "model loading cancelled");
        let device = select_device(backend)?;
        let profile = crate::models::profile(profile_id)?;
        ensure!(
            profile.purpose == "transcription",
            "selected profile is not a transcription model"
        );
        validate_language(profile_id, language)?;
        let mut inputs = crate::models::open_profile_with_cancel(root, &profile, cancel)?;
        let transcriber = Self::load_from_with_language(
            device,
            language,
            profile_id == crate::models::MULTILINGUAL_TRANSCRIPTION_PROFILE,
            |name| inputs.read(name, cancel),
        )?;
        ensure!(!cancel.load(Ordering::Relaxed), "model loading cancelled");
        Ok((transcriber, inputs.installed.profile))
    }

    fn load_from(device: Device, read: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<Self> {
        Self::load_from_with_language(device, "en", false, read)
    }

    fn load_from_with_language(
        device: Device,
        language: &str,
        multilingual: bool,
        mut read: impl FnMut(&str) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let config: Config = serde_json::from_slice(&read("config.json")?)?;
        ensure!(
            config.num_mel_bins == 80,
            "this ASR profile requires 80 mel bins"
        );
        let tokenizer = Tokenizer::from_bytes(read("tokenizer.json")?)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let decoder_prefix = decoder_prefix(language, multilingual, |s| {
            tokenizer
                .token_to_id(s)
                .filter(|id| (*id as usize) < config.vocab_size)
                .with_context(|| format!("ASR tokenizer missing or invalid {s}"))
        })?;
        // Owned bytes avoid memory-mapped files being changed while the model is in use.
        let vb = VarBuilder::from_buffered_safetensors(
            read("model.safetensors")?,
            whisper::DTYPE,
            &device,
        )?;
        let model = Whisper::load(&vb, config)?;
        let filters = include_bytes!("../assets/melfilters.bytes")
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Ok(Self {
            model,
            tokenizer,
            filters,
            device,
            language: language.into(),
            decoder_prefix,
        })
    }

    pub fn transcribe(&mut self, samples: &[f32], cancelled: &AtomicBool) -> Result<String> {
        let mel = crate::asr_frontend::pcm_to_mel(samples, &self.filters, cancelled)?;
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("transcription cancelled");
        }
        let mel = Tensor::from_vec(
            mel,
            (
                1,
                self.model.config.num_mel_bins,
                crate::asr_frontend::FRAMES,
            ),
            &self.device,
        )?;
        let features = self.model.encoder.forward(&mel, true)?;
        let token = |s: &str| {
            self.tokenizer
                .token_to_id(s)
                .with_context(|| format!("ASR tokenizer missing {s}"))
        };
        let eot = token(whisper::EOT_TOKEN)?;
        let no_timestamps = token(whisper::NO_TIMESTAMPS_TOKEN)?;
        let mut tokens = self.decoder_prefix.clone();
        let prefix = tokens.len();
        let suppress: Vec<f32> = (0..self.model.config.vocab_size as u32)
            .map(|t| {
                if self.model.config.suppress_tokens.contains(&t) || t >= no_timestamps {
                    f32::NEG_INFINITY
                } else {
                    0.0
                }
            })
            .collect();
        let suppress = Tensor::new(suppress, &self.device)?;
        for index in 0..224 {
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("transcription cancelled");
            }
            let input = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let hidden = self.model.decoder.forward(&input, &features, index == 0)?;
            let (_, len, _) = hidden.dims3()?;
            let logits = self
                .model
                .decoder
                .final_linear(&hidden.i((..1, len - 1..))?)?
                .i((0, 0))?
                .broadcast_add(&suppress)?;
            let logits = logits.to_vec1::<f32>()?;
            let next = logits
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .context("empty ASR logits")?
                .0 as u32;
            if next == eot {
                break;
            }
            tokens.push(next);
            if tokens.len() >= self.model.config.max_target_positions {
                break;
            }
        }
        self.tokenizer
            .decode(&tokens[prefix..], true)
            .map(|s| s.trim().to_owned())
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_explicit_multilingual_prefixes_keep_transcription_separate() -> Result<()> {
        let token = |s: &str| match s {
            whisper::SOT_TOKEN => Ok(50258),
            "<|en|>" => Ok(50259),
            "<|pt|>" => Ok(50267),
            whisper::TRANSCRIBE_TOKEN => Ok(50359),
            whisper::NO_TIMESTAMPS_TOKEN => Ok(50363),
            _ => anyhow::bail!("unexpected token: {s}"),
        };
        let mut legacy_tokens = Vec::new();
        let legacy = decoder_prefix("en", false, |s| {
            legacy_tokens.push(s.to_owned());
            match s {
                whisper::SOT_TOKEN => Ok(50257),
                whisper::NO_TIMESTAMPS_TOKEN => Ok(50362),
                _ => anyhow::bail!("unexpected legacy token: {s}"),
            }
        })?;
        assert_eq!(legacy, [50257, 50362]);
        assert_eq!(
            legacy_tokens,
            [whisper::SOT_TOKEN, whisper::NO_TIMESTAMPS_TOKEN]
        );
        assert_eq!(
            decoder_prefix("en", true, token)?,
            [50258, 50259, 50359, 50363]
        );
        assert_eq!(
            decoder_prefix("pt", true, token)?,
            [50258, 50267, 50359, 50363]
        );
        assert!(decoder_prefix("pt", false, token).is_err());
        assert!(decoder_prefix("auto", true, token).is_err());
        assert!(decoder_prefix("fr", true, token).is_err());
        Ok(())
    }

    #[test]
    fn language_profile_validation_never_substitutes_or_reads_models() -> Result<()> {
        for (language, _) in cueglint_core::TRANSCRIPTION_LANGUAGES {
            validate_language(crate::models::MULTILINGUAL_TRANSCRIPTION_PROFILE, language)?;
        }
        for profile in [
            crate::models::TRANSCRIPTION_PROFILE,
            crate::models::SMALL_TRANSCRIPTION_PROFILE,
        ] {
            validate_language(profile, "en")?;
            assert!(validate_language(profile, "pt").is_err());
        }
        assert!(validate_language(crate::models::GENERATION_PROFILE, "en").is_err());
        let missing =
            std::env::temp_dir().join(format!("cueglint-asr-language-{}", uuid::Uuid::new_v4()));
        let error = Transcriber::load_profile_with_language(
            &missing,
            crate::models::TRANSCRIPTION_PROFILE,
            TranscriptionBackend::Cpu,
            "pt",
            &AtomicBool::new(false),
        )
        .err()
        .context("incompatible language must fail before model I/O")?;
        assert!(error.to_string().contains("English-only"));
        assert!(!missing.exists());
        let cancelled = Transcriber::load_profile_with_language(
            &missing,
            "unknown",
            TranscriptionBackend::Metal,
            "auto",
            &AtomicBool::new(true),
        )
        .err()
        .context("cancellation must precede device and language checks")?;
        assert!(cancelled.to_string().contains("cancelled"));
        Ok(())
    }

    #[test]
    fn shared_constructor_stops_at_invalid_config_or_tokenizer_before_weights() -> Result<()> {
        for bins in [81, 80] {
            let config = serde_json::to_vec(&serde_json::json!({
                "num_mel_bins": bins, "max_source_positions": 1, "d_model": 1,
                "encoder_attention_heads": 1, "encoder_layers": 1, "vocab_size": 1,
                "max_target_positions": 1, "decoder_attention_heads": 1, "decoder_layers": 1,
            }))?;
            let mut reads = Vec::new();
            let error = Transcriber::load_from(Device::Cpu, |name| {
                reads.push(name.to_owned());
                match name {
                    "config.json" => Ok(config.clone()),
                    "tokenizer.json" => Ok(b"Original invalid tokenizer test bytes".to_vec()),
                    _ => panic!("invalid earlier inputs must prevent a weight read"),
                }
            })
            .err()
            .context("invalid authored inputs must fail")?;
            if bins == 81 {
                assert!(error.to_string().contains("80 mel bins"));
                assert_eq!(reads, ["config.json"]);
            } else {
                assert_eq!(reads, ["config.json", "tokenizer.json"]);
            }
        }
        Ok(())
    }

    #[test]
    fn managed_cancellation_and_purpose_errors_create_no_model_root() -> Result<()> {
        let missing =
            std::env::temp_dir().join(format!("cueglint-managed-asr-{}", uuid::Uuid::new_v4()));
        let cancelled = Transcriber::load_profile_with_backend(
            &missing,
            "not-a-profile",
            TranscriptionBackend::Metal,
            &AtomicBool::new(true),
        )
        .err()
        .context("cancelled load must fail before device initialization")?;
        assert!(cancelled.to_string().contains("cancelled"));
        let purpose = Transcriber::load_profile_with_backend(
            &missing,
            crate::models::GENERATION_PROFILE,
            TranscriptionBackend::Cpu,
            &AtomicBool::new(false),
        )
        .err()
        .context("generation profile must not load as ASR")?;
        assert!(purpose.to_string().contains("not a transcription model"));
        assert!(!missing.exists());
        Ok(())
    }

    #[cfg(not(feature = "metal"))]
    #[test]
    fn legacy_directory_load_needs_no_catalog_or_coordinator() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("cueglint-legacy-asr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        std::fs::write(
            directory.join("config.json"),
            b"Original invalid config test bytes",
        )?;
        let error = Transcriber::load(&directory)
            .err()
            .context("invalid config must fail")?;
        assert!(
            error.downcast_ref::<serde_json::Error>().is_some(),
            "invalid config must reach JSON validation before missing tokenizer/weight files"
        );
        assert_eq!(std::fs::read_dir(&directory)?.count(), 1);
        assert!(!directory.join(".install.lock").exists());
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn explicit_cpu_selection_is_independent_of_metal_build_support() {
        assert!(matches!(
            select_device(TranscriptionBackend::Cpu).unwrap(),
            Device::Cpu
        ));
        assert!(backend_compiled(TranscriptionBackend::Cpu));
        assert_eq!(
            backend_compiled(TranscriptionBackend::Metal),
            cfg!(feature = "metal")
        );
    }

    #[cfg(not(feature = "metal"))]
    #[test]
    fn unavailable_metal_is_rejected_before_model_file_io() {
        let missing = std::env::temp_dir().join(format!("cueglint-asr-{}", uuid::Uuid::new_v4()));
        let error = match Transcriber::load_with_backend(&missing, TranscriptionBackend::Metal) {
            Ok(_) => panic!("uncompiled Metal must not load a transcriber"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("Metal ASR is unavailable in this build")
        );
        let managed = Transcriber::load_profile_with_backend(
            &missing,
            "not-a-profile",
            TranscriptionBackend::Metal,
            &AtomicBool::new(false),
        )
        .err()
        .expect("uncompiled Metal must fail before catalog or input opening");
        assert!(
            managed
                .to_string()
                .contains("Metal ASR is unavailable in this build")
        );
        assert!(!missing.exists());
    }
}
