#!/usr/bin/env python3
"""Bounded shard probe over the exact full sRGB8 domain (V5b2d-1b).

The probe replays a bounded ordinal prefix of the full 2^24-point domain
through the sharded corpus runner and prints the wire evidence coordinates:
per-shard counters and witness counts, the streaming accounting digest, and a
throughput estimate for the full domain.  It never materialises more than one
shard of points at a time, so it exercises exactly the mechanism the full
RUN will use while staying bounded enough for a GitHub-hosted runner.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import time
from pathlib import Path

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"
DEFAULT_POINTS = 1 << 16
DEFAULT_SHARD_POINTS = 1 << 14
MAX_PROBE_POINTS = 1 << 18


def _probe_comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(
        f"corpus-probe-coordinate-{index}".encode("ascii") for index in range(10)
    )
    manifest = protocol.ComparatorManifestV2(
        protocol.ComparatorKindV1.ARB,
        *(hashlib.sha256(content).digest() for content in contents),
    )
    return protocol.ContentResolvedComparatorManifestV2.admit(
        manifest,
        {hashlib.sha256(content).digest(): content for content in contents}.get,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--points", type=int, default=DEFAULT_POINTS)
    parser.add_argument("--shard-points", type=int, default=DEFAULT_SHARD_POINTS)
    args = parser.parse_args(argv)
    points = args.points
    shard_points = args.shard_points
    if (
        points < corpus.CORPUS_SHARD_ALIGNMENT_V1
        or points > MAX_PROBE_POINTS
        or points % shard_points != 0
        or shard_points < corpus.CORPUS_SHARD_ALIGNMENT_V1
        or shard_points % corpus.CORPUS_SHARD_ALIGNMENT_V1 != 0
    ):
        print(
            f"probe coordinates are invalid: points={points} "
            f"shard_points={shard_points} "
            f"(points must be a multiple of shard_points, shard_points a "
            f"multiple of {corpus.CORPUS_SHARD_ALIGNMENT_V1}, and points "
            f"at most {MAX_PROBE_POINTS})",
            file=sys.stderr,
        )
        return 64

    job = corpus.full_domain_job_v1(protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes()))
    comparator = _probe_comparator()
    full_plan = corpus.shard_plan_v1(job.domain, shard_points)
    if type(full_plan) is not tuple:
        print(f"full-domain shard plan was rejected: {full_plan!r}", file=sys.stderr)
        return 1
    print(
        f"full-domain job identity={job.identity.hex()} "
        f"domain={job.domain.identity.hex()} "
        f"point_count={job.domain.point_count} "
        f"full shard plan windows={len(full_plan)} width={shard_points}"
    )

    runner = corpus.ShardCorpusRunnerV1(job, comparator)
    totals = [0, 0, 0, 0]
    witness_total = 0
    started = time.perf_counter()
    window_count = points // shard_points
    for index in range(window_count):
        start = index * shard_points
        shard = runner.run_shard(start, start + shard_points)
        for kind in range(4):
            totals[kind] += shard.counters[kind]
        witness_total += shard.witness_count
        print(
            f"shard {index + 1}/{window_count} [{shard.start_ordinal}, "
            f"{shard.end_ordinal}) counters={shard.counters} "
            f"witnesses={shard.witness_count} "
            f"decision_bytes={len(shard.decision_bits)} "
            f"witness_bytes={len(shard.witness_wire)}"
        )
    elapsed = time.perf_counter() - started
    if sum(totals) != points:
        print(
            f"probe counters disagree with the point count: {totals} != {points}",
            file=sys.stderr,
        )
        return 1
    print(
        f"probe replayed {points} points in {elapsed:.3f}s "
        f"({points / elapsed:.1f} points/s; full-domain estimate "
        f"{protocol.OUTPUT_CARDINALITY_V1 / (points / elapsed) / 3600:.1f} h)"
    )
    print(
        f"probe accounting digest={runner.accounting_digest.hex()} "
        f"counters={tuple(totals)} witnesses={witness_total}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
