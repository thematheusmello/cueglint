# Architecture

The initial workspace has five packages with concrete runtime boundaries:

| Package | Responsibility |
|---|---|
| cueglint-core | Typed domain state, sessions, persistence, requests, modes, knowledge and exports |
| cueglint | Native eframe/egui desktop, wgpu renderer, window coordination and background job ownership |
| cueglint-cli | Headless access to the same domain operations |
| cueglint-local | Pinned local models, Candle inference, bounded audio and macOS screen/OCR integration |
| xtask | Registry/progress validation, matched PNG comparisons and packaging |

The UI has no webview. eframe 0.35 is explicitly pinned because it is available on this
host and supports the installed Rust toolchain; this is a working-slice choice subject to
measured text, transparency, focus and accessibility checks. Native runtime evidence has
not yet established the toolkit's full reference fidelity. One coherent UI stack is retained.

SQLite transactions and explicit schema versions protect durable state. A process lock
protects a live GUI from accidental recovery by another CLI process. Transcript segments
have stable identities; a revision replaces a partial rather than appending duplicate words.
Hypotheses remain in memory and do not enter durable history, exports or backups. Captured
segments retain session-relative source time and ASR model metadata, without claiming that
an audio source identifies an individual. User-edited notes must not be
silently replaced by generated output. Deletion includes derived state and future jobs.

Session, request and context identities gate worker results. A result belongs to the frozen
context revision that created it. Cancellation, reset, session/mode/model replacement and
deletion invalidate superseded work. Long-running capture, filesystem and inference work
belongs to bounded background jobs, never audio callbacks or UI drawing.

macOS capture uses ScreenCaptureKit's native bridge; OCR uses the installed Apple Vision
framework. These bindings are native dependencies. Global shortcuts and credential storage
remain open integrations. A successful native API call is not proof of recipient-side capture
exclusion. Rust bindings to C/C++ backends must be labeled honestly.

[PhotoCraft](https://github.com/storytold/photocraft/blob/main/docs/architecture.md) is an
engineering benchmark for headless core separation and shared commands. Cueglint does not
copy its editor architecture or claim its verification history.
