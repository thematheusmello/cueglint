# Download a macOS arm64 development package

[Download the verified development artifact](https://github.com/thematheusmello/cueglint/actions/runs/44456045684/artifacts/14498617918)
from successful run `44456045684`. It was built from exact source
[`c48b499b6f443e8826e864b5d57e36cc9d91a926`](https://github.com/thematheusmello/cueglint/commit/c48b499b6f443e8826e864b5d57e36cc9d91a926)
and is named `macos-arm64-development-c48b499b6f443e8826e864b5d57e36cc9d91a926`.
GitHub records expiry on **December 3, 2026 at 04:22:37 UTC**. Sign in to GitHub
to download it. The archive predates the later Settings Back and Forward controls.

The [verification report](../parity/reports/hosted-macos-arm64-development-002.json)
records independent checks of all 700 checksum entries, both exact 349-file
notice trees, arm64 executables and strict ad-hoc signatures. The downloaded
CLI started with network and writes denied. A separate run with local test data
and network access denied transcribed an original WAV with the installed model,
answered a grounded question, saved one meeting and verified its export, backup,
restore and deletion. The one-file accuracy check measured 30% word error rate
against a 10% limit.

An [earlier archive](../parity/reports/hosted-macos-arm64-development-001.json)
has a separate native Settings, compact assistant, empty history and normal
quit report on macOS 27.2. That observation applies to source `b78c9ac`.

Unzip GitHub's artifact download to obtain:

- `cueglint-macos-arm64-dev-<full commit SHA>.zip`
- The matching `.zip.sha256` file.

Verify the available archive and extract into a new directory on an Apple silicon
Mac. For another run, use its actual archive filename:

```sh
archive="cueglint-macos-arm64-dev-c48b499b6f443e8826e864b5d57e36cc9d91a926.zip"
shasum -a 256 -c "$archive.sha256"
mkdir extracted
ditto -x -k "$archive" extracted
(cd extracted/dist && shasum -a 256 -c SHA256SUMS)
codesign --verify --deep --strict extracted/dist/Cueglint.app
codesign --verify --strict extracted/dist/cueglint-cli
```

The extracted `dist` contains `Cueglint.app`, `cueglint-cli`, its
`cueglint-cli.NOTICES` sidecar and `SHA256SUMS`. Keep the notice sidecar alongside
the CLI when moving it. The workflow verifies the archive's extraction, executable
permissions, arm64 binaries, both exact notice trees, checksums and strict ad-hoc
signatures before upload. `ditto` preserves the app metadata inside the ZIP; direct
artifact upload does not preserve executable permissions.

The archive is for arm64 development testing and uses separately installed
models. The fresh CLI profile was tested on an existing macOS 27.2 development
machine. The clean OS install, Gatekeeper, notarization, update and complete
live capture workflow remain open. Capture protection was unavailable during
the earlier GUI check, with recipient-side capture still untested. The actual
minimum supported macOS and complete platform matrix need release testing.

To build another archive, open **Actions → macOS arm64 development package →
Run workflow**, choose **main**, and run it. Repository write access is required.
The workflow uses `cargo xtask package` with Rust 1.94.1 on the arm64 `macos-15`
runner. It selects Xcode 26.3, checks the macOS SDK and C/Swift toolchains, and
passes the selected `SDKROOT` to subsequent steps. Artifacts are retained for
14 days under `macos-arm64-development-<full commit SHA>`.

Earlier failures remain recorded. [Run 44384847084](https://github.com/thematheusmello/cueglint/actions/runs/44384847084)
failed while compiling the Metal Swift bridge. Its selected Xcode version remains
unknown. [Run 44391784828](https://github.com/thematheusmello/cueglint/actions/runs/44391784828)
had no assigned runner in attempt 1, then failed SQLite C compilation on
`stdio.h` in attempt 2. The C build failure reproduced on the development Mac until the selected SDK
path was exported. [Hosted C build failure](https://github.com/thematheusmello/cueglint/actions/runs/44391784828/job/135121138343) The [original SDK failure record](../parity/reports/macos-hosted-sdk-failure-001.json)
and [original runner toolchain listing](https://github.com/actions/runner-images/blob/macos-15-arm64/20261112.0539/images/macos/macos-15-arm64-Readme.md#xcode)
remain available.

The runner and artifact behavior follows GitHub's
[runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[manual workflow instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow),
and [upload-artifact v4 permission guidance](https://github.com/actions/upload-artifact/tree/v4#permission-loss).
