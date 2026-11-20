#!/usr/bin/env bash
# Use an isolated Unix socket and two virtual sinks. No physical devices are loaded.
set -euo pipefail
fixture=$(mktemp -d)
export CUEGLINT_PULSE_FIXTURE="$fixture"
export XDG_RUNTIME_DIR="$fixture" PULSE_RUNTIME_PATH="$fixture/runtime" PULSE_STATE_PATH="$fixture/state"
export PULSE_SERVER="unix:$fixture/native" PULSE_COOKIE="$fixture/cookie"
mkdir -m 700 "$PULSE_RUNTIME_PATH" "$PULSE_STATE_PATH"
fixture_input=cueglint_decoy.monitor
extra_modules=()
case ${1:-} in
    fifo-microphone-cold|fifo-microphone-paced-silence|fifo-microphone-late-consumer)
        fixture_input=cueglint_microphone
        export CUEGLINT_FIFO_SCENARIO=${1#fifo-microphone-}
        extra_modules+=("--load=module-pipe-source source_name=$fixture_input file=$fixture/microphone.pcm format=s16le rate=16000 channels=1")
        ;;
esac
sink_parameters='rate=16000 channels=1'
if [[ ${1:-} == bounded-clock ]]; then
    # PulseAudio 17 null-sink.c sets the no-rewind maximum block to 50 ms.
    # All original-mode trials retain their default two-second maximum.
    sink_parameters+=' norewinds=true'
fi
pulseaudio -n --daemonize=no --use-pid-file=no --exit-idle-time=-1 \
    --load="module-native-protocol-unix socket=$fixture/native auth-anonymous=1" \
    --load="module-null-sink sink_name=cueglint_output $sink_parameters" \
    --load="module-null-sink sink_name=cueglint_decoy $sink_parameters" "${extra_modules[@]}" >"$fixture/server.log" 2>&1 &
daemon=$!
trap 'kill "$daemon" 2>/dev/null || true; wait "$daemon" 2>/dev/null || true; rm -rf "$fixture"' EXIT
for ((i=0;i<100;i++)); do
    if pactl info >/dev/null 2>&1; then break; fi
    if ! kill -0 "$daemon" 2>/dev/null; then cat "$fixture/server.log"; exit 1; fi
    sleep 0.05
done
pactl set-default-sink cueglint_output
pactl set-default-source "$fixture_input"
pulseaudio --version
pactl list short modules | awk -F '\t' '$2 == "module-null-sink" {print $2, $3}'

# CPAL's default ALSA capture resolves only to this virtual source, even on a machine with a microphone.
export ALSA_CONFIG_PATH="$fixture/asound.conf"
cat > "$ALSA_CONFIG_PATH" <<ALSA
pcm.!default {
    type pulse
    server "$PULSE_SERVER"
    device "$fixture_input"
}
ALSA
if [[ $fixture_input == cueglint_microphone ]]; then
    cargo test --offline --locked -p cueglint-local --test linux_paced_microphone paced_microphone_clock \
        -- --ignored --exact --nocapture | tee "$fixture/paced-mic.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/paced-mic.log"
    exit
fi
if [[ ${1:-} == native-clock ]]; then
    cc -std=c11 -Werror -x c -fsyntax-only - <<'ABI'
#include <stddef.h>
#include <pulse/def.h>
_Static_assert(sizeof(((pa_timing_info*)0)->timestamp) == 16, "unexpected timeval");
_Static_assert(offsetof(pa_timing_info, sink_usec) == 24, "unexpected sink offset");
_Static_assert(offsetof(pa_timing_info, write_index) == 56, "unexpected write offset");
_Static_assert(offsetof(pa_timing_info, read_index) == 72, "unexpected read offset");
ABI
    cargo test --offline --locked -p cueglint-local --lib audio::linux_monitor::clock_lab::first_marker_native_clocks_and_backlog \
        -- --ignored --exact --nocapture | tee "$fixture/native-clock.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/native-clock.log"
    exit
fi
if [[ ${1:-} == cold-clock || ${1:-} == bounded-clock ]]; then
    cargo test --offline --locked -p cueglint-local --test linux_dual_capture independent_virtual_microphone_and_playback \
        -- --ignored --exact --nocapture | tee "$fixture/cold.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/cold.log"
    exit
fi
if [[ ${1:-} == meeting || ${1:-} == dual-meeting ]]; then
    test "$(readlink /proc/self/ns/net)" != "$CUEGLINT_PARENT_NETNS"
    python3 - "$fixture/isolation.json" <<'NET'
import errno, json, socket, subprocess, sys
from pathlib import Path
canaries = {}
assert [row["ifname"] for row in json.loads(subprocess.check_output(["ip", "-j", "link", "show"]))] == ["lo"]
for family, flag, target in [(socket.AF_INET, "-4", ("192.0.2.1", 9)), (socket.AF_INET6, "-6", ("2001:db8::1", 9))]:
    routes = json.loads(subprocess.check_output(["ip", "-j", flag, "route", "show", "table", "all"]))
    assert all(route.get("dev") == "lo" and route.get("type", "unicast") != "unicast" for route in routes), routes
    with socket.socket(family, socket.SOCK_DGRAM) as probe:
        try:
            probe.connect(target)
        except OSError as failure:
            # IPv6 on an unconfigured loopback can fail source selection first.
            no_source = family == socket.AF_INET6 and failure.errno == errno.EADDRNOTAVAIL
            if failure.errno != errno.ENETUNREACH and not no_source:
                raise
            canaries[flag] = {"target": target[0], "errno": failure.errno, "routes": routes}
            print(f"{flag} blocked with {failure.errno}; routes={routes}")
        else:
            raise RuntimeError("unexpected routable interface inside the offline test")
Path(sys.argv[1]).write_text(json.dumps(canaries))
print("Offline namespace has only loopback and rejects IPv4 and IPv6 routes.")
NET
    test_name=local_meeting_round_trip
    runner=()
    if [[ $1 == dual-meeting ]]; then
        "$CUEGLINT_MEETING_CLI" --db "$fixture/file-control.sqlite3" --models-dir "$CUEGLINT_MODEL_DIR" \
            transcribe parity/fixtures/clean-english-v2.wav --profile whisper-base-en --backend cpu --language en \
            > "$CUEGLINT_FILE_CONTROL_REPORT"
        test_name=dual_source_meeting_round_trip
        runner=(python3 .github/scripts/linux-meeting-traffic.py "$fixture/isolation.json")
    fi
    "${runner[@]}" "$CUEGLINT_MEETING_TEST" "worker::linux_meeting_tests::$test_name" \
        --ignored --exact --nocapture | tee "$fixture/test.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed;' "$fixture/test.log"
    exit
fi
cc -std=c11 -O2 -Wall -Wextra -Werror .github/scripts/linux-clocked-tones.c \
    -o "$fixture/clocked-tones" $(pkg-config --cflags --libs libpulse-simple) -lm -pthread
dual_result=0
cargo test --offline --locked -p cueglint-local --test linux_dual_capture \
    -- --ignored --test-threads=1 --nocapture | tee "$fixture/dual.log" || dual_result=1
grep -Eq '^test result: ok\. 3 passed; 0 failed;' "$fixture/dual.log" || dual_result=1
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
exit "$dual_result"
