# Architecture

The workspace has five authored packages with concrete runtime boundaries. The patched
eframe dependency is also a workspace member so its regression runs in normal CI:

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
existing event routing. A one-shot request from `App::logic` also allows an explicit UI
pass while the root is hidden, rebuilding the assistant without showing Settings. The
renderer retains its independent hidden-window check. [Patch provenance](../vendor/eframe/CUEGLINT-PATCH.json)
records upstream hashes, modified files and retained licenses. Native child accessibility
and hidden-window behavior still need OS observations; headless tests are separate evidence.

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

Assistant answers use native selectable Markdown components. One coalesced parser worker
accepts immutable session/request/context/text identities; superseded work cannot enter the
preview cache. The cache retains at most 16 answers and 4 MiB of source. Parsing, source
comparison and indexing happen in logic/worker paths, outside drawing. Rendering bounds
nodes/text and exposes the exact full source through Copy. Links require explicit activation
and validated HTTP(S) URLs; each link's identity includes the immutable text revision to
reject delayed accessibility actions after streaming rewrites. Images remain alt text and
HTML remains literal; formatting cannot load remote content or enable tools.

Conversation drawing borrows at most 16 pre-indexed replies. Histories above 16 use explicit
Earlier/Later/Latest controls, with older views anchored to a request ID. This bounds drawing
while continuous variable-height history scrolling remains a fidelity gap. Notes retain the
existing editable TextEdit/autosave path; this increment does not generate or format notes.

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

Shortcut callbacks enqueue bounded typed actions and request a repaint; application logic
owns their effects. Registration epochs reject queued obsolete actions. The UI thread owns
registration, rebind and cleanup. Persistent canonical overrides are shared with the CLI;
missing entries use platform defaults and null disables an action. OS registration and
conflicts are separate from focused fallback and persistence. Native verification remains
open; Linux global registration is explicitly unavailable in this increment.

macOS capture uses ScreenCaptureKit's native bridge; OCR uses the installed Apple Vision
framework. These bindings are native dependencies. Credential storage remains open.
A successful native API call is not proof of recipient-side capture exclusion. Rust bindings
to C/C++ backends must be labeled honestly.

[PhotoCraft](https://github.com/storytold/photocraft/blob/main/docs/architecture.md) is an
engineering benchmark for headless core separation and shared commands. Cueglint does not
copy its editor architecture or claim its verification history.
