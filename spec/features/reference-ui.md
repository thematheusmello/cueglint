# Reviewed desktop observation contract

Baseline: `cluely-macos-2.1.28`, observed on 2026-11-17. This document is an
original description of user-visible behavior. It contains no client source,
recovered code, commercial icons, screenshots, personal titles, or account data.
Measurements are from the captured window image at its observed size. Positions
below are local to that window. Approximate measurements are marked with `~`.

## Compact assistant

Evidence: `ui-widget-compact`, SHA-256
`e15652fed79123cf14853aa292e576a392851271487e9e69b140003e5984b4a2`.

The authenticated launch showed a borderless floating assistant, 690 × 104 px.
It has an approximately 20 px outer corner radius and two horizontal rows. The
upper input row is 56 px high; the lower action row is 48 px high. Both are
translucent gray. There is no title bar or large product heading in this state.

The input begins 15 px from the left, with approximately 15 px regular sans-serif
text. The send control is a 32 × 32 px rounded square at (646, 12), with a return
arrow. It remains visible in the empty state. Toolbar icons are approximately
18–20 px. The toolbar has settings at the far left, a central group of screen
context, visibility, mode, and listening controls, and History at the right.
The History label begins around x=592 and has a down-arrow control at x=644.

The accessibility surface identifies a drag handle, an editable input, a send
button, settings, a screen-context toggle, a visibility toggle, a mode pop-up,
a listening button, and history. The selected mode label is exposed to assistive
technology even when the control displays an icon.

The visibility toggle initially reported hidden-from-capture mode enabled.
The first screenshot was white. Turning that setting off through the app made
the actual widget pixels capturable. This is evidence of this capture's behavior,
not a guarantee about every screen-sharing application or OS API.

The mode pop-up opens below the toolbar. Its observed panel begins at (345, 104),
is approximately 168 × 270 px, and has about 9 px corner radii. Rows are about
32 px high with 13 px labels, an active checkmark and a separated management
action at the bottom. The choices shown depend on the existing profile and are
not the entire built-in template inventory.

Unknown: drag bounds, multi-display behavior, keyboard ownership, global shortcut
execution, resize constraints and animation timings. Response geometry is
recorded in `reference-sessions-assistant.md`.

## Expanded library

Evidence: `ui-widget-history`, SHA-256
`b306f388e1564d5f3305ead26d09ad30ed035dcfe52c4c80eb8ccf1e6f9979b6`.

Selecting History expanded the same assistant to 690 × 760 px. Its body is black;
the 56 px search header is near black. The header contains a 40 px round back
button at (8, 8), a search input beginning at x=55, and a search icon on the right.
The bottom toolbar is approximately 48 px high and stays below the scrolling
content. It includes settings and a new-chat action with shortcut hints.

The library begins with upcoming calendar entries. A refresh control and a
show-more control are present. Loading showed three skeleton rows before actual
upcoming entries appeared. The populated upcoming rows display a small date tile,
title, relative date, and time. Session history follows, grouped under uppercase
date headings. Observed date headings are approximately 11 px with expanded letter
spacing. Session rows are approximately 56 px high, with a 32 px document tile,
14 px title, duration pill, and 12 px time. The content has roughly 12 px outer
horizontal padding. A hovered or selected row has a rounded dark gray background.
The right scrollbar is narrow. A show-more action appears after the first history
page. Search and session details are recorded in
`reference-sessions-assistant.md`.

Unknown: entirely empty library, failed load, archive/delete semantics,
general search matching/ranking, and pagination boundary behavior.

## Settings window

Evidence: `ui-settings-general`, SHA-256
`b1255ce3323b0b8d6b79a0e0f3b757a0395a8b8d74f88c3816aa54fd9c6e701a`.

Opening settings created a separate native window, 860 × 670 px, with traffic
lights. A rounded sidebar occupies (6, 6, 204, 658). Main content starts at x=230
and has approximately 600 px width. The sidebar contains a 180 × 28 px search
field, account row, and 36 px navigation rows. General, Calendar, Modes, Keyboard
Shortcuts, Security, Billing, and About are present. Selection uses a rounded gray
row; category icons have distinct small colored tiles. Back and forward controls
appear at the top of the main area. Main content scrolls independently.

