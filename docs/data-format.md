# Portable data format

Session JSON exports use `format: "cueglint-session"` and `version: 2` by default.
A frozen request with an explicit mode file scope requires `version: 3`. A saved
History chat requires `version: 4`. Session fields
remain at the top level for simple independent consumers. Earlier development exports
without those two fields were unversioned; they are not backup files and are not accepted
by backup restore. Markdown exports identify version 2 in their header.

The JSON includes stable session/segment/request identities, title, Unix millisecond dates,
session state, mode, finalized transcript, user notes, separate editable summary text and assistant
answers. Partial hypotheses are transient and are excluded. Captured transcript segments
carry microphone/system source and optional ASR identity, language and session-relative
start/end nanoseconds; source does not identify a person. Requests retain the frozen context
and revision they used. Answer metadata identifies actual model/backend/token settings.

New captured audio metadata also retains `backend`, `frontend_revision` and
`frontend_config_sha256` together. The hash identifies the canonical preprocessing settings,
including the coefficient-table identity, independently of unchanged model weights.
Older records default all three to null (unknown); they are not retroactively labelled with
the current frontend. Partial identities and malformed hashes fail before transcript writes
or backup replacement. Frozen request context and exports preserve the recorded values.

Settings contain `transcription_language`, a canonical explicit Whisper input-language code.
Missing legacy fields default to `en`; invalid codes fail before settings or backup
replacement. Version 2 and all existing session/note fields are unchanged. The value does
not control response language, translation or UI localization. New audio metadata retains
the loaded worker's selected language, independently of later preferences; existing audio
records are not relabelled. Selection does not claim detected-language or model quality.

New request snapshots have `selection_version: 1`. They retain a contiguous newest suffix
of at most 64 whole finalized transcript segments and 16 KiB of transcript UTF-8 text.
Completed chat context retains at most eight whole turns and 16 KiB of combined question
and answer text. These are separate budgets. Selection never splits a segment, a turn or a
UTF-8 character. If the newest item alone exceeds its budget, that context is empty and
`diagnostics` explains the omission. The full session transcript, answers and human notes
remain stored independently.

Snapshot `omitted_transcript_segments` and `omitted_chat_turns` count original items left
out when the request was frozen. The generator can omit more old items to fit its 4,096-token
input/output window; answer metadata reports the combined original and generator omissions.
Legacy snapshots default to `selection_version: 0` with unknown original selection counts.
They remain readable and are not rewritten or discarded to satisfy the new bounds. New
requests made from a restored session use `selection_version: 1`.

A request with screen context records either `display_id` or `window_id`.
Window records carry a nonzero native window ID for provenance at capture
time, a PNG digest in `sha256`, and a digest of the RGBA bytes in
`pixel_sha256`. Readers of window-source exports must understand
`window_id`. The snapshot also carries the OCR text, capture time, and pixel
dimensions. Original screen pixels stay in memory on the selected-window route.

In version 4, a session with `history_chat: true` is a saved text conversation
whose requests are local-only. A request's `context.past_meetings.excerpts`
array holds the frozen title, date, source meeting ID, field or transcript
segment ID, exact text, and byte range in that field. `revision_sha256`
hashes the frozen UTF-8 title, a zero byte, and the exact UTF-8 excerpt.
Restore checks each cited span against the included source meeting and
validates the revision and local-only fields. A renamed meeting keeps its
frozen citation title. If a cited span is changed or its source is deleted,
affected requests become cancelled with `past_meetings: "withdrawn"`,
and dependent assistant text and excerpts are cleared. Authored notes and
questions remain available.

Modes and their frozen request snapshots can carry `knowledge_document_ids`. The absent field
keeps whole-library retrieval. A list limits fresh retrieval to those document IDs and an empty
list excludes the library. Backups validate that IDs are unique and present in the saved library,
and that retained source quotations fit the frozen scope. Removing a document clears its ID and
source quotations from the database in one transaction. An empty selection stays explicit after
its last attachment is removed. Earlier completed answers and user notes remain historical.

Current local answer metadata includes `prompt_sha256`, the SHA-256 of the exact rendered UTF-8
prompt passed to the pinned tokenizer, and `template_revision`. Both fields are absent or
null for older answers; this means unknown provenance, not a reconstructed identity.
`LocalGenerator::render_prompt` rebuilds the private prompt without model inference from
the same immutable request, generation options, tokenizer and template source. The template
revision, model/tokenizer identities and token settings must match to reproduce its hash.
Explicit document deletion redacts retained source quotations; deliberately removed context
cannot subsequently reproduce the original prompt.

