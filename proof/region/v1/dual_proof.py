#!/usr/bin/env python3
"""Join of the dual structural candidate with both source-bound provenance
receipts and the independent semantic verification receipts.

The structural candidate proves only agreement; each source-bound receipt
proves only provenance of one lane; each semantic receipt proves only one
transcript against the independent verifier.  Only the join binds all four
evidence chains into one sealed dual proof.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum


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


class DualProofReceiptV1:
    """The sealed join; minting belongs to `join_dual_proof_v1` alone."""

    def __new__(cls, *args: object, **kwargs: object) -> "DualProofReceiptV1":
        if kwargs.get("_token") is not _SEAL_TOKEN:
            raise TypeError("DualProofReceiptV1 is sealed by the dual proof join")
        return object.__new__(cls)

    def __init__(self, *args: object, **kwargs: object) -> None:
        if kwargs.get("_token") is not _SEAL_TOKEN:
            raise TypeError("DualProofReceiptV1 is sealed by the dual proof join")


def claim_spans_full_domain_v1(claim: object) -> bool:
    raise NotImplementedError("stub")


def join_dual_proof_v1(*inputs: object) -> object:
    raise NotImplementedError("stub")
