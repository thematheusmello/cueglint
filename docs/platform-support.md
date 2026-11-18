# Platform support

No platform has completed packaged release acceptance. Development host: macOS 27.2,
build 26B123, arm64, Apple M3 Max, 48 GB memory. Host identity details and private artifacts
are excluded from public evidence.

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Native Rust desktop | Native windows/input/persistence manually exercised | Not runtime tested | Explicit port, not runtime tested |
| Packaged clean install | Not run | Not run | Not run |
| Microphone capture | Not verified | WASAPI compiled/helpers passed in PR18; real devices unverified | Not verified |
| Distinct system audio | Not verified | WASAPI loopback compiled/helpers passed in PR18; real devices unverified | Portal/audio route required, not implemented |
| Local ASR/generation | Real CPU development fixtures run; release acceptance open | Not verified | Not verified |
| Explicit hosted/compatible answers | Controlled native-headless/CLI HTTP tests; live accounts unverified | Runtime not verified | Keyless compatible route; runtime not verified |
| API-key persistence | Real CLI Keychain save/load/delete with synthetic key passed | Native credential backend implemented; runtime unverified | Unavailable; no plaintext fallback |
| Screen capture/OCR | Not verified | Not verified | Not verified |
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
The existing Settings limit of 4,096 identifier bytes is unchanged; direct capture calls
and native ID copies also enforce the wider UTF-16 boundary.
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
