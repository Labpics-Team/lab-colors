#!/usr/bin/env python3
"""Hostile contract for the folded point program of the semantic replay.

A full-domain replay must recompute `2^24` point lifts without re-running the
nodes whose value is already fixed by the literal/enum/job-shared environment.
The fold evaluates those nodes exactly once per precision rung and retains
only the point-dependent suffix in program order, so every point still replays
bit-identical rigorous interval semantics.  These tests lock the admission of
the fold, its structural dependency closure, and its differential agreement
with the unfolded reference path.
"""

from __future__ import annotations

import itertools
import sys
import unittest
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from region_proof_protocol import ProofJobV1  # noqa: E402

from semantic import intervalmath, region  # noqa: E402
from semantic.replay import SemanticReplay  # noqa: E402
from semantic.ssa import (  # noqa: E402
    EvaluationContext,
    FoldedPointProgramV1,
    SemanticFormulaError,
    parse_formula,
)


FIXTURES = ROOT / "fixtures"


@lru_cache(maxsize=2)
def parsed_formula(spec: bytes):
    # The formula bytes are release-pinned and immutable; parsing them once
    # keeps the differential gate on the replay semantics, not the parser.
    return parse_formula(spec)


def fixture_job() -> ProofJobV1:
    return ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())


def seam_ordinals() -> tuple[int, ...]:
    values = (0, 1, 10, 11, 127, 128, 254, 255)
    return tuple(
        (red << 16) | (green << 8) | blue
        for red, green, blue in itertools.product(values, repeat=3)
    )


def reference_point(
    job: ProofJobV1,
    rung: int,
    ordinal: int,
    grant: int,
) -> region.DecisionResult:
    """The unfolded replay path: full program evaluation for every point."""

    context = EvaluationContext(parsed_formula(job.formula_spec), rung, rung)
    shared = region.context_inputs(job.definition)
    reg = region.Region.from_definition(job.definition)
    return region.evaluate_rgb(context, ordinal, reg, shared, rung, grant)


class FoldAdmissionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.job = fixture_job()
        self.context = EvaluationContext(parsed_formula(self.job.formula_spec), 64, 64)
        self.shared = region.context_inputs(self.job.definition)

    def test_fold_requires_the_complete_shared_environment(self) -> None:
        for missing in ("adapting_luminance", "background_ratio", "surround"):
            partial = {key: value for key, value in self.shared.items() if key != missing}
            with self.assertRaises(SemanticFormulaError):
                self.context.fold_point_program(partial)

    def test_fold_rejects_point_coordinates_as_shared_inputs(self) -> None:
        # r8/g8/b8 must stay dynamic: folding them in would replay one point
        # against every point.
        for name in ("r8", "g8", "b8"):
            hostile = dict(self.shared)
            hostile[name] = 0
            with self.assertRaises(SemanticFormulaError):
                self.context.fold_point_program(hostile)

    def test_fold_rejects_foreign_shared_keys(self) -> None:
        hostile = dict(self.shared)
        hostile["foreign_key"] = intervalmath.exact(1)
        with self.assertRaises(SemanticFormulaError):
            self.context.fold_point_program(hostile)

    def test_fold_rejects_foreign_shared_types(self) -> None:
        wrong_real = dict(self.shared)
        wrong_real["adapting_luminance"] = 5
        with self.assertRaises(SemanticFormulaError):
            self.context.fold_point_program(wrong_real)
        wrong_enum = dict(self.shared)
        wrong_enum["surround"] = intervalmath.exact(1)
        with self.assertRaises(SemanticFormulaError):
            self.context.fold_point_program(wrong_enum)

    def test_folded_evaluation_rejects_foreign_point_coordinates(self) -> None:
        folded = self.context.fold_point_program(self.shared)
        for bad in ((-1, 0, 0), (256, 0, 0), (0, "0", 0), (0, 0, intervalmath.exact(0))):
            with self.assertRaises(SemanticFormulaError):
                self.context.evaluate_folded_point(folded, *bad)

    def test_folded_evaluation_requires_the_folding_precision_discipline(self) -> None:
        # A fold computed under one rung must never evaluate under another:
        # the guard/cap discipline is part of the replayed decision semantics.
        folded = self.context.fold_point_program(self.shared)
        foreign = EvaluationContext(self.context.formula, 128, 128)
        with self.assertRaises(SemanticFormulaError):
            foreign.evaluate_folded_point(folded, 0, 0, 0)


class FoldStructureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.job = fixture_job()
        cls.context = EvaluationContext(parsed_formula(cls.job.formula_spec), 64, 64)
        cls.shared = region.context_inputs(cls.job.definition)
        cls.folded = cls.context.fold_point_program(cls.shared)

    def test_fold_is_the_canonical_value_type(self) -> None:
        self.assertIs(type(self.folded), FoldedPointProgramV1)

    def test_dynamic_suffix_is_exactly_the_point_dependent_closure(self) -> None:
        # Independent closure recomputation: a node is dynamic exactly when it
        # transitively reads r8, g8 or b8.  The fold must retain that suffix
        # and nothing else, in program order.
        program = self.context.formula.program("point")
        dynamic = {"r8", "g8", "b8"}
        for node in program.nodes:
            if any(argument in dynamic for argument in node.arguments):
                dynamic.add(node.name)
        expected = tuple(
            node for node in program.nodes if node.name in dynamic
        )
        self.assertEqual(self.folded.dynamic_nodes, expected)
        self.assertLess(len(expected), len(program.nodes))
        self.assertEqual(
            len(expected) + len(self.folded.static_names), len(program.nodes)
        )

    def test_every_output_still_depends_on_the_point_coordinates(self) -> None:
        program = self.context.formula.program("point")
        dynamic_names = {node.name for node in self.folded.dynamic_nodes}
        for output in program.outputs:
            self.assertIn(output, dynamic_names)


class FoldDifferentialTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.job = fixture_job()
        cls.shared = region.context_inputs(cls.job.definition)
        cls.reg = region.Region.from_definition(cls.job.definition)

    def folded_lift(self, rung: int, ordinal: int) -> tuple[object, ...]:
        context = EvaluationContext(parsed_formula(self.job.formula_spec), rung, rung)
        folded = context.fold_point_program(self.shared)
        red, green, blue = region.ordinal_to_rgb(ordinal)
        outputs = context.evaluate_folded_point(folded, red, green, blue)
        return (outputs["jp"], outputs["ap"], outputs["bp"])

    def test_folded_lift_is_bit_identical_to_the_full_program(self) -> None:
        context = EvaluationContext(parsed_formula(self.job.formula_spec), 64, 64)
        samples = (0, 1, 255, 1 << 23, (1 << 24) - 1, *seam_ordinals()[:16])
        for ordinal in samples:
            red, green, blue = region.ordinal_to_rgb(ordinal)
            inputs = dict(self.shared)
            inputs["r8"] = red
            inputs["g8"] = green
            inputs["b8"] = blue
            reference = context.evaluate(context.formula.program("point"), inputs)
            folded_lift = self.folded_lift(64, ordinal)
            self.assertEqual(
                (reference["jp"], reference["ap"], reference["bp"]),
                folded_lift,
                f"folded lift drifted at ordinal {ordinal}",
            )

    def test_folded_replay_decides_exactly_like_the_unfolded_path(self) -> None:
        grant = 10**9
        for ordinal in seam_ordinals():
            expected = reference_point(self.job, 64, ordinal, grant)
            context = EvaluationContext(parsed_formula(self.job.formula_spec), 64, 64)
            folded = context.fold_point_program(self.shared)
            red, green, blue = region.ordinal_to_rgb(ordinal)
            outputs = context.evaluate_folded_point(folded, red, green, blue)
            lift = (outputs["jp"], outputs["ap"], outputs["bp"])
            actual = region.evaluate_rgb(
                context, ordinal, self.reg, self.shared, 64, grant, lift=lift
            )
            self.assertEqual(actual, expected, f"decision drifted at {ordinal}")

    def test_folded_replay_agrees_on_escalated_rungs(self) -> None:
        grant = 10**9
        # Points inside the tone band exercise the segment predicate at the
        # escalated rung; their decisions must stay bit-identical.
        in_band = [
            ordinal
            for ordinal in seam_ordinals()
            if reference_point(self.job, 64, ordinal, grant).outcome
            != region.OUTSIDE
        ]
        self.assertTrue(in_band, "the frozen seam cube must reach the band")
        for ordinal in in_band:
            expected = reference_point(self.job, 128, ordinal, grant)
            context = EvaluationContext(parsed_formula(self.job.formula_spec), 128, 128)
            folded = context.fold_point_program(self.shared)
            red, green, blue = region.ordinal_to_rgb(ordinal)
            outputs = context.evaluate_folded_point(folded, red, green, blue)
            lift = (outputs["jp"], outputs["ap"], outputs["bp"])
            actual = region.evaluate_rgb(
                context, ordinal, self.reg, self.shared, 128, grant, lift=lift
            )
            self.assertEqual(actual, expected, f"escalated decision drifted at {ordinal}")

    def test_foreign_shared_inputs_never_leak_through_a_fold(self) -> None:
        # Two different job contexts produce different folds: a replay must
        # not be able to reuse constants computed for a foreign definition.
        from fractions import Fraction

        first = EvaluationContext(parsed_formula(self.job.formula_spec), 64, 64)
        first_fold = first.fold_point_program(self.shared)
        mutated = dict(self.shared)
        mutated["adapting_luminance"] = intervalmath.exact(
            Fraction(7) + Fraction(1, 2**40)
        )
        second_fold = first.fold_point_program(mutated)
        ordinal = seam_ordinals()[64]
        red, green, blue = region.ordinal_to_rgb(ordinal)
        first_lift = first.evaluate_folded_point(first_fold, red, green, blue)
        second_lift = first.evaluate_folded_point(second_fold, red, green, blue)
        self.assertNotEqual(first_lift["jp"], second_lift["jp"])

    def test_folded_evaluation_is_deterministic(self) -> None:
        context = EvaluationContext(parsed_formula(self.job.formula_spec), 64, 64)
        folded = context.fold_point_program(self.shared)
        ordinal = seam_ordinals()[100]
        red, green, blue = region.ordinal_to_rgb(ordinal)
        first = context.evaluate_folded_point(folded, red, green, blue)
        second = context.evaluate_folded_point(folded, red, green, blue)
        self.assertEqual(first, second)


