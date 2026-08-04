"""Semantic replay of the V1 region decision rules.

This module mirrors the public decision semantics of the engine region code
on rigorous Fraction intervals: the same outcome ladder, the same branch
accounting, and the same exact-boundary discipline.  It never samples floats
into decisions; every comparison is an exact rational comparison.
"""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction

import region_proof_protocol as protocol

from . import intervalmath
from .ssa import EvaluationContext, SemanticFormulaError

INSIDE = 0
OUTSIDE = 1
BOUNDARY_UNPROVEN = 2
RESOURCE_LIMIT_REACHED = 3


@dataclass(frozen=True)
class Knot:
    tone: Fraction
    center_a: Fraction
    center_b: Fraction
    radius_squared: Fraction


@dataclass(frozen=True)
class Region:
    knots: tuple[Knot, ...]
    metric_aa: Fraction
    metric_ab: Fraction
    metric_bb: Fraction

    @classmethod
    def from_definition(cls, definition: protocol.ContextualRegionDefinitionV1) -> "Region":
        def dyadic(index: int) -> Fraction:
            return protocol._dyadic(definition.fields[index], "semantic-region", "coordinate")

        knots = tuple(
            Knot(
                dyadic(22 + 4 * index),
                dyadic(23 + 4 * index),
                dyadic(24 + 4 * index),
                dyadic(25 + 4 * index),
            )
            for index in range(definition.knot_count)
        )
        return cls(knots, dyadic(18), dyadic(19), dyadic(20))


def context_inputs(definition: protocol.ContextualRegionDefinitionV1) -> dict[str, object]:
    """Exact real inputs shared by every point lift."""

    adapting = protocol._dyadic(definition.fields[11], "semantic-region", "adapting_luminance")
    ratio = protocol._dyadic(definition.fields[12], "semantic-region", "background_ratio")
    surround = definition.fields[13][0]
    return {
        "adapting_luminance": intervalmath.exact(adapting),
        "background_ratio": intervalmath.exact(ratio),
        "surround": surround,
    }


def ordinal_to_rgb(ordinal: int) -> tuple[int, int, int]:
    return (ordinal >> 16) & 0xFF, (ordinal >> 8) & 0xFF, ordinal & 0xFF


@dataclass(frozen=True)
class DecisionResult:
    outcome: int
    consumed_branches: int
    exact_boundary: bool
    exact_branch: int


def _equal(left: intervalmath.Interval, right: intervalmath.Interval) -> bool:
    return left.lo == right.lo and left.hi == right.hi


def _overlaps(left: intervalmath.Interval, right: intervalmath.Interval) -> bool:
    return not (left.hi < right.lo or right.hi < left.lo)


def _intersection(
    left: intervalmath.Interval,
    right: intervalmath.Interval,
) -> intervalmath.Interval | None:
    lo = max(left.lo, right.lo)
    hi = min(left.hi, right.hi)
    if lo > hi:
        return None
    return intervalmath.Interval(lo, hi)


def _predicate_decision(
    ssa: EvaluationContext,
    program_name: str,
    inputs: dict[str, object],
) -> tuple[bool, bool, bool, bool]:
    """Return (resolved, inside, outside, exact_zero) for one predicate run."""

    output = {"singleton": "singleton_f", "segment": "segment_f"}[program_name]
    try:
        predicate = ssa.evaluate(ssa.formula.program(program_name), inputs)[output]
    except intervalmath.UnresolvedError:
        return False, False, False, False
    return (
        True,
        predicate.hi <= 0,
        predicate.lo > 0,
        predicate.lo == 0 and predicate.hi == 0,
    )


def _evaluate_singleton(
    ssa: EvaluationContext,
    point: tuple[intervalmath.Interval, ...],
    region: Region,
    grant: int,
) -> DecisionResult:
    knot = region.knots[0]
    tone = intervalmath.exact(knot.tone)
    if not _equal(point[0], tone):
        outcome = BOUNDARY_UNPROVEN if _overlaps(point[0], tone) else OUTSIDE
        return DecisionResult(outcome, 0, False, 0)
    if grant == 0:
        return DecisionResult(RESOURCE_LIMIT_REACHED, 0, False, 0)
    resolved, inside, outside, exact_zero = _predicate_decision(
        ssa,
        "singleton",
        {
            "singleton_a": point[1],
            "singleton_b": point[2],
            "singleton_ca": intervalmath.exact(knot.center_a),
            "singleton_cb": intervalmath.exact(knot.center_b),
            "singleton_rho": intervalmath.exact(knot.radius_squared),
            "singleton_g00": intervalmath.exact(region.metric_aa),
            "singleton_g01": intervalmath.exact(region.metric_ab),
            "singleton_g11": intervalmath.exact(region.metric_bb),
        },
    )
    if resolved and inside:
        return DecisionResult(INSIDE, 1, exact_zero, 0)
    if resolved and outside:
        return DecisionResult(OUTSIDE, 1, False, 0)
    return DecisionResult(BOUNDARY_UNPROVEN, 1, False, 0)


