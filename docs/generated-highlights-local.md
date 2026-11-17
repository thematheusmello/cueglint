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

The dedicated prompt revision is `cueglint-qwen-highlights-v2`. It instructs the
selector to read every supplied utterance before choosing by importance,
regardless of position: explicit decisions, concrete next actions, deadlines,
substantive constraints or risks, and unresolved questions. It asks the selector
to skip greetings, filler and repeated information and choose fewer or no
indices when little or nothing is useful. Only the chosen indices are returned
in input order. Tentative statements must stay tentative and no commitments or
identities may be inferred from capture sources.

The prompt quotes the transcript as JSON and Unicode-escapes `<`, so untrusted
text cannot introduce Qwen role delimiters while JSON decoding preserves the
original text. Utterance data is never instructions. The only
accepted response is a complete JSON array of up to eight distinct, ascending
integer indices belonging to that chunk. Empty arrays are allowed. Strings,
objects, prose, code fences, duplicate/out-of-chunk indices, malformed JSON and
responses exceeding 1,024 bytes fail. Token-limit/cancelled finishes fail even
when their text happens to be valid JSON. There is no repair request or retry.

Notes-only decoder policy `cueglint-highlights-json-mask-v1` now constrains the
actual F32 logits before the existing deterministic ArgMax sampler, after the
unchanged repeat penalty (1.1 over the latest 64 generated tokens). Candle 0.11.1
does not invoke `sample_f`'s callback for ArgMax, so that API cannot enforce this
mask. Generic chat supplies no constraint and retains its existing token choices,
streaming, prompt and budget. The decoder policy and model profile are unchanged
by the v2 highlight prompt.

The concrete state machine accepts canonical minified JSON only: brackets,
commas and decimal digits. It tracks the chunk's inclusive minimum/maximum
indices, last committed index, count, numeric prefix, byte count and closed state.
Every byte of a token must leave a viable prefix. For example, a `1` prefix in
the range 2 through 11 can become 10 or 11, but cannot close as 1. Leading zeros,
duplicate/descending/out-of-range indices, a ninth index and trailing text are
masked before sampling. Only actual recognized Qwen end tokens are admitted once
the closing bracket has been generated; `]` never fabricates `end_of_turn`.
The original strict parser still validates the complete result. It does not drop,
sort, repair, truncate or retry model output. The 128-token and 1,024-byte limits
remain in force; a complete allowed list needs at most 41 ASCII characters for
eight indices in 0 through 9999, leaving room for the actual end token even with
one-character tokens.

Compatibility is deliberately narrow: tokenizers 0.22.2's exact `ByteLevel`
decoder, dense IDs, at most 262,144 vocabulary entries and non-added standalone
tokens for all 13 grammar characters. Other decoder variants (including decoder
sequences), sparse/oversized vocabularies or missing character tokens fail before
inference. ByteLevel maps the allowed ASCII characters identically and joins
them without cleanup. Added tokens, non-ASCII byte fragments, spaces, unrelated
tokens and padded/unknown model-logit IDs cannot be selected. The generator
lazily caches at most 16,384 candidate pieces and 1 MiB of their bytes for its
verified tokenizer; it shares this small cache across chunks without copying
model weights. Preparation and candidate scans check cancellation every 64
entries, with additional boundary checks. Token pieces are at most 1,024 bytes;
each mask scans only this bounded cache and at most 262,144 logits. Unsupported
shapes, no legal token and nonfinite legal-token logits fail the whole job.
All preparation/masking stays on the existing bounded inference actor or CLI
worker, outside UI drawing and audio callbacks; queues and job ownership are
unchanged.

The returned `NotesCompletion::provenance` is `NotesProvenance::Local`, with one
inference record per chunk: exact prompt/template/model/tokenizer identities,
token counts and timing, `end_of_turn`, zero omissions and no screen truncation.
New constrained runs also carry `decoder_policy_revision`; all chunk records must
agree. This is an additive optional v2-record field, omitted for legacy records
and generic chat. This reader restores old records unchanged and preserves the
field in JSON exports/backups; older strict readers may reject records containing
it. No storage/schema version was changed. Invalid raw policy values and mixed
chunk policies are rejected before replacing user data.
These are execution metadata, not a passed quality evaluation. No partial
completion is returned if any chunk fails or cancellation arrives.

For a future actual-model evaluation, record the full source commit and source
file hashes, decoder-policy revision, exact prompt revision, Candle/tokenizers
versions, model/tokenizer hashes, exact whole-chunk coverage, 128-token/1,024-byte
and eight-index limits, ArgMax/repeat settings and the unchanged evaluation
fixture/criteria identities. The policy revision identifies this notes-specific
decoder configuration; it is not a hash of the entire engine. Implementation
tests do not establish salience quality or reference parity. Earlier rejected
over-eight-index pilot outputs remain failures; their result must not be upgraded
based on this source change.

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

The new original weight-free tests cover numeric-prefix viability, token-boundary
independence, merged-token masking, eight-index limits, actual EOS gating,
unsupported tokenizers, byte/logit bounds, cancellation, unchanged chat sampling
and optional provenance persistence/atomic rejection. These tests have been
written but remain **UNRUN** in this isolated implementation handoff. Only direct
rustfmt and source/whitespace checks are authorized in this worktree; root owns
the subsequent Cargo and actual-model windows.

The v2 prompt adds one original weight-free policy check for the exact selection,
trust and response instructions. It verifies the rendered system contract only;
it does not select benchmark answers or establish improved model salience. This
check is **UNRUN** in the isolated implementation handoff. The longer prompt is
encoded by the unchanged planner, so exact chunk boundaries may change while
the complete-coverage and token limits remain enforced. Earlier quality results
remain attached to their original prompt identities and are not upgraded.

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
