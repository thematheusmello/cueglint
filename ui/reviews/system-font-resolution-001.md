System-font resolver feasibility — source proposal 001

Basis: Cueglint integration 00ff533e60bbac6d6d93ebb22112865630c21c9a and
[the accepted declaration-only contract](../../spec/features/reference-typography-static.md).
No reference source, font file or private capture was accessed for this proposal.
The prototype module is enabled only for GUI test builds and has no production caller yet.
The existing font loader, sizes, geometry, fixtures, alpha 218/210 and evidence
remain unchanged in this source-only phase.

Installed native API proof

| Host | Installed API path | Ownership and limits |
|---|---|---|
| macOS | objc2-app-kit 0.3.2 `NSFont::systemFontOfSize_weight`, with the public Regular/Medium/Semibold constants. SDK 27.2 `CTFont.h` explicitly declares NSFont/CTFont bridging. `CTFontCopyAttribute(kCTFontURLAttribute)`, `CTFontCopyFamilyName`, `CTFontCopyPostScriptName`, `CTFontCopyVariation`, `CTFontCopyTraits` and `CTFontCopyTable` supply the selected local face and coordinates. | An autorelease pool, retained NSFont and owned CF Copy results are created and dropped on the loader thread. Only Rust strings, hashes, coordinates and in-memory bytes leave it. Non-file URLs or unavailable local bytes fail to the embedded fallback. |
| Windows | windows 0.62.2 has `SystemParametersInfoW(SPI_GETNONCLIENTMETRICS)` and the OS message LOGFONT. `IDWriteFactory::GetGdiInterop` / `CreateFontFromLOGFONT` match that family at the requested weight. `GetIndex`, `GetFiles`, the local file loader and optional `IDWriteFontFace5::GetFontAxisValues` supply local-file identity and coordinates. | COM interfaces and borrowed reference keys stay on the loader thread until copied. No downloadable font collection is requested. Simulated styling, nonlocal loaders, UNC paths and multiple-file faces fall back. This source path has not been built or run on Windows. |
| Linux and other hosts | Existing egui embedded fonts. Native Fontconfig was considered; its installed C API can match a system family, but it would introduce an additional platform dependency or optional loader. | No Fontconfig dependency, subprocess or new user preference. Metadata explicitly records unsupported native resolution and the embedded fallback chain. |

The Rust bindings and SDK headers above were read locally. AppKit's installed
NSFont header recommends its weight factories rather than transforming a font
instance. The public CoreText descriptor API does not expose a TTC face index.
Hidden system PostScript aliases also need not equal a file's OpenType name.
The macOS prototype therefore hashes the native `head`, `name` and `maxp` tables
and requires exactly one matching face in the OS-resolved file. It never assumes
index zero. Windows supplies its face index directly; the file/index is parsed
before adoption. Invalid or ambiguous files produce an explained fallback.

Native axis tags and values are validated against that exact file face. Unknown
axes, missing variable-font coordinates, nonfinite values and values outside the
file's ranges fail to fallback. No guessed weight or optical-size coordinates are
substituted. Native normalized AppKit weight or DirectWrite weight is recorded
separately from the requested CSS weight. CoreText/PostScript alias and SFNT name
are also separate receipt fields.

The prototype limits each file read to 32 MiB, a role set to 96 MiB and 16 roles,
collections to 256 faces, coordinates to 64 axes, native names to 4096 characters,
and each native identity table to 1 MiB. A bounded read detects growth beyond its
limit even when filesystem metadata changes. Font bytes stay in process memory;
there is no copy into the repository, application resources, logs or receipts.
Direct dependencies reuse versions already present in Cargo.lock: AppKit/objc2,
Core Foundation, Windows, skrifa (the parser used by egui) and sha2. The GUI's lock
dependency associations were updated from that existing package inventory without
adding packages or versions. Cargo has not validated the association or feature
selection during the source-only hold.

Receipts contain role, requested points and CSS weight, native API/family/name,
SFNT name, file basename/hash/length, face index, native weight, variation values
and fallback reason. The default embedded proportional chain is named and hashed
even when a native primary succeeds, because missing glyphs can use that chain.
The loader's payload wrapper has no Debug or Serialize implementation. Receipt
metadata is for developer/screenshot evidence, not settings, backups or the
product interface.

Separate typography wiring

The reviewed stylesheet is static declaration evidence. It prefers the Apple OS
system family and system fallbacks, with no alternate UI body face. The shipped
math fonts do not identify a UI body font. These facts do not establish the
computed root size, actual runtime face, optical axes, display scale or DPI.

After the public declaration contract and source review, a separate change can
map the declared pixel sizes to logical egui points as an explicit candidate.
It must preserve the 56-point assistant header and send rectangle, horizontal
editing/IME/scrolling, detail editing and chips at y=130. It must also measure the
40-point candidate title line within the existing 42-point editor rectangle and
preserve the 32-point chip heights. The 14/15/32-point probe roles are feasibility
samples, not a claim that CSS rem units or a browser's optical sizing have been
reproduced. The 14-point sample does not assert a 16-pixel computed root size.

