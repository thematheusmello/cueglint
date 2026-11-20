# Platform support

No platform has completed packaged release acceptance. Development host: macOS 27.2,
build 26B123, arm64, Apple M3 Max, 48 GB memory. Host identity details and private artifacts
are excluded from public evidence.

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Native Rust desktop | Native windows/input/persistence manually exercised | Hosted dashboard fixture rendered with Direct3D 12 software; interactive Windows desktop checks open | Hosted Ubuntu X11 dashboard ran with software Vulkan |
| Packaged clean install | Not run | Not run | Not run |
| Microphone capture | Not verified | WASAPI compiled/helpers passed in PR18; real devices unverified | Not verified |
| Distinct system audio | Not verified | WASAPI loopback compiled/helpers passed in PR18; real devices unverified | PulseAudio 17.0 on Ubuntu 26.04.1 passed virtual playback capture, normal stop and source removal |
| Capture input selectors | Implemented; headless UI/Store checks passed | Stable microphone ID path implemented; new picker checks pending | Existing CPAL microphone path; runtime unverified |
| Capture display selection | Shared OCR/system-audio ID; actual capture pending | Display/OCR unavailable; loopback is display-independent | Display/OCR unavailable, default playback monitor is display-independent |
| Local ASR/generation | Real CPU development fixtures run; release acceptance open | Not verified | Not verified |
| Explicit hosted/compatible answers | Controlled native-headless/CLI HTTP tests; live accounts unverified | Runtime not verified | Keyless compatible route; runtime not verified |
| API-key persistence | Real CLI Keychain save/load/delete with synthetic key passed | Hosted native vault round trip and cleanup with a synthetic key passed | Unavailable; no plaintext storage |
| Screen capture/OCR | Signed macOS 27.2 single-window OCR and changed-frame answers verified on original fixture. Display workflow pending | Not verified | Not verified |
| Global shortcuts/focus | Ordinary-key native path; runtime not verified | Native RegisterHotKey path; runtime not verified | Global unavailable; focused fallback only |
| Always-on-top/click-through | Not verified separately | Not verified | Not verified |
| Capture exclusion overlay/dashboard | Untested per recipient route | Untested per recipient route | Unsupported claims must stay explicit |
| Multi-monitor/fullscreen/sleep | Not run | Not run | Not run |

Compilation alone does not establish runtime support. ScreenCaptureKit, microphone and
Accessibility each have OS-version and permission boundaries. Windows loopback is distinct
from microphone capture. No universal undetectability promise is made. Compatibility claims
need app/OS versions, share mode and recipient-side observations for both windows.

The recorded shortcut defaults come from the macOS reference only. Mapping the primary
modifier to Control on Windows is an implementation decision pending its separate baseline.
Keyboard layouts, held-key timing, sleep/resume, application conflicts and registration
failure recovery need real platform checks. A registration status is not a focus/parity pass.

## Linux playback capture

Cueglint opens the session's default playback monitor on a local PulseAudio UNIX
socket. Cueglint loads the OS library `libpulse.so.0` when playback capture
starts (Debian and Ubuntu package `libpulse0`). A PulseAudio or pipewire-pulse
session provides the monitor. The microphone uses its existing ALSA/CPAL route.
Cueglint pins the recording to the monitor selected at startup, requests mono
16 kHz PCM, and reports a device error when the server removes the stream.
Restart capture to resolve the new default output. The Linux CI fixture plays
an original signal into one virtual sink while a separate silent virtual sink
serves as the default input. Ubuntu recovered 159 rising crossings in 3,200 frames
at RMS 0.250, retained the final marker within tolerance and reported the
removed monitor. [Linux playback verification](../parity/reports/linux-playback-monitor-001.json)

PipeWire compatibility, physical devices, combined
microphone capture and desktop-session latency require independent runs.

## Linux native dashboard

The authored dashboard ran from the locked x86-64 Rust checkout on Ubuntu
24.04.5 LTS. The executable rendered at scale 1 on private Xvfb and the Lavapipe
CPU Vulkan adapter. The saved 860 × 670 frame and GPU readback trace agree with
the recorded DejaVu Sans identity. Device, Wayland, interaction and package
checks remain open.
[Linux runtime evidence](../parity/reports/linux-native-dashboard-actual-001.json) ·
[Actual dashboard PNG](images/linux-native-dashboard-001.png)

## Windows build host qualification

The manual host check built both AMD64 executables from frozen source
`e1f6556` on Windows Server 2025. All 397 graph entries matched. The corrected
run inspected both executable import tables and ran CLI help in fresh empty
directories. The report retains the earlier help-name assertion failure and
both hosted log fingerprints. [Windows qualification report](../parity/reports/windows-host-qualification-001.json).

