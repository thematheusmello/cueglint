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
