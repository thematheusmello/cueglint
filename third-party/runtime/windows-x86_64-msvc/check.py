#!/usr/bin/env python3
"""Read-only source check; not a package consumer or a readiness decision."""

import argparse
import hashlib
import json
import re
import stat
import sys
import tomllib
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[3]
ROUTE = "third-party/runtime/windows-x86_64-msvc"
LIMIT = 2 * 1024 * 1024
INPUTS = {
    "Cargo.lock", "Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml",
    "crates/cueglint/Cargo.toml", "crates/cueglint-cli/Cargo.toml",
    "crates/cueglint-core/Cargo.toml", "crates/cueglint-local/Cargo.toml",
    "crates/cueglint-providers/Cargo.toml", "xtask/Cargo.toml",
    "vendor/eframe/Cargo.toml", "vendor/egui-wgpu/Cargo.toml",
}
NOTICE_EXACT = {
    "LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY.md", "ui/brand/LICENSE",
    "third-party/runtime/README.md", "third-party/runtime/omissions.json",
    "vendor/eframe/CUEGLINT-PATCH.json", "vendor/eframe/LICENSE-APACHE", "vendor/eframe/LICENSE-MIT",
    "vendor/egui-wgpu/CUEGLINT-PATCH.json", "vendor/egui-wgpu/LICENSE-APACHE", "vendor/egui-wgpu/LICENSE-MIT",
    f"{ROUTE}/README.md", f"{ROUTE}/graph.json",
}
NOTICE_ROOTS = (
    "third-party/runtime/texts/", f"{ROUTE}/texts/", "crates/cueglint/third-party/",
    "crates/cueglint-core/third-party/knowledge/",
    "crates/cueglint-providers/third-party/",
    "crates/cueglint-local/assets/licenses/asr-frontend/",
)


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(relative):
    require(isinstance(relative, str) and 0 < len(relative) <= 1024, "invalid path")
    value = PurePosixPath(relative)
    parts = value.parts
    require(parts and not value.is_absolute() and "/".join(parts) == relative, "noncanonical path")
    reserved = {"CON", "PRN", "AUX", "NUL"} | {
        f"{prefix}{number}" for prefix in ["COM", "LPT"] for number in range(1, 10)
    }
    path = ROOT
    for index, part in enumerate(parts):
        require(part not in {".", ".."} and not part.endswith((" ", ".")), "unsafe path")
        require(not any(char in part for char in '\\:<>"|?*\0'), "nonportable path")
        require(part.split(".", 1)[0].upper() not in reserved, "reserved Windows name")
        path = path / part
        info = path.lstat()
        require(not stat.S_ISLNK(info.st_mode), "symlink in declared source path")
        require(not getattr(info, "st_reparse_tag", 0), "reparse point in declared source path")
        if index + 1 < len(parts):
            require(stat.S_ISDIR(info.st_mode), "non-directory ancestor")
    require(stat.S_ISREG(info.st_mode) and info.st_size <= LIMIT, "file type/size bound")
    with path.open("rb") as stream:
        data = stream.read(LIMIT + 1)
    require(len(data) <= LIMIT, "read bound exceeded")
    return data


