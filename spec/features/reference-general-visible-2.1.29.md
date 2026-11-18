# Supplemental visible General observation: 2.1.29

The chosen baseline remains `cluely-macos-2.1.28`. This reviewed observation of
**2.1.29** on 2026-11-17 is separate drift evidence. It establishes no 2.1.28
runtime behavior, font match, matched visual pass or source equivalence.

The existing app was already running with an empty chat. General became readable
through its supported Undetectability control, temporarily changed from on to off.
Top and lower views were observed, then Undetectability was restored on. There was
no recording, inference, microphone test, account action or permission change.
Observed preference values describe this profile at that time, not defaults.

The [measurement record](../../ui/measurements/reference-general-visible-2.1.29-001.json)
retains the reviewed numbers, text bounds, colors, methods and restoration limits.
Reviewed handoff SHA-256:
`81952068cd4c69cf93ae52033e946d719d23a78c6bba68f3450fb39ad946d215`.
The app executable SHA-256 is
`8ca1e94c9e691b01ff9e9416b480e67278326c4d80abc4564e3541bbc1728cfa`,
ASAR `cfa941acee6d6c90f1c89faa786433a62e21a069264b34a8e57e26291860c1b0`.
The observed route was `renderer.v2.cluely.com/#/settings`; its renderer build,
source identity and model identity remain unknown or unexercised.

## Geometry and visible controls

The window was 860 × 670. Separate display APIs reported 2560 × 1440 logical and
pixel dimensions and backing scale 1 on macOS 27.2 (26B123), arm64, English,
dark theme. Screenshot dimension equality alone was not used to infer scale.
Rectangles below are window-relative `(x, y, width, height)` API coordinates,
translated from the returned window origin `(850, 321)`.

| Element | Rectangle or interval |
| --- | --- |
| Sidebar | (6, 6, 204, 658) |
| Search | (18, 50, 180, 28) |
| General navigation hit area | (18, 154, 180, 36), navigation stride 38 |
| Back / forward hit areas | (230, 6, 40, 36) / (271, 6, 40, 36) |
| Main scroll viewport | (210, 60, 650, 610) |
| Hero group | (230, 60, 599, 183) |
| Hero heading element | (492, 152, 75, 28) |
| Hero description group / range | (309, 184, 441, 35) / (338, 185, 384, 33) |
| Software section group | (230, 258, 599, 91) |
| Preferences section group | (230, 372, 599, 219) |
| Three switch bounds | (781, 419, 32, 19), (781, 483, 32, 19), (781, 547, 32, 19) |

Exact sampled RGBA runs at x=600 in the top PNG corroborate software-card y in
`[285, 349)` (64 high), preference-card y in `[399, 591)` (192 high), and
separators at y=462 and 526. These samples do not measure rounded corners or ink.

The visible order was General hero, Software Update, Preferences, Meetings,
Language, Audio, then Account and app. Preferences exposed Undetectability,
Open at login and Meeting notifications. Lower controls exposed transcription
and output language, microphone source and Test Microphone, Reset onboarding,
Log out and Quit. Their effects were not tested. Back/forward hit rectangles do
not establish navigation behavior. Offscreen AX bounds clip to the viewport and
do not establish unseen layout.

## Text bounds and color samples

All queried attributed ranges returned zero font/style runs. **Used font family,
point size, weight, baseline, tracking and computed line height remain unknown.**
These are AX text rectangles, not font-size measurements. The heading element
above and the queried text range below came from distinct AX observations; both
returned values are retained without normalization.

| Text role | AX element rectangle | AX range rectangle |
| --- | --- | --- |
| General | (492, 153, 75, 26) | (492, 153, 75, 26) |
| Software Update | (230, 259, 98, 16) | (230, 259, 97, 16) |
| Preferences | (230, 373, 70, 16) | (230, 373, 70, 16) |
| Undetectability label | (286, 412, 95, 17) | (286, 412, 95, 17) |
| Undetectability explanation | (286, 434, 253, 14) | (286, 434, 253, 14) |
| Open at login label | (286, 476, 83, 17) | (286, 476, 82, 17) |
| Open at login explanation | (286, 498, 225, 14) | (286, 498, 224, 14) |
| Meeting notifications label | (286, 541, 131, 17) | (286, 541, 131, 17) |
| Meeting notifications explanation | (286, 562, 265, 14) | (286, 562, 264, 14) |

The PNG has a 3,372-byte embedded Display ICC profile, SHA-256
`888b82c8c6dca8a09c3554fe9c039de82f7283daaef63324efae94907b1265b4`.
Pillow 12.3.0 ImageCms converted that profile to generated sRGB with perceptual
intent 0. The following are composited sample pixels from this state, not
stylesheet declarations or stable values across themes, wallpaper or displays.

| Role and sample `(x, y)` | Encoded RGB | Derived sRGB |
| --- | --- | --- |
| Content background (840, 500) | (42, 39, 39) | (42, 39, 39) |
| Sidebar (200, 500) | (47, 44, 44) | (47, 44, 44) |
| Selected navigation (190, 170) | (67, 64, 64) | (67, 64, 64) |
| Hero card (250, 100) | (50, 47, 47) | (50, 47, 47) |
| Preference card (240, 450) | (50, 47, 47) | (50, 47, 47) |
| Separator (400, 462) | (61, 58, 58) | (61, 58, 58) |
| Enabled switch track (785, 492) | (48, 162, 229) | (0, 164, 232) |

## Evidence and restoration

The three private REA observations have these public identities; no screenshots,
account data, raw commercial source or font bytes are committed:

| View | REA evidence ID | PNG SHA-256 |
| --- | --- | --- |
| General top, Undetectability off | `ev_d1334cd39f9792fbdf9155c0a75b7d5a2313e772239af1509f861093ed29611f` | `21e349e62116a825d263c5de74e9e61d5188c296800c71bafb349225657f204a` |
| General lower, off | `ev_0320b6e8b3f8971fe6e7c4a1a3a358a6a371f79db61350d86feea109e3c4a4cf` | `8545080875665488ad37dec95b5fb3c3bfbb91fddad34d39f52811545db4a443` |
| General top, on restored | `ev_3482fe980dc0f38f8aaa21363f713622f6e9ccb6b606878fd8b84e4a4ef9b993` | `79f92ae4fd8fb6066e2b5097d71ad1fb26bd7fd9d60c19a253f05e36b631aa5f` |

The top/lower AX captures had 112/113 nodes with no truncation. Undetectability
was off from 22:30:25.808Z until 22:33:10.787Z. Restoring it on returned the
selected-window PNG to the single transparent RGBA color `(0, 0, 0, 0)` and AX
value 1. This controlled off/on observation does not diagnose the earlier
transparent-only attempt or guarantee recipient-side exclusion.

After scrolling back to the top and restoring the setting, Keyboard Shortcuts
was selected again and Settings closed. The same process/launch identity,
chat input focus, Uses Screen value 1 and available Listen control were observed.
The original ephemeral upsell disappeared after off/on and was not recreated.
Closing Settings completed before its after-state returned `noWindowsAvailable`;
one fresh state read recovered chat without replaying the close action.

The private three-record REA bundle has SHA-256
`d9f917898c7b0e27f563e709440f21f739c604f26c21fa3fbf739def72ebe90e`.
It was exported, the opened binary closed and the investigation reset. This
observation adds no matched-platform pass; the inventory remains 73 features,
69 desktop and four companion, with zero matched platforms.
