# Cueglint continuation record

Goal active: complete the pinned native desktop experience and the inventoried companion/
self-hosted tracks. No desktop release acceptance or product parity is claimed. The initial
workspace was empty; existing user work and shared history have been preserved.

Public repository: https://github.com/thematheusmello/cueglint.
Latest main: `39894179cea55def69eac37b30f4ca1d29ecb643`, merged through
[reviewed PR #7](https://github.com/thematheusmello/cueglint/pull/7).
[Three-platform implementation CI passed](https://github.com/thematheusmello/cueglint/actions/runs/44011960435)
on its identical source tree `fd53c3a09ebff827c56ede0774c1ced9f87c58ec`. Historical Windows
hard-link and generated-file failures remain. Pages deployed successfully; the public URL
returned HTTP200. Native runtime and clean installation are separate from those CI jobs.

Current branch: `fix/redistributable-speech-fixture`, prepared against that main revision.
[Issue #8](https://github.com/thematheusmello/cueglint/issues/8) tracks replacement of the
Samantha system-voice waveform, whose public redistribution was not established under
macOS27 SLA2F. The current-tree v1 audio is removed and archived privately; byte-identical
metadata and historical measurements remain. Older Git objects still contain the asset.
Do not rewrite or force-push history without the owner's explicit approval.

V2 is an original eSpeak NG formant fixture with a separate GPL-3.0-or-later media license,
complete notice/source identities and a pinned development-only recipe. Its Python wrapper
is MIT OR Apache-2.0; no synthesizer/resampler is a Cueglint runtime dependency. Voice,
145 wpm, pitch, amplitude and gap were fixed before ASR; the written sentence, normalization,
WER ≤0.10 and warm RTF ≤1.0 are unchanged. The acoustics are different, so v1/v2 scores do
not substitute for each other.

The first fixed v2 base.en run **failed accuracy**: 5/10 errors, WER 0.50. Warm decode took
2.092 seconds for 4.2785 seconds of audio, RTF 0.489 passed. No waveform, voice, speed,
normalization, threshold or inference retry was used to improve the result. Failure is
preserved in parity/reports/redistributable-speech-v2-asr-001.json and independently scored.
The earlier base.en WER 0.10 / RTF 0.560 is retired-v1 history, not current acceptance.
The coordinated macOS workspace run passed 67 implementation tests with 2 model tests
excluded from the normal suite. Formatting/diff checks passed. No model has release-quality
acceptance; broader fixed audio tasks and model/backend evaluation remain required.

A separate model-management increment is saved locally as `feat/model-management` commit
`9540b8d`. It passed 85 normal tests and a real disposable base.en import/transcribe/remove
cycle under network denial, with originals unchanged. Review fixed draft loss, normal-quit
cleanup and stale model-cache readiness. That branch is deliberately unpublished until it
incorporates the licensed-fixture correction. Its pre-correction reports remain historical.

Working native/core capabilities include compact/expanded assistant windows, Settings,
sessions/history, edited notes, modes, text knowledge, copying/versioned exports, atomic
backup/restore and CLI access. Real local Qwen generation streams/cancels and passed narrow
original grounding/followup checks; those text-only results are separately scoped. Frozen
context is bounded independently of history, with exact prompt identity and omission counts.
Earlier native typing/copy/save/reopen and screenshots attest their recorded binary only.

Reference: official notarized Cluely macOS 2.1.28 installer and installed ASAR match;
29 private captures/23 findings. Public specs retain measurements, hashes and unknowns.
Remote renderer/model versions, exact DPI and fresh onboarding are unknown. Reference
account/mode/screen/capture-hiding state was restored; commercial artifacts, recovered code,
account data and personal captures remain outside the public repository.

A user-only handoff is pending in System Settings → Privacy & Security → Screen & System
Audio Recording. Cueglint's switch remains OFF. Preserve the existing test app, its frozen
binary and Settings focus; do not grant access while waiting for the user's action. A new
unsaved original TextEdit screen fixture is retained. Exact handles/private paths are in
ignored .private/continuation.json; no live capture is running.

Not verified: real dual-source capture, device/echo/drift/recovery, screen freshness, full
offline GUI/traffic, native child AX, global keys, capture exclusion, multi-monitor/fullscreen,
clean installs, Windows/Linux runtime and sustained resource/power use. Calendar, rich
documents, derived notes/templates, sharing/mobile/browser/sync and distribution remain open.
Registry: 73 contracts; 19 with partial executed reference evidence, 3 implemented, 0 matched
passes. Counts are generated, not completion percentages. ROADMAP.md retains every gate.

Recoverable state: installed models remain at /tmp/cueglint-models; private original model
outputs and retired waveform are outside the repository. Dist's earlier ad-hoc test app is
intact. Only disposable build outputs were cleaned after disk pressure. Do not share target
directories between divergent worktrees without rebuilding project packages; a stale-cache
collision was detected and corrected. No active synthesis/inference jobs remain.

Next exact steps:

1. Publish/review the current asset correction, preserve the explicit v2 failure, regenerate
   progress and inspect its actual platform CI. Keep issue #8 open for the historical-object
   decision; removing a current-tree file is not a Git-history purge.
2. Merge the correction into local model-management, preserve both documentation sets, run
   the licensed fixture/normal checks, and publish that increment through a PR.
3. Implement native global shortcuts from the completed API research, including focused
   Shift-chord/repeat fixes, conflict status and an unset unknown question default.
4. After the pending OS action, verify native child AX/default startup and run controlled
   dual-source audio, screen freshness, stop/save/reopen and actual offline traffic checks.
5. Continue the complete roadmap. An early gate or successful answer is not completion.
