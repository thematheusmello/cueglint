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
The vendored eframe patch initializes native AccessKit for child viewports through its
existing event routing. [Patch provenance](../vendor/eframe/CUEGLINT-PATCH.json) records
upstream hashes, the one changed file and retained licenses. Native child accessibility
still needs an actual OS observation; headless semantic tests are separate evidence.

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

New requests freeze at most 64 recent whole finalized transcript segments and eight whole
completed chat turns, with a separate 16 KiB text budget for each. History stays intact;
omission counts and diagnostics describe selection. Generation applies the model's token
budget, escapes context as data, and records the combined omissions, exact rendered prompt
hash and template revision. See the [portable data format](data-format.md) for legacy rules.

Model caches include both directory and selected profile. Capture pins its transcription
profile for the lifetime of the job. A configured capture source is distinct from OS
authorization and active hardware; permission status reads do not request access. Explicit
capture can request standard OS authorization, and denial has an actionable recovery path.

Model file operations invalidate their profile's readiness and verification epoch before
starting. Old verification results cannot restore removed files. Every terminal attempt
queues a cache reset in the inference actor before its completion event, including errors;
new answers require fresh verification. Audio/answers and model file changes exclude each
other. The UI retains a question until a new frozen request accepts it, so setup or screen
preparation errors do not discard the draft or create an empty chat.

macOS capture uses ScreenCaptureKit's native bridge; OCR uses the installed Apple Vision
framework. These bindings are native dependencies. Global shortcuts and credential storage
remain open integrations. A successful native API call is not proof of recipient-side capture
exclusion. Rust bindings to C/C++ backends must be labeled honestly.

[PhotoCraft](https://github.com/storytold/photocraft/blob/main/docs/architecture.md) is an
engineering benchmark for headless core separation and shared commands. Cueglint does not
copy its editor architecture or claim its verification history.
