#!/usr/bin/env python3
"""Hostile contract for the full-domain lane dispatch coordinator (V5b2d-1e).

The full 2^24 RUN executes as one independent dispatch per lane window of
the exact full manifest.  The coordinator must derive a deterministic lane
plan whose windows cover [0, 2^24) exactly — contiguous, packing-aligned,
no gaps, no overlaps — and must turn that plan into one `gh workflow run`
invocation per lane.  Any width that cannot produce an exact aligned cover
is a typed rejection before any dispatch exists, and a dry run must emit
the dispatch commands without touching the network.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FULL_DOMAIN = protocol.OUTPUT_CARDINALITY_V1
ALIGNMENT = corpus.CORPUS_SHARD_ALIGNMENT_V1


class LanePlanCoverTests(unittest.TestCase):
    def test_default_plan_covers_the_full_domain_exactly(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        self.assertIs(type(plan), tuple)
        self.assertEqual(len(plan), 256)
        cursor = 0
        for start, points in plan:
            self.assertEqual(start, cursor)
            self.assertEqual(points, 65536)
            self.assertEqual(start % ALIGNMENT, 0)
            self.assertEqual(points % ALIGNMENT, 0)
            cursor += points
        self.assertEqual(cursor, FULL_DOMAIN)

    def test_plan_follows_the_requested_lane_width(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(lane_width=1024, shard_width=256)
        self.assertIs(type(plan), tuple)
        self.assertEqual(len(plan), FULL_DOMAIN // 1024)
        self.assertEqual(plan[0], (0, 1024))
        self.assertEqual(plan[-1], (FULL_DOMAIN - 1024, 1024))
        self.assertEqual(sum(points for _, points in plan), FULL_DOMAIN)

    def test_plan_is_deterministic(self) -> None:
        first = corpus_dispatch.plan_json_v1(lane_width=2048, shard_width=512)
        second = corpus_dispatch.plan_json_v1(lane_width=2048, shard_width=512)
        self.assertEqual(first, second)
        decoded = json.loads(first)
        self.assertEqual(decoded["schema"], "corpus-dispatch-plan-v1")
        self.assertEqual(decoded["lane_width"], 2048)
        self.assertEqual(decoded["shard_width"], 512)
        self.assertEqual(decoded["lane_count"], FULL_DOMAIN // 2048)
        self.assertEqual(decoded["domain_points"], FULL_DOMAIN)
        self.assertEqual(len(decoded["lanes"]), decoded["lane_count"])

    def test_plan_rejects_every_width_that_cannot_cover_exactly(self) -> None:
        cases = (
            # zero / negative widths
            dict(lane_width=0, shard_width=16384),
            dict(lane_width=-65536, shard_width=16384),
            dict(lane_width=65536, shard_width=0),
            # breaks the packing alignment
            dict(lane_width=65534, shard_width=16384),
            dict(lane_width=65536, shard_width=16382),
            # shard width does not divide the lane width
            dict(lane_width=65536, shard_width=12288),
            # shard wider than the lane
            dict(lane_width=1024, shard_width=2048),
            # lane wider than the full domain
            dict(lane_width=FULL_DOMAIN * 2, shard_width=16384),
            # aligned but does not divide the domain, leaving a ragged tail
            dict(lane_width=12, shard_width=4),
            # foreign types
            dict(lane_width=65536.0, shard_width=16384),
            dict(lane_width="65536", shard_width=16384),
        )
        for case in cases:
            result = corpus_dispatch.lane_plan_v1(**case)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1, case)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT, case
            )


class DispatchCommandTests(unittest.TestCase):
    def test_dispatch_commands_cover_the_plan_exactly(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(lane_width=1024, shard_width=256)
        self.assertIs(type(plan), tuple)
        commands = corpus_dispatch.dispatch_commands_v1(plan, 256)
        self.assertEqual(len(commands), len(plan))
        for (start, points), command in zip(plan, commands):
            self.assertEqual(command[0], "gh")
            self.assertEqual(command[1], "workflow")
            self.assertEqual(command[2], "run")
            self.assertEqual(command[3], "full-domain-corpus.yml")
            self.assertIn(f"-f", command)
            joined = " ".join(command)
            self.assertIn(f"-f window_start={start}", joined)
            self.assertIn(f"-f window_points={points}", joined)
            self.assertIn(f"-f shard_points=256", joined)

    def test_dispatch_commands_reject_a_rejected_plan(self) -> None:
        result = corpus_dispatch.dispatch_commands_v1(
            corpus.ShardCorpusRejectedV1(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign"
            ),
            256,
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)


class DryRunTests(unittest.TestCase):
    def test_dry_run_never_touches_the_network(self) -> None:
        calls: list[list[str]] = []

        def boom(*args: object, **kwargs: object) -> None:
            calls.append([args, kwargs])
            raise AssertionError("dry run must not invoke subprocess")

        original = subprocess.run
        subprocess.run = boom  # type: ignore[assignment]
        try:
            with tempfile.TemporaryDirectory() as out:
                status = corpus_dispatch.main(
                    [
                        "--mode",
                        "dispatch",
                        "--lane-width",
                        "1024",
                        "--shard-width",
                        "256",
                        "--dry-run",
                        "--out",
                        out,
                    ]
                )
        finally:
            subprocess.run = original  # type: ignore[assignment]
        self.assertEqual(status, 0)
        self.assertEqual(calls, [])


class DispatchCliTests(unittest.TestCase):
    def test_plan_mode_writes_the_deterministic_plan(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            status = corpus_dispatch.main(
                [
                    "--mode",
                    "plan",
                    "--lane-width",
                    "2048",
                    "--shard-width",
                    "512",
                    "--out",
                    out,
                ]
            )
            self.assertEqual(status, 0)
            plan = json.loads((Path(out) / "dispatch-plan.json").read_text())
            self.assertEqual(plan["schema"], "corpus-dispatch-plan-v1")
            self.assertEqual(plan["lane_count"], FULL_DOMAIN // 2048)
            self.assertEqual(plan["lanes"][0], {"window_start": 0, "window_points": 2048})
            self.assertEqual(
                plan["lanes"][-1],
                {
                    "window_start": FULL_DOMAIN - 2048,
                    "window_points": 2048,
                },
            )

    def test_invalid_widths_exit_64_before_any_dispatch(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            invalid = (
                ["--mode", "plan", "--lane-width", "12", "--shard-width", "4",
                 "--out", out],
                ["--mode", "plan", "--lane-width", "65536", "--shard-width",
                 "12288", "--out", out],
                ["--mode", "dispatch", "--lane-width", "0", "--shard-width",
                 "256", "--dry-run", "--out", out],
            )
            for argv in invalid:
                self.assertEqual(corpus_dispatch.main(argv), 64, argv)
            self.assertEqual(list(Path(out).iterdir()), [])

    def test_unknown_mode_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            corpus_dispatch.main(["--mode", "launch"])


if __name__ == "__main__":
    unittest.main()
