# Data and privacy

Cueglint's local core stores structured meeting data in SQLite. Default local operation must
never send audio, screenshots, documents, prompts or embeddings to hosted inference. Model
downloads and optional update checks must be distinguishable from inference traffic. No
private Cluely API or account is a production dependency.

Raw reference artifacts live in a private workspace outside the repository. Default logs
must exclude private content and secrets. Optional provider/calendar credentials belong in
the OS credential store; no credentials have been added to this repository.

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
