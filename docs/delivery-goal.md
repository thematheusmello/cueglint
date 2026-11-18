# Active delivery goal

Status: **active and resumable**. Complete the full Cueglint implementation and
verification scope below. A working slice, merged change, CI pass or model answer
does not complete this goal. Resume from the recorded blocker and next action;
retain earlier failures and continue independent work while a surface is blocked.

The inventory remains **73 required features: 69 desktop and 4 companion** in
[`parity/features.yaml`](../parity/features.yaml). Observation, implementation and
verification are independent states. No matched reference-versus-Cueglint contract
has been promoted by the implementation evidence recorded here. Do not remove a
feature, reduce a denominator or weaken acceptance to close this goal.

The chosen reference remains `cluely-macos-2.1.28`. The installed 2.1.29 drift has a
separate [metadata-only observation](../parity/reports/reference-2.1.29-metadata-001.json).
Later [readable General observations](../parity/reports/reference-general-visible-2.1.29-001.json)
use a supported visible mode and retain measurements separately; the selected
2.1.28 baseline and unknown used-font metrics remain unchanged.
Raw commercial artifacts, recovered source, credentials and personal captures stay
outside this repository. Implementation uses reviewed contracts in `spec/features/`
and measured tokens in `ui/`.

| Part | Remaining work | Done criteria |
|---|---|---|
| 1. Reference, UI and windows | Resolve the selected baseline's unknown fonts, scale and reachable states; complete native layout, input, focus, shortcuts, displays, child windows, motion and capture controls. | Versioned reference observations and independent matched state/interaction comparisons meet declared tolerances on each claimed platform. Original Cueglint fixtures alone cannot close this part. |
| 2. Packaged local session | Run the actual package with selected local models, distinct microphone/system sources, fresh screen context, frozen requests, streaming/cancellation, stop/finalize/save/reopen and offline operation. | A controlled end-to-end session in the distributed executable preserves source/session/request identities, notes and final utterances; independent evaluation verifies results, bounded cleanup and network behavior. File-ASR and CLI-only smoke runs remain narrower evidence. |
| 3. Workflows and integrations | Complete notes/editing, generated outputs, search/history chat, modes/templates, knowledge ingestion/retrieval/deletion, settings, calendar and the observed integration inventory. | Every required workflow has working lifecycle/error behavior, persistence and reviewed acceptance evidence. Highlights must first produce valid outputs, then pass fixed salience and explicit acceptance/append checks; automatic summaries, decisions and action items need their own implementation and evidence. |
| 4. Models and intentional providers | Complete model install/import/remove/recovery, packaged backend selection, local quality and latency evaluation, and explicit BYOK/vendor reliability. | Exact source, model, tokenizer, prompt, configuration and fixture identities accompany fixed/unseen repeated quality measurements and failures. Missing/corrupt models and provider failures recover visibly; cloud use is intentional and never a silent fallback. |
| 5. Privacy, recovery and sustained use | Verify capture controls and recipients, credentials/logs, local network behavior, migrations, deletion, backup/restore, interrupted jobs and sustained meeting resources. | Independent real-runtime checks meet preset privacy, recovery, cancellation, latency, memory and queue bounds while preserving existing data and notes. Synthetic tests and endpoint hashes alone cannot establish hardware or continuous behavior. |
| 6. Distribution and public CI | Produce macOS/Windows releases and an explicit Linux support matrix; verify clean install, permissions, updates, rollback and portable data. | Each supported release artifact has identity, provenance and clean-install/update acceptance; required tests, formatting, registry and progress checks pass at its source. Public CI retains artifacts and failures without private fixtures or release/account secrets. |
| 7. Companion and self-hosted surfaces | Implement the required mobile/browser companions, open-source sharing service and self-hosted synchronization, including identity, conflicts, deletion and portability. | All four companion inventory contracts have independently evaluated behavior and deployment/client evidence. No missing companion may be treated as whole-product completion or depend on a private commercial backend. |
| 8. Contract and acceptance closure | Reconcile all 73 contracts, transitive dependencies and the twelve acceptance families; rerun stale affected evidence after source/reference/model/prompt/fixture/configuration changes. | Every required contract has complete implementation and fresh, independent acceptance evidence. Missing, blocked, failed and stale checks are resolved with retained history; public status and the registry agree with the resulting evidence. |

The native desktop remains one Rust eframe/egui stack on wgpu. Its default runtime
requires no Node, Python, Electron, webview or hosted inference. The core remains
usable from the CLI. Background work stays bounded and cancellable; inference and
capture I/O stay outside UI drawing and audio callbacks.

