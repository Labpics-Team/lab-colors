#!/usr/bin/env python3
"""Laned assembly of the independent semantic verification receipt.

One sequential semantic replay of the exact full 2^24 point domain never
fits a single verification process, so the replay runs as packing-aligned
window lanes: each lane independently replays its window of the job's domain
in the exhausted ordinal-prefix grant regime (the exact lane machinery the
full-domain RUN uses).  Assembly admits the lanes through the same lane
admission as the corpus RUN, reassembles the monolithic transcript from the
lane fragments, and seals the one `SemanticVerificationReceiptV1` — the same
receipt the monolithic verifier seals — only when the reassembled
independent replay is byte-identical to the verified transcript.  Any gap,
overlap, reorder, drift or identity mismatch returns a typed rejection and
nothing ever seals.
"""

from __future__ import annotations

import corpus
import corpus_assembly
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
