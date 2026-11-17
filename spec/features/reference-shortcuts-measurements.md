# Retained shortcuts-table measurements

Measured on 2026-11-17 from **existing** capture `ui-settings-shortcuts`,
860 × 670 image pixels, SHA-256
`2424765618413ced8e42131e0fc6e09394f4081ed8979ff732ced152bc97fd5f`.
No new capture, GUI action, implementation inspection or client-source inspection
was performed. Account/sidebar contents and customized binding values are omitted.

Coordinates start at the saved window image's top-left corner; y increases down.
All values are **screenshot pixels**, not established logical points. The file
has JPEG raster encoding despite its filename extension. Edge/ink measurements
therefore have approximately 1–2 px uncertainty. No DPI conversion is justified.

## Composition above the table

The top area has compact back/forward navigation and the plain title
**Keyboard Shortcuts**. There is no hero card, large category icon, introductory
paragraph or secondary description between that title and the column headings.

| Element | Measurement in screenshot pixels |
| --- | --- |
| Back/forward pill | Approximately `(230, 6, 80, 36)` |
| Title | Nominal left x=323; painted ink bounds `(324, 19)` to `(457, 33)`; baseline about y=30 |
| Column heading **Name** | Nominal left x=246; ink bounds `(247, 74)` to `(276, 82)` |
| Column heading **Hotkey** | Ink bounds `(744, 74)` to `(780, 84)`; no heading above the reset icons |
| Column-heading baseline | Approximately y=82 |
| First horizontal rule | y=97, approximately x=230–830 |
| Header band | Approximately y=60–98; its invisible top boundary cannot be recovered exactly |

There is roughly 40 px between the title baseline and column-header ink, then
about 15 px between the column-heading baseline and the first rule. Adding a
hero or introductory paragraph would change this observed composition.

## Table geometry and alignment

The table is flat on the warm dark window background, approximately x=230–830
(600 px wide). There is no separately painted rounded card or alternating row
fill in this capture. The sampled blank background is `#2d2928`; sampled rules
are `#3a3637` at y=97 and `#363233` at y=145. These are composited JPEG samples.

| Element | Measurement in screenshot pixels |
| --- | --- |
| Body-row pitch | **48 px**, repeated across eleven visible single-line rows |
| Row centers | y=`121 + 48n`, for n=0–10: 121 through 601 |
| Visible rules | y=97, 145, 193, 241, 289, 337, 385, 433, 481, 529, 577 |
| Last-row bottom | Inferred around y=625; no final rule is visibly painted there |
| Action icon | Painted center approximately x=255 and the row center; first icon ink about 16 × 14 px |
| Action label | Nominal left x=274; first-row ink y=116–128; baseline approximately y=125, repeated every 48 px |
| Keycap | Approximately 20 × 20 px; first-row top about y=111; rounded corners approximately 3–4 px |
| Keycap spacing | Approximately 2 px between adjacent caps |
| Keycap-group alignment | Right edge about x=779; one/two/three-cap group left edges about x=759/737/715 |
| Reset icon | Visible glyph about 10 × 10 px, centered at x=805 and the row center; second-row ink bounds `(800, 164)` to `(810, 174)` |
| Reset/cap separation | About 21 px from the last cap's right edge to the visible reset glyph's left edge |

Action labels occupy one line. No per-row status, explanatory or source line is
visible. The last row remains above one global shortcut-editing help line, whose
painted text is approximately x=234–796, y=640–651, with baseline near y=650.

## Typography and overflow limits

The raster suggests system sans-serif text: title approximately 14 px semibold,
action labels approximately 13 px regular, and column headings, keycap glyphs
and global help approximately 11 px. Painted glyph heights are about 14 px for
the title, 10–12 px for action labels, 8–10 px for headings/help and 9 px for a
sample keycap glyph. These are visual estimates, not recovered font declarations.
The title, header and action baselines above are approximate within 1–2 px.

A narrow vertical scrollbar is visible near x=852–858, starting around y=63 and
continuing nearly to the lower window edge, around y=663. Its lower boundary
merges with the rounded frame. All eleven rows and the global help line are
visible; no horizontal scrollbar, wrapped action label or clipped keycap is
visible. A single image cannot establish the scroll range, exact thumb extent,
overflow amount or behavior at another window size.

Transparent hit targets, actual table-column boundaries, exact font face/weight,
pixel-to-logical scaling, hover/focus/shortcut-recording states and responsive
behavior remain unknown. Visible glyph bounds are not measurements of clickable
button bounds.
