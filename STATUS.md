# Cueglint continuation record

The goal remains active: complete the pinned native desktop experience and the separately
inventoried companion/self-hosted tracks. No desktop release acceptance or product parity
is claimed. The initial workspace was empty; existing user work/history was preserved.

Public repository: https://github.com/thematheusmello/cueglint.
Earlier foundation main: `39894179cea55def69eac37b30f4ca1d29ecb643`, merged through
[reviewed PR #7](https://github.com/thematheusmello/cueglint/pull/7).
[Three-platform CI passed](https://github.com/thematheusmello/cueglint/actions/runs/44011960435)
at its exact matching source tree, `fd53c3a09ebff827c56ede0774c1ced9f87c58ec`.
The earlier Windows hard-link guard and generated-file failures remain in the reports.
[Pages deployment](https://github.com/thematheusmello/cueglint/actions/runs/44014204456)
succeeded; the [public dashboard](https://thematheusmello.github.io/cueglint/) returned HTTP200.

The published model increment integrates the licensed-fixture correction
from [PR #9](https://github.com/thematheusmello/cueglint/pull/9) at
`a722c56463e092dbeccbdc37f6edf34ba2a2a42b` now merged into main as `4f152790378233208ffe7f37604896c6f38b44fa` after
[all platform CI checks passed](https://github.com/thematheusmello/cueglint/actions/runs/44018968905).
The model increment [PR #10](https://github.com/thematheusmello/cueglint/pull/10) merged as
`eb98aa08bee9866dd2d00bcc720b784131525e82` after its final source
`d693c11274207acf18f4e04ac965ea2038fdaa76` passed
[three-platform CI](https://github.com/thematheusmello/cueglint/actions/runs/44022456804). Pinned
local-folder import/removal is wired through Settings and CLI, with hash/license retention,
unknown-file protection and a shared stable lock. File operations exclude audio/answers;
failed attempts invalidate readiness and reset cached inference before completion. Questions
survive setup, queue and screen-preparation errors. Graceful quit waits for import/removal
cleanup; cancelled downloads retain resumable partial files. A forced kill can leave an
orphan import stage which removal deliberately refuses; automated recovery remains open.
The earlier system-voice waveform is absent from this current tree. Historical Git objects
still retain it; no history rewrite is authorized or performed. V2 media has separate
GPL-3.0-or-later terms; wrapper/application code remains MIT OR Apache-2.0.

The coordinated macOS workspace run passed **86 implementation tests**, with two
configured-weight tests excluded from the normal suite. Formatting/diff checks passed.
The independent model-management source review found three lifecycle defects; their fixes
and regressions are included. A real base.en import → file transcription → removal cycle
passed under process network denial on a fresh disposable model root; original installed
models/source files were unchanged. That report retains its pre-review-fix source snapshot.
Native model controls, cache-memory release and Windows/Linux file behavior need runtime
verification. These are implementation checks, not desktop release acceptance.

Current increment: [draft PR #11](https://github.com/thematheusmello/cueglint/pull/11),
`feat/global-shortcuts`, published at `cdf38aff8100060aa2ce269e053a3f75a58d9098`.
Three-platform CI is pending; no green result for this increment is claimed.
Sparse shortcut overrides, disable/reset and
canonical command/control-key validation persist through the core, CLI and backups.
Legacy backups retain defaults; malformed chords and unknown actions fail before writes.
The core/CLI package run passed 29 implementation tests. The final unified workspace run
passed **110 tests including five doctests**, with two configured-weight tests ignored.
Native registration paths, conflict/rebind/retry controls, exact modifiers/repeats and the
hidden-root wake/close guards are implemented in source. Independent review found and
rechecked four lifecycle/identity defects. Mac unregister failure stays restart-required;
fresh native IDs reject obsolete callbacks. The flat shortcut table follows retained
screenshot measurements. No OS-level hotkey, current native visual or AX pass is claimed.
Two failed headless test-harness attempts and the resource-blocked isolated vendor build
remain in separate reports alongside the passing run. Formatting and owned diff checks pass.

Working native/core capabilities include compact/expanded assistant windows, Settings,
sessions/history, edited notes, modes, text knowledge, copying/versioned exports,
transactional backup/restore and CLI access. Real local generation streams and cancels.
Qwen 1.5B passed narrow original text criteria; earlier model/protocol failures remain.
The old Samantha waveform is retired from the active public tree; its ASR scores are
historical. The first licensed v2 formant run failed accuracy at WER 0.50 versus 0.10,
while warm RTF 0.489 passed. A single independent development-only CPU path using the
same pinned weights and waveform also failed at WER 0.30. Both diagnostic setup failures
and effective decoding configuration are retained. Frontend and neural kernels differ
jointly; no root cause or speed advantage is established. No recording or threshold
changes followed either result.
Earlier native typing/copy/save/reopen
checks and screenshots attest their recorded binary, not every later source change.

Request snapshots bound whole recent transcript/chat separately from full history, retaining
omission diagnostics and exact prompt hashes/template identity. Long-history, legacy-backup
and revision-overflow regressions pass. The licensed eframe child AccessKit patch remains
pending native AX verification. Historical network-denied CLI runs cover file ASR, grounded
answers, notes, save/reopen, exports, backup/restore and deletion; they do not establish live
microphone/system audio, screen/GUI offline traffic or clean installation.

Reference: official notarized Cluely macOS 2.1.28 installer/installed ASAR match; 29 private
captures and 23 bounded findings. Public specs/tokens retain measurements/hashes/unknowns.
Remote renderer/model versions, exact DPI and fresh onboarding are unknown. Reference
account/mode/screen/capture-hiding state was restored. Commercial artifacts, recovered code,
account material, personal captures and backups remain outside the public repository.
Mobile/web/sharing/sync evidence is explicitly documentation-only and runtime-unpinned.

A user-only Screen Recording handoff is still pending in System Settings → Privacy &
Security → Screen & System Audio Recording. The Cueglint switch was observed OFF. The
existing test app and Settings focus are preserved; do not grant access or replace its
frozen binary while waiting. A new unsaved TextEdit original screen fixture is retained
for that test. The ignored private continuation file records the exact handles and paths.

Not verified: real dual-source capture, device/echo/drift/recovery, screen freshness,
full offline GUI/traffic, native AX/global shortcuts, capture exclusion, multi-monitor/
fullscreen behavior, clean installs, Windows/Linux runtime and sustained resource/power
operation. Calendar, rich document parsing, derived notes/templates, broader settings,
sharing/mobile/browser/sync and distribution gates remain open in ROADMAP.md.

Registry: 73 contracts; 19 with partial executed reference evidence, 3 implemented contracts,
0 matched passes. Generated counts do not represent a completion percentage.

Recoverable data: installed development models remain in /tmp/cueglint-models; original
inference artifacts remain private outside the repo. The isolated vendor-test build ran
out of disk before executing tests; only 3.3 GiB of rebuildable workspace target output was
cleaned, then a unified workspace build passed. Models, evidence and dist's earlier ad-hoc
test app remain intact. Do not share one target directory between divergent worktrees without
rebuilding project packages; an observed stale-dependency collision was corrected.

Next exact steps:

1. Inspect actual platform CI for PR #11 and integrate only after its required checks.
   The workspace now includes the patched dependency's actual hidden-wake/close regression.
   Preserve native runtime gaps and leave the unknown Ask default unset.
2. Integrate the privately tested standard-Whisper frontend correction after this coherent
   shortcut increment. Preserve prior ASR failures; verify fixed numerical tolerances and
   then run the unchanged licensed-v2 accuracy/performance criteria without output retries.
3. After the pending OS handoff, verify native child AX/default startup and run controlled
   dual-source audio, screen freshness, stop/save/reopen and offline traffic scenarios.
4. Continue every delivery gate and companion surface. Never mark an early gate or the
   first answer as completion of the assignment.
