# Original knowledge fixture

Authored for Cueglint, MIT OR Apache-2.0; fictional facts only, no reference-app
document or capture. `original-workshop.docx` is a real ZIP containing the three
adjacent XML source parts. It was authored with `/usr/bin/zip -X -9
original-workshop.docx '[Content_Types].xml' _rels/.rels word/document.xml`.
ZIP is an authoring tool only, not an application or test runtime requirement.
Regeneration can change ZIP timestamps; archive identity is explicit below.

The archive SHA-256 is
`51f2c22dc4b2f1b1039e6b27ebfd20acb8ac147fbf7abf3756a8dcacccea30b0`.
`expected.txt` SHA-256 is
`41218b5995fc582381d5deeee0a96121217954e4e0a9833efa850ab460fc4273`.
It records the extractor's paragraph/tab/break/UTF-8 output, not a page layout
or model answer. Core and CLI tests read the actual archive. Additional original
malformed/bomb inputs are built with the Rust ZIP writer and deliberately mutate
its headers where needed; no recovered or third-party test documents are used.
