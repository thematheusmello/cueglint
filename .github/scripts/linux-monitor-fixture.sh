#!/usr/bin/env bash
# Use an isolated Unix socket and two virtual sinks. No physical devices are loaded.
set -euo pipefail
fixture=$(mktemp -d)
export CUEGLINT_PULSE_FIXTURE="$fixture"
export XDG_RUNTIME_DIR="$fixture" PULSE_RUNTIME_PATH="$fixture/runtime" PULSE_STATE_PATH="$fixture/state"
export PULSE_SERVER="unix:$fixture/native" PULSE_COOKIE="$fixture/cookie"
mkdir -m 700 "$PULSE_RUNTIME_PATH" "$PULSE_STATE_PATH"
pulseaudio -n --daemonize=no --use-pid-file=no --exit-idle-time=-1 \
    --load="module-native-protocol-unix socket=$fixture/native auth-anonymous=1" \
    --load="module-null-sink sink_name=cueglint_output rate=16000 channels=1" \
    --load="module-null-sink sink_name=cueglint_decoy rate=16000 channels=1" >"$fixture/server.log" 2>&1 &
daemon=$!
trap 'kill "$daemon" 2>/dev/null || true; wait "$daemon" 2>/dev/null || true; rm -rf "$fixture"' EXIT
for ((i=0;i<100;i++)); do
    if pactl info >/dev/null 2>&1; then break; fi
    if ! kill -0 "$daemon" 2>/dev/null; then cat "$fixture/server.log"; exit 1; fi
    sleep 0.05
done
pactl set-default-sink cueglint_output
pactl set-default-source cueglint_decoy.monitor
pulseaudio --version
cargo test --offline --locked -p cueglint-local --lib \
    audio::linux_monitor::tests::isolated_monitor_captures_output_and_reports_removal \
    -- --ignored --exact --nocapture | tee "$fixture/test.log"
grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/test.log"
