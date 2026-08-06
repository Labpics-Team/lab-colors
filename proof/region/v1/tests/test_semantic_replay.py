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
    BoundaryUnprovenWitnessV1,
    ComparatorKindV1,
    ComparatorManifestV2,
    ContentResolvedComparatorManifestV2,
    DecisionTranscriptV1,
    DecisionV1,
    ExactZeroSignalTraceV1,
    ProofJobV1,
    ReducedDomainManifestV1,
    ComparatorBudgetV1,
    ProofPolicyV1,
    ResourceLimitWitnessV1,
    RunClaimV1,
    WitnessStoreV1,
    WitnessV1,
    compare_dual_transcripts,
)

import corpus  # noqa: E402

from semantic import replay as semantic_replay  # noqa: E402
from semantic.receipt import (  # noqa: E402
    SemanticVerificationReceiptV1,
    SemanticVerificationReasonV1,
    SemanticVerificationRejectedV1,
    resolved_decision_digest_v1,
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


def protocol_policy(
    base: ProofJobV1, ladder: tuple[int, ...], work: int, pregrant: int
) -> ProofPolicyV1:
    return ProofPolicyV1(
        base.policy.equality_release,
        tuple(
            ComparatorBudgetV1(budget.kind, ladder, work, pregrant)
            for budget in base.policy.comparators
        ),
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
        # A wrong transcript must fail on replayed semantics, never
        # masquerade as a binding failure.
        self.assertNotEqual(first.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)
        self.assertNotIsInstance(first, SemanticVerificationReceiptV1)

    def test_saturate_all_inside_transcript_fails_replay(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.MPFI, 600)
        transcript = all_inside_transcript(job, comparator)
        run = run_claim(job, comparator, transcript)

        result = verify_transcript(job, comparator, transcript, run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)
        # Saturating every point to INSIDE fails at the first domain point,
        # and it must fail as a decision mismatch, not a vacuous rejection.
        self.assertEqual(result.reason, SemanticVerificationReasonV1.DECISION_MISMATCH)
        self.assertEqual(result.ordinal, tuple(job.domain.iter_ordinals())[0])

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
        # The verifier can only attest the run that binds the transcript it
        # replays.  Binary, invocation and platform causality belongs to the
        # source-bound controller, so a run becomes foreign to this
        # verification exactly when it points at a different transcript.
        foreign_run = RunClaimV1(
            job.identity,
            comparator.source_identity,
            digest(821),
            digest(822),
            digest(823),
            digest(829),
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

    def test_point_count_drift_is_rejected_before_replay(self) -> None:
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 780)
        # A transcript may be internally consistent yet claim more points
        # than the bound domain; the verifier must reject it as a foreign
        # binding instead of walking past the end of the domain ordinals.
        drifted_count = job.domain.point_count + 4
        transcript = DecisionTranscriptV1(
            job.identity,
            job.domain.identity,
            comparator.source_identity,
            drifted_count,
            bytes([0x55] * ((drifted_count + 3) // 4)),
            (0, drifted_count, 0, 0),
            0,
            digest(812),
            WitnessStoreV1.from_witnesses(()),
        )
        run = run_claim(job, comparator, transcript)

        result = verify_transcript(job, comparator, transcript, run)
        self.assertIsInstance(result, SemanticVerificationRejectedV1)
        self.assertEqual(result.reason, SemanticVerificationReasonV1.FOREIGN_BINDING)

    def test_replayed_transcript_seals_a_receipt(self) -> None:
        # Anti-vacuum: the verifier must mint a receipt for a transcript that
        # exactly matches its own independent replay, not only reject.
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 900)
        driver = semantic_replay.SemanticReplay(job, comparator)
        decisions: list[DecisionV1] = []
        witnesses: list[WitnessV1] = []
        accounting = semantic_replay.accounting_prefix_v1(
            comparator.manifest.kind,
            job,
            comparator.source_identity,
        )
        for _ in range(job.domain.point_count):
            point = driver.next_point()
            decisions.append(DecisionV1(point.outcome))
            if point.outcome == DecisionV1.INSIDE and point.exact_boundary:
                witnesses.append(
                    ExactZeroSignalTraceV1(
                        point.ordinal,
                        semantic_replay.exact_trace_digest_v1(
                            job.identity,
                            point.ordinal,
                            point.exact_branch,
                        ),
                    )
                )
            elif point.outcome == DecisionV1.BOUNDARY_UNPROVEN:
                witnesses.append(
                    BoundaryUnprovenWitnessV1(
                        point.ordinal,
                        digest(100_000 + point.ordinal),
                    )
                )
            elif point.outcome == DecisionV1.RESOURCE_LIMIT_REACHED:
                witnesses.append(
                    ResourceLimitWitnessV1(
                        point.ordinal,
                        point.resource_scope,
                        point.point_grant,
                        point.consumed,
                    )
                )
            accounting.update(
                semantic_replay.account_record(
                    point.ordinal,
                    point.final_precision,
                    point.consumed,
                    point.outcome,
                )
            )
        transcript = DecisionTranscriptV1.from_decisions(
            job,
            comparator,
            decisions,
            witnesses,
            accounting.digest(),
        )
        run = run_claim(job, comparator, transcript)

        result = verify_transcript(job, comparator, transcript, run)
        self.assertIsInstance(result, SemanticVerificationReceiptV1)
        self.assertEqual(result.run_claim_identity, run.identity)
        self.assertEqual(result.transcript_identity, transcript.identity)
        self.assertTrue(result.binds(job, comparator, run, transcript))

        # Visible coverage: the transcript carries exactly the outcomes the
        # replay produced, with one aligned witness per witness-requiring
        # outcome and none for decisive ones.
        self.assertEqual(len(decisions), job.domain.point_count)
        occurred = {decision.value for decision in decisions}
        self.assertTrue(occurred <= {member.value for member in DecisionV1})
        boundary_count = decisions.count(DecisionV1.BOUNDARY_UNPROVEN)
        boundary_witnesses = tuple(
            witness for witness in witnesses if type(witness) is BoundaryUnprovenWitnessV1
        )
        self.assertEqual(len(boundary_witnesses), boundary_count)
        self.assertEqual(
            len({witness.ordinal for witness in witnesses}),
            len(witnesses),
            "witness ordinals must stay unique and point-aligned",
        )

    def test_decision_digest_matches_dual_admission_grammar(self) -> None:
        # The semantic receipt's resolved-decision digest must agree with the
        # digest that dual comparison computes for identical decision bits;
        # any grammar drift would fork the two proof surfaces.
        job = fixture_job()
        arb = admit_manifest(ComparatorKindV1.ARB, 920)
        mpfi = admit_manifest(ComparatorKindV1.MPFI, 940)
        first = all_outside_transcript(job, arb)
        second = all_outside_transcript(job, mpfi)
        first_run = RunClaimV1.for_transcript(
            job, arb, first, digest(831), digest(832), digest(833)
        )
        second_run = RunClaimV1.for_transcript(
            job, mpfi, second, digest(834), digest(835), digest(836)
        )

        candidate = compare_dual_transcripts(
            job, arb, first, first_run, mpfi, second, second_run
        )
        self.assertEqual(
            candidate.claim.decision_digest,
            resolved_decision_digest_v1(first.domain_identity, first.decision_bits),
        )

    def test_accounting_digest_grammar_matches_independent_packing(self) -> None:
        # The accounting grammar is re-packed by hand from the declared wire
        # layout; the replay helpers must reproduce exactly that digest.
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.ARB, 910)
        records = ((5, 8, 3, 2), (9, 16, 0, 0), (70_000, 8, 12, 1))
        hasher = hashlib.sha256()
        hasher.update(b"labcolors.arb-evaluation-accounting.v1\0")
        hasher.update(job.identity)
        hasher.update(job.domain.identity)
        hasher.update(job.policy.identity)
        hasher.update(comparator.source_identity)
        for ordinal, precision, consumed, outcome in records:
            hasher.update(ordinal.to_bytes(4, "big"))
            hasher.update(precision.to_bytes(4, "big"))
            hasher.update(consumed.to_bytes(8, "big"))
            hasher.update(bytes((outcome,)))
        expected = hasher.digest()

        accounting = semantic_replay.accounting_prefix_v1(
            comparator.manifest.kind, job, comparator.source_identity
        )
        for record in records:
            accounting.update(semantic_replay.account_record(*record))
        self.assertEqual(accounting.digest(), expected)

    def test_mpfi_accounting_digest_matches_independent_packing(self) -> None:
        # The MPFI lane carries its own domain prefix; the expected digest is
        # packed from scratch again so neither lane reuses the other's truth.
        job = fixture_job()
        comparator = admit_manifest(ComparatorKindV1.MPFI, 911)
        records = ((3, 8, 1, 1), (12, 32, 0, 2), (400_000, 16, 7, 0))
        hasher = hashlib.sha256()
        hasher.update(b"labcolors.mpfi-evaluation-accounting.v1\0")
        hasher.update(job.identity)
        hasher.update(job.domain.identity)
        hasher.update(job.policy.identity)
        hasher.update(comparator.source_identity)
        for ordinal, precision, consumed, outcome in records:
            hasher.update(ordinal.to_bytes(4, "big"))
            hasher.update(precision.to_bytes(4, "big"))
            hasher.update(consumed.to_bytes(8, "big"))
            hasher.update(bytes((outcome,)))
        expected = hasher.digest()

        accounting = semantic_replay.accounting_prefix_v1(
            comparator.manifest.kind, job, comparator.source_identity
        )
        for record in records:
            accounting.update(semantic_replay.account_record(*record))
        self.assertEqual(accounting.digest(), expected)


class CertifiedPolicyDecidesEveryPointTests(unittest.TestCase):
    """A certified policy must decide every point it is certified over.

    A dual comparison refuses any transcript carrying an unresolved outcome,
    so a budget that starves the decision procedure cannot produce a proof at
    all — and the run only reports that failure after materialising the whole
    domain.  The frozen fixture declares a hostile zero-grant policy on
    purpose; the certified derivation is what makes the same points decide.
    """

    def _outcomes(self, job: ProofJobV1, kind: ComparatorKindV1) -> dict[int, int]:
        replay = semantic_replay.SemanticReplay(job, admit_manifest(kind, 700))
        outcomes: dict[int, int] = {}
        for _ in range(job.domain.point_count):
            point = replay.next_point()
            outcomes[point.ordinal] = point.outcome
        return outcomes

    def _certified_over_seams(self) -> ProofJobV1:
        base = fixture_job()
        return ProofJobV1(
            base.definition,
            base.formula_spec,
            base.domain,
            corpus.certified_work_policy_v1(
                base.definition, base.policy, base.domain.point_count
            ),
        )

    def _unresolved(self, outcomes: dict[int, int]) -> dict[int, int]:
        return {
            ordinal: outcome
            for ordinal, outcome in outcomes.items()
            if outcome
            in (
                int(DecisionV1.BOUNDARY_UNPROVEN),
                int(DecisionV1.RESOURCE_LIMIT_REACHED),
            )
        }

    def test_certified_policy_leaves_no_unresolved_point(self) -> None:
        outcomes = self._outcomes(self._certified_over_seams(), ComparatorKindV1.ARB)
        self.assertEqual(self._unresolved(outcomes), {})

    def test_the_starved_fixture_policy_cannot_decide_the_same_points(self) -> None:
        # Anti-vacuity: the seam domain really exercises the branch a grant
        # pays for, so the test above is not passing on an all-outside domain.
        outcomes = self._outcomes(fixture_job(), ComparatorKindV1.ARB)
        starved = self._unresolved(outcomes)
        self.assertTrue(starved)
        self.assertTrue(
            all(
                outcome == int(DecisionV1.RESOURCE_LIMIT_REACHED)
                for outcome in starved.values()
            )
        )

    def test_certified_domain_carries_the_region_and_its_complement(self) -> None:
        outcomes = self._outcomes(self._certified_over_seams(), ComparatorKindV1.ARB)
        self.assertIn(int(DecisionV1.INSIDE), outcomes.values())
        self.assertIn(int(DecisionV1.OUTSIDE), outcomes.values())

    def _job_over(
        self, ordinals: tuple[int, ...], ladder: tuple[int, ...], work: int
    ) -> ProofJobV1:
        base = fixture_job()
        domain = ReducedDomainManifestV1.from_ordinals(ordinals)
        return ProofJobV1(
            base.definition,
            base.formula_spec,
            domain,
            protocol_policy(base, ladder, work, work * domain.point_count),
        )

    def test_a_single_rung_budget_starves_points_that_escalate(self) -> None:
        # Falsifying case for "one branch per segment is enough per point":
        # the grant is shared across the ladder, so a point that pays at the
        # low rung reaches the next rung with nothing left.  On ladder
        # (16, 64) the fixture's own definition leaves ordinals 65792 and
        # 65794 unresolved at one branch and decides both at two.
        window = tuple(range(65_780, 65_800))
        starved = self._outcomes(
            self._job_over(window, (16, 64), 1), ComparatorKindV1.ARB
        )
        self.assertEqual(
            sorted(self._unresolved(starved)), [65_792, 65_794]
        )
        covered = self._outcomes(
            self._job_over(window, (16, 64), 2), ComparatorKindV1.ARB
        )
        self.assertEqual(self._unresolved(covered), {})

    def test_the_certified_bound_covers_that_ladder(self) -> None:
        # The derived bound is what closes the class, not a hand-picked 2.
        base = fixture_job()
        ladder = (16, 64)
        work = corpus.decision_procedure_work_bound_v1(base.definition, ladder)
        outcomes = self._outcomes(
            self._job_over(tuple(range(65_780, 65_800)), ladder, work),
            ComparatorKindV1.ARB,
        )
        self.assertEqual(self._unresolved(outcomes), {})


if __name__ == "__main__":
    unittest.main()