Production integration will replace the hardcoded-path loader with one bounded
loader job, adopt its result outside drawing, and bind screenshot readiness and
metadata to the adopted font set. Native egui shaping and rasterization can still
differ from the reference even with the same OS family. Existing fixture names
and authored v2 data remain intact; changed typography scenarios require a new
labelled fixture identity. No matched font or visual parity is established here.

Precise integration touchpoints, after feasibility review

| Caller or data | Later change and constraint |
|---|---|
| `main.rs` | Remove the prototype's test-only module gate only after the native probe. Existing options gain no font preference. A later explicitly named typography fixture version must be separately parsed; legacy and native-v2 names remain unchanged. |
| `app.rs::configure`, `Cueglint::new` and `Cueglint::poll` | Replace the current hardcoded-path font thread with one independent startup loader and one bounded result slot. Install an adopted family set in logic, outside drawing, and bind its identity to the context after egui applies it. No work enters the Store or inference actors; no Core settings, notes, sessions or migrations change. |
| `question_input` | Use the 15/500 family for typed text and explicit RichText hint. The current string hint inherits Body13 despite the typed field's size15. Preserve header56, input box y21–49, the send32 rectangle at (646,12), single-line scrolling and IME. Rem-based line height remains a separately labelled assumption. |
| `main_window` / `nav_row`, `row`, `section`, `hero` | Bind declared sizes/weights to native family requests by size and weight. Keep rows36/pitch38, General y154, tile23 and the hero's existing (230,60), 600×184 rectangle. No sidebar/page/control redesign. Explanations, descriptions, line heights and tracking require explicit role formatting rather than a global body-size increase. |
| `conversation`, `markdown/renderer.rs::Typography::answers` and `show` | Submitted-question candidate13/500; answer14/400 and line24 are conditional on the unverified root16 assumption. Current Markdown already requests body14 with automatic line height. Preserve source bytes, links, selection, copy, bounded parsing, streaming and identities. Notes16/28 remain a separate measured role. |
| `detail`, `detail_chip`, `history_row` | Candidate title32/600, line40/tracking−0.32 within the existing editor rectangle; chips14/500/line14 in the existing32 capsules at y130; library title14/500 and metadata12/500. Preserve title ID, Rename acknowledgement path, editing and horizontal scroll. Existing measured notes, chip row and generic shortcut keycaps remain intact. |
| `screenshot.rs::Metadata`, `Receipt::json`; `app.rs::screenshot_content_ready` and `capture_screenshot` | Add the adopted font-set identity/role receipts next to existing UUID, scale and adapter fields. Gate explicit capture until the adopted or fallback set is applied in the context. The one-shot request/ROOT token, actual copy/map diagnostics and private PNG export remain authoritative. Font bytes never enter metadata or snapshots. |
| `main.rs` fixture mapping, `worker.rs` fixture selection, `xtask::FixtureSet`, source fingerprint and `ui/fixtures` | A later typography-v3 view/scenario identity can reuse the unchanged authored native-v2 backup hash and stable IDs while declaring new rendering metadata. Add matching viewports/captions/runner fingerprints and explicit new reports. Preserve legacy builders, native-v2 data, provider-v2 I/O guards and every historical run. No production clock or sorting hooks. |
| `ui/design-tokens.json`, feature spec and existing layout tests | Record declaration evidence and the candidate mapping separately from capture measurements. Add meaningful size/family, title editing/clip, hint/typed, chip/nav geometry and metadata tests. Root retains registry/STATUS integration and observed/native proof states. |

Installed egui 0.35 `RichText::strong` changes color only; it does not request a
heavier font. Native weights therefore need real resolved faces/coordinates and
named families. `RichText` does expose explicit line height and letter spacing;
single-line editor tracking needs a layouter that keeps egui's cursor/IME mapping.
The native API can choose optical coordinates by requested size, so a 14-point
face must not silently serve as proof for every other size. A bounded list of
distinct size/weight requests can share identical validated font data afterward.
The Windows prototype's DirectWrite family/weight match is size-independent;
the requested size is applied by egui. It records native axes without inventing
an optical-size override, so Windows CSS optical sizing remains unverified.

Tabular numerals are declared but are not established by loading an OS face.
AppKit's digit-system-font factory can set font features that file bytes and
variation coordinates alone do not transfer into egui's current layout path.
Keep that feature as an explicit gap until verified; do not substitute a new
monospace UI face or claim that the ordinary system-face resolver implements it.

Verification state

Source tests cover oversized/empty reads, bounded consumption, role-count rejection,
invalid containers, exact native-index
validation, unique table matching, ambiguous duplicate TTC faces, and fallback
receipt formation without executing OS font APIs. An ignored developer probe
can explicitly resolve the three sample roles and print identity-only receipts on
macOS/Windows. It verifies glyph coverage and lays out an original ASCII sample in egui using each native
family alone, in memory without a native window or GPU. It fails if a role falls
back or produces invalid geometry so that feasibility failures remain visible.
This proves neither a reference match, a browser/OS fallback cascade nor physical-pixel scale. Nothing has been
executed during the coordinating agent's quiet window.

Skipped and risk: Cargo, compilation, headless tests, native font probes, native
windows/captures and all model/network/keychain activity remain unrun. The native
bridges and variation/table transfer still need runtime verification. Static
declarations and this resolver do not establish reference font or pixel parity.
