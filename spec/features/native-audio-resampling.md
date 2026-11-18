# Native audio resampling contract

This is an independent implementation contract for `audio.processing`, approved
against source `57d81abf65d060d3ea475c41c4bc5948a859b007`. It specifies no measured
Cluely DSP algorithm or quality threshold. Reference execution, hardware timing,
echo handling and full drift compensation remain open.

## Shared boundary

The microphone and system-audio callbacks retain their existing bounded packet
queues and source labels. Both are resampled separately by `audio::Endpoint` on
the capture consumer thread in `runtime.rs`, including the bounded final drain.
The public packet, utterance and endpoint interfaces do not change. WAV import,
model manifests, transcription configuration and persisted schemas do not change.

Input is finite mono PCM at a declared rate from 8000 to 192000 Hz, in at most
1024 samples per packet. Invalid source, length, rate, values, sequence overflow
or timestamp overflow is rejected before changing endpoint state.

## Resampling and stream boundaries

The private Rust resampler uses integer source/output positions and emits mono
16000 Hz PCM. At 16000 Hz it preserves every input sample bit, including the
first and last sample and signed zero. For other rates it uses a centered
Blackman-windowed sinc with cutoff `0.45 * min(input_rate, 16000)` Hz, 64 fractional
phases plus an endpoint row, and linear interpolation between coefficient rows.
Each row is normalized for unity DC gain. Half-width is
`ceil(32 * max(input_rate / 16000, 1))` source samples: at most 769 taps and 49985
f32 coefficients, under 200 kB per source. Accumulation uses f64; only values
outside f32's representable range saturate to that range.

The filter retains bounded history across packet boundaries, silence and normal
utterance cuts. It waits for at most 4 ms of source lookahead. Only the beginning
and final end of a continuity run use zero padding; packet boundaries do not.
The complete run emits exactly `ceil(input_samples * 16000 / input_rate)` samples.
History length stays at or below
`1024 + 2 * half_width + ceil(input_rate / 16000) + 1` samples. Setup, allocation
and filtering remain outside native callbacks and UI drawing.

The endpoint advances the filter during silence and retains the existing 0.008
RMS threshold, 10400-sample silence endpoint, two-second partial cadence, stable
utterance identities and twelve-second segment boundary. RMS is measured on the
resampled signal. Internal utterance cuts do not reset filter history. Public
stream finish flushes the remaining sample positions exactly once and clears
the stream. A sequence gap, changed rate or timestamp discrepancy over 250 ms
first flushes and finalizes the previous run, then starts an independent one.
An empty gap packet may finalize old audio but cannot invent a new utterance.

Timestamps use the first nonempty packet's captured start plus exact nominal
sample positions; final time is capped at the source interval's end. This is
sample-grid bookkeeping, not hardware clock calibration. Accepted callback
jitter, device clock drift, inter-source calibration and physical input/output
latency still require controlled hardware measurements.

## Acceptance fixed before execution

- Counts and bit-identical partitioning cover rates 8000, 11025, 16000, 22050,
  24000, 44100, 48000, 96000 and 192000, including empty, one-sample, filter and
  packet boundaries. Patterns include 1024, one-sample and 1/7/257/1024 blocks.
- Original 250/1000/3000/6000 Hz tones within `0.375 * input_rate` deviate by no
  more than 0.25 dB in passband amplitude. Original 8500/10000/12000/15000/20000 Hz
  tones below input Nyquist at 24000/44100/48000/96000/192000 are suppressed by at
  least 40 dB relative to their input RMS. The first upsampling images at 8000
  and 11025 Hz, including folding at output Nyquist, are at least 40 dB below
  their 1000/3000 Hz fundamentals.
- Spectral checks exclude only fixed filter support at each edge: 64 output
  samples for 8000 Hz; 47 for 11025; zero for 16000; 33 for 22050/44100; and 32
  for 24000/48000/96000/192000. No other part of a signal is masked.
- Tests cover bounded history/table size, short input below lookahead, exact
  final flush, empty/gap/rate/time resets, preserved partial identities and
  continuous samples across a twelve-second cut. Existing endpoint and desktop
  final-drain timestamp/identity/error assertions remain unchanged.

The source candidate has not been compiled or executed yet. Passing deterministic
DSP tests would establish only these original-fixture implementation properties;
live capture, ASR accuracy, resource/latency distributions, drift, echo,
sleep/resume, native route changes and matched reference acceptance remain open.
