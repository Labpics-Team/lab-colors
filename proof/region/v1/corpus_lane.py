#!/usr/bin/env python3
"""One independent lane of the V5b2d full-domain RUN (V5b2d-1c).

A lane replays exactly one packing-aligned ordinal window of the full 2^24
domain through the sharded corpus runner, starting from the grant state its
ordinal prefix leaves behind, and writes wire-only evidence to the output
directory: one
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
from dataclasses import fields
from pathlib import Path
from typing import NoReturn

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"
DEFAULT_SHARD_POINTS = 1 << 14
LANE_SCHEMA_V1 = "corpus-lane-v1"
RECORD_BYTES_V1 = 17


def lane_comparator_v1() -> protocol.ContentResolvedComparatorManifestV2:
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


BUNDLE_MANIFEST_NAME_V1 = "comparator-manifest-v2.bin"
BUNDLE_CONTENT_DIR_V1 = "content"


def _bundle_fail(detail: str) -> NoReturn:
    raise protocol.ProtocolErrorV1(
        "comparator-bundle-v1",
        0,
        protocol.ProtocolReasonV1.INVALID_MANIFEST,
        detail,
    )


def _manifest_coordinates(manifest: protocol.ComparatorManifestV2) -> dict[str, bytes]:
    return {
        field.name: getattr(manifest, field.name)
        for field in fields(manifest)
        if field.name != "kind"
    }


def write_comparator_bundle_v1(
    manifest: protocol.ComparatorManifestV2,
    contents: dict[bytes, bytes],
    out: Path,
) -> None:
    """Write the closed wire surface of one admitted comparator.

    The bundle is the canonical manifest encoding plus exactly one content
    file per manifest address, named by the address hex.  Nothing else may
    ship: a lane runner reconstructs the content-resolved comparator from
    these bytes alone.
    """

    if type(manifest) is not protocol.ComparatorManifestV2:
        _bundle_fail("a comparator bundle requires a canonical manifest")
    if type(contents) is not dict:
        _bundle_fail("a comparator bundle requires a digest-to-bytes content map")
    coordinates = _manifest_coordinates(manifest)
    if set(contents) != set(coordinates.values()):
        _bundle_fail("the content map must cover exactly the manifest addresses")
    for name, address in coordinates.items():
        content = contents.get(address)
        if type(content) is not bytes or hashlib.sha256(content).digest() != address:
            _bundle_fail(f"content for {name} does not resolve to its address")
    out.mkdir(parents=True, exist_ok=True)
    content_dir = out / BUNDLE_CONTENT_DIR_V1
    content_dir.mkdir(exist_ok=True)
    (out / BUNDLE_MANIFEST_NAME_V1).write_bytes(manifest.encode())
    for address in sorted(contents):
        (content_dir / address.hex()).write_bytes(contents[address])


def load_comparator_bundle_v1(
    directory: Path,
) -> protocol.ContentResolvedComparatorManifestV2:
    """Admit a comparator from its wire bundle, re-hashing every coordinate."""

    if not isinstance(directory, Path):
        _bundle_fail("a comparator bundle directory must be a Path")
    manifest_path = directory / BUNDLE_MANIFEST_NAME_V1
    if not manifest_path.is_file():
        _bundle_fail("the bundle carries no comparator manifest")
    manifest = protocol.ComparatorManifestV2.parse(manifest_path.read_bytes())
    content_dir = directory / BUNDLE_CONTENT_DIR_V1
    if not content_dir.is_dir():
        _bundle_fail("the bundle carries no content directory")
    contents: dict[bytes, bytes] = {}
    for path in sorted(content_dir.iterdir()):
        if not path.is_file():
            _bundle_fail(f"foreign content entry: {path.name}")
        try:
            address = bytes.fromhex(path.name)
        except ValueError:
            address = b""
        if len(address) != 32:
            _bundle_fail(f"content file is not named by a sha256 address: {path.name}")
        content = path.read_bytes()
        if hashlib.sha256(content).digest() != address:
            _bundle_fail(f"content does not hash to its address: {path.name}")
        contents[address] = content
    return protocol.ContentResolvedComparatorManifestV2.admit(manifest, contents.get)


def write_verification_evidence_v1(receipt: object, out: Path) -> None:
    """Write the verification lanes' wire evidence from a source-bound receipt.

    The evidence is exactly what the lane runner and the semantic assembly
    consume: the canonical job encoding the evaluator ran under, the
    comparator bundle rebuilt from the controller-derived comparator's
    retained preimage bytes, and the engine's sealed run coordinates — the
    decision transcript and the run claim that binds it.  Nothing is
    re-derived and no second source of truth is created — the receipt
    already carries every coordinate.
    """

    def fail(detail: str) -> NoReturn:
        raise protocol.ProtocolErrorV1(
            "verification-evidence-v1",
            0,
            protocol.ProtocolReasonV1.INVALID_MANIFEST,
            detail,
        )

    try:
        job = receipt.job  # type: ignore[union-attr]
        comparator = receipt.comparator  # type: ignore[union-attr]
        transcript = receipt.transcript  # type: ignore[union-attr]
        run_claim = receipt.run_claim  # type: ignore[union-attr]
        resolved = comparator.manifest
        preimages = comparator.preimages
    except AttributeError:
        fail("verification evidence requires a source-bound receipt carrying"
             " its job, controller-derived comparator, decision transcript,"
             " and run claim")
    if type(job) is not protocol.ProofJobV1:
        fail("verification evidence requires the canonical proof job the"
             " evaluator ran under")
    if type(resolved) is not protocol.ContentResolvedComparatorManifestV2:
        fail("verification evidence requires an admitted comparator manifest")
    if type(transcript) is not protocol.DecisionTranscriptV1:
        fail("verification evidence requires the engine's sealed decision"
             " transcript")
    if type(run_claim) is not protocol.RunClaimV1:
        fail("verification evidence requires the engine's sealed run claim")
    if (
        transcript.job_identity != job.identity
        or transcript.comparator_identity != resolved.identity
    ):
        fail("verification evidence transcript does not bind the receipt's"
             " job and comparator")
    if (
        run_claim.job_identity != job.identity
        or run_claim.comparator_identity != resolved.identity
        or run_claim.transcript_identity != transcript.identity
    ):
        fail("verification evidence run claim does not bind the receipt's"
             " job, comparator, and transcript")
    try:
        contents = {
            hashlib.sha256(getattr(preimages, field.name)).digest():
            getattr(preimages, field.name)
            for field in fields(preimages)
        }
    except (AttributeError, TypeError):
        fail("verification evidence requires the comparator's retained"
             " preimage coordinates")
    write_comparator_bundle_v1(resolved.manifest, contents, out / "comparator-bundle")
    (out / "job.bin").write_bytes(job.encode())
    (out / "transcript.bin").write_bytes(transcript.encode())
    (out / "run-claim.bin").write_bytes(run_claim.encode())


def write_lane_artifacts_v1(
    lane: corpus.WindowLaneArtifactV1,
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    shard_points: int,
    out: Path,
) -> dict:
    """Write the lane wire layout and return its manifest dict."""

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
        "shard_points": shard_points,
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
    return manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--window-start", type=int, required=True)
    parser.add_argument("--window-points", type=int, required=True)
    parser.add_argument("--shard-points", type=int, default=DEFAULT_SHARD_POINTS)
    parser.add_argument("--job", type=Path, default=None)
    parser.add_argument("--comparator-bundle", type=Path, default=None)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        base_job = protocol.ProofJobV1.parse(
            (args.job or FIXTURE_JOB_V1).read_bytes()
        )
        job = corpus.full_domain_job_v1(base_job)
        comparator = (
            load_comparator_bundle_v1(args.comparator_bundle)
            if args.comparator_bundle is not None
            else lane_comparator_v1()
        )
    except protocol.ProtocolErrorV1 as error:
        print(f"lane wire input rejected: {error}", file=sys.stderr)
        return 64
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

    write_lane_artifacts_v1(lane, job, comparator, args.shard_points, args.out)
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