Core operation is free and local without a mandatory account. Support local/open
models and intentional BYOK OpenAI, Anthropic, Gemini, Perplexity, OpenRouter and
OpenAI-compatible endpoints, with explicit model selection, scoped OS credential
storage and real integration evidence for each claimed route. Optional external
service costs remain visible; missing models never trigger automatic cloud use.

These eight parts crosscut the existing six delivery gates; they do not create or
renumber issues or milestones. Existing tracking stays at issues
[1](https://github.com/thematheusmello/cueglint/issues/1),
[2](https://github.com/thematheusmello/cueglint/issues/2),
[3](https://github.com/thematheusmello/cueglint/issues/3),
[4](https://github.com/thematheusmello/cueglint/issues/4),
[5](https://github.com/thematheusmello/cueglint/issues/5) and
[6](https://github.com/thematheusmello/cueglint/issues/6). Each feature's existing
`gate` and `issue` fields remain authoritative.

| Workstream part | Existing gates |
|---|---|
| 1. Reference, UI and windows | 1 and 4 |
| 2. Packaged local session | 2 and 4 |
| 3. Workflows and integrations | 2 and 3 |
| 4. Models and intentional providers | 2, 3 and 4 |
| 5. Privacy, recovery and sustained use | 2, 3 and 4 |
| 6. Distribution and public CI | 3 and 5 |
| 7. Companion and self-hosted surfaces | 5 |
| 8. Contract and acceptance closure | 6, covering all preceding gates |

The twelve acceptance families retain their separate reference runner, Cueglint
runner and evaluator contracts in [`parity/scenarios/`](../parity/scenarios/):

| Family | Closure evidence |
|---|---|
| `first-use` | Clean OS/app profile and install, model setup, permissions, denied/missing prerequisites and recovery; an authorized reference account only where needed. |
| `audio-correctness` | Distinct controlled live sources, formats/devices, partial/final stability, attribution, accuracy, delay, drift, echo/overlap and stop behavior. |
| `screen-freshness` | Correct selected display/window, capture identity/time/scale and fresh screen-derived answers across changes. |
| `cancellation-isolation` | Session/request/context identities and adversarial late-output isolation after stop, switch, replacement and cancellation. |
| `knowledge-history` | Ingestion/retrieval, edit/search/chat, persistence, invalid inputs, deletion and cross-session isolation. |
| `window-keyboard` | Native input/IME, focus, shortcuts, drag/resize, display transitions and child lifecycle. |
| `capture-compatibility` | Recipient-side results for named sharing/capture clients and declared OS configurations. |
| `offline` | Complete packaged local workflow with outbound denial and actual traffic observation; no implicit provider fallback. |
| `recovery` | Interrupted capture/inference/storage/update and migration/restore failures preserve existing user data and recover explicitly. |
| `sustained-operation` | Representative long sessions meet preset latency, memory, queue, CPU/GPU and power/resource limits. |
| `distribution-portability` | Identified release artifacts, clean install/update/uninstall and portable data on every claimed platform. |
| `visual-state-coverage` | Matched reachable empty/loading/error/selected/streaming states with reviewed typography, geometry, scale and motion evidence. |

The table summarizes scope; the versioned scenario files and feature acceptance
fields remain authoritative. A family remains open until its full contract has
fresh evidence, even when a narrower implementation check passes.

The latest preserved public checkpoints are:

- [PR19 hosted checks and independent artifact review](../parity/reports/pr19-hosted-ci-artifact-001.json)
  bind exact head `efc542be`, CI merge `4aaf778d` and later main merge `2d859e3`
  to tree `629d603e`. Rust passes302/304/304 on Ubuntu/macOS/Windows with3/4/4
  explicit ignores; thirteen native frames and bounded child startup pass.
  Five earlier concurrency cancellations remain separate. These checks predate
  the new microphone/display selectors.
- [Integrated capture-input checks](../parity/reports/capture-inputs-integrated-workspace-001.json)
  pass312 tests at `e4a46779`, following250 targeted checks. Explicit selectors,
  bounded catalog refresh, retained missing selections, legacy recovery and shared
  macOS display targeting are implemented. Hardware capture and platform acceptance
  remain open.
- [One actual native local text workflow](../parity/reports/native-local-session-001.json)
  returns its original control code with verified Qwen1.5/CandleCPU. Its manual
  note survives End, quit, export and history reopen. The independent review
  checks source/model bytes and retained UI/data. Audio/screen, blocked-network,
  broad quality and first-use acceptance remain separate; the specific macOS
  authorization refresh is awaiting user authentication.
- [PR16 hosted checks and independent artifact review](../parity/reports/pr16-hosted-ci-artifact-001.json)
  bind head `533d528`, CI merge `d802fa3` and main merge `37da11d` to tree `3bca981`.
  All three Rust jobs, thirteen original primary frames and native child startup
  passed. The original zero-step runner-capacity cancellation remains separate.
- [PR17 hosted checks and independent artifact review](../parity/reports/pr17-hosted-ci-artifact-001.json)
  bind head `d28b390`, CI merge `a924ca8` and main merge `76a2d81` to tree `1a9b9d5`.
  Rust totals are Ubuntu 275/3 ignored, macOS 277/4 and Windows 275/4; thirteen
  original primary frames and native child startup passed. Windows loopback
  helper tests opened no audio device. Later microphone changes need new checks.
- [Actual highlights quality run 001](../parity/reports/highlights-quality-actual-001.json)
  failed both original cases at source `d28b390`: each response selected more than
  eight utterances. Salience is `not_scored`; acceptance/append is `not_run`.
  There was one generation per case, no retry/repair/fallback and no stored
  candidate or generated notes. [Independent audit](../parity/reports/independent-highlights-quality-actual-001.json)
  preserves this result and the intact source/user data.
- [Fresh 2.1.29 reference metadata](../parity/reports/reference-2.1.29-metadata-001.json)
  records current settings bounds and display scale, but its screenshot is wholly
  transparent and AX has no candidates. No text or typography was measured, and
  the selected 2.1.28 baseline's scale uncertainty remains.
- [Local reliability workspace check](../parity/reports/local-reliability-workspace-001.json)
  at source `5845d00` passed 299 tests with four ignores and formatting; the 399
  compared source files were unchanged. This is implementation-only evidence.
  It adds no new model result, managed Metal loading, Windows microphone runtime,
  native capture, integrated-source progress check or reference-parity acceptance.
- [Actual highlights quality run 002](../parity/reports/highlights-quality-actual-002.json)
  at source `5845d00` produces valid source-bound selections but fails both unchanged
  salience gates. Precision is0.50 and recall0.667; acceptance/append stays `not_run`.
  The original failure and the new failed semantic result remain separate evidence.
- [PR18 hosted checks and independent artifact review](../parity/reports/pr18-hosted-ci-artifact-001.json)
  bind head `bcf6db2`, CI merge `7a788a9` and main merge `d78732f` to tree `6ffd902`.
  Ubuntu passed297 tests, macOS299 and Windows299, with3/4/4 ignores. Thirteen
  original native frames and bounded child startup passed. Native devices and
  model quality remain separate acceptance work.
- [Actual highlights quality run003](../parity/reports/highlights-quality-actual-003.json)
  at `9484829` still fails both fixed salience cases after generic promptv2.
  Source support is exact, but precision0.50/recall0.667 remains below0.80.
  [Independent review](../parity/reports/independent-highlights-quality-actual-003.json)
  retains the failure; unchanged cases/model must not be retried to obtain a pass.
- [The menu-fix workspace check](../parity/reports/provider-menu-workspace-001.json)
  at `136a7c7` passes304 tests with four ignores. A separate
  [native macOS flow](../parity/reports/native-provider-ui-004.json) exercises
  Keychain save/delete, synthetic listing/testing, explicit provider selection,
  one floating-assistant question and normal cleanup. The preceding incomplete
  fixture and AX-focus failures remain visible; vendor, inference, capture and
  matched-reference acceptance remain open.

Resume by inspecting the current source and this checkpoint's exact identities.
Select one remaining contract, state its intended user-visible result and its
existing blocker, and make the smallest complete change through its callers,
tests, fixtures and documentation. Give parallel work distinct file ownership and
isolated worktrees. Preserve failures and independent reviews before integration.

For highlights, keep the failed protocol and cases fixed. Review a proposed
decoder/prompt/model change independently before a new separately identified run;
do not repair old outputs, assign invented salience scores or replay the unchanged
failure. A bounded ordering/decoder/runtime diagnostic is preparation, not a
quality pass. For reference typography, build on the reviewed readable General
handoff and obtain missing metrics under a new identity; do not repeat the same
transparent/AX-empty state or launch the old build against the updated user profile.

Before publication, run the affected tests, `cargo fmt --check`,
`cargo xtask verify-registry` and `cargo xtask progress --check`; regenerate public
progress as needed. Update `STATUS.md` with the exact integrated source, retained
evidence, unresolved blockers and next executable continuation steps. Continue the
goal until all eight parts and all twelve acceptance families meet their done
criteria; a budget, checkpoint or blocked surface is not completion.
