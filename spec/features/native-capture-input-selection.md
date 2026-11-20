# Native capture input selection

This is an original Cueglint behavior contract, not a reference observation or a
hardware/parity verification result.

General offers an explicit refresh of microphone inputs and capture displays.
Enumeration runs outside drawing/audio callbacks, through one owned background
job. Its request identity rejects stale results. Cancellation preserves the last
catalog and saved selection; a native API already in progress may finish before
the job releases its slot. A cancelled or stuck native call cannot create an
unbounded succession of refresh threads. Refresh never starts recording, model
work, a download, a provider request or credential work. UI display enumeration checks existing Screen Recording permission without an
explicit permission request. OS dialogs during permission-revocation races are
not ruled out. The existing requesting display-list API keeps its semantics.

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
existing CPAL backend. Linux system audio uses its independent playback monitor. Saved
unsupported selections are retained and identified in the UI, not rewritten.
Legacy malformed selections remain readable. An explicit Use default capture
inputs action clears both saved IDs without changing notes or migrating data.

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


## Linux selected display

In an authorized local X11 session, General lists up to 32 active RandR 1.5 monitors.
An explicit ID incorporates the X screen, monitor name and available connector and
EDID data. An identity change requires selecting the display again. The default
uses the sole monitor or the unique primary. An ambiguous or absent ID fails.
The worker checks identity and geometry again after reading the frame. Wayland
sessions require a separate portal implementation. A restored macOS single-window
preference stays selected until the user explicitly chooses Display in General.
That choice preserves the saved monitor ID and does not start capture.

Frames are limited to 2,560 pixels on either side and 4,194,304 pixels total.
Oversized frames use XRender to scale the selected region before transfer.
Visible application windows on that monitor can be included. Each saved
request freezes the display ID, capture UUID, time, dimensions, PNG hash and
local English OCR text. The PNG stays in memory. Standalone capture prints
the recognized text and metadata to the invoking terminal.