A separate native Windows run verified the installed Rust 1.94.1 compiler,
standard library and channel manifests, plus the active MSVC and Windows SDK.
All 397 entries matched the frozen source graph, and the run stopped before
compilation. [Hosted provenance evidence](../parity/reports/windows-host-provenance-002.json)

The [Windows notice inventory](../third-party/runtime/windows-x86_64-msvc/README.md)
binds the dependency inputs, Ring x64 source notices, full shared HarfRust and GPU
descriptor publisher grants, and the distinct Windows WGL registry header.
[Publisher notice evidence](../parity/reports/runtime-publisher-grants-001.json)

The pinned package candidate `5a468a2` matched all 397 native graph rows and
the expected host tool provenance. Both release executables built, CLI help
ran in isolation, and the original dashboard rendered at 860 × 670 on the
Windows Server 2025 Direct3D 12 software adapter. The module sample observed
11 paths and verified four full file identities. VC runtime and CRT paths
resolved to System32, with their version/hash checks outside the sample
budget. Package adoption, clean installation and the broader native route
still require separate evidence.
[Windows candidate evidence](../parity/reports/windows-package-candidate-actual-001.json)

The later probe of the same pinned candidate verified both System32 runtime
file identities and recorded two generated WGL bindings, the selected MSVC linker
and the Cargo-declared Ring and SQLite archives. Eight of 11 sampled DLL paths
have full disk identities. Final PE member selection, SDK terms applicability,
and the clean prerequisite route remain open.
[Supplemental Windows evidence](../parity/reports/windows-package-candidate-provenance-002.json)

Both executables import `VCRUNTIME140.dll`. A clean install with the
separately installed compatible x64 Visual C++ runtime remains to be tested.
Windows runtime and toolchain notice coverage, device capture and distribution
packaging need their own evidence. [Microsoft runtime prerequisite](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170).

The hosted GUI check ran the frozen release executable on Windows Server 2025.
It produced and decoded an 860 × 670 original dashboard image at 1.0 scale
using the Microsoft Basic Render Driver over Direct3D 12. The root readback
trace matched and the app exited normally. Physical devices, interactive Windows
desktop behavior and clean installation remain open.
[Windows GUI evidence](../parity/reports/windows-release-gui-001.json)

### Windows x64 portable package

On native Windows x64 MSVC, `cargo xtask package` selects the Windows notice
inventory. It validates the shared source identity, the build inputs and the
Windows-specific readiness decision before building the two executables. The
tracked inventory is blocked while the remaining notice and runtime checks
are open.

After adoption, the command prepares `dist/cueglint-windows-x86_64-msvc/`
with `cueglint.exe`, `cueglint-cli.exe`, `cueglint.NOTICES/`,
`cueglint-cli.NOTICES/` and `SHA256SUMS`. It stages the checked files beside
the destination and publishes with a no-replacement Windows directory move.
The existing Visual C++ runtime prerequisite applies. ZIP distribution,
signing and a clean Windows install still require verification.
[Package tests](../parity/reports/windows-portable-package-001.json) ·
[Windows directory move contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefilew)

## Windows native audio sources

The Windows implementation uses the pinned `windows 0.62.2` bindings and documented
desktop WASAPI. [PR18 hosted evidence](../parity/reports/pr18-hosted-ci-artifact-001.json)
at `bcf6db2` compiled the native Windows workspace and passed four Windows-only
helpers plus thirteen portable bridge cases without opening audio devices. No
Windows release, real capture, dual-source readiness or reference parity follows.

For `SystemAudio`, `GetDefaultAudioEndpoint(eRender, eConsole)` selects the default
console playback endpoint at capture start. That endpoint stays pinned until stop. There
is no microphone fallback, per-application filter, explicit render selector or automatic
route switching. A default-device change can leave capture on the old endpoint while it
remains valid; stop and resume to select the new default. The mix may include Cueglint's
own playback and audio from other applications or Windows sessions. Protected content is
subject to the native driver/DRM restrictions. These are endpoint loopback semantics,
not a requirement to enable a separate hardware recording device.
([Endpoint selection](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint),
[loopback recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording).)

For `Microphone`, an absent saved selection uses `eCapture/eConsole`. A saved selection
is parsed through CPAL 0.18.2's public `DeviceId` API: require the WASAPI host, preserve
its opaque `id()` payload, convert to UTF-16 and resolve it with `GetDevice`. Empty,
embedded-NUL and IDs over 16,384 UTF-16 units are rejected before Win32. That size is an
implementation limit, not a Windows endpoint-format maximum. The resolved endpoint must
be active, have capture direction and return exactly the selected opaque ID. A missing,
removed or invalid selection fails visibly; no name matching, fallback or settings rewrite
occurs. A capture endpoint may be a physical microphone, line input or virtual device;
the source label identifies the configured input route. Microphone stream flags are zero,
so this route never enables render loopback.
Settings and the typed capture configuration both enforce 4,096 identifier bytes.
The underlying bridge additionally bounds native ID copies at 16,384 UTF-16 units;
that wider native guard does not expand the public Settings/capture limit.
([Endpoint IDs](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid),
[device lookup](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdevice).)

