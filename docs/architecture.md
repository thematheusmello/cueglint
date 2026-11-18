# Architecture

The workspace has six authored packages with concrete runtime boundaries. The patched
eframe and egui-wgpu dependencies are also workspace members so their regressions run in normal CI:

| Package | Responsibility |
|---|---|
| cueglint-core | Typed domain state, sessions, persistence, requests, modes, knowledge and exports |
| cueglint | Native eframe/egui desktop, wgpu renderer, window coordination and background job ownership |
| cueglint-cli | Headless access to the same domain operations |
| cueglint-local | Pinned local models, Candle inference, bounded audio and macOS screen/OCR integration |
| cueglint-providers | Explicit hosted/compatible protocols, bounded HTTP streaming and native credentials |
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

Explicit native screenshot requests retain one UUID through viewport ownership, GPU copy,
readback and private PNG export. A bounded viewport queue keeps tokens through render skips;
only actual copy submission hands them to readback. [Renderer patch provenance](../vendor/egui-wgpu/CUEGLINT-PATCH.json)
and the [transport contract](../spec/features/native-screenshot-transport.md) distinguish
finite diagnostics and headless ownership checks from actual GPU evidence. No child-window
readback or cause for the retained combined85 timeout is inferred from this source repair.

SQLite transactions and explicit schema versions protect durable state. A process lock
protects a live GUI from accidental recovery by another CLI process. Transcript segments
have stable identities; a revision replaces a partial rather than appending duplicate words.
Hypotheses remain in memory and do not enter durable history, exports or backups. Captured
segments retain session-relative source time and ASR model metadata, without claiming that
an audio source identifies an individual. User-edited notes must not be
silently replaced by generated output. Deletion includes derived state and future jobs.

An explicit desktop `--database PATH` stores UI preferences in `PATH.ui.ron`;
otherwise a nonempty `CUEGLINT_DATA_DIR` selects its own `app.ron`. Fixtures keep
their existing temporary namespace, and ordinary launches retain the platform
default. Selecting a custom namespace does not migrate default preferences. A
[controlled native launch](../parity/reports/native-provider-ui-002-failure.json)
created the custom sidecar with all checked default endpoints unchanged; this is
one macOS observation, not concurrent-writer or provider-flow acceptance.

Session, request, provider and context identities gate worker results. A result belongs to the frozen
context revision that created it. Cancellation, reset, session/mode/model replacement and
deletion invalidate superseded work. Long-running capture, filesystem and inference work
belongs to bounded background jobs, never audio callbacks or UI drawing.

Knowledge extraction is shared in `cueglint-core::knowledge`. UTF-8 stays intact;
a bounded Rust ZIP/XML profile extracts supported DOCX main-body text. The desktop
owns one parser/result slot outside the store actor, with a stable import UUID,
cooperative deadline/cancellation and a second check before insertion. The actor
keeps processing other work; stale UI acknowledgements cannot replace a newer
import. Each imported document belongs to the shared local library. Modes can limit fresh
retrieval to selected entries without duplicating extracted text. The request builder
applies this scope before ranking. [Import contract](../spec/features/native-knowledge-docx.md).

In Settings → Modes, clear **Search all library files**, select the reference files and save
the mode. **Use this mode** applies the saved choice to the next fresh chat or session.
The CLI accepts `mode set-files MODE_ID DOCUMENT_ID...`, with no document IDs for an empty
selection or `--all` for whole-library retrieval. `knowledge list` provides the document IDs.
Both clients use the same store validation and frozen request builder.

**Duplicate as draft** opens an editable copy of a saved mode in the desktop editor. Use
**Save mode** to keep the copy or **Use this mode** to save and select it for a fresh chat or
session. The CLI `mode duplicate MODE_ID` saves the copy immediately. Both actions copy the
instructions and exact file selection and leave existing sessions with their original mode.

The strict PDF route starts the same executable with one fixed internal argument;
CLI and GUI dispatch it before normal initialization. The parent sends at most
2 MiB through stdin and accepts at most 2 MiB of validated text through stdout,
with the original ten-second deadline. Only that child enables the 256 MiB
cumulative Rust allocation-work budget; normal allocation delegates to System.
This is not an RSS/native-allocation cap. Budget exhaustion exits immediately with
status 73; malformed/unsupported text exits with 74. Every parent return/drop kills
and waits for the owned child and joins both pipe workers before indexing.
External stream fields are rejected before decoding; no raw temporary document,
inference or network path is used. Normal quit waits for the matching import
completion, and the initial store snapshot succeeds before starting a parser.
The source/headless policy is checked; actual native import/quit remains separate.
[PDF contract](../spec/features/native-knowledge-pdf.md).

