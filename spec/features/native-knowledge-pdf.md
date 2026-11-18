# Bounded native PDF text ingestion

Implementation contract for the required `knowledge.ingestion`,
`knowledge.retrieval` and `knowledge.deletion` work. The reviewed reference help
lists PDF; the executed reference knowledge fixture was plain text only. This
contract does not establish reference parity, OCR, layout, citations or model
quality. PDF remains required while any implementation/verification gap is open.

The CLI and desktop retain `knowledge::read_document(&Path, &AtomicBool)`.
Explicit `.pdf` input uses the current native executable with the sole fixed
argument `--cueglint-pdf-worker-v1`. Both entry points dispatch it before CLI,
GUI, store, credential, provider or model initialization. No document path,
temporary document, network, hosted inference or external PDF program is used.
Input is raw stdin and successful output is raw UTF-8 stdout. Stderr is discarded
to avoid exposing document contents. The parent validates the full response and
inserts only after the child and both pipe workers finish successfully.

Bounds chosen before the original PDF fixtures: regular-file input and extracted
text at most 2 MiB each; original ten-second import deadline including file reads,
worker startup, pipe I/O and extraction. Cancellation/deadline polling is every
10 ms while the child runs. Every parent return/drop kills and waits for its own
child, then joins both owned pipe workers. OS calls and process reaping retain
the platform's scheduling/syscall limitations.

The child alone enables a **256 MiB cumulative Rust allocation work budget**.
The System allocator receives the same layouts/pointers; allocation and zeroed
allocation charge their requested size, reallocation charges its full new size,
and deallocation gives no credit. Exhaustion terminates immediately using
`_exit` on Unix or `ExitProcess` on Windows, without allocating/logging/unwinding.
Exit 73 means allocation budget exhaustion; 74 means malformed, encrypted,
unsupported, incomplete or otherwise rejected document text. Neither returns a
partial library entry. This is a Rust heap allocation work cap, not a process
RSS or native allocation cap. Normal application allocation remains System.

The pinned parser proposal is `lopdf = 0.47.0`, defaults disabled: no parser Rayon
pool, optional font embedding, async runtime, image embedding or clock feature.
Its previously rejected in-process CMap expansion now runs only inside the
budgeted child. The parent timeout is the hard stop for child CPU work. Source
inspection alone does not establish full or correct extraction: strict loading,
bounded decompression, and parsed content/font/CMap validation must reject the
library's fallback/skip paths before successful indexing. No bespoke PDF lexer,
cross-reference reader, fork or copied parser source is introduced.

External consumers of the core library must install `pdf_worker::Allocator` and
call `pdf_worker::dispatch()` first in their executable's main to use PDF import.
An arbitrary embedding executable or Rust test harness cannot serve as the
protocol helper. PDF integration/security checks therefore use the actual native
CLI binary, not an unbudgeted parser in the test process.

Stored `KnowledgeDocument`, user notes, UTF-8/DOCX ingestion, lexical retrieval,
exact extracted-text byte offsets, deletion/invalidation and backups keep their
existing schema and behavior. The original PDF bytes are not retained.

Current state: worker, allocator and semantic adapter candidate; original fixtures
and checks are written. Bounded dependency resolution and release-profile core/CLI
typechecking pass without code fixes. The corrected actual CLI run passes all six
named document tests, and the separate native supervision self-check passes.
Two earlier runtime failures are retained below. GUI/Windows checks, workspace
formatting and progress checks remain unrun. The
[compile record](../../parity/reports/knowledge-pdf-compile-002.json) retains the
earlier disk-floor failure and exact source/dependency/feature identities. No
feature registry state is promoted by these implementation checks.

The parsed-object adapter candidate supports source-order main-page text in
raw or single-Flate streams, with strict Adler-32 checksum, complete-stream and trailing-input
checks using already-locked pure Rust flate2/zlib-rs. The parser's stream helper
can return partial Flate output after a decoder error, so it is not used as this
integrity boundary. Loaded object/xref streams are rechecked before extraction.
The adapter accepts supported simple font encodings and fixed-width ToUnicode;
composite fonts require horizontal two-byte identity encoding. Every shown code
needs a mapping and strict UTF-16. The library's fallback/skip text collector is
not used. Parsed text operators preserve explicit line moves and source order;
TJ adjustments below -100 add a space. This does not reconstruct page geometry.

