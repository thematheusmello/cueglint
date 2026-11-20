#!/usr/bin/env bash
# One manual hosted run. Fetch the two pinned model profiles before entering the offline namespace.
set -euo pipefail
case ${1:-live-meeting} in
    live-meeting) mode=meeting ;;
    dual-source-meeting) mode=dual-meeting ;;
    *) exit 2 ;;
esac
out="$RUNNER_TEMP/cueglint-live-meeting-evidence"
mkdir -m 700 "$out"
work=$(mktemp -d "$RUNNER_TEMP/cueglint-live-meeting.XXXXXXXX")
phase=preflight
finish() {
    local code=$?
    printf '{"stage":"%s","exit_code":%d}\n' "$phase" "$code" > "$out/run-status.json"
    if [[ -n ${GITHUB_OUTPUT:-} ]]; then
        if [[ -s "$out/meeting.json" ]]; then
            echo 'transcript=available' >> "$GITHUB_OUTPUT"
        elif [[ -s "$out/before-stop.json" ]]; then
            echo 'before_stop=available' >> "$GITHUB_OUTPUT"
        fi
    fi
    rm -rf -- "$work"
}
trap finish EXIT
export CUEGLINT_DATA_DIR="$work/data" CUEGLINT_MODEL_DIR="$work/models"
export CARGO_BUILD_JOBS=2
mkdir -m 700 "$CUEGLINT_DATA_DIR" "$CUEGLINT_MODEL_DIR"
git rev-parse HEAD > "$out/source.txt"
test "$GITHUB_SHA" = "$(cat "$out/source.txt")"
fixtures=(clean-english-v2)
if [[ $mode == dual-meeting ]]; then fixtures+=(original-microphone-v1); fi
python3 - "${fixtures[@]}" <<'PY' | sha256sum -c -
import json, sys
from pathlib import Path
for name in sys.argv[1:]:
    fixture=json.loads(Path(f'parity/fixtures/{name}.json').read_bytes())
    print(fixture['sha256']+'  '+fixture['audio'])
PY
cp parity/fixtures/clean-english-v2.json "$out/fixture.json"
if [[ $mode == dual-meeting ]]; then
    cp parity/fixtures/original-microphone-v1.json "$out/microphone-fixture.json"
fi
uname -srmo > "$out/kernel.txt"
lscpu > "$out/cpu.txt"
rustc -Vv > "$out/rustc.txt"
pulseaudio --version > "$out/pulse.txt"
phase=build-cli
cargo build --locked -p cueglint-cli
cli="${CARGO_TARGET_DIR:-target}/debug/cueglint-cli"
sha256sum "$cli" | cut -d' ' -f1 > "$out/cli.sha256"
for profile in whisper-base-en qwen2.5-1.5b-instruct; do
    phase="install-$profile"
    "$cli" --models-dir "$CUEGLINT_MODEL_DIR" models install "$profile" > "$work/$profile-install.json"
    "$cli" --models-dir "$CUEGLINT_MODEL_DIR" models verify "$profile" > "$work/$profile-verified.json"
done
phase=build-worker-test
cargo test --locked -p cueglint --bin cueglint --no-run --message-format=json > "$work/build.jsonl"
test_binary=$(python3 - "$work/build.jsonl" <<'PY'
import json,sys
with open(sys.argv[1]) as f:
    rows=[json.loads(line) for line in f]
(binary,)=[r['executable'] for r in rows if r.get('reason')=='compiler-artifact'
           and r['target']['name']=='cueglint' and r['target']['kind']==['bin']
           and r['profile']['test'] and r.get('executable')]
print(binary)
PY
)
sha256sum "$test_binary" | cut -d' ' -f1 > "$out/test-binary.sha256"
ip -4 route get 192.0.2.1 >/dev/null
parent_ns=$(readlink /proc/self/ns/net)
phase=offline-worker
sudo unshare --net -- setpriv --reuid="$(id -u)" --regid="$(id -g)" --init-groups \
    env -i PATH=/usr/bin:/bin HOME="$work" LANG=C.UTF-8 \
    CUEGLINT_MODEL_DIR="$CUEGLINT_MODEL_DIR" CUEGLINT_DATA_DIR="$CUEGLINT_DATA_DIR" \
    CUEGLINT_MEETING_TEST="$test_binary" CUEGLINT_MEETING_CLI="$cli" CUEGLINT_FILE_CONTROL_REPORT="$out/system-file-asr.json" \
    CUEGLINT_TEST_REVISION="$GITHUB_SHA" \
    CUEGLINT_MEETING_REPORT="$out/meeting.json" CUEGLINT_MEETING_NETWORK_REPORT="$out/network.json" \
    CUEGLINT_PARENT_NETNS="$parent_ns" \
    .github/scripts/linux-monitor-fixture.sh "$mode" 2>&1 | tee "$out/meeting.log"
