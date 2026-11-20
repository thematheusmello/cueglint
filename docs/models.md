# Models and local inference

No model has completed release acceptance. The working CPU path uses Candle 0.11.1 for
generation and English ASR. Model revisions, download sizes, SHA-256 values, tokenizer and
license links are pinned in [the manifest](../models/local-defaults.json). Missing models
fail visibly; loading or inference never starts a download or a cloud fallback.

| Profile | Role | Model weights | Development result |
|---|---|---:|---|
| Qwen2.5 1.5B Instruct Q4_K_M | Default generation | 1,117,320,736 bytes | Narrow grounding/followup passed; highlights001 validity failed and 002/003 salience failed |
| Whisper base.en | Provisional development transcription | 290,401,888 bytes | Corrected-frontend fixed v2 WER 0.30 failed; warm RTF 0.495 passed |
| Qwen2.5 0.5B Instruct Q4_K_M | Selectable smaller candidate | 491,400,032 bytes | Grounding/completeness/followup failures retained |
| Whisper tiny.en | Selectable smaller candidate | 151,060,136 bytes | V2 direct fixture missed the spoken hour; three-profile synthetic cohort failed accuracy |
| Whisper base multilingual | Optional explicit input languages | 290,403,936 bytes | One of two Portuguese read-speech clips met the fixed 0.10 WER limit |
| Qwen2.5 7B Instruct Q4_K_M | Experimental optional generation | 4,683,074,240 bytes | Loading, highlights quality, latency and memory unmeasured |

A [63-call CPU comparison](../parity/reports/local-asr-profile-comparison-001.json)
ran four development passages and three previously undecoded English passages
through each installed Whisper profile. Held-out results use the same 22
reference words at each input alignment. Latency includes loading the model
in a fresh process. Filesystem caches and host load were uncontrolled.

| Profile | Original WAV errors | Endpoint 0 ms errors | Endpoint 10 ms errors | Held-out p90 RTF |
|---|---:|---:|---:|---:|
| Base.en | 5/22 | 5/22 | 5/22 | 0.935 |
| Tiny.en | 3/22 | 3/22 | 5/22 | 0.586 |
| Base multilingual | 4/22 | 4/22 | 3/22 | 1.095 |

Every profile exceeded the 0.10 word-error ceiling. Multilingual base
retained the known meeting hour at all three input alignments. Its p90
load-and-decode time exceeded the duration of its input. Tiny.en was the
fastest and preserved the hour only at 10 ms. Each alternative improved
one held-out passage against base.en. Two were required for admission.

The two defaults need about 1.42 GB of download storage including metadata/tokenizers.
Working memory, long-meeting growth and power use remain unmeasured; download size is not
a RAM requirement. Measurements below used an Apple M3 Max with 48 GB RAM, macOS 27.2
arm64. Other hardware has no performance guarantee. English-only ASR does not establish
multilingual transcription, translation or interface localization.

The optional `whisper-base` profile accepts 99 explicit language-token codes; this is not
99-language quality coverage. Select the model and input language separately in Local models
settings or the CLI. Installation never changes either selection:

```sh
cueglint-cli models install whisper-base
cueglint-cli settings set-transcription-profile whisper-base
cueglint-cli settings set-transcription-language pt
cueglint-cli transcribe /path/to/input.wav --profile whisper-base --language pt --backend cpu
```

The input-language default remains `en`. A one-file override does not save preferences.
Non-English input with either English-only profile fails visibly and preserves both choices;
no model is substituted. `model-status` includes the selected `transcription_language` and
marks an incompatible pair unavailable without loading a model/device. The setting neither
translates speech nor selects response or UI language. Output `language` records the worker's
explicit input selection; the model does not verify or detect it. See the
[language contract](../spec/features/local-transcription-language.md).

