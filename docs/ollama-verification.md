# Reproduce the Ollama interoperability check

On macOS, set `OLLAMA` and `CUEGLINT` to the absolute paths of the standalone Ollama 0.40.16 and Cueglint dev.3 CLI binaries. Set `GGUF` to the pinned Qwen2.5-1.5B-Instruct Q4_K_M file. Verify all three SHA-256 identities in the report. The Ollama tarball and Cueglint release ZIP contain the required executables. [Ollama release](https://github.com/ollama/ollama/releases/tag/v0.40.16) · [Cueglint release](https://github.com/thematheusmello/cueglint/releases/tag/v0.1.0-dev.3)

The following checks use a new SQLite database and an empty Cueglint model directory. Run these commands in one shell. Python 3 and curl are verification tools.

```sh
set -eu
lab=$(mktemp -d)
mkdir "$lab/home" "$lab/tmp" "$lab/cue-models" "$lab/ollama-models"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"
model=cueglint-qwen-1.5b:fixture
cat >"$lab/loopback.sb" <<'POLICY'
(version 1)
(allow default)
(deny network-outbound (remote ip "*:*"))
(allow network-outbound (remote ip "localhost:*"))
(deny network-bind (local ip "*:*"))
(allow network-bind (local ip "localhost:*"))
POLICY
ol() {
  exec env -i PATH=/usr/bin:/bin HOME="$lab/home" TMPDIR="$lab/tmp/" \
    OLLAMA_HOST="127.0.0.1:$port" OLLAMA_MODELS="$lab/ollama-models" \
    OLLAMA_NO_CLOUD=1 OLLAMA_NUM_PARALLEL=1 OLLAMA_MAX_LOADED_MODELS=1 \
    OLLAMA_CONTEXT_LENGTH=2048 /usr/bin/sandbox-exec -f "$lab/loopback.sb" "$OLLAMA" "$@"
}
cg() {
  env -i PATH=/usr/bin:/bin TMPDIR="$lab/tmp/" /usr/bin/sandbox-exec -f "$lab/loopback.sb" \
    "$CUEGLINT" --db "$lab/state.sqlite3" --models-dir "$lab/cue-models" "$@"
}
(ol serve) >"$lab/server.log" 2>&1 &
server_pid=$!
trap 'kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true' EXIT
curl --noproxy '*' -fsS --retry 12 --retry-delay 1 --retry-connrefused "$origin/api/version"
printf 'FROM %s\nPARAMETER temperature 0\n' "$GGUF" >"$lab/Modelfile"
(ol create "$model" -f "$lab/Modelfile")
cg provider show
cg provider select compatible --model "$model" --base-url "$origin/v1"
cg provider key-status | python3 -c 'import json,sys; assert json.load(sys.stdin)["state"]=="missing"'
cg provider models
cg provider test
session=$(cg session start 'Original rehearsal fixture' | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
cg transcript "$session" 'The rehearsal takes place on Thursday. Its access code is WILLOW-482.'
cg session notes "$session" 'Original rehearsal note for the stage crew.'
cg ask "$session" 'What is the rehearsal access code and on which day does the rehearsal take place? Give just those two facts.' --stream --max-tokens 80
kill "$server_pid"
wait "$server_pid" || test "$?" -eq 143
trap - EXIT
if cg ask "$session" 'Can the disconnected endpoint answer?' --stream; then exit 1; fi
cg provider show
cg session stop "$session"
cg session save "$session"
cg session show "$session"
cg provider select local
cg provider show
```

The completed run compared the streamed text with the saved suggestion, confirmed both source facts, checked the cancelled request and reopened the authored note. The synthetic provider connection test ran before the grounded answer. The metadata reports one timing for each call.
