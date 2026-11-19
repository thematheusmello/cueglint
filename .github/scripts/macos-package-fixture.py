"""Hosted smoke for the signed, round-tripped macOS desktop executable."""

import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zlib


FIXTURE = "native-v2-calendar-reminder"
ROOT_SIZE = (690, 164)
DEADLINE_SECONDS = 30
MAX_LOG_BYTES = 1024 * 1024


def require(check, message):
    if not check:
        raise ValueError(message)


def launch(binary, png, out, scratch, env):
    stdout = out / "fixture.stdout"
    stderr = out / "fixture.stderr"
    with stdout.open("xb") as output, stderr.open("xb") as errors:
        with subprocess.Popen(
            [binary, "--fixture", FIXTURE, "--screenshot", png],
            cwd=scratch, env=env, stdout=output, stderr=errors, start_new_session=True,
        ) as child:
            deadline = time.monotonic() + DEADLINE_SECONDS
            try:
                while True:
                    require(
                        all(path.stat().st_size <= MAX_LOG_BYTES for path in (stdout, stderr)),
                        "fixture exceeded its log byte limit",
                    )
                    status = child.poll()
                    if status is not None:
                        return status
                    require(time.monotonic() < deadline, "fixture missed its exit deadline")
                    time.sleep(0.05)
            finally:
                if child.poll() is None:
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass


def events(path):
    require(path.stat().st_size <= MAX_LOG_BYTES, "fixture exceeded its log byte limit")
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict):
            yield event


def verify_png(png, scale, image):
    require(png.is_file() and not png.is_symlink() and png.stat().st_size <= 16 * 1024 * 1024,
            "missing or oversized package fixture PNG")
    data = png.read_bytes()
    require(data.startswith(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR")
            and data[24:29] == b"\x08\x06\x00\x00\x00", "expected an RGBA8 PNG")
    width, height = struct.unpack_from(">II", data, 16)
    require((width, height) == tuple(points * scale for points in ROOT_SIZE)
            and image == {"width": width, "height": height},
            "fixture must be exactly 690x164 points and match the PNG dimensions")

    # Validate the actual bounded RGBA scanline stream, not just its header.
    # Its filters do not need reversing to check length and stream integrity.
    compressed = []
    offset = 8
    while True:
        require(offset + 12 <= len(data), "truncated PNG chunk")
        size, kind = struct.unpack_from(">I4s", data, offset)
        end = offset + 8 + size
        require(end + 4 <= len(data), "truncated PNG payload")
        require(zlib.crc32(data[offset + 4:end]) == struct.unpack_from(">I", data, end)[0],
                "invalid PNG chunk checksum")
        if kind == b"IDAT":
            compressed.append(data[offset + 8:end])
        offset = end + 4
        if kind == b"IEND":
            require(size == 0 and offset == len(data), "invalid PNG end chunk")
            break
    stride = 1 + 4 * width
    stream = zlib.decompressobj()
    pixels = stream.decompress(b"".join(compressed), height * stride + 1)
    require(stream.eof and not stream.unused_data and len(pixels) == height * stride
            and all(filter_type <= 4 for filter_type in pixels[::stride]), "invalid PNG pixel stream")
    return hashlib.sha256(data).hexdigest()


def verify_frame(out, png):
    saved = [e for e in events(out / "fixture.stdout") if e.get("event") == "native_screenshot_saved"]
    require(len(saved) == 1, "expected one saved screenshot receipt")
    receipt = saved[0]
    require(receipt["fixture"] == FIXTURE, "unexpected fixture receipt")
    request = uuid.UUID(receipt["request_id"])
    require(request.version == 4 and str(request) == receipt["request_id"], "invalid screenshot request ID")
    require(Path(receipt["path"]).resolve(strict=True) == png, "receipt output path differs")

    stage = "native_screenshot_stage"
    transport = "native_screenshot_transport"
    expected = [
        (stage, "requested"), (transport, "copy_submitted"), (transport, "readback_ready"),
        (stage, "event_received"), (stage, "queued"),
    ]
    failed = {
        "map_failed", "format_unsupported", "delivery_failed", "queue_overflow",
        "viewport_dropped", "worker_error", "save_error", "queue_error",
    }
    observed = []
    for event in events(out / "fixture.stderr"):
        if event.get("event") not in (stage, transport):
            continue
        require(event["request_id"] == str(request) and event["fixture"] == FIXTURE,
                "screenshot trace has a different request or fixture")
        require(event["stage"] not in failed, "screenshot trace reported a terminal error")
        mark = (event["event"], event["stage"])
        if mark in expected:
            require(event["viewport"] == "root", "screenshot trace used a non-root viewport")
            if mark == expected[0]:
                require(event["content_ready"] is True, "reminder was not ready at request")
            observed.append(mark)
    require(observed == expected, "missing, duplicate, or unordered root screenshot stages")

    adapter = receipt["adapter"]
    require(adapter["backend"] == "Metal" and adapter["name"], "expected the native Metal adapter")
    scale = receipt["pixels_per_point"]
    require(scale in (1, 2) and receipt["zoom_factor"] == 1, "unexpected native scale or zoom")
    png_sha256 = verify_png(png, scale, receipt["image"])
    receipt.pop("path")
    return {"receipt": receipt, "png_sha256": png_sha256}


def main(binary, out):
    out.mkdir(mode=0o700)
    report = {
        "schema_version": 1, "fixture": FIXTURE, "source_commit": os.environ.get("GITHUB_SHA"),
        "status": "failed", "platform": platform.system(), "architecture": platform.machine(),
        "deadline_seconds": DEADLINE_SECONDS, "viewport_points": ROOT_SIZE,
    }
    try:
        require((report["platform"], report["architecture"]) == ("Darwin", "arm64"),
                "package fixture requires an Apple silicon macOS host")
        for key, flag in (("os_version", "-productVersion"), ("os_build", "-buildVersion")):
            report[key] = subprocess.check_output(["/usr/bin/sw_vers", flag], text=True, timeout=5).strip()
        report["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        png = out / "fixture.png"
        with tempfile.TemporaryDirectory(prefix="profile-", dir=out.parent) as directory:
            scratch = Path(directory)
            env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "en_US.UTF-8", "TZ": "UTC"}
            for key, name in (("HOME", "home"), ("TMPDIR", "tmp"),
                              ("CUEGLINT_DATA_DIR", "data"), ("CUEGLINT_MODEL_DIR", "models")):
                path = scratch / name
                path.mkdir(mode=0o700)
                env[key] = str(path)
            report["exit_code"] = launch(binary, png, out, scratch, env)
            require(report["exit_code"] == 0, "packaged fixture did not exit normally")
            report.update(verify_frame(out, png))
            require(not any((scratch / "data").iterdir()) and not any((scratch / "models").iterdir()),
                    "fixture wrote to its isolated data or model directory")
        require(hashlib.sha256(binary.read_bytes()).hexdigest() == report["binary_sha256"],
                "extracted executable changed during verification")
        report["status"] = "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (out / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(*(Path(arg).resolve() for arg in sys.argv[1:]))