Settings also contain `answer_provider` with `provider`, `model_id` and optional `base_url`.
Each new generation request freezes a `provider` configuration. Missing legacy fields
default to Local; they never enable hosted inference. API keys are not part of either type
and never enter exports or logical backups. Official providers use fixed HTTPS endpoints;
compatible-server credentials are scoped to the complete canonical base URL.

Hosted answers keep separate `hosted_inference` metadata rather than inventing local model
files or hashes. It identifies provider, requested/reported model, endpoint, optional reported
version/request ID/token usage, output limit, elapsed time, finish reason, citations and
prompt/template provenance. Unreported values stay null. Core validation binds this metadata
to the request's frozen provider/model/endpoint before persistence or backup replacement.
Existing local answer metadata and ASR identities retain their original meaning.

In the desktop app, open **Settings → Security → Backup**. For backup,
enter a path for a new JSON file and select **Export backup**. To restore,
enter an existing backup path and select **Review restore**. Check the
reviewed file path and explicitly allow replacement if you want to replace
an existing library and settings. Cueglint saves pending note edits before
either operation. Desktop restore selects the Local answer provider.
Historical requests retain the provider and model they originally used.

The CLI also supports `cueglint-cli restore FILE`, with `--replace` for
an existing library. That command restores the provider preference stored
in the file. API keys stay in the machine's credential store.

Backups use `format: "cueglint-backup"` with sessions, modes, knowledge and settings.
They use version 2 for unscoped data, version 3 for explicit mode file scopes,
and version 4 when any History chat is present. Versions 1 through 3
remain readable within their original data boundaries. Saving or restoring
versioned records raises SQLite `user_version` to their required version in
the same transaction. Older Cueglint builds reject the newer database before
opening it. The guard persists after deleting versioned records. Unscoped
v2 backups can still restore to a fresh older database.
Restore canonicalizes transcript order, then
validates size, types, IDs, grounding and references before a transaction
changes existing data. Replacing a nonempty database requires explicit `--replace`. Exports
are written atomically with private file permissions and preserve existing files by default.

Version 2 adds `notes_revision`, `notes_job` and `generated_notes`. Legacy databases and
version 1 backups default these to revision 0 and absent derived records without changing
human notes, title or summary. An exact byte change to manual notes advances the revision;
an identical save does not. Accepting a ready highlights draft does not edit manual notes.
Only an explicit append of the accepted record with a matching expected notes revision
changes that text. A conflict preserves both records, and repeating a previously applied
record is a no-op even with an old expected revision.

Highlight plans freeze exact ordered transcript IDs/text/speaker/capture source under digest
version 1. Their structural chunks cover all input once, at most 128 KiB and 16 chunks; core
coverage alone makes no tokenizer-fit claim. The local planner separately reserves 128
output tokens within each actual 4,096-token prompt/output window. Whole-utterance source
text and UTF-8 byte spans are checked during completion and restore; model prose is never
substituted for a quote. Supplied selections explicitly make no model claim. Local selections
retain one identified, `end_of_turn`, unomitted inference record per chunk, without claiming
summary quality or fabricated decisions, owners or dates.

Ready/accepted records can remain historical after a session resumes; source freshness is
reported separately. Pending jobs are cancelled on resume or restore. `Store::open` also
interrupts persisted pending notes jobs before returning, including CLI opens; the CLI does
not use the GUI's separate capture-state recovery or pause a recording session on every
command. CLI Ctrl-C currently terminates the process; later notes-job recovery is not
graceful signal cancellation. Session/inference/notes records reject unknown fields, and
malformed version 2 notes fail before backup replacement. See
[local highlights](generated-highlights-local.md) for the CLI actions and remaining limits.

Settings contain a sparse `shortcut_bindings` map keyed by stable action ID. An absent key
uses the reviewed platform default; a JSON `null` disables that action. A string overrides
it with canonical tokens, for example `CommandOrControl+Shift+Backslash`. The core rejects
unknown actions, unsupported keys, modifier-only chords, repeated or misordered modifiers,
and combinations without a command/control modifier before changing stored preferences or
restoring a backup. Older backups without this map retain platform defaults. Registration
and OS conflicts are separate from persistence; a stored binding does not prove it works
while another application has focus.

An independent session consumer needs only a JSON parser, for example:

```python
import json
with open("session.json", encoding="utf-8") as stream:
    session = json.load(stream)
assert session["format"] == "cueglint-session" and session["version"] in (2, 3, 4)
print(session["title"])
print("\n".join(segment["text"] for segment in session["transcript"]))
```

This consumer example is not a Cueglint runtime requirement. Exported source locations,
transcript text, screen text retained in request context and knowledge quotations may be
private; publication remains an explicit user action. Deletion cannot revoke independent
copies already exported or stored in backups.
