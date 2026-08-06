#!/usr/bin/env python3
"""Sharded corpus runner for the V5b2d full-domain RUN (V5b2d-1b).

The full sRGB8 domain carries 2^24 points; a monolithic replay would
materialise 2^24 decisions and witnesses before the transcript exists.  The
corpus runner instead replays the domain in contiguous ordinal shards, each
shard emitting wire bytes only: one decision-bit fragment per shard, the
witness wire records in strict ordinal order, and one streaming accounting
update per point.  Assembly concatenates the shard fragments in shard order,
so the assembled transcript is byte-identical to the monolithic one by
construction and never holds 2^24 Python objects at once.

The shard plan covers exactly one canonical domain range grammar: contiguous,
4-aligned windows (the decision packing is two bits per point, so every shard
boundary must land on a whole packing byte).  Any other partition is a typed
rejection, never a panic.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import Iterable, final

import region_proof_protocol as protocol

from semantic import replay as semantic_replay

CORPUS_SHARD_ALIGNMENT_V1 = 4

BOUNDARY_ENCLOSURE_DOMAIN_V1 = (
    b"labcolors.proof-region.boundary-enclosure.v1\0"
)


def boundary_enclosure_digest_v1(
    job_identity: bytes, ordinal: int, exact_branch: int
) -> bytes:
    """Corpus-shared grammar for one boundary-unproven enclosure witness."""

    hasher = hashlib.sha256()
    hasher.update(BOUNDARY_ENCLOSURE_DOMAIN_V1)
    hasher.update(job_identity)
    hasher.update(ordinal.to_bytes(4, "big"))
    hasher.update(exact_branch.to_bytes(8, "big"))
    return hasher.digest()


class ShardCorpusReasonV1(StrEnum):
    FOREIGN_INPUT = "foreign_input"
    SHARD_ORDER = "shard_order"
    INCOMPLETE_COVER = "incomplete_cover"


@dataclass(frozen=True)
class ShardCorpusRejectedV1:
    reason: ShardCorpusReasonV1
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not ShardCorpusReasonV1:
            raise TypeError("invalid shard corpus rejection reason")
        if type(self.detail) is not str or not self.detail:
            raise TypeError("invalid shard corpus rejection detail")


def _reject(reason: ShardCorpusReasonV1, detail: str) -> ShardCorpusRejectedV1:
    return ShardCorpusRejectedV1(reason, detail)


def shard_plan_v1(
    domain: protocol.ReducedDomainManifestV1,
    shard_points: int,
) -> tuple[tuple[int, int], ...] | ShardCorpusRejectedV1:
    """Contiguous 4-aligned windows covering the whole declared domain.

    The windows follow the domain's range grammar in order; every window is
    exactly `shard_points` wide except the final window of each range, which
    may be shorter but stays 4-aligned because the plan rejects any range or
    shard width that would leave a packing byte straddling a shard boundary.
    """

    if type(domain) is not protocol.ReducedDomainManifestV1:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "the shard plan requires a canonical reduced-domain manifest",
        )
    if (
        type(shard_points) is not int
        or shard_points < CORPUS_SHARD_ALIGNMENT_V1
        or shard_points % CORPUS_SHARD_ALIGNMENT_V1 != 0
    ):
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "shard width must be a positive multiple of the packing alignment",
        )
    windows: list[tuple[int, int]] = []
    for start, end in domain.ranges:
        length = end - start
        if length % CORPUS_SHARD_ALIGNMENT_V1 != 0:
            return _reject(
                ShardCorpusReasonV1.FOREIGN_INPUT,
                f"range [{start}, {end}) is not packing-aligned",
            )
        cursor = start
        while cursor < end:
            window_end = min(cursor + shard_points, end)
            windows.append((cursor, window_end))
            cursor = window_end
    return tuple(windows)


@dataclass(frozen=True)
class ShardArtifactV1:
    """Wire-only evidence of one replayed shard."""

    start_ordinal: int
    end_ordinal: int
    decision_bits: bytes
    witness_wire: bytes
    counters: tuple[int, int, int, int]
    witness_count: int

    def __post_init__(self) -> None:
        if (
            type(self.start_ordinal) is not int
            or type(self.end_ordinal) is not int
            or self.end_ordinal <= self.start_ordinal
            or self.start_ordinal < 0
            or self.end_ordinal > protocol.OUTPUT_CARDINALITY_V1
        ):
            raise TypeError("shard window is outside the sRGB8 ordinal space")
        point_count = self.end_ordinal - self.start_ordinal
        if (
            type(self.decision_bits) is not bytes
            or len(self.decision_bits) != (point_count + 3) // 4
        ):
            raise TypeError("shard decision fragment length disagrees with the window")
        if type(self.witness_wire) is not bytes:
            raise TypeError("shard witness wire must be immutable bytes")
        if (
            type(self.counters) is not tuple
            or len(self.counters) != 4
            or any(type(count) is not int or count < 0 for count in self.counters)
            or sum(self.counters) != point_count
        ):
            raise TypeError("shard counters disagree with the window")
        if type(self.witness_count) is not int or self.witness_count < 0:
            raise TypeError("shard witness count is invalid")


class ShardCorpusRunnerV1:
    """Streams one semantic replay through contiguous ordinal shards.

    The runner owns nothing but the replay cursor, the accounting hasher and
    the last consumed ordinal; every shard is consumed immediately into wire
    bytes, so memory stays bounded by the shard width regardless of the total
    point count.
    """

    def __init__(
        self,
        job: protocol.ProofJobV1,
        comparator: protocol.ContentResolvedComparatorManifestV2,
        evidence_job_identity: bytes | None = None,
        retain_records: bool = False,
    ) -> None:
        self._replay = semantic_replay.SemanticReplay(job, comparator)
        self._accounting = semantic_replay.accounting_prefix_v1(
            comparator.manifest.kind, job, comparator.identity
        )
        if evidence_job_identity is None:
            evidence_job_identity = job.identity
        if type(evidence_job_identity) is not bytes or len(evidence_job_identity) != 32:
            raise TypeError("evidence job identity must be 32 bytes")
        self._job_identity = evidence_job_identity
        self._retain_records = retain_records
        self._records = bytearray()
        self._next_ordinal: int | None = None

    @property
    def accounting_digest(self) -> bytes:
        return self._accounting.digest()

    @property
    def accounting_records(self) -> bytes:
        """Raw account_record bytes in replay order, when retention is on."""

        return bytes(self._records)

    def run_shard(self, start_ordinal: int, end_ordinal: int) -> ShardArtifactV1:
        """Replay the exact window into wire fragments, in engine order."""

        if (
            type(start_ordinal) is not int
            or type(end_ordinal) is not int
            or end_ordinal <= start_ordinal
        ):
            raise TypeError("shard window is invalid")
        if self._next_ordinal is not None and start_ordinal != self._next_ordinal:
            raise ValueError(
                f"shard window [{start_ordinal}, {end_ordinal}) breaks replay order:"
                f" next ordinal is {self._next_ordinal}"
            )
        point_count = end_ordinal - start_ordinal
        decision_bits = bytearray((point_count + 3) // 4)
        witness_wire = bytearray()
        counters = [0, 0, 0, 0]
        witness_count = 0
        for index in range(point_count):
            point = self._replay.next_point()
            if point.ordinal != start_ordinal + index:
                raise ValueError(
                    f"replay ordinal {point.ordinal} drifted from window position"
                    f" {start_ordinal + index}"
                )
            decision = protocol.DecisionV1(point.outcome)
            decision_bits[index // 4] |= int(decision) << (6 - 2 * (index % 4))
            counters[int(decision)] += 1
            if (
                decision == protocol.DecisionV1.INSIDE
                and point.exact_boundary
            ):
                witness_wire.append(1)
                witness_wire.extend(point.ordinal.to_bytes(4, "big"))
                witness_wire.extend(
                    semantic_replay.exact_trace_digest_v1(
                        self._job_identity, point.ordinal, point.exact_branch
                    )
                )
                witness_count += 1
            elif decision == protocol.DecisionV1.BOUNDARY_UNPROVEN:
                witness_wire.append(2)
                witness_wire.extend(point.ordinal.to_bytes(4, "big"))
                witness_wire.extend(
                    boundary_enclosure_digest_v1(
                        self._job_identity, point.ordinal, point.exact_branch
                    )
                )
                witness_count += 1
            elif decision == protocol.DecisionV1.RESOURCE_LIMIT_REACHED:
                witness_wire.append(3)
                witness_wire.extend(point.ordinal.to_bytes(4, "big"))
                witness_wire.append(point.resource_scope)
                witness_wire.extend(point.point_grant.to_bytes(8, "big"))
                witness_wire.extend(point.consumed.to_bytes(8, "big"))
                witness_count += 1
            record = semantic_replay.account_record(
                point.ordinal,
                point.final_precision,
                point.consumed,
                point.outcome,
            )
            if self._retain_records:
                self._records.extend(record)
            self._accounting.update(record)
        self._next_ordinal = end_ordinal
        return ShardArtifactV1(
            start_ordinal,
            end_ordinal,
            bytes(decision_bits),
            bytes(witness_wire),
            tuple(counters),
            witness_count,
        )


def assemble_transcript_from_shards_v1(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    shards: Iterable[ShardArtifactV1],
    accounting_digest: bytes,
) -> protocol.DecisionTranscriptV1 | ShardCorpusRejectedV1:
    """Seal the transcript of one domain from its shard wire fragments.

    The fragments must cover the domain's ordinals exactly, in order, with no
    gap and no overlap; the assembled decision bits and witness body are the
    plain concatenations of the shard fragments, which is byte-identical to
    the monolithic packing because every shard boundary is packing-aligned.
    """

    if type(job) is not protocol.ProofJobV1:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "transcript assembly requires a canonical proof job",
        )
    domain = job.domain
    ranges = iter(domain.ranges)
    expected_start = next(ranges, None)
    decision_parts: list[bytes] = []
    witness_parts: list[bytes] = []
    counters = [0, 0, 0, 0]
    witness_count = 0
    previous_end: int | None = None
    current_range: tuple[int, int] | None = expected_start
    for shard in shards:
        if type(shard) is not ShardArtifactV1:
            return _reject(
                ShardCorpusReasonV1.FOREIGN_INPUT,
                "transcript assembly requires canonical shard artifacts",
            )
        if current_range is None or shard.start_ordinal < current_range[0]:
            return _reject(
                ShardCorpusReasonV1.SHARD_ORDER,
                "shard escapes the domain range grammar",
            )
        if previous_end is not None and shard.start_ordinal != previous_end:
            if (
                current_range is None
                or previous_end != current_range[1]
                or shard.start_ordinal < previous_end
            ):
                return _reject(
                    ShardCorpusReasonV1.SHARD_ORDER
                    if previous_end is not None
                    and shard.start_ordinal < previous_end
                    else ShardCorpusReasonV1.INCOMPLETE_COVER,
                    "shards must cover the domain contiguously and in order",
                )
            current_range = next(ranges, None)
            if (
                current_range is None
                or shard.start_ordinal != current_range[0]
            ):
                return _reject(
                    ShardCorpusReasonV1.INCOMPLETE_COVER,
                    "shard does not start at the next domain range",
                )
        if current_range is not None and shard.end_ordinal > current_range[1]:
            return _reject(
                ShardCorpusReasonV1.SHARD_ORDER,
                "shard overruns its domain range",
            )
        decision_parts.append(shard.decision_bits)
        witness_parts.append(shard.witness_wire)
        for index in range(4):
            counters[index] += shard.counters[index]
        witness_count += shard.witness_count
        previous_end = shard.end_ordinal
    if previous_end is None:
        return _reject(
            ShardCorpusReasonV1.INCOMPLETE_COVER,
            "no shards were admitted for the domain",
        )
    if current_range is None or previous_end != current_range[1]:
        return _reject(
            ShardCorpusReasonV1.INCOMPLETE_COVER,
            "shards stop before the domain end",
        )
    if next(ranges, None) is not None:
        return _reject(
            ShardCorpusReasonV1.INCOMPLETE_COVER,
            "shards leave trailing domain ranges uncovered",
        )
    witness_store = protocol.WitnessStoreV1(b"".join(witness_parts), witness_count)
    decision_bits = b"".join(decision_parts)
    transcript = protocol.DecisionTranscriptV1(
        job.identity,
        domain.identity,
        comparator.identity,
        domain.point_count,
        decision_bits,
        tuple(counters),
        witness_store.equality_count,
        accounting_digest,
        witness_store,
    )
    protocol.validate_witness_alignment_v1(
        domain,
        decision_bits,
        domain.point_count,
        tuple(counters),
        witness_store,
    )
    return transcript


def decision_procedure_work_bound_v1(
    definition: protocol.ContextualRegionDefinitionV1,
    precision_ladder: tuple[int, ...],
) -> int:
    """Branch evaluations one point can need across the whole ladder.

    One `decide` call spends at most one predicate branch per region segment:
    a knot ladder of `n` knots has `n - 1` segments, and the degenerate
    single-knot region still needs one branch for the tone that equals its
    knot.  That cap is per rung, but the point's grant is *shared across the
    ladder*: a point that pays a branch at a low rung and stays
    `BOUNDARY_UNPROVEN` escalates and pays again at the next rung.  Budgeting
    only one rung therefore starves points that escalate past that share —
    the very points the ladder exists for.

    Sufficiency is structural, not measured: the rung loop runs at most
    `len(ladder)` times and each call consumes at most the segment count, so
    entering rung `i` with `(len(ladder) - i) * segments` left is enough for
    every remaining rung.  The bound is not claimed minimal for a given
    definition — a smaller grant may happen to suffice, but only under a
    numerical property of that definition, which is not proven here.
    """

    if type(definition) is not protocol.ContextualRegionDefinitionV1:
        raise TypeError("a work bound requires a canonical region definition")
    if (
        type(precision_ladder) is not tuple
        or not precision_ladder
        or any(type(rung) is not int for rung in precision_ladder)
    ):
        raise TypeError("a work bound requires a canonical precision ladder")
    return len(precision_ladder) * max(1, definition.knot_count - 1)


def domain_points_before_v1(
    domain: protocol.ReducedDomainManifestV1, window_start: int
) -> int:
    """Domain points the monolithic run consumes before reaching an ordinal.

    The ordinal-prefix grant is charged once per *domain point* in iteration
    order, so a lane's prefix is the number of domain points below its
    window — not the window's ordinal.  The two coincide only when the domain
    covers every ordinal below the window, which is exactly the exact full
    manifest; on any reduced domain the ordinal would overcount the prefix
    and starve the lane.
    """

    if type(domain) is not protocol.ReducedDomainManifestV1:
        raise TypeError("a domain prefix requires a canonical domain manifest")
    if type(window_start) is not int or window_start < 0:
        raise TypeError("a domain prefix requires a non-negative window start")
    points = 0
    for start, end in domain.ranges:
        if end <= window_start:
            points += end - start
        elif start < window_start:
            points += window_start - start
    return points


def certified_work_policy_v1(
    definition: protocol.ContextualRegionDefinitionV1,
    base: protocol.ProofPolicyV1,
    point_count: int,
) -> protocol.ProofPolicyV1:
    """The base policy's ladder under the work a certification actually needs.

    The work grant is not a free knob for a certified materialisation: a dual
    comparison refuses any transcript carrying an unresolved outcome, so one
    starved point is enough to make the proof unreachable.  A budget below
    `decision_procedure_work_bound_v1` is not proven to decide the domain —
    a zero grant decides no boundary point at all, and a partial one leaves
    whichever points escalate past their share on `RESOURCE_LIMIT_REACHED`.
    The derived bound is proven sufficient, not proven minimal; see
    `decision_procedure_work_bound_v1` for both halves of that claim.

    The pregrant is an absolute total over the domain's ordinal prefix rather
    than a rate, so it is re-derived for the domain being certified: every
    point owns its per-point grant whether or not it spends it.
    """

    if type(base) is not protocol.ProofPolicyV1:
        raise TypeError("a certified policy requires a canonical base policy")
    if type(point_count) is not int or point_count <= 0:
        raise TypeError("a certified policy requires a positive point count")
    budgets = []
    for budget in base.comparators:
        # Each comparator declares its own ladder, and the bound follows that
        # ladder: a shared constant would starve whichever engine escalates
        # further.
        work = decision_procedure_work_bound_v1(definition, budget.precision_ladder)
        budgets.append(
            protocol.ComparatorBudgetV1(
                budget.kind, budget.precision_ladder, work, work * point_count
            )
        )
    return protocol.ProofPolicyV1(base.equality_release, tuple(budgets))


def full_domain_job_v1(base: protocol.ProofJobV1) -> protocol.ProofJobV1:
    """The declared definition and formula certified over the exact manifest.

    The base job's work grant is deliberately not carried over.  The frozen
    protocol fixture declares a hostile zero-grant policy, and borrowing it
    for a certified run is what leaves the region's boundary points
    unresolved: the grant is checked before the predicate branch runs, so a
    starved point never decides at any precision.  A certified full-domain
    materialisation must prove every point inside or outside, so it declares
    the work its own decision procedure needs; only the precision ladder and
    the equality release stay the base job's declaration.
    """

    if type(base) is not protocol.ProofJobV1:
        raise TypeError("full-domain job derivation requires a canonical proof job")
    manifest = protocol.exact_full_domain_manifest_v1()
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        manifest,
        certified_work_policy_v1(base.definition, base.policy, manifest.point_count),
    )


def _validate_lane_window(
    window_start: int, window_points: int
) -> ShardCorpusRejectedV1 | None:
    if (
        type(window_start) is not int
        or type(window_points) is not int
        or window_start < 0
        or window_points <= 0
        or window_start % CORPUS_SHARD_ALIGNMENT_V1 != 0
        or window_points % CORPUS_SHARD_ALIGNMENT_V1 != 0
        or window_start + window_points > protocol.OUTPUT_CARDINALITY_V1
    ):
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            f"lane window [{window_start!r}, +{window_points!r}) must be a"
            f" positive packing-aligned window inside the sRGB8 ordinal space",
        )
    return None


def lane_window_job_v1(
    full_job: protocol.ProofJobV1,
    window_start: int,
    window_points: int,
    kind: protocol.ComparatorKindV1,
) -> protocol.ProofJobV1 | ShardCorpusRejectedV1:
    """The lane execution job for one window of a full-domain job.

    The window job replays the same definition and formula over the single
    window range, starting from the grant state the ordinal prefix left
    behind: every point before the window owns its per-point pregrant whether
    or not it spends it, so the window's remaining pregrant is the declared
    total minus that prefix, never a negative debt.  Reconstructing the
    prefix state — instead of assuming it is already exhausted — is what
    makes a lane byte-identical to the same window of the monolithic run in
    every grant regime, and its fragments stay bound to the full-domain job
    identity through the runner's evidence identity.
    """

    if type(full_job) is not protocol.ProofJobV1:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "a lane window job requires a canonical proof job",
        )
    if type(kind) is not protocol.ComparatorKindV1:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "a lane window job requires a typed comparator kind",
        )
    rejection = _validate_lane_window(window_start, window_points)
    if rejection is not None:
        return rejection
    # A lane claims byte-identity with the same window of the monolithic run,
    # so that window must exist in the run: one crossing a domain gap has no
    # monolithic counterpart at all and would replay ordinals the run never
    # visits.  Ranges are sorted and never adjacent, so containment means
    # containment in one range.
    if not any(
        start <= window_start and window_start + window_points <= end
        for start, end in full_job.domain.ranges
    ):
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            f"lane window [{window_start}, +{window_points}) is not contained"
            f" in one range of the job's domain",
        )
    budgets = []
    matched = False
    for budget in full_job.policy.comparators:
        if budget.kind == kind:
            # A lane replays a window of the same run, so it must start from
            # the grant state the ordinal prefix left behind: every preceding
            # point owns its pregrant whether or not it spends it.  Assuming
            # an exhausted prefix is only correct when the whole pregrant is
            # zero, and it silently starves every lane of a granted run.
            # The monolithic run charges one grant per *domain point*, so the
            # prefix is counted over the domain, not over the ordinal space —
            # the two coincide only when the domain covers every ordinal
            # below the window.
            budgets.append(
                protocol.ComparatorBudgetV1(
                    budget.kind,
                    budget.precision_ladder,
                    budget.per_point_work,
                    max(
                        0,
                        budget.global_pregrant
                        - budget.per_point_work
                        * domain_points_before_v1(full_job.domain, window_start),
                    ),
                )
            )
            matched = True
        else:
            budgets.append(budget)
    if not matched:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            f"the job policy carries no budget for comparator kind {kind!r}",
        )
    window_manifest = protocol.ReducedDomainManifestV1(
        ((window_start, window_start + window_points),), window_points
    )
    return protocol.ProofJobV1(
        full_job.definition,
        full_job.formula_spec,
        window_manifest,
        protocol.ProofPolicyV1(full_job.policy.equality_release, tuple(budgets)),
    )


@dataclass(frozen=True)
class WindowLaneArtifactV1:
    """Wire evidence of one independently replayed full-domain window."""

    window_start: int
    window_points: int
    shards: tuple[ShardArtifactV1, ...]
    counters: tuple[int, int, int, int]
    witness_count: int
    window_accounting_digest: bytes
    accounting_records: bytes

    def __post_init__(self) -> None:
        if _validate_lane_window(self.window_start, self.window_points) is not None:
            raise TypeError("lane window is invalid")
        if type(self.shards) is not tuple or not self.shards or any(
            type(shard) is not ShardArtifactV1 for shard in self.shards
        ):
            raise TypeError("lane must carry at least one canonical shard")
        if (
            type(self.counters) is not tuple
            or len(self.counters) != 4
            or sum(self.counters) != self.window_points
        ):
            raise TypeError("lane counters disagree with the window")
        if type(self.witness_count) is not int or self.witness_count < 0:
            raise TypeError("lane witness count is invalid")
        if type(self.window_accounting_digest) is not bytes or len(
            self.window_accounting_digest
        ) != 32:
            raise TypeError("lane accounting digest must be a sha256 digest")
        if type(self.accounting_records) is not bytes or len(
            self.accounting_records
        ) != 17 * self.window_points:
            raise TypeError("lane accounting records disagree with the window")


def run_window_lane_v1(
    full_job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    window_start: int,
    window_points: int,
    shard_points: int,
) -> WindowLaneArtifactV1 | ShardCorpusRejectedV1:
    """Replay one packing-aligned window of the full domain as a lane.

    The lane is prefix-independent by construction: it executes the window
    job from the grant state the ordinal prefix leaves behind and binds every
    witness digest to the full-domain job identity, so contiguous lanes
    concatenate into the exact monolithic shard stream under any declared
    budget.
    """

    if type(full_job) is not protocol.ProofJobV1:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "a lane requires a canonical full-domain proof job",
        )
    if type(comparator) is not protocol.ContentResolvedComparatorManifestV2:
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "a lane requires an admitted comparator manifest",
        )
    if (
        type(shard_points) is not int
        or shard_points < CORPUS_SHARD_ALIGNMENT_V1
        or shard_points % CORPUS_SHARD_ALIGNMENT_V1 != 0
        or type(window_points) is int
        and window_points % shard_points != 0
    ):
        return _reject(
            ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane shard width must divide the window and stay packing-aligned",
        )
    window_job = lane_window_job_v1(
        full_job, window_start, window_points, comparator.manifest.kind
    )
    if type(window_job) is not protocol.ProofJobV1:
        return window_job
    plan = shard_plan_v1(window_job.domain, shard_points)
    if type(plan) is not tuple:
        return plan
    runner = ShardCorpusRunnerV1(
        window_job,
        comparator,
        evidence_job_identity=full_job.identity,
        retain_records=True,
    )
    shards = tuple(runner.run_shard(start, end) for start, end in plan)
    counters = [0, 0, 0, 0]
    witness_count = 0
    for shard in shards:
        for index in range(4):
            counters[index] += shard.counters[index]
        witness_count += shard.witness_count
    return WindowLaneArtifactV1(
        window_start,
        window_points,
        shards,
        tuple(counters),
        witness_count,
        runner.accounting_digest,
        runner.accounting_records,
    )
