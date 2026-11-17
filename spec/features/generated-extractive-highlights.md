# Original Cueglint extractive highlights

This is an implementation contract for an original partial capability. It does
not establish the reference app's automatic notes trigger or output quality.

- Only an explicit action on a stopped, finalized, nonempty session starts local
  extraction. Hosted answer selection never changes this local-only path.
- Prepare and run share the existing bounded inference actor and frozen session,
  source digest, operation, local model profile/root, job and cancellation token.
  No Store writes or model work occurs while drawing.
- Complete ordered whole-transcript coverage is planned before a job begins.
  Output contains strict utterance indices; stored text is the exact original
  utterance with source byte spans. Any chunk failure rejects the whole result.
- Local highlight decoding masks each complete token piece against a bounded
  minified-JSON state machine before sampling: at most eight distinct ascending
  indices from that chunk, no leading zeros or trailing data, and actual end-of-turn
  only after the array closes. The existing parser remains a separate check.
  Generic chat has no such mask. Unsupported tokenizer decoders fail closed.
- A ready draft is separate from human notes. Accept, Discard, Copy and Append
  require deliberate actions; no title, summary or commitment is generated.
- Append drains manual autosaves before capturing the acknowledged notes revision,
  then locks the editor until its exact typed ACK. A conflict preserves notes,
  requires explicit review and never retries automatically. Derived updates cannot
  acknowledge manual saves or replace dirty edits.
- Cancellation, session/model/root changes and quit reject stale results. Quit
  waits for the current job's terminal actor event. Imported/process-abandoned
  pending jobs retire through Core recovery. Ready/accepted records are preserved.
- v1 data remains readable. New v2 backups/exports include strict source-bound
  records and coexist with frozen provider configuration/hosted provenance.

Verification uses original controlled fixtures and weight-free protocol/egui
checks. Local model salience quality, native window interaction and reference
notes parity require separate evidence.
