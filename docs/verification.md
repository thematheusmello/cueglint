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
readback and saved PNG. Independent review checked all13 assets (10 unique PNGs).
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
Current-head CI/native and actual distribution/device checks remain separate.
