# Cueglint continuation record

Isolated implementation handoff from input-binding base
`6a6de8b92bb73bdb1602dece82ab4972772ce1da`: local extractive highlights now have
notes-only `cueglint-highlights-json-mask-v1` token masking. The original prompt,
profiles, whole-chunk coverage, parser and 128-token/1,024-byte/eight-index limits
are unchanged. Generic chat remains unconstrained. New optional decoder-policy
provenance is an additive v2-record extension; older strict readers may reject
new constrained records. No schema version change or quality/parity pass is
claimed. Prior actual pilot rejections for more than eight indices remain failed.

Exact next steps for this isolated source increment (all Cargo/model work UNRUN):
run `cargo test -p cueglint-local --lib`, `cargo test -p cueglint-core`, then the
appropriate workspace tests, `cargo fmt --check` and `cargo xtask progress --check`
in root's authorized build window. Review `notes_decode.rs`, both `decode_tokens`
callers and optional provenance backup/export validation. Only after a genuine
reviewed code fix and build verification, rerun the original real-model salience
evaluation with unchanged fixtures/criteria and record the exact source commit,
policy, configuration and model/tokenizer identities described in
`docs/generated-highlights-local.md`. This implementation agent has not accessed
private evaluation fixtures, golds, raw outputs or installed model/tokenizer
payloads. Root owns integration with the separately frozen failed actual evidence.

The goal remains active: complete the pinned native desktop experience and the separately
inventoried companion/self-hosted tracks. No desktop release acceptance or product parity
is claimed. The initial workspace was empty; existing work and shared history were preserved,
except the separately owner-authorized path-only media cleanup described below.

Public repository: https://github.com/thematheusmello/cueglint.

Current candidate code `6d6e371` passed277 workspace tests, zero failed, four explicit
configured-weight/oracle/native-probe ignores and zero filtered. Source endpoints stayed
unchanged. It includes explicit CPU-default ASR selection, test-only native-font resolution,
Windows source-only WASAPI loopback, fixed General geometry, settled legacy-font capture
metadata and explicit Store lock cleanup after SQLite closes.

The same layout/font code at3e4cce4 passed276 tests and13 fresh native GPU fixtures.
Independent review matched transport/font identities and10 unique current original images,
with no adverse static wrapping/overlap. These are own-rendering fixtures, not reference
comparisons, real OS interaction, new resolver adoption or native capture/model quality.
The failed2f AX-property expectation, failed1b backport CLI recovery, failed duplicate-handle
reproducer and prior04ae macOS CI geometry failure remain public reports. Exact original
fork timing is unknown. The duplicate-handle regression now passes and live ownership
remains exclusive. Native unlock errors in Drop remain best effort.

The smaller PR16 backport codecdac6d7 independently passed257 tests, zero failed,
three ignores and zero filtered. Its docs/report head533d528 was pushed for new hosted
checks; verify the returned PR head and CI before merging. It excludes the separate
ASR backend, OS resolver prototype and Windows capture work. Windows loopback has nine
portable cases passed on macOS; its two native schema/join cases, Windows compilation
and driver capture remain unrun. The optional mic generalization is an isolated draft.

The complete eight-part goal remains active. The two-case original highlights protocol
is frozen before outputs (SHAfcbdc4217ab91ba1a242287045fb933a6791bbf3bcb94c94e14637cac64f8659),
but models have not been run for it. Private runner preparation, reviewed release/binary
binding and actual one-attempt offline quality evaluation are next independent work.

