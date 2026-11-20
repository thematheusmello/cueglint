"""Exercise the ordinary native assistant on a fresh, offline hosted X display."""
import csv
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import time

BIN, OUT = map(Path, sys.argv[1:])
DB = Path(os.environ["CUEGLINT_DATA_DIR"]) / "meeting.sqlite"
PROMPT = "What is three plus four? Spell the answer in one word."
MODEL = "qwen2.5-1.5b-instruct"


def command(*args):
    return subprocess.check_output(args, text=True, timeout=15).strip()


def until(action, timeout, description, app):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if app.poll() is not None:
            raise RuntimeError(f"Cueglint exited with {app.returncode} while waiting for {description}")
        value = action()
        if value:
            return value
        time.sleep(0.5)
    raise TimeoutError(description)


def window_for(app, height):
    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Cueglint Assistant$"],
                           text=True, capture_output=True, timeout=5)
    if found.returncode not in (0, 1):
        raise RuntimeError(found.stderr)
    for wid in found.stdout.split():
        if command("xdotool", "getwindowpid", wid) != str(app.pid):
            continue
        geometry = command("xdotool", "getwindowgeometry", "--shell", wid).splitlines()
        if "WIDTH=690" in geometry and f"HEIGHT={height}" in geometry:
            return wid
    return None


def click(wid, x, y):
    command("xdotool", "mousemove", "--window", wid, str(x), str(y), "click", "1")


def recognize(wid, stage, header=False):
    frame = OUT / f"{stage}.png"
    command("import", "-window", wid, str(frame))
    scaled = Path(os.environ["HOME"]) / "ocr.png"
    crop = ["-crop", "622x45+7+6", "+repage"] if header else []
    command("convert", str(frame), *crop, "-colorspace", "Gray", "-resize", "200%", "-negate", str(scaled))
    tsv = command("tesseract", str(scaled), "stdout", "--psm", "7" if header else "11", "tsv")
    (OUT / f"{stage}.tsv").write_text(tsv + "\n")
    return [row for row in csv.DictReader(tsv.splitlines(), delimiter="\t") if row.get("text", "").strip()]


def find_word(wid, stage, word, *, header=False):
    for row in recognize(wid, stage, header):
        if row["text"].strip("!?.,:;").casefold() == word:
            x = (int(row["left"]) + int(row["width"]) // 2) // 2
            y = (int(row["top"]) + int(row["height"]) // 2) // 2
            return (x + 7, y + 6) if header else (x, y)
    return None


def counters():
    loopback, = json.loads(command("ip", "-j", "-s", "link"))
    assert loopback["ifname"] == "lo"
    for family in ("-4", "-6"):
        assert not json.loads(command("ip", family, "-j", "route", "show", "default"))
    stats = loopback["stats64"]
    return {f"{direction}_{unit}": stats[direction][unit]
            for direction in ("rx", "tx") for unit in ("bytes", "packets")}


def desktop(reopen):
    label = "reopen" if reopen else "first"
    with (OUT / f"{label}-app.log").open("w") as log:
        app = subprocess.Popen([BIN / "cueglint", "--overlay", "--database", DB], stdout=log, stderr=log)
        try:
            def window(height):
                wid = until(lambda: window_for(app, height), 60, f"{height}px assistant window", app)
                command("xdotool", "windowfocus", "--sync", wid)
                return wid

            wid = window(104)
            if reopen:
                click(wid, 620, 80)
                wid = window(760)
                _, row_y = until(lambda: find_word(wid, "history", "stopped"), 30, "saved History row", app)
                click(wid, 185, row_y)
                x, y = until(lambda: find_word(wid, "detail", "conversation"), 30, "Open conversation button", app)
                click(wid, x, y)
            else:
                # X11 maps the child before its first native input field is ready.
                until(lambda: find_word(wid, "ready", "ask", header=True), 60, "painted Ask field", app)
                click(wid, 120, 32)
                command("xdotool", "type", "--clearmodifiers", "--delay", "60", "What")
                until(lambda: find_word(wid, "first-word", "what", header=True), 20, "focused typed field", app)
                command("xdotool", "type", "--clearmodifiers", "--delay", "40", PROMPT[4:])
                until(lambda: find_word(wid, "typed", "three", header=True), 20, "typed original question", app)
                command("xdotool", "key", "ctrl+a", "ctrl+c")
                def copied_prompt():
                    clip = subprocess.run(["xclip", "-selection", "clipboard", "-out"],
                                          capture_output=True, text=True, timeout=5)
                    (OUT / "selected-draft.txt").write_text(clip.stdout)
                    return clip.returncode == 0 and clip.stdout == PROMPT
                until(copied_prompt, 10, "complete original question in the native selection", app)
                command("xdotool", "key", "End", "Return")
            wid = window(500)
            until(lambda: find_word(wid, f"{label}-answer", "seven"), 180, "answer visible in native frame", app)
            if not reopen:
                # Wait for terminal persistence before asking the ordinary Quit shortcut to close.
                with sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=1) as reader:
                    def completed():
                        return reader.execute(
                            "SELECT 1 FROM sessions WHERE json_extract(data,'$.requests[0].status')='completed'"
                        ).fetchone()
                    until(completed, 180, "completion of the submitted request", app)
            command("xdotool", "key", "ctrl+q")
            if app.wait(timeout=30) != 0:
                raise RuntimeError("ordinary Quit exited unsuccessfully")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()


def saved():
    cli = [BIN / "cueglint-cli", "--db", DB, "session"]
    record, = json.loads(command(*cli, "list"))
    session = json.loads(command(*cli, "show", record["id"]))
    question, = session["requests"]
    answer, = session["suggestions"]
    assert question["prompt"] == PROMPT and question["status"] == "completed"
    assert answer["request_id"] == question["id"] and re.search(r"\bseven\b", answer["text"], re.I)
    model = json.loads((OUT / "model.json").read_text())["profile"]
    weights, = [f for f in model["files"] if f["name"].endswith(".gguf")]
    assert model["id"] == answer["inference"]["model_profile"] == MODEL
    assert answer["inference"]["model_sha256"] == weights["sha256"]
    assert not answer["hosted_inference"] and not session["transcript"]
    return session


before = counters()
report = {"kind": "implementation-only", "network_before": before, "question": PROMPT}
manager = subprocess.Popen(["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    desktop(False)
    first = saved()
    (OUT / "saved.json").write_text(json.dumps(first, indent=2) + "\n")
    desktop(True)
    second = saved()
    assert first["id"] == second["id"] and first["requests"] == second["requests"] and first["suggestions"] == second["suggestions"]
    report["reopened_session"] = second["id"]
finally:
    manager.terminate()
    manager.wait(timeout=10)
    report["network_after"] = counters()
    (OUT / "result.json").write_text(json.dumps(report, indent=2) + "\n")
assert report["network_after"] == before == dict.fromkeys(before, 0)
