# Session and assistant observation contract

Baseline: `cluely-macos-2.1.28`, existing authenticated profile, 2026-11-17.
This is reviewed behavior, not a recovered implementation. Personal session text
and commercial assets are excluded.

## Saved meetings

A history row opens a 690 × 760 px detail view in the floating window. The 56 px
search header remains. The detail begins 20 px from the left with an editable
approximately 32 px bold session title. Date and participant chips, approximately
32 px high, sit below it. A tracked uppercase section label identifies notes or
transcript; its copy action is right-aligned. The bottom 48 px toolbar has Back,
the view toggle, and a resume action. Opening a populated session first showed a
spinner and loading label, then its contents.

Notes expose an editable Markdown surface. Populated notes have approximately
16 px text, 28 px line height and 18 px semibold subsection headings, with ample
spacing between sections. The investigator did not edit or copy existing notes.
An empty summary has a muted editing prompt and disabled copy action.

The transcript toggle retains the title, chips and footer. Empty transcript has
a centered muted message and disabled copy action. Populated transcript has an
approximately 11 px timestamp gutter at x=20, 10 px uppercase speaker labels at
x=84 and 16 px utterance text at x=84, with roughly 28 px line height. Speaker
labels distinguish self and other participant. A populated transcript enables
copy. This observation does not establish ASR accuracy, speaker-detection
accuracy, timestamp calibration, original-language correctness or live latency.

Cueglint uses the captured start position for the timestamp beside a recorded
utterance. Manual and legacy entries without an audio position use the time
they were added relative to the session start. The same offsets order the rows.

At a 690-unit window width, the Cueglint transcript reserves the first 64 logical
units of each row for time and spacing. The timestamp starts at x=20, and the
speaker and all wrapped lines start at x=84. Selectable utterance text is set
at 16 units with a 28-unit line pitch. The headless regression checks actual
text-run bounds for an original paragraph. The reference display scale and
font identity are still unresolved.

Resume controls were observed but not used on existing sessions. Persistence,
rename semantics, copy fidelity, interrupted save recovery and live capture
remain untested.

## Library search

Typing in the library search field filters the session list without submitting
an assistant question. An uppercase token matched an existing mixed-case title.
A unique unmatched token produced a no-results notice. Both states show a leading
action that can ask the assistant the typed query. The no-results notice is
centered near y=176 in muted approximately 13 px text. Result rows retain date
grouping, icon, title, duration and time. This establishes one case-insensitive
title match; body search, ranking, accents, punctuation and delayed response
ordering remain unknown.

## Assistant probe

The investigator selected the built-in general mode and disabled screen context
before submitting this original script:

> Reply with exactly: The cobalt kite is ready.

No microphone session was started and no user screen fixture was submitted. The
completed response contained exactly `The cobalt kite is ready.` This is one
deterministic prompt observation and does not establish general model quality.

Submitting expanded the assistant to 690 × 500 px: a 56 px input header, 396 px
conversation body and 48 px bottom toolbar. The body remained translucent gray,
unlike the opaque black library. The header gained a back action and follow-up
input. While generation was pending, the send action became a stop action and
the accessibility surface reported loading. A small white activity dot was
visible during a later pending request. On completion the action became send
and a response-copy control appeared. The footer changed from history to a
new-chat action with its keyboard hint.

The short user query occupied a right-aligned rounded bubble at approximately
(366, 74, 295, 40). A longer query wrapped in a wider bubble. Assistant text starts
at x=20, around y=132, in approximately 14 px text. The input remained a single
horizontal line for a long typed prompt; its text scrolled horizontally instead
of growing vertically in this observed state.

### Short answer comparison

The `assistant-short-v1` fixture replays this exact original question and answer
in a 690 × 500 window. It uses synthetic session records, disables capture and
inference and retains Cueglint’s fixture marker. The reference is the November 17
`ui-assistant-complete` observation from Cluely 2.1.28.
[Capture identity](../../ui/reference-manifest.json) · [Measured geometry](../../ui/design-tokens.json)

Compare the bubble top and right edges to y=74 and x=661 within two logical
units. Compare its width and height against 295 × 40 within the registry’s existing two-unit geometry tolerance.
Measure the assistant text left edge against x=20 within two units, and record
its first ink near y=132 separately. Use the full native frame with its reported
pixel scale. Report each difference individually. The reference display scale
and used typeface remain unresolved.

An original five-stanza poem prompt was submitted and stopped while the loading
indicator was present. The loading indicator disappeared, the send action
returned, and no assistant poem was visible in the immediate cancelled state.
No special cancelled-message label was visible. A later reinspection still
showed the send action and no poem response. This establishes cancellation during
pending generation in this run; partial-token cancellation and adversarial
late-message isolation remain unknown. New chat cleared the test conversation
and collapsed the widget to 690 × 104 px. The next observed meeting-library page
had no new meeting entry for these text-only probes; general chat persistence
is unverified.

An original 80-item fictional workshop prompt showed a pending activity dot and
later a completed numbered list. The list used real numbered-list accessibility
semantics, approximately 14 px text and 32 px row pitch. A right scrollbar and
a round scroll-to-bottom action appeared for long content. The investigator did
not capture an intermediate token update, so incremental streaming is unverified.
Exact latency, retry and error presentation still need evidence. No offline or
local model behavior is inferred from this hosted reference.

## Controlled live capture gap

The existing microphone source is a physical microphone. The investigator has
not established a source routing setup that isolates a synthetic original audio
fixture from ambient microphone input and other system sounds. Starting live
capture without that isolation would record the user's environment. Live
start/stop, partial/final transcript evolution, meeting-note generation and live
audio-grounded assistance remain blocked pending a controlled source. Existing
transcripts provide layout evidence only.
