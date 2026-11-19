# Native backup restore

In Settings → Security → Backup, enter the path to a Cueglint backup and
select **Review restore**. The review names the selected file and explains
that it contains the library and settings. Replacing an existing library
requires a separate unchecked-by-default confirmation. Cancelling the
review submits no restore command.

The app saves pending note edits before starting restore. Restore requires
an idle workspace so capture, generation and imports can finish first.
The store worker reads a regular UTF-8 file of at most 64 MiB, validates
the version, records and original settings, and prepares a runtime for the
restored settings. A rejected file or preparation failure keeps the
current library. Once the transaction commits, the worker uses the new
runtime and the app reloads the library and preferences.

The restored desktop starts with the Local answer provider and keeps
historical provider attribution. API keys stay in the machine's credential
store. Recording sessions in the archive reopen paused and interrupted
requests remain cancelled. A subsequent provider change or recording
resume requires a user action.

The acceptance fixture must exercise both paths through the Settings
action and actual store worker. A declined replacement or invalid file
must preserve the current library. An explicit successful replacement must
reopen the original history and pending notes must be saved before worker
restore starts. Verify that restored settings select Local and that the
source archive is byte-for-byte unchanged. Use exact values for each
check. The CLI's restore options are described in the
[portable data format](../../docs/data-format.md).
