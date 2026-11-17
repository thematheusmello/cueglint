Independent visual repair proposal — native run 002

Reviewed six actual Cueglint GPU PNGs against eight retained Cluely macOS 2.1.28 captures. This is a diagnostic review, not a reference-parity pass. File hashes, source identities and measurement methods are in evidence.json. No private text or pixels are exported.

Practical repair order

1. Center the existing assistant input text in its 56 px header. The near-identical follow-up hint has visible glyph rows y=17–26 in Cueglint and y=23–33 in the reference, approximately 6 px higher. Compact input shows the same direction. The captured source already requests 15 px text; adjust placement and inspect the actual font metrics before increasing the size. Preserve the 32 px send rectangle at (646,12), header height and horizontal prompt scrolling. Exact font and text-baseline identity remain unmeasured.

2. Finish detail heading and chip styling. Keep the corrected 28 px note pitch and 32 px chip height. Native chips have 5 px corners and bright gray fill; retained detail uses dark capsules with approximately 16 px ends. Use original glyphs if adding calendar/person icons. The editable title looks regular and sits approximately 8 px higher than the retained bold title, while both chip rows start at y=130. Improve weight and visible placement while preserving editing, saving and chip position. Inspect short and wrapping titles after a repair.

3. Tighten Settings navigation spacing. Selected rows are approximately 36 px tall in both captures, but native rows have 42 px pitch and the reference 38 px pitch. General starts around y=160 versus y=154; colored tiles span approximately 19 px versus 23 px. Reduce the gap before changing row height. Retain Cueglint's additional Knowledge entry and original icons. The main hero already has the intended (230,60), 600×184 footprint.

Other observed differences

Settings replaces the reference back/forward capsule with a General label. Its first Preferences card presents different controls. This establishes a hierarchy difference, not absent setting effects; inspect existing behavior before adding navigation or OS toggles. History date headings use ISO date/UTC instead of uppercase weekday/month text with tracking; this is a later formatting repair. Upcoming entries cannot be judged with no connected calendar. Completed native answer fixtures show setup and transcript/notes actions taking roughly the lower 110 px; model-unavailable and hosted-ready conditions differ, so ready-state visibility needs a separate labelled comparison. Preserve the existing fixtures.

Qualified comparisons

The detail fixture is active; retained detail is saved/idle. Pause/End versus Resume is not a valid behavior failure. Native long Markdown is at the end of its list; reference long content is near the beginning. There is no observed horizontal clipping or footer overlap. The partial native top row is a scrolling boundary. Its scrollbar is not visible; different positions and unknown hover/fade prevent proving missing scrollbar or scroll-to-bottom behavior.

Reference DPI, font, service and backend remain unknown. Native RGBA readback and host-composited RGB reference cannot support opacity/blur or full-image MAE claims. Leave chosen alpha 218/210 and declared tolerances unchanged. Branding and free/local operation are intentional differences. Primary-viewport fixtures do not verify real child windows, focus, OS materials, capture exclusion, accessibility or shortcuts.

Fixture repeatability

Captured source bf1d uses ordinary Store::start_session, assigning wall-clock milliseconds and random UUIDs. Visible date/time therefore vary; equal updated_at values use random IDs to break history ties. Possible row-order variation is source-supported inference, not observed across reruns. Preserve run 002. For future regression runs, freeze clock/identities only in explicit versioned fixture construction and retain old evidence; do not change production sorting or rewrite fixtures to make comparisons pass.

Skipped and risk: no builds, runtime interactions, model/audio, network or pixel-scoring run; original source reads were limited to Cueglint fixture/layout code. This proposal does not approve whole-product parity or completion.