Latest public main: `4738a4706d99fda2fef62965979e0da0622f1c88`, merging
[PR #15](https://github.com/thematheusmello/cueglint/pull/15) after current-head
three-platform Rust checks44107427184 and native fixture/child checks44107427212 passed.
The initial macOS no-runner cancellation remains recorded; its targeted rerun passed.
The current combined increment is [draft PR #16](https://github.com/thematheusmello/cueglint/pull/16),
with verified code `00ff533e60bbac6d6d93ebb22112865630c21c9a`:253 workspace tests passed,
zero failed, three configured-weight/oracle ignores and zero filtered. All13 new
native GPU fixtures passed. Metadata/report follow-ups retain that exact code identity;
new-head hosted checks must be verified separately. Earlier public headbd5e787 and its
source85 failed native batch remain historical.

Prior main: `4f726a761d409f46b87095d6fc1ae30ddc39e3b3`, merging
[PR #14](https://github.com/thematheusmello/cueglint/pull/14) after
[three-platform checks](https://github.com/thematheusmello/cueglint/actions/runs/44089963011).
It retains the native package, visual diagnostic and fixed Metal ASR evidence, including
prior failures. [PR #13](https://github.com/thematheusmello/cueglint/pull/13) supplied the
original identity/translucency/runtime source45f2428. Its three-platform Rust and native
macOS six-frame/AccessKit-child jobs passed. The public dashboard at
https://thematheusmello.github.io/cueglint/ returned200 with the new native/Metal reports
visible. A later deployment must be checked separately.

The merged provider increment: [PR #15](https://github.com/thematheusmello/cueglint/pull/15),
published at `843a0111fc1aa27574bc26628ee5ca48a2e75ff8`: explicit BYOK OpenAI, Anthropic, Gemini, Perplexity, OpenRouter and
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

Historical combined source `85a4979` on `feat/desktop-workflows-combined` includes the
reviewed native layout/fixed fixtures and explicit local extractive highlights on
provider base843a011. The full unfiltered workspace passed239 tests, zero failures,
three configured-weight/oracle ignores and zero filtered tests. Both existing native
Missing-lookup checks ran. Separate integration-manifest/Git validation covers326
files; the root test wrapper retained unchanged endpoint results, not a separate full
per-file pre/post manifest. Notes v2 restores v1 without replacing human notes, title,
summary or hosted provenance; typed ACK, autosave/CAS and cancellation guards remain.
Model highlight salience/quality, summary/decisions/action items and reference automatic
triggers have not been verified.

The same combined executable produced7/7new native-v2 captures and5/6legacy captures.
Legacy Markdown timed out30s after its screenshot request; no readback event or PNG.
The failed run is retained with zero output retries. Source investigation found a
pre-submission surface path can consume screenshot tokens; that is a possible cause,
not proven by the trace. Null viewport visibility reflects unknown occlusion, not proof
of a hidden window. The later renderer repair is described below; it does not identify that old failure cause. The provider
form capture visibly shows an inactive Compatible draft, empty key and disclosures;
it performs no network/credential operation. Earlier ca7 source13/13 captures and its
independent visual audit remain historical, not a combined-source pass.

The installed reference auto-updated to2.1.29 at2026-11-17T15:09:56.211900Z. The
retained official2.1.28 installer/ASAR were rechecked read-only; baseline2.1.28 stays
chosen. A reviewed typography declaration specification is public, separate from actual
computed/used fonts and unknown display scale. A source-only OS font resolver workstream
uses documented system APIs and embeds no Apple/reference font files.

Current-head provider native CI44107427212 passed six frames and child startup. Rust
run44107427184 first failed because macOS never acquired a hosted runner (no steps),
while Linux/Windows passed. The targeted rerun passed
macOS job134150044903, Linux134150047481 and Windows134150049324. Keep both attempts. All
current843 checks are green; that source is merged as4738a47.
Native computer-control and browser access remain unavailable after reset. The user
reported reconnection; a fresh inventory still returned the native-pipe and browser-host
startup errors. The question is answered; no repeat request is pending. The isolated a9 provider preview never launched,
and that bundle's screen-permission state is unknown. Stable dist GUI0ff61d45/CLIdbc1d13c
remains frozen. Do not repeat the prior broad Screen Recording enablement request.

Renderer/audio source00ff533 now preserves pending screenshot tokens per viewport
until an actual copy submission, binds receipts to ROOT plus a UUID, and diagnoses
finite pre-submit/map/readback failures. Its full253-pass workspace and354-file
pre/post snapshots were independently audited. Both source-stop calls run even after
an error; callback delivery closes, final packets drain under a128-packet cap, both
endpoints finish, and decoder/delivery errors retain source/segment identity. Seven
synthetic audio tests passed. Live driver shutdown, echo/drift and actual dual-source
transcription remain unverified.

New captures at the same source/GUI passed6/6legacy plus7/7native-v2. Independent
review checked13 request/receipt chains and10 unique original images, including two
initially occluded windows that recovered with their original token. Own-GPU readback
is separate from OS input/AX/focus/blur/capture exclusion and reference visual parity.
The old12/13batch remains failed without a guessed root cause.

Optional open-model API evidence is now retained for the frozen a9 packaged CLI with
official llama.cppb12359 and pinned Qwen1.5 Q4_K_M. Actual003 loaded one model but
failed missing CPU startup proof before any question; all14children were reaped.
004 changed only documented bounded verbosity and exact tagged log-emitter parsing,
retaining original prompt/model/CPU/quality criteria. It completed a real catalog
and two streamed/persisted answers to one original question. Root and independent
blind semantic review accepted support, relevance and completeness. Four scoped
native-read commands ran, one explicit Missing status was observed, no credential
write ran, and all22owned children were reaped. Independent91-endpoint and SQLite
audits match. No default DB, user key or live vendor account was used.

StartupCPUproof shows0/29offload and CPU model/KV/compute buffers. Both305-token
prompts evaluated fully with cached n_tokens0; checkpoint capability and graph/model/OS
caches remained. Provider completions1273/1227ms and CLI commands1353.188/1256.454ms
are n1+n1 functional observations, not broad or reference latency benchmarks. An
unrelated project's Cargo process was observed immediately afterward; overlap is unknown.
One unique question does not establish general model quality. Ollama/LM Studio and
live vendor accounts remain unverified. Preflights001/002, the attempted prompt-template
change and actual003 failure remain retained. No answer was retried.

Next source work is isolated and unverified: a runtime OS-font resolver/role binding
prototype (no font assets redistributed) and explicit CPU/Metal ASR selection preserving
legacy worker behavior and defaultCPU. No native font/device probe or new ASR model run
has been performed. Next exact validation: review each frozen source, run targeted
weight-free checks, then separately approve controlled native probes. Prepare a frozen
original highlights model/fixture/rubric plan before testing actual highlight salience.
The full live/offline/session and companion roadmap remains open.

The isolated `codex/windows-system-audio-001` worktree starts at `04ae900`. Its source-only
Windows system source uses pinned `windows 0.62.2` WASAPI bindings, a COM-owned STA worker,
the default console render endpoint, validated float32/PCM16/24/32 input, explicit
silence/timestamp handling and bounded nonblocking source-labelled packets. It retains
both-source stop attempts and the capped final drain. Native startup/stop/join can still
wait on uninterruptible OS/driver calls. The one-line desktop caller change skips the
macOS screen prerequisite only on Windows; the separate ASR backend change is not included.
No Windows compilation, test execution, native/device probe, capture, model access, key
access, GUI action or publication was performed. Direct source formatting, diff and metadata
consistency checks are the only executable validation in this branch. Nine portable pure tests and two
Windows-only schema/join tests are authored but unrun. At this system-only checkpoint,
Windows microphone CPAL silence/timestamp handling remained a separate risk; no dual-source
readiness was claimed.
Exact continuation: root reviews/integrates the frozen commit and dependency association,
runs `cargo test -p cueglint-local --locked audio::` and
`cargo test -p cueglint --locked runtime::audio_tests`, and allocates a Windows `--locked`
compile. Then run `cargo fmt --check`, regenerate the root-owned progress files and run
`cargo xtask progress --check`. Generated files are intentionally unchanged here despite
the registry gap text update. Follow the separately scoped original-fixture Windows checks
in `docs/platform-support.md` before runtime, power, recovery or parity claims. Default
render selection is pinned for each capture; explicit selection/automatic switching remain
open. Observation, partial implementation, not-run verification and prior failures remain.

The next isolated `codex/windows-microphone-001` candidate starts at `cb43faa`. It reuses
that original WASAPI worker for microphone capture endpoints with flags zero and system
render loopback, with exact packet/error source labels and at most two owned instances.
Saved CPAL WASAPI IDs map through public host/id accessors to the same opaque native ID;
empty/NUL/over-16,384-unit UTF-16 IDs fail before Win32. Active/capture direction and exact
returned identity are required; no fallback or stored data rewrite occurs. Default mic
state/identity is polled every 250 ms and change/removal stops visibly; explicit selection
ignores other default changes. This is a declared interval, not a measured timing bound.
The prior system pin behavior, shared format/clock/silence checks, cleanup, owned joins,
both-source stop and capped final drain remain. Mac/Linux microphone and macOS system
byte sections, public interfaces, Settings, runtime/ASR and dependency files are unchanged.
The original design receipt is outside the repo; no CPAL source was copied or forked.
Thirteen portable and four Windows CPAL/schema/join tests are authored but unrun. Only
direct formatting, whitespace and source/metadata consistency checks are allocated here;
no Cargo, new target, native/device/audio/model/key/network/GUI/service/public action.
An initial clock text comparison flagged Rustfmt line wrapping; the handoff retains that
failure and the subsequent literal-preserving token comparison of the unchanged math.
Exact continuation: root independently reviews/integrates the frozen candidate, runs
`cargo test -p cueglint-local --locked audio::` and
`cargo test -p cueglint --locked runtime::audio_tests` under its build allocation, then
checks Windows compilation and schema cases. Run `cargo fmt --check`; root regenerates
progress and runs `cargo xtask progress --check`. Before runtime claims, use the original
controlled Windows selection/permission/silence/clock/stop/power checks in the platform
matrix with frozen source/binary/OS/driver/endpoint/fixture/configuration identities.
Native compilation/runtime and reference parity remain unverified. Uninterruptible COM,
driver calls and joining still have no hard deadline guarantee. No automatic rebind exists.


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

The explicit local ASR processor increment is source-only in an isolated worktree from
`00ff533`: typed CPU-default settings, CLI/native choices, compile-only readiness and
actual-device provenance use the existing Candle backend. Legacy development-worker
`Transcriber::load` behavior is preserved. Generation/highlights/cache and retained
Metal evidence are unchanged. Focused parsing, storage/restore, readiness, capture-write
guard and native accessibility tests are authored but have not run. No Cargo, model,
device, capture or GUI checks were performed for this increment.

Before accepting that increment, run its focused core/local/CLI/native tests and the
unfiltered locked workspace suite after root releases build allocation; leave existing
model/oracle ignores intact. Then run `cargo fmt --check` and `cargo xtask progress
--check`, preserving failures and source identities. A Metal-enabled compile and actual
device/packaged/live checks remain separate work; prior 28-call results attest only their
retained source and worker, including the 10.58-second first base warmup.

A separate source-only input-binding increment is prepared in an isolated worktree from
`d28b390`: managed Qwen/ASR consume their same verified read-only file handles, and native
ASR events retain the loader's bound profile. No reader lease, root/lock creation,
whole-GGUF staging buffer, schema/UI/dependency change or cache acknowledgement protocol
is added. Legacy/read-only roots and direct-directory ASR APIs are preserved. The existing
published/frozen quality evidence and release binaries are unchanged.

Eight new weight-free tests plus existing lifecycle/device/provenance regressions are
authored and UNRUN. Only direct Rust formatting and source/diff review were performed;
no Cargo, new target, model/tokenizer payload, native/device, network or key action ran.
Before integration, Root must release the benchmark/source hold, review the committed
delta, then run focused local/native/CLI tests and the full locked workspace suite with
configured-model/oracle/native-probe ignores retained. Run `cargo fmt --check` and
`cargo xtask progress --check`, preserving failures. Cooperative atomic replacement and
open-handle removal need declared Windows/Linux/macOS results; read-only directory mode
coverage is Unix-specific, while read-only artifact coverage is portable. Uncooperative
in-place writes and new model/Metal/package/live claims remain outside this increment.
CLI `transcribe.elapsed_ms` now includes verification; old timing scopes remain recorded.

Next exact steps:

1. PR15 source/tests and the actual release package pass their declared checks. Publish
   the retained hosted/release audit, then inspect the native provider form and controlled
   workflow once computer access reconnects. Sky native pipe/browser host failed even after
   reset; the prepared isolated preview never launched and no test UI key was created.
   The access question is pending. Live accounts/model quality remain separate opt-in checks;
   keys belong in the app, never chat. Keep earlier failures and budget limits visible.
2. Integrate the clean `feat/desktop-highlights-integration` descendant of 843a011, retaining
   the current provider/privacy/data-format docs and registry. Run `cargo test --workspace
   --locked` unfiltered on root's existing target under `nice -n 10`/`CARGO_BUILD_JOBS=2`
   with a 6 GiB free-space floor; leave the 3 configured-model/oracle checks ignored. The two
   existing native credential Missing tests may read only their own fresh UUID endpoint
   references. Run `cargo fmt --check` and `cargo xtask progress --check`; preserve every
   failure and exact source identity.
3. Freeze the resulting source/binary before any root-owned native UI window. Recheck the
   provider form and combined native-v2 fixtures, then plan a frozen original fixture/model/
   prompt evaluation before any highlights quality claim. Keep the stable a9ca43a bundle
   unchanged until an explicitly authorized replacement. No real highlight quality pass.
4. Freeze the intended stable bundle and resolve only its actual OS permission need. Test
   corrected main-display capture and screen freshness with the original controlled fixture.
5. Continue every remaining desktop gate, platform and companion surface. An early gate,
   first answer, passing compilation or successful provider mock does not finish the goal.

Recoverable raw paths, current owned process handles, source identities and active agent
allocations are in the private continuation record. Installed models and evidence remain;
use separate target directories for divergent worktrees and reclaim only owned disposable
build output after checking that its jobs have ended.
