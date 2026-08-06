#!/usr/bin/env python3
"""Lane assembly for the V5b2d full-domain RUN (V5b2d-1d).

Assembly is the inverse of the lane runner.  It admits each lane's wire
evidence against the exact job it was produced for, proves the lanes cover
the job's domain with no gap, no overlap and no duplicate, concatenates the
retained accounting records in ordinal order to rebuild the single streaming
accounting digest under the full-domain job prefix, and seals the full
transcript from the shard fragments.  sha256 is not composable, so the lanes
carry raw record bytes instead of per-lane digests; a correct cover is
byte-identical to the monolithic replay, and every cover violation, foreign
identity, corrupted record stream or tampered fragment is rejected.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

from semantic import replay as semantic_replay  # noqa: E402

ASSEMBLY_SCHEMA_V1 = "corpus-assembly-v1"
RECORD_BYTES_V1 = corpus_lane.RECORD_BYTES_V1


@dataclass(frozen=True)
class AdmittedLaneV1:
    """One admitted lane: wire fragments plus retained accounting records."""

    window_start: int
    window_points: int
    shards: tuple[corpus.ShardArtifactV1, ...]
    accounting_records: bytes

    def __post_init__(self) -> None:
        if (
            type(self.window_start) is not int
            or type(self.window_points) is not int
            or self.window_start < 0
            or self.window_points <= 0
            or self.window_start % corpus.CORPUS_SHARD_ALIGNMENT_V1 != 0
            or self.window_points % corpus.CORPUS_SHARD_ALIGNMENT_V1 != 0
            or self.window_start + self.window_points
            > protocol.OUTPUT_CARDINALITY_V1
        ):
            raise TypeError(
                "lane window is outside the packing-aligned ordinal grammar"
            )
        if type(self.shards) is not tuple or not self.shards:
            raise TypeError("lane must carry at least one shard artifact")
        cursor = self.window_start
        for shard in self.shards:
            if type(shard) is not corpus.ShardArtifactV1:
                raise TypeError("lane shards must be canonical artifacts")
            if shard.start_ordinal != cursor:
                raise TypeError("lane shards must be contiguous in ordinal order")
            cursor = shard.end_ordinal
        if cursor != self.window_start + self.window_points:
            raise TypeError("lane shards must cover exactly the lane window")
        if (
            type(self.accounting_records) is not bytes
            or len(self.accounting_records) != RECORD_BYTES_V1 * self.window_points
        ):
            raise TypeError("lane records disagree with the lane window")
        for offset in range(0, len(self.accounting_records), RECORD_BYTES_V1):
            ordinal = int.from_bytes(
                self.accounting_records[offset : offset + 4], "big"
            )
            if ordinal != self.window_start + offset // RECORD_BYTES_V1:
                raise TypeError("lane records drift from the lane ordinals")


def _reject(reason: corpus.ShardCorpusReasonV1, detail: str):
    return corpus.ShardCorpusRejectedV1(reason, detail)


def _parse_hex_identity(value) -> bytes | None:
    if type(value) is not str or len(value) != 64:
        return None
    try:
        return bytes.fromhex(value)
    except ValueError:
        return None


def load_lane_v1(
    directory: Path,
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
):
    """Admit one lane directory against the job it claims to serve."""

    if type(job) is not protocol.ProofJobV1:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane admission requires a canonical proof job",
        )
    if type(comparator) is not protocol.ContentResolvedComparatorManifestV2:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane admission requires a canonical comparator manifest",
        )
    manifest_path = directory / "lane-manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text("ascii"))
    except (OSError, ValueError):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"lane manifest is missing or corrupt in {directory}",
        )
    if type(manifest) is not dict:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane manifest must be a JSON object",
        )
    manifest_counters = manifest.get("counters")
    if type(manifest_counters) is not list or any(
        type(value) is not int for value in manifest_counters
    ):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane manifest counters must be a list of integers",
        )
    if manifest.get("schema") != corpus_lane.LANE_SCHEMA_V2:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign lane schema"
        )
    identities = {
        "job_identity": job.identity,
        "domain_identity": job.domain.identity,
        "policy_identity": job.policy.identity,
        "comparator_source_identity": comparator.source_identity,
    }
    for key, expected in identities.items():
        value = _parse_hex_identity(manifest.get(key))
        if value != expected:
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                f"lane manifest binds a foreign {key}",
            )
    window_start = manifest.get("window_start")
    window_points = manifest.get("window_points")
    if (
        type(window_start) is not int
        or type(window_points) is not int
        or window_start < 0
        or window_points <= 0
        or window_start % corpus.CORPUS_SHARD_ALIGNMENT_V1 != 0
        or window_points % corpus.CORPUS_SHARD_ALIGNMENT_V1 != 0
        or window_start + window_points > protocol.OUTPUT_CARDINALITY_V1
        or manifest.get("record_count") != window_points
        or manifest.get("record_bytes") != RECORD_BYTES_V1
    ):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane manifest carries invalid window coordinates",
        )

    try:
        records = (directory / "lane-records.bin").read_bytes()
    except OSError:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "lane records are missing"
        )
    if hashlib.sha256(records).hexdigest() != manifest.get("records_sha256"):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane records fail their manifest digest",
        )
    if len(records) != RECORD_BYTES_V1 * window_points:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane record length disagrees with the window",
        )
    for offset in range(0, len(records), RECORD_BYTES_V1):
        ordinal = int.from_bytes(records[offset : offset + 4], "big")
        if ordinal != window_start + offset // RECORD_BYTES_V1:
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "lane records drift from the lane ordinals",
            )

    window_job = corpus.lane_window_job_v1(
        job, window_start, window_points, comparator.manifest.kind
    )
    if type(window_job) is not protocol.ProofJobV1:
        return window_job
    window_accounting = semantic_replay.accounting_prefix_v1(
        comparator.manifest.kind, window_job, comparator.source_identity
    )
    window_accounting.update(records)
    if window_accounting.digest().hex() != manifest.get(
        "window_accounting_digest"
    ):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane records fail the window accounting digest",
        )

    shard_entries = manifest.get("shards")
    if type(shard_entries) is not list or not shard_entries:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "lane manifest has no shards"
        )
    shards: list[corpus.ShardArtifactV1] = []
    counters = [0, 0, 0, 0]
    witness_count = 0
    cursor = window_start
    for entry in shard_entries:
        if type(entry) is not dict:
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign shard entry"
            )
        try:
            decision = (directory / entry["decision_file"]).read_bytes()
            witness = (directory / entry["witness_file"]).read_bytes()
        except (KeyError, OSError, TypeError):
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "shard fragment is missing",
            )
        if hashlib.sha256(decision).hexdigest() != entry.get("decision_sha256"):
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "decision fragment fails its manifest digest",
            )
        if hashlib.sha256(witness).hexdigest() != entry.get("witness_sha256"):
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "witness fragment fails its manifest digest",
            )
        entry_counters = entry.get("counters")
        if (
            type(entry_counters) is not list
            or len(entry_counters) != 4
            or any(type(value) is not int for value in entry_counters)
            or type(entry.get("witness_count")) is not int
            or type(entry.get("start_ordinal")) is not int
            or type(entry.get("end_ordinal")) is not int
        ):
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "shard entry carries invalid metadata",
            )
        if entry["start_ordinal"] != cursor:
            return _reject(
                corpus.ShardCorpusReasonV1.SHARD_ORDER,
                "lane shards must be contiguous in ordinal order",
            )
        try:
            shard = corpus.ShardArtifactV1(
                entry["start_ordinal"],
                entry["end_ordinal"],
                decision,
                witness,
                tuple(entry_counters),
                entry["witness_count"],
            )
        except TypeError as error:
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, str(error)
            )
        shards.append(shard)
        for index in range(4):
            counters[index] += shard.counters[index]
        witness_count += shard.witness_count
        cursor = shard.end_ordinal
    if cursor != window_start + window_points:
        return _reject(
            corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            "lane shards stop before the window end",
        )
    if counters != list(manifest_counters) or witness_count != manifest.get(
        "witness_count"
    ):
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane totals disagree with the shard fragments",
        )
    try:
        return AdmittedLaneV1(window_start, window_points, tuple(shards), records)
    except TypeError as error:
        return _reject(corpus.ShardCorpusReasonV1.FOREIGN_INPUT, str(error))


def assemble_lanes_v1(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    lanes,
):
    """Seal the transcript of the job's domain from admitted lanes in order."""

    if type(job) is not protocol.ProofJobV1:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane assembly requires a canonical proof job",
        )
    if type(comparator) is not protocol.ContentResolvedComparatorManifestV2:
        return _reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane assembly requires a canonical comparator manifest",
        )
    lanes = tuple(lanes)
    if not lanes:
        return _reject(
            corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            "no lanes were admitted for the domain",
        )
    ranges = iter(job.domain.ranges)
    current_range = next(ranges, None)
    if current_range is None:
        return _reject(
            corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            "the job carries an empty domain",
        )
    cursor = current_range[0]
    shards: list[corpus.ShardArtifactV1] = []
    accounting = semantic_replay.accounting_prefix_v1(
        comparator.manifest.kind, job, comparator.source_identity
    )
    for lane in lanes:
        if type(lane) is not AdmittedLaneV1:
            return _reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                "lane assembly requires canonical admitted lanes",
            )
        if lane.window_start != cursor:
            if lane.window_start < cursor:
                return _reject(
                    corpus.ShardCorpusReasonV1.SHARD_ORDER,
                    "lanes overlap or arrive out of ordinal order",
                )
            return _reject(
                corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
                "the lane cover has a gap before the next lane",
            )
        if current_range is None:
            return _reject(
                corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
                "a lane starts beyond the covered domain end",
            )
        if lane.window_start + lane.window_points > current_range[1]:
            return _reject(
                corpus.ShardCorpusReasonV1.SHARD_ORDER,
                "a lane overruns its domain range",
            )
        accounting.update(lane.accounting_records)
        shards.extend(lane.shards)
        cursor += lane.window_points
        if cursor == current_range[1]:
            current_range = next(ranges, None)
            if current_range is not None:
                cursor = current_range[0]
    if current_range is not None:
        return _reject(
            corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            "the lane cover stops before the domain end",
        )
    return corpus.assemble_transcript_from_shards_v1(
        job, comparator, shards, accounting.digest()
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lanes-root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    job = corpus.full_domain_job_v1(
        protocol.ProofJobV1.parse(corpus_lane.FIXTURE_JOB_V1.read_bytes())
    )
    comparator = corpus_lane.lane_comparator_v1()

    if not args.lanes_root.is_dir():
        print(
            f"lanes root is not a directory: {args.lanes_root}", file=sys.stderr
        )
        return 64

    lane_dirs = sorted(
        path
        for path in args.lanes_root.iterdir()
        if path.is_dir() and (path / "lane-manifest.json").exists()
    )
    lanes = []
    for lane_dir in lane_dirs:
        lane = load_lane_v1(lane_dir, job, comparator)
        if type(lane) is not AdmittedLaneV1:
            print(f"lane rejected: {lane_dir.name} ({lane!r})", file=sys.stderr)
            return 64
        lanes.append(lane)
    lanes.sort(key=lambda lane: lane.window_start)

    transcript = assemble_lanes_v1(job, comparator, lanes)
    if type(transcript) is not protocol.DecisionTranscriptV1:
        print(
            f"lane assembly rejected: lanes={len(lanes)} ({transcript!r})",
            file=sys.stderr,
        )
        return 64

    args.out.mkdir(parents=True, exist_ok=True)
    receipt = {
        "schema": ASSEMBLY_SCHEMA_V1,
        "job_identity": job.identity.hex(),
        "domain_identity": job.domain.identity.hex(),
        "policy_identity": job.policy.identity.hex(),
        "comparator_source_identity": comparator.source_identity.hex(),
        "transcript_identity": transcript.identity.hex(),
        "accounting_digest": transcript.accounting_digest.hex(),
        "point_count": transcript.point_count,
        "counters": list(transcript.counters),
        "witness_count": transcript.witness_store.count,
        "lane_count": len(lanes),
        "lanes": [
            {"window_start": lane.window_start, "window_points": lane.window_points}
            for lane in lanes
        ],
    }
    (args.out / "assembly-receipt.json").write_bytes(
        json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode("ascii")
    )
    print(
        f"assembled lanes={len(lanes)} points={transcript.point_count} "
        f"counters={transcript.counters} "
        f"transcript={transcript.identity.hex()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
