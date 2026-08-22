#!/usr/bin/env python3
"""Laned assembly of the independent semantic verification receipt.

One sequential semantic replay of the exact full 2^24 point domain never
fits a single verification process, so the replay runs as packing-aligned
window lanes: each lane independently replays its window of the job's domain
from the grant state its ordinal prefix leaves behind (the exact lane
machinery the full-domain RUN uses).  Assembly admits the lanes through the same lane
admission as the corpus RUN, reassembles the monolithic transcript from the
lane fragments, and seals the one `SemanticVerificationReceiptV1` — the same
receipt the monolithic verifier seals — only when the reassembled
independent replay is byte-identical to the verified transcript.  Any gap,
overlap, reorder, drift or identity mismatch returns a typed rejection and
nothing ever seals.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import corpus
import corpus_assembly
import corpus_lane
import region_proof_protocol as protocol
from semantic import verifier as semantic_verifier
from semantic.receipt import (
    SemanticVerificationReasonV1,
    SemanticVerificationReceiptV1,
    SemanticVerificationRejectedV1,
)

VerificationAssemblyResultV1 = (
    SemanticVerificationReceiptV1 | SemanticVerificationRejectedV1
)

SEALED_RECEIPT_NAME_V1 = "semantic-verification-receipt.bin"


def _reject(
    reason: SemanticVerificationReasonV1, detail: str
) -> SemanticVerificationRejectedV1:
    return SemanticVerificationRejectedV1(reason, 0, detail)


def assemble_semantic_verification_v1(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    transcript: protocol.DecisionTranscriptV1,
    run: protocol.RunClaimV1,
    lanes: object,
) -> VerificationAssemblyResultV1:
    """Seal the semantic verification receipt from a complete lane cover.

    The lanes are the same admitted wire lanes the corpus RUN assembly uses:
    independently replayed windows of the job's domain.  The receipt seals
    exactly when their cover is exact and their reassembled replay binds the
    verified transcript byte for byte.
    """

    binding = semantic_verifier.bind_transcript_v1(job, comparator, transcript, run)
    if binding is not None:
        return binding
    try:
        lane_tuple = tuple(lanes)  # type: ignore[arg-type]
    except Exception:
        # The lane cover is a hostile boundary: any iterator failure —
        # not just a non-iterable — must land as the typed rejection.
        return _reject(
            SemanticVerificationReasonV1.INVALID_INPUT,
            "laned verification requires an iterable lane cover",
        )
    if not lane_tuple or any(
        type(lane) is not corpus_assembly.AdmittedLaneV1 for lane in lane_tuple
    ):
        return _reject(
            SemanticVerificationReasonV1.INVALID_INPUT,
            "laned verification requires canonical admitted lane evidence",
        )
    assembled = corpus_assembly.assemble_lanes_v1(job, comparator, lane_tuple)
    if type(assembled) is not protocol.DecisionTranscriptV1:
        reason = (
            SemanticVerificationReasonV1.INVALID_INPUT
            if assembled.reason is corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            else SemanticVerificationReasonV1.FOREIGN_BINDING
        )
        return _reject(reason, f"the lane cover does not assemble: {assembled.detail}")
    if assembled.identity != transcript.identity:
        return _reject(
            SemanticVerificationReasonV1.DECISION_MISMATCH,
            "the independent lane replay does not bind the verified transcript",
        )
    return SemanticVerificationReceiptV1._seal(job, comparator, run, transcript)


def main(argv: list[str] | None = None) -> int:
    """Seal the semantic verification receipt from disk evidence and lanes.

    Consumes exactly the wire surfaces the engine RUN and the verification
    lanes upload: the engine evidence directory (job, comparator bundle,
    transcript, run claim) and a root of lane artifact directories.  Any
    missing, corrupt, or unbound coordinate is a fail-closed exit before any
    receipt exists.
    """

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--lanes-root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        job = protocol.ProofJobV1.parse((args.evidence / "job.bin").read_bytes())
        comparator = corpus_lane.load_comparator_bundle_v1(
            args.evidence / "comparator-bundle"
        )
        transcript = protocol.DecisionTranscriptV1.parse(
            (args.evidence / "transcript.bin").read_bytes()
        )
        run = protocol.RunClaimV1.parse(
            (args.evidence / "run-claim.bin").read_bytes()
        )
    except (OSError, protocol.ProtocolErrorV1) as exc:
        print(f"engine evidence rejected: {exc}", file=sys.stderr)
        return 64

    if not args.lanes_root.is_dir():
        print(f"lanes root is not a directory: {args.lanes_root}", file=sys.stderr)
        return 64
    lane_dirs = sorted(
        path
        for path in args.lanes_root.iterdir()
        if path.is_dir() and (path / "lane-manifest.json").exists()
    )
    lanes = []
    for lane_dir in lane_dirs:
        lane = corpus_assembly.load_lane_v1(lane_dir, job, comparator)
        if type(lane) is not corpus_assembly.AdmittedLaneV1:
            print(f"lane rejected: {lane_dir.name} ({lane!r})", file=sys.stderr)
            return 64
        lanes.append(lane)
    lanes.sort(key=lambda lane: lane.window_start)

    result = assemble_semantic_verification_v1(job, comparator, transcript, run, lanes)
    if type(result) is not SemanticVerificationReceiptV1:
        print(f"semantic verification rejected: {result!r}", file=sys.stderr)
        return 64
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / SEALED_RECEIPT_NAME_V1).write_bytes(result.encode())
    print(
        f"sealed lanes={len(lanes)} points={transcript.point_count} "
        f"receipt={result.identity.hex()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
