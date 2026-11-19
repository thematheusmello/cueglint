# Windows x86_64 development notice inputs

The source inventory for `x86_64-pc-windows-msvc` has `bundle_ready=false`.
The manifest records three distinct graph observations for the default
`cueglint` and `cueglint-cli` roots.

- `graph.json` is the original offline macOS-host collection at `237fd249`.
  Its 397 package IDs include 386 normal-edge candidates, 26 proc macros
  within those candidates, and 11 build-only candidates.
- [Native Windows provenance run 44448095996](https://github.com/thematheusmello/cueglint/actions/runs/44448095996)
  verified the same IDs, reported license expressions and selected feature
  unions at frozen source `e1f6556`. It recorded the Windows Rust 1.94.1 library
  notice and compiler, standard-library and channel manifests, plus the
  actual MSVC and SDK selection.
- The source rebind at `25ec373` verified the same normalized rows using
  locked offline Cargo on arm64 macOS. The only Cargo input change is the
  macOS ScreenCaptureKit feature. The shared macOS inventory digest also
  changed with that rebind. Its earlier `e983a407` provenance and the macOS-only
  scope of its license choices are recorded. Native preflight for the current
  Windows source remains open. [Source rebind report](../../../parity/reports/windows-notice-source-rebind-001.json)

The manifest associates each package with original notice and source identities.
Shared originals use matching SHA-256 digests. Forty additional cached archives
match `Cargo.lock`. Their original filename notices have ten unique payloads.
Five archives lack standalone notices. Seven exact upstream Git blobs at the
archive-recorded revisions match retained originals. The original WGL XML and
generator copyright and license ranges keep their original bytes.

The Windows `ring@0.17.14` supplement preserves 23 original header blocks tied to
38 source ranges from the locked archive. The accompanying provenance records
15 core C sources, 12 AMD64 COFF assembly/object pairs and their generators.
The existing common and arm64 header notice is also retained. The Windows
source selection and these archive identities cover the published source route.
Compiler and final linker output still need evidence.

For `hexf-parse@0.2.1`, the original omission remains recorded. This source
inventory uses the exact locked publisher's CC0-1.0 declaration and the
identified full canonical CC0 instrument. The instrument has separate provenance
from the package declaration. The Windows-wide terms and applicability review
remains open. Three manifest gaps still block the bundle, covering Windows
license choices and native applicability, the native current-source preflight,
and the SDK, CRT and runtime route.

The previous CLI help assertion failure remains in the
[host qualification report](../../../parity/reports/windows-host-qualification-001.json).
The two original vcpkg CRLF files initially produced different filtered Git
blob IDs. The scoped `texts/** -text` rule preserves their raw Git bytes.

The inventory includes four default font identities, the frontend coefficient
source, and the authored brand. Segoe UI is read from the installed OS. The
native import-library hashes identify source inputs. Packaging still requires
reviewed prerequisites, exact two-binary and two-notice-tree checksums, ZIP
round trip, and clean Windows runtime evidence.

## Check the source inventory

From the checkout, using Python 3.11 or later:

```sh
python3 -B third-party/runtime/windows-x86_64-msvc/check.py
python3 -B third-party/runtime/windows-x86_64-msvc/check.py --require-ready
```

The first command validates bounded paths, the twelve Cargo and configuration
identities, the graph and notice references, and declared file bytes. The
second is expected to exit nonzero while the remaining source blockers are
open.