The same bounded inference actor owns local answers and explicit provider jobs. Settings
default to Local; old records without provider fields also decode as Local. Each accepted
request freezes the complete provider/model/endpoint alongside its context revision.
Selection changes cancel generation and network listing/probing. UUID-and-credential-scope
bound key save/delete/status operations can finish without a later settings change losing a
user's key. A matching acknowledgement alone unlocks or clears its masked input.

The provider crate uses a current-thread Tokio/reqwest client with cancellation polling,
120-second total and20-second connect deadlines, bounded bodies/SSE/text/catalogs, and no
redirects, retries or inherited proxy. A process-wide resolver permit bounds outstanding
uninterruptible OS DNS work. It may remain occupied until a cancelled resolver returns.
Credential APIs can also be uninterruptible; drawing and the store actor do not wait on them.
Active-key prefixes are withheld before streaming callbacks, preventing a fragmented echo
from exposing the key. Errors contain typed summaries rather than response bodies.

Local inference metadata and hosted provenance are separate. Hosted records keep requested
and reported models, endpoint, optional usage/version/request identifiers, citations and
prompt/template hashes. Unknown remote model revisions stay unknown. Completion validates
the provenance against the frozen request. No provider tool execution is enabled.

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
editable TextEdit/autosave path. Explicit derived highlights stay separate and
require review before revision-checked append. Prompt template
`cueglint-qwen-highlights-v2` asks for importance-based whole-utterance selection
before input ordering; the notes-only JSON decoder and source validation remain
separate. The [actual prompt-v2 evaluation](../parity/reports/highlights-quality-actual-003.json)
still fails both fixed semantic gates, so prompt clarity and valid formatting do
not establish summary/action generation or useful salience.

New requests freeze at most 64 recent whole finalized transcript segments and eight whole
completed chat turns, with a separate 16 KiB text budget for each. History stays intact;
omission counts and diagnostics describe selection. Generation applies the model's token
budget, escapes context as data, and records the combined omissions, exact rendered prompt
hash and template revision. See the [portable data format](data-format.md) for legacy rules.

Model caches include both directory and selected profile. Capture pins its transcription
profile for the lifetime of the job. A configured capture source is distinct from OS
authorization and active hardware; permission status reads do not request access. Explicit
capture can request standard OS authorization, and denial has an actionable recovery path.

General's microphone and capture-display pickers use the existing typed Settings
and bounded store/runtime channels. Explicit Refresh inputs owns one background
job, with request-bound cancellation and stale-result rejection. Drawing reads a
snapshot; enumeration starts no recording or model work. Cancellation retains the
slot until an in-flight native API returns. Failed refreshes keep the last list
and error, and missing or unsupported saved selections remain stored.

The microphone uses its serialized CPAL ID; Windows passes that exact WASAPI
identity through the existing native endpoint bridge. One macOS display ID feeds
both OCR and system audio; null resolves the OS main display, while a missing
explicit ID fails. Changing the display retires old screen previews and requests.
Screen sampling occurs for each enabled question or explicit preview; audio starts
when a session starts/resumes. The original display-list API can request access;
the refresh-specific authorized-list API makes no explicit permission request.
Legacy malformed IDs stay readable and have an explicit default-input recovery
action. [Contract](../spec/features/native-capture-input-selection.md) and
[implementation evidence](../parity/reports/capture-inputs-integrated-workspace-001.json)
do not establish actual device, permission, capture or reference acceptance.

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
framework. These bindings are native dependencies. API keys use native macOS Keychain or
Windows credential storage, scoped to provider plus the canonical full API base URL.
Unsupported stores fail visibly without plaintext fallback; an explicitly keyless compatible
endpoint remains possible. Calendar credential integration is still open.
A successful native API call is not proof of recipient-side capture exclusion. Rust bindings
to C/C++ backends must be labeled honestly.

[PhotoCraft](https://github.com/storytold/photocraft/blob/main/docs/architecture.md) is an
engineering benchmark for headless core separation and shared commands. Cueglint does not
copy its editor architecture or claim its verification history.
