# Explicit local transcription processor

This is an original Cueglint behavior contract, not a reference observation or parity
claim. Local transcription offers CPU and an explicitly compiled Metal backend using
the existing pinned Candle engine. CPU is the saved default on every build. Choosing a
transcription processor does not change the answer provider, generation device, model
profiles or installed files, and performs no inference or network request.

The native Local models settings and CLI share a typed saved selection. The CLI also
accepts a one-file override. Missing legacy settings select CPU. Backups preserve the
choice without changing the schema version, notes or session data; unknown backend
values are rejected before replacement. Restoring Metal on a CPU-only build preserves
that choice and exposes its unavailable state, with an explicit CPU selection available.

Build support is a pure compile-time check. Readiness and drawing never initialize a
device. A compiled Metal backend may still fail when a worker initializes its device;
the error is visible and never falls back to CPU or a service. Model-file verification
does not establish Metal device availability. `model-status` distinguishes the selected
processor, build support and untested device availability, returning null availability
for a verified, compiled Metal selection.

Audio and model settings cannot be saved while capture is initializing, listening or
finalizing. Each capture worker owns the settings frozen at its start. Existing bounded
queues, session identities, shutdown/final transcript draining and cancellation remain.
Backend changes do not cancel answer/highlight jobs or reset their CPU model cache.
Persisted audio provenance records the actual device label, model hashes and frontend
revision/configuration, independently of later settings changes. Existing unknown
legacy audio provenance stays unknown.

`Transcriber::load_profile_with_backend` is the managed application API. It returns the
profile that supplied the verified inputs alongside the transcriber. Native transcript
events and CLI results retain that profile's metadata rather than resolving its ID again.
The direct-directory `load_with_backend` method remains explicit; legacy `load` remains
build-selected for compatibility with retained development harnesses. Both direct methods
keep their unmanaged behavior without requiring a catalog root or coordination file.
No new inference engine or runtime dependency is introduced. Source tests are separate
from actual CPU/Metal quality, startup, live-capture and packaged-app verification.
