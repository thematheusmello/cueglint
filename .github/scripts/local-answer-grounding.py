#!/usr/bin/env python3
"""Check frozen local answer scores or replay the protocol with installed models on macOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import selectors
import signal
import socket
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "parity/fixtures/local-answer-grounding-replay-001.json"
REPORT = ROOT / "parity/reports/local-answer-grounding-replay-001.json"
PROFILES = ["qwen2.5-1.5b-instruct", "qwen2.5-7b-instruct"]
SANDBOX = ["/usr/bin/sandbox-exec", "-p", "(version 1)(allow default)(deny network*)"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_bytes())


def order(cases):
    for index, case in enumerate([*cases, cases[0]]):
        for profile in (PROFILES if index % 2 == 0 else reversed(PROFILES)):
            yield case, profile, index == len(cases)


def check(fixture, report):
    assert digest(FIXTURE.read_bytes()) == report["preregistered_protocol_sha256"]
    cases = {case["id"]: case for case in report["cases"]}
    expected = list(order(fixture["cases"]))
    assert len(expected) == len(report["runs"]) == 16
    assert list(cases) == [r["id"] for r in fixture["cases"]]
    for index, ((case, profile, repeated), row) in enumerate(zip(expected, report["runs"]), 1):
        assert (index, case["id"], profile, repeated) == (row["index"], row["case"], row["profile"], row["repeat"])
        assert row["prompt_sha256"] == cases[case["id"]]["rendered_prompt_sha256"]
        assert row["answer"].strip() and row["finish_reason"] == "end_of_turn"
        assert 0 < row["first_stream_text_ms"] <= row["wall_ms"]
        assert 0 < row["generation_ms"] <= row["wall_ms"]
        assert row["max_rss_bytes"] > 0 and 0 < row["generated_tokens"] <= 256
        assert row["score"]["pass"] == all(row["score"][field] in ("supported", "abstained") for field in ("color", "hour"))
    assert report["runs"][0]["prompt_sha256"] == fixture["observed_ubuntu_prompt_sha256"]
    assert report["passed"] == all(r["score"]["pass"] for r in report["runs"] if not r["repeat"])
    for profile in PROFILES:
        rows = [r for r in report["runs"] if r["profile"] == profile]
        assert rows[0]["answer"] == rows[-1]["answer"]
        scores = [r["score"] for r in rows if not r["repeat"]]
        summary = report["summary"][profile]
        assert summary["distinct_cases"] == 7 and summary["n_timing"] == 8
        assert sum(s["pass"] for s in scores) == summary["complete_cases"]
        assert sum(not s["pass"] for s in scores) == summary["cases_with_unsupported_specifics"]
        for label, key in [("abstained", "unavailable_or_conflicting_slots_correctly_abstained"), ("supported", "determinate_slots_correct")]:
            assert sum(s[field] == label for s in scores for field in ("color", "hour")) == summary[key]
        for field, stats in summary["timing"].items():
            samples = [r[field] for r in rows]
            assert [statistics.median(samples), max(samples), min(samples), max(samples)] == [
                stats[k] for k in ("median", "p95", "minimum", "maximum")
            ]


def canary():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        nc = ["/usr/bin/nc", "-G", "1", "-vz", "127.0.0.1", str(listener.getsockname()[1])]
        subprocess.run(nc, capture_output=True, check=True, timeout=5)
        listener.accept()[0].close()
        denied = subprocess.run([*SANDBOX, *nc], capture_output=True, timeout=5)
        if not denied.returncode or b"operation not permitted" not in denied.stderr.lower():
            raise RuntimeError("Loopback sandbox denial could not be verified")


def generate(argv, env, directory, index):
    start = time.monotonic()
    before_text = None
    stream = bytearray()
    timing = directory / f"{index}.time"
    stdout = directory / f"{index}.json"
    with stdout.open("xb") as out:
        child = subprocess.Popen(["/usr/bin/time", "-lp", "-o", str(timing), *argv],
                                 env=env, cwd=directory, stdout=out, stderr=subprocess.PIPE,
                                 start_new_session=True)
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(child.stderr, selectors.EVENT_READ)
                while time.monotonic() - start <= 600:
                    if not ready.select(timeout=1):
                        continue
                    data = os.read(child.stderr.fileno(), 4096)
                    if not data:
                        break
                    if before_text is None and data.strip():
                        before_text = round(1000 * (time.monotonic() - start))
                    stream.extend(data)
                else:
                    raise TimeoutError(f"Ask {index} exceeded 600 seconds")
            child.wait(timeout=5)
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
    wall = round(1000 * (time.monotonic() - start))
    if child.returncode:
        raise RuntimeError(f"Ask {index} exited {child.returncode}: {stream.decode(errors='replace')}")
    reply = load(stdout)
    output = reply["output"]
    assert reply["applied"] and before_text is not None and stream.decode().strip() == output["text"].strip()
    rss = int(re.search(r"(\d+)\s+maximum resident set size", timing.read_text())[1])
    return {"output": output, "wall_ms": wall, "first_stream_text_ms": before_text, "max_rss_bytes": rss}


def replay(args, fixture, report):
    if platform.system() != "Darwin":
        raise RuntimeError("This runner uses the macOS network sandbox and time utility")
    cli = args.cli.resolve(strict=True)
    roots = dict(zip(PROFILES, [args.small_models.resolve(strict=True), args.large_models.resolve(strict=True)]))
    dest = args.output.resolve()
    dest.mkdir(mode=0o700)
    canary()
    env = {"HOME": str(dest), "TMPDIR": str(dest), "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
           "LANG": "C", "RAYON_NUM_THREADS": "4"}
    with tempfile.TemporaryDirectory(dir=dest) as tmp:
        work = Path(tmp)

        def command(db, profile, *words):
            return [*SANDBOX, str(cli), "--db", str(db), "--models-dir", str(roots[profile]), *words]

        def result(db, profile, *words):
            return json.loads(subprocess.check_output(command(db, profile, *words), env=env, timeout=120))

        seed = work / "seed.sqlite3"
        result(seed, PROFILES[0], "session", "start", "Original Linux monitor meeting", "--mode", "meeting")
        base = result(seed, PROFILES[0], "backup")
        for profile in PROFILES:
            checked = result(seed, profile, "models", "verify", profile)
            assert checked["profile"]["files"] == report["models"][profile]["artifacts"]
        (dest / "environment.json").write_text(json.dumps({
            "cli_sha256": digest(cli.read_bytes()), "system": platform.platform(),
            "protocol_sha256": digest(FIXTURE.read_bytes()), "rayon_num_threads": 4,
            "canary": "reachable outside; denied with EPERM inside the same CLI sandbox",
        }, indent=2))
        matches = 0
        with (dest / "results.jsonl").open("x") as output:
            for index, (case, profile, repeat) in enumerate(order(fixture["cases"]), 1):
                session = base["sessions"][0]
                first = min(s["created_at"] for s in case["transcript"])
                last = max(s["updated_at"] for s in case["transcript"])
                session.update(status="stopped", transcript=case["transcript"], context_revision=3,
                               created_at=first-10000, updated_at=last, saved_at=last)
                backup = work / "fixture.json"
                backup.write_text(json.dumps(base))
                db = work / f"{index}.sqlite3"
                assert result(db, profile, "restore", str(backup))["restored"]
                argv = command(db, profile, "ask", session["id"], case["question"],
                               "--profile", profile, "--max-tokens", "256", "--stream")
                measured = generate(argv, env, work, index)
                actual = measured["output"]
                historical = report["runs"][index-1]
                assert actual["prompt_sha256"] == historical["prompt_sha256"], f"Frozen prompt changed in case {case['id']}"
                assert actual["model"]["profile_id"] == profile and actual["model"]["backend"] == "candle-cpu-0.11.1"
                assert actual["template_revision"] == report["config"]["template"]
                row = {"index": index, "case": case["id"], "profile": profile, "repeat": repeat,
                       "matches_recorded_text": actual["text"] == historical["answer"], **measured}
                matches += row["matches_recorded_text"]
                output.write(json.dumps(row) + "\n")
                output.flush()
                print(f"{index}/16 {case['id']} {profile} {measured['wall_ms']} ms", flush=True)
        print(f"{matches}/16 replies reproduced the archived wording. Score changed replies against the frozen rubric.")


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("Run without Python optimization to enable evidence assertions.")
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("check", help="Recalculate published receipt totals and timing results")
    run = sub.add_parser("replay", help="Run sixteen real Ask calls using existing model files")
    for name in ("cli", "small-models", "large-models", "output"):
        run.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    fixture, report = load(FIXTURE), load(REPORT)
    check(fixture, report)
    if args.action == "replay":
        replay(args, fixture, report)
    else:
        print("Verified seven cases, sixteen runs, both profiles, summary counts and timing distributions.")