The multilingual profile's three artifacts total 292,886,385 bytes. Its exact revision,
config/tokenizer SHA-256 values, tokenizer Git-object check and weight LFS metadata are in
[the source record](../models/whisper-base-sources.json). The downloaded Hugging Face model
card declares Apache-2.0 in YAML; the pinned upstream OpenAI README explicitly releases
code and model weights under MIT. The catalog retains both actual declarations and links
the pinned Hugging Face card. This provenance difference is preserved, without inventing
an SPDX dual-license expression. The source record links both publisher texts and hashes.
The packaged CPU CLI subsequently ran verified weights with process networking
blocked. The synthetic English control had 0/18 word errors and synthetic Portuguese
had 12/19. A separately declared Brazilian Portuguese read-speech check measured
3/36 and 7/24 word errors. Only the first human clip met the predeclared 0.10 WER
limit. Both calls used the pinned original publisher bytes and ran once each. This
small cohort leaves model selection, broader speech quality and live latency open.
[Packaged synthetic results](../parity/reports/packaged-multilingual-asr-001.json) ·
[Human read-speech results and fixture attribution](../parity/reports/packaged-human-portuguese-asr-001.json)

Install explicitly through General settings or the CLI:

```sh
cargo run --release -p cueglint-cli -- models install qwen2.5-1.5b-instruct
cargo run --release -p cueglint-cli -- models install whisper-base-en
cargo run --release -p cueglint-cli -- model-status
```

