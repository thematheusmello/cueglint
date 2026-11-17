//! Local Candle Whisper inference. No network is opened by model loading or decoding.
use anyhow::{Context, Result, ensure};
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, Config, model::Whisper};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tokenizers::Tokenizer;

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

    /// Supported profiles are explicitly English-only; other language claims are unverified.
    pub fn load(directory: &Path) -> Result<Self> {
        let config: Config =
            serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)?;
        ensure!(
            config.num_mel_bins == 80,
            "this ASR profile requires 80 mel bins"
        );
        let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let device = Device::Cpu;
        #[cfg(feature = "metal")]
        let device =
            Device::new_metal(0).context("Metal initialization failed; select CPU explicitly")?;
        // Owned bytes avoid memory-mapped files being changed while the model is in use.
        let vb = VarBuilder::from_buffered_safetensors(
            std::fs::read(directory.join("model.safetensors"))?,
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
