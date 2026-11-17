# Cueglint continuation record

Goal remains active: complete native desktop parity, then the inventoried companion and
optional self-hosted surfaces. This checkpoint is an experimental implementation increment,
not desktop release acceptance or a claim that the assignment is complete.

Repository: https://github.com/thematheusmello/cueglint (PUBLIC, verified).
Initial workspace was empty; no user code/history was replaced. Published source:
`70d28afe9d40c877a1b59531a2aeaa41da3bd3b4`; current working branch is `feat/live-local-quality`.
The reviewed implementation increment is published as commit
`83cb59829702d73f423d194fa1eb99b0cd658aec` in [draft PR #7](https://github.com/thematheusmello/cueglint/pull/7).
Its [three-platform CI](https://github.com/thematheusmello/cueglint/actions/runs/44008138782)
passed implementation tests on all three runners, including the Windows export guard.
Windows then failed the generated-file comparison; LF checkout is now enforced and a new
run is required. Superseded duplicate runs were cancelled, not counted as passes.
[Bootstrap CI](https://github.com/thematheusmello/cueglint/actions/runs/43998492498) failed
the Windows hard-link export guard; macOS/Linux implementation checks passed. The current
cross-platform identity fix preserves that regression; new Windows CI remains required.
[Pages deployment](https://github.com/thematheusmello/cueglint/actions/runs/43999231537)
succeeded and [public progress](https://thematheusmello.github.io/cueglint/) returned HTTP200.

Working: native Rust/wgpu Settings and a separate compact/expanded assistant; local
sessions, saved history, edited notes, modes, text knowledge, copying/exports, transactional
backup/restore and a CLI sharing the core. Explicit pinned model installation/hash checks
and real cancellable local generation work. Native UI typing/send, expansion, Copy→Paste
were manually exercised with original content. The larger local defaults pass the narrow
original checks; broad answer reliability and complete capture acceptance remain open.

Checks: the current macOS coordinated workspace run passed 66 implementation tests, with 2 model
tests intentionally ignored in the normal suite. Formatting and structural registry checks
passed. Real model runs in parity/reports/ preserve the initial and protocol-correction
failures. Independently scored Qwen 1.5B passed all three original fact cases, both workshop
facts and a benign followup. Whisper base.en met the unchanged WER 0.10 and warm RTF 1.0
thresholds (actual 0.10 / 0.560). These single-run development results attest their recorded
source/binary snapshots, not later source changes or broad quality. Failures remain intact.

New source bounds frozen request transcript/chat independently of full history and records
actual prompt hash/template identity. Regression checks cover long histories, legacy backup
compatibility and exhausted revisions without worker panics. The native UI exposes real
permission status separately from requested sources and persists model choices. The eframe
child AccessKit patch preserves upstream licenses; its native accessibility check is pending.
The macOS process-network-denial CLI workflow passed with real file ASR and local
generation, save/reopen, versioned export, transactional backup/restore and deletion.
`parity/reports/offline-cli-002.json` records the unchanged source/binary snapshot; a
separate Python JSON consumer checked the first run's original export. This is not live
audio, screen/GUI acceptance or a packet-traffic audit. The later targeted xtask suite
passed seven tests, including contract-freshness invalidation, historical-failure indexing,
atomic no-clobber reports, retained late identity errors, link-cycle rejection and Cargo
artifact discovery. Offline run 003 passed before the final input-validation tightening;
its snapshot remains historical rather than a whole-tree-current acceptance claim.

Reference: official notarized Cluely 2.1.28 installer and installed ASAR match; 29 private
captures and 23 bounded findings. Public specs/tokens include hashes, geometry and unknowns.
Remote renderer/model identities, exact DPI and fresh onboarding remain unknown. The existing
reference account/mode/screen/capture-hiding settings were restored. Commercial packages,
raw analysis, personal captures and backup remain outside the repository.

Not verified: controlled live microphone+system audio, hardware recovery/echo/drift,
screen/OCR freshness, blocked-network inference workflow, global shortcuts, full overlay
accessibility/capture exclusion, multi-monitor/fullscreen behavior, clean installs,
Windows/Linux runtime and sustained power/resource operation. Calendar, broad document
parsing, note templates, local model import/removal, sharing/mobile/browser/sync remain open.
The current source defaults to the compact assistant; `--settings` opens Settings directly.
Startup state tests pass; the earlier packaged source still opens Settings and is frozen
during the OS-permission handoff.

Registry: 73 contracts; 19 with partial executed reference evidence, 3 fully implemented data/
model-install contracts, 0 matched verification passes. Counts are generated, not completion
percentages. Issues 1–6 and milestones 1–6 preserve the complete roadmap.

Recoverable local state: verified development models are in /tmp/cueglint-models; private
original inference artifacts are in /tmp/cueglint-inference-benchmark (0700). Public reports
use relocatable path placeholders. Native UI review uses an isolated temporary app/database;
never substitute its data for a user's meetings. Build output is disposable; dev/test debug
symbols and incrementals are disabled after a disk-full recovery. Current tool jobs should
be checked before starting duplicate Cargo/model work. Private reference checkpoint location
and temporary job handles are recorded in ignored .private/continuation.json.

Next exact steps:

1. Publish the reviewed branch with generated progress and verify the actual three-platform
   CI result, keeping the earlier Windows failure visible until its replacement passes.
2. The controlled packaged screen preview reported missing Screen Recording access. A
   user-only handoff is pending in System Settings → Privacy & Security → Screen & System
   Audio Recording. The existing test app, its binary and Settings focus are preserved.
   Do not grant access or replace its binary while waiting for the user's action.
3. After the handoff, verify actual native child accessibility/default startup and run
   original screen/audio fixtures: dual sources, timing, stop/finalization, screen freshness,
   streamed answer, save/reopen and actual offline traffic. The prior ad-hoc package was
   verified on the development host, not a clean machine or a notarized distribution.
4. Continue model import/removal, derived notes/templates, rich documents and all remaining
   gates in ROADMAP.md. No early gate completes the assignment.
