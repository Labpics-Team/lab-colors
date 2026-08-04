#!/usr/bin/env python3
"""Hostile contract for the dual proof join: candidate + both source-bound
provenance receipts + both independent semantic receipts seal exactly one
DualProofReceiptV1, and nothing else ever does."""

from __future__ import annotations

import hashlib
import importlib.util
import sys
import unittest
from contextlib import ExitStack
from functools import cache
from pathlib import Path
from unittest import mock

PROOF = Path(__file__).resolve().parents[1]
ARB_TESTS = PROOF / "arb" / "tests"
MPFI_TESTS = PROOF / "mpfi" / "tests"
sys.path[:0] = [str(PROOF), str(ARB_TESTS), str(MPFI_TESTS)]

import executor  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
from build import transport as build_transport  # noqa: E402

import dual_proof  # noqa: E402
from arb import receipt as arb_receipt  # noqa: E402
from mpfi import receipt as mpfi_receipt  # noqa: E402
from semantic import replay as semantic_replay  # noqa: E402
from semantic.receipt import SemanticVerificationReceiptV1  # noqa: E402
from semantic.verifier import verify_transcript  # noqa: E402


def _load_harness(name: str, path: Path) -> object:
    # The two lanes each ship a module named test_receipt; load them under
    # distinct names so both hostile harnesses stay reachable at once.
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"unreachable harness {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


ARB_PIPELINE_HARNESS = _load_harness("test_pipeline", ARB_TESTS / "test_pipeline.py")
ARB_HARNESS = _load_harness("dual_proof_arb_harness", ARB_TESTS / "test_receipt.py")
MPFI_HARNESS = _load_harness("dual_proof_mpfi_harness", MPFI_TESTS / "test_receipt.py")


def digest(label: int) -> bytes:
    return hashlib.sha256(f"dual-proof-test-{label}".encode("ascii")).digest()


@cache
def dual_domain_ordinals() -> tuple[int, ...]:
    """Fixture ordinals both lanes resolve without unresolved witnesses.

    Dual admission refuses any transcript carrying an unresolved outcome, so
    the join contract runs on the exact subset of the fixture domain whose
    points both independent replays decide inside or outside.
    """

    job = ARB_PIPELINE_HARNESS._job()
    contents = tuple(
        f"dual-proof-domain-scan-{index}".encode("ascii") for index in range(10)
    )
    manifest = protocol.ContentResolvedComparatorManifestV2.admit(
        protocol.ComparatorManifestV2(
            protocol.ComparatorKindV1.ARB,
            *(hashlib.sha256(content).digest() for content in contents),
        ),
        {
            hashlib.sha256(content).digest(): content for content in contents
        }.get,
    )
    driver = semantic_replay.SemanticReplay(job, manifest)
    ordinals: list[int] = []
    for _ in range(job.domain.point_count):
        point = driver.next_point()
        if point.outcome in (
            protocol.DecisionV1.INSIDE,
            protocol.DecisionV1.OUTSIDE,
        ):
            ordinals.append(point.ordinal)
    if not ordinals:
        raise AssertionError("fixture domain carries no resolved point")
    return tuple(ordinals)


@cache
def dual_job() -> protocol.ProofJobV1:
    base = ARB_PIPELINE_HARNESS._job()
    return protocol.ProofJobV1(
        base.definition,
        base.formula_spec,
        protocol.ReducedDomainManifestV1.from_ordinals(dual_domain_ordinals()),
        base.policy,
    )


@cache
def arb_lane_request() -> object:
    return ARB_PIPELINE_HARNESS._request(job=dual_job())


@cache
def mpfi_lane_request() -> object:
    base = MPFI_HARNESS._request()
    return MPFI_HARNESS.receipt.MpfiPipelineRequestV1(
        base.source_lock,
        base.admitted_sources,
        base.build_sources,
        base.generated_formula,
        base.build_limits,
        dual_job(),
        base.runtime_binding,
    )


