"""Count interface traffic across one owned worker in its isolated Linux network namespace."""

import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time


def device_counters(text):
    counters = {}
    for line in text.splitlines()[2:]:
        name, values = line.strip().split(":", 1)
        columns = [int(value) for value in values.split()]
        if len(columns) != 16 or name in counters:
            raise ValueError("unexpected /proc/self/net/dev layout")
        counters[name] = dict(zip(("rx_bytes", "rx_packets", "tx_bytes", "tx_packets"),
                                  (columns[0], columns[1], columns[8], columns[9])))
    return counters


def snapshot():
    return {"monotonic_ns": time.monotonic_ns(),
            "netns": os.readlink("/proc/self/ns/net"),
            "interfaces": dict((name, index) for index, name in socket.if_nameindex()),
            "counters": device_counters(Path("/proc/self/net/dev").read_text())}


def compare(before, after, parent):
    if before["netns"] == parent or before["netns"] != after["netns"]:
        raise ValueError("network namespace identity changed or matched the parent")
    if (before["interfaces"] != after["interfaces"] or set(before["interfaces"]) != {"lo"}
            or set(before["counters"]) != {"lo"} or set(after["counters"]) != {"lo"}):
        raise ValueError("expected the same single loopback device in both samples")
    return {key: value - before["counters"]["lo"][key]
            for key, value in after["counters"]["lo"].items()}


def main():
    target = Path(os.environ["CUEGLINT_MEETING_NETWORK_REPORT"])
    canaries = json.loads(Path(sys.argv[1]).read_text())
    before = snapshot()
    parent = os.environ["CUEGLINT_PARENT_NETNS"]
    compare(before, before, parent)
    receipt = {"schema_version": 1, "source_commit": os.environ["CUEGLINT_TEST_REVISION"],
               "scope": "kernel interface counters in the private worker namespace, including loopback; Unix sockets are outside these counters",
               "maximum_delta_per_counter": 0, "parent_netns": parent, "pre_worker_canaries": canaries,
               "before": before, "state": "worker running"}
    with target.open("x") as output:
        json.dump(receipt, output, indent=2)
    try:
        code = subprocess.run(sys.argv[2:]).returncode
    except OSError as error:
        receipt["launch_errno"] = error.errno
        code = 1
    receipt["worker_exit_code"] = code
    receipt["after"] = snapshot()
    try:
        delta = compare(before, receipt["after"], parent)
        receipt["delta"] = delta
        receipt["traffic_check_passed"] = all(value == 0 for value in delta.values())
    except ValueError as error:
        receipt["traffic_check_passed"] = False
        receipt["traffic_check_error"] = str(error)
    receipt["state"] = "worker exited"
    target.write_text(json.dumps(receipt, indent=2) + "\n")
    return 0 if code == 0 and receipt["traffic_check_passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
