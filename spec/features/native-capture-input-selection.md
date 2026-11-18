# Native capture input selection

This is an original Cueglint behavior contract, not a reference observation or a
hardware/parity verification result.

General offers an explicit refresh of microphone inputs and capture displays.
Enumeration runs outside drawing/audio callbacks, through one owned background
job. Its request identity rejects stale results. Cancellation preserves the last
catalog and saved selection; a native API already in progress may finish before
the job releases its slot. A cancelled or stuck native call cannot create an
unbounded succession of refresh threads. Refresh never starts recording, model
work, a download, a provider request or credential work. Display enumeration
checks existing Screen Recording permission without requesting a new grant.

The microphone picker stores CPAL's serialized stable device ID, with an explicit
OS-default choice represented by null. macOS/Linux capture resolves that exact
ID through the existing CPAL host. Windows passes that serialized WASAPI ID to
the existing native bridge, which resolves and checks the opaque endpoint ID,
capture direction and active state. An unavailable explicit ID never falls back
to another input. Device names are labels, not identifiers or speaker identities.

On macOS, one capture-display selection applies to both screen OCR and
ScreenCaptureKit system audio. Null means the OS main display at the next capture;
an explicit missing display fails and remains saved. Changing the selection
invalidates pending screen work and its preview. Windows display/OCR selection is
unavailable in the current implementation; WASAPI system audio uses the default
render endpoint independently of displays. Linux microphone selection uses its
existing CPAL backend; screen OCR and system audio remain unavailable. Saved
unsupported selections are retained and identified in the UI, not rewritten.

Catalogs are bounded to 128 microphone inputs and 32 displays. Invalid or oversized
IDs/catalogs fail visibly, without silently choosing or truncating a selection.
Refresh errors retain the previous catalog and identify it as stale. A saved ID
missing from a successful refresh is still displayed with reconnect/refresh or
explicit-default recovery. Updating input settings is refused while capture is
initializing, listening or finalizing. Existing notes and session data are kept.

Headless tests cover settings validation/preservation, refresh ownership and
native UI command flow into stored capture configuration. They do not open
devices or establish permissions, driver behavior, audio quality, route recovery,
reference matching or packaged release acceptance. No audio injection is added.
