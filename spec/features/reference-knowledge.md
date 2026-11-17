# Controlled reference-file observation

Baseline: `cluely-macos-2.1.28`, 2026-11-17. The complete fixture and instructions
below were authored for this investigation. They contain no user or commercial
content. The investigator used only a clearly named temporary mode and restored
the original active mode afterward.

## Original fixture

Filename: `cueglint-original-kite-fixture.txt`; 305 UTF-8 bytes; SHA-256
`ba9a7373d2d810e46ef6963d51b3dc7b0aef41f260e691e7a07368ea2e9d3d5f`.
The bytes have LF newlines, including a final newline:

```text
Original controlled reference fixture for Cueglint research.

The fictional Cobalt Kite Workshop uses the safety word ZEPHYR-817.
Its inventory contains eight cobalt kites and three ochre spools.
Customer returns are accepted for fourteen days.
These fictional facts are for a UI and retrieval test only.
```

The temporary mode instructions were:

> Answer questions from the attached fictional workshop document. Use only its stated facts.

## Observed ingestion

The investigator added the file through the app's native file picker, verifying
the selected filename before upload. The initial file row showed the filename,
305 B size, a numeric upload percentage, a progress accessibility element, and
a remove action. The add-file action was disabled during this initial upload.
The next state exposed a document link, file size, remove action and processing
notice. Once processing completed, the notice disappeared. This sequence was
observed for one small plain-text file only.

The file row appeared as an approximately 368 × 76 px rounded card in the mode
detail pane, with a file-type tile, approximately 13 px filename, 12 px metadata
and a right-aligned remove icon. An add-another-file action sat beneath it.
Uploading did not require activating the mode first. The investigator activated
it after processing completed.

## Observed use

With screen context disabled, no audio session running, and a new empty chat,
the investigator submitted:

> From the attached fictional workshop file, give only the safety word and number of cobalt kites.

The completed answer was `ZEPHYR-817; 8 cobalt kites.` A muted approximately 12 px
file-use provenance label appeared above the answer. Neither queried fact was
in the question or mode instructions. This supports document-grounded behavior
for this one fixture. It does not establish the retrieval algorithm, index
format, chunking, citation fidelity, large-document quality, unsupported-file
handling or a guaranteed response format.

## Cleanup and limits

The investigator cleared the test chat, removed the original file through its
own row action, restored the original active mode, and deleted only the temporary
test mode. A later list inspection showed neither the named mode nor the earlier
empty inspection draft. The file row disappeared after removal. A file-input
control retained its last chosen filename in the DOM; that is not an attached
file row. Server-side erasure or retention was not verified.

The uploaded document was entirely fictional. No personal document, existing
meeting or existing mode was edited or uploaded. No deletion of user-owned
content was attempted. File-type coverage, the displayed 20 MB limit, duplicate
handling, cancellation, ingestion failure and retrieval after removal remain
unverified.
