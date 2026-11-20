#!/usr/bin/env bash
# Ubuntu runtime packages: xvfb xauth x11-xserver-utils python3-tk fonts-dejavu-core libtesseract5 tesseract-ocr-eng
set -euo pipefail

if [[ $# != 1 || ! -x $1 ]]; then
    echo "Usage: $0 /path/to/cueglint-cli" >&2
    exit 2
fi
cli=$(realpath -- "$1")
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
install -m 600 /dev/null "$fixture/Xauthority"
# Xvfb creates a new display, requires its own cookie and disables TCP. Nothing
# from the calling desktop or audio session is selected.
env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
    XDG_RUNTIME_DIR="$fixture" XDG_SESSION_TYPE=x11 PULSE_SERVER="unix:$fixture/no-audio" \
    xvfb-run --auto-servernum --error-file=/dev/stderr --auth-file="$fixture/Xauthority" \
    --server-args="-nolisten tcp -screen 0 3840x2160x24 -screen 1 1024x768x24" \
    /usr/bin/python3 - "$cli" "$fixture" <<'PY'
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import tkinter as tk
import uuid
from pathlib import Path

cli, temp_dir = map(Path, sys.argv[1:])
display = os.environ["DISPLAY"]
assert re.fullmatch(r":\d+(?:\.0)?", display), "Expected a local Xvfb socket"
assert Path(os.environ["XAUTHORITY"]) == temp_dir / "Xauthority"
display = display.split(".")[0]
first = "CEDAR WATER NINE"
second = "MAPLE RIVER FOUR"
decoy = "VIOLET SNOW SEVEN"
base = [str(cli), "--db", str(temp_dir / "fixture.sqlite3")]
model_root = os.environ.get("CUEGLINT_SCREEN_FIXTURE_MODELS")
if model_root:
    assert Path(model_root).is_absolute() and Path(model_root).is_dir(), "Supply an absolute installed model root"
    base.extend(["--models-dir", model_root])
prompt = "Which three words are displayed now? Reply using only those words."
assert all(marker not in prompt.upper() for marker in (first, second, decoy))
frozen = []
answer_runs = []


def command(screen, *args, success=True, timeout=40):
    result = subprocess.run(base + list(args), env={**os.environ, "DISPLAY": f"{display}.{screen}"},
                            capture_output=True, timeout=timeout)
    if success:
        assert result.returncode == 0, (args, result.stderr.decode(errors="replace"))
        assert len(result.stdout) <= 32 * 1024, "Screen CLI returned oversized fixture metadata"
        return json.loads(result.stdout)
    assert result.returncode != 0, (args, "absent display succeeded")
    assert not result.stdout.strip(), (args, "absent display emitted capture output")
    return result.stderr.decode(errors="replace")


def surface(screen, width, height, marker, background):
    root = tk.Tk(screenName=f"{display}.{screen}")
    root.overrideredirect(True)
    root.geometry(f"{width}x{height}+0+0")
    canvas = tk.Canvas(root, bg=background, borderwidth=0, highlightthickness=0)
    canvas.pack(fill="both", expand=True)
    text = canvas.create_text(width // 2, height // 2, text=marker, fill="#101820",
                              font=("DejaVu Sans", -86 if screen == 0 else -48, "bold"))
    root.update()
    assert root.winfo_viewable() and (root.winfo_width(), root.winfo_height()) == (width, height)
    # The pointer query is a round trip on this X connection after the Tk draw.
    root.winfo_pointerxy()
    return root, canvas, text


windows = []
ids_seen = set()


def capture(phase, screen, selected, size, marker, absent):
    before = time.time_ns() // 1_000_000
    row = command(screen, "screen", "capture", "--display", str(selected))
    after = time.time_ns() // 1_000_000
    assert isinstance(row, dict) and "png" not in row and "pixels" not in row, phase
    assert row["display_id"] == selected and (row["width"], row["height"]) == size, (phase, row)
    assert isinstance(row["captured_at"], int) and before <= row["captured_at"] <= after, phase
    assert uuid.UUID(row["id"]).variant == uuid.RFC_4122 and row["id"] not in ids_seen, phase
    ids_seen.add(row["id"])
    assert re.fullmatch(r"[a-f0-9]{64}", row["sha256"]), phase
    words = " ".join(re.findall(r"[A-Z0-9]+", row["text"].upper()))
    assert marker in words and all(other not in words for other in absent), (phase, row["text"])
    print(json.dumps({"phase": phase, "capture": row}, sort_keys=True), flush=True)
    return row


def screen_identity(context, standalone, before, after):
    for key in ("display_id", "width", "height", "sha256", "text"):
        assert context[key] == standalone[key], (key, context, standalone)
    assert isinstance(context["captured_at"], int) and before <= context["captured_at"] <= after
    assert uuid.UUID(context["id"]).variant == uuid.RFC_4122 and context["id"] not in ids_seen
    ids_seen.add(context["id"])


def exercise_request(session, standalone, marker, excluded):
    display_id = str(standalone["display_id"])
    before = time.time_ns() // 1_000_000
    result = command(0, "request", "freeze", session, prompt, "--screen-display", display_id)
    after = time.time_ns() // 1_000_000
    req = result["request"]
    assert result["inference_executed"] is False and req["status"] == "cancelled"
    assert req["session_id"] == session and req["prompt"] == prompt
    assert req["provider"]["provider"] == "local"
    screen_identity(req["context"]["screen"], standalone, before, after)
    frozen.append(req)
    print(json.dumps({"phase": "frozen screen", "session_id": session, "request_id": req["id"],
                      "status": req["status"], "screen": req["context"]["screen"]}, sort_keys=True), flush=True)
    if not model_root:
        return
    entry = {"phase": "local answer", "expected": marker, "session_id": session,
             "prompt": prompt, "answer_quality_passed": False, "generation_completed": False}
    started = time.monotonic()
    answer_before = time.time_ns() // 1_000_000
    try:
        answer = command(0, "ask", session, prompt, "--screen-display", display_id, "--max-tokens", "24",
                         timeout=180)
        answer_after = time.time_ns() // 1_000_000
        assert answer["applied"] is True and answer["session_id"] == session
        actual = answer["output"]
        assert actual["max_new_tokens"] == 24 and actual["model"]["backend"].startswith("candle-")
        current = command(0, "session", "show", session)
        persisted = next(r for r in current["requests"] if r["id"] == answer["request_id"])
        assert persisted["status"] == "completed" and persisted["provider"]["provider"] == "local"
        screen_identity(persisted["context"]["screen"], standalone, answer_before, answer_after)
        saved = next(a for a in current["suggestions"] if a["request_id"] == answer["request_id"])
        assert saved["text"] == actual["text"]
        if answer_runs and answer_runs[0]["generation_completed"]:
            prior = answer_runs[0]
            assert any(turn["request_id"] == prior["request_id"] and turn["prompt"] == prompt
                       and turn["response"] == prior["output"]["text"] for turn in persisted["context"]["chat"]), "Second request lost the preceding answer"
            entry["prior_request_retained"] = prior["request_id"]
        words = " ".join(re.findall(r"[A-Z0-9]+", actual["text"].upper()))
        entry.update(request_id=answer["request_id"], frozen_screen=persisted["context"]["screen"],
                     generation_completed=True, output=actual,
                     answer_quality_passed=marker in words and all(other not in words for other in excluded))
    except (AssertionError, ValueError, KeyError, StopIteration, subprocess.TimeoutExpired) as error:
        entry["error"] = str(error)
    entry["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    answer_runs.append(entry)
    print(json.dumps(entry, sort_keys=True), flush=True)


try:
    primary, canvas, label = surface(0, 3840, 2160, first, "#ffffff")
    windows.append(primary)
    secondary, small_canvas, small_label = surface(1, 1024, 768, decoy, "#d5edf7")
    windows.append(secondary)
    with cli.open("rb") as executable:
        binary_hash = hashlib.file_digest(executable, "sha256").hexdigest()

    def catalog(screen):
        monitors = command(screen, "screen", "displays")
        assert isinstance(monitors, list) and len({d["id"] for d in monitors}) == len(monitors), monitors
        print(json.dumps({"phase": "display-inventory", "cli_sha256": binary_hash,
                          "x_screen": screen, "displays": monitors}, sort_keys=True), flush=True)
        return monitors

    def selected_id(screen, width, height):
        monitors = catalog(screen)
        matches = [d["id"] for d in monitors if (d["width"], d["height"]) == (width, height)]
        assert len(matches) == 1 and type(matches[0]) is int, (width, height, monitors)
        return matches[0]

    target = selected_id(0, 3840, 2160)
    other = selected_id(1, 1024, 768)
    assert target != other, "The two X screens did not expose distinct display IDs"
    primary_size, secondary_size = (2560, 1440), (1024, 768)
    session = command(0, "session", "start", "Original Linux screen fixture")["id"]
    initial = capture("first original marker", 0, target, primary_size, first, (second, decoy))
    exercise_request(session, initial, first, (second, decoy))
    other_before = capture("separate display", 1, other, secondary_size, decoy, (first, second))
    canvas.itemconfigure(label, text=second)
    primary.update()
    primary.winfo_pointerxy()
    changed = capture("changed original marker", 0, target, primary_size, second, (first, decoy))
    assert initial["sha256"] != changed["sha256"] and initial["captured_at"] < changed["captured_at"]
    exercise_request(session, changed, second, (first, decoy))
    retained = command(0, "session", "show", session)
    by_id = {r["id"]: r for r in retained["requests"]}
    assert len(frozen) == 2 and len({r["id"] for r in frozen}) == 2
    for r in frozen:
        assert by_id[r["id"]] == r and by_id[r["id"]]["status"] == "cancelled"
    screens = [r["context"]["screen"] for r in frozen]
    assert screens[0]["captured_at"] < screens[1]["captured_at"] and screens[0]["sha256"] != screens[1]["sha256"]
    print(json.dumps({"phase": "reopened frozen requests", "session_id": session, "passed": True,
                      "request_ids": [r["id"] for r in frozen],
                      "screen_ids": [screen["id"] for screen in screens]}), flush=True)
    canvas.configure(bg="#cfefdb")
    primary.update()
    primary.winfo_pointerxy()
    recolored = capture("same text new background", 0, target, primary_size, second, (first, decoy))
    assert changed["sha256"] != recolored["sha256"], "Changing only the selected background preserved the PNG hash"
    other_after = capture("untouched separate display", 1, other, secondary_size, decoy, (first, second))
    assert other_before["sha256"] == other_after["sha256"], "Untouched secondary pixels changed"
    message = command(1, "screen", "capture", "--display", str(target), success=False)
    assert str(target) in message and "display" in message.lower(), message
    print(json.dumps({"phase": "foreign screen ID refused", "x_screen": 1, "display_id": target,
                      "error": message.strip(), "passed": True}, sort_keys=True), flush=True)

    # Xvfb builds differ in how they expose user-defined, output-less RandR
    # monitors. Check with X's independent client before testing these rectangles.
    xrandr = ["xrandr", "--display", f"{display}.1"]
    configured = []
    for name, x in (("fixture-left", 0), ("fixture-right", 512)):
        result = subprocess.run(xrandr + ["--setmonitor", name, f"512/135x768/203+{x}+0", "none"],
                                capture_output=True, timeout=5)
        configured.append(result.returncode)
    listing = subprocess.run(xrandr + ["--listactivemonitors"], capture_output=True, text=True, timeout=5)
    pairs = re.findall(r"^\s*\d+: [+*]*(fixture-(?:left|right)) 512/\d+x768/\d+\+(\d+)\+0(?:\s|$)",
                       listing.stdout, re.MULTILINE)
    if configured != [0, 0] or listing.returncode != 0 or dict(pairs) != {"fixture-left": "0", "fixture-right": "512"}:
        print(json.dumps({"phase": "same-root monitor rectangles", "status": "unavailable on fixture server",
                          "configuration_exit_codes": configured, "active_monitor_listing": listing.stdout.strip()}),
              flush=True)
    else:
        small_canvas.delete("all")
        for x, marker, color in ((0, "ALDER PARK", "#ffffff"), (512, "CORAL POND", "#e9f2ef")):
            small_canvas.create_rectangle(x, 0, x+512, 768, fill=color, outline=color)
            small_canvas.create_text(x+256, 384, text=marker, fill="#101820", font=("DejaVu Sans", -48, "bold"))
        secondary.update()
        secondary.winfo_pointerxy()
        tiles = [d["id"] for d in catalog(1) if (d["width"], d["height"]) == (512,768)]
        assert len(tiles) == 2, "X advertises two selected rectangles; the product must list both"
        observed = set()
        for tile in tiles:
            # Determine which published ID is attached to each rectangle by its
            # unique original marker. Both results must exclude the other tile.
            row = command(1, "screen", "capture", "--display", str(tile))
            assert row["display_id"] == tile and (row["width"], row["height"]) == (512, 768)
            text = " ".join(re.findall(r"[A-Z0-9]+", row["text"].upper()))
            found = {word for word in ("ALDER PARK", "CORAL POND") if word in text}
            assert len(found) == 1 and found.isdisjoint(observed), (tile, text)
            observed.update(found)
            print(json.dumps({"phase": "same-root selected tile", "capture": row}, sort_keys=True), flush=True)
        assert observed == {"ALDER PARK", "CORAL POND"}
    print(json.dumps({"phase": "screen acquisition and frozen context", "passed": True}), flush=True)
    if model_root:
        assert len(answer_runs) == 2 and all(r["answer_quality_passed"] for r in answer_runs), "Local answer checks failed; see per-answer results"
    else:
        print(json.dumps({"phase": "local answer", "status": "skipped; no installed model root supplied"}), flush=True)

finally:
    for window in reversed(windows):
        window.destroy()
PY
