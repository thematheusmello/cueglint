The Whisper frontend is pure Rust with bounded input and output. Its five normal
analytic tests cover centering, one-sided power, left/right reflection, the discarded
final STFT frame, validation and cancellation. Model accuracy checks stay separate.

To compare features with an independently retained standard-Whisper oracle, explicitly
provide a mono 16kHz PCM16 WAV, an 80x3000 row-major little-endian float32 tensor and
its independently recorded SHA-256:

```sh
CUEGLINT_TEST_FRONTEND_WAV=/path/to/input.wav \
CUEGLINT_TEST_FRONTEND_ORACLE=/path/to/oracle.f32 \
CUEGLINT_TEST_FRONTEND_ORACLE_SHA256=the_64_character_recorded_hash \
cargo test --locked -p cueglint-local --test frontend_oracle -- --ignored --nocapture
```

The maximum absolute error is fixed at 0.0005 and RMSE at 0.00001. The check verifies
the oracle hash and records source/configuration/filter/input/output identities; it
loads no models and does not change ASR thresholds. WAV evidence uses one frozen
buffer capped at 2 MiB; the reported WAV hash and decoded f32 hash bind the exact
input passed to the frontend. Optionally set
`CUEGLINT_TEST_FRONTEND_OUTPUT` to a new destination to retain computed tensor bytes,
including a failed comparison. Existing output files are never overwritten. No oracle
or workstation paths are embedded in the test.

Managed local ASR uses an explicit `TranscriptionBackend::Cpu` or `Metal` through
`Transcriber::load_profile_with_backend`. Saved application settings default to CPU, including
builds with `cueglint-local/metal` enabled. `backend_compiled` reports build support
only; actual Metal initialization happens on the capture/inference worker during model
loading, with a clear error and no CPU or hosted fallback. Feature extraction stays CPU.
`Transcriber::backend()` reports the selected Candle device. The legacy `load` method
keeps its build-selected behavior for existing development harnesses.

Managed generation and ASR construct from the same opened, hash-verified artifact handles.
The GGUF stays a file read tensor by tensor; tokenizer/config/safetensors use owned buffers
from their retained handles. All catalog artifacts, including licenses, are verified.
Ordinary verification returns metadata and closes the handles. Neither route creates a
lock file or requires a writable model root. Handles close after construction; loaded
models own their data and the existing generation/highlights cache policy stays unchanged.
The direct-directory ASR methods remain available without adding catalog/lock requirements.
This binds inputs across cooperative atomic rename/unlink; it is not a reader lease or
protection against uncooperative in-place writes. See the [input contract](../../spec/features/verified-model-inputs.md).
CLI `transcribe` reports the profile returned by the managed loader; its `elapsed_ms`
includes verification, loading and decoding. Earlier timings keep their recorded scope.

The CLI can override a saved choice for one file or persist it deliberately:

```sh
cargo run -p cueglint-cli -- transcribe /path/to/input.wav --backend cpu
cargo run -p cueglint-cli --features cueglint-local/metal -- transcribe /path/to/input.wav --backend metal
cargo run -p cueglint-cli -- settings set-transcription-backend cpu
```

The native Local models settings use the same saved choice. A CPU-only build preserves
a restored Metal choice, displays its unavailable state and allows selecting CPU. File
verification and compile support do not prove device availability: `model-status` returns
`transcription_available` null for a verified, compiled Metal profile because the command
does not load a device, and reports `device_availability` as `not_tested`. CPU readiness
keeps its existing verified-file meaning. Generation remains CPU and does not share the
ASR backend choice.

The retained [Metal file evaluation](../../../parity/reports/metal-asr-actual-002.json)
belongs to its exact earlier source and development worker: all 28 transcripts matched
CPU, the first base-model warmup took 10.58 seconds, and no live or packaged capture
performance is established by that report. A new backend configuration needs new evidence.
