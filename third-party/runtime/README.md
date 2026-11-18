# Runtime notice source collection

This directory retains original notice text and a portable source inventory for
the proposed macOS app and CLI notice bundle. It changes no packager or runtime.
`bundle_ready` is **false**. No complete license, exact linked-runtime, package,
platform or reference acceptance is claimed.

The source baseline is merged commit `57d81abf65d060d3ea475c41c4bc5948a859b007`.
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

The collection includes the four embedded egui font notices, nested ring,
atomic-waker, Unicode-table and tracing-spin notices, and ring's upstream AUTHORS
file. The SQLite public-domain notice is an exact, bounded excerpt of its original
header, with archive path and byte range recorded. Source files named `copying.rs`,
unused SQLCipher material and TIFF test-fixture attribution are explicitly excluded.
Installed OS fonts and model payloads are not copied or inspected.

The Rust standard-library envelope is the original `COPYRIGHT-library.html` from
the installed Rust 1.94.1 compiler component. The manifest records its exact bytes,
installer component membership, channel-manifest identity and the publisher's
component URL/checksum. The distribution archive was **not fetched or reverified**;
this provenance boundary is retained. The document covers library source and build
dependencies, rather than proving target-specific linked-code attribution.

## Remaining gaps

- `dispatch` 0.2.0 reports MIT, but its checked archive README/module header and
  recorded repository root provide no original copyright or permission notice.
- `hexf-parse` 0.2.1 reports CC0-1.0, but its checked archive module header and
  recorded repository root/README provide no original copyright or permission
  notice. No generic CC0 text was substituted.
- Exact selected-build/runtime scope, license choices and component/asset
  applicability still require review. Original text retention alone does not
  resolve those questions.
- The Rust component archive verification boundary remains as described above.
- No validation/copy helper, package build, signing or app/CLI notice-tree check is
  implemented or run here.

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
| `bundle_ready` | Reviewed readiness gate; the current `false` must stop before build/sign/output. File presence never promotes it automatically. |
| `inputs[]` | `{source_path, sha256}` for the lockfile, toolchain/config and Cargo manifests. Paths are relative to the repository root. |
| `files[]` | Unique `{source_path, bytes, sha256}` records for exact notice and attribution payloads. |
| `packages[]` | Portable package IDs, declared terms, conservative features/dependency edges, original source identity and references to `files[]`. This is no legal selection or linked-code claim. |
| `embedded_assets[]` | Font, coefficient/helper and original mark associations; asset payloads are not copied by this collection. |
| `toolchains[]` | Notice/component provenance and its explicit verification boundary. |
| `additional_retained_envelopes[]` | Existing PDF originals and modified-dependency attribution, including conservative extras. |
| `gaps[]`, `gap_evidence` | Each gap has an explicit Boolean `blocks_bundle`; missing originals and unreviewed mandatory coverage/choice/applicability block. Retained scope/provenance limits and later package verification do not become circular prerequisites. |
| `copy_contract` | Proposed dual output layout; this is not an implementation. |

Before a later package build, require `bundle_ready: true` and no gap with
`blocks_bundle: true`; there is no consumer whitelist or override. The current
missing original texts and mandatory review decisions keep that gate closed.
Nonblocking scope/provenance limits remain recorded, and the later package-output
receipt is not a prerequisite for invoking the helper.

Reject duplicate or unsafe
paths, absolute/drive paths, `..`, symlinks, non-regular files, missing/altered
payloads and input drift. Validate every expected size and SHA-256. Check input
identities again after the locked build, before copying/signing. Any ignored
component or later scope change needs its own reviewed record.

The proposed output roots are
`dist/Cueglint.app/Contents/Resources/THIRD_PARTY/` and
`dist/cueglint-cli.NOTICES/`. Copy the same `manifest.json` and exact `files[]`
payloads into each, preserving their repository-relative `source_path` layout.
The manifest is copied separately and does not contain its own hash. Validate all
inputs before writing output; copy notices before app signing. Bind the manifest
and payload hashes in package evidence. A CLI distributed alone must keep its
notice sidecar. Nothing here makes an app-only copy sufficient for the CLI.

Proposed bounded consumer limits, grounded in this data: 2 MiB manifest, 1,024
payload files, 2 MiB per payload and 16 MiB total payload bytes. These are a review
proposal for the build helper, not changes to application import/model budgets.

Meaningful later checks are exact bytes/hashes in both trees and rejection of
missing/altered payloads, stale inputs and unsafe paths before build/sign/output.
Actual packaging/signature/platform checks require separate source-bound evidence.