def replay_transcript(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
) -> protocol.DecisionTranscriptV1:
    """Transcript bytes an honest engine would emit for this job and lane.

    The decisions, witnesses and accounting come from the independent
    semantic replay itself, so a correct engine and the verifier agree by
    construction while any mutation breaks one side of the join.
    """

    driver = semantic_replay.SemanticReplay(job, comparator)
    decisions: list[protocol.DecisionV1] = []
    witnesses: list[protocol.WitnessV1] = []
    accounting = semantic_replay.accounting_prefix_v1(
        comparator.manifest.kind, job, comparator.identity
    )
    for _ in range(job.domain.point_count):
        point = driver.next_point()
        decisions.append(protocol.DecisionV1(point.outcome))
        if point.outcome == protocol.DecisionV1.INSIDE and point.exact_boundary:
            witnesses.append(
                protocol.ExactZeroSignalTraceV1(
                    point.ordinal,
                    semantic_replay.exact_trace_digest_v1(
                        job.identity, point.ordinal, point.exact_branch
                    ),
                )
            )
        elif point.outcome == protocol.DecisionV1.BOUNDARY_UNPROVEN:
            witnesses.append(
                protocol.BoundaryUnprovenWitnessV1(
                    point.ordinal, digest(100_000 + point.ordinal)
                )
            )
        elif point.outcome == protocol.DecisionV1.RESOURCE_LIMIT_REACHED:
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


class _ReplayedRunBackend:
    """Native run backend whose child process reports the honest replay.

    The controller derives its comparator manifest from the BUILD observation
    and hands its identity to the evaluator through argv; an honest engine
    binds the transcript to exactly that manifest, so the replayed transcript
    must agree with the argv coordinate byte for byte.
    """

    def __init__(
        self,
        job: protocol.ProofJobV1,
        comparator: protocol.ContentResolvedComparatorManifestV2,
        platform: str,
    ) -> None:
        self.job = job
        self.comparator = comparator
        self.platform = platform

    def probe(self, guard: object) -> executor.SupportedV1:
        if not guard.is_current():
            raise AssertionError("controller supplied a stale probe guard")
        return executor.SupportedV1(self.platform, executor.SANDBOX_POLICY_RELEASE_V1)

    def run(
        self,
        request: executor.ExecutionRequestV1,
        _capability: executor.SupportedV1,
    ) -> executor.ExecutionResultV1:
        marker = b"--manifest-identity"
        try:
            marker_index = request.argv.index(marker)
        except ValueError as error:
            raise AssertionError("manifest identity marker is missing") from error
        if marker_index + 1 >= len(request.argv):
            raise AssertionError("manifest identity value is missing")
        argv_identity = bytes.fromhex(request.argv[marker_index + 1].decode("ascii"))
        if argv_identity != self.comparator.identity:
            raise AssertionError(
                "discovery manifest drifted from the controller-derived one"
            )
        transcript = replay_transcript(self.job, self.comparator)
        return executor.CompletedV1(
            hashlib.sha256(request.executable).digest(),
            transcript.encode(),
            b"",
        )


def mint_arb_receipt() -> arb_receipt.SourceBoundEvaluatorReceiptV1:
    # Discovery pass: the harness default backend seals a receipt whose
    # comparator manifest is the controller-derived one; the derivation never
    # reads the job, so the default request coordinates the honest pass, whose
    # transcript must replay semantically on the reduced dual domain.
    discovery, _ = ARB_HARNESS._execute()
    if type(discovery) is not arb_receipt.SourceBoundEvaluatorReceiptV1:
        raise AssertionError(f"Arb discovery pass did not seal a receipt: {discovery!r}")
    manifest = discovery.comparator.manifest
    job = dual_job()
    backend = _ReplayedRunBackend(job, manifest, executor.EXECUTION_PLATFORM_V1)
    build_backend = ARB_PIPELINE_HARNESS._BuildBackend(
        (ARB_PIPELINE_HARNESS._static_elf(b"source-bound-receipt"),) * 2
    )
    controller = arb_receipt.SourceBoundArbControllerV1(
        Path("/usr/bin/docker"),
        Path("/sys/fs/cgroup/labcolors/proof"),
    )
    patches = (
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self: build_backend.probe(),
        ),
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "run_build",
            autospec=True,
            side_effect=lambda _self, request: build_backend.run_build(request),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self, guard: backend.probe(guard),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "run",
            autospec=True,
            side_effect=lambda _self, request, capability: backend.run(
                request, capability
            ),
        ),
        mock.patch.object(executor, "enter_observer_cgroup_v1", return_value=None),
    )
    with ExitStack() as stack:
        for patch in patches:
            stack.enter_context(patch)
        result = controller.execute(arb_lane_request())
    if type(result) is not arb_receipt.SourceBoundEvaluatorReceiptV1:
        raise AssertionError(f"Arb controller did not seal a receipt: {result!r}")
    return result


