# Models and local inference

No model has completed release acceptance. The working CPU path uses Candle 0.11.1 for
generation and English ASR. Model revisions, download sizes, SHA-256 values, tokenizer and
license links are pinned in [the manifest](../models/local-defaults.json). Missing models
fail visibly; loading or inference never starts a download or a cloud fallback.

| Profile | Role | Model weights | Development result |
|---|---|---:|---|
| Qwen2.5 1.5B Instruct Q4_K_M | Default generation | 1,117,320,736 bytes | Original narrow grounding, completeness and followup cases passed |
| Whisper base.en | Provisional development transcription | 290,401,888 bytes | Corrected-frontend fixed v2 WER 0.30 failed; warm RTF 0.495 passed |
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

The `whisper-standard-v1` frontend corrects centered/reflected framing and uses one-sided
power without mirrored-bin doubling. Its canonical configuration includes the filter-table
hash; transcripts and CLI output retain backend/frontend identities. Fixed numerical tests
and an independently hashed feature tensor check pass, separately from transcription
quality. The [actual corrected-model pair](../parity/reports/whisper-frontend-v2-asr-001.json)
still failed the unchanged WER limit: both cold and warm output had three errors in ten
words (0.30). Warm decoding took 2.118 seconds for 4.2785 seconds of audio (RTF 0.495),
passing its narrow realtime criterion. It matched the earlier independent standard path.
There was one cold/warm pair under process network denial, with no output retries or
fallbacks. Earlier failures remain visible. FFT attribution and exact upstream licenses
are retained beside the local crate's assets.

V2 is separately GPL-3.0-or-later, with an original formant-synthesis recording and pinned
development-only regeneration tools. The old waveform is removed from the active tree;
its unchanged metadata, measured reports and retirement ledger remain. The owner-authorized
path-only cleanup removed it from writable branch history; read-only PR objects and host
caches may still retain it. See [history cleanup](history-cleanup.md) and
[attribution](../THIRD_PARTY.md).

These are single development samples, not quality or latency distributions, live audio
evidence or matched product parity. A later source change requires a new run before its
output receives the same claim. The original [v1 failures](../parity/reports/local-inference-smoke.json)
and [v2 failures](../parity/reports/local-inference-protocol-v2.json) remain immutable. A
substring checker initially accepted a fabricated owner; independent review rejected it.

Optional Metal ASR remains off by default. Its separate development worker has the bounded
file-ASR measurements below; the packaged app still uses the CPU default. No Python service is a
runtime dependency. Native Metal uses objc2 bindings and Apple frameworks; it is not a
pure-Rust inference runtime. Any future C/C++ backend must be identified as an FFI runtime. After
installation, the complete core workflow still needs an outbound-network-blocked test,
actual traffic inspection, representative unseen tasks and sustained-session measurements.

The fixed 12-speaker LibriSpeech clean-English CPU run at source25afff84 completed all28
calls and three model loads under process network denial. Base.en scored10/279 word
errors (WER0.03584) and every scored RTF was≤1 (worst0.893). Small.en scored7/279
(WER0.02509), but all12 human clips exceeded RTF1 (worst2.637). Its separate v2 warm
run reached WER0.10 and failed speed atRTF1.556. The suite therefore failed; the default
remains base.en. The earlier base-v2 WER0.30 failure is unchanged. This is a small clean
read-English file set, not live dual-source capture or general language/reference parity.
The retained [actual report](../parity/reports/human-asr-cpu-actual-002.json) includes all
outputs, fixed references, load/decode timing distributions and configuration identities;
the [independent review](../parity/reports/independent-human-asr-cpu-review-002.json)
recomputed every score and input/model/source endpoint.

The separate [native Metal run](../parity/reports/metal-asr-actual-002.json) at source
`45f2428` executed the same28 calls and three loads once, under process network denial.
Its [independent evaluation](../parity/reports/independent-metal-asr-actual-review-002.json)
checked all raw events, scores, input/model identities and304 endpoint guards. The unchanged
corpus WER≤0.10 and every scored warm RTF≤1 gates passed:

| Fixed case | WER | Median warm decode | Worst scored warm RTF |
|---|---:|---:|---:|
| Base.en,12 human utterances | 10/279 (0.03584) | 680ms | 0.0995 |
| Small.en,12 human utterances | 7/279 (0.02509) | 1,335ms | 0.2345 |
| Small.en, synthetic v2 warm call | 1/10 (0.10) | 733ms | 0.1712 |

Base.en's first call took **10.58 seconds**, RTF2.157. It is the predeclared warmup excluded
from warm gates, and remains a real first-call cost. One human clip per profile exceeded
WER0.10; the declared accuracy gate is corpus micro-WER. All28 transcripts are byte-identical
to the CPU outputs, so this is a timing improvement for these batches, not improved accuracy.
The CPU small-model speed failures and base synthetic-v2 WER0.30 failure remain unchanged.
This single M3 Max development-worker run does not verify packaged GPU operation, live audio,
overlap, sustained use, other languages or reference parity. The first rejected preflight,
cleanup-accounting correction and separate accepted preflight are retained alongside it.
