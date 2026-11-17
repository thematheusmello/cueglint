# Reference research method

The reference investigation and implementation are separate. The reference worker
may inspect the shipped app and operate its UI. Implementation workers receive
only reviewed behavior descriptions, measurements, capability facts, evidence IDs
and hashes. No recovered client code, pseudocode, commercial binary assets,
credentials, personal captures, or reference account content enter this repository.

The raw work area is private and outside the project. It contains the downloaded
installer, original package inventory, provenance reports, prelaunch account-data
backup, captures and REA evidence export. Its parent directory is mode 0700. It
must never be added to this repository or to a public release.

The investigator first froze version, bundle identity, executable and ASAR hashes,
then checked the installed signature and Gatekeeper result. The official download
page was resolved to its Mac installer, downloaded, mounted read-only and verified.
Its application version, ASAR digest and signing identity matched the installed
copy. The installed app was left in place. The existing account-data directories
and preferences were backed up before launch.

REA identified the installed desktop application as an Electron candidate and
read the packaged fuse state. A summary-first static analysis selected only the
package manifest, main entry, preload entry and HTML entry. It did not execute
them. Four unpacked native-addon integrity declarations disagreed with the
companion bytes, while macOS deep strict signature verification passed. REA's
explicit record-and-continue policy retained those contradictions as untrusted
observations. No integrity protection was bypassed or modified. Selected-source
analysis, alias limits and node ceilings make graph coverage partial. Static
syntax findings are not claims of runtime behavior.

UI actions used the app's own controls through the available computer-use API.
Window captures and accessibility observations establish visible behavior only in
the recorded state. The existing account initially had capture hiding enabled;
the investigator temporarily disabled that setting to measure the windows and
restored it before handoff. Reading the account did not establish clean-account
defaults. Existing mode text, meeting titles, contact information and session
contents are excluded from reviewed specifications. A new-mode action created
one empty autosaved inspection draft; only that draft was removed. A second
clearly named temporary mode used an original fictional plain-text fixture for
ingestion and a document-grounded question. Its file and mode were removed.
The original mode and screen-context setting were restored, test chats were
cleared, and the final accessibility state confirmed the original capture-hiding
setting enabled. Existing meetings and modes were not edited.

Host storage exhaustion briefly interrupted observation. Actions were not
replayed blindly: the worker first read the saved state after storage recovered,
then continued only from verified fields. The project owner reclaimed only the
task's build output. Raw captures and account backups were preserved. The large
initial REA JSON export was archived with gzip; later complete Evidence is
checkpointed separately before the investigation is reset.

Each public capture reference contains a hash and neutral description, not the
image. Colors are sampled from composited screenshots. Font sizes and corner
radii are estimates unless stated otherwise. Remote renderer/service versions
are not frozen by the desktop package, so claims include the observation window.

The finding ledger uses observed, derived and unknown separately. Missing evidence
does not establish a missing feature. Implementation checks cannot establish
reference parity. First use, failure states, model quality, latency, capture
compatibility, cross-platform behavior and long-running operation require their
own matched scenarios before parity claims.