The microphone owner checks active state every 250 ms between native packets. For a
default microphone it also checks the current console-default ID and stops on a changed
or missing endpoint. Explicit selection ignores unrelated default changes. Resume resolves
the default or the same saved ID anew; there is no automatic rebind. This polling interval
is a declared scheduling choice, not a measured or hard detection bound. Native activation,
privacy denial/revocation, default changes and unplug behavior need an actual Windows host.

The worker activates `IAudioClient`, reads `GetMixFormat`, and initializes shared mode
with `AUDCLNT_SHAREMODE_SHARED`; only `SystemAudio` adds `AUDCLNT_STREAMFLAGS_LOOPBACK`.
It accepts IEEE float32 or signed PCM16/24/32, including extensible PCM with valid precision
no wider than its container, at 8–192 kHz and 1–32 channels. Channel masks, precision, block alignment and
byte rate are checked. Other layouts fail visibly. Channels are averaged into mono;
the packet keeps the native sample rate for the existing endpoint/resampler. The requested
native buffer is 100 ms; the returned capacity must be positive and no more than two
seconds. Each native packet must fit that capacity before its bytes are borrowed.
Returned mix-format and endpoint-ID allocations are freed on their owner thread.
([Mix format and ownership](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getmixformat),
[initialization](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize).)

`IAudioCaptureClient::GetBuffer`/`ReleaseBuffer` stay on that owner thread. Silent buffers
produce zeros without reading their pointer; null pointers, inconsistent sizes and
nonfinite samples fail. Native QPC positions in 100 ns units map to the shared capture
origin using a clock calibration bracket no wider than 1 ms and a 1 ms future-time allowance. An
uncertain timestamp, later discontinuity, device-position gap or backward clock stops the
source with an error. The first discontinuity flag is ignored because no previous packet
exists. This deliberately conservative recovery policy needs real silence/resume and
route-change checks; no timestamp accuracy or drift bound has been measured.
([Buffer contract](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer),
[buffer flags](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags).)

One shared worker implementation has an owned instance per enabled source, at most two
capture threads. Each has a bounded startup acknowledgement, independent sequence/clock
state and the same capture origin. They use the existing shared 128-packet queue. Each
packet holds at most 1,024 mono samples; queue sends never block and dropped packets retain
a sequence gap. Cancellation is checked
between native packets and output blocks. Stop closes the shared mute gate, signals and
unparks the worker, joins its handle once, and reports native read/release/stop failures.
Both-source stopping and the capped final queued-audio drain remain in place. Resource
guards also attempt release and stop during panic unwinding; those panic-path cleanup
attempts are best effort. OS/driver calls and COM release cannot be interrupted here, so
startup acknowledgement and joining have no hard timeout guarantee. No thread is detached.

The chosen MMDevice/WASAPI interfaces have a documented Vista API minimum; discontinuity
flags are documented for Windows 7 onward. This is not Cueglint's supported Windows
version declaration. The worker uses its own COM STA, respecting Microsoft's Windows 8
first-use caveat. It polls at 10 ms instead of using event-driven loopback, which Microsoft
documents as directly supported from Windows 10 version 1703. Sustained power, scheduler
latency and actual teardown timing remain unmeasured.
([IAudioClient requirements](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iaudioclient),
[COM initialization](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex),
[loopback event boundary](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording).)

Windows microphone bytes now use this original native bridge. CPAL 0.18.2's installed
reader discards `SILENT`/`TIMESTAMP_ERROR` before its application callback, so a downstream
callback change cannot repair that boundary. CPAL still supplies device enumeration,
public ID parsing and non-Windows streams; there is no dependency fork/version change.
PR18 supplies compilation and synthetic execution evidence for this bridge; it does
not establish real microphone capture or dual-source readiness.
Synthetic tests cover selectors/UTF-16, source identity, conversion, layouts, silence,
null/size bounds, queue overflow, cancellation gates, clocks, cleanup and join ownership.
The recorded Windows-only CPAL/schema/join cases and thirteen portable bridge tests
passed in PR18. Real-device packet flags, driver formats and callback teardown
remain unverified. Synthetic verified-input replacement/removal also passed on
that Windows host; native model-file UI and real inference remain separate.

Later sources need their own appropriate locked workspace, formatting, registry and
generated-progress checks. Before a device/runtime claim, freeze source/binary/OS,
driver, render endpoint, mix format, original fixture and configuration identities. On
real Windows, test system-only playback/silence/resume, queue pressure and immediate stop;
then device removal/default-route changes, service failure, sleep/resume, power/latency
and last-utterance persistence. Independently verify the microphone flag/selection boundary
before testing overlap, echo, drift and dual-source transcription. A separate Windows
reference baseline and clean-install acceptance remain open.

