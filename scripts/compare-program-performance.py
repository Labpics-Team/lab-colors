#!/usr/bin/env python3
"""Compare the public Program consumer using alternating processes."""
import argparse
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import re
import statistics
import subprocess
import time

from program_cpu_profile import summarize_profile

parser = argparse.ArgumentParser()
parser.add_argument("--harness", type=Path, required=True)
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--candidate", type=Path, required=True)
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
def tool_version(*command):
    return subprocess.check_output(command, text=True, timeout=15).strip()

def runtime_payload(root):
    # Count generated consumer code together, so moving a projection between
    # WASM and JavaScript cannot appear to reduce its delivered byte cost.
    paths = [root / "pkg/labcolors_bg.wasm", root / "pkg/labcolors.js", root / "index.js"]
    paths.extend(sorted((root / "program-wire").glob("*.js")))
    paths.extend(sorted((root / "pkg/snippets").rglob("*.js")))
    files = []
    for path in paths:
        content = path.read_bytes()
        files.append({"path": path.relative_to(root).as_posix(), "bytes": len(content),
                      "sha256": hashlib.sha256(content).hexdigest()})
    return {"files": files, "totalBytes": sum(item["bytes"] for item in files)}


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
    "runtimePayload": {label: runtime_payload(root) for label, root in roots.items()},
    "harnessSha256": hashlib.sha256(args.harness.read_bytes()).hexdigest(),
    "samplesAfterWarmup": 3,
    "rounds": 4,
}


def read_measurement(output, label, pattern, nodes):
    measurement = json.loads(output)
    expected = metadata["samplesAfterWarmup"]
    if (measurement["wasmSha256"] != metadata["wasmSha256"][label]
            or measurement["pattern"] != pattern or measurement["nodes"] != nodes
            or measurement["samples"] != expected or len(measurement["samplesAfterWarmup"]) != expected):
        raise ValueError("consumer output does not match the requested artifact and workload")
    cycles = [measurement["cold"], *measurement["samplesAfterWarmup"]]
    if len({cycle["checksum"] for cycle in cycles}) != 1:
        raise ValueError("consumer output differs between cold and warmed cycles")
    for cycle in cycles:
        if len(cycle["updatesMs"]) != 64 or not all(
                type(value) in (int, float) and math.isfinite(value) and value >= 0
                for value in (cycle["compileMs"], cycle["cycleMs"], *cycle["updatesMs"])):
            raise ValueError("consumer cycle has no updates or invalid timings")
    return measurement


(args.output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps({"environment": metadata}), flush=True)
observations = []
scenarios = [
    (nodes, ("repeated", "distinct", "shared", "grouped") + (("ring",) if nodes <= 256 else ()))
    for nodes in (8, 64, 256, 1024, 4096)
]
# Keep all original observations ahead of the connected scaling cases.
scenarios.extend([(1024, ("ring",)), (2048, ("ring",))])
deadline = time.monotonic() + args.budget
for nodes, patterns in scenarios:
    for pattern in patterns:
        for round_index in range(metadata["rounds"]):
            order = ("baseline", "candidate") if round_index % 2 == 0 else ("candidate", "baseline")
            for label in order:
                started = time.monotonic()
                command = ["node", str(args.harness), str(roots[label]), pattern, str(nodes),
                           str(metadata["samplesAfterWarmup"])]
                record = {"variant": label, "pattern": pattern, "nodes": nodes, "round": round_index}
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
                                record["measurement"] = read_measurement(process.stdout, label, pattern, nodes)
                                record["status"] = "completed"
                            except (ValueError, TypeError, KeyError, OverflowError) as error:
                                record.update(status="invalid-output", error=str(error))
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
                print(json.dumps({key: value for key, value in record.items()
                                  if key != "stdout" or "measurement" not in record}),
                      flush=True)

summary = []
for nodes, patterns in scenarios:
    for pattern in patterns:
        row = {"pattern": pattern, "nodes": nodes}
        matching = [r for r in observations if r["pattern"] == pattern and r["nodes"] == nodes]
        for label in roots:
            records = [r for r in matching if r["variant"] == label]
            row[label] = {"statuses": [r["status"] for r in records]}
            if len(records) == metadata["rounds"] and all(r["status"] == "completed" for r in records):
                measurements = [r["measurement"] for r in records]
                samples = [s for m in measurements for s in m["samplesAfterWarmup"]]
                row[label].update(
                    coldCycleMedianMs=statistics.median(m["cold"]["cycleMs"] for m in measurements),
                    compileMedianMs=statistics.median(s["compileMs"] for s in samples),
                    cycleMedianMs=statistics.median(s["cycleMs"] for s in samples),
                    updatesMedianMs=statistics.median(sum(s["updatesMs"]) for s in samples),
                    maxRssKiB=max(m["maxRssKiB"] for m in measurements),
                    wasmSha256=sorted({m["wasmSha256"] for m in measurements}),
                    checksums=sorted({s["checksum"] for s in samples}),
                )
        if all("compileMedianMs" in row[label] for label in roots):
            if (len(row["baseline"]["checksums"]) != 1
                    or row["baseline"]["checksums"] != row["candidate"]["checksums"]):
                raise SystemExit(f"output checksum changed: {pattern}/{nodes}")
            row["coldCycleSpeedup"] = row["baseline"]["coldCycleMedianMs"] / row["candidate"]["coldCycleMedianMs"]
            row["compileSpeedup"] = row["baseline"]["compileMedianMs"] / row["candidate"]["compileMedianMs"]
            row["cycleSpeedup"] = row["baseline"]["cycleMedianMs"] / row["candidate"]["cycleMedianMs"]
            row["updatesSpeedup"] = row["baseline"]["updatesMedianMs"] / row["candidate"]["updatesMedianMs"]
        summary.append(row)
