#!/usr/bin/env python3
"""Compare the public Program consumer using alternating processes."""
import argparse
import hashlib
import json
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
observations = []
scenarios = {
    nodes: ("repeated", "distinct", "shared", "grouped") + (("ring",) if nodes <= 256 else ())
    for nodes in (8, 64, 256, 1024, 4096)
}
deadline = time.monotonic() + args.budget
for nodes, patterns in scenarios.items():
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
                print(json.dumps({key: record[key] for key in
                                  ("variant", "pattern", "nodes", "round", "status", "elapsedSeconds")}),
                      flush=True)

summary = []
for nodes, patterns in scenarios.items():
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
                    maxRssKiB=max(m["maxRssKiB"] for m in measurements),
                    wasmSha256=sorted({m["wasmSha256"] for m in measurements}),
                    checksums=sorted({s["checksum"] for s in samples}),
                )
        if all("compileMedianMs" in row[label] for label in roots):
            if row["baseline"]["checksums"] != row["candidate"]["checksums"]:
                raise SystemExit(f"output checksum changed: {pattern}/{nodes}")
            row["compileSpeedup"] = row["baseline"]["compileMedianMs"] / row["candidate"]["compileMedianMs"]
            row["cycleSpeedup"] = row["baseline"]["cycleMedianMs"] / row["candidate"]["cycleMedianMs"]
        summary.append(row)
(args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
# A timeout remains an incomplete observation, never a successful latency sample.
# Non-timeout consumer failures invalidate the comparison.
if any(record["status"] not in ("completed", "timeout", "budget-exhausted") for record in observations):
    raise SystemExit("consumer failure: inspect retained observations")
