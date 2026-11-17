# Pinned client typography declarations

Baseline: `cluely-macos-2.1.28`. Review date: 2026-11-17. This is a
reviewed, non-code specification of shipped client declarations. It does not
establish computed browser styles, an active native font, display scale, font
rasterization or a visual comparison pass.

The retained official installer was rechecked read-only: DMG SHA256
`8d10f720c6eec4f07703e96b0f3808bdc0cea782007c154a8ed07291b4118dc4`,
ASAR `aa2434cbaba31a0fb1ed189db8d02a9f1c9d84e9e098a02984f0dc306cabcd88`.
REA summary-first analysis and selected evidence were retained privately before
exporting this specification. Its private reviewed-spec SHA256 is
`1601be78c2fb6082514580ab094b856cc351e14fb721994f9459165140dfffc5`;
final REA bundle SHA256 is
`73f66ad4eef7b5a68b9721b20ab338c89cc91b1153c1cf88d30c203b0103ce19`.
Static declaration evidence and earlier runtime screenshots remain distinct.

The declared proportional family prefers the Apple OS system face, then
BlinkMacSystemFont, system-ui, Segoe UI and generic sans-serif. These are family
preferences, not proof of which face rendered a particular screenshot. Inline
code has a separate monospace family. The packaged font inventory contains math
fonts; no redistributable proportional UI font was established.

| Role | Declared size | Declared weight | Declared line height / tracking |
| --- | --- | --- | --- |
| Assistant input and hint | 15 CSS px | 500 | 1.25 rem |
| Plain answer | 0.875 rem | normal | 1.71429 × font size |
| Submitted question | 13 CSS px | 500 | 1.25 rem |
| Settings navigation and row labels | 13 CSS px | 500 | unspecified here |
| Settings explanation | 11 CSS px | normal | 1.4 × font size |
| Section heading | 12 CSS px | 500 | unspecified here |
| Large page heading | 22 CSS px | 600 | 1.75 rem; −0.025 em |
| Page description | 12 CSS px | normal | 1.45 × font size |
| Saved meeting title display and editor | 32 CSS px | 600 | 1.25 × font size; −0.01 em |
| Metadata chips | 14 CSS px | 500 | 1.0 × font size |
| Library title | 14 CSS px | 500 | unspecified here |
| Library metadata | 12 CSS px | 500 | tabular numerals |
| Inline code | 13 CSS px | separate monospace stack | unspecified here |

The root rem size is unverified. A 16 px root would yield a 14 px answer with
approximately 24 px line height; that conversion is conditional, not measured.
CSS pixels are not established to equal native logical points or backing pixels.
Normal is an inferred conventional 400 where no explicit numeric weight was found.

Cueglint may resolve system fonts through documented OS APIs at runtime, with
bounded loading outside drawing, verified collection face/variation identity and
an embedded fallback. Record the selected face, index, axes, byte hash and fallback
reason in developer capture metadata. Do not copy reference fonts or redistribute
Apple font files. A candidate one-CSS-pixel-to-one-logical-point mapping is an
implementation decision requiring later matched-scale checks.

The installed application separately auto-updated to **2.1.29**, observed at
2026-11-17T15:09:56.211900Z. It was not launched or substituted as the baseline.
Future reference execution must re-establish the chosen build or declare a new
baseline. Hosted renderer/model identities and actual used fonts remain unknown.
