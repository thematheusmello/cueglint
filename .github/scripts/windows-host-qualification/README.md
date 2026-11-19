# Windows host qualification

This manual, main-only workflow is a non-distributing development probe. It
checks out a pinned public source separately from the workflow controller. The
default `frozen` selects `e1f6556`. The `package-candidate` choice selects the
pushed Windows package source `5a468a2`. The `calendar-candidate` choice selects
`ba4a3d2`, which includes the native Calendar reminder and macOS audio timing
increments. Both candidates bind their macOS and Windows notice inventories.
Each choice validates the commit, tree and exact build input bytes. The probe compiles directly and keeps all binaries on the
runner. Package building, notice adoption, signing and release are separate.

The original cross-host graph in `expected-graph.json` remains byte-identical to
the audit at `237fd249e34b512d019776b9f3095928eb59729d`. All twelve locked and
configuration inputs match the newer frozen source. That rebind is recorded in
`expected-source-inputs.json`. The package candidate uses
`expected-package-candidate-inputs.json` and the separately reviewed offline
Windows graph from the [source rebind](../../../parity/reports/windows-notice-source-rebind-001.json).
The Calendar candidate records a separate source pin and the exact input
equality with its offline cross-target projection. The hosted job compares the
selected source against the same 397-row native graph before building. A
mismatch fails without automatic retry, graph adoption or notice approval.

