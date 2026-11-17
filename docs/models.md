# Models and local inference

No model has completed release acceptance. The working CPU path uses Candle 0.11.1 for
generation and English ASR. Model revisions, download sizes, SHA-256 values, tokenizer and
license links are pinned in [the manifest](../models/local-defaults.json). Missing models
fail visibly; loading or inference never starts a download or a cloud fallback.

| Profile | Role | Model weights | Development result |
|---|---|---:|---|
| Qwen2.5 1.5B Instruct Q4_K_M | Default generation | 1,117,320,736 bytes | Original narrow grounding, completeness and followup cases passed |
| Whisper base.en | Default transcription | 290,401,888 bytes | Original clean English fixture WER 0.10; warm RTF 0.560 |
| Qwen2.5 0.5B Instruct Q4_K_M | Selectable smaller candidate | 491,400,032 bytes | Grounding/completeness/followup failures retained |
| Whisper tiny.en | Selectable smaller candidate | 151,060,136 bytes | WER 0.20 failed the same 0.10 threshold |

The two defaults need about 1.42 GB of download storage including metadata/tokenizers.
Working memory, long-meeting growth and power use remain unmeasured; download size is not
a RAM requirement. Measurements below used an Apple M3 Max with 48 GB RAM, macOS 27.2
arm64. Other hardware has no performance guarantee. English-only ASR does not establish
multilingual transcription, translation or interface localization.

Install explicitly through General settings or the CLI:

```sh
cargo run --release -p cueglint-cli -- models install qwen2.5-1.5b-instruct
cargo run --release -p cueglint-cli -- models install whisper-base-en
cargo run --release -p cueglint-cli -- model-status
```

Downloads resume from partial files, verify pinned sizes/hashes, and serialize changes per
profile. Set the model directory in Settings, with `--models-dir` on the CLI, or with
`CUEGLINT_MODEL_DIR`. Selection precedence is an explicit CLI `--profile`, then the saved
profile preference, then the measured default. Older explicit preferences are preserved.
Arbitrary local model import and removal remain incomplete.

The [v3 run](../parity/reports/local-inference-candidates-v3.json) and
[independent review](../parity/reports/independent-v3-review.json) preserve exact source,
binary, fixture and configuration identities. Three basic grounding cases passed, including
refusal to invent an owner/deadline; complete-answer upper bounds were 6.60–7.55 seconds
including model verification/loading. The original workshop document yielded both requested
facts in 7.11 seconds, and the prior-chat marker was recalled. Direct-model streaming and
cancellation passed. The 3.61-second original ASR fixture decoded in 2.02 seconds warm,
with one word error out of ten. Thresholds were unchanged before evaluation.

These are single development samples, not quality or latency distributions, live audio
evidence or matched product parity. A later source change requires a new run before its
output receives the same claim. The original [v1 failures](../parity/reports/local-inference-smoke.json)
and [v2 failures](../parity/reports/local-inference-protocol-v2.json) remain immutable. A
substring checker initially accepted a fabricated owner; independent review rejected it.

Optional Metal ASR compilation is unbenchmarked and off by default. No Python service is a
runtime dependency. Any future C/C++ backend must be identified as an FFI runtime. After
installation, the complete core workflow still needs an outbound-network-blocked test,
actual traffic inspection, representative unseen tasks and sustained-session measurements.
