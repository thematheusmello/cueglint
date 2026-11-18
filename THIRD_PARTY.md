# Third-party attribution

Cueglint's authored code is MIT OR Apache-2.0. Dependencies retain their own licenses;
`Cargo.lock` pins exact versions. Model weights are not covered by the code license.

- eframe/egui/wgpu: MIT OR Apache-2.0 (see each package), native UI/rendering.
  eframe 0.35.0 is vendored with child-window AccessKit initialization before visibility and a one-shot
  hidden-root UI wake; the original hidden-window renderer check is retained.
  Exact source/archive identities, patch scope and original licenses are retained in
  `vendor/eframe/CUEGLINT-PATCH.json` and `vendor/eframe/LICENSE-*`.
- pulldown-cmark 0.13.6: MIT, bounded native answer Markdown parsing.
  url 2.5.8: MIT OR Apache-2.0, explicit HTTP(S) link validation; no resource fetching.
  Exact upstream notices are retained in `crates/cueglint/third-party/`.
- Candle: MIT OR Apache-2.0, Rust inference. The Whisper mel-filter coefficient asset
  `crates/cueglint-local/assets/melfilters.bytes` is from Hugging Face Candle's Whisper
  example. Candle's audio processing attributes whisper.cpp (MIT). The original upstream
  asset SHA-256 is `85818f156f7e189453901a515e4726d270d307f976e161cf9403e8caab405498`;
  obtained from Candle commit `cfcbd7186d13271843ce7eb41d25659bd0835850`.
  The local standard-Whisper frontend adapts only Candle 0.11.1's FFT/DFT helpers
  from commit `59f8e903ca80a32ea7f8514b39748fc979e69e82`. The helper retains the
  inherited whisper.cpp MIT notice, including Copyright (c) 2023 Georgi Gerganov,
  from commit `4774d2feb01a772a15de81ffc34b34a1f294f020`. Exact license texts,
  source identities and modification scope are in
  `crates/cueglint-local/assets/licenses/asr-frontend/`; the helper source also
  preserves the full inherited MIT notice. Newly authored feature assembly
  remains MIT OR Apache-2.0.
- ScreenCaptureKit Rust bindings: MIT OR Apache-2.0; uses Apple's platform frameworks
  and a bundled native bridge. This is a native dependency, not a pure Rust OS implementation.
- objc2/Vision bindings: their Zlib/Apache-2.0/MIT terms; Apple's installed Vision framework
  supplies local OCR. This is not an independently implemented OCR model.
- CPAL: Apache-2.0, native audio capture.
- global-hotkey 0.10.0, keyboard-types and crossbeam-channel: MIT OR Apache-2.0,
  ordinary-key macOS shortcut registration and bounded event delivery. Windows uses
  the documented OS RegisterHotKey path through existing winit/windows-sys bindings.
- SQLite via rusqlite: rusqlite MIT; SQLite public domain; bundled native SQLite library.
- Knowledge ingestion: quick-xml 0.41.0, zip 9.6.1 and lopdf 0.47.0 are MIT;
  flate2 1.1.11 is MIT OR Apache-2.0 and zlib-rs 0.6.8 is Zlib. Exact original
  notices and direct source identities are retained in the
  [knowledge dependency directory](crates/cueglint-core/third-party/knowledge/README.md).
  The [resolved PDF ledger](crates/cueglint-core/third-party/knowledge/pdf-resolved-source-ledger.json)
  records 28 additional pinned packages and 59 original notices under their respective
  terms. Retaining source notices does not complete binary-package validation;
  distribution-readiness decisions and actual package acceptance remain open.
