# Verification

```sh
cargo test --workspace
cargo fmt --check
cargo xtask verify-registry
cargo xtask progress
cargo xtask progress --check
cargo xtask visual /private/reference.png /private/cueglint.png --report /private/comparison.json
cargo xtask audio-fixtures
cargo xtask native-screens --output-dir /tmp/new-native-frames --report /tmp/new-native-frames/report.json
cargo run --locked -p cueglint --example native-child-check
cargo xtask inference-bench --models-dir /path/to/installed/models --report /tmp/inference-check.json
cargo xtask offline-cli-check --models-dir /path/to/installed/models --report /tmp/offline-cli-check.json
cargo xtask package
```

`visual` rejects viewport mismatches and never resizes or masks whole panels. Predeclared
initial tolerances: 2 logical pixels for geometry, 1 pixel for text baselines, mean absolute
RGBA channel error at most 3/255 and at most 2% of pixels changing by more than 16/255.
Pixel comparisons do not replace typography/geometry/interaction review or measure motion.
Intentional branding masks must be narrow, declared and separately reviewed before use.

[Explicit transcription-language checks](../parity/reports/transcription-language-runtime-001.json)
pass 386 release/offline/locked workspace tests with four explicit ignores and
zero failures/filtering across 34 summaries, plus separate PDF supervision. The
ten new language regressions cover code validation, legacy backup/data retention,
English-prefix compatibility, explicit multilingual tokens, native picker semantics
and capture-worker/session identity. No multilingual weights or real inference
are exercised. Original compile, test-fixture-order and resource-guard failures
remain recorded; none is replaced with an inferred pass.

`audio-fixtures` runs implementation-level core and local-pipeline tests. It is not a real capture/ASR
benchmark. Hardware audio, offline inference, recipient capture compatibility, clean installs,
and sustained sessions remain separate checks. No synthetic answer establishes AI quality.

The twelve required acceptance scenarios are defined in `parity/scenarios/`. The reference
runner, Cueglint runner and evaluator must remain logically independent. Private commercial
fixtures never enter public CI. Public CI runs authored tests and structural checks, without
reference-account or release secrets on untrusted pull requests.

A matched parity pass requires reference-executed observation and a run identity containing
source commit/hash, platform/time, configuration/model/fixture hashes and artifact references.
Non-model claims use an explicit `not-applicable` model identity. `cargo xtask progress`
rejects passed source evidence when implementation, model manifest, fixtures, tokens,
reviewed specifications or contract/acceptance/tolerance fields change. Verification metadata
and review links are excluded from that fingerprint to avoid a self-referential hash.
Remote service changes invalidate affected reference comparisons. Preserve failures and
stale/blocked states instead of rewriting historical reports.

ASR evaluation must fix language-specific error rate, partial stability, finalization, source
attribution and latency thresholds before running. Assistant tasks score support, relevance,
completeness, unsupported specifics, usefulness and delay on fixed plus unseen fixtures,
with repeated real inference runs. Thresholds remain unestablished pending reference runs.

The first CPU model run in `parity/reports/local-inference-smoke.json` failed quality. The
protocol correction run also failed; both remain preserved. The larger measured defaults
passed the same narrow original cases in `local-inference-candidates-v3.json`, independently
scored in `independent-v3-review.json`. Each report attests only its recorded source/binary
and model identities. None promotes a parity contract to passed. See [models](models.md).

The Samantha v1 audio asset is retired from active public fixtures for its redistribution
terms. Its historical scores cannot establish acceptance for the separately versioned
eSpeak formant v2 waveform. The original sentence, normalization, WER ≤0.10 and warm RTF
≤1.0 thresholds remain fixed; changed acoustics require a new measured run. The retirement
ledger preserves old identities and outcomes, including failed cases. The v2 media has
separate GPL-3.0-or-later terms and a pinned development-only generation recipe. The first
base.en v2 run failed WER at 0.50 and passed warm RTF at 0.489; no waveform or scoring
changes followed that result. Keep the failure report when evaluating subsequent models.
One independent development-only CPU path using the same pinned weights/waveform also
failed, at WER 0.30. Its prefix, effective token suppression and setup failures are retained
in `asr-independent-transformers-v2-001/002.json`. This changes preprocessing and neural
kernels jointly and does not establish a cause or speed improvement. A feature-only
comparison in `asr-preprocessing-tensors-v2-001.json` isolates framing and power-folding
differences; its numerical controls are separate from ASR quality acceptance.
The corrected Rust frontend passed frozen numerical tolerances and exact retained-tensor
checks. Its single actual cold/warm CPU pair at source `0b04d488` still failed WER at
0.30; warm RTF 0.495 passed. Source, model, input and binary endpoint hashes matched.
The first runner stopped before inference on an overly specific quiet `nc` stderr check;
that blocked preflight is retained separately. A verbose controlled canary established
process network denial before the only model pair. No waveform/score change or output retry
followed either accuracy failure. The normal workspace run passed 119 tests with two
configured-weight tests and one external numeric-oracle test excluded; those checks have
their own reports and must not be counted as ordinary CI coverage.

