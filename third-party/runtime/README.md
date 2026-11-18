# Runtime notice source collection

This directory retains notice text, publisher licensing declarations and a
portable source inventory for the macOS app and CLI notice bundle. The
provenance supplement changes no packager or runtime.
`bundle_ready` is **true** for the reviewed source inputs and one controlled
`aarch64-apple-darwin` package attempt with the exact twelve inputs and default
flags below. The [source decision](../../parity/reports/runtime-notice-readiness-001.json)
adopts the recorded choices and repaired attribution. Actual package/signing,
clean installation, exact linked-code and platform/reference acceptance remain open.

The original collection baseline is merged commit
`57d81abf65d060d3ea475c41c4bc5948a859b007`; its historical report remains unchanged.
The [provenance supplement](../../parity/reports/runtime-notice-provenance-supplement-001.json)
is based on `522507ac948b6e83277c72ac4a18d432ceb32851` and preserves its original pending
decisions. The later source decision below resolves them without erasing findings.
The reused offline metadata snapshot has SHA-256
`958b45fb775d522defeeb2b7037a480da2b8b0cf730835a34d1bfb8e7ee5846c`.
Its 442 normal-edge candidates form a **conservative workspace feature
supergraph** for `aarch64-apple-darwin`, including 27 proc-macro packages.
Workspace-member defaults enable Glow even though the current packaging command
selects only `cueglint` and `cueglint-cli`. This inventory is not an exact list of
code linked into those two executables; build tools and optional code need review.

Originals come from checksum-verified locked crate archives, upstream Git blobs at
their archive-recorded revisions, or existing same-commit workspace originals.
Identical original bytes can share a stored file; each package still records its
own archive or Git provenance. This is byte deduplication, not substitution of a
generic license template. Inherited workspace notices require the same recorded
upstream commit. Existing notices remain unchanged, including all 59 files in the
PDF source ledger and their conservative/non-host extras.

Two exact original `Cargo.toml.orig` declarations now accompany `dispatch` and
`hexf-parse`. Each matches its locked archive and upstream Git blob at the
archive-recorded revision. They declare MIT and CC0-1.0 respectively. The author
fields identify package authors; they do not establish copyright ownership or
years. No copyright line is invented or borrowed from another package.

