"""Interrupt an actual Cueglint CLI restore after observing a changed database page."""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import struct
import subprocess
import tempfile
import time
from contextlib import closing

ROOT = Path(__file__).resolve().parents[2]
SEED = ROOT / "parity/fixtures/original-kiln-backup-v2.json"
TABLES = ("sessions", "modes", "knowledge", "settings")
JOURNAL_MAGIC = bytes.fromhex("d9d505f920a163d7")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def normalize_backup(backup):
    backup.pop("exported_at")
    for table in ("sessions", "modes", "knowledge"):
        backup[table].sort(key=lambda row: row["id"])
    return backup


def make_fixture(seed, root, group, include_seed):
    backup = copy.deepcopy(seed)
    original = backup["sessions"][0]
    backup["sessions"] = [original] if include_seed else []
    for index in range(128):
        row = dict(original)
        row.update(
            id=f"original-{group}-{index:03}",
            title=f"{group} test record {index:03}",
            notes=(f"{group} {index:03} original clay inventory. " * 3500)[:98304],
            transcript=[],
            requests=[],
            suggestions=[],
            notes_revision=1,
        )
        backup["sessions"].append(row)
    path = root / f"{group}.json"
    path.write_bytes(canonical(backup))
    return path


def sql_snapshot(db, expected):
    with closing(sqlite3.connect(db.as_uri() + "?mode=ro", uri=True)) as conn:
        assert conn.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
        rows = {
            table: conn.execute(f"SELECT id,data FROM {table} ORDER BY id").fetchall()
            for table in TABLES
        }
        for table in ("sessions", "modes", "knowledge"):
            assert [(row_id, json.loads(data)) for row_id, data in rows[table]] == [
                (row["id"], row) for row in expected[table]
            ]
        assert [(row_id, json.loads(data)) for row_id, data in rows["settings"]] == [
            ("main", expected["settings"])
        ]
        return {
            "row_hashes": {k: sha(canonical(v)) for k, v in rows.items()},
            "counts": {k: len(v) for k, v in rows.items()},
            "schema": conn.execute("PRAGMA user_version").fetchone()[0],
            "page_bytes": conn.execute("PRAGMA page_size").fetchone()[0],
        }


def interrupt(command, db, replacement, page_size):
    journal = db.with_name(db.name + "-journal")
    pages = range(2, min(db.stat().st_size // page_size, 512), 19)
    with db.open("rb", buffering=0) as database:

        def read_page(n):
            return os.pread(database.fileno(), page_size, n * page_size)

        original = {n: read_page(n) for n in pages}
        before_db = sha(db.read_bytes())
        start = time.monotonic()
        child = subprocess.Popen(
            command + ["restore", str(replacement), "--replace"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            while time.monotonic() - start < 15 and child.poll() is None:
                try:
                    with journal.open("rb") as stream:
                        if stream.read(8) != JOURNAL_MAGIC:
                            continue
                except FileNotFoundError:
                    continue
                changed = next((n for n in pages if read_page(n) != original[n]), None)
                if changed is not None:
                    break
            else:
                raise RuntimeError(
                    "No live restore with a synced journal and changed database page"
                )
            child.send_signal(signal.SIGSTOP)
            pid, status = os.waitpid(child.pid, os.WUNTRACED)
            assert (
                pid == child.pid
                and os.WIFSTOPPED(status)
                and os.WSTOPSIG(status) == signal.SIGSTOP
            )
            content = journal.read_bytes()
            magic, records, _, db_pages, sector, pagesize = struct.unpack(
                ">8sIIIII", content[:28]
            )
            assert magic == JOURNAL_MAGIC and 0 < records < 0xFFFFFFFF
            assert db_pages >= 2 and pagesize == page_size
            changed_page = read_page(changed)
            current_db = sha(db.read_bytes())
            assert changed_page != original[changed] and current_db != before_db
            witness = {
                "elapsed_ms": round((time.monotonic() - start) * 1000, 2),
                "journal_header_magic_hex": magic.hex(),
                "header_page_records": records,
                "original_db_pages": db_pages,
                "sector_bytes": sector,
                "page_bytes": pagesize,
                "changed_db_page_index_zero_based": changed,
                "before_page_sha256": sha(original[changed]),
                "suspended_page_sha256": sha(changed_page),
                "before_db_sha256": before_db,
                "suspended_db_sha256": current_db,
                "journal_bytes": len(content),
                "journal_sha256": sha(content),
            }
        finally:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=5)
        assert child.returncode == -signal.SIGKILL and journal.is_file()
    return witness


def run(cli, root):
    db = root / "library.sqlite3"
    command = [
        "/usr/bin/sandbox-exec",
        "-p",
        "(version 1)(allow default)(deny network*)",
        str(cli),
        "--db",
        str(db),
    ]

    def execute(*args):
        return subprocess.check_output(
            command + list(map(str, args)), stdin=subprocess.DEVNULL, timeout=40
        )

    seed = json.loads(SEED.read_text())
    first = seed["sessions"][0]
    assert (
        first["requests"][0]["status"] == "completed"
        and first["suggestions"][0]["text"] == "MARBLE-42"
    )
    before = make_fixture(seed, root, "before", True)
    after = make_fixture(seed, root, "after", False)
    execute("restore", before, "--replace")
    old = normalize_backup(json.loads(execute("backup")))
    old_sql = sql_snapshot(db, old)
    assert old == normalize_backup(json.loads(before.read_bytes()))
    assert old_sql["counts"]["sessions"] == 129
    witness = interrupt(command, db, after, old_sql["page_bytes"])
    # This fresh product process is the first SQLite opener after the kill.
    recovered = normalize_backup(json.loads(execute("backup")))
    recovered_sql = sql_snapshot(db, recovered)
    assert recovered == old and recovered_sql == old_sql
    assert not db.with_name(db.name + "-journal").exists()
    execute("restore", after, "--replace")
    positive = normalize_backup(json.loads(execute("backup")))
    positive_sql = sql_snapshot(db, positive)
    assert positive == normalize_backup(json.loads(after.read_bytes()))
    assert positive_sql["counts"]["sessions"] == 128 and all(
        s["id"] != first["id"] for s in positive["sessions"]
    )
    return {
        "cli_sha256": sha(cli.read_bytes()),
        "seed_sha256": sha(SEED.read_bytes()),
        "expanded_fixture_sha256": [sha(p.read_bytes()) for p in (before, after)],
        "expanded_fixture_bytes": [p.stat().st_size for p in (before, after)],
        "interruption": witness,
        "initial_logical_sha256": sha(canonical(old)),
        "reopened_logical_sha256": sha(canonical(recovered)),
        "completed_logical_sha256": sha(canonical(positive)),
        "independent_sqlite_before": old_sql,
        "independent_sqlite_reopened": recovered_sql,
        "independent_sqlite_completed": positive_sql,
    }


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("Run with Python assertions enabled")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=Path, help="Cueglint CLI executable to test")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(
        prefix="cueglint-interrupted-restore-"
    ) as directory:
        print(
            json.dumps(
                run(args.cli.resolve(strict=True), Path(directory).resolve()), indent=2
            )
        )