The General view has a rounded 600 × 184 px hero at (230, 60), a centered 56 px
settings tile, approximately 22 px title, and 12 px centered description. Below
it are a software-update card, a three-row preferences card, meeting usage,
language selection, audio, and account/app actions. Card rows are approximately
64 px. Preference switches are right-aligned. The observed controls cover
capture visibility, launch at login, meeting reminders, transcription language,
answer language, microphone source/test, onboarding reset, sign-out, and quit.
The software version displayed 2.1.28. These controls were observed; their full
effects were not tested.

The captured warm gray colors vary slightly across pixels because of compositing.
Use `ui/design-tokens.json` as measured approximations, not recovered style values.

The Calendar view has a matching hero card and an account connection row with a
management action. The existing account showed a connected calendar. The Security
view has sign-in, active-device and account-removal sections. No password or
account-removal action was opened. The About view has a release-note card, help
link, support action and installed-version row. Its performance claims were not
tested. The Billing view has a plan comparison, interval selector and billing
details action; no plan, payment or billing action was invoked. Its commercial
entitlement behavior is outside the observations.

## Modes and reference files

Evidence: `ui-settings-modes`, SHA-256
`1a10622f2bd48446ab3d5ac1fa4d999f5aaef25f49746af62a3b511a8268be56`;
`ui-new-mode`, SHA-256
`ea3ac107811b7c07683de066e4db17658c500f3a232bcc8e261b1b9c1173f22c`.

The Modes view uses a list at x=242–410 and a detail area at x=436–816. The list
has 40 px rows, a new-mode action, built-in choices and user-created choices.
Built-in choices include general assistance and several meeting or interview
types. Selecting an item previews its instructions and reference-file state;
the active choice is marked separately. A fixed activation area at the bottom
contains a blue 128 × 32 px button. Selecting a preview does not itself activate it.

Creating a mode immediately adds an empty automatically saved draft. Its editor
has a name, an ellipsis action menu, meeting-context text area, and reference-file
drop zone. The observed text area is approximately 368 × 200 px. File help lists
PDF, word-processing, presentation, plain text, Markdown and source-code files,
with a 20 MB per-file limit. The menu has a delete action. The investigator
removed only the empty draft created for this observation and did not alter an
existing mode. One original plain-text upload and document-grounded query were
later tested in a second temporary mode; see `reference-knowledge.md`. Failure
recovery and broad file coverage remain unknown. No existing mode prompt text
was exported.

## Keyboard settings

Evidence: `ui-settings-shortcuts`, SHA-256
`2424765618413ced8e42131e0fc6e09394f4081ed8979ff732ced152bc97fd5f`.

The shortcuts page is a two-column table with action labels, keycap-like hotkey
controls, and per-row reset arrows. It describes recording a new combination,
Escape cancellation, clearing with Delete, and restoring defaults. No shortcut
was edited. Rows with disabled reset buttons support the following observed
defaults: visibility `Command+Backslash`; new chat `Command+R`; settings
`Command+Comma`; session start/stop `Shift+Command+Backslash`; window movement
`Command+Arrow`; response scrolling `Shift+Command+Up/Down`. The question hotkey
had an enabled reset arrow, so its displayed value is a user customization and
is not recorded as a product default. Actual execution of these shortcuts is
unverified.

## Unobserved first use and platform states

This account was already authenticated. The investigator did not reset onboarding,
sign out, change credentials, change plan, replace the installed application,
grant new permissions, or export personal session content. First-use screens,
permission denial, billing changes, fresh-install behavior, external screen-share
compatibility, and other operating systems remain unknown. The current official
Mac download has the same ASAR digest as the running installation.

Private startup inspection found fixed application-data selection and shared
preferences. It did not establish a safe generic command-line profile override.
No alternate-profile launch or package modification was attempted. Fresh first
use needs an isolated OS account or an explicitly authorized profile reset.