def decide(
    ssa: EvaluationContext,
    point: tuple[intervalmath.Interval, ...],
    region: Region,
    precision: int,
    grant: int,
) -> DecisionResult:
    """Replay the public region decision entry point on rigorous intervals."""

    if precision < 2:
        return DecisionResult(BOUNDARY_UNPROVEN, 0, False, 0)
    if len(region.knots) == 1:
        return _evaluate_singleton(ssa, point, region, grant)

    tone = point[0]
    first = intervalmath.exact(region.knots[0].tone)
    last = intervalmath.exact(region.knots[-1].tone)
    if tone.hi < first.lo or tone.lo > last.hi:
        return DecisionResult(OUTSIDE, 0, False, 0)
    outside_possible = not (tone.lo >= first.hi) or not (tone.hi <= last.lo)

    any_segment = False
    all_inside = True
    all_outside = True
    exact_zero = False
    exact_branch = 0
    consumed = 0
    for index in range(len(region.knots) - 1):
        left = region.knots[index]
        right = region.knots[index + 1]
        segment_domain = intervalmath.Interval(
            min(left.tone, right.tone),
            max(left.tone, right.tone),
        )
        intersection = _intersection(tone, segment_domain)
        if intersection is None:
            continue
        any_segment = True
        if consumed == grant:
            return DecisionResult(RESOURCE_LIMIT_REACHED, consumed, False, 0)
        inputs = {
            "segment_t": intersection,
            "segment_a": point[1],
            "segment_b": point[2],
            "segment_t0": intervalmath.exact(left.tone),
            "segment_t1": intervalmath.exact(right.tone),
            "segment_c0a": intervalmath.exact(left.center_a),
            "segment_c0b": intervalmath.exact(left.center_b),
            "segment_c1a": intervalmath.exact(right.center_a),
            "segment_c1b": intervalmath.exact(right.center_b),
            "segment_rho0": intervalmath.exact(left.radius_squared),
            "segment_rho1": intervalmath.exact(right.radius_squared),
            "segment_g00": intervalmath.exact(region.metric_aa),
            "segment_g01": intervalmath.exact(region.metric_ab),
            "segment_g11": intervalmath.exact(region.metric_bb),
        }
        consumed += 1
        resolved, inside, outside, branch_exact = _predicate_decision(ssa, "segment", inputs)
        if not resolved:
            all_inside = False
            all_outside = False
            continue
        all_inside = all_inside and inside
        all_outside = all_outside and outside
        if branch_exact and not exact_zero:
            exact_branch = index
        exact_zero = exact_zero or branch_exact

    if not any_segment:
        return DecisionResult(BOUNDARY_UNPROVEN, consumed, False, 0)
    if all_outside:
        return DecisionResult(OUTSIDE, consumed, False, 0)
    if all_inside and not outside_possible:
        return DecisionResult(INSIDE, consumed, exact_zero, exact_branch)
    return DecisionResult(BOUNDARY_UNPROVEN, consumed, False, 0)


def evaluate_rgb(
    ssa: EvaluationContext,
    ordinal: int,
    region: Region,
    shared_inputs: dict[str, object],
    precision: int,
    grant: int,
) -> DecisionResult:
    """Replay one point: exact-real lift, then the decision rules."""

    red, green, blue = ordinal_to_rgb(ordinal)
    inputs = dict(shared_inputs)
    inputs["r8"] = red
    inputs["g8"] = green
    inputs["b8"] = blue
    try:
        outputs = ssa.evaluate(ssa.formula.program("point"), inputs)
    except (intervalmath.UnresolvedError, SemanticFormulaError):
        return DecisionResult(BOUNDARY_UNPROVEN, 0, False, 0)
    point = (outputs["jp"], outputs["ap"], outputs["bp"])
    return decide(ssa, point, region, precision, grant)
