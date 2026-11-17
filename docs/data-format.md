# Portable data format

Session JSON exports use `format: "cueglint-session"` and `version: 1`. Session fields
remain at the top level for simple independent consumers. Earlier development exports
without those two fields were unversioned; they are not backup files and are not accepted
by backup restore. Markdown exports identify version 1 in their header.

The JSON includes stable session/segment/request identities, title, Unix millisecond dates,
session state, mode, finalized transcript, user notes, separate derived summary and assistant
answers. Partial hypotheses are transient and are excluded. Captured transcript segments
carry microphone/system source and optional ASR identity, language and session-relative
start/end nanoseconds; source does not identify a person. Requests retain the frozen context
and revision they used. Answer metadata identifies actual model/backend/token settings.

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
requests made from a restored session use version 1.

Current answer metadata includes `prompt_sha256`, the SHA-256 of the exact rendered UTF-8
prompt passed to the pinned tokenizer, and `template_revision`. Both fields are absent or
null for older answers; this means unknown provenance, not a reconstructed identity.
`LocalGenerator::render_prompt` rebuilds the private prompt without model inference from
the same immutable request, generation options, tokenizer and template source. The template
revision, model/tokenizer identities and token settings must match to reproduce its hash.
Explicit document deletion redacts retained source quotations; deliberately removed context
cannot subsequently reproduce the original prompt.

Backups use `format: "cueglint-backup"` and `version: 1`, with sessions, modes, knowledge and
settings. Restore validates size, types, IDs, grounding and references before a transaction
changes existing data. Replacing a nonempty database requires explicit `--replace`. Exports
are written atomically with private file permissions and preserve existing files by default.

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
assert session["format"] == "cueglint-session" and session["version"] == 1
print(session["title"])
print("\n".join(segment["text"] for segment in session["transcript"]))
```

This consumer example is not a Cueglint runtime requirement. Exported source locations,
transcript text, screen text retained in request context and knowledge quotations may be
private; publication remains an explicit user action. Deletion cannot revoke independent
copies already exported or stored in backups.
