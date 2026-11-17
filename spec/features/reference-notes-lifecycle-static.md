# Notes lifecycle and mode-template addendum

Static review date: **2026-11-17 UTC**. Subject: shipped Cluely macOS 2.1.28,
ASAR SHA-256
`aa2434cbaba31a0fb1ed189db8d02a9f1c9d84e9e098a02984f0dc306cabcd88`.
The installed archive still matched the frozen reference. This pass performed
no application launch, GUI action, capture, service request or account change.

The statements below are **analyst inferences from inert client syntax**. They
are not newly observed execution. The runtime can load a hosted renderer whose
build remains unpinned; packaged syntax alone does not prove current visibility,
server behavior or entitlement enforcement.

## Meeting completion and notes availability

The inspected end-session path intends to keep a meeting paused while finishing
transcription and saving its ready transcript entries. Successful completion
leaves the live meeting and opens the library/dashboard. Its failure presentation
retains the paused meeting when applicable and offers a retry action. These are
inferred client transitions, not tested failure or recovery guarantees. [E1]

The notes view distinguishes live, analyzing and finished records. While a record
is analyzing, the client separately waits for title availability and full
completion and then refreshes record/list data. These separate milestones do not
establish their timing or arrival order. A session-data loading state is also
present. [E1]

**The server summary-generation trigger remains unknown.** End-of-meeting is the
identified client lifecycle boundary, but the reviewed client does not establish
the server's generation conditions, model, prompt, silence policy, processing
duration or behavior after a second end/resume cycle. Waiting for analysis must
not be described as proof of a particular summary-generation implementation.

## User edits and regeneration

Title and summary edits use a record-saving path independent of chat generation.
The summary editor intends to commit changed text when focus leaves the editor.
It has a saving presentation, a save-failure notice and readback handling. It
also avoids replacing editor content from refreshed record data while the editor
is focused. Concurrency, crash recovery, conflict resolution and durable save
success were not executed or verified. [E1]

The inspected summary surface contains an editable notes area and summary-copy
action; transcript display/copy and session resumption are separate controls.
No per-note template selector or note-regeneration action was established in
this surface. Regeneration references found in the examined chat module belong
to assistant-answer regeneration. They do not establish regenerated meeting
notes. This is a bounded finding, not proof such a feature is absent everywhere.
[E1]

## Templates and reference-file attachments

The Settings template catalog supplies assistant-response instructions. It
distinguishes a preview from the active mode and includes a new-mode action,
activation/pending/active presentations, an editable name and meeting-context
area, and a mode deletion action. A selected template can become a saved mode;
preview selection is distinct from activation. [E2]

Reference files are associated with saved modes in the inspected editor. The
packaged controls include adding/removing files, upload progress, upload
cancellation/retry, processing and processing-check failure presentations.
The displayed file guidance includes a per-file size limit. These controls do
not establish broad file compatibility, server deletion or retrieval quality.
[E2]

**No note-format template contract was established.** The identified templates
and files guide the assistant's conversation mode. Whether that active mode also
affects a generated summary, whether a meeting retains its original mode, and
whether templates/files can be attached directly to an individual saved note
remain unknown. Demo labels and generic chat helpers were not promoted into
production notes capabilities.

## Evidence and coverage

- **E1:** REA application Evidence
  `ev_b220771936f4b903fd9df82c0a30207a4fd4f1d8faa33dd9c1ca4ecdf1d1dca6`.
  The two selected lifecycle/editor modules were parsed successfully; semantic
  coverage is partial. Focused module views:
  `ev_a242b4acf1af90435ef28b7c8287b700f3ccd0a2ff3a443b471c3b3d16be1f4b`
  and `ev_eb4747919cc506fb319c753f009fffae73a8178a393a5d56c2656345dd88f665`.
- **E2:** REA application Evidence
  `ev_cee83d32e5d2a00e54b2253598b58988f2d8c8f697d3dcc5b353abde315edc4a`.
  The selected Settings module was parsed successfully; semantic coverage is
  partial. Source-module identity view:
  `ev_02afd681723d24c4348d9bb0bb9a3e61b0240f30e17483bb494df27a123af97c`.

Both analyses selected exact packaged sources and left other imports unexamined.
Alias/node/unknown ceilings limit relationship coverage. Four previously known
unpacked native-addon integrity contradictions remain recorded as untrusted;
the package was not modified. Small private anchor ranges, hashes and selected
metadata support the manual review. No recovered source, pseudocode, API calls
or commercial assets are included here. No registry or parity status is changed
by this addendum.
