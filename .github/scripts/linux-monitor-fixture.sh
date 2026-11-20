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
if [[ ${1:-} == meeting ]]; then
    test "$(readlink /proc/self/ns/net)" != "$CUEGLINT_PARENT_NETNS"
    python3 - <<'NET'
import errno, json, socket, subprocess
assert [row["ifname"] for row in json.loads(subprocess.check_output(["ip", "-j", "link", "show"]))] == ["lo"]
for family, target in [(socket.AF_INET, ("192.0.2.1", 9)), (socket.AF_INET6, ("2001:db8::1", 9))]:
    with socket.socket(family, socket.SOCK_DGRAM) as probe:
        try:
            probe.connect(target)
        except OSError as failure:
            if failure.errno != errno.ENETUNREACH:
                raise
        else:
            raise RuntimeError("unexpected routable interface inside the offline test")
print("Offline namespace has only loopback and rejects IPv4 and IPv6 routes.")
NET
    "$CUEGLINT_MEETING_TEST" worker::linux_meeting_tests::local_meeting_round_trip \
        --ignored --exact --nocapture | tee "$fixture/test.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/test.log"
    exit
fi
cargo test --offline --locked -p cueglint-local --lib \
    audio::linux_monitor::tests::isolated_monitor_captures_output_and_reports_removal \
    -- --ignored --exact --nocapture | tee "$fixture/test.log"
grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/test.log"

# Poison only this process tree's library search path. Pure CLI/desktop startup
# succeeds, then an explicit system capture must diagnose the missing library.
mkdir "$fixture/missing"
: > "$fixture/missing/libpulse.so.0"
cargo build --offline --locked -p cueglint-cli -p cueglint
for app in cueglint-cli cueglint; do
    binary="${CARGO_TARGET_DIR:-target}/debug/$app"
    if readelf -d "$binary" | grep -F 'Shared library: [libpulse.so.0]' >/dev/null; then
        echo "$app has an unconditional libpulse.so.0 dependency" >&2
        exit 1
    fi
    LD_LIBRARY_PATH="$fixture/missing" "$binary" --help > "$fixture/$app.help"
    test -s "$fixture/$app.help"
done
LD_LIBRARY_PATH="$fixture/missing" cargo test --offline --locked -p cueglint-local --lib \
    audio::linux_monitor::tests::system_capture_reports_unavailable_library \
    -- --ignored --exact --nocapture | tee "$fixture/unavailable.log"
grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/unavailable.log"
