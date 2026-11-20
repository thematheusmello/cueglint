# Data and privacy

Cueglint's local core stores structured meeting data in SQLite. Default local operation must
never send audio, screenshots, documents, prompts or embeddings to hosted inference. Model
downloads and optional update checks must be distinguishable from inference traffic. No
private Cluely API or account is a production dependency.

General Settings and `cueglint-cli check-update` contact the public GitHub
release feed only on request. The check sends an anonymous GET with a fixed
application name, uses a ten-second deadline and reads up to 128 KiB. Opening
the selected release page is a separate action in the system browser. The CLI
check works without opening the meeting database.
[Release check evidence](../parity/reports/native-manual-release-check-001.json) ·
[GitHub release API](https://docs.github.com/en/rest/releases/releases#list-releases)

The desktop's `--database PATH` also isolates UI preferences in `PATH.ui.ron`, taking
precedence over `CUEGLINT_DATA_DIR`. Without an explicit database, a nonempty
`CUEGLINT_DATA_DIR` stores the database and UI preferences (`app.ron`) together. An unset
or empty override keeps the existing default locations; no preferences are migrated.
Labelled fixtures retain their isolated temporary UI paths and in-memory databases.

Raw reference artifacts live in a private workspace outside the repository. Default logs
must exclude private content and secrets. Optional provider/calendar credentials belong in
the OS credential store; no credentials have been added to this repository. Calendar event pages stay in memory.
The reminder opt-in and up to 256 opaque occurrence digests live in the device UI
preferences. A granted token's reminder scope stays in the Calendar vault.
Only the selected event's display title enters a session after Start.

Optional answer providers require deliberate selection in General settings or the CLI.
Saving a key or selection makes no model request. Refresh models explicitly contacts the
selected catalog; Test connection sends a tiny synthetic instruction without meeting data.
A remote question sends the frozen prompt and selected transcript/chat/knowledge/mode and
enabled screen text. Audio and transcription remain local. The UI names the destination
and states that provider charges may apply. Provider retention and billing follow that
account's terms; client cancellation cannot guarantee server processing or billing stops.

API keys are scoped to provider plus the canonical full base URL and stored through native
macOS/Windows credentials. They are absent from settings, persisted request records, SQLite, logical backups,
exports and default logs. Custom endpoints cannot inherit official-provider keys. Credential
failures never become plaintext storage or an implicit keyless fallback on a supported OS.
Other platforms currently report unavailable key persistence and can use an explicitly
keyless compatible endpoint. See [provider setup](providers.md) for the current limits.

Session capture, listening, display and saving are distinct states. The local user must see
active sources and be able to stop immediately. Audio buffers, frames, context and jobs are
bounded; raw audio retention is a separate explicit setting. Retention and deletion controls
remain required until tested in the working UI.

Saved capture selections use stable IDs rather than device names. Refresh inputs
lists native inputs/displays in one bounded background job, without starting
recording, inference, networking or credential work. Its display-list path makes
no explicit permission request; OS dialogs during permission-revocation races are
not ruled out. Refresh failure, cancellation or disappearance does not silently
replace a saved ID with another device. Previous lists remain labelled stale.
Legacy malformed IDs are readable; Use default capture inputs explicitly clears
both IDs without migrating preferences or changing session data.

On macOS, the configured display supplies system audio and display-based
screen context. Audio starts or resumes with the session. Windows loopback
uses its default playback endpoint independently of the display. Test receipts
use original synthetic fixtures and record source and log identities.
[Capture-input workspace checks](../parity/reports/capture-inputs-integrated-workspace-001.json).

In **Single window** mode on macOS 15.2 or later, the system picker asks for one
window for each screen-enabled question or preview. The native stream stops
before Cueglint uses the sample for local OCR. The image stays in memory. A
saved request keeps the recognized text, selected window ID, capture time,
dimensions and hashes of its PNG and RGBA pixels. Historical Display requests
retain their existing saved fields. The existing provider setting controls
whether that recognized text is sent with a request. [Window capture contract](../spec/features/macos-window-context.md).

Knowledge accepts bounded UTF-8/Markdown, supported DOCX main-body text and a strict
text-PDF subset. DOCX uses local Rust ZIP/XML code in one cancellable background
slot; it never executes macros, resolves relationships or fetches remote content.
PDF uses the same native executable's fixed early self-child mode before app/model
initialization. Raw stdin and validated UTF-8 stdout are each bounded at 2 MiB,
with a ten-second parent deadline and a child-only 256 MiB cumulative Rust
allocation-work budget, not an RSS cap. External `F`, `FFilter` and `FDecodeParms`
stream fields are rejected. This import path performs no inference/network work,
creates no temporary document and does not retain the original PDF/DOCX bytes.
Extracted text and
its source filename/path enter the local library and versioned backups; offsets
refer to extracted text, not PDF pages or ZIP bytes. Deleting an imported item keeps
the original source file and uses the existing derived-data/context invalidation.
Parent cancellation/failure kills and waits for the PDF child and joins its pipe
workers before return; no partial text is indexed. Budget exhaustion exits without
logging document content. OS calls retain scheduling/syscall limits; actual native
import/quit and forced-process-death cleanup are not established by headless checks.
Unsupported content fails visibly. Exact [DOCX](../spec/features/native-knowledge-docx.md)
and [PDF](../spec/features/native-knowledge-pdf.md) profiles retain their wider gaps.

The [portable format](data-format.md) is versioned UTF-8 JSON plus human-readable
note/transcript exports. The desktop reviews the chosen archive, requires
explicit replacement of existing data, and validates the backup before a
transaction changes the library. Desktop restore selects Local answers and
preserves historical provider attribution. Deletion cannot revoke copies
already exported or held in independent backups. Optional sharing requires
explicit publication and revocation.

Inspect actual implementation and [STATUS.md](../STATUS.md) for current capabilities; this
document states required guarantees, not evidence that all controls are complete.
