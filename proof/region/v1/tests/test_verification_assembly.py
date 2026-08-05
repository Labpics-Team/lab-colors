#!/usr/bin/env python3
"""Hostile contract for laned semantic verification.

A full-domain transcript can only be verified by independent replay, and one
sequential replay of 2^24 points never fits a single verification process.
The laned path therefore splits the domain into packing-aligned windows,
replays each window independently in the exhausted ordinal-prefix grant
regime, and seals exactly one `SemanticVerificationReceiptV1` — the same
receipt the monolithic verifier seals — only when the lane-assembled replay
is byte-identical to the verified transcript.  No lane cover, no receipt.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
import verification_assembly  # noqa: E402
from semantic.verifier import verify_transcript  # noqa: E402

SHARD_POINTS = 16
WINDOW_POINTS = 32
DOMAIN_ORDINALS = tuple(range(2 * WINDOW_POINTS))


def digest(label: int) -> bytes:
    return hashlib.sha256(f"verification-assembly-{label}".encode("ascii")).digest()


def _exhausted_policy(policy: protocol.ProofPolicyV1) -> protocol.ProofPolicyV1:
    return protocol.ProofPolicyV1(
        policy.equality_release,
        tuple(
            protocol.ComparatorBudgetV1(
                budget.kind, budget.precision_ladder, budget.per_point_work, 0
            )
            for budget in policy.comparators
        ),
    )


def test_job() -> protocol.ProofJobV1:
    base = protocol.ProofJobV1.parse(corpus_lane.FIXTURE_JOB_V1.read_bytes())
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        protocol.ReducedDomainManifestV1.from_ordinals(DOMAIN_ORDINALS),
        _exhausted_policy(base.policy),
    )


def honest_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
) -> protocol.DecisionTranscriptV1:
    runner = corpus.ShardCorpusRunnerV1(job, comparator)
    shards = [runner.run_shard(start, end) for start, end in corpus.shard_plan_v1(
        job.domain, SHARD_POINTS
    )]
    assembled = corpus.assemble_transcript_from_shards_v1(
        job, comparator, shards, runner.accounting_digest
    )
    if type(assembled) is not protocol.DecisionTranscriptV1:
        raise AssertionError(f"honest transcript did not assemble: {assembled!r}")
    return assembled


def run_claim(transcript: protocol.DecisionTranscriptV1) -> protocol.RunClaimV1:
    return protocol.RunClaimV1.for_transcript(
        test_job(), comparator(), transcript, digest(1), digest(2), digest(3)
    )


def comparator() -> protocol.ContentResolvedComparatorManifestV2:
    return corpus_lane.lane_comparator_v1()


def lanes(
    job: protocol.ProofJobV1,
    comparator_manifest: protocol.ContentResolvedComparatorManifestV2,
) -> tuple:
    return tuple(
        corpus.run_window_lane_v1(
            job, comparator_manifest, start, WINDOW_POINTS, SHARD_POINTS
        )
        for start in range(0, job.domain.point_count, WINDOW_POINTS)
    )


class LanedSemanticVerificationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.job = test_job()
        cls.comparator = comparator()
        cls.transcript = honest_transcript(cls.job, cls.comparator)
        cls.run_claim = run_claim(cls.transcript)
        cls.monolithic = verify_transcript(
            cls.job, cls.comparator, cls.transcript, cls.run_claim
        )

    def test_monolithic_baseline_seals_a_receipt(self) -> None:
        from semantic.receipt import SemanticVerificationReceiptV1

        self.assertIs(type(self.monolithic), SemanticVerificationReceiptV1)

    def test_laned_verification_seals_the_same_receipt(self) -> None:
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, self.transcript, self.run_claim, lanes(
                self.job, self.comparator
            )
        )
        self.assertIs(type(result), type(self.monolithic))
        self.assertEqual(result.identity, self.monolithic.identity)
        self.assertTrue(
            result.binds(self.job, self.comparator, self.run_claim, self.transcript)
        )

    def test_laned_verification_rejects_foreign_inputs(self) -> None:
        from semantic.receipt import (
            SemanticVerificationReasonV1,
            SemanticVerificationRejectedV1,
        )

        good_lanes = lanes(self.job, self.comparator)
        cases = (
            (object(), self.comparator, self.transcript, self.run_claim, good_lanes),
            (self.job, object(), self.transcript, self.run_claim, good_lanes),
            (self.job, self.comparator, object(), self.run_claim, good_lanes),
            (self.job, self.comparator, self.transcript, object(), good_lanes),
            (self.job, self.comparator, self.transcript, self.run_claim, ()),
            (self.job, self.comparator, self.transcript, self.run_claim, (object(),)),
        )
        for case in cases:
            result = verification_assembly.assemble_semantic_verification_v1(*case)
            self.assertIs(
                type(result),
                SemanticVerificationRejectedV1,
                f"foreign input was not rejected: {case!r}",
            )
            self.assertEqual(result.reason, SemanticVerificationReasonV1.INVALID_INPUT)

    def test_laned_verification_rejects_foreign_bindings(self) -> None:
        from semantic.receipt import (
            SemanticVerificationReasonV1,
            SemanticVerificationRejectedV1,
        )

        good_lanes = lanes(self.job, self.comparator)
        foreign_transcript = honest_transcript(
            protocol.ProofJobV1(
                self.job.definition,
                self.job.formula_spec,
                protocol.ReducedDomainManifestV1.from_ordinals(DOMAIN_ORDINALS[:-4]),
                self.job.policy,
            ),
            self.comparator,
        )
        # transcript bound to a foreign job
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, foreign_transcript, self.run_claim, good_lanes
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)
        # run claim forged against a foreign transcript identity
        foreign_run = protocol.RunClaimV1(
            self.job.identity,
            self.comparator.identity,
            digest(1),
            digest(2),
            digest(3),
            foreign_transcript.identity,
        )
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, self.transcript, foreign_run, good_lanes
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)

    def test_incomplete_lane_cover_never_seals(self) -> None:
        from semantic.receipt import SemanticVerificationRejectedV1

        all_lanes = lanes(self.job, self.comparator)
        for drop in range(len(all_lanes)):
            partial = all_lanes[:drop] + all_lanes[drop + 1 :]
            result = verification_assembly.assemble_semantic_verification_v1(
                self.job, self.comparator, self.transcript, self.run_claim, partial
            )
            self.assertIs(
                type(result), SemanticVerificationRejectedV1,
                f"dropping lane {drop} still sealed a receipt",
            )

    def test_reordered_or_overlapping_lanes_never_seal(self) -> None:
        from semantic.receipt import SemanticVerificationRejectedV1

        all_lanes = lanes(self.job, self.comparator)
        reversed_cover = tuple(reversed(all_lanes))
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, self.transcript, self.run_claim, reversed_cover
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)
        duplicated = all_lanes + all_lanes[-1:]
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, self.transcript, self.run_claim, duplicated
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)

    def test_diverging_replay_never_seals(self) -> None:
        from semantic.receipt import (
            SemanticVerificationReasonV1,
            SemanticVerificationRejectedV1,
        )

        # A transcript whose committed accounting digest does not replay from
        # the lane records is a foreign transcript for this evidence.
        mutated = bytearray(self.transcript.accounting_digest)
        mutated[0] ^= 0xFF
        foreign = protocol.DecisionTranscriptV1(
            self.transcript.job_identity,
            self.transcript.domain_identity,
            self.transcript.comparator_identity,
            self.transcript.point_count,
            self.transcript.decision_bits,
            self.transcript.counters,
            self.transcript.exact_equality_count,
            bytes(mutated),
            self.transcript.witness_store,
        )
        foreign_run = protocol.RunClaimV1.for_transcript(
            self.job, self.comparator, foreign, digest(1), digest(2), digest(3)
        )
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job,
            self.comparator,
            foreign,
            foreign_run,
            lanes(self.job, self.comparator),
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.DECISION_MISMATCH)

    def test_lanes_replayed_over_a_shifted_window_never_seal(self) -> None:
        from semantic.receipt import SemanticVerificationRejectedV1

        # Lanes whose windows are shifted by one alignment unit replay real
        # points but never cover the verified domain, so no receipt seals.
        shifted = tuple(
            corpus.run_window_lane_v1(
                self.job, self.comparator, start, WINDOW_POINTS, SHARD_POINTS
            )
            for start in range(4, 4 + self.job.domain.point_count, WINDOW_POINTS)
        )
        result = verification_assembly.assemble_semantic_verification_v1(
            self.job, self.comparator, self.transcript, self.run_claim, shifted
        )
        self.assertIs(type(result), SemanticVerificationRejectedV1)


if __name__ == "__main__":
    unittest.main()