- The [runtime notice source collection](third-party/runtime/README.md) retains
  original locked-archive, exact-commit workspace, embedded-font and nested
  notices with a portable [coverage manifest](third-party/runtime/manifest.json).
  Its 442-package host metadata set is a conservative workspace feature
  supergraph, not an exact linked-runtime count. The
  [provenance supplement](parity/reports/runtime-notice-provenance-supplement-001.json)
  adds exact publisher declarations, separately labelled canonical MIT/CC0 terms
  and readable original font copyright metadata. The [scoped source decision](parity/reports/runtime-notice-readiness-001.json)
  adopts the recorded choices, mandatory envelopes and exact publisher terms.
  Standalone-file omissions remain provenance facts rather than erased findings.
  The [ring header repair](parity/reports/runtime-notice-ring-source-headers-001.json)
  additionally retains exact original ISC source notices for the reviewed arm64
  source family; all existing ring envelopes remain unchanged.
  The installed standard-library notice's provenance boundary and later package
  checks remain explicit. No packager behavior or license-completeness claim is
  implied by this source decision; actual output/signing and clean-install checks remain open.
- Tokenizers: Apache-2.0; configured with the Rust regex backend.
- Optional Qwen2.5 7B Instruct Q4_K_M model: the official Qwen model and tokenizer retain
  [Apache-2.0 terms](https://huggingface.co/Qwen/Qwen2.5-7B-Instruct/blob/a09a35458c702b33eeacc393d103063234e8bc28/LICENSE).
  [bartowski's quantization](https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/blob/8911e8a47f92bac19d6f5c64a2e2095bd2f7d031/README.md)
  identifies llama.cpp b3772 and the Qwen base; its exact conversion-input revision is
  unspecified. Artifact revisions and hashes are pinned in `models/local-defaults.json`.
  Explicit installation retains the official `LICENSE` beside the weights; model weights
  are not bundled with Cueglint.
- reqwest 0.12.28 and keyring 3.6.3: MIT OR Apache-2.0, explicit hosted/compatible
  HTTP requests and native macOS/Windows credential storage. Their original notices
  are retained in `crates/cueglint-providers/third-party/`. The provider crate also
  uses the locked Tokio, futures-util, rustls, url and zeroize dependencies under
  their upstream licenses. It contains original protocol fixtures and no vendor SDK code.

No Cluely code, commercial icons, fonts or binary assets are included. Cueglint uses its
own mark and original text fixtures. Research specs preserve visible behavior only.

`parity/fixtures/clean-english-v2.wav`, its transcript and fixture metadata are separately
licensed **GPL-3.0-or-later**. See the accompanying `.NOTICE` and `.COPYING`, which retain
the pinned eSpeak NG formant source/data and FFmpeg generation identities. The original
generation wrapper is MIT OR Apache-2.0. These tools are used only to regenerate test data;
they are not linked into or required by Cueglint. This fixture does not establish human
speech quality, microphone capture or system-audio correctness.

The earlier Samantha system-voice waveform was retired from active public assets because
[macOS27 SLA §2F](https://www.apple.com/legal/sla/docs/macOS27.pdf) restricts public sharing
of System Voices. Its metadata and measured reports remain historical records. See the
[retirement ledger](parity/reports/asr-fixture-retirement-001.json). The owner-authorized
[path-only branch cleanup](docs/history-cleanup.md) removed that path from writable public
histories. Original GitHub PR refs/caches can still retain older objects; no universal purge
is claimed. Historical reports retain their original identities.

The 12-utterance LibriSpeech test-clean manifest, reference text and notices in
`parity/fixtures/librispeech-testclean-12-v1/` retain **CC-BY-4.0** dataset terms and
attribution to Vassil Panayotov and contributors. Audio/archive files are not distributed
here. The separately pinned Whisper small.en research candidate retains its declared
Apache-2.0 model terms; it is not a development default or an installed-profile claim.
See the retained dataset/model notices, legal texts and frozen protocol.

Cueglint's original C/glint mark, geometric wordmark, vector/raster/icon exports and native
renderer are MIT OR Apache-2.0. They use no external image or font outlines; provenance
and export checks are in `ui/brand/manifest.json`.
