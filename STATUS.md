# Cueglint continuation record

The goal remains active: complete the pinned native desktop experience and the separately
inventoried companion/self-hosted tracks. No desktop release acceptance or product parity
is claimed. The initial workspace was empty; existing work and shared history were preserved,
except the separately owner-authorized path-only media cleanup described below.

Public repository: https://github.com/thematheusmello/cueglint.
Latest public main: `4f726a761d409f46b87095d6fc1ae30ddc39e3b3`, merging
[PR #14](https://github.com/thematheusmello/cueglint/pull/14) after
[three-platform checks](https://github.com/thematheusmello/cueglint/actions/runs/44089963011).
It retains the native package, visual diagnostic and fixed Metal ASR evidence, including
prior failures. [PR #13](https://github.com/thematheusmello/cueglint/pull/13) supplied the
original identity/translucency/runtime source45f2428. Its three-platform Rust and native
macOS six-frame/AccessKit-child jobs passed. The public dashboard at
https://thematheusmello.github.io/cueglint/ returned200 with the new native/Metal reports
visible. A later deployment must be checked separately.

Current increment: [draft PR #15](https://github.com/thematheusmello/cueglint/pull/15),
published first at `511c3aa3e5bbee8ce249ee899bf41298582051db`: explicit BYOK OpenAI, Anthropic, Gemini, Perplexity, OpenRouter and
OpenAI-compatible endpoints, preserving local/open models as default. Original feature
commit `53d43f68f48999e3ee9b92045e67d58585a89237` is integrated onto current main as
`7a9f6e7967feda234fd50d45a4ac27c7987d2c98`. The frozen full workspace passed195 tests,
zero failures, with three configured-weight/oracle checks ignored. Independent review
reproduced292 source-file hashes and found no remaining material defect. Root verified
all29 feature files and113 runtime/build files match after integration; only six baseline
docs/registry files differed. Controlled tests cover six protocols and the headless native
input→production worker/client→Store→conversation path, plus real CLI subprocess routing.

A separate one-time macOS native credential run passed15 production-CLI commands: fresh
UUID-endpoint Missing, save synthetic key, Stored, authenticated catalog/synthetic probe,
exact-scope recheck, delete and Missing. No user key/default DB, live vendor or model was
used; the temporary key's removal was verified. Independent read-only audit checked all
steps, reconstructed the synthetic HTTP body, and matched SQLite to its logical backup.
The actual release package at a9ca43a passed signature/plist/checksums. Its frozen CLI
passed another15-command native credential run with two synthetic requests and finalMissing;
that new artifact justified separate revalidation. Hosted head511 passed all three Rust
platforms plus six native GPU frames and child startup. The default General capture does
not show the provider form below the fold. Native provider-window/GUI credentials, Windows
keychain and live account/model quality still need checks.
Native answer budget is256, CLI default128/configurable1–1024, synthetic probe16. Model
listing does not prove these budgets are adequate for reasoning/auto-thinking models.

Prior provider failures remain: macOS mock-socket inheritance interrupted oversized SSE;
password response ownership failed compilation; an empty-session assertion ignored seeded
fixture sessions. Fixes retained size assertions and exact session-vector preservation.
Independent review also found a streamed key-prefix leak and stale same-endpoint model
probe success. Both are repaired and covered by stronger regressions. Credential saves
remain UUID/scope pinned through provider changes; generation/list/probe work is cancelled.
The public provider records distinguish these checks from live service availability.

The merged increment includes an original Cueglint geometric mark, wordmark, transparent
SVG/PNG exports and app icon. The compact/chat tint layers are translucent without opaque
underpainting. Notes use28-point pitch and32-point metadata chips. The six actual executable
screenshots at repaired runtime `bf1d373` are versioned in docs/images and linked in README.
Alpha/blur are implementation choices, not measured reference values. An independent review
of six native/eight reference frames records remaining input baseline, heading/chip and
Settings spacing differences in ui/reviews/native-runtime-002.md. Exact font/DPI/blur and
some state correspondence remain unknown; no matched visual pass is claimed.

Native answers render CommonMark paragraphs, lists, code and validated HTTP(S) links. One
cancellable/coalesced parser worker and bounded previews keep parsing outside drawing.
Conversation drawing borrows up to16 indexed replies with Earlier/Later/Latest navigation
and preserves an older anchor on arrivals. Full original-source copying is retained.
Independent review found and checked repairs for aggregate draw bounds and stale link
activation after streaming rewrites. Continuous long-history scrolling, extended tables
and native selection/AX/focus parity remain gaps. Editable notes and autosave are preserved.

A real compact startup exposed an AccessKit ordering abort that primary screenshots missed.
The shared child initializer now creates hidden, attaches accessibility, then restores
requested visibility. The real default/shown/hidden child example passed locally and in CI;
its earlier abort and test-harness failures remain. A separate root screenshot readback
progress gap is repaired by continuing periodic submissions until the event arrives, issuing
one capture command. Hosted Settings now completes; the three earlier failed runs remain.
The earlier runtime-repair workspace run passed161 tests including five doctests, with two configured-
weight checks and one external numeric oracle excluded. Headless tests and primary GPU
readback do not establish native focus, blur, window exclusion or whole-product parity.

A real release-mode `cargo xtask package` completed at45f2428. Ad-hoc signature, bundle
metadata, icon and checksums passed. The signed GUI SHA256 is
`4165e05565e5965bfd4d00dc359575274c46b45df5d0227ffbcf89709c041b7c`;
CLI SHA256 is `8d561597c576e83216eb55e2c24d3680688b32214a377192bdd7b845b203e2ed`.
The assistant launched with native Accessibility controls, then was quit before benchmarking.
The stable bundle still reported screen access needed despite the existing Cueglint OS
switch being ON. No GUI was running at that handoff. This is not notarization, clean install
or distribution acceptance. The previous dist and user database were backed up privately.

Working capabilities include compact/expanded native windows, Settings, sessions/history,
edited notes, modes, bounded UTF-8 knowledge, copying/versioned exports, transactional
backup/restore, CLI access and real streamed/cancellable local generation. Model downloads
have pinned hashes, resume, explicit local import/removal, shared locks, readiness invalidation
and cache reset. Orphan import stages after forced termination remain a recovery gap.
Shortcut overrides/disable/reset persist; native registration and conflict/rebind guards
are implemented, while actual hotkey/focus/fullscreen behavior still needs controlled checks.

Qwen1.5B passed narrow original text criteria. The standard Whisper frontend passed frozen
numerical controls; the corrected base.en CPU synthetic-v2 pair still failed WER0.30 against
0.10, while warmRTF0.495 passed. No output retry or scoring/fixture change followed it.
The fixed12-speaker clean-English CPU evaluation completed28 calls/three loads once.
Base.en passed at10/279 errors (WER0.03584), worstRTF0.893. Small.en had7/279 errors but
all12 clips failed speed; v2 warmWER0.10 passed andRTF1.556 failed. That CPU suite remains
FAILED. The separate native Candle/Metal development worker completed the same28 calls and
three loads once at45f2428 and passed the fixed warm gates. Human worstRTF was0.0995 base
and0.2345 small; small-v2 warmRTF0.1712. Base's first warmup took10.58 seconds and remains
prominent. All28 transcripts equal CPU outputs, so no accuracy improvement is claimed.
Independent raw-score/endpoint checks matched. CPU/default/package settings remain unchanged.
This is file ASR on one M3 Max, not live dual-source audio, broad language or reference parity.
The rejected Metal preflight001 and corrected accepted002 remain retained separately.

An independently executed offline CLI run at41cc passed all16 steps: file ASR, grounded
answer, notes, save/reopen, export, backup/restore and deletion under process network denial.
Source and installed models matched. The auxiliary wrapper and redaction audit failed after
execution; read-only recovery and those failures remain. The outer numeric exit is unknown;
all16 CLI step exits were zero. Full offline GUI/screen/audio traffic remains unverified.

Generated extractive highlights are being reconciled from `feat/generated-notes-core`
into isolated `feat/generated-highlights-integration` based on integrated 7a9f6e7:
Core job/candidate/CAS persistence, full transcript chunk planning, local strict extraction,
CLI review/generate/accept/discard/append and version 2 portable data. The final isolated run
passed99 tests with three excluded checks and an independent source review. They are not
integrated into canonical source yet and have no real highlight-model quality result.
The bounded desktop actor, explicit review and autosave/CAS integration are now
prepared in that isolated tree. Its initial integrated Core/local/CLI stage passed 95
tests (3 model/oracle checks ignored, 1 native credential lookup test filtered), and
the desktop stage passed 97 tests (1 native credential lookup filtered). The final
filtered workspace stage passed 229 tests, with 3 ignored and the exact 2 unchanged
native BYOK lookup tests filtered. Earlier compile/fixture failures remain retained.
Next: independent source review, root integration retaining current provider docs,
and the unchanged native BYOK checks in root verification. No actual highlight-model
inference was run. Final formatting/progress receipts are provided with the handoff. This is whole-utterance extraction,
not verified summary/decisions/action-item or reference automatic-trigger behavior.

Reference: official notarized Cluely macOS2.1.28 installer/installed ASAR match;29 private
captures and23 bounded findings. Reviewed public contracts/tokens retain measurements,
hashes and unknowns. Hosted renderer/model versions, exact DPI and fresh onboarding remain
unknown. Raw commercial artifacts, recovered source, accounts, private captures and backups
stay outside the repository. Mobile/web/sharing/sync observations are documentation-only.

The user enabled Screen Recording and the Cueglint switch was observed ON. One actual
request in frozen source2a25 captured display3 while the fixture was on another monitor;
none of three facts reached OCR and its answer was wrong. That failure is retained privately
and publicly summarized. Default-display selection now uses the OS main display, with exact
explicit selection and missing-target tests. Corrected live freshness is not verified.
The original TextEdit fixture remains; its attempted monitor move was unverified. The old
executable is retained. Stop/save of the controlled session followed a private database
backup; unrelated data stayed unchanged. No new permission question is pending. Freeze the
next intended stable bundle before preparing any specific OS refresh request.

The owner-authorized path-only WAV cleanup updated all seven writable public branches
atomically with exact leases, preserving source/reports.25 branch-reachable IDs and five
signature headers changed; the private review including the read-only merge had26/six.
Read-only closed-PR objects and host caches may retain the old asset. Old evidence IDs and
sealed backups remain, with mapping in docs/history-cleanup.md. Do not repeat the rewrite.
V2 media retains separate GPL3-or-later terms; newly authored app code is MIT OR Apache-2.0.

Registry:73 contracts;19 have partial executed reference evidence,4 are implemented,
0 matched passes and0/3 clean-install platforms. Counts are not a completion percentage.
Real dual-source capture, routes/echo/drift/recovery, corrected screen freshness, full offline
GUI traffic, native keyboard/window/AX, recipient capture compatibility, Windows/Linux
runtime and sustained resource/power use remain open. Calendar, rich documents, templates,
sharing/mobile/browser/sync and distribution remain in the full ROADMAP.

Next exact steps:

1. PR15 source/tests and the actual release package pass their declared checks. Publish
   the retained hosted/release audit, then inspect the native provider form and controlled
   workflow once computer access reconnects. Sky native pipe/browser host failed even after
   reset; the prepared isolated preview never launched and no test UI key was created.
   The access question is pending. Live accounts/model quality remain separate opt-in checks;
   keys belong in the app, never chat. Keep earlier failures and budget limits visible.
2. Parallel `feat/native-v2-ui` owns reviewed visual repairs and separate fixed-time/ID
   fixtures; legacy names/builders stay unchanged. No native capture until root releases
   the UI window. Check new source and captures before integration.
3. Parallel highlights commit473c27d reconciles Core/local/CLI and native async/CAS/ACK.
   Its filtered workspace passed229 tests with3ignored and2nativeMissing lookups filtered;
   final docs-only delta preserves all tested Rust/Cargo bytes. Independent review is queued,
   then root must merge and run the two excluded BYOK tests. No real highlight quality pass.
4. Freeze the intended stable bundle and resolve only its actual OS permission need. Test
   corrected main-display capture and screen freshness with the original controlled fixture.
5. Continue every remaining desktop gate, platform and companion surface. An early gate,
   first answer, passing compilation or successful provider mock does not finish the goal.

Recoverable raw paths, current owned process handles, source identities and active agent
allocations are in the private continuation record. Installed models and evidence remain;
use separate target directories for divergent worktrees and reclaim only owned disposable
build output after checking that its jobs have ended.
