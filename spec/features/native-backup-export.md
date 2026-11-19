# Native backup export

Cueglint's original backup control is in Settings → Security → Backup.
Enter the path for a new JSON file and select **Export backup**. The path
field has an accessible label. The action becomes available with a loaded
store, a supplied path, and no pending conflicting operation.

Cueglint saves any pending note edit before the store worker reads the
backup. The worker uses the versioned logical format and private atomic
writer used by the CLI. The UI reports the destination or the reason
the write failed. An existing file is preserved.

Restore uses the CLI's validated transaction. The documented format
covers window context, retained OCR, source provenance, and retained
records. See [Portable data format](../../docs/data-format.md).

The regression uses original fixture data and drives the Settings action
and store worker without a native window. It restores the exported
notes and verifies that an occupied destination and a newer note both
survive a second export attempt. Actual packaged interaction and
matched-reference comparisons remain open.
