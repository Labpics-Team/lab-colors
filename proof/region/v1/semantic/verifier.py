"""Independent semantic replay of one engine transcript.

The verifier recomputes every region decision from immutable job bytes with
its own SSA interpretation and rigorous interval arithmetic.  It never reads
Arb or MPFI code and never compares one engine transcript against another.
"""

from __future__ import annotations

import region_proof_protocol as protocol

from semantic.receipt import (
    SemanticVerificationReceiptV1,
    SemanticVerificationRejectedV1,
)


def verify_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    transcript: protocol.DecisionTranscriptV1,
    run: protocol.RunClaimV1,
) -> SemanticVerificationReceiptV1 | SemanticVerificationRejectedV1:
    """Replay every transcript decision and seal a receipt on full success.

    The replay owns the mathematical conclusion: decision bits, witness
    digests, resource grants and the accounting digest must all reproduce
    from the job bytes under the bound comparator's digest grammars.  Any
    mismatch, contradiction, unresolved replay or foreign binding returns a
    typed rejection; a receipt is sealed only after the complete replay.
    """

    raise NotImplementedError("semantic replay not implemented")
