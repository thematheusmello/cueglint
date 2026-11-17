# Models and local inference

No model has completed release acceptance. The working CPU path uses Candle 0.11.1 for
generation and English ASR. Model revisions, download sizes, SHA-256 values, tokenizer and
license links are pinned in [the manifest](../models/local-defaults.json). Missing models
fail visibly; loading or inference never starts a download or a cloud fallback.

| Profile | Role | Model weights | Development result |
|---|---|---:|---|
| Qwen2.5 1.5B Instruct Q4_K_M | Default generation | 1,117,320,736 bytes | Original narrow grounding, completeness and followup cases passed |
| Whisper base.en | Provisional development transcription | 290,401,888 bytes | Licensed v2 synthetic fixture WER 0.50 failed; warm RTF 0.489 passed |
| Qwen2.5 0.5B Instruct Q4_K_M | Selectable smaller candidate | 491,400,032 bytes | Grounding/completeness/followup failures retained |
| Whisper tiny.en | Selectable smaller candidate | 151,060,136 bytes | Retired v1 fixture WER 0.20 failed; v2 not run |

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
General settings also imports a local artifact folder or removes a local copy. The CLI
equivalents are:

```sh
cueglint-cli models import whisper-base-en /path/to/whisper-base-en-artifacts
cueglint-cli models remove whisper-base-en
```

Import supports the catalog-pinned profiles above, with exactly their required filenames,
sizes and hashes. Folders/files must be regular filesystem objects; symlinked artifacts
are rejected. Original files remain untouched. Copies stage privately and are verified
before publication. A bad or cancelled pre-publication import preserves valid existing
files. Publication completes once started. Removal preflights the entire profile folder,
refuses unknown/link/device entries, and preserves its stable lock file. Meetings, notes
and the selected profile preference are kept; a removed selected model requires explicit
reinstallation/import or a different selection. Neither action uses the network.

The [real file lifecycle run](../parity/reports/model-management-real-001.json) imported
292,809,504 bytes of base.en artifacts under process network denial, independently checked
the pinned hashes, transcribed with the copy, removed that copy and verified the original
source remained unchanged. It is one macOS CLI case; native controls, Windows/Linux file
behavior and GUI cache release remain separate checks. Arbitrary architectures/weights are
unsupported. GUI cleanup of unverified partial downloads and recovery of orphan import
stages after forced process termination remain gaps; CLI removal refuses unknown stages
rather than deleting them blindly.

The [v3 run](../parity/reports/local-inference-candidates-v3.json) and
[independent review](../parity/reports/independent-v3-review.json) preserve exact source,
binary, fixture and configuration identities. Three basic grounding cases passed, including
refusal to invent an owner/deadline; complete-answer upper bounds were 6.60–7.55 seconds
including model verification/loading. The original workshop document yielded both requested
facts in 7.11 seconds, and the prior-chat marker was recalled. Direct-model streaming and
cancellation passed. The ASR portion used the now-retired Samantha v1 asset; its WER/RTF
values are historical and do not establish acceptance for the licensed replacement.

The [first fixed v2 run](../parity/reports/redistributable-speech-v2-asr-001.json) kept the
original words and normalization, WER ≤0.10 and warm RTF ≤1.0. Base.en produced five errors
in ten words: WER 0.50 failed, while warm decoding took 2.092 seconds for 4.2785 seconds of
audio (RTF 0.489). The voice, speed and waveform were fixed before scoring; there was no
retry or threshold change. Base.en remains a provisional local default, not a model with
release-quality evidence. Broader fixed recordings and model/backend evaluation are needed.

V2 is separately GPL-3.0-or-later, with an original formant-synthesis recording and pinned
development-only regeneration tools. The old waveform is removed from the active tree;
its unchanged metadata, measured reports and retirement ledger remain. Earlier Git objects
still retain the asset because history has not been rewritten. See [attribution](../THIRD_PARTY.md).

These are single development samples, not quality or latency distributions, live audio
evidence or matched product parity. A later source change requires a new run before its
output receives the same claim. The original [v1 failures](../parity/reports/local-inference-smoke.json)
and [v2 failures](../parity/reports/local-inference-protocol-v2.json) remain immutable. A
substring checker initially accepted a fabricated owner; independent review rejected it.

Optional Metal ASR compilation is unbenchmarked and off by default. No Python service is a
runtime dependency. Any future C/C++ backend must be identified as an FFI runtime. After
installation, the complete core workflow still needs an outbound-network-blocked test,
actual traffic inspection, representative unseen tasks and sustained-session measurements.
