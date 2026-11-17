# Cueglint continuation record

The goal remains active: complete the pinned native desktop experience and the separately
inventoried companion/self-hosted tracks. No desktop release acceptance or product parity
is claimed. The initial workspace was empty; user work and shared history were preserved.

Public repository: https://github.com/thematheusmello/cueglint.
Latest public main: `72d88e8356eea06a959543af442778cfd850b254`, integrating
[PR #12](https://github.com/thematheusmello/cueglint/pull/12). Its final source
`41cc436768ffd296282a28b92f22e28019641628` passed
[three-platform implementation CI](https://github.com/thematheusmello/cueglint/actions/runs/44036051097).
The [public progress dashboard](https://thematheusmello.github.io/cueglint/) has a separately
recorded successful deployment. Historical failures remain in `parity/reports/`.

Current increment: [draft PR #13](https://github.com/thematheusmello/cueglint/pull/13),
`feat/native-markdown`, published first at `c042c3aea49d4f2709bacf6d39f60ec9d39271fd`. Native answers
render CommonMark paragraphs, code, lists and explicit validated HTTP(S) links. Copy
preserves the full original source. One cancellable/coalesced parser worker and bounded previews keep
formatting outside drawing. Conversation drawing borrows up to16 indexed replies; histories
above16 expose Earlier/Later/Latest windows and preserve an older request anchor on arrivals.
Continuous long-history scrolling and native extended-table layout remain gaps. Editable
notes and their autosave path are preserved; this increment does not generate notes.

Independent source review found and rechecked two defects: unbounded aggregate conversation
draw and stale link accessibility activation after a streaming rewrite. Both are resolved;
retained reports record initial/final source identities and remaining limits. The separate
notes/transcript detail view still clones Session; no global performance claim is made.
The macOS workspace run passed **154 tests including five doctests**, with two configured-weight
checks and one external numeric-oracle check excluded. Native OS selection, accessibility,
focus, rendering fidelity and timing are not established by those headless tests.

The new `cargo xtask native-screens` command builds the actual GUI and captures six labelled
original fixtures, including Markdown and an80-item list. PNG encoding/export runs off draw;
private atomic no-clobber writes and receipts preserve dimensions, scale, zoom and adapter.
A separate macOS CI job captures without touching the pending local permission handoff.
Its first attempt exceeded the original nine-minute cold-build limit before any GUI/frame.
The failed artifact/report is retained; the development build allowance is now fifteen
minutes. A Linux runner termination failure is also retained; its explicit end-of-options
correction passed12 macOS runner tests and awaits Linux CI. No native frame is claimed. Build/fixture limits kill owned
process groups and preserve failed reports/logs; sampled log sizes may overshoot between polls.
Primary GPU readback does not verify OS compositing/blur, real child windows or reference parity.

Working native/core capabilities include compact/expanded assistant windows, Settings,
sessions/history, edited notes, modes, bounded UTF-8 knowledge, copying/versioned exports,
transactional backup/restore and CLI access. Real local generation streams and cancels.
Models support integrity-checked downloads, pinned-folder import and removal with shared locks,
readiness invalidation and cache reset. A forced kill can leave an orphan import stage;
automated recovery remains open. Sparse shortcut overrides/disable/reset persist through
core, CLI and backups. Native registration, conflict/rebind guards and hidden-root wake are
implemented; actual OS hotkey, focus and native child accessibility verification remain open.

Qwen1.5B passed narrow original text criteria. The standard Whisper frontend passed its
frozen numerical controls; one actual corrected base.en CPU cold/warm pair still **failed
WER at0.30 against0.10**. Warm RTF0.495 passed. Inputs, source/model/binary endpoints and
independent scoring matched. No output retries, fallbacks, fixture or threshold changes
followed. Older WER0.50 and independent-path WER0.30 results, numerical diagnostics and the
initial runner's failed preflight remain visible. No broad ASR quality pass is established.

A new independently executed offline CLI run at source41cc passed all16 steps: file ASR,
a grounded answer, notes, save/reopen, versioned export, backup/restore and deletion. Source
and installed models were unchanged. The auxiliary artifact wrapper and redaction audit
failed after execution; read-only recovery and those failures are preserved. The documented
child numeric exit value is unknown; all16 CLI step exits were zero. This does not establish
live microphone/system audio, screen/GUI offline traffic, ASR quality or clean installation.

A12-speaker clean human-English corpus and one Whisper small.en CPU candidate are frozen
privately with licenses, selection rules, lossless checks and predeclared criteria. No
candidate/human-set inference has run. The next benchmark uses one retained warmup and one
scored fixed-order pass per profile; it cannot erase the separate v2 accuracy failure.

Reference: official notarized Cluely macOS2.1.28 installer/installed ASAR match;29 private
captures and23 bounded findings. Public specs/tokens retain measurements, hashes and unknowns.
Remote renderer/model versions, exact DPI and fresh onboarding are unknown. Reference
account/mode/capture-hiding state was restored. Commercial artifacts, recovered code,
account material, personal captures and backups remain outside this public repository.
Mobile/web/sharing/sync evidence is documentation-only and runtime-unpinned.

A user-only Screen Recording handoff remains pending in System Settings → Privacy &
Security → Screen & System Audio Recording. Cueglint's switch was observed OFF. Preserve the
existing frozen test app, Settings focus and new unsaved TextEdit original fixture. Do not
grant access or replace that frozen binary while waiting. The private continuation file
records exact handles and paths. No microphone/system source is enabled in that app.

The old Samantha system-voice waveform is absent from the active public tree. Historical
Git objects still retain it. A reviewed private path-only cleanup preview exists, but its
old leases are stale; no history rewrite/force push is authorized or performed. Refresh the
proposal only if the pending user decision authorizes cleanup. V2 media has separate
GPL-3.0-or-later terms; application code remains MIT OR Apache-2.0.

Not verified: real dual-source capture, device/echo/drift/recovery, screen freshness,
full offline GUI/traffic, native AX/global shortcuts, capture exclusion, multi-monitor/
fullscreen behavior, clean installs, Windows/Linux runtime and sustained resource/power
operation. Calendar, rich document parsing, generated notes/templates, broader settings,
sharing/mobile/browser/sync and distribution gates remain open in ROADMAP.md.

Registry:73 contracts;19 with partial executed reference evidence,3 implemented contracts,
0 matched passes. Generated counts do not represent a completion percentage. Earlier native
screenshots and typing/copy/save/reopen checks attest their recorded binary, not later source.

Recoverable data: installed development models remain in /tmp/cueglint-models; original
inference artifacts remain private. Only rebuildable task-owned build output was cleaned
when disk filled; models/evidence/dist stayed intact. Use separate target directories for
divergent worktrees. The private continuation file tracks current jobs and raw evidence.

Next exact steps:

1. Publish the reviewed runner corrections, table-scope correction and failed CI evidence.
   Inspect final PR13 three-platform CI and retry native frames with the bounded cold-build
   allowance; no visual result exists until actual PNGs are inspected.
2. Retain screenshot failures and geometry/content gaps. Advance native capture/selection
   checks only after the pending OS handoff; preserve the frozen app until that outcome.
3. Review the frozen human-ASR harness before inference. Keep strict scoring, timeouts and
   the v2 failure. Implement generated notes with edit-conflict and stale-result guards.
4. Continue all delivery gates and companion surfaces. Never mark an early gate or the
   first answer as completion of the assignment.
