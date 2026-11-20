# Native fixture data v2

`native-v2.json` is an original, authored Cueglint backup with fixed IDs, UTC
timestamps, transcript segments, completed answer references, modes and settings.
The fixture version is 2; the portable backup schema remains 1. No reference
capture, recovered source, model output or personal data is included.

The `native-v2-` names select the same primary viewports as legacy `dashboard`,
`overlay`, `history`, `session`, `markdown` and `markdown-long`. All v2 views restore
this backup into a pristine fixture-only in-memory Store. Session and overlay
select `native-v2-design-review`; Markdown views select their corresponding fixed
session. Dashboard/history have no selected session. All four sessions remain in
the restored Store, so ordinary commands act on the displayed IDs.

The additional `provider-v2` scenario uses the same backup without changing those
six v2 views. Its separately hashed `provider-v2.json` supplies an original caption,
compatible-server draft, manual model/endpoint and empty password at 860×670. It
scrolls the existing General content to the actual provider controls. The selected
answer provider stays local; fixture network/key operations remain disabled in
both UI and actor. Screenshot readiness requires the panel/key/charge text to be
visible. It demonstrates rendering, not OS interaction or provider inference.

The separate `calendar-v2` set contains `native-v2-calendar-settings` (connected
Settings, 860×670), `native-v2-calendar-history` (upcoming events, 690×760), and
`native-v2-calendar-reminder` (compact assistant with a due reminder, 690×164).
All three use the same in-memory session backup and the original sample state in
[`calendar_fixture.rs`](../../crates/cueglint/src/calendar_fixture.rs). History
starts with three visible entries and can expand to the fourth locally. The
reminder fixes the preview clock at 09:28 UTC on 2026-10-06, two minutes before
the original Harbor exhibit event. It uses the live reminder layout with an
explicit sample label and disabled **Start** and **Dismiss** actions.

On macOS, capture these three frames separately from the legacy and native-v2 sets.

```sh
artifacts=$(mktemp -d "${TMPDIR:-/tmp}/cueglint-calendar-frames.XXXXXXXX")
cargo xtask native-screens --fixture-set calendar-v2 --output-dir "$artifacts/calendar-v2" --report "$artifacts/calendar-v2/report.json"
```

Hosted macOS uploads the separate `native-calendar-fixture-frames-<sha>`
artifact with all three frames, sample source hash, and failed-run evidence.
The reminder capture requires the rendered banner, matching logical dimensions
and visible pixels in its band. The existing
`calendar_fixtures_show_sample_states_and_disable_requests` headless test checks
the exact reminder title, simulated time caption, and disabled actions.

The design review is explicitly paused, the other sessions are stopped/saved, and
the answer provider is local with unavailable model files. The long answer has 80
original list items. These are declared fixture states, not evidence of recording,
model readiness, live answers or reference-equivalent lifecycle behavior.

Legacy names retain the original builder, random IDs and wall-clock timestamps.
The legacy History builder gives the two authored rows a deterministic update
order when their timestamps would tie.
Old fixture inputs and reports are not rewritten. New native capture runs can use:

```sh
cargo xtask native-screens --fixture-set native-v2 --output-dir /fresh/artifacts --report /fresh/native-v2.json
```

That command opens native windows; run it only in an available desktop window.
The runner retains failures and binds scenario names, fixture SHA-256, source and
binary identities and provider view-state hash before/after the run. `ui/fixtures` also participates in the
source fingerprint. Fixed data does not freeze font loading, GPU/OS composition,
caret timing or DPI, and does not establish matched visual parity.

The exact `e66444f` native run retained both original Calendar frames.
[Settings preview](../../docs/images/native-calendar-settings-001.png) ·
[History preview](../../docs/images/native-calendar-history-001.png) ·
[Independent verification](../../parity/reports/native-calendar-fixtures-001.json)

The exact `81b2c7b` native run verified the third Calendar frame.
[Reminder preview](../../docs/images/native-calendar-reminder-001.png) ·
[Independent reminder verification](../../parity/reports/native-calendar-reminder-001.json)

The `transcript-v1` set opens one stopped meeting with two original wrapped
utterances and another with an empty transcript. Both use the normal detail
view and a versioned, in-memory backup with fixed session and segment times.
The populated view enables Copy. The empty view displays its empty message
and disables Copy. Selecting Show notes and then Show transcript preserves the
meeting. The runner waits for the selected transcript and settled fonts and
verifies the 690 × 760 logical viewport within two units.

For independent inspection of the populated native frame, the first timestamp
begins at x=20 and the utterance column at x=84. The tolerance for the left
edge of visible ink is four logical units. Adjacent baselines within the same
utterance have a target pitch of 28 units with one unit of tolerance. Use the
scale in the frame receipt. These checks concern the recorded geometry only.

```sh
cargo xtask native-screens --fixture-set transcript-v1 --output-dir /tmp/cueglint-transcript-frames --report /tmp/cueglint-transcript-frames/report.json
```