The package candidate release-build receipt also checks the two generated
`glutin_wgl_sys` bindings under Cargo's reported output directory. It records
bounded file lengths, hashes and reviewed type-source comment presence. The
generated source stays on the runner. [Cargo build-script messages](https://doc.rust-lang.org/cargo/reference/external-tools.html#build-script-output)

The same release build records the first linker resolved after `vcvarsall`
when it matches the selected x64 MSVC toolset. Cargo build-script messages bind
the declared Ring and bundled SQLite libraries to their output directories.
The receipt hashes those archives and records missing or partial identities.
The Ring build script also produces its test archive in release builds.
Final linker member selection requires separate evidence.

The Windows Rust CI job runs `test-evidence.ps1` to check CRT priority and
the WGL collector with synthetic Cargo messages. The manual probe then
measures the actual Windows host and native build.

The job parses its PowerShell files before running them. It records the actual
rolling runner image, pinned Rust host, selected Visual Studio toolset and SDK,
installed notice-file hashes, graph, both Cargo-reported AMD64 PE32+ binaries,
and `dumpbin /headers /dependents`. By default it runs only CLI `--help`, with fresh explicit
database and empty-model paths; the GUI and model/capture paths are not run.
The known runner image is VS2026; `vswhere` and its default-toolset file select
the installed tools. SDK `10.0.26100.0` must actually be present. The receipt
requires nonempty, bounded records for the installed Windows x64 Rust compiler
and standard library components and the installed channel manifest.

Select `provenance_only` to check the selected source, host tools and locked
Windows graph, including the SDK and toolset selected by `vcvarsall`. The probe
ends before compilation. Choose either `provenance_only` or `gui_fixture`.

The optional manual input `gui_fixture` defaults to false. When selected, the
same Cargo-reported release GUI runs `--fixture native-v2-dashboard --screenshot`
with a fresh owned PNG path after CLI help succeeds. This uses original in-memory
data, disables global shortcut registration and skips permission/model checks;
it does not capture the desktop, access credentials, submit questions or download.
Child-only temporary, data and model paths are isolated below the evidence root.
The 30-second process deadline uses a sampled 1 MiB per-stream log limit, a tree
kill on failure and bounded root-exit/stream cleanup. The PNG is limited to 16 MiB
and 4096 pixels per side. Request/ROOT transport/font/path/scale identities and
decoded dimensions must agree before a pass. Adapter and PNG hashes enter the
text receipt; the PNG stays in a nested ephemeral directory and is never uploaded.
The current runner's window session, graphics adapter and Windows image decoder
remain actual-run checks. No fallback backend, retry or additional build is used.

The optional GUI probe also samples selected runtime DLLs from the child process
handle. Its `loaded_dlls` receipt records up to 32 selected VC runtime, UCRT and
graphics module paths. The file versions and hashes describe the readable on-disk
Windows files. Within the identity budget, the probe checks system
`VCRUNTIME140.dll`, `ucrtbase.dll`, and then the other selected system CRT
modules before the remaining paths. Sampling runs alongside the render with a
five-second cooperative window and bounded hash reads. Incomplete sampling,
timeouts and setup errors have separate statuses. The render has its own
outcome, and this sample covers only the modules observed during the original
fixture.
[Module enumeration](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-enumprocessmodulesex) ·
[Module path lookup](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getmodulefilenameexw)

To run the Calendar candidate with its original GUI fixture and private maps:

```sh
gh workflow run windows-host-qualification.yml --ref main -f qualification_source=calendar-candidate -f gui_fixture=true -f product_link_maps=true
```

The opt-in map mode wraps rustc only during the joint release command. It adds
`/MAP` to the final two target bin links. The collector requires fresh Cargo
artifacts and matches each executable to its byte-identical intermediate PE,
link marker, map name, PE timestamp and image base. The report exposes fixed
origin aliases, public row counts, hashes and the unclassified count. Raw
maps, symbol names and map-mode build log contents stay on the runner.
Aliases describe retained public source labels. Complete library member
selection and licensing require separate evidence. If a map is refused after
build, the job still attempts CLI help and the requested GUI fixture before
reporting the refusal.
[Cargo workspace wrapper](https://doc.rust-lang.org/cargo/reference/config.html#buildrustc-workspace-wrapper) ·
[MSVC map format](https://learn.microsoft.com/en-us/cpp/build/reference/map-generate-mapfile?view=msvc-170)

Only bounded generated text is retained in job logs, including the canonical
receipt and failure excerpts. There is no artifact-upload step. Raw evidence
files and build outputs remain on the ephemeral runner. A job cancelled before
the always-step may retain only its preceding logs. A successful always-step
does not establish a successful qualification. The job has a 45-minute deadline,
two Cargo build jobs, post-command log bounds, and a 10-second CLI-help deadline
(with bounded cleanup and stream-drain waits);
these are not continuous memory or disk guarantees. Pinned Rust setup and locked
public Cargo sources may be fetched on the fresh host; there is no offline claim.

Windows notice readiness remains **false**. Hash discovery does not establish
license applicability or redistribution rights. A successful technical run would
still require Windows Rust/SDK/CRT/native-route terms review and separate package
output checks. PE imports do not cover all runtime-loaded libraries. No Windows
device, model, hardware or reference-parity result follows from this probe.
An opt-in fixture pass covers primary-window rendering/readback only; ordinary
GUI input, child windows, backdrop/compositing and clean installation remain open.

The [first hosted run](https://github.com/thematheusmello/cueglint/actions/runs/44418418630)
matched all 397 dependencies, built both AMD64 PE32+ executables, and inspected
their imports. The CLI printed its help and exited zero. The run failed because
the help check expected the executable name without the Windows suffix. The
corrected check requires `cueglint-cli.exe` in the actual usage line and still
requires the command list. The [corrected run](https://github.com/thematheusmello/cueglint/actions/runs/44421629393)
passed on the original frozen source.

Primary command and runner documentation:

- [Pinned GitHub runner inventory](https://github.com/actions/runner-images/blob/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/images/windows/Windows2025-VS2026-Readme.md)
- [Visual C++ tool discovery](https://github.com/microsoft/vswhere/wiki/Find-VC)
- [Microsoft command-line build environment](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170)
- [DUMPBIN dependencies](https://learn.microsoft.com/en-us/cpp/build/reference/dependents?view=msvc-170)
- [Visual C++ redistribution terms boundary](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)

## Installed SDK terms

The official Windows SDK `10.0.26100.9457` bootstrap carries the same
248,573-byte RTF as the installed SDK directory in run `44483229488`.
The embedded manifest and both SHA-256 observations match. The NuGet-linked
terms have different bytes and normalized text. The installer and original
terms stay outside this repository. This identifies the observed SDK license
source. Visual Studio terms, licensed builder basis and applicability to
linked components remain open.
[Installed SDK terms origin](../../../parity/reports/windows-sdk-installed-terms-origin-001.json)

## Linker control

Run the original linker control on the frozen package candidate's toolchain.

```sh
gh workflow run windows-host-qualification.yml --ref main -f qualification_source=package-candidate -f link_calibration_only=true
```

The controller selects `vcvarsall x64`, checks the exact source graph, and runs
the original control in the disposable runner. The mode is exclusive with
`gui_fixture` and `provenance_only`.

The control builds an original library with `used.obj` and `unused.obj`.
Pinned rustc links one AMD64 PE and requests the linker's library trace through
`-W linker-messages` and `/VERBOSE:LIB`. A passing receipt requires a complete
search trace and one load of the used member from the owned archive. The
archive, executable and trace digests are in `link-calibration.json`. The
resulting PE also checks the linked C function at startup. Its private map
independently identifies the retained public symbol's origin. The receipt
records map and trace verdicts and a bounded trace outline made from fixed
labels and flags. Raw diagnostics, map and binaries stay on the disposable
runner. The first native run refused the trace after a successful link.
The second run executed the original control and confirmed its retained public
symbol in the map. The trace listed six searches of the archive and lacked the
symbol and member markers. Both overall refusals remain recorded. Product PE
origins, complete member selection, package notices and clean installation
require their own evidence.
[Original refusal](../../../parity/reports/windows-link-calibration-refusal-001.json) ·
[Map observation and trace refusal](../../../parity/reports/windows-link-calibration-map-002.json)

Run `test-link-calibration.ps1` for deterministic trace and PE controls. It
loads the parser functions and uses synthetic inputs. [Map output](https://learn.microsoft.com/en-us/cpp/build/reference/map-generate-mapfile?view=msvc-170)

[MSVC archive operations](https://learn.microsoft.com/en-us/cpp/build/reference/managing-a-library?view=msvc-170) ·
[/VERBOSE](https://learn.microsoft.com/en-us/cpp/build/reference/verbose-print-progress-messages?view=msvc-170) ·
[Pinned rustc linker diagnostics](https://github.com/rust-lang/rust/blob/e408947bfd200af42db322daf0fadfe7e26d3bd1/compiler/rustc_codegen_ssa/src/back/link.rs#L855-L892)
