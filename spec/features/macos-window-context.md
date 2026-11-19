# Single-window screen context on macOS

Cueglint's screen context is off by default. When the user selects **Single
window** in Settings, **Capture screen now** and a screen-enabled **Ask** each
present the macOS system picker for one window. The user chooses the content
for that invocation. Device settings and system audio retain the existing
display selection. The saved preference identifies the capture route. The
selected window and permission live only for the active capture.

A picker cancellation, failure, or deadline leaves the question unsubmitted.
A second operation while the picker, stream or cleanup owns the slot shows a
busy result. Cancel, context reset, source change and session switch invalidate
the operation's generation identity before later callbacks may act. Existing
display capture remains an independent explicit route. The window route
requires macOS 15.2 or later and the public window metadata API.

Present through the system picker on the app's main thread. Limit the picker
to one window with selection changes disabled. Before opening the stream,
require one real included window, no display selection, valid bounded
geometry, and an owner distinct from Cueglint. Use the exact picker filter.
The selection does not look up any other window or display. Fail before
inference on unsupported OS, malformed selection, cancelled or empty frames,
native delays, or lost source ownership.

Request video only. Keep a three-buffer native queue and at most one retained
complete sample. Capture after this invocation's observed selection. Cap
dimensions using the selected content geometry and point scale while
preserving aspect ratio. Confirm native stream stop before converting and
using that sample, then reuse local PNG and Vision text processing. A native
timeout that leaves stream stop uncertain keeps the capture slot occupied
and shows an error. Dropping a job alone does not establish native shutdown.

Use a stable capture UUID, the typed window identity, capture timestamp,
actual dimensions, exact pixel hash, and recognized text in the frozen
snapshot. Preserve historical display snapshot JSON and route settings
during import and readback. Keep bytes in memory and diagnostics free of raw
screen content. The process does not persist a raw capture automatically.
The existing intentional provider selection still applies to any
generation request carrying screen context.

The developer probe on macOS 27.2 selected an original AppKit test window
using a per-session grant while broad screen access preflight was false.
The one-frame screenshot method failed after selection. Video-only SCStream
delivered a complete 538×288 frame and local OCR found the target's original
marker without the decoy. The second request was sampled before the fixture
changed. The next native trial must observe the updated marker on the source
window before the user presents the corresponding fresh picker. The plan
checks the actual two capture IDs, times, pixels and frozen request contexts
against fixed, prior ground truth.

Sources

- [Apple's system-picker consent session](https://developer.apple.com/videos/play/wwdc2023/10053/)
- [Apple's ScreenCaptureKit sample](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos?language=objc)
- [Original window feasibility evidence](../../parity/reports/macos-scoped-screen-feasibility-001.json)
