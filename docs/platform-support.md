# Platform support

No platform has completed packaged release acceptance. Development host: macOS 27.2,
build 26B123, arm64, Apple M3 Max, 48 GB memory. Host identity details and private artifacts
are excluded from public evidence.

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Native Rust desktop | Native windows/input/persistence manually exercised | Not runtime tested | Explicit port, not runtime tested |
| Packaged clean install | Not run | Not run | Not run |
| Microphone capture | Not verified | Not verified | Not verified |
| Distinct system audio | Not verified | Loopback required, not implemented | Portal/audio route required, not implemented |
| Local ASR/generation | Real CPU development fixtures run; release acceptance open | Not verified | Not verified |
| Screen capture/OCR | Not verified | Not verified | Not verified |
| Global shortcuts/focus | Not verified | Not verified | Not verified |
| Always-on-top/click-through | Not verified separately | Not verified | Not verified |
| Capture exclusion overlay/dashboard | Untested per recipient route | Untested per recipient route | Unsupported claims must stay explicit |
| Multi-monitor/fullscreen/sleep | Not run | Not run | Not run |

Compilation alone does not establish runtime support. ScreenCaptureKit, microphone and
Accessibility each have OS-version and permission boundaries. Windows loopback is distinct
from microphone capture. No universal undetectability promise is made. Compatibility claims
need app/OS versions, share mode and recipient-side observations for both windows.
