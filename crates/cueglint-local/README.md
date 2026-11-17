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
