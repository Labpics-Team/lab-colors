#!/usr/bin/env python3
"""Compare the public Program consumer using alternating processes."""
import argparse
import hashlib
import io
import json
import math
import os
import platform
from pathlib import Path, PurePosixPath
import re
import stat
import statistics
import subprocess
import tarfile
import time


def verified_candidate(root, tarball, expected_digest, source_revision):
    """Bind an already extracted package to the caller's verified archive."""
    archive_bytes = tarball.read_bytes()
    if hashlib.sha256(archive_bytes).hexdigest() != expected_digest:
        raise ValueError("candidate tarball SHA-256 differs from the verified artifact")
    root = root.resolve(strict=True)
    members = {}
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as archive:
        seen = set()
        for member in archive.getmembers():
            parts = PurePosixPath(member.name).parts
            if (not parts or parts[0] != "package" or "\\" in member.name
                    or any(part in (".", "..") or ":" in part for part in parts)
                    or member.name.rstrip("/") != "/".join(parts)
                    or member.name.rstrip("/") in seen):
                raise ValueError("unsafe or duplicate candidate archive member")
            seen.add(member.name.rstrip("/"))
            if member.isdir():
                continue
            if not member.isfile() or len(parts) < 2:
                raise ValueError("candidate archive must contain only regular files and directories")
            relative = "/".join(parts[1:])
            path = root.joinpath(*parts[1:])
            if (not path.resolve(strict=True).is_relative_to(root)
                    or not stat.S_ISREG(path.lstat().st_mode)):
                raise ValueError("candidate archive member is not a regular extracted file")
            with archive.extractfile(member) as stream:
                packed = stream.read()
            if path.read_bytes() != packed:
                raise ValueError("extracted candidate differs from archive: " + relative)
            members[relative] = hashlib.sha256(packed).hexdigest()
    actual = set()
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValueError("extracted candidate contains a symbolic link")
        if path.is_file():
            actual.add(path.relative_to(root).as_posix())
    if actual != members.keys():
        raise ValueError("extracted candidate file inventory differs from archive")
    build = json.loads((root / "build-metadata.json").read_text())
    if build["sourceSha"] != source_revision:
        raise ValueError("packed build-metadata sourceSha differs from candidate source")
    runtime = [item for item in build["wasm"] if item["role"] == "runtime"]
    wasm = (root / "pkg/labcolors_bg.wasm").read_bytes()
    if (len(runtime) != 1 or runtime[0]["path"] != "pkg/labcolors_bg.wasm"
            or runtime[0]["bytes"] != len(wasm)
            or runtime[0]["sha256"] != hashlib.sha256(wasm).hexdigest()):
        raise ValueError("packed runtime WASM differs from build-metadata")
    return {
        "kind": "verified-npm-tarball", "sourceRevision": source_revision,
        "tarballSha256": expected_digest, "tarballBytes": len(archive_bytes),
        "buildMetadataSha256": members["build-metadata.json"],
        "packageFilesSha256": hashlib.sha256(json.dumps(members, sort_keys=True).encode()).hexdigest(),
    }


def measurement_output(stdout, pattern, nodes, wasm_digest, wasm_bytes, producer_revision):
    measurement = json.loads(stdout)
    if (measurement["pattern"] != pattern or type(measurement["nodes"]) is not int
            or measurement["nodes"] != nodes or type(measurement["samples"]) is not int
            or measurement["samples"] != 3 or measurement["wasmSha256"] != wasm_digest
            or type(measurement["wasmBytes"]) is not int or measurement["wasmBytes"] != wasm_bytes
            or measurement["producer"] != producer_revision):
        raise ValueError("consumer identity differs from the requested measurement")
    def finite(value):
        try:
            return type(value) in (int, float) and math.isfinite(value) and value >= 0
        except OverflowError:
            return False
    warm = measurement["samplesAfterWarmup"]
    if not isinstance(warm, list) or len(warm) != 3:
        raise ValueError("consumer omitted warm measurements")
    samples = [measurement["cold"], *warm]
    for sample in samples:
        updates = sample["updatesMs"]
        if (not finite(sample["compileMs"]) or not finite(sample["cycleMs"])
                or not isinstance(updates, list) or len(updates) != 64
                or not all(finite(value) for value in updates)
                or type(sample["checksum"]) is not int or not 0 <= sample["checksum"] <= 0xffffffff):
            raise ValueError("consumer timing or checksum payload is invalid")
    if (not finite(measurement["initializationMs"]) or not finite(measurement["maxRssKiB"])
            or len({sample["checksum"] for sample in samples}) != 1):
        raise ValueError("consumer initialization, resource or repeated outcome is invalid")
    return measurement


