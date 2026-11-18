# Independent integration review proposal: explicit local transcription language

What this change does: It saves an explicit input-language choice shared by the native settings row and CLI, and adds an optional pinned multilingual Whisper profile. The loaded worker owns the selected language and records it in new audio provenance while legacy English behavior remains intact.

Verdict: the two repairs and integrated source are acceptable for the bounded implementation increment. No open source defect was identified. Root's recorded full workspace run passed; final metadata, formatting and worktree-correct progress checks remain Root's publication gate. This does not complete the eight workstreams, 73 contracts or twelve acceptance families.

The reviewed and tested subject is staged tree `e80c9c6212112850b86d226bc68c08d57a9451aa` based on `7d1c279ccaa78acd8ffbbf3dc06de4e99c9387fe`. It is a dirty staged feature tree, not a clean-HEAD test claim. The review evidence records exact blob IDs, lengths and SHA-256 values for all 19 feature paths. All 19 working/index blobs still match that tested tree at this review's freeze.

Source review followed AGENTS.md and Ponytail full, assuming one local desktop/CLI user with the existing bounded capture jobs and queues. This reviewer read source, reviewed contracts, authorized public metadata and Root's original validation records. The reviewer ran no Cargo command, model, native UI, capture, network, credential or publishing operation and changed no source, index or registry.

The original unbuilt review did not catch the compiler closure error or invalid session fixture. Their actual failures remain retained. The reviewed repairs are minimal:

- The accessibility widget-info callback can run more than once. Cloning its small selected-label String avoids moving it out of an `Fn` closure. The picker, accessibility value, settings command and capture locks are preserved.
- The metadata test now creates and stops the unrelated session before creating the capture-owned session. The production single-active-session guard stays unchanged. Every stale-session, transcript-text, worker-language, profile, backend and saved-settings assertion remains.

The formatted original language source matches the integrated source across fifteen feature files. The only differences in the other two source files are those exact repairs. All five older catalog profile objects, their order and defaults remain unchanged; the optional sixth profile and public-source record retain their exact pins. English constructors and the two-token English prefix, the multilingual TRANSCRIBE prefix, strict input-language validation, atomic settings/restore behavior, existing notes/history and worker/session ownership still follow the reviewed contracts. Existing test assertions were retained, with the expected profile inventory count increasing from five to six.

There is no overlap between this increment's 19 paths and the 34 canonical paths changed since its original source base. The planned later update from base `7d1c279` to main `e9808eeebcf1613c3094fffc9769481935c11e1a` changes exactly seven listed metadata paths, disjoint from the feature paths. That plan preserves the tested feature blobs. This review does not attest to later unreviewed changes. The updated AGENTS.md requirement for a dedicated CARGO_TARGET_DIR and verified embedded worktree root for cargo xtask was read; Root must follow it for final progress generation/checking.

Recorded runs, independently checked against original logs:

| Attempt | Actual outcome |
| --- | --- |
| 001 | Formatting passed. Workspace compilation failed at E0507 before Rust tests executed. The candidate's retained compile-red report matches the original terminal and log hashes. |
| 002 | Formatting passed. The first executed test harness reported 135 passed, 1 failed and 1 ignored. The failed language metadata fixture attempted a second active session. The retained fixture-red report matches the original terminal and log hashes. No full-workspace pass follows. |
| 003 | Formatting and the repaired exact test passed: 1 passed, 0 failed, 0 ignored, 136 filtered. A later workspace compilation hit the disk floor. It has no workspace result record or full test summary. |
| 004 | Formatting passed. Workspace compilation again hit the disk floor. It has no workspace result record or full test summary. |
| 005 | Formatting passed. `cargo test --release --offline --locked --workspace` exited 0. The terminal guard passed with no errors. All 34 standard summary rows total 386 passed, 0 failed, 4 ignored, 0 measured and 0 filtered. Independently counted named-result lines match 386 successes and four ignores. |

All ten new language tests occur exactly once in attempt 005 and passed. Their exact names and all other named results appear in the companion evidence. Two renamed existing ASR selection tests also retain their prior assertions. The source receipts before/after each run are identical; each staged tree is stable; recorded protected binary identities match; completed owned process groups are reaped and absent. The same guard implementation hash is retained across all five attempts. The two cache recovery records were checked for receipt consistency and matching protected records, not audited as independent deletion inventories.

The PDF result is separate: the custom `pdf_supervision` executable printed its successful completion after its source assertions for actual parser dispatch, six controlled IPC failures, cancellation, the ten-second deadline, child reaping and stored-data preservation. It has no standard Rust test summary and is not added to the 386 count. The CLI PDF suite's five named tests and two PDF text-unit tests also passed.

The four ignored checks remain ignored:

- `system_fonts::tests::explicit_native_font_probe`: explicit native OS font probe.
- `original_clean_english_fixture_measures_actual_warm_asr`: requires an explicitly supplied verified Whisper profile.
- `supplied_standard_whisper_oracle_matches_frozen_numerical_tolerances`: requires explicit external WAV/oracle/hash inputs.
- `real_local_generation_streams_records_identity_and_honors_cancellation`: requires an explicitly supplied verified Qwen profile.

The feature registry was unchanged during testing relative to the tested base, with SHA-256 `5f57c251abc9b7bc3bf49d13e03cb091d52cd6037bf679fe475e085ba36ab1c7`. This differs from the older source review's registry identity because independent canonical work intervened. Later implementation-state metadata must preserve observation/verification distinctions and every missing, blocked, failed and stale check. Ninety-nine accepted language tokens establish no ninety-nine-language quality coverage.

Root's next checks are to preserve the nineteen tested feature blobs through the metadata-only update, retain attempts 001–004 and the exact successful attempt-005 identities, update STATUS.md with actual results and continuation, and finish formatting and worktree-correct registry/progress checks. If those pass, make one atomic logical commit and push its tracked branch immediately; resolve a failed push before another commit. A full workspace rerun is not needed solely for disjoint metadata changes; source or build-input changes need an appropriate new check.

Not checked: real model loading/inference, multilingual accuracy, native-device/capture behavior, offline-package/signing or reference parity. Risk: implementation tests prove only their bounded behavior; the full eight-workstream, 73-contract and twelve-family delivery remains active.
