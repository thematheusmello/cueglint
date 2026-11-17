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

The design review is explicitly paused, the other sessions are stopped/saved, and
the answer provider is local with unavailable model files. The long answer has 80
original list items. These are declared fixture states, not evidence of recording,
model readiness, live answers or reference-equivalent lifecycle behavior.

Legacy names retain the original builder, random IDs and wall-clock timestamps.
Old fixture inputs and reports are not rewritten. New native capture runs can use:

```sh
cargo xtask native-screens --fixture-set native-v2 --output-dir /fresh/artifacts --report /fresh/native-v2.json
```

That command opens native windows; run it only in an available desktop window.
The runner retains failures and binds scenario names, fixture SHA-256, source and
binary identities before/after the run. `ui/fixtures` also participates in the
source fingerprint. Fixed data does not freeze font loading, GPU/OS composition,
caret timing or DPI, and does not establish matched visual parity.
