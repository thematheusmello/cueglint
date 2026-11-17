# Cueglint original identity

The open **C** holds a glint in its opening: context, with a useful cue. The broad
curve and short rounded terminals keep the silhouette readable at small sizes.
The glint is the union of two convex diamonds, so the native renderer fills it
correctly without treating a concave star as a convex polygon. The lowercase
wordmark uses original path geometry; it has no font dependency.

| Asset | Use |
| --- | --- |
| `cueglint-mark.svg` | Teal mark, transparent, 64-unit view box |
| `cueglint-mark-mono.svg` | Warm dark monochrome mark for light surfaces |
| `cueglint-mark-inverse.svg` | Off-white monochrome mark for dark surfaces |
| `cueglint-mark-{16,24,32,64,128,256,512}.png` | Transparent teal raster exports |
| `cueglint-mark-mono.png`, `cueglint-mark-inverse.png` | Transparent 256 px monochrome exports |
| `cueglint-logo.svg`, `cueglint-logo-inverse.svg` | Transparent mark and outlined wordmark, for light/dark surfaces |
| `cueglint-logo.png`, `cueglint-logo-inverse.png` | Transparent 1328 × 320 px lockups |
| `cueglint-app-icon.svg`, `cueglint-app-icon-{256,1024}.png` | Warm dark app tile with transparent corners |
| `Cueglint.icns` | macOS icon, 16–1024 px representations |
| `preview.png` | Visual review on solid light and dark surfaces, including 16/24/32/64 px marks |

Use the mark at **16 px minimum**, preferably 24 px or larger. Leave at least
8 units of clear space around its visible bounds on the 64-unit grid. Use a
monochrome variant for small marks on light surfaces. Keep the original aspect
ratio; do not add a second sparkle, a shadow, or a gradient.

| Color | Value |
| --- | --- |
| Cueglint teal | `#95DCCC` |
| Warm dark | `#2C2829` |
| Off-white | `#F4F2F3` |

The palette comes from the existing Cueglint native UI. The mark, wordmark,
app tile and their geometry were authored for Cueglint on 2026-11-17. No external
image, font outline, commercial product mark, recovered source or personal
capture was used. No image model was used. This identity makes no reference
parity claim.

All artwork, exports, authoring code and native rendering code are available
under **MIT OR Apache-2.0**; see [LICENSE](LICENSE) and the repository license
texts. `manifest.json` records source and export hashes and the checks performed.

The native renderer is `crates/cueglint/src/brand.rs`:

```rust
brand::mark(ui, 24.0, teal);             // Allocates a square and paints vectors.
brand::paint(painter, center, 24.0, ink); // Paints inside an existing allocation.
viewport.with_icon(brand::app_icon());   // Decodes the embedded PNG at startup.
```

Painting performs no file I/O, image decoding or background work. The C curve
uses 32 segments in egui and cubic curves in the exports; both share the 64-unit
geometry. `app_icon()` uses the project's existing `image` dependency.

To regenerate the SVG, PNG, preview and ICNS exports on macOS:

```sh
swift ui/brand/export.swift ui/brand
```

This authoring command uses system CoreGraphics, AppKit and `iconutil`. It is
not part of the application's build or runtime. PNG dimensions, actual alpha
values and ICNS round-trip contents were checked after export; the native
module contains focused tests for its embedded icon and drawing bounds.