def check():
    raw = read(f"{ROUTE}/manifest.json")
    manifest = json.loads(raw)
    require(manifest["schema_version"] == 1, "unsupported manifest schema")
    require(type(manifest["bundle_ready"]) is bool, "invalid readiness flag")
    snapshot = manifest["source_snapshot"]
    require(snapshot["target"] == "x86_64-pc-windows-msvc", "wrong target")
    require(snapshot["roots"] == ["cueglint@0.1.0", "cueglint-cli@0.1.0"], "wrong roots")
    reuse = manifest["reused_source_inventory"]
    require(reuse["source_path"] == "third-party/runtime/manifest.json", "wrong reuse source")
    shared_bytes = read(reuse["source_path"])
    require(digest(shared_bytes) == reuse["current_checkout_sha256"], "macOS inventory changed")
    shared_manifest = json.loads(shared_bytes)
    rebind = manifest["source_rebind"]
    frozen = manifest["frozen_windows_provenance"]
    changes = {item["source_path"]: item for item in rebind["input_changes"]}
    require(len(changes) == len(rebind["input_changes"]), "duplicate source rebind identity")
    mac_change = changes[reuse["source_path"]]
    checkout_rebind = reuse["current_checkout_rebind"]
    require(checkout_rebind["shared_manifest_record"] == "publisher_grant_supplement", "macOS rebind record")
    supplement = shared_manifest["publisher_grant_supplement"]
    require(mac_change["previous_sha256"] == reuse["sha256"]
            and mac_change["current_sha256"] == checkout_rebind["previous_sha256"]
            and checkout_rebind["previous_sha256"] == supplement["previous_manifest_sha256"]
            and checkout_rebind["source_base_commit"] == supplement["source_base_commit"], "macOS reuse history")
    checkout = manifest["source_collection"]["original_text_checkout_policy"]
    require(checkout["source_path"] == f"{ROUTE}/.gitattributes", "checkout policy path")
    require(digest(read(checkout["source_path"])) == checkout["sha256"], "checkout policy identity")
    inputs = manifest["inputs"]
    require(len(inputs) == len(INPUTS), "input count")
    require({item["source_path"] for item in inputs} == INPUTS, "input set")
    for item in inputs:
        name = item["source_path"]
        require(digest(read(name)) == item["sha256"], f"input identity changed: {name}")
        if name in changes:
            require(item["sha256"] == changes[name]["current_sha256"], "Cargo rebind identity")
    records = manifest["files"]
    require(0 < len(records) <= 1024, "payload count bound")
    payloads, portable, total = {}, set(), 0
    for item in records:
        name = item["source_path"]
        require(name in NOTICE_EXACT or name.startswith(NOTICE_ROOTS), "unreviewed notice root")
        require(name not in payloads and name.casefold() not in portable, "duplicate/case collision")
        require(type(item["bytes"]) is int and 0 <= item["bytes"] <= LIMIT, "payload size")
        require(re.fullmatch("[0-9a-f]{64}", item["sha256"]), "invalid SHA256")
        total += item["bytes"]
        require(len(raw) + total <= 16 * 1024 * 1024, "aggregate payload bound")
        data = read(name)
        require(len(data) == item["bytes"] and digest(data) == item["sha256"], "payload identity")
        payloads[name] = data
        portable.add(name.casefold())
    for name in portable:
        require(not any(parent in portable for parent in (
            "/".join(name.split("/")[:index]) for index in range(1, len(name.split("/")))
        )), "file/directory prefix collision")
    graph_path = snapshot["graph_record"]
    require(graph_path == f"{ROUTE}/graph.json" and graph_path in payloads, "graph reference")
    graph = json.loads(payloads[graph_path])
    require(graph["schema_version"] == 1 and graph["target"] == snapshot["target"], "graph scope")
    require(graph["source_commit"] == snapshot["base_commit"], "graph source")
    require(graph["source_tree"] == snapshot["base_tree"], "graph tree")
    rows = graph["packages"]
    require(len(rows) == 397, "selected graph count")
    for key, count in [("candidate_packages", 397), ("normal_candidates", 386),
                       ("build_only_candidates", 11), ("proc_macro_candidates", 26)]:
        require(graph[key] == count and snapshot[key] == count, "declared graph count")
    graph_ids = {item["id"]: item for item in rows}
    require(len(graph_ids) == len(rows), "duplicate graph ID")
    projection = sorted((row["id"], row["license_expression_reported"], sorted(row["features"]))
                        for row in rows)
    normalized = digest(json.dumps(projection, separators=(",", ":")).encode())
    require(normalized == rebind["normalized_graph_sha256"] == frozen["normalized_graph_sha256"],
            "normalized graph identity")
    native_graph = frozen["native_graph"]
    require(native_graph["expected_graph_sha256"] == digest(payloads[graph_path])
            and native_graph["packages"] == len(rows) and native_graph["differences"] == 0,
            "frozen native graph identity")
    require(rebind["source_commit"] != frozen["source_commit"]
            and rebind["collector_host"] == "aarch64-apple-darwin"
            and rebind["native_windows_at_rebound_source_verified"] is False
            and frozen["provenance_only"] is True, "native graph scope")
    require(sum(item["graph_role"] == "normal_or_proc_macro" for item in rows) == 386, "normal count")
    require(sum(item["graph_role"] == "build_only" for item in rows) == 11, "build-only count")
    require(sum(item["proc_macro"] is True for item in rows) == 26, "proc-macro count")
    packages = manifest["packages"]
    require(len(packages) == 397 and {item["id"] for item in packages} == set(graph_ids), "package closure")
    locked = {item["name"] + "@" + item["version"]: item for item in tomllib.loads(read("Cargo.lock").decode())["package"]}
    referenced = set()
    for package in packages:
        key = package["id"]
        require(key == package["name"] + "@" + package["version"], "package ID")
        for field in ["license_expression_reported", "graph_role", "proc_macro"]:
            require(package[field] == graph_ids[key][field], "graph/package mismatch")
        shared = package.get("source_record_reuse")
        if shared:
            require(shared["manifest_sha256"] == reuse["sha256"] and shared["package_id"] == key,
                    "historical package source reuse")
        source = package["source"]
        if source["kind"] == "registry_archive":
            require(source["archive_sha256"] == locked[key]["checksum"], "lock/archive identity")
        require(package["notices"], "package notice association missing")
        for notice in package["notices"]:
            name = notice["file"]
            require(name in payloads, "dangling package notice")
            if "original_sha256" in notice:
                require(digest(payloads[name]) == notice["original_sha256"], "original notice identity")
            if "git_blob_sha1" in notice:
                blob = b"blob " + str(len(payloads[name])).encode() + b"\0" + payloads[name]
                require(hashlib.sha1(blob).hexdigest() == notice["git_blob_sha1"], "upstream blob identity")
            referenced.add(name)
    for group in ["embedded_assets", "toolchains", "additional_retained_envelopes"]:
        for association in manifest[group]:
            for name in association["notice_files"]:
                require(name in payloads, "dangling asset/toolchain/envelope notice")
                referenced.add(name)
    require(referenced == set(payloads), "unassociated payload")

    rust, = manifest["toolchains"]
    installation = rust["native_windows_installation"]
    notice, = rust["notice_files"]
    require(rust["actual_windows_host_notice_verified"] is True
            and installation["source_commit"] == frozen["source_commit"]
            and installation["workflow_run_id"] == frozen["run_id"]
            and installation["target"] == snapshot["target"], "Windows Rust provenance scope")
    require(installation["notice"]["sha256"] == rust["candidate_notice_sha256"] == digest(payloads[notice])
            and installation["notice"]["bytes"] == len(payloads[notice]), "Windows Rust notice identity")
    require({item["relative_path"] for item in installation["installer_records"]} == {
        r"lib\rustlib\manifest-rustc-x86_64-pc-windows-msvc",
        r"lib\rustlib\manifest-rust-std-x86_64-pc-windows-msvc",
        r"lib\rustlib\multirust-channel-manifest.toml",
    }, "Windows Rust installer record membership")

    by_id = {package["id"]: package for package in packages}
    ring = by_id["ring@0.17.14"]
    proof_path = f"{ROUTE}/texts/ring-0.17.14/ring-windows-x86_64-source-provenance.json"
    ring_proof = json.loads(payloads[proof_path])
    ring_notice = f"{ROUTE}/texts/ring-0.17.14/NOTICE-windows-x86_64-source-headers.txt"
    arm64_notice = ring_proof["retained_terms"]["previous_source_notice"]
    ring_notices = {entry["file"]: entry for entry in ring["notices"]}
    require({proof_path, ring_notice, arm64_notice["source_path"]} <= set(ring_notices)
            and ring_notices[ring_notice]["provenance_record"] == proof_path, "Ring notice closure")
    require(ring_proof["target"] == snapshot["target"]
            and ring_proof["archive"]["sha256"] == ring["source"]["archive_sha256"]
            and ring_proof["payload"]["path"] == PurePosixPath(ring_notice).name
            and ring_proof["payload"]["sha256"] == digest(payloads[ring_notice])
            and ring_proof["payload"]["bytes"] == len(payloads[ring_notice])
            and arm64_notice["sha256"] == digest(payloads[arm64_notice["source_path"]]),
            "Ring source attribution identity")
    gaps = manifest["gaps"]
    require(0 < len(gaps) <= 64, "gap count bound")
    require(len({gap["id"] for gap in gaps}) == len(gaps), "duplicate gap")
    require(all(type(gap["blocks_bundle"]) is bool for gap in gaps), "gap readiness flag")
    blockers = [gap["id"] for gap in gaps if gap["blocks_bundle"]]
    host_gap = next((gap for gap in gaps if gap["id"] == "native-Windows-host-graph-and-Rust-notice"), None)
    require(host_gap is not None, "missing native host source disposition")
    if not host_gap["blocks_bundle"]:
        candidate = manifest["package_candidate_qualification"]
        require(host_gap["source_readiness_disposition"]["evidence"] == "package_candidate_qualification",
                "native host source disposition")
        report_path = "parity/reports/windows-package-candidate-actual-001.json"
        require(candidate["report"] == report_path, "candidate report path")
        candidate_bytes = read(report_path)
        require(digest(candidate_bytes) == candidate["report_sha256"], "candidate report identity")
        report = json.loads(candidate_bytes)
        pin_path = ".github/scripts/windows-host-qualification/expected-package-candidate-inputs.json"
        pin = read(pin_path)
        require(digest(pin) == candidate["expected_inputs_sha256"] == report["input_record"]["sha256"],
                "candidate pinned input record")
        expected = json.loads(pin)
        require(report["passed"] is True
                and report["source_commit"] == candidate["source_commit"] == expected["source_commit"]
                and report["source_tree"] == candidate["source_tree"] == expected["source_tree"]
                and report["controller_commit"] == candidate["controller_commit"]
                and report["hosted"]["run_id"] == candidate["workflow_run_id"]
                and report["hosted"]["job_id"] == candidate["job_id"]
                and report["receipt"]["raw_qualification"]["sha256"] == candidate["raw_qualification_sha256"],
                "candidate host provenance identity")
        historical_inputs = {i["source_path"]: i["sha256"] for i in inputs}
        historical_shared_sha256 = reuse["current_checkout_sha256"]
        calendar_record = shared_manifest.get("calendar_cargo_rebind")
        if calendar_record is not None:
            path = "third-party/runtime/calendar-cargo-rebind.json"
            require(calendar_record["source_path"] == path, "Calendar rebind path")
            record_bytes = read(path)
            require(digest(record_bytes) == calendar_record["sha256"], "Calendar rebind identity")
            record = json.loads(record_bytes)
            require(record["schema_version"] == 2
                    and record["collector_host"] == "aarch64-apple-darwin"
                    and record["native_windows_at_current_inputs_verified"] is False
                    and record["frozen_windows_input_record"] == {"source_path": pin_path, "sha256": digest(pin)},
                    "Calendar rebind scope")
            require({i["source_path"]: i["sha256"] for i in shared_manifest["inputs"]} == historical_inputs,
                    "Calendar shared checkout inputs")
            updates = record["input_changes"]
            require(len(updates) == 3
                    and {i["source_path"] for i in updates} == {
                        "Cargo.lock", "crates/cueglint-providers/Cargo.toml", "crates/cueglint/Cargo.toml"},
                    "Calendar rebind input set")
            for item in updates:
                name = item["source_path"]
                require(historical_inputs[name] == item["current_sha256"]
                        and item["current_sha256"] != item["previous_sha256"]
                        and len(read(name)) == item["bytes"], "Calendar rebind current input identity")
                historical_inputs[name] = item["previous_sha256"]
            historical_shared_sha256 = record["previous_shared_manifest_sha256"]
            windows_projections = [row for row in record["comparison"]["projections"]
                                   if row["target"] == snapshot["target"] and row["edges"] == "normal,build"]
            require(windows_projections == [{"target": snapshot["target"], "edges": "normal,build",
                    "rows": len(rows), "base_normalized_sha256": normalized,
                    "current_normalized_sha256": normalized, "differences": 0}], "Calendar rebind graph identity")
        require(len(expected["inputs"]) == len(inputs)
                and {i["source_path"]: i["sha256"] for i in expected["inputs"]} == historical_inputs
                and expected["notice_manifest_sha256"] == historical_shared_sha256
                and expected["windows_notice_manifest_sha256"] ==
                    candidate["tested_windows_manifest_sha256"] == report["notice_manifest_sha256_at_run"],
                "candidate payload and build input scope")
        candidate_graph = report["native_graph"]
        require(candidate_graph["expected_graph_sha256"] == digest(payloads[graph_path])
                and candidate_graph["independent_row_count"] == candidate_graph["packages"] == len(rows)
                and candidate_graph["differences"] == 0
                and report["rust_library_notice"]["sha256"] == rust["candidate_notice_sha256"]
                and report["rust_library_notice"]["bytes"] == len(payloads[notice])
                and report["rust_installation_records"] == installation["installer_records"],
                "candidate native graph and Rust notice evidence")
    hexf_gap = next((gap for gap in gaps if gap["id"] == "missing-original-text:hexf-parse@0.2.1"), None)
    require(hexf_gap is not None, "missing hexf-parse omission")
    if not hexf_gap["blocks_bundle"]:
        disposition = hexf_gap["source_readiness_disposition"]
        hexf = by_id["hexf-parse@0.2.1"]
        publisher, instrument = hexf["notices"]
        require(disposition["standalone_original_still_missing"] is True
                and disposition["instrument_is_not_package_original"] is True
                and disposition["copyright_ownership_inferred"] is False
                and instrument["not_package_original"] is True
                and disposition["archive_sha256"] == hexf["source"]["archive_sha256"]
                and disposition["publisher_declaration_sha256"] == digest(payloads[publisher["file"]])
                and disposition["canonical_instrument_sha256"] == digest(payloads[instrument["file"]]),
                "hexf-parse scoped source disposition")
    require(not manifest["bundle_ready"] or not blockers, "ready flag contradicts blockers")
    return {"validation": "passed", "manifest_sha256": digest(raw), "target": snapshot["target"],
            "packages": len(packages), "files": len(payloads), "payload_bytes": total,
            "bundle_ready": manifest["bundle_ready"], "blocking_gaps": blockers}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-ready", action="store_true")
    args = parser.parse_args()
    result = check()
    print(json.dumps(result, indent=2))
    if args.require_ready and (not result["bundle_ready"] or result["blocking_gaps"]):
        print("Windows source readiness is not adopted; packaging remains blocked.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyError, OSError, TypeError, ValueError) as error:
        print(json.dumps({"validation": "failed", "error": str(error)}), file=sys.stderr)
        sys.exit(1)