On macOS, the [native provider attempt](../parity/reports/native-provider-ui-002-failure.json)
at `22a6590` failed before URL/model/key input and sent no HTTP request or Keychain
mutation. It establishes one-launch custom UI-persistence isolation and normal
quit, not a provider/credential interaction pass. Later popup fixes have no native
acceptance claim in this receipt.

The later [native provider check](../parity/reports/native-provider-ui-004.json)
at `136a7c7` exercises macOS Keychain save/delete, synthetic compatible-server
listing/testing, intentional provider selection, one floating-assistant response
and normal quit. The custom UI sidecar exists and all checked default data
endpoints remain unchanged. Input used pointer focus followed by keyboard typing;
the AX Click/text-focus deficiency and earlier failed attempts remain visible.
This is neither live vendor/model evidence nor capture, clean-install, Windows or
Linux runtime acceptance.

The [input-selection increment](../spec/features/native-capture-input-selection.md)
adds explicit microphone selection and, on macOS, one display choice shared by
OCR and system audio. Default uses the OS main display; unavailable explicit IDs
remain saved and fail instead of choosing another target. Input enumeration runs
only after an explicit refresh, outside drawing/callbacks; cancellation waits for
an already-running OS API to return before releasing its sole refresh slot.
The public display-list entry keeps its permission-requesting behavior, while UI
refresh uses the existing-grant entry without an explicit grant request. This is
not a guarantee against OS dialogs during revocation races.

At source `1fa113e`, [targeted core/local/desktop tests](../parity/reports/capture-inputs-targeted-workspace-001.json)
passed 250 checks with four ignores. The integrated source `e4a46779` separately
[passed 312 workspace checks](../parity/reports/capture-inputs-integrated-workspace-001.json),
again with four ignores and zero failures/filtering. Only their named `crates`
subtrees are byte-equal. These macOS implementation runs do not establish the new
picker on Windows, actual microphone/system capture, corrected screen freshness,
permission recovery, a packaged offline session or a matched platform pass.
The separate native local audit and permission handoff remain outside these reports.

## Non-Windows microphone PCM frontend

[Source-bound PCM checks](../parity/reports/microphone-pcm-runtime-001.json) cover
CPAL 0.18.2's twelve integer/floating PCM frontend types through the actual typed
dispatch and packet-writer seam. DSD/future formats fail before the builder;
selected devices and their default configs are unchanged. Existing ordered
float32 averaging is bit-identical. Complete frames, first/last values, bounded
queue/mute/stop behavior and checked metadata pass original synthetic controls.
Malformed interleaved frames are reported; nonfinite values reach the existing
Endpoint rejection boundary without zero-fill. No callback I/O/inference/allocation
is added. Windows retains its separate WASAPI bridge.

These are frontend/code claims, not physical format support: macOS's installed
CPAL default path recognizes F32/I16, and ALSA exposes its own subset. Ten PCM
checks are included in45 passing audio tests; five desktop caller checks also
pass. A 376-test staged workspace passes on the development Mac with four ignores.
Real drivers, permissions, callback timing, route recovery, drift/echo, ASR quality
and Windows/Linux native runtime still require separate controlled evidence.


The frozen Calendar candidate built two AMD64 executables on the Windows 2025
hosted image. The CLI help and the 860 × 670 dashboard rendered through the
Direct3D 12 software adapter both exited successfully. The desktop linker map
was rejected. The collector retained no map hash or parser detail and ended
before evaluating the CLI map. The next probe records each outcome independently.
[Windows runtime and map refusal](../parity/reports/windows-current-product-map-refusal-002.json)

The follow-up Windows run identified the desktop map size as the refusal reason.
It measured 33,654,718 bytes, exceeding the former 32 MiB limit by 100,286.
The CLI map passed identity checks and contained 13,116 public-symbol rows,
including 257 with unclassified labels. CLI help and the native dashboard
ran successfully. [Per-binary Windows refusal](../parity/reports/windows-current-product-map-refusal-003.json)

The 64 MiB reader then validated both linker maps for frozen product
`5e6ab95`. The desktop map contained 20,565 public-symbol rows, including
444 unclassified rows. The CLI map contained 13,116, including 257
unclassified rows. The AMD64 CLI printed help and the desktop rendered the
same 860 × 670 dashboard pixels as the prior run. These counts describe the
source-label spellings retained by the linker. Archive-member origins,
SDK and runtime redistribution applicability, notice adoption, signing and
clean installation remain open. [Verified Windows qualification](../parity/reports/windows-frozen-product-link-maps-001.json)
