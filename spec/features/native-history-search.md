# History search by creation date

Cueglint accepts the UTC creation date shown above a meeting in History.
Enter the full date or a prefix such as `2024-02`. Results retain their date
groups and current ordering. Opening a result still opens that exact meeting
and its saved notes. Later edits leave the searchable creation date intact.

The CLI uses the same date and text search with
`cueglint-cli session list 2024-02`. Title, notes, summary and finalized
transcript queries continue to work. Transient transcript text stays outside
the saved library search.

The original regression fixture crosses UTC midnight on leap day. A
meeting created in February and edited in March remains searchable in
February. Compare the returned identities and displayed date exactly.
[Reference observations](reference-sessions-assistant.md) cover one
case-insensitive title search. Date search is a Cueglint contract.
