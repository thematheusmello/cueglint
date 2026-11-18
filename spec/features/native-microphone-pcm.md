# Native microphone PCM conversion

This original implementation contract extends `audio.microphone`. It follows the
reviewed PCM proposal `b60d15d15d03fbc89e738e64509c9a4e45fe52bd8285af190b7cf5552c940c8b`
against source `57d81abf65d060d3ea475c41c4bc5948a859b007`. It specifies no measured
Cluely audio algorithm and establishes no physical device or driver acceptance.

## Selection and source ownership

The macOS/Linux CPAL microphone path retains the saved-device resolution or the
explicit default-device choice and its default stream configuration. It does not
negotiate another format, retry another device or substitute system audio.
Permission checks, native stream play/stop ownership, the shared stop/mute gate,
bounded queue and `Microphone` source label remain with their existing callers.
Windows WASAPI and ScreenCaptureKit system audio do not change. The resampler,
WAV boundary, settings, model defaults and persisted schemas do not change.

The selected configuration must have 1–32 channels and a nominal rate of
8000–192000 Hz before stream construction. A typed match covers CPAL's actual
12 PCM frontend types: i8/i16/I24/i32/i64/u8/u16/U24/u32/u64/f32/f64. DsdU8,
DsdU16, DsdU32 and future unknown formats fail before entering the stream builder.
Typed frontend support does not establish that a backend or device offers it.

The inspected CPAL 0.18.2 macOS default-configuration implementation recognizes
F32 and I16. Its supported-configuration enumeration has a separate F32-only
limitation. The installed ALSA mapping has ten PCM variants, omitting I64/U64.
No attempt is made to override those backend restrictions.

## Conversion and callbacks

The private packet writer is the actual callback path and the unit-test seam.
It uses CPAL's existing `Sample`/`FromSample` conversions, then the previous
ordered f32 sum divided by channel count for each interleaved frame. Float32
conversion is identity, preserving the prior mixing bits. Signed integers use
their signed full-scale denominator; unsigned samples are centered before
normalization. IEEE f32 rounding can make 32/64-bit positive maxima equal +1.
I24 and U24 use CPAL's typed four-byte containers, never a packed three-byte cast.
Float64 is converted per sample before the same channel mix.

NaN, infinity and float64-to-f32 overflow are not replaced with zero or silently
clipped. They reach the existing shared endpoint validation and visible capture
failure path. A malformed incomplete interleaved frame instead increments the
existing device-error counter and publishes none of that malformed callback.
Packet sequence and timestamp overflow also increment that counter and retain
any earlier successfully queued packets.

The callback uses borrowed slices, one stack `AudioPacket`, scalar conversion,
atomics and bounded `try_send`. Closure preparation, configuration and Arc clones
occur before callbacks. No authored callback allocation, filesystem/network I/O,
logging, waiting, model inference or UI drawing is added. Each packet has at most
1024 mono samples and at most 32 channel conversions per frame; total callback
work stays linear in the OS-provided input slice. Actual callback wall time and
native buffer sizes still require hardware measurement.

Empty input publishes nothing. Complete partial packets retain their first and
last frames. Queue-full/disconnected delivery returns and keeps the existing
drop-counter semantics. The stop/mute gate publishes no new packet when observed
and retains the previous sequence advancement while muted. In-flight native
callback shutdown and scheduling are not established by deterministic tests.

## Acceptance fixed before execution

- The production typed dispatch selects all twelve PCM types with the exact
  `SizedSample::FORMAT` and storage size. All three DSD variants fail without
  entering the builder body; the future-format wildcard remains fail-closed.
- Every signed/unsigned width covers minimum, adjacent-to-zero/midpoint,
  quarter-scale and maximum normalization with exact f32 rounding. Float32
  channel mixing matches the previous callback bit for bit; float64 covers its
  declared cast and representability boundaries.
- Original frames cover 1/2/32 channels and 1/1023/1024/1025/2065 frames for every
  PCM type. Single and 1/7/257/1024-frame callback partitions produce identical
  flattened PCM, ordered packet sequences and timestamps within one source
  sample of the fixture clock. Unused packet slots remain zero.
- Queued nonfinite samples are rejected by the shared endpoint without changing
  its pending utterance. Full/disconnected queues return with the expected drop
  count. Mute/stop gates and resumed sequence gaps remain explicit.
- Invalid configuration, incomplete frames and sequence/timestamp overflow
  publish no malformed block and preserve earlier queued content. Existing
  resampler, endpoint and desktop final-drain assertions remain unchanged.

## Pinned source inspection

The selected installed source bytes match the Cargo.lock-checksummed archives:

- CPAL 0.18.2 archive SHA256
  `6f02e8d0327b42d3e2e4ab2119af397344eb9fc54a34bf0ddeaa1277af8681f1`,
  VCS `e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7`: sample formats, typed build
  signature and macOS/ALSA backend mappings.
- dasp_sample 0.11.0 archive SHA256
  `0c87e182de0887fd5361989c677c4e8f5000cd9491d6d563161a8f3a5519fc7f`,
  VCS `97c3bb9b2363c0b46ac1633858bf1054fd02a980`: scalar conversions, identity
  conversion and checked I24/U24 constructors. No dependency is added.

This source candidate has not been compiled or run. Synthetic implementation
checks cannot establish native CPAL scheduling, permissions, driver/format
support, device routing, echo, clock calibration, ASR quality or reference parity.