def summary_row(pattern, nodes, observations, labels):
    row = {"pattern": pattern, "nodes": nodes}
    if labels == ("candidate",):
        row["comparison"] = "candidate-only"
    matching = [r for r in observations if r["pattern"] == pattern and r["nodes"] == nodes]
    for label in labels:
        records = [r for r in matching if r["variant"] == label]
        row[label] = {"statuses": [r["status"] for r in records]}
        if len(records) == 2 and all(r["status"] == "completed" for r in records):
            measurements = [r["measurement"] for r in records]
            samples = [s for m in measurements for s in m["samplesAfterWarmup"]]
            row[label].update(
                compileMedianMs=statistics.median(s["compileMs"] for s in samples),
                cycleMedianMs=statistics.median(s["cycleMs"] for s in samples),
                maxRssKiB=max(m["maxRssKiB"] for m in measurements),
                wasmSha256=sorted({m["wasmSha256"] for m in measurements}),
                checksums=sorted({s["checksum"] for s in samples}),
            )
    if labels == ("baseline", "candidate") and all("compileMedianMs" in row[label] for label in labels):
        if row["baseline"]["checksums"] != row["candidate"]["checksums"]:
            row["comparisonError"] = f"output checksum changed: {pattern}/{nodes}"
        else:
            for operation in ("compile", "cycle"):
                median = operation + "MedianMs"
                if row["candidate"][median] > 0:
                    row[operation + "Speedup"] = row["baseline"][median] / row["candidate"][median]
    return row


