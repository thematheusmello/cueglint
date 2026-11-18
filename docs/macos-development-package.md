# Download a macOS arm64 development package

[Download the verified development artifact](https://github.com/thematheusmello/cueglint/actions/runs/44400700716/artifacts/14473932623)
from successful run `44400700716`. It was built from exact source
[`b78c9ac116c788440ac71d6e45f31e9ea0e3ca50`](https://github.com/thematheusmello/cueglint/commit/b78c9ac116c788440ac71d6e45f31e9ea0e3ca50)
and is named `macos-arm64-development-b78c9ac116c788440ac71d6e45f31e9ea0e3ca50`.
GitHub records its expiry as **2026-12-02 at 21:38:50 UTC**. Sign in to GitHub
to download it. Newer source commits need separate package evidence.

The [verification report](../parity/reports/hosted-macos-arm64-development-001.json)
records independent checks of all 696 checksum entries, both exact 347-file notice
trees, arm64 executables and strict ad-hoc signatures. The downloaded CLI's help
and the app's Settings, compact assistant, empty history and normal quit were
checked in a fresh app profile on an existing arm64 macOS 27.2 machine, with no
models or sessions present.

Unzip GitHub's artifact download to obtain:

- `cueglint-macos-arm64-dev-<full commit SHA>.zip`
- The matching `.zip.sha256` file.

Verify the available archive and extract into a new directory on an Apple silicon
Mac. For another run, use its actual archive filename:

```sh
archive="cueglint-macos-arm64-dev-b78c9ac116c788440ac71d6e45f31e9ea0e3ca50.zip"
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

The archive is for arm64 development testing and uses models you install
separately. This first launch used a fresh app profile on an existing development
machine. Clean OS installation, Gatekeeper, notarization, updates/rollback, model
setup and a full meeting workflow remain open. Capture protection was unavailable
on that host. Recipient-side capture remains untested. Mach-O records deployment target `11.0` and SDK `26.2`. Launch testing covers
the development Mac running macOS 27.2. The actual minimum supported macOS and the full macOS/Windows/Linux
matrix remain unqualified.

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
