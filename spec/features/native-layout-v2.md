# Reviewed native layout repair v2

This original implementation contract follows the partial diagnostic in
`ui/reviews/native-runtime-002.md`. It is not a matched-reference verification.
Reference font identity, DPI, materials, opacity and blur remain unknown.

The existing assistant input moves down six logical UI units inside its
56-unit header. Its width, native single-line editing/IME/scrolling, send rectangle
at (646, 12) with size 32×32, and header/toolbar alpha 218 remain unchanged.
Conversation body alpha stays 210. Native pixels still require a separate capture.
Editable text remains 15-point; egui's independent hint atoms retain Body 13-point.

Detail chips retain 32-unit height and the row beginning at y=130. Their ends use
radius 16 and an original dark fill (`#252525`), chosen from the reviewed dark
capsule description rather than claimed as a measured reference color. Shortcut
keycaps retain their own compact geometry and fill. The editable title moves down
eight units; it retains 32-point single-line horizontal editing. A 700 weight is
requested only when the already-used installed font exposes a `wght` axis, within
that axis's supported range; otherwise it retains its regular fallback. No font
bytes are bundled and no synthetic bold or reference font identity is claimed.

Settings navigation begins at y=154, retains 36-unit rows, uses 38-unit pitch and
23-unit original colored icon tiles. Knowledge remains present. The main hero
keeps its existing 600×184 footprint and content anchor; no inert settings are added.

Future regression capture uses explicit `native-v2-` scenario names and the authored
fixed-ID/time backup in `ui/fixtures/native-v2.json`. Legacy names, builder and
evidence remain separate. Headless checks establish layout/editing and fixture
replay behavior only; real child windows, OS accessibility/materials, capture
exclusion and reference parity remain separate observations.
