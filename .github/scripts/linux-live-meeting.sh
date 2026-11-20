#!/usr/bin/env bash
# One manual hosted run. Fetch the two pinned model profiles before entering the offline namespace.
set -euo pipefail
out="$RUNNER_TEMP/cueglint-live-meeting-evidence"
mkdir -m 700 "$out"
work=$(mktemp -d "$RUNNER_TEMP/cueglint-live-meeting.XXXXXXXX")
phase=preflight
finish() {
    local code=$?
    printf '{"stage":"%s","exit_code":%d}\n' "$phase" "$code" > "$out/run-status.json"
    rm -rf -- "$work"
}
trap finish EXIT
export CUEGLINT_DATA_DIR="$work/data" CUEGLINT_MODEL_DIR="$work/models"
export CARGO_BUILD_JOBS=2
mkdir -m 700 "$CUEGLINT_DATA_DIR" "$CUEGLINT_MODEL_DIR"
git rev-parse HEAD > "$out/source.txt"
test "$GITHUB_SHA" = "$(cat "$out/source.txt")"
sha256sum -c <(python3 - <<'PY'
import json
from pathlib import Path
fixture=json.loads(Path('parity/fixtures/clean-english-v2.json').read_bytes())
print(fixture['sha256']+'  '+fixture['audio'])
PY
)
cp parity/fixtures/clean-english-v2.json "$out/fixture.json"
uname -srmo > "$out/kernel.txt"
lscpu > "$out/cpu.txt"
rustc -Vv > "$out/rustc.txt"
pulseaudio --version > "$out/pulse.txt"
phase=build-cli
cargo build --locked -p cueglint-cli
cli="${CARGO_TARGET_DIR:-target}/debug/cueglint-cli"
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
    CUEGLINT_MEETING_TEST="$test_binary" CUEGLINT_TEST_REVISION="$GITHUB_SHA" \
    CUEGLINT_MEETING_REPORT="$out/meeting.json" CUEGLINT_PARENT_NETNS="$parent_ns" \
    .github/scripts/linux-monitor-fixture.sh meeting 2>&1 | tee "$out/meeting.log"
