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

pub struct Transcriber {
    model: Whisper,
    tokenizer: Tokenizer,
    filters: Vec<f32>,
    device: Device,
}

impl Transcriber {
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

    /// Supported profiles are English-only. Device failures never fall back to CPU or a service.
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
        ensure!(!cancel.load(Ordering::Relaxed), "model loading cancelled");
        let device = select_device(backend)?;
        let profile = crate::models::profile(profile_id)?;
        ensure!(
            profile.purpose == "transcription",
            "selected profile is not a transcription model"
        );
        let mut inputs = crate::models::open_profile_with_cancel(root, &profile, cancel)?;
        let transcriber = Self::load_from(device, |name| inputs.read(name, cancel))?;
        ensure!(!cancel.load(Ordering::Relaxed), "model loading cancelled");
        Ok((transcriber, inputs.installed.profile))
    }

    fn load_from(device: Device, mut read: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<Self> {
        let config: Config = serde_json::from_slice(&read("config.json")?)?;
        ensure!(
            config.num_mel_bins == 80,
            "this ASR profile requires 80 mel bins"
        );
        let tokenizer = Tokenizer::from_bytes(read("tokenizer.json")?)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
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
        let start = token(whisper::SOT_TOKEN)?;
        let eot = token(whisper::EOT_TOKEN)?;
        let no_timestamps = token(whisper::NO_TIMESTAMPS_TOKEN)?;
        // English-only profiles use SOT + NO_TIMESTAMPS; language/task tokens are multilingual.
        let mut tokens = vec![start, no_timestamps];
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
            error.downcast_ref::<std::io::Error>().is_none(),
            "config validation must precede missing tokenizer/weight files"
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