(args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
# Keep partial diagnostics without reporting a successful comparison.
if any(record["status"] != "completed" for record in observations):
    raise SystemExit("incomplete consumer comparison: inspect retained observations")


if args.profile:
    # Instrumented processes never enter observations or the latency summary.
    # Cap diagnostics at one ordinary process budget, shared by all runs; short cases leave time for larger controls.
    memory_observer = Path(__file__).with_name("observe-program-memory.mjs").resolve(strict=True)
    profile_cases = [(pattern, nodes, label)
                     for pattern, nodes in (("repeated", 256), ("repeated", 1024), ("distinct", 1024),
                                            ("ring", 256), ("ring", 1024), ("ring", 2048))
                     for label in roots]
    profile_budget = min(args.timeout, args.budget)
    profile_root = (args.output / "profiles").resolve()
    profile_root.mkdir(parents=True, exist_ok=True)
    profiles = {
        "includedInLatencySummary": False,
        "samplingIntervalUs": 1000,
        "budgetSeconds": profile_budget,
        "processTimeoutSeconds": args.timeout,
        "harnessSha256": metadata["harnessSha256"],
        "memoryObserverSha256": hashlib.sha256(memory_observer.read_bytes()).hexdigest(),
        "observations": [],
    }
    profile_deadline = time.monotonic() + profile_budget
    for pattern, nodes, label in profile_cases:
        profile_path = profile_root / f"{pattern}-{nodes}-{label}.cpuprofile"
        profile_path.unlink(missing_ok=True)
        memory_path = profile_root / f"{pattern}-{nodes}-{label}.memory.json"
        memory_path.unlink(missing_ok=True)
        record = {
            "variant": label, "pattern": pattern, "nodes": nodes,
            "sourceRevision": args.baseline_revision if label == "baseline" else args.candidate_revision,
            "wasmSha256": metadata["wasmSha256"][label],
            "profileFile": profile_path.name,
            "memoryObserverSha256": profiles["memoryObserverSha256"],
        }
        command = ["node", "--cpu-prof", f"--cpu-prof-dir={profile_root}",
                   f"--cpu-prof-name={profile_path.name}", "--cpu-prof-interval=1000",
                   str(memory_observer), str(args.harness), str(roots[label]), pattern, str(nodes),
                   str(metadata["samplesAfterWarmup"])]
        started = time.monotonic()
        remaining = profile_deadline - started
        if remaining <= 0:
            record["status"] = "budget-exhausted"
        else:
            try:
                process = subprocess.run(
                    command, capture_output=True, text=True,
                    timeout=min(args.timeout, remaining), check=False,
                    env={**os.environ, "LABCOLORS_MEMORY_REPORT": str(memory_path)},
                )
                record.update(returncode=process.returncode, stdout=process.stdout, stderr=process.stderr)
                record["status"] = "failed"
                if process.returncode == 0:
                    try:
                        measurement = read_measurement(process.stdout, label, pattern, nodes)
                        checksums = [measurement["cold"]["checksum"]]
                        reference = next(row[label] for row in summary
                                         if row["pattern"] == pattern and row["nodes"] == nodes)
                        if checksums != reference["checksums"]:
                            raise ValueError("profiled consumer differs from the measured workload")
                        memory = json.loads(memory_path.read_text())
                        if (memory["phase"] != "after-consumer"
                                or type(memory["linearMemoryBytes"]) is not int
                                or memory["linearMemoryBytes"] <= 0
                                or not all(isinstance(memory[key], dict)
                                           for key in ("processMemory", "heap", "code"))):
                            raise ValueError("post-consumer memory diagnostic is invalid")
                        record["memory"] = memory
                        profile = json.loads(profile_path.read_text())
                        if not isinstance(profile.get("nodes"), list) or not profile["nodes"]:
                            raise ValueError("CPU profile contains no call tree")
                        record.update(status="completed", checksums=checksums,
                                      cpuSummary=summarize_profile(profile))
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
        print(json.dumps({"cpuProfile": {key: value for key, value in record.items()
                                         if key != "stdout" or record["status"] != "completed"}}),
              flush=True)
    if any(record["status"] != "completed" for record in profiles["observations"]):
        raise SystemExit("incomplete consumer profile: inspect retained diagnostics")