The package associations separately identify an existing canonical MIT permission
body and complete CC0 instrument. Their bytes match pinned SPDX instruments;
their role is explicitly **not an original notice from either package**. This
does not change the omission proof or substitute a claimed upstream LICENSE file.
The scoped source decision adopts this declared-term distribution basis: MIT requires retaining
the applicable copyright and permission notices, without naming a standalone
file; CC0 has no attribution condition. The full terms and rights limits remain.
See the [MIT terms](https://opensource.org/license/mit),
[CC0 legal code](https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt),
[official CC0 FAQ](https://wiki.creativecommons.org/wiki/CC0_FAQ) and
[Cargo metadata semantics](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields).

The collection includes the four embedded egui font notices, nested ring,
atomic-waker, Unicode-table and tracing-spin notices, and ring's upstream AUTHORS
file. The SQLite public-domain notice is an exact, bounded excerpt of its original
header, with archive path and byte range recorded. Source files named `copying.rs`,
unused SQLCipher material and TIFF test-fixture attribution are explicitly excluded.
Installed OS fonts and model payloads are not copied or inspected.

The [ring source-header repair](../../parity/reports/runtime-notice-ring-source-headers-001.json)
retains eight byte-distinct complete ISC legal-comment excerpts from ten source
files. They preserve additional Google, Intel, Trent Clarke, Simon Sapin and
David Judd notices that the original Brian Smith umbrella does not name. The
review covers common/arm64 native sources, their conservative owned includes
and ordinary library modules, including the equivalent SIMD paths. The published
archive, source/header hashes and exact byte ranges bind each excerpt. Its VCS
marker reports `dirty=true`; no unfetched Git equality is asserted. Existing
ring, BoringSSL, other-bits, fiat and once_cell notices remain unchanged. This
repairs a concrete attribution input. The later Root decision adopts it with
the one-byte authored-wrapper correction, preserving every original legal range.

The readable font supplement transcribes the exact Noto and Ubuntu copyright
name-ID0 records from the locked font assets. Its provenance records the font,
record and archive identities. It retains the original OFL/UFL bodies and avoids
depending on unverified easy access to embedded binary metadata. This does not
assert a prior licence violation or modify any font.

`distribution_review_proposal` records the 35 declared-expression choices and
mandatory AND, nested and asset envelopes. Root explicitly adopts all 35 choices
and seven AND envelopes in `readiness_decision`; file presence never promotes
the gate automatically. The seven AND
packages retain every mandatory component. All original alternatives remain,
including the unselected GPL text for the selected Apache branch of `self_cell`.
The 442-node normal graph conditionally covers the selected GUI/CLI roots under
the unchanged twelve inputs, arm64 target and build flags. Cargo feature union
supports that conservative coverage; it does not identify exact linked code or
arbitrary native/build-script payloads. See
[Cargo feature unification](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).

The exact retained objc2 licence caveat is associated with 23 candidates: four
MIT-only core crates and 19 trio-licensed candidates (dispatch2 plus 18 framework
crates). Its author's open question about SDK-derived works establishes no
additional notice condition or proved prohibition. The packaging source copies
executables, icon, plist and notices, rather than installed SDK headers, stub
files or OS framework bundles. Compiled Rust wrappers may be included; their
exact binary inclusion and SDK-term applicability are not resolved here.

The Rust standard-library envelope is the original `COPYRIGHT-library.html` from
the installed Rust 1.94.1 compiler component. The manifest records its exact bytes,
installer component membership, channel-manifest identity and the publisher's
component URL/checksum. The distribution archive was **not fetched or reverified**;
this provenance boundary is retained. The document covers library source and build
dependencies, rather than proving target-specific linked-code attribution.

## Preserved findings and remaining validation

- `dispatch` 0.2.0 reports MIT, but its checked archive README/module header and
  recorded repository root provide no original copyright or permission notice.
- `hexf-parse` 0.2.1 reports CC0-1.0, but its checked archive module header and
  recorded repository root/README provide no original copyright or permission
  notice. No generic CC0 text was substituted.
- The exact publisher declarations plus identified terms resolve the two
  standalone-file findings for this source scope. Their omissions stay unchanged
  in `omissions.json` and the historical reports; no upstream file or owner/year
  is invented. The repaired ISC input and recorded choices are adopted.
- Source title and the retained component/SDK applicability questions remain
  explicit boundaries; no further notice condition was established by this review.
- The Rust component archive verification boundary remains as described above.
- The source-owned validation/copy helper has separately retained headless tests
  and an expected readiness refusal. This supplement runs source-data checks
  only; no successful package build, signing or actual app/CLI notice-tree
  acceptance is established.

The two bounded original-text checks are retained in `omissions.json`, including
archive/file hashes, reported metadata labels and recorded root-tree identities.
They do not claim to inspect every historical file or another upstream revision.
Raw HTTP responses, private paths and tooling diagnostics remain outside the repo.

## Minimal manifest v1 consumer contract

`manifest.json` proposes these fields. The package/asset evidence is descriptive;
the consumer must not fetch a registry, invoke a model, or infer coverage from it.

| Field | Meaning |
| --- | --- |
| `schema_version` | Integer `1`; reject unsupported versions. |
| `bundle_ready` | Reviewed source gate; current `true` is bound to `readiness_decision`. False or any blocking gap still stops before build/sign/output. |
| `inputs[]` | `{source_path, sha256}` for the lockfile, toolchain/config and Cargo manifests. Paths are relative to the repository root. |
| `files[]` | Unique `{source_path, bytes, sha256}` records for exact notice and attribution payloads. |
| `packages[]` | Portable package IDs, declared terms, conservative features/dependency edges, source identity and references to `files[]`. Publisher declarations, originals and canonical instruments have distinct roles. This is no legal selection or linked-code claim. |
| `embedded_assets[]` | Font, coefficient/helper and original mark associations; asset payloads are not copied by this collection. |
| `toolchains[]` | Notice/component provenance and its explicit verification boundary. |
| `additional_retained_envelopes[]` | Existing PDF originals and modified-dependency attribution, including conservative extras. |
| `gaps[]`, `gap_evidence` | Each gap has an explicit Boolean `blocks_bundle`; missing originals and unreviewed mandatory coverage/choice/applicability block. Retained scope/provenance limits and later package verification do not become circular prerequisites. |
| `copy_contract` | Source-owned dual output implementation; real output/signing validation remains open. |
| `distribution_review_proposal` | Supplement provenance, canonical-instrument roles, proposed choices, mandatory envelopes, conditional coverage and the exact SDK caveat. Descriptive data does not override the gate. |
| `readiness_decision` | Explicit scope, choices, dispositions, retained limits and evidence; no consumer exception. |
| `ring_source_header_repair` | One original-excerpt payload, published-archive provenance, byte ranges and selected-source review. Original repair evidence and later authored-wrapper revision; the separate source decision controls the gate. |

Before a later package build, require `bundle_ready: true` and no gap with
`blocks_bundle: true`; there is no consumer whitelist or override. The original
three findings now have explicit nonblocking source dispositions; their definitions
and historical records remain. Different inputs/target/features require review.
Nonblocking scope/provenance limits remain recorded, and the later package-output
receipt is not a prerequisite for invoking the helper.

Reject duplicate or unsafe
paths, absolute/drive paths, `..`, symlinks, non-regular files, missing/altered
payloads and input drift. Validate every expected size and SHA-256. Check input
identities again after the locked build, before copying/signing. Any ignored
component or later scope change needs its own reviewed record.

The implemented output roots are
`dist/Cueglint.app/Contents/Resources/THIRD_PARTY/` and
`dist/cueglint-cli.NOTICES/`. Copy the same `manifest.json` and exact `files[]`
payloads into each, preserving their repository-relative `source_path` layout.
The manifest is copied separately and does not contain its own hash. Validate all
inputs before writing output; copy notices before app signing. Bind the manifest
and payload hashes in package evidence. A CLI distributed alone must keep its
notice sidecar. Nothing here makes an app-only copy sufficient for the CLI.

Implemented bounded consumer limits, grounded in this data: 2 MiB manifest, 1,024
payload files, 2 MiB per payload and 16 MiB total payload bytes. These constrain the build helper and do not change application import/model budgets.

Meaningful later checks are exact bytes/hashes in both trees and rejection of
missing/altered payloads, stale inputs and unsafe paths before build/sign/output.
Actual packaging/signature/platform checks require separate source-bound evidence.
