#!/usr/bin/env python3
"""Exercise the actual comparison runner with independent synthetic processes."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "a" * 40
CANDIDATE = "b" * 40


class ProgramPerformanceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="program-performance-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.roots = {label: self.root / label for label in ("baseline", "candidate")}
        for label, root in self.roots.items():
            for name in ("index.js", "program-wire/abi-v1.js", "pkg/labcolors_bg.wasm"):
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(label)

    def compare(self, *, extra=(), mutate=None, timeout=False):
        output = self.root / "result"
        command = [str(ROOT / "scripts/compare-program-performance.py"),
                   "--harness", str(ROOT / "packages/colors/bench/program.mjs"),
                   "--baseline", str(self.roots["baseline"]),
                   "--candidate", str(self.roots["candidate"]),
                   "--baseline-revision", BASELINE, "--candidate-revision", CANDIDATE,
                   "--head-revision", CANDIDATE, "--output", str(output), *extra]
        calls = []
        seen = {}

        def version(arguments, **_):
            if arguments[0] == "git":
                return (BASELINE if "-C" in arguments else CANDIDATE) + "\n"
            return "fixture-version\n"

        def execute(arguments, **_):
            label = Path(arguments[-7]).name
            pattern, nodes = arguments[-6], int(arguments[-5])
            samples, warmups, frames = map(int, (arguments[-4], arguments[-2], arguments[-1]))
            calls.append((label, pattern, nodes, samples, warmups, frames))
            if timeout and len(calls) == 1:
                raise subprocess.TimeoutExpired(arguments, 1, output="partial")
            key = (label, pattern, nodes)
            index = seen.get(key, 0)
            seen[key] = index + 1
            traces = {"baseline": ([1, 100, 101], [2, 3, 4]),
                      "candidate": ([2, 4, 6], [90, 100, 110])}
            values = traces[label][index % 2]

            def cycle(value):
                return {"compileMs": value, "updatesMs": [value * 2 / frames] * frames,
                        "cycleMs": value * 4, "checksum": 42}

            measurement = {
                "pattern": pattern, "nodes": nodes, "samples": samples,
                "warmups": warmups, "frames": frames,
                "measurementMode": "diagnostic", "stabilization": "unverified",
                "wasmSha256": hashlib.sha256((self.roots[label] / "pkg/labcolors_bg.wasm").read_bytes()).hexdigest(),
                "cold": cycle(1000), "warmupSamples": [cycle(500) for _ in range(warmups - 1)],
                "samplesAfterWarmup": [cycle(values[i % 3]) for i in range(samples)],
                "maxRssKiB": 100,
            }
            if mutate is not None and len(calls) == 1:
                mutate(measurement)
            if "--cpu-prof" in arguments:
                profile_root = Path(next(arg.split("=", 1)[1] for arg in arguments if arg.startswith("--cpu-prof-dir=")))
                profile_name = next(arg.split("=", 1)[1] for arg in arguments if arg.startswith("--cpu-prof-name="))
                (profile_root / profile_name).write_text(json.dumps({"nodes": [{"id": 1}]}))
            return subprocess.CompletedProcess(arguments, 0, json.dumps(measurement), "")

        failure = None
        with patch.object(sys, "argv", command), patch("subprocess.check_output", version), \
                patch("subprocess.run", execute), contextlib.redirect_stdout(io.StringIO()):
            try:
                runpy.run_path(command[0], run_name="__main__")
            except SystemExit as error:
                failure = error.code
        return output, calls, failure

    def test_process_medians_and_paired_ratios_are_not_pooled_samples(self):
        output, calls, failure = self.compare(extra=("--warmups", "4", "--frames", "8", "--profile"))
        self.assertIsNone(failure)
        self.assertTrue(all(call[-3:] == (3, 4, 8) for call in calls))
        row = json.loads((output / "summary.json").read_text())[0]
        self.assertEqual(row["baseline"]["compileMedianMs"], 51.5)
        self.assertNotEqual(row["baseline"]["compileMedianMs"], 3.5)  # pooled median
        self.assertEqual(row["compilePairedSpeedups"], [25, .03])
        self.assertAlmostEqual(row["compileSpeedup"], 12.515)
        self.assertEqual(row["samplingUnit"], "process")
        environment = json.loads((output / "environment.json").read_text())
        self.assertEqual(environment["stabilization"], "unverified")
        raw = json.loads((output / "repeated-8-0-baseline.json").read_text())
        self.assertEqual(len(raw["measurement"]["warmupSamples"]), 3)
        profiles = json.loads((output / "profiles/index.json").read_text())
        self.assertEqual(len(profiles["observations"]), 6)
        self.assertTrue(all(p["status"] == "completed" for p in profiles["observations"]))

    def test_missing_warmup_invalidates_the_comparison(self):
        output, _, failure = self.compare(extra=("--warmups", "2"), mutate=lambda m: m["warmupSamples"].clear())
        self.assertIsNotNone(failure)
        row = json.loads((output / "summary.json").read_text())[0]
        self.assertNotIn("compileSpeedup", row)
        self.assertIn("invalid-output", row["baseline"]["statuses"])

    def test_wrong_frame_count_invalidates_the_comparison(self):
        output, _, failure = self.compare(mutate=lambda m: m["samplesAfterWarmup"][0]["updatesMs"].pop())
        self.assertIsNotNone(failure)
        row = json.loads((output / "summary.json").read_text())[0]
        self.assertNotIn("updatesSpeedup", row)

    def test_timeout_is_retained_without_a_speed_ratio(self):
        output, _, failure = self.compare(timeout=True)
        self.assertIsNone(failure)
        row = json.loads((output / "summary.json").read_text())[0]
        self.assertIn("timeout", row["baseline"]["statuses"])
        self.assertNotIn("cycleSpeedup", row)
        raw = json.loads((output / "repeated-8-0-baseline.json").read_text())
        self.assertEqual(raw["stdout"], "partial")


if __name__ == "__main__":
    unittest.main()
