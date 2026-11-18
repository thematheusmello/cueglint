# Data and privacy

Cueglint's local core stores structured meeting data in SQLite. Default local operation must
never send audio, screenshots, documents, prompts or embeddings to hosted inference. Model
downloads and optional update checks must be distinguishable from inference traffic. No
private Cluely API or account is a production dependency.

The desktop's `--database PATH` also isolates UI preferences in `PATH.ui.ron`, taking
precedence over `CUEGLINT_DATA_DIR`. Without an explicit database, a nonempty
`CUEGLINT_DATA_DIR` stores the database and UI preferences (`app.ron`) together. An unset
or empty override keeps the existing default locations; no preferences are migrated.
Labelled fixtures retain their isolated temporary UI paths and in-memory databases.

Raw reference artifacts live in a private workspace outside the repository. Default logs
must exclude private content and secrets. Optional provider/calendar credentials belong in
the OS credential store; no credentials have been added to this repository.

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

The [portable format](data-format.md) is versioned UTF-8 JSON plus human-readable
note/transcript exports. Backups validate before a transaction changes the current database. Deleting
local data cannot revoke copies already exported or held in independent backups; the app
must explain that behavior. Optional sharing requires explicit publication and revocation.

Inspect actual implementation and [STATUS.md](../STATUS.md) for current capabilities; this
document states required guarantees, not evidence that all controls are complete.
