#!/usr/bin/env python3
"""Hostile contract for the windowed corpus lane runner (V5b2d-1c).

The full 2^24 RUN executes as independent lanes, each replaying one
4-aligned ordinal window of the exact full manifest.  A lane reconstructs the
grant state its ordinal prefix leaves behind, so its fragments reassemble
byte-identically with the monolithic shard stream under any declared
budget, not only when the pregrant is spent; windows that violate the
packing alignment, the ordinal bounds, or the shard grammar never execute,
and contiguous lane windows must reassemble the exact monolithic
transcript, including its streaming accounting digest rebuilt from the lane
record fragments.
"""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from functools import cache
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

from semantic import replay as semantic_replay  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"

ARB_KIND = protocol.ComparatorKindV1.ARB


@cache
def _base_job() -> protocol.ProofJobV1:
    return protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())


@cache
def _comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(
        f"corpus-lane-manifest-{index}".encode("ascii") for index in range(10)
    )
    return protocol.ContentResolvedComparatorManifestV2.admit(
        protocol.ComparatorManifestV2(
            ARB_KIND,
            *(hashlib.sha256(content).digest() for content in contents),
        ),
        {
            hashlib.sha256(content).digest(): content for content in contents
        }.get,
    )


def _job_with_pregrant(
    ordinals: tuple[int, ...],
    pregrant: int,
    per_point_work: int | None = None,
) -> protocol.ProofJobV1:
    base = _base_job()
    arb, mpfi = base.policy.comparators
    arb_budget = protocol.ComparatorBudgetV1(
        arb.kind,
        arb.precision_ladder,
        arb.per_point_work if per_point_work is None else per_point_work,
        pregrant,
    )
    policy = protocol.ProofPolicyV1(
        base.policy.equality_release, (arb_budget, mpfi)
    )
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        protocol.ReducedDomainManifestV1.from_ordinals(ordinals),
        policy,
    )


def _monolithic_shards(
    job: protocol.ProofJobV1, shard_points: int
) -> tuple[corpus.ShardCorpusRunnerV1, tuple[corpus.ShardArtifactV1, ...]]:
    plan = corpus.shard_plan_v1(job.domain, shard_points)
    if type(plan) is not tuple:
        raise AssertionError(f"shard plan rejected: {plan!r}")
    runner = corpus.ShardCorpusRunnerV1(job, _comparator())
    shards = tuple(runner.run_shard(start, end) for start, end in plan)
    return runner, shards


class LaneValidationTests(unittest.TestCase):
    def test_lane_rejects_every_foreign_or_misaligned_coordinate(self) -> None:
        job = _job_with_pregrant(tuple(range(256)), 0)
        comparator = _comparator()
        cases = (
            # window start breaks the packing alignment
            dict(window_start=2, window_points=64, shard_points=32),
            # window width breaks the packing alignment
            dict(window_start=0, window_points=66, shard_points=32),
            # empty window
            dict(window_start=0, window_points=0, shard_points=32),
            # shard width not aligned
            dict(window_start=0, window_points=64, shard_points=30),
            # shard width does not divide the window
            dict(window_start=0, window_points=64, shard_points=48),
            # window escapes the sRGB8 ordinal space
            dict(
                window_start=protocol.OUTPUT_CARDINALITY_V1 - 32,
                window_points=64,
                shard_points=32,
            ),
            # negative start
            dict(window_start=-4, window_points=64, shard_points=32),
        )
        for case in cases:
            result = corpus.run_window_lane_v1(job, comparator, **case)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1, case)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT, case
            )

    def test_lane_rejects_foreign_job_and_comparator(self) -> None:
        job = _job_with_pregrant(tuple(range(256)), 0)
        result = corpus.run_window_lane_v1(
            job.domain, _comparator(), 0, 64, 32
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        result = corpus.run_window_lane_v1(job, job, 0, 64, 32)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)


