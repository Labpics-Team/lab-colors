"""Per-point semantic replay over the declared budget and digest grammars.

The replay recomputes every decision from immutable job bytes: the precision
ladder, the ordinal-prefix grant accounting, and the engine-shared exact-zero
and accounting digest grammars.  Boundary enclosures stay engine-private; the
verifier only consumes their presence and alignment.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass

import region_proof_protocol as protocol

from . import intervalmath, region
from .ssa import EvaluationContext, FoldedPointProgramV1, SemanticFormulaError, parse_formula

EXACT_TRACE_DOMAIN_V1 = b"labcolors.proof-region.exact-zero-signal-trace.v1\0"

ACCOUNTING_DOMAINS_V1 = {
    protocol.ComparatorKindV1.ARB: b"labcolors.arb-evaluation-accounting.v1\0",
    protocol.ComparatorKindV1.MPFI: b"labcolors.mpfi-evaluation-accounting.v1\0",
}


class ReplayIntegrityError(RuntimeError):
    """The independent replay contradicted its own accounting invariants."""

    def __init__(self, ordinal: int, detail: str) -> None:
        super().__init__(f"ordinal {ordinal}: {detail}")
        self.ordinal = ordinal
        self.detail = detail


@dataclass(frozen=True)
class PointReplay:
    ordinal: int
    outcome: int
    final_precision: int
    consumed: int
    point_grant: int
    resource_scope: int
    exact_boundary: bool
    exact_branch: int


def exact_trace_digest_v1(job_identity: bytes, ordinal: int, exact_branch: int) -> bytes:
    """Engine-shared grammar: one job, one ordinal, one exact branch."""

    hasher = hashlib.sha256()
    hasher.update(EXACT_TRACE_DOMAIN_V1)
    hasher.update(job_identity)
    hasher.update(ordinal.to_bytes(4, "big"))
    hasher.update(exact_branch.to_bytes(8, "big"))
    return hasher.digest()


def accounting_prefix_v1(
    kind: protocol.ComparatorKindV1,
    job: protocol.ProofJobV1,
    comparator_identity: bytes,
) -> hashlib.sha256:
    domain = ACCOUNTING_DOMAINS_V1.get(kind)
    if domain is None:
        raise ReplayIntegrityError(0, f"no accounting domain for comparator kind {kind!r}")
    hasher = hashlib.sha256()
    hasher.update(domain)
    hasher.update(job.identity)
    hasher.update(job.domain.identity)
    hasher.update(job.policy.identity)
    hasher.update(comparator_identity)
    return hasher


def account_record(ordinal: int, precision: int, consumed: int, outcome: int) -> bytes:
    return (
        ordinal.to_bytes(4, "big")
        + precision.to_bytes(4, "big")
        + consumed.to_bytes(8, "big")
        + bytes((outcome,))
    )


class SemanticReplay:
    """Stateful replay driver mirroring the engine evaluation loop."""

    def __init__(
        self,
        job: protocol.ProofJobV1,
        comparator: protocol.ContentResolvedComparatorManifestV2,
    ) -> None:
        budget = next(
            item for item in job.policy.comparators if item.kind == comparator.manifest.kind
        )
        if not budget.precision_ladder:
            raise ReplayIntegrityError(0, "comparator budget carries an empty precision ladder")
        self._job = job
        self._budget = budget
        self._formula = parse_formula(job.formula_spec)
        self._region = region.Region.from_definition(job.definition)
        self._shared_inputs = region.context_inputs(job.definition)
        # The domain ordinals are consumed sequentially, exactly like the
        # engine loop; materialising up to 2^24 integers would only waste
        # memory without changing the replay semantics.
        self._ordinals = iter(job.domain.iter_ordinals())
        self._global_remaining = budget.global_pregrant
        # One folded point program per rung: the shared context and its static
        # constants are built once, so each point only replays the dynamic
        # suffix of the lift.  A failed fold is cached as None: it poisons
        # every point lift at that rung and must not be recomputed 2^24 times.
        self._folded_rungs: dict[
            int, tuple[EvaluationContext, FoldedPointProgramV1 | None]
        ] = {}

    @property
    def budget(self) -> protocol.ComparatorBudgetV1:
        return self._budget

    def _folded_rung(
        self, rung: int
    ) -> tuple[EvaluationContext, FoldedPointProgramV1 | None]:
        cached = self._folded_rungs.get(rung)
        if cached is None:
            context = EvaluationContext(self._formula, rung, rung)
            try:
                folded = context.fold_point_program(self._shared_inputs)
            except (intervalmath.UnresolvedError, SemanticFormulaError, KeyError):
                folded = None
            cached = (context, folded)
            self._folded_rungs[rung] = cached
        return cached

    def next_point(self) -> PointReplay:
        """Replay one domain point exactly like the engine loop does."""

        try:
            ordinal = next(self._ordinals)
        except StopIteration:
            raise ReplayIntegrityError(0, "domain ordinal stream exhausted early") from None
        budget = self._budget
        point_grant = min(budget.per_point_work, self._global_remaining)
        point_remaining = point_grant
        point_consumed = 0
        resource_scope = 1 if budget.per_point_work <= self._global_remaining else 2
        # A point owns its ordinal-prefix pregrant even when it uses none.
        self._global_remaining -= point_grant
        ladder = budget.precision_ladder
        final_precision = ladder[0]
        final = region.DecisionResult(region.BOUNDARY_UNPROVEN, 0, False, 0)
        red, green, blue = region.ordinal_to_rgb(ordinal)
        for rung in ladder:
            final_precision = rung
            try:
                ssa, folded = self._folded_rung(rung)
                outputs = (
                    None
                    if folded is None
                    else ssa.evaluate_folded_point(
                        folded, self._shared_inputs, red, green, blue
                    )
                )
            except (intervalmath.UnresolvedError, SemanticFormulaError, KeyError):
                outputs = None
            if outputs is None:
                # Same admission as the unfolded lift path in
                # region.evaluate_rgb: an unresolvable lift — including one
                # poisoned by an unresolvable static fold — escalates the rung.
                final = region.DecisionResult(region.BOUNDARY_UNPROVEN, 0, False, 0)
            else:
                final = region.evaluate_rgb(
                    ssa,
                    ordinal,
                    self._region,
                    self._shared_inputs,
                    rung,
                    point_remaining,
                    lift=(outputs["jp"], outputs["ap"], outputs["bp"]),
                )
            if final.consumed_branches > point_remaining:
                raise ReplayIntegrityError(ordinal, "predicate consumed more than granted")
            point_remaining -= final.consumed_branches
            point_consumed += final.consumed_branches
            if final.outcome != region.BOUNDARY_UNPROVEN:
                break
        return PointReplay(
            ordinal=ordinal,
            outcome=final.outcome,
            final_precision=final_precision,
            consumed=point_consumed,
            point_grant=point_grant,
            resource_scope=resource_scope,
            exact_boundary=final.exact_boundary,
            exact_branch=final.exact_branch,
        )
