# Extractive highlights integration

This increment combines the reviewed Core, local-model and CLI implementation
with explicit desktop generation and review. Highlights select original whole
utterances. Model salience quality, native window interaction and reference
notes-trigger/regeneration behavior remain unverified. Nothing generates a title,
summary, owner, date or commitment, or runs automatically when a session ends.

## Store and portable records

Manual `update_notes` increments `notes_revision` only when the exact bytes
change. Accepted highlights remain separate from notes until an explicit
revision-checked append. Append is atomic and idempotent; a conflict preserves
user text and never retries with a newer revision. Legacy `update_summary`, title
and transcript semantics remain separate.

New session JSON/Markdown exports and backups use version 2. v1 databases/backups
remain readable with notes revision 0 and no derived records. Imported transcripts
are sorted before strict validation, before any replacement transaction. Imported
pending jobs are cancelled as restored; process startup interrupts unfinished
notes jobs. Unknown or malformed inference/notes/session fields are rejected.
Session exports have an envelope and are not a backup import API.

Provider configuration and hosted answer provenance coexist with these records.
The integration regression restores an original v1 backup with hosted metadata,
then rejects malformed v2 highlights atomically. Unknown hosted versions remain
unknown. No credential enters portable data, and highlights use only the selected
local generation profile even when the answer provider is hosted.

## Bounded desktop worker

The existing two-slot inference channel owns one cached `LocalGenerator`.
Preparation receives stopped-session `NotesInput`, retains opaque `PreparedNotes`
in that actor and returns only its typed `NotesPlan`. The Store rechecks the
complete source through `begin_notes`, creates the job ID and queues the run.
Both phases share an operation ID, session, source digest, profile/root and
cancellation token; the run additionally binds the exact pending job ID.

Only a matching whole completion may publish a draft. Progress contains validated
chunk counts, with no partial prose. Missing models, capacity failures and full
queues do not select another provider or create a publishable partial job.
Navigation, resumption, deletion, local profile/root changes and quit retire work;
mode, knowledge and screen settings are excluded from highlights input. Ready and
accepted records survive cancellation. Late events cannot reopen an operation or
clear a newer one. The GUI has no restore command and adds no generic Store epoch.

Quit signals cancellation and waits for that job's terminal actor event. A Cancel
command ACK is not a terminal event. The persistent idle inference thread is not
joined; abrupt process interruption is recovered on the next Store open.

## Notes editor and review

The existing detail Notes footer opens the optional native highlights window.
Generation requires a stopped, finalized, nonempty transcript. Review shows exact
original utterances, byte spans, source freshness and stored provenance. Copy,
Accept and Discard are explicit. Accept and Discard never edit manual notes.
The normal editor retains native selection, IME, copy and 500 ms autosave.

Append retains an intent while draining dirty notes and every manual-save ACK.
It captures the acknowledged notes revision only after that drain, then makes
the editor read-only until the matching operation/session/action ACK arrives.
A conflict requires explicit review/refresh before another append. Only a manual
Notes ACK clears `notes_saving`; derived updates preserve dirty buffers. Deferred
navigation cancels even a Generate action whose ACK has not yet arrived. Quit
retires deferred session creation and append intent.

## Verification and remaining work

The first integrated Core/local/CLI stage passed 95 weight-free tests, with three
configured-weight/oracle checks ignored and the existing native credential lookup
CLI test explicitly filtered. The desktop stage passed 97 tests with its native
credential lookup test filtered. The first desktop compile ownership error and
new fixture failures are retained separately; corrections preserve the assertions.
The final workspace receipt binds its own frozen source manifest.

New protocol checks use controlled typed events and Supplied selections, without
model execution or a quality claim. Native egui tests cover autosave before CAS,
editor locking, exact ACKs, dirty updates, conflict review and terminal cancellation.
A real fixture Worker additionally checks typed highlights replies remain separate
from manual Notes replies. No native windows, credential operations or model
inference are used by these highlights checks. Existing native BYOK tests remain
unchanged and must run in the root's final combined verification scope.

Independent source review of 473c27d found no material defect within its declared
scope. The reviewed highlights source is now combined with native layout commits
cf6814b/ca7ea5f on provider base 843a011 in an isolated integration branch. The fixed
native-v2 version 1 backup additionally checks defaulted notes records and the first
revision after a real worker edit. The combined workspace has not yet been tested.

Next: root integration and unfiltered workspace, formatting and progress checks,
then a frozen original fixture/model/prompt plan before any local-model quality
evaluation. Keep prior v1 evidence and all failed/stale checks historical. No
summary, decision/action-item or reference automatic-trigger parity follows from
implementation-only tests.
