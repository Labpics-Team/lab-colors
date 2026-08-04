#!/usr/bin/env python3
"""Join of the dual structural candidate with both source-bound provenance
receipts and the independent semantic verification receipts.

The structural candidate proves only agreement; each source-bound receipt
proves only provenance of one lane; each semantic receipt proves only one
transcript against the independent verifier.  Only the join binds all five
evidence chains into one sealed dual proof, and reduced-domain proofs never
span the full V1 domain, so no family mint can rest on them.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import final

import region_proof_protocol as protocol  # noqa: E402  (repo-local module)


DUAL_PROOF_RECEIPT_ID_LABEL_V1 = (
    b"labcolors.proof-region.dual-proof-receipt.v1\0"
)

_SEAL_TOKEN = object()


class DualProofRejectionReasonV1(StrEnum):
    FOREIGN_INPUT = "foreign_input"
    FOREIGN_BINDING = "foreign_binding"


@dataclass(frozen=True)
class DualProofRejectedV1:
    reason: DualProofRejectionReasonV1
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not DualProofRejectionReasonV1:
            raise TypeError("invalid dual proof rejection reason")
        if type(self.detail) is not str or not self.detail:
            raise TypeError("invalid dual proof rejection detail")


def _reject(reason: DualProofRejectionReasonV1, detail: str) -> DualProofRejectedV1:
    return DualProofRejectedV1(reason, detail)


def _lane_coordinates_v1(receipt: object) -> tuple[bytes, bytes, bytes, bytes]:
    """Comparator, run-claim, transcript and receipt identities of one lane.

    Both source-bound receipt types expose the same four evidence chains but
    keep the run claim on different surfaces, so the join reads each lane
    through one narrow helper instead of duplicating the branch everywhere.
    """

    from arb import receipt as arb_receipt
    from mpfi import receipt as mpfi_receipt

    if type(receipt) is arb_receipt.SourceBoundEvaluatorReceiptV1:
        run_claim = receipt.run_claim
    elif type(receipt) is mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1:
        run_claim = receipt.evidence.run_claim
    else:
        raise TypeError("lane coordinates require a source-bound receipt")
    return (
        receipt.comparator.identity,
        run_claim.identity,
        receipt.transcript.identity,
        receipt.identity,
    )


def _bound_v1(
    candidate: object,
    first_receipt: object,
    second_receipt: object,
    first_semantic: object,
    second_semantic: object,
) -> bool:
    """Every cross-chain coordinate must agree before the join may seal."""

    from semantic.receipt import SemanticVerificationReceiptV1

    if type(candidate) is not protocol.DualComparisonCandidateV1:
        return False
    if (
        type(first_semantic) is not SemanticVerificationReceiptV1
        or type(second_semantic) is not SemanticVerificationReceiptV1
    ):
        return False
    try:
        first_lane = _lane_coordinates_v1(first_receipt)
        second_lane = _lane_coordinates_v1(second_receipt)
    except TypeError:
        return False
    claim = candidate.claim
    if claim.comparator_identities != (first_lane[0], second_lane[0]):
        return False
    if claim.run_claim_identities != (first_lane[1], second_lane[1]):
        return False
    if claim.transcript_identities != (first_lane[2], second_lane[2]):
        return False
    for semantic, lane in ((first_semantic, first_lane), (second_semantic, second_lane)):
        if (
            semantic.job_identity != claim.job_identity
            or semantic.comparator_identity != lane[0]
            or semantic.run_claim_identity != lane[1]
            or semantic.transcript_identity != lane[2]
            or semantic.decision_digest != claim.decision_digest
        ):
            return False
    return (
        first_semantic.identity != second_semantic.identity
        and first_lane[3] != second_lane[3]
    )


def _canonical_lane_order_v1(first_receipt: object, second_receipt: object) -> bool:
    from arb import receipt as arb_receipt
    from mpfi import receipt as mpfi_receipt

    return type(first_receipt) is arb_receipt.SourceBoundEvaluatorReceiptV1 and type(
        second_receipt
    ) is mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1


@final
class DualProofReceiptV1:
    """The sealed join; minting belongs to `join_dual_proof_v1` alone."""

    claim: protocol.DualComparisonClaimV1
    arb_receipt: object
    mpfi_receipt: object
    first_semantic_receipt: object
    second_semantic_receipt: object
    full_domain: bool
    identity: bytes

    def __new__(cls, *args: object, **kwargs: object) -> "DualProofReceiptV1":
        if kwargs.get("_token") is not _SEAL_TOKEN:
            raise TypeError("DualProofReceiptV1 is sealed by the dual proof join")
        return object.__new__(cls)

    def __init_subclass__(cls, **kwargs: object) -> None:
        # The seal belongs to exactly one type: a subclass would let foreign
        # code mint receipts through an inherited constructor.
        raise TypeError("DualProofReceiptV1 is final")

    def __init__(
        self,
        candidate: object,
        arb_receipt: object,
        mpfi_receipt: object,
        first_semantic_receipt: object,
        second_semantic_receipt: object,
        *,
        _token: object = None,
    ) -> None:
        if _token is not _SEAL_TOKEN or not _bound_v1(
            candidate,
            arb_receipt,
            mpfi_receipt,
            first_semantic_receipt,
            second_semantic_receipt,
        ):
            raise TypeError("DualProofReceiptV1 is sealed by the dual proof join")
        claim = candidate.claim  # type: ignore[union-attr]
        encoded = b"".join(
            (
                claim.identity,
                _lane_coordinates_v1(arb_receipt)[3],
                _lane_coordinates_v1(mpfi_receipt)[3],
                first_semantic_receipt.identity,  # type: ignore[union-attr]
                second_semantic_receipt.identity,  # type: ignore[union-attr]
            )
        )
        hasher = hashlib.sha256()
        hasher.update(DUAL_PROOF_RECEIPT_ID_LABEL_V1)
        hasher.update(len(encoded).to_bytes(8, "big"))
        hasher.update(encoded)
        for name, value in (
            ("claim", claim),
            ("arb_receipt", arb_receipt),
            ("mpfi_receipt", mpfi_receipt),
            ("first_semantic_receipt", first_semantic_receipt),
            ("second_semantic_receipt", second_semantic_receipt),
            ("full_domain", _claim_spans_full_domain_v1(claim)),
            ("identity", hasher.digest()),
        ):
            object.__setattr__(self, name, value)

    def __setattr__(self, name: str, value: object) -> None:
        raise AttributeError("DualProofReceiptV1 is immutable")

    def __delattr__(self, name: str) -> None:
        raise AttributeError("DualProofReceiptV1 is immutable")

    def binds(
        self,
        candidate: object,
        arb_receipt: object,
        mpfi_receipt: object,
        first_semantic_receipt: object,
        second_semantic_receipt: object,
    ) -> bool:
        """Replay every binding coordinate against live canonical objects."""

        if not _bound_v1(
            candidate,
            arb_receipt,
            mpfi_receipt,
            first_semantic_receipt,
            second_semantic_receipt,
        ):
            return False
        candidate_claim = candidate.claim  # type: ignore[union-attr]
        return (
            candidate_claim.identity == self.claim.identity
            and _lane_coordinates_v1(arb_receipt)[3]
            == _lane_coordinates_v1(self.arb_receipt)[3]
            and _lane_coordinates_v1(mpfi_receipt)[3]
            == _lane_coordinates_v1(self.mpfi_receipt)[3]
            and first_semantic_receipt.identity  # type: ignore[union-attr]
            == self.first_semantic_receipt.identity
            and second_semantic_receipt.identity  # type: ignore[union-attr]
            == self.second_semantic_receipt.identity
        )


def _claim_spans_full_domain_v1(claim: protocol.DualComparisonClaimV1) -> bool:
    """Total predicate for an already canonical claim.

    The mint gate binds the exact full manifest's content identity: a raw
    claim keeps its domain identity as an unverified coordinate, so a bare
    `2^24` point count never authorizes a family mint on its own.
    """

    return (
        claim.domain_point_count == protocol.OUTPUT_CARDINALITY_V1
        and claim.domain_identity
        == protocol.exact_full_domain_manifest_v1().identity
    )


def claim_spans_full_domain_v1(claim: object) -> bool | DualProofRejectedV1:
    """Only an exact full-domain claim spans the complete V1 point space.

    A reduced-domain claim never authorizes a family mint: the join records
    the shortfall instead of hiding it. A noncanonical claim returns the
    typed rejection instead of panicking the public path.
    """

    if type(claim) is not protocol.DualComparisonClaimV1:
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_INPUT,
            "full-domain span requires a dual comparison claim",
        )
    return _claim_spans_full_domain_v1(claim)


def join_dual_proof_v1(*inputs: object) -> DualProofReceiptV1 | DualProofRejectedV1:
    """Seal exactly one dual proof from the complete five-chain admission."""

    if len(inputs) != 5:
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_INPUT,
            "the dual proof join requires candidate, two source-bound receipts"
            " and two semantic receipts",
        )
    candidate, first_receipt, second_receipt, first_semantic, second_semantic = inputs
    from semantic.receipt import SemanticVerificationReceiptV1

    if type(candidate) is not protocol.DualComparisonCandidateV1:
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_INPUT,
            "the structural candidate is not a canonical admission",
        )
    try:
        _lane_coordinates_v1(first_receipt)
        _lane_coordinates_v1(second_receipt)
    except TypeError:
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_INPUT,
            "source-bound lanes require canonical Arb and MPFI receipts",
        )
    if (
        type(first_semantic) is not SemanticVerificationReceiptV1
        or type(second_semantic) is not SemanticVerificationReceiptV1
    ):
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_INPUT,
            "semantic lanes require canonical verification receipts",
        )
    if not _bound_v1(
        candidate, first_receipt, second_receipt, first_semantic, second_semantic
    ):
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_BINDING,
            "the five evidence chains do not bind one dual proof",
        )
    if not _canonical_lane_order_v1(first_receipt, second_receipt):
        return _reject(
            DualProofRejectionReasonV1.FOREIGN_BINDING,
            "dual lane order is Arb then MPFI",
        )
    return DualProofReceiptV1(
        candidate,
        first_receipt,
        second_receipt,
        first_semantic,
        second_semantic,
        _token=_SEAL_TOKEN,
    )
