#!/usr/bin/env python3
"""Hostile contract for the lane assembly of the full-domain RUN (V5b2d-1d).

The full 2^24 RUN executes as independent lanes; assembly is the inverse
operation.  It admits each lane's wire evidence against the exact full-domain
job, proves the lanes cover the domain with no gap, no overlap and no
duplicate, concatenates the retained accounting records in ordinal order to
rebuild the single streaming accounting digest under the full-domain job
prefix, and seals the full transcript from the shard fragments.  Any cover
violation, foreign identity, corrupted record stream or tampered fragment is
rejected; a correct cover is byte-identical to the monolithic replay.
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
import corpus_assembly  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"

ARB_KIND = protocol.ComparatorKindV1.ARB


@cache
def _base_job() -> protocol.ProofJobV1:
    return protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())


@cache
def _comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(
        f"corpus-assembly-manifest-{index}".encode("ascii") for index in range(10)
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
    ordinals: tuple[int, ...], pregrant: int
) -> protocol.ProofJobV1:
    base = _base_job()
    arb, mpfi = base.policy.comparators
    budget = protocol.ComparatorBudgetV1(
        arb.kind, arb.precision_ladder, arb.per_point_work, pregrant
    )
    policy = protocol.ProofPolicyV1(base.policy.equality_release, (budget, mpfi))
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        protocol.ReducedDomainManifestV1.from_ordinals(ordinals),
        policy,
    )


def _lane(job, comparator, start: int, points: int, shard_points: int, producer=None):
    """One lane artifact, optionally produced against a different job.

    Assembly admits lanes as files, so a hostile lane never has to come from
    the job it is presented against: `producer` builds it against a domain
    that does contain the window, exactly as a foreign or stale artifact
    would arrive from another run.
    """

    lane = corpus.run_window_lane_v1(
        producer if producer is not None else job, comparator, start, points, shard_points
    )
    if type(lane) is not corpus.WindowLaneArtifactV1:
        raise AssertionError(f"lane rejected: {lane!r}")
    return lane


def _admitted(lane) -> corpus_assembly.AdmittedLaneV1:
    return corpus_assembly.AdmittedLaneV1(
        lane.window_start, lane.window_points, lane.shards, lane.accounting_records
    )


def _write_lane_dir(
    root: Path,
    name: str,
    job,
    comparator,
    start: int,
    points: int,
    shard_points: int,
) -> Path:
    lane = _lane(job, comparator, start, points, shard_points)
    out = root / name
    corpus_lane.write_lane_artifacts_v1(lane, job, comparator, shard_points, out)
    return out


class AssemblyApiTests(unittest.TestCase):
    def test_lane_cover_assembly_is_byte_identical_to_the_monolith(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        plan = corpus.shard_plan_v1(job.domain, 8)
        self.assertIs(type(plan), tuple)
        runner = corpus.ShardCorpusRunnerV1(job, comparator)
        monolithic = tuple(runner.run_shard(start, end) for start, end in plan)
        monolithic_transcript = corpus.assemble_transcript_from_shards_v1(
            job, comparator, monolithic, runner.accounting_digest
        )
        self.assertIs(type(monolithic_transcript), protocol.DecisionTranscriptV1)

        lanes = (
            _admitted(_lane(job, comparator, 0, 64, 8)),
            _admitted(_lane(job, comparator, 64, 64, 8)),
        )
        assembled = corpus_assembly.assemble_lanes_v1(job, comparator, lanes)
        self.assertIs(type(assembled), protocol.DecisionTranscriptV1)
        self.assertEqual(assembled.encode(), monolithic_transcript.encode())
        self.assertEqual(assembled.identity, monolithic_transcript.identity)
        self.assertEqual(assembled.accounting_digest, runner.accounting_digest)

    def test_assembly_rejects_a_gap_in_the_lane_cover(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        lanes = (
            _admitted(_lane(job, comparator, 0, 64, 8)),
            _admitted(_lane(job, comparator, 96, 32, 8)),
        )
        result = corpus_assembly.assemble_lanes_v1(job, comparator, lanes)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_overlapping_and_duplicate_lanes(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        overlap = (
            _admitted(_lane(job, comparator, 0, 64, 8)),
            _admitted(_lane(job, comparator, 32, 96, 8)),
        )
        result = corpus_assembly.assemble_lanes_v1(job, comparator, overlap)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.SHARD_ORDER)
        duplicate = (
            _admitted(_lane(job, comparator, 0, 64, 8)),
            _admitted(_lane(job, comparator, 0, 64, 8)),
        )
        result = corpus_assembly.assemble_lanes_v1(job, comparator, duplicate)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.SHARD_ORDER)

    def test_assembly_rejects_a_lane_set_that_skips_the_domain_prefix(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        lanes = (
            _admitted(_lane(job, comparator, 64, 64, 8)),
            _admitted(_lane(job, comparator, 0, 64, 8)),
        )
        result = corpus_assembly.assemble_lanes_v1(job, comparator, lanes)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_a_lane_overrunning_its_domain_range(self) -> None:
        ordinals = tuple(range(64)) + tuple(range(128, 192))
        job = _job_with_pregrant(ordinals, 0)
        comparator = _comparator()
        self.assertEqual(job.domain.ranges, ((0, 64), (128, 192)))
        wide = _job_with_pregrant(tuple(range(192)), 0)
        lanes = (_admitted(_lane(job, comparator, 0, 128, 8, producer=wide)),)
        result = corpus_assembly.assemble_lanes_v1(job, comparator, lanes)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.SHARD_ORDER)

    def test_assembly_rejects_lanes_overrunning_the_domain(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        lanes = (
            _admitted(_lane(job, comparator, 0, 128, 8)),
            _admitted(
                _lane(
                    job, comparator, 128, 64, 8,
                    producer=_job_with_pregrant(tuple(range(192)), 0),
                )
            ),
        )
        result = corpus_assembly.assemble_lanes_v1(job, comparator, lanes)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_an_empty_lane_set(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        result = corpus_assembly.assemble_lanes_v1(job, _comparator(), ())
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_foreign_job_and_comparator(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        lanes = (_admitted(_lane(job, comparator, 0, 64, 8)),)
        self.assertIs(
            type(corpus_assembly.assemble_lanes_v1(job.domain, comparator, lanes)),
            corpus.ShardCorpusRejectedV1,
        )
        self.assertIs(
            type(corpus_assembly.assemble_lanes_v1(job, job, lanes)),
            corpus.ShardCorpusRejectedV1,
        )


class WireLoadTests(unittest.TestCase):
    def test_load_lane_admits_the_wire_layout(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        with tempfile.TemporaryDirectory() as tmp:
            out = _write_lane_dir(Path(tmp), "lane-a", job, comparator, 0, 64, 8)
            loaded = corpus_assembly.load_lane_v1(out, job, comparator)
            self.assertIs(type(loaded), corpus_assembly.AdmittedLaneV1)
            self.assertEqual(loaded.window_start, 0)
            self.assertEqual(loaded.window_points, 64)
            self.assertEqual(
                len(loaded.accounting_records),
                64 * corpus_assembly.RECORD_BYTES_V1,
            )
            self.assertEqual(loaded.shards, _lane(job, comparator, 0, 64, 8).shards)

    def test_load_lane_rejects_a_foreign_job_identity(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        with tempfile.TemporaryDirectory() as tmp:
            out = _write_lane_dir(Path(tmp), "lane-a", job, comparator, 0, 64, 8)
            manifest = json.loads((out / "lane-manifest.json").read_text())
            manifest["job_identity"] = "00" * 32
            (out / "lane-manifest.json").write_bytes(
                json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
            )
            result = corpus_assembly.load_lane_v1(out, job, comparator)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )

    def test_load_lane_rejects_corrupted_record_bytes(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        with tempfile.TemporaryDirectory() as tmp:
            out = _write_lane_dir(Path(tmp), "lane-a", job, comparator, 0, 64, 8)
            records = (out / "lane-records.bin").read_bytes()
            (out / "lane-records.bin").write_bytes(records[:-1])
            result = corpus_assembly.load_lane_v1(out, job, comparator)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_load_lane_rejects_a_tampered_record_ordinal(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        with tempfile.TemporaryDirectory() as tmp:
            out = _write_lane_dir(Path(tmp), "lane-a", job, comparator, 0, 64, 8)
            records = bytearray((out / "lane-records.bin").read_bytes())
            records[0:4] = (999).to_bytes(4, "big")
            records = bytes(records)
            (out / "lane-records.bin").write_bytes(records)
            manifest = json.loads((out / "lane-manifest.json").read_text())
            manifest["records_sha256"] = hashlib.sha256(records).hexdigest()
            (out / "lane-manifest.json").write_bytes(
                json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
            )
            result = corpus_assembly.load_lane_v1(out, job, comparator)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )

    def test_load_lane_rejects_swapped_shard_fragments(self) -> None:
        job = _job_with_pregrant(tuple(range(128)), 0)
        comparator = _comparator()
        with tempfile.TemporaryDirectory() as tmp:
            out = _write_lane_dir(Path(tmp), "lane-a", job, comparator, 0, 64, 32)
            first = (out / "shard-00000.decision.bin").read_bytes()
            second = (out / "shard-00001.decision.bin").read_bytes()
            self.assertNotEqual(first, second)
            (out / "shard-00000.decision.bin").write_bytes(second)
            (out / "shard-00001.decision.bin").write_bytes(first)
            result = corpus_assembly.load_lane_v1(out, job, comparator)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )


class AssemblyCliTests(unittest.TestCase):
    def test_assembly_cli_rejects_partial_cover_with_exit_64(self) -> None:
        full_job = corpus.full_domain_job_v1(_base_job())
        comparator = corpus_lane.lane_comparator_v1()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "lanes"
            _write_lane_dir(root, "lane-a", full_job, comparator, 0, 64, 32)
            _write_lane_dir(root, "lane-b", full_job, comparator, 64, 64, 32)
            with tempfile.TemporaryDirectory() as out:
                status = corpus_assembly.main(
                    ["--lanes-root", str(root), "--out", out]
                )
                self.assertEqual(status, 64)

    def test_load_lane_rejects_a_foreign_comparator_binding(self) -> None:
        full_job = corpus.full_domain_job_v1(_base_job())
        comparator = corpus_lane.lane_comparator_v1()
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = _write_lane_dir(
                Path(tmp), "lane-a", full_job, comparator, 0, 64, 32
            )
            manifest = json.loads((out_dir / "lane-manifest.json").read_text())
            manifest["comparator_identity"] = "11" * 32
            (out_dir / "lane-manifest.json").write_bytes(
                json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
            )
            result = corpus_assembly.load_lane_v1(out_dir, full_job, comparator)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )

    def test_assembly_cli_rejects_a_missing_lanes_root_with_exit_64(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            status = corpus_assembly.main(
                [
                    "--lanes-root",
                    str(Path(tmp) / "missing"),
                    "--out",
                    str(Path(tmp) / "out"),
                ]
            )
            self.assertEqual(status, 64)


if __name__ == "__main__":
    unittest.main()
