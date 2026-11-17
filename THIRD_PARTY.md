# Third-party attribution

Cueglint's authored code is MIT OR Apache-2.0. Dependencies retain their own licenses;
`Cargo.lock` pins exact versions. Model weights are not covered by the code license.

- eframe/egui/wgpu: MIT OR Apache-2.0 (see each package), native UI/rendering.
  eframe 0.35.0 is vendored with child-window AccessKit initialization and a one-shot
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
- Tokenizers: Apache-2.0; configured with the Rust regex backend.

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
