"""Exercise real local CLI cancellation in a disposable library on a POSIX host."""
import argparse
import hashlib
import json
from pathlib import Path
import os
import signal
import sqlite3
import subprocess
import tempfile
import time


def exercise(cli, models, directory, highlights):
    database = directory / ("highlights.sqlite" if highlights else "answer.sqlite")
    command = [cli, "--db", database, "--models-dir", models]

    def run(*args):
        output = subprocess.check_output([*command, *args], timeout=120)
        return json.loads(output)

    session = run("session", "start", "Original interruption control")["id"]
    note = "Original human note: hold the cedar samples."
    if highlights:
        run("session", "notes", session, note)
        run("transcript", session, "The cedar samples arrive on Tuesday.")
        run("session", "stop", session)
        args = ["highlights", "generate", session]
    else:
        args = ["ask", session, "List every integer from one through eighty in order.", "--max-tokens", "200"]

    def read_without_recovery():
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as conn:
            row = conn.execute("SELECT data FROM sessions WHERE id=?", (session,)).fetchone()
            return json.loads(row[0])

    with subprocess.Popen([*command, *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          start_new_session=True) as child:
        try:
            deadline = time.monotonic() + 90
            while child.poll() is None and time.monotonic() < deadline:
                before = read_without_recovery()
                if highlights:
                    active = (before.get("notes_job") or {}).get("status") == "pending"
                else:
                    active = before.get("active_request_id") is not None
                if active:
                    break
                time.sleep(.015)
            else:
                raise RuntimeError("No pending job observed; verify the pinned local answer model is installed.")
            start = time.monotonic()
            child.send_signal(signal.SIGINT)
            stdout, _ = child.communicate(timeout=30)
            elapsed_ms = round((time.monotonic() - start) * 1000, 2)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
    assert child.returncode == 130 and stdout == b""
    saved = read_without_recovery()
    assert saved == run("session", "show", session)
    result = {"signal_to_exit_ms": elapsed_ms, "exit_code": child.returncode}
    if highlights:
        job = saved["notes_job"]
        assert job["id"] == before["notes_job"]["id"]
        assert (job["status"], job["cancel_reason"]) == ("cancelled", "user")
        assert saved["notes"] == note and saved["generated_notes"] is None
        result["job_id"] = job["id"]
    else:
        request, = saved["requests"]
        assert request["id"] == before["active_request_id"] and request["status"] == "cancelled"
        assert saved["active_request_id"] is None and not saved["suggestions"]
        retry = run("ask", session, "Reply with only the word juniper.", "--max-tokens", "12")
        saved = read_without_recovery()
        first, second = saved["requests"]
        assert first == request and second["id"] != request["id"] and second["status"] == "completed"
        only_answer, = saved["suggestions"]
        assert retry["applied"] and only_answer["request_id"] == second["id"]
        assert only_answer["text"].strip().lower().rstrip(".") == "juniper"
        result.update(request_id=request["id"], retry_request=second["id"], retry_answer=only_answer["text"])
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--models-dir", required=True, type=Path)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("This external SIGINT rehearsal requires a POSIX host.")
    cli = args.cli.resolve(strict=True)
    models = args.models_dir.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="cueglint-interrupt-") as directory:
        results = {key: exercise(cli, models, Path(directory), key == "highlights")
                   for key in ("highlights", "answer")}
    results["cli_sha256"] = hashlib.sha256(cli.read_bytes()).hexdigest()
    print(json.dumps(results, indent=2))
