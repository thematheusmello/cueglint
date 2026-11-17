# Verification

```sh
cargo test --workspace
cargo fmt --check
cargo xtask verify-registry
cargo xtask progress
cargo xtask progress --check
cargo xtask visual /private/reference.png /private/cueglint.png --report /private/comparison.json
cargo xtask audio-fixtures
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

Historical CI failures remain in `parity/reports/ci-bootstrap.json`: macOS and Linux passed
implementation checks, while Windows exposed a hard-link export/database alias failure.
The cross-platform file-identity fix keeps the original regression test; new Windows CI
must verify it. These jobs do not run native GUI, capture, model weights or clean installs.
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
