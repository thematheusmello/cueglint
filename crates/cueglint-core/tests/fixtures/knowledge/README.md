# Original knowledge fixture

Authored for Cueglint, MIT OR Apache-2.0; fictional facts only, no reference-app
document or capture. `original-workshop.docx` is a real ZIP containing the three
adjacent XML source parts. It was authored with `/usr/bin/zip -X -9
original-workshop.docx '[Content_Types].xml' _rels/.rels word/document.xml`.
ZIP is an authoring tool only, not an application or test runtime requirement.
Regeneration can change ZIP timestamps; archive identity is explicit below.

The archive SHA-256 is
`4e711ec59d674bac633d051419f064997248b32b8fd3f0efd314cb230c58c84f`.
`expected.txt` SHA-256 is
`ca0bb1781eaf54cc0964c5f21cc4e7a11be1580f9dc91f1930b0d0bb0e215ee0`.
It records the extractor's paragraph/tab/break/UTF-8 output, not a page layout
or model answer. Core and CLI tests read the actual archive. Additional original
malformed/bomb inputs are built with the Rust ZIP writer and deliberately mutate
its headers where needed; no recovered or third-party test documents are used.

`original-workshop.pdf` is a separately authored two-page PDF 1.7 with classic
cross-references, Helvetica text, three Flate-compressed streams and an original
fixed-width ToUnicode map for ASCII, é and €. All facts are fictional. Its
uncompressed content/CMap source is `original-pdf-content.txt`; the expected
source-order text is `expected-pdf.txt`. A one-off standard-library authoring
script wrote stream lengths/offsets and compressed the authored bytes. That tool
is not required by the app or tests. No reference or third-party PDF was used.

PDF SHA-256: `25e149e38322c571328959264aef91d26b7a339280cfec102d054b6600ff4a31`.
Expected text: `e9bab2d35c3b57a88a9003fb54a6f02100a1f84cf70e897a45031557f21c1539`.
Readable content source: `a3586aa6c6c369aaca47122b162c18417f406f7ed7325662361c005e90ad5ce7`.
Adversarial PDFs are authored in the CLI integration checks and are sent only to
the actual bounded native child, never parsed in the parent test process.