parser = argparse.ArgumentParser()
parser.add_argument("--harness", type=Path, required=True)
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--candidate", type=Path, required=True)
parser.add_argument("--candidate-tarball", type=Path)
parser.add_argument("--candidate-tarball-sha256")
parser.add_argument("--baseline-revision", required=True)
parser.add_argument("--candidate-revision", required=True)
parser.add_argument("--head-revision", required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--timeout", type=int, default=180)
parser.add_argument("--budget", type=int, default=900)
parser.add_argument("--profile", action="store_true",
                    help="retain separate CPU profiles after the latency comparison")
args = parser.parse_args()
if args.timeout <= 0 or args.budget <= 0:
    parser.error("timeout and budget must be positive seconds")
if bool(args.candidate_tarball) != bool(args.candidate_tarball_sha256):
    parser.error("candidate tarball and its verified SHA-256 must be supplied together")
if args.candidate_tarball_sha256 and re.fullmatch(r"[0-9a-f]{64}", args.candidate_tarball_sha256) is None:
    parser.error("candidate tarball SHA-256 must be lowercase hexadecimal")
for revision in (args.baseline_revision, args.candidate_revision, args.head_revision):
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        parser.error("revisions must be full lowercase Git commit IDs")
args.harness = args.harness.resolve(strict=True)
if not args.harness.is_file():
    parser.error("harness must be a file")
for root in (args.baseline, args.candidate):
    for relative in ("index.js", "program-wire/abi-v1.js", "pkg/labcolors_bg.wasm"):
        if not (root / relative).is_file():
            parser.error(f"missing package file: {root / relative}")
args.output.mkdir(parents=True, exist_ok=True)
roots = {"baseline": args.baseline, "candidate": args.candidate}
package_inputs = {label: {"kind": "source-build", "sourceRevision": revision}
                  for label, revision in (("baseline", args.baseline_revision), ("candidate", args.candidate_revision))}
if args.candidate_tarball:
    try:
        package_inputs["candidate"] = verified_candidate(args.candidate, args.candidate_tarball,
                                                         args.candidate_tarball_sha256, args.candidate_revision)
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        parser.error(str(error))
    if not (args.candidate / "program-wire/abi-v3.js").is_file():
        parser.error("verified candidate omits the public V3 ABI")
def tool_version(*command):
    return subprocess.check_output(command, text=True, timeout=15).strip()

checkout_revision = tool_version("git", "rev-parse", "HEAD")
if checkout_revision != args.candidate_revision:
    raise SystemExit("candidate revision does not match the checked out source")
baseline_checkout_revision = tool_version("git", "-C", str(args.baseline), "rev-parse", "HEAD")
if baseline_checkout_revision != args.baseline_revision:
    raise SystemExit("baseline revision does not match the checked out source")
metadata = {
    "baselineRevision": args.baseline_revision,
    "candidateRevision": args.candidate_revision,
    "headRevision": args.head_revision,
    "checkoutRevision": checkout_revision,
    "baselineCheckoutRevision": baseline_checkout_revision,
    # LCEN producerRevision identifies the Core subtree, not the repository commit.
    "producerRevision": {label: tool_version("git", "rev-parse", revision + ":crates/labcolors-core")
                         for label, revision in (("baseline", args.baseline_revision), ("candidate", args.candidate_revision))},
    "host": platform.uname()._asdict(),
    "cpus": os.cpu_count(),
    "timeoutSeconds": args.timeout,
    "budgetSeconds": args.budget,
    "rustc": tool_version("rustc", "--version", "--verbose"),
    "cargo": tool_version("cargo", "--version"),
    "node": tool_version("node", "--version"),
    "npm": tool_version("npm", "--version"),
    "wasmSha256": {label: hashlib.sha256((root / "pkg/labcolors_bg.wasm").read_bytes()).hexdigest()
                   for label, root in roots.items()},
    "wasmBytes": {label: (root / "pkg/labcolors_bg.wasm").stat().st_size for label, root in roots.items()},
    "packageInputs": package_inputs,
    "harnessSha256": hashlib.sha256(args.harness.read_bytes()).hexdigest(),
    "samplesAfterWarmup": 3,
    "rounds": 2,
}
if any(re.fullmatch(r"[0-9a-f]{40}", revision) is None for revision in metadata["producerRevision"].values()):
    raise SystemExit("source Core subtree identity is not a canonical Git object ID")
observations = []
scenarios = {
    nodes: ("repeated", "distinct", "shared", "grouped") + (("ring",) if nodes <= 256 else ())
    for nodes in (8, 64, 256, 1024, 4096)
}
candidate_only = ("v3-gray", "v3-limit") if args.candidate_tarball else ()
if candidate_only:
    scenarios[1] = candidate_only
    metadata["candidateOnlyPatterns"] = list(candidate_only)
(args.output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps({"environment": metadata}), flush=True)

def variants(pattern):
    return ("candidate",) if pattern in candidate_only else tuple(roots)

deadline = time.monotonic() + args.budget
for nodes, patterns in scenarios.items():
    for pattern in patterns:
        for round_index in range(2):
            labels = variants(pattern)
            order = labels if round_index == 0 else labels[::-1]
            for label in order:
                started = time.monotonic()
                command = ["node", str(args.harness), str(roots[label]), pattern, str(nodes), "3"]
                record = {"variant": label, "pattern": pattern, "nodes": nodes, "round": round_index}
                if pattern in candidate_only:
                    record["comparison"] = "candidate-only"
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    record["status"] = "budget-exhausted"
                else:
                    try:
                        process = subprocess.run(command, capture_output=True, text=True,
                                                 timeout=min(args.timeout, remaining), check=False)
                        record.update(returncode=process.returncode, stdout=process.stdout, stderr=process.stderr)
                        if process.returncode == 0:
                            try:
                                record["measurement"] = measurement_output(process.stdout, pattern, nodes,
                                                                             metadata["wasmSha256"][label],
                                                                             metadata["wasmBytes"][label],
                                                                             metadata["producerRevision"][label])
                                record["status"] = "completed"
                            except (ValueError, KeyError, TypeError) as error:
                                record["status"] = "invalid-output"
                                record["error"] = str(error)
                        else:
                            record["status"] = "failed"
                    except subprocess.TimeoutExpired as error:
                        def decoded(value):
                            return value.decode(errors="replace") if isinstance(value, bytes) else (value or "")
                        record.update(status="timeout", stdout=decoded(error.stdout), stderr=decoded(error.stderr))
                record["elapsedSeconds"] = time.monotonic() - started
                observations.append(record)
                name = f"{pattern}-{nodes}-{round_index}-{label}.json"
                (args.output / name).write_text(json.dumps(record, indent=2) + "\n")
                logged = {key: record[key] for key in
                          ("variant", "pattern", "nodes", "round", "status", "elapsedSeconds")}
                if "measurement" in record:
                    logged["measurement"] = record["measurement"]
                print(json.dumps(logged), flush=True)

summary = []
comparison_errors = []
for nodes, patterns in scenarios.items():
    for pattern in patterns:
        row = summary_row(pattern, nodes, observations, variants(pattern))
        if "comparisonError" in row:
            comparison_errors.append(row["comparisonError"])
        summary.append(row)
(args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
# A timeout remains an incomplete observation, never a successful latency sample.
# Non-timeout consumer failures invalidate the comparison.
if comparison_errors or any(record["status"] not in ("completed", "timeout", "budget-exhausted") for record in observations):
    raise SystemExit("consumer failure: inspect retained observations")

if args.profile:
    # Instrumented processes never enter observations or the latency summary.
    # Cap diagnostics at one ordinary process budget, shared by all profile runs.
    profile_cases = [(pattern, nodes, label)
                     for pattern, nodes in (("repeated", 1024), ("distinct", 1024), ("ring", 256))
                     for label in roots]
    profile_cases.extend((pattern, 1, "candidate") for pattern in candidate_only)
    profile_budget = min(args.timeout, args.budget)
    profile_timeout = profile_budget / len(profile_cases)
    profile_root = (args.output / "profiles").resolve()
    profile_root.mkdir(parents=True, exist_ok=True)
    profiles = {
        "includedInLatencySummary": False,
        "samplingIntervalUs": 1000,
        "budgetSeconds": profile_budget,
        "processTimeoutSeconds": profile_timeout,
        "harnessSha256": metadata["harnessSha256"],
        "observations": [],
    }
    profile_deadline = time.monotonic() + profile_budget
    for pattern, nodes, label in profile_cases:
        profile_path = profile_root / f"{pattern}-{nodes}-{label}.cpuprofile"
        profile_path.unlink(missing_ok=True)
        record = {
            "variant": label, "pattern": pattern, "nodes": nodes,
            "sourceRevision": args.baseline_revision if label == "baseline" else args.candidate_revision,
            "wasmSha256": metadata["wasmSha256"][label],
            "profileFile": profile_path.name,
        }
        if pattern in candidate_only:
            record["comparison"] = "candidate-only"
        command = ["node", "--cpu-prof", f"--cpu-prof-dir={profile_root}",
                   f"--cpu-prof-name={profile_path.name}", "--cpu-prof-interval=1000",
                   str(args.harness), str(roots[label]), pattern, str(nodes), "3"]
        started = time.monotonic()
        remaining = profile_deadline - started
        if remaining <= 0:
            record["status"] = "budget-exhausted"
        else:
            try:
                process = subprocess.run(command, capture_output=True, text=True,
                                         timeout=min(profile_timeout, remaining), check=False)
                record.update(returncode=process.returncode, stdout=process.stdout, stderr=process.stderr)
                record["status"] = "failed"
                if process.returncode == 0:
                    try:
                        measurement = measurement_output(process.stdout, pattern, nodes,
                                                         record["wasmSha256"], metadata["wasmBytes"][label],
                                                         metadata["producerRevision"][label])
                        samples = [measurement["cold"], *measurement["samplesAfterWarmup"]]
                        checksums = sorted({sample["checksum"] for sample in samples})
                        reference = next(row[label] for row in summary
                                         if row["pattern"] == pattern and row["nodes"] == nodes)
                        if (measurement["wasmSha256"] != record["wasmSha256"]
                                or measurement["pattern"] != pattern or measurement["nodes"] != nodes
                                or measurement["samples"] != 3 or len(samples) != 4
                                or len(checksums) != 1
                                or ("checksums" in reference and checksums != reference["checksums"])):
                            raise ValueError("profiled consumer differs from the measured workload")
                        profile = json.loads(profile_path.read_text())
                        if not isinstance(profile.get("nodes"), list) or not profile["nodes"]:
                            raise ValueError("CPU profile contains no call tree")
                        record.update(status="completed", checksums=checksums)
                    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
                        record.update(status="invalid-output", error=str(error))
            except subprocess.TimeoutExpired as error:
                def decoded(value):
                    return value.decode(errors="replace") if isinstance(value, bytes) else (value or "")
                record.update(status="timeout", stdout=decoded(error.stdout), stderr=decoded(error.stderr))
        record["elapsedSeconds"] = time.monotonic() - started
        if profile_path.is_file():
            profile_bytes = profile_path.read_bytes()
            record.update(profileBytes=len(profile_bytes),
                          profileSha256=hashlib.sha256(profile_bytes).hexdigest())
        profiles["observations"].append(record)
        (profile_root / "index.json").write_text(json.dumps(profiles, indent=2) + "\n")
        print(json.dumps({"cpuProfile": {key: record[key] for key in
                                         ("variant", "pattern", "nodes", "status", "elapsedSeconds")}}),
              flush=True)
    if any(record["status"] not in ("completed", "timeout", "budget-exhausted")
           for record in profiles["observations"]):
        raise SystemExit("profiled consumer failure: inspect retained diagnostics")