class ReplayDriverIntegrationTests(unittest.TestCase):
    def test_driver_replay_matches_the_unfolded_reference_point_by_point(self) -> None:
        job = fixture_job()
        formula = parsed_formula(job.formula_spec)
        reg = region.Region.from_definition(job.definition)
        shared = region.context_inputs(job.definition)
        budget = next(
            item
            for item in job.policy.comparators
            if item.kind == job.policy.comparators[0].kind
        )

        from region_proof_protocol import ComparatorKindV1, ComparatorManifestV2, ContentResolvedComparatorManifestV2
        import hashlib

        def content(key: bytes) -> bytes:
            return b"semantic-folding-test-" + key

        comparator = ContentResolvedComparatorManifestV2.admit(
            ComparatorManifestV2(
                kind=ComparatorKindV1.ARB,
                **{
                    field: hashlib.sha256(f"folding-{field}".encode()).digest()
                    for field in (
                        "engine_release",
                        "upstream_source",
                        "arithmetic_input_set",
                        "wrapper_source",
                        "evaluator_source",
                        "build_identity",
                        "operation_allowlist",
                        "test_observation",
                        "legal_file_set",
                        "exclusions",
                    )
                },
            ),
            content,
        )

        driver = SemanticReplay(job, comparator)
        reference_contexts = {}
        for ordinal in job.domain.iter_ordinals():
            point = driver.next_point()
            self.assertEqual(point.ordinal, ordinal)

            # Independent reference: the unfolded ladder loop.
            grant = min(budget.per_point_work, 10**18)
            remaining = grant
            consumed = 0
            final = None
            for rung in budget.precision_ladder:
                context = reference_contexts.get(rung)
                if context is None:
                    context = EvaluationContext(formula, rung, rung)
                    reference_contexts[rung] = context
                final = region.evaluate_rgb(context, ordinal, reg, shared, rung, remaining)
                remaining -= final.consumed_branches
                consumed += final.consumed_branches
                if final.outcome != region.BOUNDARY_UNPROVEN:
                    break
            self.assertEqual(point.outcome, final.outcome, f"outcome at {ordinal}")
            self.assertEqual(point.final_precision, rung, f"precision at {ordinal}")
            self.assertEqual(point.consumed, consumed, f"consumption at {ordinal}")
            self.assertEqual(point.exact_boundary, final.exact_boundary)
            self.assertEqual(point.exact_branch, final.exact_branch)


if __name__ == "__main__":
    unittest.main()
