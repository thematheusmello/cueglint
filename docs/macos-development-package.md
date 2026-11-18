# Download a macOS arm64 development package

Open **Actions → macOS arm64 development package → Run workflow**, choose
**main**, and run it. Repository write access is required. The job uses
`cargo xtask package` with Rust 1.94.1 on the arm64 `macos-15` runner. It selects
Xcode 26.3 and checks for a macOS 26 or newer SDK before building.

The first hosted attempt reached the runner and failed while compiling the
`apple-metal` Swift bridge because Metal tensor and sampler types were missing.
The runner image lists Xcode 16.4 as its default and Xcode 26.3 with the macOS
26.2 SDK as an installed option. The SDK selection is a build requirement. The
minimum supported macOS runtime still needs installation and launch tests.
[Failed package run](https://github.com/thematheusmello/cueglint/actions/runs/44384847084) ·
[Runner image toolchains](https://github.com/actions/runner-images/blob/macos-15-arm64/20261112.0539/images/macos/macos-15-arm64-Readme.md#xcode)

After a successful run, sign in to GitHub and download the
`macos-arm64-development-<full commit SHA>` artifact from the run's **Artifacts**
section. Artifacts are retained for 14 days. Unzip GitHub's download to obtain:

- `cueglint-macos-arm64-dev-<full commit SHA>.zip`
- The matching `.zip.sha256` file.

Use the actual archive filename below. Verify it and extract into a new directory
on an Apple silicon Mac:

```sh
archive="cueglint-macos-arm64-dev-FULL_COMMIT_SHA.zip"
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

The archive is for arm64 development testing. It has an ad-hoc signature and
uses models you install separately. The run and extracted package should be
checked before treating the archive as a distribution candidate.

The runner and artifact behavior follows GitHub's
[runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[manual workflow instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow),
and [upload-artifact v4 permission guidance](https://github.com/actions/upload-artifact/tree/v4#permission-loss).
