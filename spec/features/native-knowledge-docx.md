# Bounded native DOCX ingestion

Implementation-only contract for `knowledge.ingestion`, `knowledge.retrieval`
and `knowledge.deletion`. The reviewed reference help in `reference-ui.md`
lists PDF and word-processing files; `reference-knowledge.md` executed only
one small plain-text upload. This increment does not establish reference parity,
PDF support, indexing-state fidelity, citation quality or model quality.

CLI `knowledge add` and the desktop's typed import use the same Rust core
extractor. Existing UTF-8 text, Markdown and source-code bytes remain unchanged.
An explicit `.docx` extension selects a macro-free ZIP/WordprocessingML profile;
known unsupported binary extensions and PDF/ZIP magic cannot silently enter the
plain-text path. PDF remains required work, explicitly unsupported here.

The DOCX profile requires consistent local/central ZIP headers, unique safe
entry paths, UTF-8 XML 1.0, the package content-types and relationship namespaces,
one internal `word/document.xml` main relationship, and a matching macro-free
content type. It accepts the transitional and strict WordprocessingML namespaces.
It reads literal main-body `w:t` text in XML order, preserves spaces and decoded
XML character references, inserts paragraph newlines (including self-closing
empty paragraphs), text-run tabs and explicit breaks. Paragraph tab-stop
definitions do not add text characters.
Field instructions, deleted/moved-from content, drawings, objects, pictures and
alternate content are excluded. Other parts and relationships are never resolved,
executed, extracted to disk or fetched. No macros, images, OCR or external programs
run. Headers, footnotes, comments, page geometry, styles, hidden-run semantics,
layout order, field results and all OOXML schema variants are not reconstructed.
Image-only or otherwise textless documents fail visibly.

Fixed bounds: regular-file input and extracted text each at most 2 MiB; at most
128 ZIP entries and 16 MiB declared expanded package content; main XML at most
4 MiB and each of two package metadata parts at most 64 KiB; XML depth 64,
64 attributes/namespace declarations per element, and 200,000 events per part.
ZIP entry bytes are separately bounded while actually inflating and checked
against declared sizes and CRCs. ZIP traversal has a 32 MiB requested-I/O and
65,536 read/seek-operation budget. DTDs, custom entities, processing instructions,
encrypted entries, symlinks and unsupported compression fail. Stored/DEFLATE
entries use the already-locked pure Rust decompressor. ZIP64 EOCD/locator magic
is conservatively rejected anywhere in the bounded input, including compressed
bytes, before any ZIP64 metadata could allocate. This can reject an otherwise
valid small document containing those byte sequences; ZIP64 is unsupported.

Cancellation and a ten-second deadline are checked around actual reads, ZIP
metadata work, XML events and completion. OS metadata/open/read calls themselves
cannot be interrupted by this API. Desktop imports use one background slot and
one result slot; cancellation keeps the slot occupied until that call returns.
The store actor holds no parsing transaction, remains available for other work,
and checks request cancellation/deadline again before the only insertion. The
UI locks the source field during import; a new dropped source or shutdown cancels
the previous request immediately. Delayed UI results must match the current
import UUID. Imports are global library operations, independent of selected
session/mode, and completion uses the then-current store selection.

The stored `KnowledgeDocument` schema stays unchanged. For DOCX, byte offsets
refer to the exact extracted text, with the original filename/path as attribution;
they are not offsets into ZIP bytes or page coordinates. The original package is
not retained. Existing lexical retrieval, frozen-context deletion/invalidation,
backup/restore and user notes retain their behavior and validation. No embeddings
or semantic-ranking claims follow.

The original fixture and adversarial cases exercise real extraction, storage,
retrieval, backup, deletion, malformed containers/XML, expansion, cancellation
and deadline rejection. See `crates/cueglint-core/tests/fixtures/knowledge/`.
These are implementation checks; they do not close registry/reference gaps.

Primary implementation references:

- [Microsoft WordprocessingML document/body/paragraph/run/text structure](https://learn.microsoft.com/en-us/office/open-xml/word/how-to-open-and-add-text-to-a-word-processing-document).
- [Microsoft package relationship example](https://learn.microsoft.com/en-us/office/dev/add-ins/word/create-better-add-ins-for-word-with-office-open-xml).
- [Microsoft Word `w:tab` text-run/tab-stop distinction](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.linq.w.tab?view=openxml-3.0.1).
- [zip 9.6.1](https://docs.rs/zip/9.6.1/zip/), its exact-version unstable lazy `ZipIterable` API and MIT source notices.
- [quick-xml 0.41.0](https://docs.rs/quick-xml/0.41.0/quick_xml/), namespace resolution/event APIs and MIT source notices.

PDF parser follow-up: reviewed `lopdf` 0.47.0 (MIT), crate SHA-256
`c6ad532ff9262dd4bfd6e085400cbb39959d48cb8d08518b01b68f77bdf6feca`, upstream
commit `e9a0380b26d678b307e37d529ed8b8b04339727b`. Its stream-size options do
not bound `src/encodings/cmap.rs:97` range enumeration into the reverse map.
Tiny attacker-selected 32-bit ranges can cause excessive allocation/work before
the caller regains control. This rejects that in-process parser route, without
removing PDF from required scope. A subsequent bounded parser/isolation design
needs its own evidence and original adversarial fixtures.

The subsequent [published-source review](../../parity/reports/knowledge-pdf-source-review-001.json)
rejects direct in-process `pdfrum` 0.5.3 facade adoption as well: content strings
silently truncate at 32,767 bytes, some ToUnicode ranges commit shortened mappings
without diagnostics, and total font/cache/extraction work is not caller-bounded.
No parser was linked or executed for that review. A strict complete-or-error
PDF profile or native process isolation still needs implementation and original
adversarial verification; PDF stays in required scope.