class LaneWindowJobTests(unittest.TestCase):
    def test_lane_window_job_reconstructs_the_ordinal_prefix_grant(self) -> None:
        # A lane replays a window of the same run, so it must start from the
        # grant state the ordinal prefix left behind: every preceding point
        # owns its pregrant whether or not it spends it.
        job = _job_with_pregrant(tuple(range(256)), 12345, per_point_work=3)
        window_job = corpus.lane_window_job_v1(job, 64, 128, ARB_KIND)
        self.assertIs(type(window_job), protocol.ProofJobV1)
        arb, _ = window_job.policy.comparators
        self.assertEqual(arb.global_pregrant, 12345 - 3 * 64)

    def test_lane_prefix_counts_domain_points_not_ordinals(self) -> None:
        # The monolithic run charges one grant per domain point in iteration
        # order.  Counting the window's ordinal instead overcounts the prefix
        # on any domain with a gap and starves the lane against the very run
        # it replays: here the ordinal would consume the whole pregrant.
        ordinals = tuple(range(0, 128)) + tuple(range(65_792, 65_920))
        job = _job_with_pregrant(ordinals, 256, per_point_work=1)
        window_job = corpus.lane_window_job_v1(job, 65_792, 128, ARB_KIND)
        self.assertIs(type(window_job), protocol.ProofJobV1)
        arb, _ = window_job.policy.comparators
        self.assertEqual(arb.global_pregrant, 256 - 128)

    def test_lane_window_job_clamps_an_exhausted_prefix_to_zero(self) -> None:
        # A prefix longer than the pregrant leaves nothing behind, and the
        # remaining grant is a count, never a negative debt.
        job = _job_with_pregrant(tuple(range(256)), 32, per_point_work=1)
        window_job = corpus.lane_window_job_v1(job, 64, 128, ARB_KIND)
        self.assertIs(type(window_job), protocol.ProofJobV1)
        arb, _ = window_job.policy.comparators
        self.assertEqual(arb.global_pregrant, 0)

    def test_lane_window_job_binds_only_its_own_window_and_lane(self) -> None:
        job = _job_with_pregrant(tuple(range(256)), 12345)
        window_job = corpus.lane_window_job_v1(job, 64, 128, ARB_KIND)
        self.assertIs(type(window_job), protocol.ProofJobV1)
        self.assertEqual(window_job.domain.ranges, ((64, 192),))
        self.assertEqual(window_job.domain.point_count, 128)
        arb, mpfi = window_job.policy.comparators
        self.assertEqual(arb.per_point_work, job.policy.comparators[0].per_point_work)
        self.assertEqual(mpfi, job.policy.comparators[1])
        self.assertEqual(
            window_job.definition.definition_digest,
            job.definition.definition_digest,
        )
        self.assertNotEqual(window_job.identity, job.identity)

    def test_lane_window_job_rejects_a_window_outside_its_domain(self) -> None:
        # A lane claims byte-identity with the same window of the monolithic
        # run, so a window the run never visits has no counterpart to be
        # identical to.  Rejecting it up front also stops a full replay of
        # ordinals that assembly would discard afterwards.
        ordinals = tuple(range(0, 128)) + tuple(range(65_792, 65_920))
        job = _job_with_pregrant(ordinals, 256, per_point_work=1)
        for window_start, window_points in ((128, 64), (64, 128), (65_888, 64)):
            result = corpus.lane_window_job_v1(
                job, window_start, window_points, ARB_KIND
            )
            self.assertIs(
                type(result),
                corpus.ShardCorpusRejectedV1,
                (window_start, window_points),
            )
            self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)
        # The windows that do lie inside one range stay admissible.
        for window_start in (0, 64, 65_792, 65_856):
            self.assertIs(
                type(corpus.lane_window_job_v1(job, window_start, 64, ARB_KIND)),
                protocol.ProofJobV1,
                window_start,
            )

    def test_lane_window_job_rejects_foreign_coordinates(self) -> None:
        job = _job_with_pregrant(tuple(range(256)), 0)
        result = corpus.lane_window_job_v1(job.domain, 0, 64, ARB_KIND)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        result = corpus.lane_window_job_v1(job, 2, 64, ARB_KIND)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)


