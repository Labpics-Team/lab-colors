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
    "harnessSha256": hashlib.sha256(args.harness.read_bytes()).hexdigest(),
    "samplesAfterWarmup": 3,
    "rounds": 2,
}
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
        for round_index in range(2):
            order = ("baseline", "candidate") if round_index == 0 else ("candidate", "baseline")
            for label in order:
                started = time.monotonic()
                command = ["node", str(args.harness), str(roots[label]), pattern, str(nodes), "3"]
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
                                record["measurement"] = json.loads(process.stdout)
                                record["status"] = "completed"
                            except ValueError:
                                record["status"] = "invalid-output"
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
            if len(records) == 2 and all(r["status"] == "completed" for r in records):
                measurements = [r["measurement"] for r in records]
                samples = [s for m in measurements for s in m["samplesAfterWarmup"]]
                row[label].update(
                    compileMedianMs=statistics.median(s["compileMs"] for s in samples),
                    cycleMedianMs=statistics.median(s["cycleMs"] for s in samples),
                    updatesMedianMs=statistics.median(sum(s["updatesMs"]) for s in samples),
                    maxRssKiB=max(m["maxRssKiB"] for m in measurements),
                    wasmSha256=sorted({m["wasmSha256"] for m in measurements}),
                    checksums=sorted({s["checksum"] for s in samples}),
                )
        if all("compileMedianMs" in row[label] for label in roots):
            if row["baseline"]["checksums"] != row["candidate"]["checksums"]:
                raise SystemExit(f"output checksum changed: {pattern}/{nodes}")
            row["compileSpeedup"] = row["baseline"]["compileMedianMs"] / row["candidate"]["compileMedianMs"]
            row["cycleSpeedup"] = row["baseline"]["cycleMedianMs"] / row["candidate"]["cycleMedianMs"]
            row["updatesSpeedup"] = row["baseline"]["updatesMedianMs"] / row["candidate"]["updatesMedianMs"]
        summary.append(row)
(args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
# A timeout remains an incomplete observation, never a successful latency sample.
# Non-timeout consumer failures invalidate the comparison.
if any(record["status"] not in ("completed", "timeout", "budget-exhausted") for record in observations):
    raise SystemExit("consumer failure: inspect retained observations")


def summarize_profile(profile):
    """Суммирует интервалы CDP samples/timeDeltas; hitCount не задаёт время."""
    nodes = {node["id"]: node for node in profile["nodes"]}
    if not nodes or len(nodes) != len(profile["nodes"]):
        raise ValueError("CPU profile node IDs are empty or duplicated")
    root = profile["nodes"][0]["id"]
    parents, order = {}, []
    pending = [(root, None)]
    while pending:
        node_id, parent = pending.pop()
        if node_id not in nodes or node_id in parents:
            raise ValueError("CPU profile is not a rooted call tree")
        parents[node_id] = parent
        order.append(node_id)
        pending.extend((child, node_id) for child in nodes[node_id].get("children", []))
    if len(order) != len(nodes):
        raise ValueError("CPU profile contains unreachable nodes")

    samples, deltas = profile["samples"], profile["timeDeltas"]
    if not samples or len(samples) != len(deltas):
        raise ValueError("CPU profile sample intervals are missing or incomplete")
    self_us = dict.fromkeys(nodes, 0)
    for node_id, delta in zip(samples, deltas):
        if (type(node_id) is not int or node_id not in nodes
                or type(delta) is not int or delta < 0):
            raise ValueError("CPU profile sample interval is invalid")
        self_us[node_id] += delta

    sampled_us = sum(deltas)
    duration_us = profile["endTime"] - profile["startTime"]
    if not math.isfinite(duration_us) or sampled_us <= 0 or duration_us < sampled_us:
        raise ValueError("CPU profile sample intervals exceed its duration")

    inclusive_us = self_us.copy()
    for node_id in reversed(order):
        parent = parents[node_id]
        if parent is not None:
            inclusive_us[parent] += inclusive_us[node_id]

    def top(weights):
        # Это предел объёма лога; totals охватывают все узлы и отсчёты.
        ids = sorted((node_id for node_id in nodes if weights[node_id]),
                     key=lambda node_id: (-weights[node_id], node_id))[:20]
        return [{"id": node_id, "frame": nodes[node_id]["callFrame"],
                 "selfUs": self_us[node_id], "inclusiveUs": inclusive_us[node_id]}
                for node_id in ids]

    return {"nodeCount": len(nodes), "sampleCount": len(samples),
            "profileDurationUs": duration_us, "sampledTimeUs": sampled_us,
            "unattributedTailUs": duration_us - sampled_us,
            "topBySelf": top(self_us), "topByInclusive": top(inclusive_us)}

if args.profile:
    # Instrumented processes never enter observations or the latency summary.
    # Cap diagnostics at one ordinary process budget, shared equally by six runs.
    profile_cases = [(pattern, nodes, label)
                     for pattern, nodes in (("repeated", 1024), ("distinct", 1024), ("ring", 256))
                     for label in roots]
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
                        measurement = json.loads(process.stdout)
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
    if any(record["status"] not in ("completed", "timeout", "budget-exhausted")
           for record in profiles["observations"]):
        raise SystemExit("profiled consumer failure: inspect retained diagnostics")
