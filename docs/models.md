# Models and local inference

No model has completed release acceptance. Two real CPU development configurations are
pinned in `models/local-defaults.json`: Qwen2.5-0.5B-Instruct Q4_K_M (491,400,032-byte GGUF,
plus tokenizer) and Whisper tiny.en (151,060,136-byte weights, plus config/tokenizer).
Both install and verify through the working manager. Missing models fail visibly; no
automatic cloud fallback exists. Arbitrary model imports/removal remain incomplete.

The implemented backend is Candle 0.11.1 on CPU for generation and ASR. Optional Metal ASR
compilation is not benchmarked or enabled by default. Any optional C/C++ inference backend
must be identified as an FFI runtime rather than pure Rust. No Python service is a runtime
dependency. Optional external providers require intentional selection and visible context.

Before advertising a model, record its exact repository revision, file SHA-256, tokenizer,
quantization, prompt/configuration hashes, weight license, download bytes, storage and memory
guidance, actual hardware, cold/warm samples, median and tail latency, ASR error metrics and
answer-quality rubric results. Downloads must be resumable and verified; local import,
selection, removal and failure recovery are required.

The [actual development report](../parity/reports/local-inference-smoke.json) preserves
failures. Tiny.en decoded the original 3.61-second clean English fixture in 1.27 seconds warm
(RTF 0.353), but word error rate 0.20 failed the preset 0.10 threshold. Qwen streamed and
cancelled, and answered two fact questions, but invented an unsupported owner/deadline,
omitted the count in the original reference-file question and refused a benign followup.
These are measured development limits, not acceptable release quality. Decoding-protocol/prompt corrections and larger models are the next bounded evaluation; thresholds and failed runs stay intact.

After model installation, the core session and screen-context workflow must work with
outbound networking blocked. Deterministic fixtures may stabilize UI captures but do not
count as inference or model-quality evidence. See [verification](verification.md).
