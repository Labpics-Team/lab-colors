"""Sealed semantic verification evidence for one engine transcript.

A receipt certifies that the third verifier independently replayed every
decision of one transcript from immutable job bytes.  It does not compare
engine transcripts and it does not mint a dual proof.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from functools import cached_property

import region_proof_protocol as protocol


_RECEIPT_TOKEN = object()

RECEIPT_ID_LABEL_V1 = (
    b"labcolors.proof-region.semantic-verification-receipt.v1\0"
)
DECISION_DIGEST_DOMAIN_V1 = b"labcolors.proof-region.resolved-decisions.v1\0"


class SemanticVerificationReasonV1(StrEnum):
    FOREIGN_BINDING = "foreign_binding"
    DECISION_MISMATCH = "decision_mismatch"
    WITNESS_REPLAY_MISMATCH = "witness_replay_mismatch"
    WITNESS_CONTRADICTION = "witness_contradiction"
    RESOURCE_REPLAY_MISMATCH = "resource_replay_mismatch"
    ACCOUNTING_REPLAY_MISMATCH = "accounting_replay_mismatch"
    REPLAY_UNRESOLVED = "replay_unresolved"
    INVALID_INPUT = "invalid_input"


@dataclass(frozen=True)
class SemanticVerificationRejectedV1:
    reason: SemanticVerificationReasonV1
    ordinal: int
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not SemanticVerificationReasonV1:
            raise TypeError("rejection reason must be SemanticVerificationReasonV1")
        if (
            type(self.ordinal) is not int
            or self.ordinal < 0
            or self.ordinal >= protocol.OUTPUT_CARDINALITY_V1
        ):
            raise TypeError("rejection ordinal outside sRGB8")
        if type(self.detail) is not str or not self.detail:
            raise TypeError("rejection detail must be a nonempty string")


def resolved_decision_digest_v1(
    domain_identity: bytes,
    decision_bits: bytes,
) -> bytes:
    """Canonical resolved-decisions digest shared with dual comparison."""

    hasher = hashlib.sha256()
    hasher.update(DECISION_DIGEST_DOMAIN_V1)
    hasher.update(domain_identity)
    hasher.update(len(decision_bits).to_bytes(8, "big"))
    hasher.update(decision_bits)
    return hasher.digest()


class SemanticVerificationReceiptV1:
    """Verifier-sealed semantic evidence for one engine transcript."""

    job_identity: bytes
    comparator_identity: bytes
    run_claim_identity: bytes
    transcript_identity: bytes
    decision_digest: bytes

    def __new__(cls, *args, **kwargs):
        if kwargs.pop("_token", None) is not _RECEIPT_TOKEN:
            raise TypeError("SemanticVerificationReceiptV1 is verifier-sealed")
        return object.__new__(cls)

    def __init_subclass__(cls, **kwargs) -> None:
        # The seal belongs to exactly one type: a subclass would let foreign
        # code mint receipts through an inherited constructor.
        raise TypeError("SemanticVerificationReceiptV1 is final")

    def __init__(
        self,
        job_identity: bytes,
        comparator_identity: bytes,
        run_claim_identity: bytes,
        transcript_identity: bytes,
        decision_digest: bytes,
        *,
        _token: object = None,
    ) -> None:
        for name in (
            "job_identity",
            "comparator_identity",
            "run_claim_identity",
            "transcript_identity",
            "decision_digest",
        ):
            value = locals()[name]
            if type(value) is not bytes or len(value) != 32 or value == bytes(32):
                raise TypeError(f"semantic receipt coordinate {name} is not a digest")
        object.__setattr__(self, "job_identity", job_identity)
        object.__setattr__(self, "comparator_identity", comparator_identity)
        object.__setattr__(self, "run_claim_identity", run_claim_identity)
        object.__setattr__(self, "transcript_identity", transcript_identity)
        object.__setattr__(self, "decision_digest", decision_digest)

    def __setattr__(self, name: str, value: object) -> None:
        raise AttributeError("SemanticVerificationReceiptV1 is immutable")

    @classmethod
    def _seal(
        cls,
        job: protocol.ProofJobV1,
        comparator: protocol.ContentResolvedComparatorManifestV2,
        run: protocol.RunClaimV1,
        transcript: protocol.DecisionTranscriptV1,
    ) -> "SemanticVerificationReceiptV1":
        """Only the verifier may cross the seal after a complete replay."""

        return cls(
            job.identity,
            comparator.identity,
            run.identity,
            transcript.identity,
            resolved_decision_digest_v1(
                transcript.domain_identity,
                transcript.decision_bits,
            ),
            _token=_RECEIPT_TOKEN,
        )

    def encode(self) -> bytes:
        return b"".join(
            getattr(self, name)
            for name in (
                "job_identity",
                "comparator_identity",
                "run_claim_identity",
                "transcript_identity",
                "decision_digest",
            )
        )

    @cached_property
    def identity(self) -> bytes:
        encoded = self.encode()
        hasher = hashlib.sha256()
        hasher.update(RECEIPT_ID_LABEL_V1)
        hasher.update(len(encoded).to_bytes(8, "big"))
        hasher.update(encoded)
        return hasher.digest()

    def binds(
        self,
        job: protocol.ProofJobV1,
        comparator: protocol.ContentResolvedComparatorManifestV2,
        run: protocol.RunClaimV1,
        transcript: protocol.DecisionTranscriptV1,
    ) -> bool:
        """Replay every binding coordinate against live canonical objects."""

        if (
            type(job) is not protocol.ProofJobV1
            or type(comparator) is not protocol.ContentResolvedComparatorManifestV2
            or type(run) is not protocol.RunClaimV1
            or type(transcript) is not protocol.DecisionTranscriptV1
        ):
            return False
        return (
            self.job_identity == job.identity
            and self.comparator_identity == comparator.identity
            and self.run_claim_identity == run.identity
            and self.transcript_identity == transcript.identity
            and self.decision_digest
            == resolved_decision_digest_v1(
                transcript.domain_identity,
                transcript.decision_bits,
            )
        )
