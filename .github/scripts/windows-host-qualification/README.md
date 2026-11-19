# Windows host qualification

This manual, main-only workflow is a non-distributing development probe. It
checks out frozen public source `e1f655617acfd5171cca0cfc25370f1128f6e362`
separately from the workflow controller. It does not invoke `cargo xtask package`,
adopt Windows notices, sign, archive, upload binaries or license files, or release
anything. The existing macOS route is unchanged.

The original cross-host graph in `expected-graph.json` remains byte-identical to
the audit at `237fd249e34b512d019776b9f3095928eb59729d`. All twelve locked and
configuration inputs match the newer frozen source. That rebind is recorded in
`expected-source-inputs.json`; it is not a native Windows graph execution.
The job must compare the native Windows graph before building. A mismatch fails
without automatic retry, graph adoption or notice approval.

The job parses its PowerShell files before running them. It records the actual
rolling runner image, pinned Rust host, selected Visual Studio toolset and SDK,
installed notice-file hashes, graph, both Cargo-reported AMD64 PE32+ binaries,
and `dumpbin /headers /dependents`. By default it runs only CLI `--help`, with fresh explicit
database and empty-model paths; the GUI and model/capture paths are not run.
The known runner image is VS2026; `vswhere` and its default-toolset file select
the installed tools. SDK `10.0.26100.0` must actually be present.

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
requires the command list. A fresh hosted run will check the rest of the
qualification against the same frozen public source.

Primary command and runner documentation:

- [Pinned GitHub runner inventory](https://github.com/actions/runner-images/blob/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/images/windows/Windows2025-VS2026-Readme.md)
- [Visual C++ tool discovery](https://github.com/microsoft/vswhere/wiki/Find-VC)
- [Microsoft command-line build environment](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170)
- [DUMPBIN dependencies](https://learn.microsoft.com/en-us/cpp/build/reference/dependents?view=msvc-170)
- [Visual C++ redistribution terms boundary](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)