Historical CI failures remain in `parity/reports/ci-bootstrap.json`: macOS and Linux passed
implementation checks, while Windows exposed a hard-link export/database alias failure.
The cross-platform file-identity fix kept the original regression test and passed the
[later three-platform CI](https://github.com/thematheusmello/cueglint/actions/runs/44011960435).
The [licensed-fixture correction](https://github.com/thematheusmello/cueglint/actions/runs/44018968905)
and [model-management source](https://github.com/thematheusmello/cueglint/actions/runs/44020584244)
also passed at their recorded heads. Later heads need their own checks. These jobs do not
run native GUI, capture, model weights or clean installs.
The generated progress report indexes historical JSON reports and rejects malformed files
instead of silently dropping failed runs.
Evidence commands refuse existing report paths and publish through the core's atomic,
private no-clobber writer. Late fingerprint failures are recorded as identity errors with
collected outputs retained and the run failed. Fingerprint collection rejects source
symlinks and special files. Endpoint hashes do not claim continuous filesystem monitoring.

`offline-cli-check` is a macOS-only development runner. It builds the actual CLI, proves a
reachable loopback connection fails under the same process network-denial policy, then runs
real file ASR, local grounded generation, notes, save/reopen, versioned export, backup/restore
and deletion under that policy. Reports preserve failures and source/binary identities;
original fixture data stays in a private temporary directory. This does not verify live
microphone/system audio, screen context, GUI, packet traffic or the full offline scenario.

`native-screens` builds the actual native GUI through Cargo's reported executable path and
captures six visibly labelled original fixtures in isolated primary viewports. The macOS
CI job retains every frame, stdout/stderr and report, including failed runs. GPU readback
receipts record image dimensions, effective pixels per point, zoom and the actual adapter
when available. The evaluator decodes and hashes the same bounded PNG bytes; receipt/path,
fixture, size and positive scale must match. Existing output paths are refused and PNGs are
written atomically with private permissions. Encoding runs on the screenshot worker.

The build has a fifteen-minute deadline, each fixture 30 seconds. The runner samples log sizes
every25ms and kills owned process groups on limits; their leaders are reaped. Logs can exceed
the limit between samples. Source/binary endpoint mismatches fail the run. Native screenshots
here are primary viewport GPU readbacks: they cannot verify OS backdrop blur, shadows,
compositing, real child-window behavior, focus, shortcuts, recipient capture exclusion or
native accessibility. They are not matched reference comparisons or model inference.

The independent run in `independent-offline-cli-002.json` passed all16 CLI steps at source
`41cc4367`, including one file transcription and a grounded answer. Independent post-run
checks matched notes, export/backup/restored data, deletion, source and installed-model
identities. The outer artifact wrapper and a later redaction audit each failed; their records
and the read-only recovery are preserved. The documented child numeric exit value was not
persisted and remains unknown; all16 individual CLI exit codes were zero. This did not rerun
inference, clear the fixed-v2 ASR quality failure, or verify the full live offline workflow.

The first remote native-fixture attempt at PR13 head `c042c3a` timed out during its cold
build at the original nine-minute bound. It produced no GUI binary, frame or PNG. Its
artifact/report and unchanged source endpoints are preserved in
`native-fixture-ci-build-failure-001.json`. A separate macOS workspace CI step completed
in624seconds, demonstrating that the original build allowance was insufficient. The new
fifteen-minute bound applies only to development compilation; frame and visual/quality
criteria are unchanged. Linux CI also exposed ambiguous negative-PGID parsing in the
runner's kill command. The retained portability report records its failure and the explicit
end-of-options correction; passing macOS tests alone do not establish the Linux fix.

The second remote native attempt built successfully and saved five verified PNGs at the
nominal dimensions (PPP1, zoom1, Metal paravirtual adapter). Dashboard timed out at30seconds
without a receipt or PNG; the run remains failed in `native-fixture-ci-002.json`. Independent
pixel review found denser notes/chips and an absent scrollbar in the long-answer captured
state, with hover/fade uncertainty. It found no horizontal clipping or footer overlap.
The root Settings capture path already exists through logic/poll; its timeout cause remains
unconfirmed. Finite content-free stderr stage diagnostics now support the next investigation;
stdout retains one saved receipt. The current source corrects notes pitch/chip size and
opaque underpainting. The separate local run at `50d6d8a` passed all six native fixtures
on the M3 Max, including Settings. `native-local-brand-001.json` retains its Metal adapter,
PPP1/zoom1, PNG hashes and finite stages; the versioned PNGs are in `docs/images/`.
This is a local implementation smoke pass; the hosted timeout's cause is still unknown.
`independent-native-local-brand-review-001.json` independently reconstructs its source
fingerprint, hashes the actual Cargo-reported binary, checks all raw receipts/stages and
views all six PNGs. It confirms chosen alpha and the corrected28/32 spacing. Scrollbar
visibility in the captured long-answer state remains unverified.

The first integrated branding test failed compilation because the pinned egui0.35 exposes
`Context::run_ui`, not the old `run` test API. That log is retained; only the test harness was
corrected. The final unified run passed161 tests including five doctests, with the same
three configured-weight/external-oracle exclusions. The separate actual CPU ASR suite above
is failed overall and is not counted as a normal workspace pass.
`native-brand-workspace-001.json` records the test counts, excluded checks, source scope
and retained failure/log hashes. The initial failing branding test used the old egui test
API; the final source uses the pinned `run_ui` API. No visual threshold was changed.

Real compact startup subsequently exposed a separate AccessKit ordering defect that primary
viewport screenshots do not exercise. The actual native child check reproduced the abort:
the adapter was attached after showing the child. Child creation now starts hidden, attaches
AccessKit, then restores the requested visibility. The shared method also covers recreation.
The native example creates default, shown and hidden immediate children, validates native
scale and returns only after all callbacks execute. Debug assertions query the native
window's visibility where supported. It exited0 on the actual macOS host. The initial
example compile error and an unsupported egui visibility-metadata assertion are retained;
the latter was replaced with the native window check. This does not verify all native AX,
focus, keyboard, capture-exclusion or platform behavior. CI runs it separately even when
primary fixture capture fails.

The third hosted primary run still failed Settings after requesting capture, without a
screenshot event. Its trace and independent source review remain preserved. The source
review found that periodic repaint stopped after the request, while WGPU readback callbacks
need later submissions or polling. ROOT now continues the existing100ms repaint interval
until the screenshot event arrives, issuing the capture command once. Six idle logic ticks
failed before that correction and pass afterward with exact event/job consumption. The
existing30-second capture bound and visual tolerances are unchanged.

At PR13 head `45f2428`, [three-platform Rust CI](https://github.com/thematheusmello/cueglint/actions/runs/44077169925)
passed tests, formatting, registry and progress checks. The fourth
[hosted native run](https://github.com/thematheusmello/cueglint/actions/runs/44077170066)
passed all six primary fixtures and its separate actual child-startup step. The downloaded
309,882-byte ZIP matches GitHub's SHA-256; all six PNG dimensions/hashes and executable/source
endpoints match their receipts. Settings completed in3.426 seconds. The artifact and child
output hashes are retained in `native-fixture-ci-004.json`; earlier failures remain. This
supports the repair without isolating the sole cause or establishing a matched visual pass.

`native-package-release-002.json` records the actual release-mode package at source45f2428:
ad-hoc signature, bundle metadata, icon and executable checksums passed. The compact native
assistant launched with Accessibility controls, then was quit before model evaluation.
Screen capture still reported access needed despite the existing OS switch being ON.
This is not notarization, clean installation or a completed screen/audio workflow.

The [independent native visual diagnostic](../ui/reviews/native-runtime-002.md) compared six
working-executable frames with eight private reference captures. It identifies input baseline,
heading/chip and Settings spacing repairs, while preserving unknown font/DPI/blur values and
state mismatches. Its redacted finding ledger retains hashes without private pixels or paths.
The clock/random-identity fixture limitation must be addressed in a new versioned fixture,
without rewriting old evidence or changing production sorting.

The actual Metal file-ASR evaluation is separate from ordinary tests and the packaged app.
Its fixed28 calls and three loads passed the predeclared warm gates, with a10.58-second first
base warmup retained prominently. All output transcripts equal the CPU baseline; earlier CPU
speed/accuracy failures remain. See [models](models.md), `metal-asr-actual-002.json` and its
independent review for exact identities, per-call results and limits. No live-capture,
sustained-use or reference-quality claim follows from this development worker.

## Native transport and audio finalization repair

Source00ff533 passed253 workspace tests, zero failed, three configured-weight/oracle
ignores and zero filtered. Full354-file pre/post manifests, Cargo.lock and the test
log match the retained source. Formatting, registry and generated-progress checks
passed. A separate evaluator checked source identities, submission ownership and
audio cleanup; executions belong to Root.

Seven new audio tests cover both source-stop attempts, closing callback delivery,
bounded final drain, malformed packets, finalized utterance delivery, ASR errors
and receiver disconnection. Native driver/callback shutdown and real dual-source
transcription remain unverified. These tests do not establish ASR quality.

The actual normal executable passed six legacy and seven native-v2 fixtures once
per set. ROOT plus one UUID binds each request through GPU copy submission,
readback and saved PNG. Independent review checked all 13 assets (10 unique PNGs).
Two initially occluded windows recovered while retaining their request. The old
source85 Markdown timeout remains failed; its cause is not established by this
new success.

Run both sets into separate fresh destinations:

```sh
cargo xtask native-screens --fixture-set legacy --output-dir /tmp/cueglint-legacy-new --report /tmp/cueglint-legacy-new/report.json
cargo xtask native-screens --fixture-set native-v2 --output-dir /tmp/cueglint-v2-new --report /tmp/cueglint-v2-new/report.json
```

CI triggers on fixture data and runs both sets, retaining each result and failure.
[Workspace](../parity/reports/desktop-repair-workspace-001.json),
[source review](../parity/reports/independent-screenshot-audio-source-review-001.json),
[native result](../parity/reports/native-desktop-repair-001.json) and
[independent frame review](../parity/reports/independent-native-desktop-repair-001.txt)
are implementation-only evidence. OS input/focus/IME/AX, blur/compositing, capture
exclusion and matched reference comparisons remain open.


The General layout regression now holds the hero at y60 across different embedded
font metrics and requires the real accessible label value. Native screenshot requests
wait for registered fonts to be applied and one completed root UI pass. Receipts bind
font-set identities and record names, indices, hashes, coordinates and fallback; this
is not glyph-level or reference font matching. Headless constructors start no font
file job. The existing platform-file loader remains distinct from the OS resolver.

`general-readiness-backport-workspace-002.json` records257 passing tests atcdac6d7;
its earlier CLI lock failure and deterministic pre-repair duplicate-handle failure
remain separate reports. Store now explicitly unlocks after SQLite closes, preventing
an extra handle from extending closed ownership. Unlock errors in Drop are best effort.
Later source heads and actual distribution/device checks require their own evidence.

The [PR16 hosted receipt](../parity/reports/pr16-hosted-ci-artifact-001.json) binds
head `533d528`, CI merge `d802fa3` and main merge `37da11d` to tree `3bca981`.
The [PR17 receipt](../parity/reports/pr17-hosted-ci-artifact-001.json) binds head
`d28b390`, CI merge `a924ca8` and main merge `76a2d81` to tree `1a9b9d5`.
The [PR18 receipt](../parity/reports/pr18-hosted-ci-artifact-001.json) binds head
`bcf6db2`, CI merge `7a788a9` and main merge `d78732f` to tree `6ffd902`.
Formatting, locked workspace tests, registry and progress checks passed on all
three platforms at these exact trees:

| Source | Ubuntu passed / ignored | macOS passed / ignored | Windows passed / ignored |
|---|---:|---:|---:|
| PR16 `533d528` | 255 / 3 | 257 / 3 | 253 / 3 |
| PR17 `d28b390` | 275 / 3 | 277 / 4 | 275 / 4 |
| PR18 `bcf6db2` | 297 / 3 | 299 / 4 | 299 / 4 |

Each job has zero failed and zero filtered cases. Counts sum actual result lines
within each platform; adding platforms would not yield a unique-test count.
PR17 Windows compilation and two native loopback schema/join helpers passed
without opening audio, alongside nine portable bridge tests. Those earlier trees
exclude the later microphone/shared-bridge source. PR18 compiled that source and
passed four Windows native helpers and thirteen portable bridge cases without
opening devices, plus the recorded synthetic verified-input filesystem cases.
Actual driver/device, permission, routing and sustained-use checks remain open.

All three exact sources also passed six legacy and seven native-v2 original primary
captures plus actual default/shown/hidden child startup with AccessKit enabled on
the identified macOS CI host. Independent artifact review verified source
fingerprints, all thirteen PNG/UUID/ROOT/copy/readback/font-ready/log bindings and
ten unique fixed views. The provider form is visible in these original fixtures.
Binary/font bytes are absent from the archives; their stable hashes are production
receipts, not independent byte rehashes. This establishes no native input/AX,
child pixels, OS blur/composition, capture exclusion, model quality, release or
matched reference pass.

PR16 native attempt 1 remains cancelled before runner allocation: run
`44163918502`, job `134324817805`, runner 0 and zero steps, with the official
capacity annotations retained. Attempt 2 at the same source passed in job
`134346562139`. This targeted infrastructure retry did not repair an application
assertion. Earlier real layout, Store-lock and timeout failures remain preserved.

The [actual highlights run 001](../parity/reports/highlights-quality-actual-001.json)
failed both original cases under the frozen protocol at `d28b390`; each completed
generation was rejected for selecting more than eight utterances. There was one
attempt per case and no retry, output repair or fallback. Salience is `not_scored`;
acceptance/append is `not_run`, with zero attempts. No candidate or generated notes
was stored, and existing notes/title/summary/source rows were preserved. Its
[independent audit](../parity/reports/independent-highlights-quality-actual-001.json)
checks retained command streams, ownership/reaping, source/model endpoints,
expected prompt identities and immutable database comparisons. The audit did not
rerun the model. Generated arrays, token counts and engine-emitted prompt/config
metadata remain unknown; external hashes do not replace them. Completion durations
include setup/initialization/validation, not a model latency distribution.

The later [local reliability workspace run](../parity/reports/local-reliability-workspace-001.json)
at `5845d00` passed 299 tests, zero failed, four ignored and zero filtered in
37.27 seconds, using locked offline Cargo resolution and two test threads. Root's
399-file source comparison reported no changes, and formatting passed. Its log
and sanitized proposal hashes are retained. This remains one implementation test
run: no new model inference, managed Metal loading, Windows microphone/native
device execution or reference comparison. Dependency-offline Cargo is not the
offline product acceptance scenario. Registry/progress and the later integrated
source need their own checks.

The earlier [2.1.29 metadata observation](../parity/reports/reference-2.1.29-metadata-001.json)
provided API settings bounds of 860×670 and a separate current main-display
backing scale of 1. Its successful screenshot was wholly transparent and AX had
no candidates. No readable label, font, text/control geometry or widget typography
was recovered in that attempt. Capture exclusion/offscreen state is not an established
cause, and observer permission preflight does not establish reference-app
permissions. The chosen 2.1.28 baseline and its historical scale/font uncertainty
remain unchanged. No matched typography or visual-parity claim follows.

The [active delivery goal](delivery-goal.md) retains the complete eight-part scope,
the original six delivery gates and all twelve acceptance families. These new
reports are narrower evidence; they do not promote registry parity states.


The [second actual highlights evaluation](../parity/reports/highlights-quality-actual-002.json)
at source5845d00 changes the notes-only decoder and verified-input implementation
under a separately identified pre-output amendment. Both once-only model calls
produce valid source-bound JSON with actual end-of-turn and decoder-policy metadata.
Both fail the unchanged0.80 salience precision/recall gates: precision0.50,
recall0.667, category coverage0.60 and critical coverage0.75. Ready drafts remain
unaccepted; acceptance/append is not run. Source/model/binary/configuration/hardware
endpoints match and all85 launched children are reaped. No retry/repair/truncation,
new held-out semantic task, latency distribution or reference pass is claimed.
The [independent source reviews](../parity/reports/independent-local-reliability-source-reviews-001.json)
cover managed inputs, Windows microphone and bounded notes decoding; passing those
reviews cannot upgrade either actual quality failure.

An [independent audit](../parity/reports/independent-highlights-quality-actual-002.json)
recomputed those failed semantic scores and checked all170 streams,85 command
records,255 journal events,six endpoint sets,399 frozen-source Git blobs and
persisted data. Its first checker assumed an empty modes list; the recorded
source-seeded builtins corrected that audit assumption without changing product
code, golds, thresholds or results. Both semantic failures remain failed.

The later [visible General observation](../parity/reports/reference-general-visible-2.1.29-001.json)
records readable top/lower views with supported Undetectability temporarily off,
then restored on. Its [reviewed specification](../spec/features/reference-general-visible-2.1.29.md)
and [measurements](../ui/measurements/reference-general-visible-2.1.29-001.json)
retain window-relative control/text rectangles, independent display scale and
encoded/ICC-derived color samples. Used font family, point size and computed
metrics remain unknown. The original transparent attempt is retained; this later
controlled observation does not diagnose it or change the 2.1.28 baseline.

The [third actual highlights evaluation](../parity/reports/highlights-quality-actual-003.json)
at `94848291` uses generic salience prompt v2 with the unchanged notes-only decoder,
model pins, two original cases and fixed scoring gates. Both single generations
again select indices 0 through 7 and fail salience: precision 0.50, recall 2/3,
category coverage 0.60 and critical coverage 0.75. Exact support and plan coverage
are 1.0, but useful selection has not improved. Ready drafts remain unaccepted;
acceptance/append and conditional lifecycle checks are not run. The independent
[audit](../parity/reports/independent-highlights-quality-actual-003.json) verifies
409 frozen source files, six identity endpoints, 85 command records, 170 streams,
255 journal events, preserved user data and complete owned-leader reaping. Prior
actual001/002 failures remain failed. This repeated-fixture regression run is
neither held-out quality nor matched reference acceptance.

The [salience-v2 workspace receipt](../parity/reports/salience-v2-workspace-001.json)
at `a7501e7` records 300 passing tests; the separate
[UI-storage receipt](../parity/reports/ui-storage-workspace-001.json) at `22a6590`
records 303. Each has zero failures/filtered tests, four ignores, unchanged source
endpoints and normal owned-process cleanup. These are weight-free implementation
runs, not model quality, actual UI interaction or offline-product acceptance.

The [native provider UI attempt](../parity/reports/native-provider-ui-002-failure.json)
at `22a6590` failed before endpoint/model/key entry: the provider menu persisted
after AX selection, followed later by missing content AX/stale paint. The sampled
main thread was in idle AppKit event wait, which does not prove a global deadlock
or diagnose the later accessibility loss. Normal Command+Q quit the app. There
were zero HTTP requests and Keychain mutations. This one controlled launch did
create the custom database's UI sidecar while all five checked default data
endpoints remained unchanged. Native provider behavior after later fixes still
requires separate evidence; the original failure is retained.

The [provider-menu workspace check](../parity/reports/provider-menu-workspace-001.json)
at `136a7c7` passes 304 tests with four ignores, zero failures and zero filtering.
Its eight-case regression covers semantic AX/Enter activation for new selections
and reselection in both menus. This is implementation evidence; the separate
[native follow-up](../parity/reports/native-provider-ui-004.json) observes the
provider menu close in the real macOS executable and uses ordinary pointer focus
and keyboard input for the form.

That native launch checks a scoped synthetic key from missing through masked
entry, save/clear/stored, deletion and missing again. Local remains active during
save, model listing and the connection probe. Explicit provider selection shows
the destination; one original question displays the synthetic `OK` answer. Its
ended session and completed response persist in the isolated database. Local is
restored before normal quit, both owned fixture servers exit, and all five checked
default data endpoints are unchanged. The [independent review](../parity/reports/independent-native-provider-ui-004.json)
checks original captures, source, persisted data and bounded cleanup evidence.

The [first fixture in that launch](../parity/reports/native-provider-ui-003-failure.json)
omitted a terminal `finish_reason` event. Its connection test failed with an
incomplete-response error; a separately identified corrected fixture adds the
required terminal event without changing application validation. AX Click then
keyboard entry also failed before pointer focus recovered ordinary typing. Neither
failure is erased by the later narrow success. No model inference, live vendor,
audio/screen capture, full offline meeting or matched visual acceptance ran.

The [PR19 hosted receipt](../parity/reports/pr19-hosted-ci-artifact-001.json)
binds head `efc542be`, CI merge `4aaf778d` and the later main merge `2d859e39`
to tree `629d603e`. The independent audit observed a draft, open PR; the later
Root merge annotation is recorded separately. Ubuntu passed 302 tests with three
ignores, macOS/Windows each passed 304 with four ignores; all jobs had zero
failures/filtering. Thirteen original primary-viewport captures, ten unique
inspected images and separate native child startup passed their recorded binding
checks. Five cancelled earlier push/PR runs and the metadata-tool allowlist error
remain retained. This source predates the capture-input selectors and cannot
verify the later increment, native devices, model quality or reference parity.

The [capture-input targeted run](../parity/reports/capture-inputs-targeted-workspace-001.json)
at `1fa113e` passed 250 tests, zero failed/filtered and four ignored in 27.397
seconds. Its final formatting check passed, with all 422 source-file endpoints
unchanged. The distinct [integrated workspace run](../parity/reports/capture-inputs-integrated-workspace-001.json)
at `e4a46779` passed 312 tests across 29 terminal suite summaries, zero failed/filtered
and four ignored in 38.287 seconds. Read-only Git comparison establishes equal
`crates` subtrees at these two named commits; it does not equate whole-repository
fingerprints or extend the claim to later source/copy changes.

The tests cover native-widget commands into real Store settings and typed capture
configuration, missing/legacy-ID preservation and explicit recovery, busy-capture
rejection, bounded refresh cancellation/stale results, and retiring screen work
when the selected display changes. The native font probe, configured Whisper and
Qwen tests, and external numeric oracle remain ignored. No hardware enumeration,
audio/screen capture, packaged native local workflow, current permission state,
offline-product traffic check or broad model-quality pass follows. Later native
observations and their pending independent audit require their own evidence;
previous screen denial, wrong-display capture, provider and model-quality failures
remain preserved.

## Provider fixture readiness repair

[PR20 head 9a4fcc5](../parity/reports/pr20-hosted-ci-artifact-failure-001.json)
passes hosted Rust 310/312/312, but its native gate fails. Twelve original frames
and bounded child startup succeed; provider-v2 reaches the 30-second deadline
without a screenshot request, receipt or PNG. First-only font-loading traces do
not establish its final state or cause. The missing provider frame remains failed.

[Source b9e1e44](../parity/reports/provider-scroll-workspace-001.json) defers the
fixture's semantic scroll until the adopted fonts have completed a ROOT pass.
The existing password, charges and panel-top visibility gate stays unchanged.
Two once-per-stage diagnostics distinguish font and content readiness. The new
regression exercises logic polling and preceding-content reflow. Its RED run fails
on the old reveal behavior. The first corrected workspace then catches a stale
six-stage test assertion; the exact eight-stage success mask fixes that expectation
and retains the finite-diagnostics check. Both failed logs remain hash-bound.

The final local workspace passes 312 tests, zero failed/filtered and four ignored
checks in 29 summaries. All 435 tracked source hashes independently match the
runtime commit; log SHA256 is
`2c71ecd37ca460ae34e19a892cf0ba426af46372b2c924398426fba456214123`.
This is implementation evidence. Fresh hosted/native artifacts, actual input/capture,
offline operation and reference matching remain separate. The signed app waiting
for OS authentication was neither replaced nor relaunched by this repair.

## Bounded DOCX integration

[Source1236843](../parity/reports/knowledge-docx-integrated-workspace-001.json)
combines the reviewed DOCX increment with the previous font/readiness repair.
The full local workspace passes 325 tests, zero failures/filtering and four retained
ignores across 31 summaries. All 452 tested Git blobs independently match the
commit. Twenty candidate files are unchanged; the application additionally contains
only the separately verified font patch. Core extraction/storage/retrieval/deletion,
real CLI DOCX operations, AX disabled/click handling, stale UUID/cancellation and
preserved notes/data are covered. [Source audit](../parity/reports/knowledge-docx-source-001.json)
retains limits and the empty-paragraph/tab-stop/enabled-state repairs.

The initial core check failed because independently produced backups differed only
in their exported_at timestamp by 1ms. That failure remains hash-bound; its corrected
assertion requires a positive, nondecreasing export time and equality of every
other versioned field. The new 325-test result is implementation evidence. Native
import, Windows/Linux runtime, broad OOXML, PDF and reference acceptance remain open.

## PR21 hosted DOCX checks

[The independent audit](../parity/reports/pr21-hosted-ci-artifact-001.json) binds
final head `882b23e6` and CI merge `3cbc4262` to tree `43774786`. The later merged
main `18551b8` has the same tree; the audit's draft/open metadata snapshot predates
that merge. Runtime code remains identical to reviewed integration `1236843`.
Ubuntu passes 323 tests, macOS and Windows each 325, with 3/4/4 explicit ignores
and zero failures/filtering across 31 summaries per platform. All thirteen original
core/CLI/worker/headless AccessKit DOCX regressions and the two retained provider
font/screenshot regressions pass on each platform. Required formatting, workspace,
registry and generated-progress steps succeed at the audited head.

The original 837286-byte ZIP artifact `14394939608` has SHA256
`06d7abf139929dd39f4894d1bd2e989644722677a4d4e3a15d6ec4490cca4bb9`.
All thirteen primary-window fixture PNGs have distinct request UUIDs and verified
ROOT/copy/readback/receipt/font/source bindings; ten unique originals were viewed.
The full empty-key provider panel is visible after font/content readiness. Separate
native child startup succeeds. Independent source fingerprint
`9cf534a3dfd7b3c53f5256c166a5e4b89af12c919f72bc3453315ebf6c1ece7d`
covers 246 paths and 73 contracts. Executable and font payload bytes are absent;
their identities remain runner/app receipts. Five superseded or redundant runs
remain cancelled at attempt 1, and the earlier backup-timestamp failure stays
retained. This is implementation evidence, not interactive native DOCX import,
input/focus, live hardware/capture, model quality, PDF, offline or reference parity.

## Neutral-penalty selection diagnostic

[Six reviewed runs](../parity/reports/selection-diagnostic-actual-v5-001.json)
compare masked/unmasked Candle and unmasked official llama.cpp across two orderings
of one original fictional fixture. All controls complete; all six exact diagnostic
target matches are false. Forward unmasked outputs agree exactly, while reverse
outputs differ. The result reproduces a failure outside masking/Candle without
proving capacity as the sole cause or a specific backend bug. Repetition 1.0 differs
from production 1.1, so this is neither quality003 nor a held-out quality result.

A prior v4 batch stopped on a mis-specified native cache field. Pinned source
separates prompt reuse in timings.cache_n from final retained history in top-level
tokens_cached. V5 corrects only that measurement and requires typed reuse 0 and 425
processed prompt tokens, with unchanged parser/settings/raw/ownership/deadline and
observed token-cap controls. V4's failure and five unrun arms remain retained. All
six v5 outputs, counter/EOS conventions, token 128247 warning, requested/log-only
native CPU placement and recorded retirement limits remain in the independent
audit. Defaults and failed production highlights quality are unchanged.

## Model download phase budgets and actual recovery

[The phase-budget report](../parity/reports/model-download-phase-budgets-001.json)
binds runtime `56d885c` to 330 full release-workspace passes, four explicit ignores,
zero failures/filtering and 456 independently matched source blobs. A retained old
ureq 3.2.0 RED run has two failures: a legitimate body outlives its header budget,
and a trickling body outlives its total body budget. The same regression bytes pass
with pinned ureq 3.5.2. All 20 model-module and 17 controlled provider checks pass;
production timeout values, model defaults, prompts and provider client are unchanged.

[The actual recovery report](../parity/reports/model-install-recovery-001.json)
retains the original 30.46-second transfer failure and an independent strict-canary
preflight failure with zero model calls. The corrected probe keeps the same OS
permission-denial criterion. One subsequent install call completes in 126.13 seconds;
three pinned artifacts total 4,690,117,228 bytes and pass size/SHA checks. Metadata and
verification run under a tested process network-denial policy; intentional model
acquisition is separate. Six CLI and two canary groups retire; settings/defaults,
source and frozen native executable remain unchanged. Raw HTTP Range response
headers were not retained and no wire-byte claim is made. This does not establish
model initialization, usefulness, memory, native cancellation, full offline sessions,
platform hardware or reference parity.

## PR22 hosted model-download checks

[The independent audit](../parity/reports/pr22-hosted-ci-artifact-001.json) binds
head `6183c223` and CI merge `c355912b` to tree `ebffc2b7`. A later Git check
confirms merged main `a38f047` has the same tree; the audit's open/draft snapshot
predates that merge. Ubuntu passes 328 tests, macOS and Windows each 330, with
3/4/4 explicit ignores and zero failures/filtering across 31 summaries each.
Formatting, workspace, registry and progress steps all succeed. The original
passing test names retain the thirteen DOCX/import regressions, two provider
regressions and five profile/timeout regressions. Ignored font, ASR/oracle and
local-generation checks remain ignored. Progress remains 19/73 observed, 4/73
implemented and 0/73 verified.

The original 837359-byte ZIP artifact `14402715140` has SHA256
`c4b86c9b6b822162a6a90d47134d8d420d70835d6d14bc91be522616d82ee194`,
matching GitHub's digest. All thirteen frames have distinct request UUIDs and
verified ROOT/copy/readback/receipt/font/source bindings; ten unique originals
were inspected. The full provider panel passes its unchanged visibility gate
after font/content readiness. Separate native child initialization succeeds.
The independently replayed source fingerprint
`9550adbd0bfaff3cad057b080921c5c8871c8e5b78cbe421d132064ec098685e`
covers 246 paths and 73 contracts. Application binary and font payload bytes are
absent; those identities remain receipts, and reference font matching is unverified.
Four superseded/redundant concurrency cancellations and the earlier failed checks
remain retained. CI does not establish 7B initialization/quality, resource readiness,
interactive import, native input/focus, devices/capture, offline or reference parity.

The subsequent first eight-case 7B candidate attempt, A001, stops in monitor
preflight with zero CLI/model calls and all eight cases unrun. A v3 controller-only
process-monitor correction passes source/synthetic review. This preparation
does not change models, defaults, scoring or the predeclared resource gates.

## Strict PDF implementation checks

[The targeted runtime record](../parity/reports/knowledge-pdf-targeted-runtime-001.json)
binds the isolated correction `14142ba27d41855bef30a2ce4e2b24833ab1db10` to six
named CLI tests: one DOCX and five PDF, with zero failures, ignores or filtering.
These exercise the actual CLI and allocation-budgeted self-child, original
Unicode/multipage and modern-stream input, retrieval byte offsets, deletion/context
invalidation, backups/notes, all fixed bounds and whole-document rejection.
The separate `harness = false` supervision executable passes actual parser dispatch,
six controlled IPC failures, readiness before cancellation, the unchanged ten-second
deadline and child reaping before return. Its one self-check is recorded separately
from named Rust test totals. The earlier three selected core passes, with two
filtered out, belong to the preceding `f0774e8` source and are not added to those six.

The [retained failures](../parity/reports/knowledge-pdf-external-stream-red-001.json)
include a first CLI attribution assertion whose broad query also matched preserved
DOCX text. Only that query changes to its original unique token; every offset,
backup, note and deletion assertion remains. The later external-stream RED fails
one test with four filtered out: direct `F` inline data was wrongly accepted, and
the other external cases were not reached. The shared decoder now rejects parsed
`F`, `FFilter` and `FDecodeParms` before consuming bytes. All four direct/escaped,
file-specification and external-filter cases are reached and rejected in GREEN.
Neither the earlier failure nor the correction establishes external file reads.

The [PDF contract](../spec/features/native-knowledge-pdf.md) keeps 2 MiB input/text,
a ten-second parent deadline and a child-only 256 MiB cumulative Rust allocation-work
budget. It does not cap process RSS or native allocation. Strict raw/single-Flate
streams, font/CMap mappings and complete text are required; scans/OCR, forms/images,
annotations, marked content, unsupported filters/fonts, incremental/linearized PDFs
and layout reconstruction remain outside the supported profile. No partial library
entry is inserted after failure. Existing UTF-8/DOCX and storage schema remain intact.
The [dependency/typecheck record](../parity/reports/knowledge-pdf-compile-002.json)
retains its disk-floor preflight failure and original notices. Two upstream notices'
blank EOFs remain reported; their exact bytes are preserved.

Those isolated checks predate integration onto PR22's newer HTTP-client source.
The [integrated record](../parity/reports/knowledge-pdf-integrated-workspace-001.json)
binds `778ada4a7b3e5e2442ae79a92cd850f401c90856` to 339 passing workspace tests,
zero failures/filtering and four explicit ignores across 32 summaries, plus a
separate actual PDF supervision self-check. The headless document-import quit
and failed-snapshot start-order regressions both pass. The preceding test-only
`43c87f0` run has zero passes, one failure and 132 filtered; ordinary cancellation
acknowledgements prematurely permitted Close before the caller repair. Both close
gates now await the matching import completion, and the initial snapshot succeeds
before a parser starts. This establishes the source/headless normal-quit policy,
not an actual native-window quit or forced-process-death cleanup guarantee.

Actual desktop import/quit, Windows build/runtime, broader PDF, citation/model
quality, offline and matched-reference acceptance remain open. The current packager
copies binaries, icon and plist but does not bundle the retained third-party notices;
notice bundling and resulting binary-package validation remain distribution work.
No feature state or acceptance threshold is promoted by these implementation checks.

## PR23 hosted PDF/caller checks

[The independent hosted audit](../parity/reports/pr23-hosted-ci-artifact-001.json)
binds exact head `9744d97c` and CI merge `cfc56fe4` to tree `f2ec0f8b`. A later
Git check confirms merged main `57d81ab` has the same tree; the audit's open/draft
snapshot predates the merge. Windows and macOS each pass 339 tests and Ubuntu 337,
with zero failures/filtering and 4/4/3 explicit ignores across 32 summaries each.
All 29 selected import/provider/profile/timeout/PDF/caller regressions pass. Each
original platform log has one separately counted `harness = false` PDF supervision
receipt. Formatting, workspace, registry and progress steps all succeed. Progress
remains 19/73 observed, 4/73 implemented and 0/73 verified.

The original 842229-byte ZIP artifact `14408142877` has SHA256
`959fb07d831a27d96d663627f418f8222acf025d64a2f6cda5665917dc4f20ad`,
matching GitHub's digest. All thirteen frames have distinct request UUIDs and
verified ROOT/copy/readback/receipt/font/source bindings; all ten unique originals
were viewed. The full provider panel meets its unchanged visibility gate after
font/content readiness. Native default/shown/hidden child initialization also
passes. The independently replayed source fingerprint
`b9f3d04035175c786e988784345780366a439802baf480e9a5e452e400d089c0`
covers 320 Git paths and 73 contracts. Executable and font payload bytes are absent;
their identities remain receipts and reference font matching remains unverified.

All five concurrency cancellations remain cancelled at attempt 1. Earlier PDF
query/external-stream and caller-quit RED failures remain retained in their source
records. These hosted controlled CLI/process tests and fixture frames do not
establish actual native import/quit, input/focus, hardware, model quality, resource
readiness, offline, release-package notice bundling or reference acceptance.

## Runtime notice consumer

[Implementation-only consumer evidence](../parity/reports/runtime-notice-consumer-001.json)
binds clean source `1dc46ede6732c818aaa394a2edb5c8ae87d421d3`, tree `716d719e`,
on macOS 27.2/26B123 arm64 and Rust 1.94.1. Formatting and all 26 xtask tests pass,
with zero failures, ignores or filtering. These check source/configuration policy,
notice reference closure, bounded path/tree validation, both synthetic notice
outputs and exact hashes. [Independent review](../parity/reports/runtime-notice-consumer-independent-001.json)
verifies all twelve inputs and 342 source payloads, 899 references to 340 unique
payloads, unchanged five regression bodies and unchanged application/defaults.

The first candidate stopped at a test-fixture borrow compiler error; no tests ran.
After the one-character fixture repair, source `bb00789` actually fails all five
boundary regressions (0/5, 19 filtered). Source `041e338` passes those regressions
but fails the full real-inventory test (25/1) because the reviewed public README
was missing from the explicit source allowlist. Only that exact path was added;
`1dc46ed` passes all 26 checks. All earlier failures and hashes remain retained.

A separate actual invocation of the newly compiled `xtask package` exits 1 with
all three declared inventory blockers. Controlled child-command markers verify
that neither Cargo nor codesign was invoked; no `dist` is created and the manifest
is unchanged. This is expected refusal evidence, not package acceptance.

The source inventory is a conservative 442-package metadata candidate supergraph,
not exact linked-runtime coverage. `bundle_ready` remains false for two missing
original texts and mandatory attribution/coverage decisions. The consumer uses
2MiB manifest/per-file, 1024-file and 16MiB aggregate payload bounds, exact twelve
configuration identities and explicit public notice roots. A finite declared-tree
walk rejects unknown directories before traversal. Both manifest/payload trees
are copied before signing; their hashes are checked after signing. Source includes
locked Cargo JSON artifact selection rather than stale `target/release` paths.
These package call sites are reviewed; no successful bundle/sign/clean-install
run is claimed. Partial `dist` output after a future packaging error remains an
explicit release-staging gap. Windows/Linux hosted and release checks remain
separate. No native UI, audio, model, provider, offline or reference check ran here.

## Streaming audio resampling

[Original synthetic DSP and caller evidence](../parity/reports/audio-resampling-runtime-001.json)
binds source `1f65cb1dfad365ba62a3848f21a7f6308d117345`, tree `14eb6766`,
on macOS 27.2/26B123 arm64 with Rust 1.94.1. The local audio filter runs35 tests
(55 filtered); desktop audio callers run five (129 filtered), with no failures or
ignores. All 14 new tests and every preexisting assertion remain unchanged after
scope freeze. A separate full release/offline/locked workspace run passes353 tests,
zero failures/filtering and four explicit ignores across 32 summaries, plus one
separately counted PDF supervision self-check. This is the audio increment on
PR23 main; later PR24 composition is a separate source state.

All 60 actual Rust spectral rows meet the predeclared gates:34 passband,22 alias
and four image checks. Maximum absolute passband deviation is0.002193dB;
weakest alias/first-image suppression is80.869811/78.282515dB, versus fixed
0.25dB and40dB limits. The source-owned sinusoid/support/count/bit-partition
fixtures are original; neither thresholds nor coefficients were retuned after
outputs. The separate56 equation calculations remain design-only evidence.

[Independent targeted audit](../parity/reports/audio-resampling-independent-targeted-runtime-001.json)
binds all 543 Git-file identities and the guard/config/log/metric data. Raw Rust
unit times0.45s and rounded0.00s differ from commands131.974/114.352s including
compilation; the247.103s targeted guard and489.729s full-workspace guard are not
audio, ASR or answer latency. Owned process groups were reaped and source/frozen
executables remain unchanged.

No physical audio source, route change, acoustic echo, calibrated hardware clock,
ASR quality, live UI, permission, sustained-resource or reference acceptance ran.
The12s utterance cut remains packet-granular; RMS gating intentionally differs
from the raw resampler's all-sample count. Windows/Linux native runtime and package
claims require separate evidence.

## PR24 hosted notice checks

[Independent hosted audit](../parity/reports/pr24-hosted-ci-artifact-001.json)
binds head `aa7cc95`, CI merge `d3254d7` and tree `4ec7993a`. A later Git check
confirms merged main `7b01211` has the same tree; the original audit still records
its earlier draft/open snapshot. All required steps pass. Windows/macOS each
pass 352 tests and Ubuntu 350, with zero failures/filtering,4/4/3 ignores and32
summaries each. Each platform also passes its separate PDF supervision check;
all 42 selected regressions are present.

The original842524-byte native ZIP matches GitHub digest
`6c08349ff7f51a0a0c7ae2b42f37650b38492dca377e30c118be4b60ba1e8e97`.
All 13 frame/UUID/ROOT/copy/readback/font/content bindings and child startup pass;
all ten unique originals were viewed. Five cancellations remain retained. The
unchanged native source fingerprint covers321 paths/73 contracts; notice manifest
`aa3621d6` is bound separately. Actual packaging, complete attribution review,
model quality, native input/hardware, offline and reference acceptance remain open.

The first composed prepublication run remains
[failed at a model-download fixture](../parity/reports/audio-composed-model-fixture-red-001.json):
305 passes/one failure/one ignore across20 completed summaries before abort.
An owned loopback probe shows macOS accepted sockets inherit the mock listener's
nonblocking flag; reading before the remaining request bytes arrives returns
OS error35. The shared test fixture now explicitly restores blocking before its
existing timeout-bound reads. Production model code, every assertion and all
existing deadlines are unchanged. The failure and probe remain historical;
a repaired run is evaluated separately.

The repaired [frozen staged composition](../parity/reports/audio-composed-workspace-001.json)
passes 366 workspace tests with zero failures/filtering and four explicit ignores
across 32 summaries, plus separate PDF supervision. All five original download
fixture checks also pass with 85 filters. Formatting, registry and generated progress
checks pass. This evidence records HEAD, staged-tree and merge-parent identities
separately; it does not call the staged build a clean published-commit build.
Only generated progress files changed during checks. Final publication metadata
preserves compiled sources; hosted exact-head checks remain independent.

## Microphone PCM frontend checks

[Runtime evidence](../parity/reports/microphone-pcm-runtime-001.json) and
[independent full-workspace audit](../parity/reports/microphone-pcm-independent-workspace-001.json)
bind staged tree `bbd348fde09dc67b2147ead85cb246ee6d8fbd71`, with recorded
base HEAD 522507a/tree b14eaba. This was a frozen staged feature, not a clean-HEAD
PCM test claim. The later fast-forward to merged main 5f8add8 had the identical
base tree and preserved the staged index exactly.

All ten new PCM tests pass (90 filtered), inside 45 total audio tests (55 filtered),
plus five desktop caller tests (129 filtered). There are 50 distinct selected names,
not 60. Fmt passes. The full release/offline/locked workspace passes 376 tests,
zero failures/filtering and four explicit ignores across 32 summaries, plus one
separate PDF supervision check. All 817 indexed source blobs match the snapshots;
only reviewed microphone/module/contract changes are added. The existing
resampler/Endpoint assertions, Windows/system capture, selection/configuration,
models and runtime caller code remain unchanged.

The ten controls cover actual typed dispatch for 12 frontend formats, exact integer
normalization/float32 identity, first/last/frame partitioning, nonfinite rejection,
malformed data, overflow, queue pressure and mute/stop metadata. Command durations
include compilation; they do not measure callback or audio/answer latency.
The first private patch-preparation assertion failed before mutation when a closing
hunk marker was selected; line-start parsing corrected the preparation, and the
whole old BASE57 audio.rs was never copied over the current resampler.

No native audio devices or OS permissions were opened. Backend availability,
physical formats, realtime scheduling, route recovery, speech/ASR accuracy,
hardware clocks/echo, sustained power and reference/packaged acceptance remain
unverified. Current platform table entries remain unchanged.

## PR25 hosted audio checks

[Independent exact-head hosted audit](../parity/reports/pr25-hosted-ci-artifact-001.json)
records Windows/macOS/Ubuntu 366/366/364 passes, zero failures/filtering and 4/4/3
ignores across 32 summaries each, plus one separate PDF supervision receipt each.
All 83 common names and four Windows-only audio checks pass. Head 522507a, CI
merge ed3e075 and later main 5f8add8 share tree b14eaba. The original audit's draft
snapshot predates merge. All 13 native frames and child startup pass, including
independent inspection of ten original PNGs and matching ZIP digest. Five
cancellations and the prior composed model-fixture failure remain retained.
Those hosted fixtures do not establish native devices, actual ASR/model quality,
physical capture, offline meetings, package installation or reference parity.

The [notice provenance supplement](../parity/reports/runtime-notice-provenance-supplement-001.json)
is separately checked by [Root's staged source run](../parity/reports/runtime-notice-provenance-runtime-001.json)
and an [independent byte/provenance review](../parity/reports/runtime-notice-provenance-independent-001.json).
All 26 xtask tests pass with zero failures, ignores or filtering; formatting,
registry and progress checks pass. The exact 818-file snapshots and staged tree
remain unchanged, as do the frozen app/CLI identities. The three notice blockers
and false readiness gate remain. These checks establish no complete source-header
attribution, package/signing, native interaction or reference acceptance.

The [PR26 hosted audit](../parity/reports/pr26-hosted-ci-artifact-001.json) records
Windows/macOS/Ubuntu 376/376/374 passes, zero failures/filtering, 4/4/3 explicit
ignores and 32 summaries plus separate PDF supervision each. Head `c99041c`, CI
merge `eae5350` and later merged main `672dd36` share tree `81e1c00d`. Thirteen
native frames and child startup pass; ten distinct original images were viewed
and the original ZIP digest matches GitHub. Five cancellations remain. Its
open/draft snapshot predates merge; hardware, models, packaging and reference
acceptance remain separate.

The [notice source decision](../parity/reports/runtime-notice-readiness-001.json)
is supported by the [independent conditional input review](../parity/reports/runtime-notice-readiness-independent-001.json)
and the [376-test frozen composition](../parity/reports/runtime-notice-composed-workspace-001.json).
The later adopted-data tree separately passes all 26 xtask tests with zero
failures/ignores/filtering, plus formatting, registry and progress checks. All
source/index and protected-binary identities remain stable within each run.
The one-byte authored-wrapper whitespace failure remains retained. The true
source gate permits the exact-input/default-arm64 controlled package attempt;
actual artifacts, dual notice trees, signing and clean installation are unrun.

The later [local arm64 package](../parity/reports/macos-arm64-package-003.json)
binds source `9ae9063` to the app and CLI hashes, 347 files in each notice tree
and the exact 696-entry checksum manifest. Independent inspection verified both
arm64 executables and strict ad-hoc signing. The packaged CLI help ran successfully.
Clean install, notarization and release acceptance remain open.

[PR28's hosted audit](../parity/reports/pr28-hosted-ci-artifact-001.json) reports
386 passed tests on Windows and macOS and 384 on Ubuntu. Thirteen native fixture
frames and child startup passed. Its draft snapshot predates the merge into main
at `92a6bb8`, which has the same tree as the tested head and CI merge.

The [live settings pointer trial](../parity/reports/native-language-pointer-002.json)
used the packaged app and an isolated database. An actual pointer action saved
German. The on-disk value survived a normal quit and a new process showed German
on reopen. The earlier input misses and Tab moving focus to Search settings
remain in the report. This trial does not exercise capture or inference.

The [packaged multilingual file trial](../parity/reports/packaged-multilingual-asr-001.json)
uses two new authored eSpeak NG speech files and one run per file with the pinned
Whisper base weights. The verified install and process-level network canary
precede both CPU runs. English scores 0/18 word errors and passes the frozen
0.10 WER limit. Portuguese scores 12/19 and fails. Both commands return valid
language, model and frontend identities. These measurements include CLI setup
and model loading. Multilingual human speech, live devices and the full offline
session remain open.
