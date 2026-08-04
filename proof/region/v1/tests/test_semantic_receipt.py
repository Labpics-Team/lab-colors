#!/usr/bin/env python3
"""Hostile contract for the sealed semantic verification receipt V1."""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import region_proof_protocol as protocol  # noqa: E402

from region_proof_protocol import (  # noqa: E402
    ComparatorKindV1,
    ComparatorManifestV2,
    ContentResolvedComparatorManifestV2,
    DecisionTranscriptV1,
    DecisionV1,
    ProofJobV1,
    RunClaimV1,
)

from semantic import receipt as semantic_receipt  # noqa: E402
from semantic.receipt import (  # noqa: E402
    SemanticVerificationReceiptV1,
    SemanticVerificationReasonV1,
    SemanticVerificationRejectedV1,
    resolved_decision_digest_v1,
)


FIXTURES = ROOT / "fixtures"


def digest(label: int) -> bytes:
    return hashlib.sha256(f"semantic-receipt-test-{label}".encode("ascii")).digest()


SYNTHETIC_CONTENT = {
    digest(index): f"semantic-receipt-test-{index}".encode("ascii")
    for index in range(1_000)
}


def admit_manifest(kind: ComparatorKindV1, seed: int) -> ContentResolvedComparatorManifestV2:
    return ContentResolvedComparatorManifestV2.admit(
        ComparatorManifestV2(
            kind=kind,
            engine_release=digest(seed),
            upstream_source=digest(seed + 1),
            arithmetic_input_set=digest(seed + 2),
            wrapper_source=digest(seed + 3),
            evaluator_source=digest(seed + 4),
            build_identity=digest(seed + 5),
            operation_allowlist=digest(seed + 6),
            test_observation=digest(seed + 7),
            legal_file_set=digest(seed + 8),
            exclusions=digest(seed + 9),
        ),
        SYNTHETIC_CONTENT.get,
    )


def fixture_job() -> ProofJobV1:
    return ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())


def outside_transcript(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV2,
) -> DecisionTranscriptV1:
    return DecisionTranscriptV1.from_decisions(
        job,
        comparator,
        (DecisionV1.OUTSIDE for _ in range(job.domain.point_count)),
        (),
        digest(900),
    )


def run_claim(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV2,
    transcript: DecisionTranscriptV1,
) -> RunClaimV1:
    return RunClaimV1.for_transcript(
        job,
        comparator,
        transcript,
        digest(901),
        digest(902),
        digest(903),
    )


class ReceiptSealingTests(unittest.TestCase):
    def test_direct_construction_is_sealed(self) -> None:
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                digest(1), digest(2), digest(3), digest(4), digest(5)
            )
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                digest(1),
                digest(2),
                digest(3),
                digest(4),
                digest(5),
                _token=object(),
            )

    def test_seal_replays_coordinates_and_binds(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 100)
        transcript = outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)

        receipt = SemanticVerificationReceiptV1._seal(job, comparator, run, transcript)
        self.assertEqual(receipt.job_identity, job.identity)
        self.assertEqual(receipt.comparator_identity, comparator.identity)
        self.assertEqual(receipt.run_claim_identity, run.identity)
        self.assertEqual(receipt.transcript_identity, transcript.identity)
        self.assertEqual(
            receipt.decision_digest,
            resolved_decision_digest_v1(
                transcript.domain_identity, transcript.decision_bits
            ),
        )
        self.assertTrue(receipt.binds(job, comparator, run, transcript))
        self.assertEqual(len(receipt.encode()), 160)
        self.assertEqual(len(receipt.identity), 32)

        other_comparator = admit_manifest(ComparatorKindV1.ARB, 200)
        other_transcript = outside_transcript(job, other_comparator)
        other_run = run_claim(job, other_comparator, other_transcript)
        self.assertFalse(receipt.binds(job, other_comparator, other_run, other_transcript))
        self.assertFalse(receipt.binds(None, comparator, run, transcript))

    def test_receipt_is_immutable(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.MPFI, 300)
        transcript = outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)
        receipt = SemanticVerificationReceiptV1._seal(job, comparator, run, transcript)
        with self.assertRaises(AttributeError):
            receipt.job_identity = digest(6)

    def test_seal_rejects_noncanonical_digests(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 400)
        transcript = outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)
        receipt = SemanticVerificationReceiptV1._seal(job, comparator, run, transcript)
        token = semantic_receipt._RECEIPT_TOKEN
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                bytes(32),
                receipt.comparator_identity,
                receipt.run_claim_identity,
                receipt.transcript_identity,
                receipt.decision_digest,
                _token=token,
            )
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                receipt.job_identity,
                receipt.comparator_identity,
                receipt.run_claim_identity,
                receipt.transcript_identity,
                b"short",
                _token=token,
            )


class RejectionShapeTests(unittest.TestCase):
    def test_rejection_is_typed_and_validated(self) -> None:
        rejection = SemanticVerificationRejectedV1(
            SemanticVerificationReasonV1.DECISION_MISMATCH, 7, "sign disagrees"
        )
        self.assertEqual(rejection.reason, SemanticVerificationReasonV1.DECISION_MISMATCH)
        self.assertEqual(rejection.ordinal, 7)

        with self.assertRaises(TypeError):
            SemanticVerificationRejectedV1("decision_mismatch", 0, "foreign reason")
        with self.assertRaises(TypeError):
            SemanticVerificationRejectedV1(
                SemanticVerificationReasonV1.DECISION_MISMATCH, -1, "ordinal"
            )
        with self.assertRaises(TypeError):
            SemanticVerificationRejectedV1(
                SemanticVerificationReasonV1.DECISION_MISMATCH,
                protocol.OUTPUT_CARDINALITY_V1,
                "ordinal",
            )
        with self.assertRaises(TypeError):
            SemanticVerificationRejectedV1(
                SemanticVerificationReasonV1.DECISION_MISMATCH, 0, ""
            )

    def test_rejection_reasons_are_a_closed_sum(self) -> None:
        self.assertEqual(
            sorted(reason.value for reason in SemanticVerificationReasonV1),
            [
                "accounting_replay_mismatch",
                "decision_mismatch",
                "foreign_binding",
                "replay_unresolved",
                "resource_replay_mismatch",
                "witness_contradiction",
                "witness_replay_mismatch",
            ],
        )


if __name__ == "__main__":
    unittest.main()