Source limits: 4 MiB for each load-time structural stream and each rechecked
stream/page, 16 MiB total adapter-checked structural/content/font bytes, 4,096
loaded objects, 128 pages, nesting/page-tree/graphics depth 64, 200,000 parsed
content/CMap operations, 65,536 CMap target entries and 32,767 bytes per content
string. The parser may decode structural bytes before the adapter counters;
the child's cumulative allocation cap and parent deadline cover that phase.
Fonts are loaded from checked plaintext CMaps to avoid duplicate decompression.

Unsupported content regions fail the whole import: XObjects/forms/images,
annotations, marked content, extended graphics states, unsupported fonts,
variable-width/vertical CMaps, non-Flate filters, predictors, incremental and
linearized PDFs. Parsed external-stream fields `F`, `FFilter` and `FDecodeParms`
are rejected before cloning or decoding stream bytes. External files are never
resolved or read; `F` means inline bytes cannot be treated as the document text.
The shared check covers content, Unicode-map and rechecked structural streams,
including escaped dictionary names such as `#46`. A conservative pre-load guard rejects the `XRefStm` name,
including `#xx` escaped spelling, anywhere in the bounded input because the parser
removes hybrid metadata from its returned trailer. This can reject otherwise
supported input containing that name in literal or compressed bytes. Hybrid
support needs retained structural provenance before that guard can be removed.
Textless/scanned pages reject the whole document, including after an earlier
supported text page. None of these gaps closes the full PDF/reference workstream.

The original two-page fixture and CLI source checks cover extraction, full text
larger than one content string, Unicode font mapping, modern object/xref streams,
exact retrieval offsets, deletion/context invalidation, backup/notes and existing
UTF-8/DOCX preservation. Rejection cases cover malformed/truncated/Adler-32-failed and
encrypted input, expansion, mapping/range/fallback loss, cycles/nesting and fixed
bounds. A separate actual-worker fixture expects immediate budget exit 73.
Initial guarded execution passes three selected core tests and the CLI DOCX
workflow. The first PDF run passes four tests and fails an attribution assertion
whose broad OR query also matches the preserved DOCX; the query-only correction
uses the PDF's unique `731` token and retains all assertions. A separate original
regression then confirms external `F` data is wrongly accepted before this shared
check was added. Both failures are retained in the
[external-stream RED record](../../parity/reports/knowledge-pdf-external-stream-red-001.json).
At the frozen correction `14142ba27d41855bef30a2ce4e2b24833ab1db10`, the corrected
full CLI run passes one DOCX and five PDF tests, with no failures, ignores or
filtering. Every external-stream case is reached and rejected; the complete
retrieval/offset/backup/notes/deletion assertions pass. The separate native
supervision executable passes its actual parser dispatch, six IPC failures,
ready-before-cancel and unchanged ten-second deadline checks. Child processes are
reaped and existing Store data is preserved. The
[targeted runtime record](../../parity/reports/knowledge-pdf-targeted-runtime-001.json)
keeps the standalone self-check separate from named Rust tests and the earlier
three selected core tests with two filtered out. Source/lock remain unchanged
during each guarded execution. Windows and actual native workflow evidence remain
required; these source-specific implementation checks establish no reference parity.

`cueglint-core` has one additional test-only `harness = false` native executable
for the exact `read_document` self-child route. It installs the real allocator
and dispatcher, and its controlled fixture workers exercise cancellation after
readiness, the unchanged ten-second deadline, oversized/invalid/NUL/empty output,
failure/budget status and no partial store changes. Every fixture checks its
recorded PID has exited/reaped after return; pipe workers must join before return.
This source is not a packaged worker mode. `xtask package` builds/copies only the
normal `cueglint` and `cueglint-cli` binaries. Actual allocator and hostile PDF
checks remain in the CLI integration suite using the real native CLI executable.

Primary references:

- [Adobe PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf), object/stream structure and text extraction conventions.
- [PDF Association text and ToUnicode errata](https://pdf-issues.pdfa.org/32000-2-2020/clause09.html).
- [PDF Association common stream dictionary, Table 5](https://pdf-issues.pdfa.org/32000-2-2020/clause07.html): external `F` data replaces inline stream bytes.
- [Published lopdf 0.47.0 source](https://docs.rs/crate/lopdf/0.47.0/source/), MIT archive SHA-256 `c6ad532ff9262dd4bfd6e085400cbb39959d48cb8d08518b01b68f77bdf6feca`, upstream commit `e9a0380b26d678b307e37d529ed8b8b04339727b`.
