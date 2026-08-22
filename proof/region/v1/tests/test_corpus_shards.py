#!/usr/bin/env python3
"""Hostile contract for the sharded corpus runner (V5b2d-1b).

The shard stream must reassemble into the exact transcript a monolithic
replay packs, byte for byte, and nothing but a contiguous, packing-aligned,
in-order cover of the declared domain may assemble.  The full-domain job
derivation and its mint-gate coordinates are pinned here without running the
2^24 replay, which stays a long-running corpus lane.
"""

from __future__ import annotations

import hashlib
import importlib.util
import sys
import unittest
from functools import cache
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
ARB_TESTS = PROOF / "arb" / "tests"
MPFI_TESTS = PROOF / "mpfi" / "tests"
sys.path[:0] = [str(PROOF), str(ARB_TESTS), str(MPFI_TESTS)]

import corpus  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

from semantic import replay as semantic_replay  # noqa: E402


def _load_harness(name: str, path: Path) -> object:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"unreachable harness {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


ARB_PIPELINE_HARNESS = _load_harness(
    "corpus_arb_pipeline_harness", ARB_TESTS / "test_pipeline.py"
)


def digest(label: int) -> bytes:
    return hashlib.sha256(f"corpus-shard-test-{label}".encode("ascii")).digest()


@cache
def _base_job() -> protocol.ProofJobV1:
    return ARB_PIPELINE_HARNESS._job()


@cache
def _comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(
        f"corpus-shard-manifest-{index}".encode("ascii") for index in range(10)
    )
    return protocol.ContentResolvedComparatorManifestV2.admit(
        protocol.ComparatorManifestV2(
            protocol.ComparatorKindV1.ARB,
            *(hashlib.sha256(content).digest() for content in contents),
        ),
        {
            hashlib.sha256(content).digest(): content for content in contents
        }.get,
    )


def _job_over_ordinals(ordinals: tuple[int, ...]) -> protocol.ProofJobV1:
    base = _base_job()
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        protocol.ReducedDomainManifestV1.from_ordinals(ordinals),
        base.policy,
    )


def monolithic_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
) -> protocol.DecisionTranscriptV1:
    """Honest monolithic pass using the corpus witness grammar."""

    driver = semantic_replay.SemanticReplay(job, comparator)
    accounting = semantic_replay.accounting_prefix_v1(
        comparator.manifest.kind, job, comparator.source_identity
    )
    decisions: list[protocol.DecisionV1] = []
    witnesses: list[protocol.WitnessV1] = []
    for _ in range(job.domain.point_count):
        point = driver.next_point()
        decision = protocol.DecisionV1(point.outcome)
        decisions.append(decision)
        if decision == protocol.DecisionV1.INSIDE and point.exact_boundary:
            witnesses.append(
                protocol.ExactZeroSignalTraceV1(
                    point.ordinal,
                    semantic_replay.exact_trace_digest_v1(
                        job.identity, point.ordinal, point.exact_branch
                    ),
                )
            )
        elif decision == protocol.DecisionV1.BOUNDARY_UNPROVEN:
            witnesses.append(
                protocol.BoundaryUnprovenWitnessV1(
                    point.ordinal,
                    corpus.boundary_enclosure_digest_v1(
                        job.identity, point.ordinal, point.exact_branch
                    ),
                )
            )
        elif decision == protocol.DecisionV1.RESOURCE_LIMIT_REACHED:
            witnesses.append(
                protocol.ResourceLimitWitnessV1(
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
    return protocol.DecisionTranscriptV1.from_decisions(
        job, comparator, decisions, witnesses, accounting.digest()
    )


def sharded_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    shard_points: int,
) -> object:
    plan = corpus.shard_plan_v1(job.domain, shard_points)
    if type(plan) is not tuple:
        raise AssertionError(f"shard plan rejected: {plan!r}")
    runner = corpus.ShardCorpusRunnerV1(job, comparator)
    shards = tuple(runner.run_shard(start, end) for start, end in plan)
    return corpus.assemble_transcript_from_shards_v1(
        job, comparator, shards, runner.accounting_digest
    )


class ShardPlanTests(unittest.TestCase):
    def test_plan_covers_the_domain_with_aligned_windows(self) -> None:
        domain = _job_over_ordinals(tuple(range(32))).domain
        plan = corpus.shard_plan_v1(domain, 8)
        self.assertIs(type(plan), tuple)
        self.assertEqual(plan, ((0, 8), (8, 16), (16, 24), (24, 32)))
        full = corpus.shard_plan_v1(
            protocol.exact_full_domain_manifest_v1(), 1 << 16
        )
        self.assertIs(type(full), tuple)
        self.assertEqual(len(full), 1 << 8)
        self.assertEqual(full[0], (0, 1 << 16))
        self.assertEqual(full[-1], (protocol.OUTPUT_CARDINALITY_V1 - (1 << 16), protocol.OUTPUT_CARDINALITY_V1))

    def test_unaligned_shard_width_is_rejected(self) -> None:
        domain = _base_job().domain
        for width in (0, 3, 5, -8):
            result = corpus.shard_plan_v1(domain, width)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1, width)
            self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)

    def test_foreign_domain_is_rejected(self) -> None:
        result = corpus.shard_plan_v1(_base_job(), 8)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)


