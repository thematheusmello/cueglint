# Local extractive highlights

This increment prepares and runs whole-utterance selection. It does not generate
an abstractive summary, decisions, action items, titles, owners or dates. It does
not establish reference parity or model salience quality. The CLI and desktop expose explicit
generation/review actions. Weight-free protocol and native egui checks cover the integrated desktop source;
native window interaction and actual model extraction remain unverified.

`LocalGenerator::plan_notes(&NotesInput, &AtomicBool)` returns `PreparedNotes`.
`PreparedNotes::plan()` exposes the core coverage plan, while
`prompt_token_counts()` exposes each exact prompt count. Prompts, encoded token
IDs and model/tokenizer identities remain private. Planning performs no model
forward passes or Store mutations. Run it on the same background worker that
owns the generator, outside UI/audio callbacks.

Planning validates the core digest-v1 identity against every ordered, finalized
utterance. It preserves exact text, speaker and capture-source values. It rejects
partial, missing, reordered, duplicate or invalid source IDs, changed digests,
more than 10,000 utterances and more than 128 KiB of transcript text. A tokenizer
configured to truncate is rejected rather than silently dropping input.

The planner greedily groups whole utterances. Every candidate prompt is encoded
with the generator's actual tokenizer; each accepted chunk reserves 128 output
tokens within a 4,096-token total. An oversized single utterance or a plan needing
more than 16 chunks fails before any generation. No chat suffix, 64-segment limit,
screen, reference files, previous answers, user notes, title or mode instructions
enter these prompts. A capacity error leaves the full source untouched.

`LocalGenerator::generate_notes(&PreparedNotes, &AtomicBool, on_progress)` runs
chunks serially through the existing Candle/Qwen token loop. A plan prepared for
another model/tokenizer is rejected. Progress reports `completed_chunks` and
`total_chunks`, first at zero and after each validated chunk; it exposes no draft
prose. Cancellation is checked between planning steps, before/after each chunk,
and after the final progress callback. The shared generation loop retains its
existing cancellation checks between prompt batches and generated tokens.

The dedicated prompt revision is `cueglint-qwen-highlights-v1`. It quotes the
transcript as JSON and Unicode-escapes `<`, so untrusted text cannot introduce
Qwen role delimiters while JSON decoding preserves the original text. The only
accepted response is a complete JSON array of up to eight distinct, ascending
integer indices belonging to that chunk. Empty arrays are allowed. Strings,
objects, prose, code fences, duplicate/out-of-chunk indices, malformed JSON and
responses exceeding 1,024 bytes fail. Token-limit/cancelled finishes fail even
when their text happens to be valid JSON. There is no repair request or retry.

The returned `NotesCompletion::provenance` is `NotesProvenance::Local`, with one
inference record per chunk: exact prompt/template/model/tokenizer identities,
token counts and timing, `end_of_turn`, zero omissions and no screen truncation.
These are execution metadata, not a passed quality evaluation. No partial
completion is returned if any chunk fails or cancellation arrives.

The caller first obtains stopped-session input, prepares all chunks, then calls
`Store::begin_notes(session_id, prepared.plan().clone())`. It retains the returned
job ID while generating. On success it submits the single complete result to
`Store::complete_notes(session_id, job_id, completion)` and respects its stale
result. On cancellation/error it retires that exact job with `cancel_notes` and
`NotesCancelReason::User`/`Failed`. Session resumption, changed sources, deletion,
restore and recovery remain core lifecycle boundaries; the generator never
commits, accepts, discards or applies records itself.

Ready highlights remain separate from user-edited notes. Accept/reject and the
revision-checked, idempotent append are later explicit caller actions. Core
rendering uses the exact original source utterances and byte spans; model text is
never used as a quote or commitment. Legacy summary, title and existing notes are
unchanged.

Weight-free tests use an in-memory tokenizer and a controlled response runner.
They cover 200-utterance full coverage and exact earliest/middle/latest selection,
real encoding counts, pre-generation capacity failures, injected role strings,
Unicode, invalid source/selection identities, truncation, chunk failures, model
changes, cancellation and explicit core draft completion. They do not load
weights, run inference, download files or assess highlight quality. The existing
chat-v3 prompt renderer and decoding policy are retained; existing chat tests
remain required.

## CLI actions

```text
cueglint-cli highlights review SESSION
cueglint-cli --models-dir MODELS highlights generate SESSION
cueglint-cli highlights accept SESSION JOB
cueglint-cli highlights discard SESSION JOB
cueglint-cli highlights append SESSION GENERATED --expected-notes-revision REVISION
```

Review prints the current job, any accepted record, their source freshness, manual
notes and revision as JSON. Generate uses the stored generation profile or an
explicit `--profile`; it never downloads or falls back to another provider/model.
It requires a stopped full transcript and no pending/unreviewed job. It prepares
all chunks before `begin_notes`, then leaves one ready draft after complete
validation. A handled error retires only the matching job. It does not accept or
append automatically. Progress on stderr counts validated chunks, not salience.

Accept and Discard require the stable current job ID. Append requires an accepted
ID and the reviewed notes revision; a conflict prints the typed JSON outcome and
exits unsuccessfully, without retrying. Already-applied records remain no-ops.
`session notes` and `session summary` retain their existing manual semantics.
Historical source snapshots are labelled by review and may only be appended by
that explicit accepted-ID/revision action.

There is no graceful Ctrl-C bridge. Process interruption can leave a pending job;
the next `Store::open` marks that notes job interrupted before the CLI action,
without invoking GUI capture-state recovery or pausing the existing CLI recording
workflow. Regeneration is an explicit new job, never continuation of partial model
output. Weight-free CLI tests seed Supplied drafts and controlled failures; actual
local notes inference and quality evaluation remain unverified.

## Desktop actions

Open a stopped session, select Notes, then Highlights. Generate locally uses the
selected local answer profile even when the answer provider is hosted. The bounded
inference actor prepares the complete source, the Store revalidates and creates
the job, then the same actor generates the strict selections. Progress exposes
chunk counts. Cancel, session navigation, profile/root changes and quit retire
the exact job and reject late results. Quit waits for that job to return.

The review surface shows original quoted utterances, stored provenance and
current/historical source labels. Accept and Discard preserve the manual editor.
Copy is explicit. Append first drains manual autosaves, captures the acknowledged
notes revision, then holds the editor read-only until its typed operation ACK.
A conflict preserves notes and requires explicit review before a new append.
Only a manual Notes ACK clears the saving state; background highlights updates
do not replace a dirty buffer. No generated title or summary is written.
