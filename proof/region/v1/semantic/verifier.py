"""Third verifier: independent semantic replay of one engine transcript.

The verifier recomputes every decision from immutable job bytes with its own
strict SSA interpreter and rigorous interval arithmetic, then seals a receipt
or rejects with the first concrete ordinal where the transcript disagrees.
It never imports engine code and never trusts engine-internal enclosures.
"""

from __future__ import annotations

import region_proof_protocol as protocol

from . import intervalmath, replay, region
from .receipt import (
    SemanticVerificationReasonV1,
    SemanticVerificationRejectedV1,
    SemanticVerificationReceiptV1,
)
from .ssa import SemanticFormulaError

VerificationResultV1 = SemanticVerificationReceiptV1 | SemanticVerificationRejectedV1


def _reject(
    reason: SemanticVerificationReasonV1,
    ordinal: int,
    detail: str,
) -> SemanticVerificationRejectedV1:
    return SemanticVerificationRejectedV1(reason, ordinal, detail)


def _foreign_binding(detail: str) -> SemanticVerificationRejectedV1:
    return _reject(SemanticVerificationReasonV1.FOREIGN_BINDING, 0, detail)


def verify_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    transcript: protocol.DecisionTranscriptV1,
    run: protocol.RunClaimV1,
) -> VerificationResultV1:
    if (
        type(job) is not protocol.ProofJobV1
        or type(comparator) is not protocol.ContentResolvedComparatorManifestV2
        or type(transcript) is not protocol.DecisionTranscriptV1
        or type(run) is not protocol.RunClaimV1
    ):
        raise TypeError("semantic verification requires canonical V1 objects")

    if transcript.job_identity != job.identity:
        return _foreign_binding("transcript binds a foreign job")
    if transcript.domain_identity != job.domain.identity:
        return _foreign_binding("transcript binds a foreign domain")
    if transcript.comparator_identity != comparator.identity:
        return _foreign_binding("transcript binds a foreign comparator")
    if run.job_identity != job.identity:
        return _foreign_binding("run claim binds a foreign job")
    if run.comparator_identity != comparator.identity:
        return _foreign_binding("run claim binds a foreign comparator")
    if run.transcript_identity != transcript.identity:
        return _foreign_binding("run claim binds a foreign transcript")
    if transcript.point_count != job.domain.point_count:
        return _foreign_binding("transcript point count drifts from the bound domain")
    # Binary, invocation and platform are declared execution coordinates.
    # Their causality belongs to the source-bound controller's receipt; the
    # semantic verifier binds the run through job, comparator and transcript
    # only and never re-declares an executable anchor it cannot observe.

    try:
        driver = replay.SemanticReplay(job, comparator)
    except (SemanticFormulaError, KeyError, StopIteration) as error:
        return _reject(
            SemanticVerificationReasonV1.REPLAY_UNRESOLVED,
            0,
            f"replay cannot be initialised: {error}",
        )

    accounting = replay.accounting_prefix_v1(
        comparator.manifest.kind,
        job,
        comparator.identity,
    )
    decisions = transcript.iter_decisions()
    witnesses = transcript.iter_witnesses()
    next_witness = next(witnesses, None)

    try:
        for expected_index in range(transcript.point_count):
            point = driver.next_point()
            ordinal = point.ordinal
            decision = next(decisions)
            if int(decision) != point.outcome:
                return _reject(
                    SemanticVerificationReasonV1.DECISION_MISMATCH,
                    ordinal,
                    (
                        f"transcript records {int(decision)}, "
                        f"semantic replay decides {point.outcome}"
                    ),
                )

            expects_exact = point.outcome == region.INSIDE and point.exact_boundary
            expects_boundary = point.outcome == region.BOUNDARY_UNPROVEN
            expects_resource = point.outcome == region.RESOURCE_LIMIT_REACHED

            if next_witness is not None and next_witness.ordinal == ordinal:
                witness = next_witness
                next_witness = next(witnesses, None)
                if expects_exact:
                    if type(witness) is not protocol.ExactZeroSignalTraceV1:
                        return _reject(
                            SemanticVerificationReasonV1.WITNESS_CONTRADICTION,
                            ordinal,
                            "exact boundary requires an exact-zero trace witness",
                        )
                    expected_digest = replay.exact_trace_digest_v1(
                        job.identity,
                        ordinal,
                        point.exact_branch,
                    )
                    if witness.trace_digest != expected_digest:
                        return _reject(
                            SemanticVerificationReasonV1.WITNESS_REPLAY_MISMATCH,
                            ordinal,
                            "exact-zero trace digest does not replay",
                        )
                elif expects_boundary:
                    if type(witness) is not protocol.BoundaryUnprovenWitnessV1:
                        return _reject(
                            SemanticVerificationReasonV1.WITNESS_CONTRADICTION,
                            ordinal,
                            "boundary outcome requires a boundary enclosure witness",
                        )
                elif expects_resource:
                    if type(witness) is not protocol.ResourceLimitWitnessV1:
                        return _reject(
                            SemanticVerificationReasonV1.WITNESS_CONTRADICTION,
                            ordinal,
                            "resource outcome requires a resource witness",
                        )
                    if witness.scope != point.resource_scope:
                        return _reject(
                            SemanticVerificationReasonV1.RESOURCE_REPLAY_MISMATCH,
                            ordinal,
                            "resource scope does not replay",
                        )
                    if witness.granted != point.point_grant:
                        return _reject(
                            SemanticVerificationReasonV1.RESOURCE_REPLAY_MISMATCH,
                            ordinal,
                            "resource grant does not replay",
                        )
                    if witness.consumed != point.consumed:
                        return _reject(
                            SemanticVerificationReasonV1.RESOURCE_REPLAY_MISMATCH,
                            ordinal,
                            "resource consumption does not replay",
                        )
                else:
                    return _reject(
                        SemanticVerificationReasonV1.WITNESS_CONTRADICTION,
                        ordinal,
                        "decisive outcome carries no witness",
                    )
            elif expects_exact or expects_boundary or expects_resource:
                return _reject(
                    SemanticVerificationReasonV1.WITNESS_REPLAY_MISMATCH,
                    ordinal,
                    "replay expects a witness the transcript does not carry",
                )

            accounting.update(
                replay.account_record(
                    ordinal,
                    point.final_precision,
                    point.consumed,
                    point.outcome,
                )
            )
    except intervalmath.UnresolvedError as error:
        return _reject(
            SemanticVerificationReasonV1.REPLAY_UNRESOLVED,
            0,
            f"semantic replay needs more guard precision: {error}",
        )
    except replay.ReplayIntegrityError as error:
        return _reject(
            SemanticVerificationReasonV1.REPLAY_UNRESOLVED,
            error.ordinal,
            error.detail,
        )

    if next_witness is not None:
        return _reject(
            SemanticVerificationReasonV1.WITNESS_CONTRADICTION,
            next_witness.ordinal,
            "transcript carries witnesses beyond the replayed domain",
        )
    if accounting.digest() != transcript.accounting_digest:
        return _reject(
            SemanticVerificationReasonV1.ACCOUNTING_REPLAY_MISMATCH,
            0,
            "accounting digest does not replay from decisions and grants",
        )
    return SemanticVerificationReceiptV1._seal(job, comparator, run, transcript)
