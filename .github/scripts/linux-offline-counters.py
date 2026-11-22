"""Print traffic counters from the calling process's isolated Linux network namespace."""
import json
import os
import subprocess
import sys


def counters(parent, namespace, links, routes):
    if namespace == parent:
        raise ValueError("The runner still owns the parent network namespace")
    if [link["ifname"] for link in links] != ["lo"] or any(routes):
        raise ValueError("Expected only loopback, with no IPv4 or IPv6 default route")
    stats = links[0]["stats64"]
    result = {f"{direction}_{unit}": stats[direction][unit]
              for direction in ("rx", "tx") for unit in ("bytes", "packets")}
    if any(type(value) is not int or value != 0 for value in result.values()):
        raise ValueError(f"Expected zero network traffic, found {result}")
    return result


if __name__ == "__main__":
    def ip(*args):
        return json.loads(subprocess.check_output(["ip", "-j", *args]))
    snapshot = counters(sys.argv[1], os.readlink("/proc/self/ns/net"), ip("-s", "link"),
                        [ip(family, "route", "show", "default") for family in ("-4", "-6")])
    print(json.dumps(snapshot, sort_keys=True))