def mint_mpfi_receipt() -> mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1:
    discovery, _ = MPFI_HARNESS._execute()
    if type(discovery) is not mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1:
        raise AssertionError(f"MPFI discovery pass did not seal a receipt: {discovery!r}")
    manifest = discovery.comparator.manifest
    job = dual_job()
    binary = ARB_PIPELINE_HARNESS._static_elf(b"mpfi-source-bound")
    build_backend = ARB_PIPELINE_HARNESS._BuildBackend(
        (binary, binary),
        probe=ARB_PIPELINE_HARNESS._docker_capability(
            MPFI_HARNESS.mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1
        ),
    )
    run_backend = _ReplayedRunBackend(
        job, manifest, executor.EXECUTION_PLATFORM_V1
    )
    patches = (
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self: build_backend.probe(),
        ),
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "run_build",
            autospec=True,
            side_effect=lambda _self, request: build_backend.run_build(request),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self, guard: run_backend.probe(guard),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "run",
            autospec=True,
            side_effect=lambda _self, request, capability: run_backend.run(
                request, capability
            ),
        ),
        mock.patch.object(executor, "enter_observer_cgroup_v1"),
    )
    controller = mpfi_receipt.MpfiSourceBoundControllerV1(
        Path("/usr/bin/docker"),
        Path("/sys/fs/cgroup/labcolors/proof"),
    )
    with ExitStack() as stack:
        for patch in patches:
            stack.enter_context(patch)
        result = controller.execute(mpfi_lane_request())
    if type(result) is not mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1:
        raise AssertionError(f"MPFI controller did not seal a receipt: {result!r}")
    return result


@cache
def dual_chain() -> tuple[
    protocol.ProofJobV1,
    arb_receipt.SourceBoundEvaluatorReceiptV1,
    mpfi_receipt.MpfiSourceBoundEvaluatorReceiptV1,
    SemanticVerificationReceiptV1,
    SemanticVerificationReceiptV1,
    protocol.DualComparisonCandidateV1,
]:
    """The complete honest chain: two source-bound lanes that emit what the
    independent verifier recomputes, admitted into one structural candidate."""

    job = dual_job()
    arb = mint_arb_receipt()
    mpfi = mint_mpfi_receipt()
    first_semantic = verify_transcript(
        job, arb.comparator.manifest, arb.transcript, arb.run_claim
    )
    second_semantic = verify_transcript(
        job, mpfi.comparator.manifest, mpfi.transcript, mpfi.evidence.run_claim
    )
    if type(first_semantic) is not SemanticVerificationReceiptV1:
        raise AssertionError(f"Arb transcript failed semantic replay: {first_semantic!r}")
    if type(second_semantic) is not SemanticVerificationReceiptV1:
        raise AssertionError(f"MPFI transcript failed semantic replay: {second_semantic!r}")
    candidate = protocol.compare_dual_transcripts(
        job,
        arb.comparator.manifest,
        arb.transcript,
        arb.run_claim,
        mpfi.comparator.manifest,
        mpfi.transcript,
        mpfi.evidence.run_claim,
    )
    if type(candidate) is not protocol.DualComparisonCandidateV1:
        raise AssertionError(f"dual structural admission failed: {candidate!r}")
    return job, arb, mpfi, first_semantic, second_semantic, candidate


class DualProofSealTests(unittest.TestCase):
    def test_only_the_join_can_mint_the_receipt(self) -> None:
        with self.assertRaises(TypeError):
            dual_proof.DualProofReceiptV1()
        with self.assertRaises(TypeError):
            dual_proof.DualProofReceiptV1(digest(1), digest(2), _token=object())

        with self.assertRaises(TypeError):

            class Forgery(dual_proof.DualProofReceiptV1):
                def __new__(cls, *args: object, **kwargs: object) -> "Forgery":
                    return object.__new__(cls)

    def test_rejection_coordinates_are_validated(self) -> None:
        with self.assertRaises(TypeError):
            dual_proof.DualProofRejectedV1("foreign_input", "detail")
        with self.assertRaises(TypeError):
            dual_proof.DualProofRejectedV1(
                dual_proof.DualProofRejectionReasonV1.FOREIGN_INPUT, ""
            )
        rejection = dual_proof.DualProofRejectedV1(
            dual_proof.DualProofRejectionReasonV1.FOREIGN_BINDING, "detail"
        )
        self.assertEqual(
            rejection.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_BINDING
        )


