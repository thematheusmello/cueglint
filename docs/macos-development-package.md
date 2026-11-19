# Download a macOS arm64 development package

Download the ZIP and matching `.sha256` file from `v0.1.0-dev.2` for Apple
silicon. The tag resolves to source `4c4d57badef0fa7daa870c8d4a41f78eacc06381`.
It includes native Calendar controls, upcoming events and offset-aware dates.
The two public assets matched the audited archive and checksum after an
anonymous download.
[Public prerelease](https://github.com/thematheusmello/cueglint/releases/tag/v0.1.0-dev.2) ·
[Verification report](../parity/reports/hosted-macos-arm64-development-004.json)

The audit verified strict ad-hoc signatures for both arm64 executables, both
349-file notice trees and all 700 checksum entries. The packaged CLI help ran
from an isolated directory with process networking and writes denied. The
original Actions artifact from run `44477592140` requires GitHub sign-in and
expires on **December 3, 2026 at 07:29:23 UTC**.
[Build artifact](https://github.com/thematheusmello/cueglint/actions/runs/44477592140/artifacts/14508420316)

The earlier `v0.1.0-dev.1` CLI at `6dccb46` answered two questions about original
saved meetings with the pinned local Qwen model under process network denial.
It froze the source excerpts and withdrew dependent answers after deletion.
Authored prompts and notes survived.
[Earlier prerelease](https://github.com/thematheusmello/cueglint/releases/tag/v0.1.0-dev.1) ·
[Earlier verification](../parity/reports/hosted-macos-arm64-development-003.json)

The prior `c48b499` archive exercised file ASR, a grounded local answer, export,
backup, restore and deletion. Its one-file accuracy check measured 30% word error
rate against a 10% limit.
[Prior archive report](../parity/reports/hosted-macos-arm64-development-002.json)

An [earlier archive](../parity/reports/hosted-macos-arm64-development-001.json)
has a separate native Settings, compact assistant, empty history and normal
quit report on macOS 27.2. That observation applies to source `b78c9ac`.

The prerelease provides the development ZIP and checksum directly. A download
from the Actions run wraps these same two files in an outer ZIP.

Verify the available archive and extract into a new directory on an Apple silicon
Mac. For another run, use its actual archive filename:

```sh
archive="cueglint-macos-arm64-dev-4c4d57badef0fa7daa870c8d4a41f78eacc06381.zip"
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

The packaged-fixture step launches the extracted signed executable with
`native-v2-calendar-reminder` in an isolated profile. It checks normal exit,
the 690 × 164-point Metal frame, and the matching screenshot trace. A separate
Actions artifact holds the fixture evidence. The first hosted execution of
this check is pending.

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
