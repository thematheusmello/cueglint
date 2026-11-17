# Cueglint continuation record

Goal remains active: complete native desktop parity, then the inventoried companion and
optional self-hosted surfaces. This checkpoint is an experimental implementation increment,
not desktop release acceptance or a claim that the assignment is complete.

Repository: https://github.com/thematheusmello/cueglint (PUBLIC, verified).
Initial workspace was empty; no user code/history was replaced. Published source: `2a25b5719fac555e27a864eb2a95f6d40eddf1b7`.
[CI](https://github.com/thematheusmello/cueglint/actions/runs/43998492498) is running;
[Pages deployment](https://github.com/thematheusmello/cueglint/actions/runs/43998492573)
succeeded and [public progress](https://thematheusmello.github.io/cueglint/) returned HTTP200.

Working: native Rust/wgpu Settings and a separate compact/expanded assistant; local
sessions, saved history, edited notes, modes, text knowledge, copying/exports, transactional
backup/restore and a CLI sharing the core. Explicit pinned model installation/hash checks
and real cancellable local generation work. Native UI typing/send, expansion, Copy→Paste
were manually exercised with original content. Model answers are not yet reliable.

Checks: the coordinated workspace run passed 43 implementation tests, with 2 model tests
intentionally ignored without configured local weights. Formatting and registry/progress
checks passed. Separate real local model runs are in parity/reports/ and were independently
scored: Qwen streamed/cancelled, 2/3 basic fact checks passed, but unsupported-owner,
reference-document completeness and benign followup failed. Tiny.en WER 0.20 failed the
preset 0.10 threshold; warm RTF 0.353 passed speed. Failures have not been hidden or rebaselined.

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
Default launch currently opens Settings; --overlay exposes the assistant. This differs from
the observed reference startup and remains an explicit UI gap.

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

1. Monitor the actual three-platform CI run above and repair failures. A macOS arm64
   release bundle and CLI built successfully; its ad-hoc signature, CLI help and native
   fixture launch were verified on the development host, not a clean machine.
2. On a reviewed feature branch, correct the English Whisper decoder prefix and separate the
   user's question from untrusted context in the Qwen prompt; rerun the same fixtures and
   preset thresholds. Evaluate larger base.en/1.5B profiles only when meaningful, retaining
   all failed configurations and identities.
3. Run the packaged native executable with original screen/audio fixtures; verify permission
   setup, actual dual-source capture, source timing, stop/finalization, screen freshness,
   streaming answer, save/reopen, and offline traffic. Ask only for concrete user-only access.
4. Continue all gates in ROADMAP.md; do not mark the goal complete at an early gate.