class DualProofJoinTests(unittest.TestCase):
    def test_complete_dual_chain_seals_one_receipt(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()

        proof = dual_proof.join_dual_proof_v1(candidate, arb, mpfi, first, second)
        self.assertIs(type(proof), dual_proof.DualProofReceiptV1)
        self.assertEqual(proof.claim, candidate.claim)
        self.assertEqual(proof.arb_receipt, arb)
        self.assertEqual(proof.mpfi_receipt, mpfi)
        self.assertEqual(proof.first_semantic_receipt, first)
        self.assertEqual(proof.second_semantic_receipt, second)
        self.assertTrue(proof.binds(candidate, arb, mpfi, first, second))
        self.assertFalse(proof.binds(candidate, mpfi, arb, first, second))
        self.assertFalse(proof.binds(candidate, arb, mpfi, second, first))
        self.assertFalse(proof.binds(candidate.claim, arb, mpfi, first, second))
        self.assertEqual(len(proof.identity), 32)

        repeat = dual_proof.join_dual_proof_v1(candidate, arb, mpfi, first, second)
        self.assertEqual(proof.identity, repeat.identity)

        with self.assertRaises(AttributeError):
            proof.claim = candidate.claim  # type: ignore[misc]

    def test_join_rejects_noncanonical_inputs(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()
        cases = (
            (candidate.claim, arb, mpfi, first, second),
            (candidate, candidate, mpfi, first, second),
            (candidate, arb, mpfi, second, None),
            (candidate, arb, mpfi, first, second, candidate),
            (None, arb, mpfi, first, second),
        )
        for inputs in cases:
            result = dual_proof.join_dual_proof_v1(*inputs)
            self.assertIs(type(result), dual_proof.DualProofRejectedV1, inputs)
            self.assertEqual(
                result.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_INPUT
            )

    def test_swapped_source_bound_lanes_do_not_seal(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()

        result = dual_proof.join_dual_proof_v1(candidate, mpfi, arb, first, second)
        self.assertIs(type(result), dual_proof.DualProofRejectedV1)
        self.assertEqual(
            result.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_BINDING
        )

    def test_swapped_semantic_receipts_do_not_seal(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()

        result = dual_proof.join_dual_proof_v1(candidate, arb, mpfi, second, first)
        self.assertIs(type(result), dual_proof.DualProofRejectedV1)
        self.assertEqual(
            result.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_BINDING
        )

    def test_repeated_lane_semantic_receipt_does_not_seal(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()

        for repeated in (first, second):
            result = dual_proof.join_dual_proof_v1(
                candidate, arb, mpfi, repeated, repeated
            )
            self.assertIs(type(result), dual_proof.DualProofRejectedV1)
            self.assertEqual(
                result.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_BINDING
            )

    def test_reduced_domain_proof_does_not_permit_family_mint(self) -> None:
        job, arb, mpfi, first, second, candidate = dual_chain()

        proof = dual_proof.join_dual_proof_v1(candidate, arb, mpfi, first, second)
        self.assertIs(type(proof), dual_proof.DualProofReceiptV1)
        self.assertFalse(proof.full_domain)
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(candidate.claim))

        claim = candidate.claim
        full_identity = protocol.exact_full_domain_manifest_v1().identity
        full_claim = protocol.DualComparisonClaimV1(
            claim.job_identity,
            claim.definition_digest,
            full_identity,
            claim.policy_identity,
            protocol.OUTPUT_CARDINALITY_V1,
            claim.comparator_identities,
            claim.run_claim_identities,
            claim.transcript_identities,
            claim.decision_digest,
        )
        self.assertTrue(dual_proof.claim_spans_full_domain_v1(full_claim))
        # A bare point count never authorizes the mint: the reduced-domain
        # identity proves the domain is not the exact full manifest.
        foreign_identity = protocol.DualComparisonClaimV1(
            claim.job_identity,
            claim.definition_digest,
            claim.domain_identity,
            claim.policy_identity,
            protocol.OUTPUT_CARDINALITY_V1,
            claim.comparator_identities,
            claim.run_claim_identities,
            claim.transcript_identities,
            claim.decision_digest,
        )
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(foreign_identity))
        one_short = protocol.DualComparisonClaimV1(
            claim.job_identity,
            claim.definition_digest,
            full_identity,
            claim.policy_identity,
            protocol.OUTPUT_CARDINALITY_V1 - 1,
            claim.comparator_identities,
            claim.run_claim_identities,
            claim.transcript_identities,
            claim.decision_digest,
        )
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(one_short))

        foreign = dual_proof.claim_spans_full_domain_v1(candidate)
        self.assertIs(type(foreign), dual_proof.DualProofRejectedV1)
        self.assertEqual(
            foreign.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_INPUT
        )


if __name__ == "__main__":
    unittest.main()
