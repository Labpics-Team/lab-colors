#!/usr/bin/env python3
"""Hostile contract for independent semantic replay of engine transcripts."""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from region_proof_protocol import (  # noqa: E402
    ComparatorKindV1,
    ComparatorManifestV2,
    ContentResolvedComparatorManifestV2,
    DecisionTranscriptV1,
    DecisionV1,
    ExactZeroSignalTraceV1,
    ProofJobV1,
    ReducedDomainManifestV1,
    RunClaimV1,
)

from semantic.receipt import (  # noqa: E402
    SemanticVerificationReceiptV1,
    SemanticVerificationReasonV1,
    SemanticVerificationRejectedV1,
)
from semantic.verifier import verify_transcript  # noqa: E402


FIXTURES = ROOT / "fixtures"


def digest(label: int) -> bytes:
    return hashlib.sha256(f"semantic-replay-test-{label}".encode("ascii")).digest()


SYNTHETIC_CONTENT = {
    digest(index): f"semantic-replay-test-{index}".encode("ascii")
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


def run_claim(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV2,
    transcript: DecisionTranscriptV1,
) -> RunClaimV1:
    return RunClaimV1.for_transcript(
        job,
        comparator,
        transcript,
        digest(801),
        digest(802),
        digest(803),
    )


def all_outside_transcript(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV2,
) -> DecisionTranscriptV1:
    return DecisionTranscriptV1.from_decisions(
        job,
        comparator,
        (DecisionV1.OUTSIDE for _ in range(job.domain.point_count)),
        (),
        digest(810),
    )


def all_inside_transcript(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV2,
) -> DecisionTranscriptV1:
    ordinals = tuple(job.domain.iter_ordinals())
    witnesses = tuple(
        ExactZeroSignalTraceV1(ordinal, digest(10_000 + position))
        for position, ordinal in enumerate(ordinals)
    )
    return DecisionTranscriptV1.from_decisions(
        job,
        comparator,
        (DecisionV1.INSIDE for _ in ordinals),
        witnesses,
        digest(811),
    )


class HostileReplayTests(unittest.TestCase):
    def test_two_identical_wrong_transcripts_both_fail_replay(self) -> None:
        # Independence means replay recomputes from job bytes: one wrong
        # transcript fails, and an identical copy fails exactly the same way.
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 500)
        transcript = all_outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)

        first = verify_transcript(job, comparator, transcript, run)
        second = verify_transcript(job, comparator, transcript, run)

        self.assertIsInstance(first, SemanticVerificationRejectedV1)
        self.assertIsInstance(second, SemanticVerificationRejectedV1)
        self.assertEqual(first.reason, second.reason)
        self.assertEqual(first.ordinal, second.ordinal)
        self.assertNotIsInstance(first, SemanticVerificationReceiptV1)

    def test_saturate_all_inside_transcript_fails_replay(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.MPFI, 600)
        transcript = all_inside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)

        result = verify_transcript(job, comparator, transcript, run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)

    def test_foreign_comparator_binding_is_rejected_before_replay(self) -> None:
        job = fixture_job()
        bound = admit_manifest(ComparatorKindV1.ARB, 700)
        foreign = admit_manifest(ComparatorKindV1.ARB, 750)
        transcript = all_outside_transcript(job, bound)
        run = run_claim(job, bound, transcript)

        result = verify_transcript(job, foreign, transcript, run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)

    def test_foreign_run_binding_is_rejected_before_replay(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 760)
        transcript = all_outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)
        foreign_run = RunClaimV1(
            job.identity,
            comparator.identity,
            digest(821),
            digest(822),
            digest(823),
            transcript.identity,
        )
        self.assertNotEqual(foreign_run.identity, run.identity)

        result = verify_transcript(job, comparator, transcript, foreign_run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)

    def test_foreign_job_binding_is_rejected_before_replay(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.MPFI, 770)
        transcript = all_outside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)
        foreign_domain = ReducedDomainManifestV1.from_ordinals((0,))
        foreign_job = ProofJobV1(
            job.definition,
            job.formula_spec,
            foreign_domain,
            job.policy,
        )
        self.assertNotEqual(foreign_job.identity, job.identity)

        result = verify_transcript(foreign_job, comparator, transcript, run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)


if __name__ == "__main__":
    unittest.main()