class ShardAssemblyByteIdentityTests(unittest.TestCase):
    def test_sharded_transcript_is_byte_identical_to_monolithic(self) -> None:
        ordinals = tuple(range(512))
        job = _job_over_ordinals(ordinals)
        comparator = _comparator()
        monolithic = monolithic_transcript(job, comparator)
        for shard_points in (4, 8, 64, len(ordinals)):
            assembled = sharded_transcript(job, comparator, shard_points)
            self.assertIs(
                type(assembled), protocol.DecisionTranscriptV1, shard_points
            )
            self.assertEqual(assembled.encode(), monolithic.encode(), shard_points)
            self.assertEqual(assembled.identity, monolithic.identity, shard_points)

    def test_shard_window_breaking_replay_order_is_refused(self) -> None:
        job = _job_over_ordinals(tuple(range(128)))
        runner = corpus.ShardCorpusRunnerV1(job, _comparator())
        runner.run_shard(0, 8)
        with self.assertRaises(ValueError):
            runner.run_shard(12, 16)

    def test_assembly_rejects_out_of_order_shards(self) -> None:
        job = _job_over_ordinals(tuple(range(256)))
        comparator = _comparator()
        runner = corpus.ShardCorpusRunnerV1(job, comparator)
        first = runner.run_shard(0, 8)
        second = runner.run_shard(8, 16)
        result = corpus.assemble_transcript_from_shards_v1(
            job, comparator, (second, first), runner.accounting_digest
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertIn(
            result.reason,
            (
                corpus.ShardCorpusReasonV1.SHARD_ORDER,
                corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            ),
        )

    def test_assembly_rejects_a_gap_in_the_cover(self) -> None:
        job = _job_over_ordinals(tuple(range(256)))
        comparator = _comparator()
        runner = corpus.ShardCorpusRunnerV1(job, comparator)
        first = runner.run_shard(0, 8)
        runner.run_shard(8, 12)
        third = runner.run_shard(12, 16)
        result = corpus.assemble_transcript_from_shards_v1(
            job, comparator, (first, third), runner.accounting_digest
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_a_truncated_cover(self) -> None:
        job = _job_over_ordinals(tuple(range(256)))
        comparator = _comparator()
        runner = corpus.ShardCorpusRunnerV1(job, comparator)
        first = runner.run_shard(0, 8)
        result = corpus.assemble_transcript_from_shards_v1(
            job, comparator, (first,), runner.accounting_digest
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)

    def test_assembly_rejects_foreign_shards(self) -> None:
        job = _job_over_ordinals(tuple(range(64)))
        comparator = _comparator()
        runner = corpus.ShardCorpusRunnerV1(job, comparator)
        result = corpus.assemble_transcript_from_shards_v1(
            job, comparator, (digest(1),), runner.accounting_digest
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)


class FullDomainJobTests(unittest.TestCase):
    def test_full_domain_job_binds_the_exact_full_manifest(self) -> None:
        job = corpus.full_domain_job_v1(_base_job())
        manifest = job.domain
        self.assertEqual(manifest.ranges, ((0, protocol.OUTPUT_CARDINALITY_V1),))
        self.assertEqual(manifest.point_count, protocol.OUTPUT_CARDINALITY_V1)
        self.assertEqual(
            manifest.identity, protocol.exact_full_domain_manifest_v1().identity
        )
        self.assertEqual(
            job.definition.definition_digest,
            _base_job().definition.definition_digest,
        )
        self.assertNotEqual(job.policy.identity, _base_job().policy.identity)

    def test_full_domain_job_rejects_foreign_input(self) -> None:
        with self.assertRaises(TypeError):
            corpus.full_domain_job_v1(_base_job().domain)

    def test_full_domain_job_declares_the_work_a_certification_needs(self) -> None:
        # The frozen fixture policy is a hostile zero-grant declaration, and
        # borrowing it for a certified run leaves every boundary point on
        # RESOURCE_LIMIT_REACHED — a failure the dual admission only reports
        # after the whole domain has been materialised.
        base = _base_job()
        self.assertTrue(
            all(budget.per_point_work == 0 for budget in base.policy.comparators)
        )
        job = corpus.full_domain_job_v1(base)
        for derived, declared in zip(
            job.policy.comparators, base.policy.comparators, strict=True
        ):
            # The bound follows each comparator's own ladder, so it is read
            # per budget rather than once for the whole policy.
            bound = corpus.decision_procedure_work_bound_v1(
                base.definition, declared.precision_ladder
            )
            self.assertGreater(bound, 0)
            self.assertEqual(derived.per_point_work, bound)
            # The pregrant is an absolute total over the domain's ordinal
            # prefix, not a rate, so it follows the domain being certified.
            self.assertEqual(
                derived.global_pregrant, bound * protocol.OUTPUT_CARDINALITY_V1
            )

    def test_full_domain_job_keeps_the_declared_ladder_and_release(self) -> None:
        base = _base_job()
        job = corpus.full_domain_job_v1(base)
        self.assertEqual(
            job.policy.equality_release, base.policy.equality_release
        )
        for derived, declared in zip(
            job.policy.comparators, base.policy.comparators, strict=True
        ):
            self.assertEqual(derived.kind, declared.kind)
            self.assertEqual(derived.precision_ladder, declared.precision_ladder)

    def test_work_bound_covers_every_rung_not_only_the_first(self) -> None:
        # A point's grant is shared across the ladder: one that pays a branch
        # at a low rung and stays BOUNDARY_UNPROVEN escalates and pays again.
        # Budgeting a single rung starves points that escalate past that
        # share — the very points the ladder exists for.
        definition = _base_job().definition
        segments = max(1, definition.knot_count - 1)
        for ladder in ((64,), (16, 64), (64, 128), (32, 64, 128, 256)):
            self.assertEqual(
                corpus.decision_procedure_work_bound_v1(definition, ladder),
                len(ladder) * segments,
                ladder,
            )
        with self.assertRaises(TypeError):
            corpus.decision_procedure_work_bound_v1(definition.fields, (64, 128))
        for ladder in (None, (), (64, "128")):
            with self.assertRaises(TypeError):
                corpus.decision_procedure_work_bound_v1(definition, ladder)

    def test_certified_budget_follows_each_comparators_own_ladder(self) -> None:
        base = _base_job()
        arb, mpfi = base.policy.comparators
        mixed = protocol.ProofPolicyV1(
            base.policy.equality_release,
            (
                protocol.ComparatorBudgetV1(arb.kind, (64,), 0, 0),
                protocol.ComparatorBudgetV1(mpfi.kind, (16, 64, 128), 0, 0),
            ),
        )
        policy = corpus.certified_work_policy_v1(base.definition, mixed, 512)
        segments = max(1, base.definition.knot_count - 1)
        self.assertEqual(policy.comparators[0].per_point_work, 1 * segments)
        self.assertEqual(policy.comparators[1].per_point_work, 3 * segments)
        for budget in policy.comparators:
            self.assertEqual(budget.global_pregrant, budget.per_point_work * 512)


class DomainPrefixTests(unittest.TestCase):
    def test_prefix_counts_domain_points_not_ordinals(self) -> None:
        # The monolithic run charges one grant per domain point in iteration
        # order, so a lane's prefix is the number of domain points below its
        # window.  Using the ordinal overcounts on any reduced domain and
        # starves the lane against the run it replays.
        domain = protocol.ReducedDomainManifestV1(
            ((0, 128), (65792, 65920)), 256
        )
        self.assertEqual(corpus.domain_points_before_v1(domain, 0), 0)
        self.assertEqual(corpus.domain_points_before_v1(domain, 64), 64)
        self.assertEqual(corpus.domain_points_before_v1(domain, 128), 128)
        self.assertEqual(corpus.domain_points_before_v1(domain, 65792), 128)
        self.assertEqual(corpus.domain_points_before_v1(domain, 65856), 192)
        self.assertEqual(
            corpus.domain_points_before_v1(domain, protocol.OUTPUT_CARDINALITY_V1),
            256,
        )

    def test_prefix_is_the_ordinal_only_on_the_exact_full_manifest(self) -> None:
        full = protocol.exact_full_domain_manifest_v1()
        for window_start in (0, 65536, 16_711_680):
            self.assertEqual(
                corpus.domain_points_before_v1(full, window_start), window_start
            )

    def test_prefix_rejects_foreign_coordinates(self) -> None:
        full = protocol.exact_full_domain_manifest_v1()
        with self.assertRaises(TypeError):
            corpus.domain_points_before_v1(full.ranges, 0)
        with self.assertRaises(TypeError):
            corpus.domain_points_before_v1(full, -1)

    def test_full_domain_claim_coordinates_admit_the_mint_gate(self) -> None:
        import dual_proof

        job = corpus.full_domain_job_v1(_base_job())
        claim = protocol.DualComparisonClaimV1(
            job.identity,
            job.definition.definition_digest,
            job.domain.identity,
            job.policy.identity,
            job.domain.point_count,
            (digest(1), digest(2)),
            (digest(3), digest(4)),
            (digest(5), digest(6)),
            digest(7),
        )
        self.assertTrue(dual_proof.claim_spans_full_domain_v1(claim))

    def test_boundary_enclosure_digest_is_deterministic(self) -> None:
        first = corpus.boundary_enclosure_digest_v1(digest(9), 42, 1)
        second = corpus.boundary_enclosure_digest_v1(digest(9), 42, 1)
        third = corpus.boundary_enclosure_digest_v1(digest(9), 43, 1)
        self.assertEqual(first, second)
        self.assertNotEqual(first, third)
        self.assertEqual(len(first), 32)


if __name__ == "__main__":
    unittest.main()