class LaneByteIdentityTests(unittest.TestCase):
    def test_lane_fragments_match_the_monolithic_stream_after_exhaustion(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        _, monolithic = _monolithic_shards(job, 8)
        lane = corpus.run_window_lane_v1(job, comparator, 64, 64, 8)
        self.assertIs(type(lane), corpus.WindowLaneArtifactV1)
        self.assertEqual(lane.window_start, 64)
        self.assertEqual(lane.window_points, 64)
        self.assertEqual(lane.shards, monolithic[8:])
        totals = [0, 0, 0, 0]
        witnesses = 0
        for shard in monolithic[8:]:
            for kind in range(4):
                totals[kind] += shard.counters[kind]
            witnesses += shard.witness_count
        self.assertEqual(lane.counters, tuple(totals))
        self.assertEqual(lane.witness_count, witnesses)
        self.assertEqual(len(lane.accounting_records), 64 * 17)

    def test_lane_window_accounting_digest_follows_the_window_grammar(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        lane = corpus.run_window_lane_v1(job, comparator, 32, 96, 8)
        self.assertIs(type(lane), corpus.WindowLaneArtifactV1)
        window_job = corpus.lane_window_job_v1(job, 32, 96, ARB_KIND)
        accounting = semantic_replay.accounting_prefix_v1(
            ARB_KIND, window_job, comparator.source_identity
        )
        accounting.update(lane.accounting_records)
        self.assertEqual(lane.window_accounting_digest, accounting.digest())

    def test_lane_fragments_match_the_monolithic_stream_under_a_spent_grant(
        self,
    ) -> None:
        # The full-domain proof requires shard/order independence, so a lane
        # must reassemble byte-identically in every grant regime — not only
        # when the pregrant is exhausted and no point can decide a boundary.
        # This job grants one branch per point, which is what the decision
        # procedure needs to resolve the near-black boundary points.
        granted = _job_with_pregrant(tuple(range(128)), 128, per_point_work=1)
        comparator = _comparator()
        _, monolithic = _monolithic_shards(granted, 8)
        # Anti-vacuity: the same window without a grant leaves points at
        # RESOURCE_LIMIT_REACHED, so this window really exercises the branch
        # an exhausted lane regime would starve.
        _, starved = _monolithic_shards(
            _job_with_pregrant(tuple(range(128)), 0, per_point_work=0), 8
        )
        self.assertGreater(sum(shard.counters[3] for shard in starved[:8]), 0)
        self.assertEqual(sum(shard.counters[3] for shard in monolithic), 0)
        # The window that actually spends grant is the one holding the
        # near-black boundary ordinals; an untouched window would pass even
        # under an exhausted lane regime.
        spending = corpus.run_window_lane_v1(granted, comparator, 0, 64, 8)
        self.assertIs(type(spending), corpus.WindowLaneArtifactV1)
        self.assertEqual(spending.shards, monolithic[:8])
        self.assertEqual(
            sum(spending.counters[kind] for kind in (2, 3)),
            0,
            "a granted budget must leave no unresolved point behind",
        )
        trailing = corpus.run_window_lane_v1(granted, comparator, 64, 64, 8)
        self.assertIs(type(trailing), corpus.WindowLaneArtifactV1)
        self.assertEqual(trailing.shards, monolithic[8:])


class LaneCoverReassemblyTests(unittest.TestCase):
    def test_contiguous_lane_windows_rebuild_the_monolithic_transcript(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        runner, monolithic = _monolithic_shards(job, 8)
        monolithic_transcript = corpus.assemble_transcript_from_shards_v1(
            job, comparator, monolithic, runner.accounting_digest
        )
        self.assertIs(type(monolithic_transcript), protocol.DecisionTranscriptV1)

        first = corpus.run_window_lane_v1(job, comparator, 0, 64, 8)
        second = corpus.run_window_lane_v1(job, comparator, 64, 64, 8)
        self.assertIs(type(first), corpus.WindowLaneArtifactV1)
        self.assertIs(type(second), corpus.WindowLaneArtifactV1)

        accounting = semantic_replay.accounting_prefix_v1(
            ARB_KIND, job, comparator.source_identity
        )
        accounting.update(first.accounting_records)
        accounting.update(second.accounting_records)
        assembled = corpus.assemble_transcript_from_shards_v1(
            job,
            comparator,
            first.shards + second.shards,
            accounting.digest(),
        )
        self.assertIs(type(assembled), protocol.DecisionTranscriptV1)
        self.assertEqual(assembled.encode(), monolithic_transcript.encode())
        self.assertEqual(assembled.identity, monolithic_transcript.identity)


class LaneCliTests(unittest.TestCase):
    def test_lane_cli_writes_the_wire_fragments_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            status = corpus_lane.main(
                [
                    "--window-start",
                    "0",
                    "--window-points",
                    "64",
                    "--shard-points",
                    "32",
                    "--out",
                    out,
                ]
            )
            self.assertEqual(status, 0)
            out_path = Path(out)
            manifest = json.loads((out_path / "lane-manifest.json").read_text())
            self.assertEqual(manifest["schema"], "corpus-lane-v2")
            self.assertEqual(manifest["window_start"], 0)
            self.assertEqual(manifest["window_points"], 64)
            self.assertEqual(manifest["shard_points"], 32)
            self.assertEqual(sum(manifest["counters"]), 64)
            self.assertEqual(len(manifest["shards"]), 2)
            for index, shard in enumerate(manifest["shards"]):
                decision = (out_path / shard["decision_file"]).read_bytes()
                witness = (out_path / shard["witness_file"]).read_bytes()
                self.assertEqual(
                    hashlib.sha256(decision).hexdigest(),
                    shard["decision_sha256"],
                    index,
                )
                self.assertEqual(
                    hashlib.sha256(witness).hexdigest(),
                    shard["witness_sha256"],
                    index,
                )
            records = (out_path / "lane-records.bin").read_bytes()
            self.assertEqual(len(records), 64 * 17)
            self.assertEqual(
                hashlib.sha256(records).hexdigest(),
                manifest["records_sha256"],
            )

    def test_lane_cli_rejects_invalid_coordinates_with_exit_64(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            invalid = (
                ["--window-start", "2", "--window-points", "64",
                 "--shard-points", "32", "--out", out],
                ["--window-start", "0", "--window-points", "66",
                 "--shard-points", "32", "--out", out],
                ["--window-start", "0", "--window-points", "64",
                 "--shard-points", "48", "--out", out],
                [
                    "--window-start",
                    str(protocol.OUTPUT_CARDINALITY_V1 - 32),
                    "--window-points",
                    "64",
                    "--shard-points",
                    "32",
                    "--out",
                    out,
                ],
            )
            for argv in invalid:
                self.assertEqual(corpus_lane.main(argv), 64, argv)


if __name__ == "__main__":
    unittest.main()
