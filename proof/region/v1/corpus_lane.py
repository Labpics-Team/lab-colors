#!/usr/bin/env python3
"""One independent lane of the V5b2d full-domain RUN (V5b2d-1c).

A lane replays exactly one packing-aligned ordinal window of the full 2^24
domain through the sharded corpus runner in the exhausted ordinal-prefix
grant regime and writes wire-only evidence to the output directory: one
decision-bit fragment and one witness fragment per shard, the raw accounting
record bytes, and one deterministic lane manifest.  Independent lanes over
contiguous windows concatenate into the exact monolithic shard stream, so
the full RUN can execute as parallel dispatch lanes and reassemble without
ever materialising 2^24 objects.

Invalid coordinates never execute: they exit 64 before any replay starts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"
DEFAULT_SHARD_POINTS = 1 << 14
LANE_SCHEMA_V1 = "corpus-lane-v1"
RECORD_BYTES_V1 = 17


def _lane_comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(
        f"corpus-lane-coordinate-{index}".encode("ascii") for index in range(10)
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
    parser.add_argument("--window-start", type=int, required=True)
    parser.add_argument("--window-points", type=int, required=True)
    parser.add_argument("--shard-points", type=int, default=DEFAULT_SHARD_POINTS)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    job = corpus.full_domain_job_v1(
        protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())
    )
    comparator = _lane_comparator()
    lane = corpus.run_window_lane_v1(
        job, comparator, args.window_start, args.window_points, args.shard_points
    )
    if type(lane) is not corpus.WindowLaneArtifactV1:
        print(
            f"lane window rejected: start={args.window_start} "
            f"points={args.window_points} shard_points={args.shard_points} "
            f"({lane!r})",
            file=sys.stderr,
        )
        return 64

    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    shard_entries = []
    for index, shard in enumerate(lane.shards):
        decision_file = f"shard-{index:05d}.decision.bin"
        witness_file = f"shard-{index:05d}.witness.bin"
        (out / decision_file).write_bytes(shard.decision_bits)
        (out / witness_file).write_bytes(shard.witness_wire)
        shard_entries.append(
            {
                "start_ordinal": shard.start_ordinal,
                "end_ordinal": shard.end_ordinal,
                "counters": list(shard.counters),
                "witness_count": shard.witness_count,
                "decision_file": decision_file,
                "witness_file": witness_file,
                "decision_sha256": hashlib.sha256(shard.decision_bits).hexdigest(),
                "witness_sha256": hashlib.sha256(shard.witness_wire).hexdigest(),
            }
        )
    (out / "lane-records.bin").write_bytes(lane.accounting_records)
    manifest = {
        "schema": LANE_SCHEMA_V1,
        "window_start": lane.window_start,
        "window_points": lane.window_points,
        "shard_points": args.shard_points,
        "job_identity": job.identity.hex(),
        "domain_identity": job.domain.identity.hex(),
        "policy_identity": job.policy.identity.hex(),
        "comparator_identity": comparator.identity.hex(),
        "counters": list(lane.counters),
        "witness_count": lane.witness_count,
        "record_count": lane.window_points,
        "record_bytes": RECORD_BYTES_V1,
        "records_sha256": hashlib.sha256(lane.accounting_records).hexdigest(),
        "window_accounting_digest": lane.window_accounting_digest.hex(),
        "shards": shard_entries,
    }
    (out / "lane-manifest.json").write_bytes(
        json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode("ascii")
    )
    print(
        f"lane [{lane.window_start}, {lane.window_start + lane.window_points}) "
        f"shards={len(lane.shards)} counters={lane.counters} "
        f"witnesses={lane.witness_count} "
        f"records={len(lane.accounting_records)}B "
        f"window_accounting_digest={lane.window_accounting_digest.hex()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