The optional `qwen2.5-7b-instruct` profile uses
[bartowski's single-file Q4_K_M quantization](https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/blob/8911e8a47f92bac19d6f5c64a2e2095bd2f7d031/README.md)
with the official Qwen tokenizer and Apache-2.0 license. Its three pinned artifacts total
4,690,117,228 bytes (4.368 GiB). The quantizer identifies the Qwen base and llama.cpp b3772,
but does not pin the exact conversion-input model revision. It is experimental and needs
separate fixed and unseen highlights evaluation. Installation does not select it; use the
existing explicit override for a deliberate highlights run:

```sh
cueglint-cli models install qwen2.5-7b-instruct
cueglint-cli models verify qwen2.5-7b-instruct
cueglint-cli highlights generate SESSION_ID --profile qwen2.5-7b-instruct
```

Allow roughly 12–14 GiB of process memory for planning on the CPU path. This is a
source-derived estimate, not a measured peak or an upper bound: Candle dequantizes token
embeddings and can retain additional ARM64 packed weights. Download size is not a RAM
requirement. A clean direct install grows a partial file and renames it after verification;
an import copy or extra download cache needs additional disk space.

[One actual macOS CLI recovery](../parity/reports/model-install-recovery-001.json)
verifies all three 7B artifacts after the fixed-client transfer completed in 126.13 seconds.
The earlier transfer and separate probe-preflight failures remain recorded. Installation
leaves model selection unchanged; loading/inference, quality and process memory are still
unmeasured. [The source regression check](../parity/reports/model-download-phase-budgets-001.json)
keeps response/body deadlines distinct, rejects stalled or over-budget bodies, and preserves
existing files on failure. GUI stalled-download cancellation remains unverified.

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

Managed generation and transcription consume the same read-only file handles that supplied
their pinned hashes. GGUF loading stays tensor by tensor without an extra whole-model
buffer; ASR uses owned buffers from the retained handles. All required artifacts, including
licenses, still verify. This preserves inputs across cooperative atomic replacement/removal
without creating reader locks or requiring a writable root. Legacy catalog directories
without `.install.lock` still work. Uncooperative in-place writes are outside this contract.
Status verification returns a metadata snapshot and closes its handles; it promises no
future lease. Model constructors close theirs after owned tensors/tokenizers are ready.
The native answer/highlight cache remains an owned loaded snapshot with its existing reset
policy. CLI transcription metadata comes from that load's bound profile, and `elapsed_ms`
now includes verification as well as loading and decoding. Historical measurements retain
their original source and timing scope. [Contract](../spec/features/verified-model-inputs.md).

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

The separate [actual highlights run 001](../parity/reports/highlights-quality-actual-001.json)
at source `d28b390` used the pinned Qwen2.5 1.5B Q4_K_M CPU profile and a frozen
two-case protocol. Each case made one generation attempt and failed validation:
the response selected more than eight utterances. No retry, output repair or
fallback followed. Neither candidate nor generated notes was stored; existing
notes, summary, title and all 24 source rows were preserved. Salience remains
`not_scored`, and acceptance/append checks remain `not_run`. The
[independent audit](../parity/reports/independent-highlights-quality-actual-001.json)
binds the source, CLI, fixed fixtures, expected prompts, external configuration,
model endpoint digests and retained command/persistence evidence.

The exact generated arrays, token counts and engine-emitted prompt/configuration
metadata were not retained. Frozen-source control flow and the exact error support
completed decode followed by count rejection; they do not recover unknown output.
CLI completion times of 10.44028 and 12.7807 seconds include initialization and
validation, rather than isolated model latency or a latency distribution. These
failures leave broader summary, decision, action-item and reference quality open.

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

The [second local highlights evaluation](../parity/reports/highlights-quality-actual-002.json)
at `5845d00` uses the same original cases, Qwen1.5B pins, prompts, eight-item cap,
128-token budget and0.80 salience thresholds after a notes-only decoder correction.
Both attempts complete valid source-bound JSON with actual end-of-turn and policy
provenance. Both still fail salience: precision0.50, recall0.667, category coverage0.60
and critical coverage0.75. No acceptance/append runs after failure. This is a format
and provenance correction, with a failed usefulness evaluation, not general model
quality or reference parity. Original execution001 remains failed separately.

The [third local highlights evaluation](../parity/reports/highlights-quality-actual-003.json)
at `94848291` changes the generic salience prompt to v2, while retaining the same
model pins, decoder, two original cases, eight-item cap, 128-token budget and
semantic thresholds. Both valid end-of-turn generations again select the first
eight indices: precision 0.50, recall 2/3, category coverage 0.60 and critical
coverage 0.75. The prompt clarification did not improve these fixed-case scores.
Exact support and full plan coverage remain separate from usefulness. No draft
was accepted or appended; manual data and prior failures remain unchanged.
The [independent audit](../parity/reports/independent-highlights-quality-actual-003.json)
binds the retained execution to source/build/configuration and persisted drafts;
it did not rerun the model. This is repeated-fixture regression evidence, not new
held-out quality, broad usefulness or reference parity.

## Prerecorded two-source transcription

The production endpoint and pinned Whisper base.en CPU decoder processed two
interleaved 16 kHz file inputs with network access denied for the test process.
Both routes kept their segment identity from partial to final. Their final word
error counts were 0/17 and 2/17 against a predeclared per-route limit of 10%.
A compound-word spacing difference accounts for both edits. The signed CLI
returned the same normalized words on the full WAV for that case. Native devices and a complete offline meeting remain open.
[Run and model identities](../parity/reports/offline-two-route-file-asr-001.json)

## Structured meeting-note experiments

Both experiments used pinned Qwen2.5 1.5B on Candle CPU, with network access denied
and a read-only model cache. Each used a fictional transcript and ran two formats
once. The first experiment selected every expected utterance along with unrelated
text and an unapproved scheduling proposal. All six sections missed the requirement
to select exactly the expected source IDs.
[Initial structured-note report](../parity/reports/meeting-notes-format-quality-actual-001.json)

The follow-up checked each proposed excerpt against its section and focus on an
unseen transcript. Both decisions sections reached precision 1.0 and recall 0.5.
The rack summary retained an unrelated cost. The packing-cost summary and both
action sections were empty. All six sections missed the same gate.
[Unseen eligibility report](../parity/reports/meeting-notes-format-eligibility-unseen-001.json)

Both runs used a private development engine and verified complete transcript
coverage, exact source spans and preservation of manual notes, title and summary
through draft creation. The quality gate stopped the runs before acceptance, append
and restore. Native format editing and the complete generation and review workflow
remain pending.
