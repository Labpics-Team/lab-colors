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
parser.add_argument("--samples", type=int, default=3)
parser.add_argument("--warmups", type=int, default=1,
                    help="cycles before retained samples, including the cold cycle")
parser.add_argument("--frames", type=int, default=64)
parser.add_argument("--profile", action="store_true",
                    help="retain separate CPU profiles after the latency comparison")
args = parser.parse_args()
if args.timeout <= 0 or args.budget <= 0:
    parser.error("timeout and budget must be positive seconds")
if not 3 <= args.samples <= 100 or not 1 <= args.warmups <= (2**53 - 1 - args.samples):
    parser.error("samples must be 3..100 and warmups a positive safe integer")
if not 0 < args.frames <= 2**53 - 1 or args.frames % 4:
    parser.error("frames must be a positive safe integer divisible by four")
for revision in (args.baseline_revision, args.candidate_revision, args.head_revision):
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        parser.error("revisions must be full lowercase Git commit IDs")
args.harness = args.harness.resolve(strict=True)
if not args.harness.is_file():
    parser.error("harness must be a file")
protocol_source = args.harness.with_name("measurement-protocol.mjs")
if not protocol_source.is_file():
    parser.error("the harness measurement-protocol.mjs must be present")
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
    "npm": tool_version("npm.cmd" if os.name == "nt" else "npm", "--version"),
    "wasmSha256": {label: hashlib.sha256((root / "pkg/labcolors_bg.wasm").read_bytes()).hexdigest()
                   for label, root in roots.items()},
    "harnessSha256": hashlib.sha256(args.harness.read_bytes()).hexdigest(),
    "protocolSha256": hashlib.sha256(protocol_source.read_bytes()).hexdigest(),
    "runnerSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "measurementMode": "diagnostic",
    "stabilization": "unverified",
    "samplingUnit": "process",
    "aggregation": "median-of-paired-process-ratios",
    "samplesAfterWarmup": args.samples,
    "warmups": args.warmups,
    "frames": args.frames,
    "rounds": 2,
}
(args.output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")


def validate_measurement(measurement, pattern, nodes, label):
    expected = {"pattern": pattern, "nodes": nodes, "samples": args.samples,
                "warmups": args.warmups, "frames": args.frames,
                "wasmSha256": metadata["wasmSha256"][label],
                "measurementMode": "diagnostic", "stabilization": "unverified"}
    if any(measurement.get(key) != value for key, value in expected.items()):
        raise ValueError("consumer differs from the requested workload or artifact")
    warmup = measurement["warmupSamples"]
    retained = measurement["samplesAfterWarmup"]
    if len(warmup) != args.warmups - 1 or len(retained) != args.samples:
        raise ValueError("consumer omitted or added cycles")
    samples = [measurement["cold"], *warmup, *retained]
    for sample in samples:
        updates = sample["updatesMs"]
        if len(updates) != args.frames:
            raise ValueError("consumer omitted or added frames")
        if (any(type(value) not in (int, float) or not math.isfinite(value) or value < 0
                for value in [sample["compileMs"], sample["cycleMs"], *updates])
                or min(sample["compileMs"], sample["cycleMs"], sum(updates)) <= 0):
            raise ValueError("consumer returned invalid timing")
    if len({sample["checksum"] for sample in samples}) != 1:
        raise ValueError("consumer results changed between cycles")
    return samples


observations = []
scenarios = {
    nodes: ("repeated", "distinct", "shared", "grouped", "sparse") + (("ring",) if nodes <= 1024 else ())
    for nodes in (8, 64, 256, 1024, 4096)
}
scenarios[2048] = ("ring",)
scenarios[8] += ("complete",)
scenarios[16] = ("bipartite",)
deadline = time.monotonic() + args.budget
for nodes, patterns in scenarios.items():
    for pattern in patterns:
        for round_index in range(2):
            order = ("baseline", "candidate") if round_index == 0 else ("candidate", "baseline")
            for label in order:
                started = time.monotonic()
                command = ["node", str(args.harness), str(roots[label]), pattern, str(nodes),
                           str(args.samples), "auto", str(args.warmups), str(args.frames)]
                record = {"variant": label, "pattern": pattern, "nodes": nodes,
                          "round": round_index, "command": command}
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
                                validate_measurement(record["measurement"], pattern, nodes, label)
                                record["status"] = "completed"
                            except (ValueError, KeyError, TypeError, AttributeError) as error:
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
                print(json.dumps({key: record[key] for key in
                                  ("variant", "pattern", "nodes", "round", "status", "elapsedSeconds")}),
                      flush=True)

summary = []
for nodes, patterns in scenarios.items():
    for pattern in patterns:
        row = {"pattern": pattern, "nodes": nodes, "measurementMode": "diagnostic",
               "stabilization": "unverified", "samplingUnit": "process"}
        matching = [r for r in observations if r["pattern"] == pattern and r["nodes"] == nodes]
        for label in roots:
            records = [r for r in matching if r["variant"] == label]
            row[label] = {"statuses": [r["status"] for r in records]}
            if len(records) == 2 and all(r["status"] == "completed" for r in records):
                measurements = [r["measurement"] for r in records]
                process_medians = [{
                    "round": record["round"],
                    "compileMs": statistics.median(s["compileMs"] for s in record["measurement"]["samplesAfterWarmup"]),
                    "updatesMs": statistics.median(sum(s["updatesMs"]) for s in record["measurement"]["samplesAfterWarmup"]),
                    "cycleMs": statistics.median(s["cycleMs"] for s in record["measurement"]["samplesAfterWarmup"]),
                } for record in records]
                row[label].update(
                    processMedians=process_medians,
                    compileMedianMs=statistics.median(s["compileMs"] for s in process_medians),
                    updatesMedianMs=statistics.median(s["updatesMs"] for s in process_medians),
                    cycleMedianMs=statistics.median(s["cycleMs"] for s in process_medians),
                    coldCycleMedianMs=statistics.median(m["cold"]["cycleMs"] for m in measurements),
                    maxRssKiB=max(m["maxRssKiB"] for m in measurements),
                    wasmSha256=sorted({m["wasmSha256"] for m in measurements}),
                    checksums=sorted({m["cold"]["checksum"] for m in measurements}),
                )
        if all("compileMedianMs" in row[label] for label in roots):
            if row["baseline"]["checksums"] != row["candidate"]["checksums"]:
                raise SystemExit(f"output checksum changed: {pattern}/{nodes}")
            for metric in ("compile", "updates", "cycle"):
                by_side = {label: {p["round"]: p[f"{metric}Ms"] for p in row[label]["processMedians"]}
                           for label in roots}
                ratios = [by_side["baseline"][index] / by_side["candidate"][index] for index in range(2)]
                row[f"{metric}PairedSpeedups"] = ratios
                row[f"{metric}Speedup"] = statistics.median(ratios)
        summary.append(row)
(args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
# A timeout remains an incomplete observation, never a successful latency sample.
# Non-timeout consumer failures invalidate the comparison.
if any(record["status"] not in ("completed", "timeout", "budget-exhausted") for record in observations):
    raise SystemExit("consumer failure: inspect retained observations")

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
                   str(args.harness), str(roots[label]), pattern, str(nodes),
                   str(args.samples), "auto", str(args.warmups), str(args.frames)]
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
                        samples = validate_measurement(measurement, pattern, nodes, label)
                        checksums = sorted({sample["checksum"] for sample in samples})
                        reference = next(row[label] for row in summary
                                         if row["pattern"] == pattern and row["nodes"] == nodes)
                        if (measurement["wasmSha256"] != record["wasmSha256"]
                                or measurement["pattern"] != pattern or measurement["nodes"] != nodes
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
