#!/usr/bin/env python3
"""Hostile contract for the controller-owned Arb provenance receipt."""

from __future__ import annotations

import hashlib
import os
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import fields, replace
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
TESTS = ARB / "tests"
sys.path[:0] = [str(PROOF), str(TESTS)]

from build import transport as build_transport  # noqa: E402

import executor  # noqa: E402
import provenance  # noqa: E402
from arb import pipeline, receipt  # noqa: E402
from arb import runtime as arb_runtime  # noqa: E402
from region_proof_protocol import (  # noqa: E402
    BoundaryUnprovenWitnessV1,
    ComparatorKindV1,
    ComparatorManifestV2,
    ContentResolvedComparatorManifestV2,
    DecisionTranscriptV1,
    DecisionV1,
    RunClaimV1,
)
from test_pipeline import (  # noqa: E402
    _BuildBackend,
    _docker_capability,
    _foreign_comparator,
    _job,
    _request,
    _static_elf,
)


# Test-hang ceiling only: the child uses local pipe IPC and has no product
# deadline, but a broken fork branch must not occupy CI indefinitely.
_FORK_REPORT_TIMEOUT_SECONDS = 5.0


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


def _transcript_for_request(
    request: executor.ExecutionRequestV1,
    *,
    unresolved: bool = False,
) -> bytes:
    job = _job()
    if unresolved:
        decisions = (
            DecisionV1.BOUNDARY_UNPROVEN,
            *(DecisionV1.OUTSIDE for _ in range(job.domain.point_count - 1)),
        )
        witnesses = (
            BoundaryUnprovenWitnessV1(
                next(job.domain.iter_ordinals()),
                _digest("last-enclosure"),
            ),
        )
    else:
        decisions = tuple(DecisionV1.OUTSIDE for _ in range(job.domain.point_count))
        witnesses = ()
    transcript = DecisionTranscriptV1.from_decisions(
        job,
        _foreign_comparator(),
        decisions,
        witnesses,
        _digest("accounting"),
    )
    return replace(
        transcript,
        comparator_identity=bytes.fromhex(request.argv[2].decode("ascii")),
    ).encode()


class _NativeRunBackend:
    def __init__(
        self,
        *,
        unresolved: bool = False,
        result: executor.ExecutionResultV1 | None = None,
    ) -> None:
        self.unresolved = unresolved
        self.result = result
        self.requests: list[executor.ExecutionRequestV1] = []

    def probe(self, guard: object) -> executor.CapabilityReportV1:
        if not guard.is_current():
            raise AssertionError("controller supplied a stale probe guard")
        return executor.SupportedV1("linux-x86_64", executor.SANDBOX_POLICY_RELEASE_V1)

    def run(
        self,
        request: executor.ExecutionRequestV1,
        capability: executor.SupportedV1,
    ) -> executor.ExecutionResultV1:
        self.requests.append(request)
        if self.result is not None:
            result = self.result
        else:
            result = executor.CompletedV1(
                hashlib.sha256(request.executable).digest(),
                _transcript_for_request(request, unresolved=self.unresolved),
                b"",
            )
        return result


def _controller(
    binary: bytes,
    run_backend: _NativeRunBackend,
) -> tuple[receipt.SourceBoundArbControllerV1, tuple[object, ...]]:
    build_backend = _BuildBackend((binary, binary))
    controller = receipt.SourceBoundArbControllerV1(
        Path("/usr/bin/docker"),
        Path("/sys/fs/cgroup/labcolors/proof"),
    )
    return controller, (
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
                request,
                capability,
            ),
        ),
        mock.patch.object(executor, "enter_observer_cgroup_v1", return_value=None),
    )


def _execute(
    *,
    unresolved: bool = False,
    process_result: executor.ExecutionResultV1 | None = None,
) -> tuple[receipt.SourceBoundResultV1, _NativeRunBackend]:
    backend = _NativeRunBackend(
        unresolved=unresolved,
        result=process_result,
    )
    controller, patches = _controller(_static_elf(b"source-bound-receipt"), backend)
    with patches[0], patches[1], patches[2], patches[3], patches[4]:
        return controller.execute(_request()), backend


def _tamper(value: object, field: str, replacement: object) -> object:
    clone = object.__new__(type(value))
    for name, current in vars(value).items():
        object.__setattr__(clone, name, current)
    object.__setattr__(clone, field, replacement)
    return clone


def _replace_limits(
    value: executor.ExecutionLimitsV1,
    **changes: int,
) -> executor.ExecutionLimitsV1:
    values = {
        name: getattr(value, name)
        for name in (
            "max_executable_bytes",
            "max_stdin_bytes",
            "max_argument_bytes",
            "max_stdout_bytes",
            "max_stderr_bytes",
            "wall_timeout_ns",
            "memory_max_bytes",
            "pids_max",
        )
    }
    values.update(changes)
    return executor.ExecutionLimitsV1(**values)


def _replace_runtime_binding(
    value: arb_runtime.ArbRuntimeBindingV1,
    **limit_changes: int,
) -> arb_runtime.ArbRuntimeBindingV1:
    return arb_runtime.ArbRuntimeBindingV1(
        value.profile,
        _replace_limits(value.limits, **limit_changes),
    )


def _replace_invocation(
    value: executor.ExecutionRequestV1,
    **changes: object,
) -> executor.ExecutionRequestV1:
    values: dict[str, object] = {
        "executable": value.executable,
        "argv": value.argv,
        "environment": value.environment,
        "cwd": value.cwd,
        "stdin": value.stdin,
        "umask": value.umask,
        "limits": value.limits,
    }
    values.update(changes)
    return executor.ExecutionRequestV1(**values)


class SourceBoundReceiptTests(unittest.TestCase):
    def test_public_verifier_rejects_a_top_level_switch_during_replay(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        evidence = _tamper(result.evidence, "request", result.evidence.request)
        switched_request = _request(
            runtime_binding=_replace_runtime_binding(
                result.evidence.request.runtime_binding,
                wall_timeout_ns=result.evidence.request.runtime_binding.limits.wall_timeout_ns
                - 1,
            )
        )
        real_replay = provenance.replay_admitted_source_closure_v1
        switched = False

        def replay_then_switch(
            *args: object,
            **kwargs: object,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal switched
            replayed = real_replay(*args, **kwargs)
            object.__setattr__(evidence, "request", switched_request)
            switched = True
            return replayed

        with mock.patch.object(
            provenance,
            "replay_admitted_source_closure_v1",
            side_effect=replay_then_switch,
        ):
            self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))

        self.assertTrue(switched)
        self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_public_verifier_rejects_a_nested_request_switch_during_replay(
        self,
    ) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        evidence = _tamper(result.evidence, "request", result.evidence.request)
        switched_binding = _replace_runtime_binding(
            evidence.request.runtime_binding,
            wall_timeout_ns=evidence.request.runtime_binding.limits.wall_timeout_ns - 1,
        )
        real_replay = provenance.replay_admitted_source_closure_v1
        switched = False

        def replay_then_switch(
            *args: object,
            **kwargs: object,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal switched
            replayed = real_replay(*args, **kwargs)
            object.__setattr__(evidence.request, "runtime_binding", switched_binding)
            switched = True
            return replayed

        with mock.patch.object(
            provenance,
            "replay_admitted_source_closure_v1",
            side_effect=replay_then_switch,
        ):
            self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))

        self.assertTrue(switched)
        self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_public_verifier_rejects_a_source_archive_switch_during_replay(
        self,
    ) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        evidence = _tamper(result.evidence, "request", result.evidence.request)
        source = evidence.request.admitted_sources.sources[0]
        original_archive = source.archive_bytes
        real_replay = provenance.replay_admitted_source_closure_v1
        switched = False

        def replay_then_switch(
            *args: object,
            **kwargs: object,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal switched
            replayed = real_replay(*args, **kwargs)
            object.__setattr__(source, "_archive_bytes", original_archive + b"x")
            switched = True
            return replayed

        try:
            with mock.patch.object(
                provenance,
                "replay_admitted_source_closure_v1",
                side_effect=replay_then_switch,
            ):
                self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))
        finally:
            object.__setattr__(source, "_archive_bytes", original_archive)

        self.assertTrue(switched)
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_public_verifier_rejects_a_build_repair_during_replay(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        original_binary = result.evidence.build.binary
        evidence = _tamper(
            result.evidence,
            "build",
            _tamper(result.evidence.build, "_binary", b"corrupt"),
        )
        real_replay = provenance.replay_admitted_source_closure_v1
        switched = False

        def replay_then_repair(
            *args: object,
            **kwargs: object,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal switched
            replayed = real_replay(*args, **kwargs)
            object.__setattr__(evidence.build, "_binary", original_binary)
            switched = True
            return replayed

        with mock.patch.object(
            provenance,
            "replay_admitted_source_closure_v1",
            side_effect=replay_then_repair,
        ):
            self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))

        self.assertTrue(switched)
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_public_verifier_rejects_a_process_repair_during_replay(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        evidence = _tamper(result.evidence, "process", result.evidence.process)
        original_stdout = evidence.process.stdout
        object.__setattr__(evidence.process, "stdout", b"corrupt")
        real_replay = provenance.replay_admitted_source_closure_v1
        switched = False

        def replay_then_repair(
            *args: object,
            **kwargs: object,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal switched
            replayed = real_replay(*args, **kwargs)
            object.__setattr__(evidence.process, "stdout", original_stdout)
            switched = True
            return replayed

        try:
            with mock.patch.object(
                provenance,
                "replay_admitted_source_closure_v1",
                side_effect=replay_then_repair,
            ):
                self.assertFalse(receipt.replay_evidence_is_well_bound_v1(evidence))
        finally:
            object.__setattr__(evidence.process, "stdout", original_stdout)

        self.assertTrue(switched)
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_public_verifier_rejects_a_poisoned_cached_identity_before_replay(
        self,
    ) -> None:
        """Cached identities are observable state, not an unguarded speed cache."""

        targets = (
            (
                "request definition",
                lambda evidence: evidence.request.job.definition,
                "definition_digest",
            ),
            (
                "request domain",
                lambda evidence: evidence.request.job.domain,
                "identity",
            ),
            (
                "request policy",
                lambda evidence: evidence.request.job.policy,
                "identity",
            ),
            ("request job", lambda evidence: evidence.request.job, "identity"),
            (
                "inner comparator manifest",
                lambda evidence: evidence.build.comparator.manifest.manifest,
                "identity",
            ),
            (
                "resolved comparator manifest",
                lambda evidence: evidence.build.comparator.manifest,
                "identity",
            ),
            ("transcript", lambda evidence: evidence.transcript, "identity"),
            ("run claim", lambda evidence: evidence.run_claim, "identity"),
        )
        for name, target_selector, field_name in targets:
            with self.subTest(cache=name):
                result, _backend = _execute()
                self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
                evidence = result.evidence
                target = target_selector(evidence)
                original_identity = getattr(target, field_name)
                forged_identity = _digest(f"forged {name}")
                real_replay = provenance.replay_admitted_source_closure_v1
                repaired = False
                object.__setattr__(target, field_name, forged_identity)

                def replay_then_repair(
                    *args: object,
                    **kwargs: object,
                ) -> provenance.ReplayedSourceClosureV1:
                    nonlocal repaired
                    replayed = real_replay(*args, **kwargs)
                    object.__setattr__(target, field_name, original_identity)
                    repaired = True
                    return replayed

                try:
                    with mock.patch.object(
                        provenance,
                        "replay_admitted_source_closure_v1",
                        side_effect=replay_then_repair,
                    ):
                        self.assertFalse(
                            receipt.replay_evidence_is_well_bound_v1(evidence)
                        )
                finally:
                    object.__setattr__(target, field_name, original_identity)

                self.assertFalse(repaired)
                self.assertTrue(receipt.replay_evidence_is_well_bound_v1(evidence))

    def test_source_bound_policy_identity_binds_immutable_coordinates(self) -> None:
        capability = _docker_capability()
        request = _request()
        # This golden belongs to the exact observed capability fixture; changing
        # its daemon, CLI path or host user must deliberately rederive it.
        self.assertEqual(
            receipt.source_bound_policy_identity_v3(
                capability,
                request.host_trust,
                request.runtime_binding,
            ).hex(),
            "a94f16cb61a7a1951457a4254a328bbb5eb07c66cd54d570eaf794eaa5b6bb8e",
        )

    def test_controller_uses_shared_observer_placement_and_fails_closed(self) -> None:
        backend = _NativeRunBackend()
        controller, patches = _controller(
            _static_elf(b"shared-observer-placement"),
            backend,
        )
        with patches[0], patches[1], patches[2], patches[3], mock.patch.object(
            executor,
            "enter_observer_cgroup_v1",
            return_value=None,
        ) as placement:
            result = controller.execute(_request())

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        placement.assert_called_once_with(Path("/sys/fs/cgroup/labcolors/proof"))
        self.assertFalse(hasattr(receipt, "_enter_observer_cgroup_v1"))

        failed_backend = _NativeRunBackend()
        failed_controller, failed_patches = _controller(
            _static_elf(b"shared-observer-placement-failure"),
            failed_backend,
        )
        forbidden_run_backend = mock.Mock(
            side_effect=AssertionError(
                "RUN backend must not be constructed after placement failure"
            )
        )
        placement_build_calls: list[int] = []

        def fail_placement(_parent: Path) -> None:
            placement_build_calls.append(build_runs.call_count)
            raise OSError("observer group unavailable")

        with failed_patches[0], failed_patches[1] as build_runs, failed_patches[2], failed_patches[3], mock.patch.object(
            executor,
            "enter_observer_cgroup_v1",
            side_effect=fail_placement,
        ) as placement, mock.patch.object(
            receipt,
            "_NATIVE_RUN_BACKEND_TYPE",
            new=forbidden_run_backend,
        ), mock.patch.object(
            executor,
            "NativeLinuxBackendV1",
            new=forbidden_run_backend,
        ):
            failed = failed_controller.execute(_request())

        placement.assert_called_once_with(Path("/sys/fs/cgroup/labcolors/proof"))
        self.assertEqual(placement_build_calls, [2])
        forbidden_run_backend.assert_not_called()
        self.assertEqual(
            failed,
            receipt.SourceBoundRejectedV1(
                receipt.SourceBoundFailureReasonV1.OBSERVER_PLACEMENT_FAILED,
                "dedicated controller could not enter the observer cgroup",
            ),
        )
        self.assertEqual(failed_backend.requests, [])
        self.assertEqual(
            failed_controller.execute(_request()),
            receipt.SourceBoundRejectedV1(
                receipt.SourceBoundFailureReasonV1.CONTROLLER_CONSUMED,
                "controller authority is one-shot",
            ),
        )

    def test_controller_rejects_invalid_or_nonexact_native_coordinates_on_construction(self) -> None:
        class PathSubclass(type(Path())):
            pass

        valid_docker = Path("/usr/bin/docker")
        valid_parent = Path("/sys/fs/cgroup/labcolors/proof")
        for docker_path, cgroup_parent in (
            (object(), valid_parent),
            (Path("relative"), valid_parent),
            (Path("/docker\0"), valid_parent),
            (Path("/docker\n"), valid_parent),
            (Path("/docker,comma"), valid_parent),
            (Path("/docker\ud800"), valid_parent),
            (PathSubclass("/usr/bin/docker"), valid_parent),
            (valid_docker, object()),
            (valid_docker, Path("relative")),
            (valid_docker, Path("/proof\0")),
            (valid_docker, Path("/proof\ud800")),
            (valid_docker, Path("//proof")),
            (valid_docker, PathSubclass("/proof")),
        ):
            with self.subTest(
                docker_path=type(docker_path).__name__,
                cgroup_parent=type(cgroup_parent).__name__,
            ):
                with self.assertRaises(TypeError):
                    receipt.SourceBoundArbControllerV1(  # type: ignore[arg-type]
                        docker_path,
                        cgroup_parent,
                    )

    def test_source_bound_policy_identity_consumes_explicit_trust_coordinate(self) -> None:
        capability = _docker_capability()
        request = _request()
        trust = object()
        with mock.patch.object(
            receipt.pipeline,
            "pipeline_policy_identity_v2",
            return_value=_digest("pipeline-policy"),
        ) as policy_identity:
            receipt.source_bound_policy_identity_v3(
                capability,
                trust,
                request.runtime_binding,
            )
        policy_identity.assert_called_once_with(trust, capability.policy)

    def test_identity_rejection_remains_typed_at_the_receipt_boundary(self) -> None:
        invocation_rejection = executor.ExecutionIdentityRejectedV1(
            executor.ExecutionIdentityReasonV1.REQUEST_NOT_ADMITTED,
        )
        admitted_invocation_identity = hashlib.sha256(b"admitted invocation").digest()
        with mock.patch.object(
            receipt.executor,
            "invocation_identity_v1",
            side_effect=(admitted_invocation_identity, invocation_rejection),
        ):
            result, _backend = _execute()
        self.assertEqual(
            result,
            pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                invocation_rejection,
            ),
        )

        platform_rejection = executor.ExecutionIdentityRejectedV1(
            executor.ExecutionIdentityReasonV1.FOREIGN_PLATFORM,
        )
        admitted_platform_identity = executor.platform_identity_v1(
            executor.SupportedV1(
                executor.EXECUTION_PLATFORM_V1,
                executor.SANDBOX_POLICY_RELEASE_V1,
            )
        )
        with mock.patch.object(
            receipt.executor,
            "platform_identity_v1",
            side_effect=(admitted_platform_identity, platform_rejection),
        ):
            result, _backend = _execute()
        self.assertEqual(
            result,
            pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                platform_rejection,
            ),
        )

    def test_only_controller_execution_can_seal_a_receipt(self) -> None:
        result, backend = _execute()

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertIs(type(result.comparator), pipeline.DiagnosticArbComparatorV1)
        self.assertFalse(hasattr(result.evidence, "source_closure"))
        self.assertFalse(hasattr(result.evidence, "operation"))
        self.assertEqual(result.run_claim.identity, result.claim.run_claim_identity)
        self.assertEqual(result.evidence.identity, result.claim.replay_evidence_identity)
        self.assertEqual(
            result.claim.provenance_policy_identity,
            receipt.source_bound_policy_identity_v3(
                result.evidence.build.docker_capability,
                result.evidence.request.host_trust,
                result.evidence.request.runtime_binding,
            ),
        )
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(result.evidence))
        self.assertEqual(len(backend.requests), 1)
        self.assertIs(backend.requests[0].executable, result.executable)
        self.assertIs(result.evidence.invocation.executable, result.executable)
        first, second = result.evidence.build.build_processes
        self.assertIs(first.stdout, result.evidence.build.rebuild_binaries[0])
        self.assertIs(second.stdout, result.evidence.build.rebuild_binaries[1])
        self.assertEqual(
            first.input_transfer.bundle_identity,
            second.input_transfer.bundle_identity,
        )

    def test_source_bound_controller_uses_one_source_operation_snapshot(self) -> None:
        request = _request()
        run_backend = _NativeRunBackend()
        controller, patches = _controller(
            _static_elf(b"one-source-bound-operation"),
            run_backend,
        )
        real_admit = provenance._admit_source_archive_once
        real_materialize = provenance._materialize_replayed_source_files_v1

        with (
            patches[0],
            patches[1],
            patches[2],
            patches[3],
            patches[4],
            mock.patch.object(
                provenance,
                "_admit_source_archive_once",
                wraps=real_admit,
            ) as replay,
            mock.patch.object(
                provenance,
                "_materialize_replayed_source_files_v1",
                wraps=real_materialize,
            ) as materialize,
        ):
            result = controller.execute(request)

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertEqual(len(run_backend.requests), 1)
        self.assertEqual(replay.call_count, 3)
        self.assertEqual(materialize.call_count, 3)

    def test_source_bound_snapshot_survives_source_replay_reentrancy(self) -> None:
        request = _request()
        original_domain = request.job.domain
        foreign_domain = type(original_domain).from_ordinals((0,))
        run_backend = _NativeRunBackend()
        controller, patches = _controller(
            _static_elf(b"source-bound-reentrancy"),
            run_backend,
        )
        real_replay = provenance.replay_admitted_source_closure_v1
        replay_calls = 0

        def replay_then_mutate(
            source_lock: provenance.ArbSourceLockV1,
            admitted_sources: provenance.AdmittedArbSourcesV1,
        ) -> provenance.ReplayedSourceClosureV1:
            nonlocal replay_calls
            replay_calls += 1
            snapshot = real_replay(source_lock, admitted_sources)
            object.__setattr__(request.job, "domain", foreign_domain)
            return snapshot

        try:
            with (
                patches[0],
                patches[1],
                patches[2],
                patches[3],
                patches[4],
                mock.patch.object(
                    provenance,
                    "replay_admitted_source_closure_v1",
                    side_effect=replay_then_mutate,
                ),
            ):
                result = controller.execute(request)
        finally:
            object.__setattr__(request.job, "domain", original_domain)

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertEqual(replay_calls, 1)
        self.assertEqual(result.evidence.request.job.domain, original_domain)

    def test_evidence_verifier_owns_one_fresh_source_operation_snapshot(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        real_admit = provenance._admit_source_archive_once
        real_materialize = provenance._materialize_replayed_source_files_v1

        with (
            mock.patch.object(
                provenance,
                "_admit_source_archive_once",
                wraps=real_admit,
            ) as replay,
            mock.patch.object(
                provenance,
                "_materialize_replayed_source_files_v1",
                wraps=real_materialize,
            ) as materialize,
        ):
            self.assertTrue(receipt.replay_evidence_is_well_bound_v1(result.evidence))

        self.assertEqual(replay.call_count, 3)
        self.assertEqual(materialize.call_count, 3)

    def test_no_public_object_or_diagnostic_can_mint(self) -> None:
        result, _backend = _execute()
        with self.assertRaises(TypeError):
            receipt.ContentResolvedEvaluatorReplayV1(
                result.evidence.request,
                result.evidence.build,
                result.evidence.invocation,
                result.evidence.platform,
                result.evidence.process,
                result.evidence.transcript,
                result.evidence.run_claim,
            )
        with self.assertRaises(TypeError):
            receipt.SourceBoundEvaluatorReceiptV1(result.claim, result.evidence)
        self.assertFalse(hasattr(receipt, "admit_source_bound_receipt_v1"))
        self.assertFalse(hasattr(receipt.SourceBoundArbControllerV1, "mint"))
        self.assertFalse(hasattr(pipeline, "DiagnosticPipelineObservationV1"))
        self.assertFalse(hasattr(receipt.SourceBoundEvaluatorReceiptV1, "parse"))

    def test_receipt_keeps_snapshot_only_on_the_private_operation_path(self) -> None:
        source = (ARB / "receipt.py").read_text(encoding="utf-8")

        self.assertNotIn("pipeline._sealed_build_input_bundle_is_well_bound_v1", source)
        self.assertNotIn("pipeline._build_process_bytes_v1", source)
        self.assertNotIn("executor._execution_identity_v1", source)
        self.assertNotIn("executor._enter_observer_cgroup_v1", source)
        self.assertNotIn("executor._canonical_cgroup_parent_v1", source)
        self.assertNotIn("sealed_build_input_bundle_is_well_bound_v1", source)
        self.assertIn("pipeline._seal_build_input_from_snapshot_v1", source)
        self.assertIn("pipeline._owned_arb_input_is_bound_v1", source)
        self.assertIn("pipeline._derive_arb_comparator_for_build_v1", source)
        self.assertIn("build_transport.build_process_bytes_v1", source)
        self.assertNotIn("pipeline.replay_pipeline_request_v1", source)
        self.assertIn("build_transport.docker_command_coordinate_v1", source)
        self.assertTrue(hasattr(pipeline, "arb_input_is_bound_v1"))
        self.assertTrue(hasattr(build_transport, "build_process_bytes_v1"))
        self.assertTrue(hasattr(build_transport, "docker_command_coordinate_v1"))
        self.assertTrue(hasattr(executor, "invocation_identity_v1"))
        self.assertTrue(hasattr(executor, "platform_identity_v1"))
        self.assertTrue(hasattr(executor, "canonical_cgroup_parent_v1"))
        self.assertTrue(hasattr(executor, "enter_observer_cgroup_v1"))

    def test_reference_does_not_describe_shipped_arb_receipt_as_future(self) -> None:
        documentation = (PROOF / "PROTOCOL.md").read_text(encoding="utf-8")
        prose = " ".join(documentation.split())

        self.assertIn(
            "## Воспроизведение Arb, связанное с источником",
            documentation,
        )
        self.assertIn("SourceBoundEvaluatorReceiptV1", documentation)
        self.assertIn("executor.enter_observer_cgroup_v1", documentation)
        for stale_claim in (
            "заявленные результаты будущих Arb/MPFI processes",
            "он ещё не строит и не запускает evaluator",
            "только controlled-executor slice сможет",
            "будущий source-bound receipt",
            "predicate с будущей source/build/run цепью",
            "V5b2c-0",
            "V5b2b",
            "в c0",
            "c1a",
        ):
            with self.subTest(stale_claim=stale_claim):
                self.assertNotIn(stale_claim, prose)

    def test_job_first_binds_at_run_not_source_or_build(self) -> None:
        request = _request()
        first_budget, second_budget = request.job.policy.comparators
        different_job = replace(
            request.job,
            policy=replace(
                request.job.policy,
                comparators=(
                    replace(
                        first_budget,
                        per_point_work=first_budget.per_point_work + 1,
                    ),
                    second_budget,
                ),
            ),
        )
        different_request = replace(request, job=different_job)
        self.assertNotEqual(request.job.identity, different_request.job.identity)

        first_build = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((_static_elf(b"job-independent"),) * 2)
        ).build(request)
        second_build = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((_static_elf(b"job-independent"),) * 2)
        ).build(different_request)
        self.assertIs(type(first_build), pipeline.DiagnosticBuildObservationV1)
        self.assertIs(type(second_build), pipeline.DiagnosticBuildObservationV1)

        first_source = receipt._source_identity_v1(request)
        second_source = receipt._source_identity_v1(different_request)
        self.assertEqual(first_source, second_source)
        self.assertEqual(first_build.input_bundle_identity, second_build.input_bundle_identity)
        self.assertEqual(
            receipt._build_identity_v2(request, first_source, first_build),
            receipt._build_identity_v2(
                different_request,
                second_source,
                second_build,
            ),
        )

    def test_root_and_build_coordinates_are_recomputed(self) -> None:
        result, _backend = _execute()
        dag = result.evidence
        for field_name in (
            "source_identity",
            "build_identity",
            "run_identity",
            "_identity",
        ):
            with self.subTest(root=field_name):
                self.assertFalse(
                    receipt.replay_evidence_is_well_bound_v1(
                        _tamper(dag, field_name, _digest(field_name))
                    )
                )
        for field_name in (
            "structural_source_identity",
            "build_input_identity",
            "formula_support_identity",
            "pipeline_policy_identity",
            "flint_commit_content_identity",
            "flint_project_pinned_release_only_identity",
            "binary_sha256",
            "input_bundle_identity",
            "input_bundle_sha256",
        ):
            with self.subTest(build=field_name):
                build = _tamper(dag.build, field_name, _digest(field_name))
                self.assertFalse(
                    receipt.replay_evidence_is_well_bound_v1(
                        _tamper(dag, "build", build)
                    )
                )
        different_capability = _docker_capability(
            daemon_marker=b"different-docker-daemon"
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(
                    dag,
                    "build",
                    _tamper(
                        dag.build,
                        "docker_capability",
                        different_capability,
                    ),
                )
            )
        )
        for field_name in (
            "flint_commit_content_file_count",
            "flint_project_pinned_release_only_file_count",
        ):
            build = _tamper(
                dag.build,
                field_name,
                getattr(dag.build, field_name) + 1,
            )
            self.assertFalse(
                receipt.replay_evidence_is_well_bound_v1(
                    _tamper(dag, "build", build)
                )
            )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(
                    dag,
                    "build",
                    _tamper(dag.build, "host_trust", "foreign-host-trust"),
                )
            )
        )
        build = _tamper(
            dag.build,
            "input_bundle_length",
            dag.build.input_bundle_length + 1,
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(_tamper(dag, "build", build))
        )

    def test_source_process_transfer_and_comparator_mutations_fail(self) -> None:
        result, _backend = _execute()
        dag = result.evidence
        source = dag.request.admitted_sources.sources[0]
        admitted = _tamper(
            dag.request.admitted_sources,
            "sources",
            (
                _tamper(source, "tree_identity", _digest("tree")),
                *dag.request.admitted_sources.sources[1:],
            ),
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(dag, "request", _tamper(dag.request, "admitted_sources", admitted))
            )
        )

        first = dag.build.build_processes[0]
        self.assertFalse(hasattr(first.input_transfer, "__dict__"))
        self.assertFalse(hasattr(first, "__dict__"))
        forged_transfer = tuple.__new__(
            type(first.input_transfer),
            (
                first.input_transfer.bundle_identity,
                first.input_transfer.expected_length + 1,
                first.input_transfer.expected_sha256,
                first.input_transfer.written_length,
                first.input_transfer.written_sha256,
            ),
        )
        forged_process = tuple.__new__(
            type(first),
            (
                first.returncode,
                first.stdout,
                first.stderr,
                forged_transfer,
            ),
        )
        forged_build = _tamper(
            dag.build,
            "build_processes",
            (forged_process, dag.build.build_processes[1]),
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(dag, "build", forged_build)
            )
        )

        forged_process = tuple.__new__(
            type(first),
            (
                first.returncode + 1,
                first.stdout,
                first.stderr,
                first.input_transfer,
            ),
        )
        forged_build = _tamper(
            dag.build,
            "build_processes",
            (forged_process, dag.build.build_processes[1]),
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(dag, "build", forged_build)
            )
        )

        preimages = _tamper(
            dag.build.comparator.preimages,
            "engine_release",
            b"foreign engine release",
        )
        comparator = _tamper(dag.build.comparator, "preimages", preimages)
        build = _tamper(dag.build, "comparator", comparator)
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(_tamper(dag, "build", build))
        )

        manifest = dag.build.comparator.manifest.manifest
        _ = manifest.identity
        mutated_manifest = _tamper(
            manifest,
            "engine_release",
            manifest.upstream_source,
        )
        resolved = _tamper(
            dag.build.comparator.manifest,
            "manifest",
            mutated_manifest,
        )
        comparator = _tamper(dag.build.comparator, "manifest", resolved)
        build = _tamper(dag.build, "comparator", comparator)
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(_tamper(dag, "build", build))
        )

        # Keep the BUILD preimage itself intact: a verifier that only checks
        # self-consistency would otherwise accept this fully well-formed but
        # source-unrelated comparator manifest.
        preimage_fields = fields(pipeline.ArbComparatorPreimagesV1)
        preimage_values = tuple(
            dag.build.comparator.preimages.build_identity
            if field.name == "build_identity"
            else f"forged-comparator-{index}".encode("ascii")
            for index, field in enumerate(preimage_fields)
        )
        self.assertEqual(len(set(preimage_values)), len(preimage_values))
        preimages = pipeline.ArbComparatorPreimagesV1(*preimage_values)
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(value).digest() for value in preimage_values),
        )
        by_digest = {
            hashlib.sha256(value).digest(): value for value in preimage_values
        }
        resolved = ContentResolvedComparatorManifestV2.admit(
            manifest,
            by_digest.get,
        )
        comparator = pipeline.DiagnosticArbComparatorV1(
            preimages,
            resolved,
            dag.build.structural_source_identity,
            dag.build.build_input_identity,
            dag.build.pipeline_policy_identity,
            dag.build.binary_sha256,
            dag.build.rebuild_sha256s,
            _token=pipeline._COMPARATOR_TOKEN,
        )
        build = _tamper(dag.build, "comparator", comparator)
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(_tamper(dag, "build", build))
        )

    def test_source_replay_rejects_a_self_consistent_forged_manifest(self) -> None:
        request = _request()
        lock = request.source_lock.sources[2]
        source = request.admitted_sources.sources[2]
        forged_files = list(source.files)
        forged_files[1] = replace(forged_files[1], path=forged_files[0].path)
        forged_files_value = tuple(sorted(forged_files, key=lambda item: item.path))

        with self.subTest(boundary="manifest"):
            with self.assertRaises(TypeError):
                provenance.archive_file_manifest_bytes_v1(forged_files_value)

        case_collision = list(source.files)
        case_collision[1] = replace(
            case_collision[1],
            path=case_collision[0].path.lower(),
        )
        with self.subTest(boundary="ASCII case normalization"):
            with self.assertRaises(TypeError):
                provenance.archive_file_manifest_bytes_v1(
                    tuple(sorted(case_collision, key=lambda item: item.path))
                )

        forged_source = _tamper(source, "files", forged_files_value)
        forged_source = _tamper(
            forged_source,
            "tree_identity",
            provenance._tree_identity(forged_files_value),
        )
        with self.subTest(boundary="archive replay"):
            with self.assertRaises(provenance.ProvenanceErrorV1) as caught:
                provenance.source_archive_replay_coordinates_v1(lock, forged_source)
            self.assertEqual(
                caught.exception.reason,
                provenance.ProvenanceReasonV1.FOREIGN_BINDING,
            )

    def test_invocation_process_and_same_object_mutations_fail(self) -> None:
        result, _backend = _execute()
        dag = result.evidence
        equal_executable_copy = bytes(bytearray(dag.invocation.executable))
        self.assertEqual(equal_executable_copy, dag.invocation.executable)
        self.assertIsNot(equal_executable_copy, dag.invocation.executable)
        operation = pipeline._snapshot_pipeline_operation_v1(dag.request)
        request = operation.request
        baseline = receipt.ContentResolvedEvaluatorReplayV1(
            request,
            dag.build,
            dag.invocation,
            dag.platform,
            dag.process,
            dag.transcript,
            dag.run_claim,
            _operation=operation,
            _token=receipt._EVIDENCE_TOKEN,
        )
        self.assertEqual(baseline.identity, dag.identity)
        mutants = (
            _replace_invocation(dag.invocation, executable=equal_executable_copy),
            _replace_invocation(
                dag.invocation,
                argv=dag.invocation.argv + (b"ambient",),
            ),
            _replace_invocation(
                dag.invocation,
                environment=((b"LC_ALL", b"POSIX"), (b"TZ", b"UTC")),
            ),
            _replace_invocation(dag.invocation, cwd=b"/tmp"),
            _replace_invocation(dag.invocation, stdin=dag.invocation.stdin + b"x"),
            _replace_invocation(dag.invocation, umask=0o022),
        )
        for invocation in mutants:
            self.assertFalse(
                receipt.replay_evidence_is_well_bound_v1(
                    _tamper(dag, "invocation", invocation)
                )
            )
            forged_claim = RunClaimV1.for_transcript(
                request.job,
                dag.build.comparator.manifest,
                dag.transcript,
                dag.build.binary_sha256,
                executor.invocation_identity_v1(invocation),
                executor.platform_identity_v1(dag.platform),
            )
            with self.assertRaises(TypeError):
                receipt.ContentResolvedEvaluatorReplayV1(
                    request,
                    dag.build,
                    invocation,
                    dag.platform,
                    dag.process,
                    dag.transcript,
                    forged_claim,
                    _operation=operation,
                    _token=receipt._EVIDENCE_TOKEN,
                )
        mutated_binding = _replace_runtime_binding(
            dag.request.runtime_binding,
            wall_timeout_ns=dag.request.runtime_binding.limits.wall_timeout_ns - 1,
        )
        request = _tamper(dag.request, "runtime_binding", mutated_binding)
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(dag, "request", request)
            )
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(
                    dag,
                    "process",
                    replace(dag.process, stdout=dag.process.stdout + b"x"),
                )
            )
        )
        with self.assertRaises(TypeError):
            executor.SupportedV1(
                "foreign-linux-x86_64",
                executor.SANDBOX_POLICY_RELEASE_V1,
            )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(
                    dag,
                    "platform",
                    tuple.__new__(
                        executor.SupportedV1,
                        (
                            "foreign-linux-x86_64",
                            executor.SANDBOX_POLICY_RELEASE_V1,
                        ),
                    ),
                )
            )
        )
        self.assertFalse(
            receipt.replay_evidence_is_well_bound_v1(
                _tamper(dag, "build", _tamper(dag.build, "_binary", equal_executable_copy))
            )
        )

    def test_unresolved_typed_transcript_still_gets_provenance_receipt(self) -> None:
        result, _backend = _execute(unresolved=True)
        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertEqual(result.transcript.counters[2], 1)
        self.assertEqual(result.transcript.counters[3], 0)
        self.assertFalse(hasattr(result, "mathematical_proof"))

    def test_signal_timeout_and_oom_remain_process_failures(self) -> None:
        binary_digest = hashlib.sha256(_static_elf(b"source-bound-receipt")).digest()
        outcomes = (
            executor.SignaledV1(binary_digest, b"", b"", 11, True),
            executor.TimedOutV1(binary_digest, b"", b"", 60_000_000_000),
            executor.OomKilledV1(binary_digest, b"", b"", 1),
        )
        for outcome in outcomes:
            with self.subTest(outcome=type(outcome).__name__):
                result, _backend = _execute(process_result=outcome)
                self.assertIs(type(result), pipeline.ExecutionRejectedV1)
                self.assertEqual(
                    result.reason,
                    pipeline.ExecutionFailureReasonV1.PROCESS_FAILED,
                )
                self.assertIs(result.observation, outcome)

    def test_versioned_evaluator_exit_classes_remain_distinct(self) -> None:
        binary_digest = hashlib.sha256(_static_elf(b"source-bound-receipt")).digest()
        cases = (
            (
                arb_runtime.ARB_EXIT_INPUT_REJECTED_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_INPUT_REJECTED,
            ),
            (
                arb_runtime.ARB_EXIT_INPUT_LIMIT_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_INPUT_LIMIT,
            ),
            (
                arb_runtime.ARB_EXIT_OUTPUT_LIMIT_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_OUTPUT_LIMIT,
            ),
            (
                arb_runtime.ARB_EXIT_RESOURCE_LIMIT_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_RESOURCE_LIMIT,
            ),
            (
                arb_runtime.ARB_EXIT_INTERNAL_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_INTERNAL,
            ),
            (
                arb_runtime.ARB_EXIT_IO_V1,
                pipeline.ExecutionFailureReasonV1.EVALUATOR_IO,
            ),
            (99, pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT),
        )
        for exit_code, expected in cases:
            with self.subTest(exit_code=exit_code):
                observed = executor.ExitNonZeroV1(
                    binary_digest,
                    b"",
                    b"typed evaluator failure",
                    exit_code,
                )
                result, _backend = _execute(process_result=observed)
                self.assertEqual(result.reason, expected)
                self.assertIs(result.observation, observed)

    def test_impossible_evaluator_output_is_a_backend_contract_failure(self) -> None:
        binary_digest = hashlib.sha256(_static_elf(b"source-bound-receipt")).digest()
        output_limit = _request().runtime_binding.limits.max_stdout_bytes
        outcomes = (
            executor.ExitNonZeroV1(
                binary_digest,
                b"impossible partial transcript",
                b"job rejected: bad_magic\n",
                arb_runtime.ARB_EXIT_INPUT_REJECTED_V1,
            ),
            executor.OutputLimitExceededV1(
                binary_digest,
                bytes(output_limit),
                b"",
                executor.OutputStreamV1.STDOUT,
                output_limit,
            ),
        )
        for outcome in outcomes:
            with self.subTest(outcome=type(outcome).__name__):
                result, _backend = _execute(process_result=outcome)
                self.assertEqual(
                    result.reason,
                    pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                )
                self.assertIs(result.observation, outcome)

    def test_controller_rejects_a_forked_child_without_consuming_parent_authority(
        self,
    ) -> None:
        backend = _NativeRunBackend()
        controller, patches = _controller(_static_elf(b"source-bound-receipt"), backend)
        request = _request()
        read_fd, write_fd = os.pipe()
        with patches[0], patches[1], patches[2], patches[3], patches[4]:
            child_pid = os.fork()
            if child_pid == 0:
                exit_status = 1
                try:
                    os.close(read_fd)
                    child = controller.execute(request)
                    payload = (
                        f"{type(child).__name__}:"
                        f"{child.reason.value}:"
                        f"{child.detail}"
                    ).encode()
                    os.write(write_fd, payload)
                    exit_status = 0
                except Exception as error:
                    try:
                        os.write(
                            write_fd,
                            f"ERROR:{type(error).__name__}:{error}".encode(),
                        )
                    except OSError:
                        pass
                finally:
                    try:
                        os.close(write_fd)
                    finally:
                        os._exit(exit_status)
            os.close(write_fd)
            os.set_blocking(read_fd, False)
            child_payload = bytearray()
            deadline = time.monotonic() + _FORK_REPORT_TIMEOUT_SECONDS
            timed_out = False
            try:
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        timed_out = True
                        break
                    readable, _writable, _exceptional = select.select(
                        (read_fd,), (), (), remaining
                    )
                    if not readable:
                        timed_out = True
                        break
                    chunk = os.read(read_fd, 4096)
                    if not chunk:
                        break
                    child_payload.extend(chunk)
            finally:
                os.close(read_fd)
            if timed_out:
                try:
                    os.kill(child_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            waited_pid, status = os.waitpid(child_pid, 0)
            if timed_out:
                self.fail(
                    "forked authority probe did not report within "
                    f"{_FORK_REPORT_TIMEOUT_SECONDS:g} seconds"
                )
            parent = controller.execute(request)

        self.assertEqual(waited_pid, child_pid)
        self.assertTrue(os.WIFEXITED(status), status)
        self.assertEqual(os.WEXITSTATUS(status), 0)
        self.assertEqual(
            bytes(child_payload).decode(),
            "SourceBoundRejectedV1:controller_process_changed:"
            "controller authority cannot cross a process boundary",
        )
        self.assertIs(type(parent), receipt.SourceBoundEvaluatorReceiptV1)

    def test_controller_is_one_shot(self) -> None:
        backend = _NativeRunBackend()
        controller, patches = _controller(_static_elf(b"source-bound-receipt"), backend)
        with patches[0], patches[1], patches[2], patches[3], patches[4]:
            first = controller.execute(_request())
            second = controller.execute(_request())
        self.assertIs(type(first), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertEqual(
            second,
            receipt.SourceBoundRejectedV1(
                receipt.SourceBoundFailureReasonV1.CONTROLLER_CONSUMED,
                "controller authority is one-shot",
            ),
        )


@unittest.skipUnless(
    sys.platform == "linux"
    and os.environ.get("LABCOLORS_ARB_PIPELINE_DOCKER")
    and os.environ.get("LABCOLORS_EXECUTOR_CGROUP_V1")
    and os.environ.get("LABCOLORS_GMP_ARCHIVE")
    and os.environ.get("LABCOLORS_MPFR_ARCHIVE")
    and os.environ.get("LABCOLORS_FLINT_ARCHIVE"),
    "requires Linux, Docker, a delegated cgroup, and all three exact source archives",
)
class NativeSourceBoundReceiptIntegrationTests(unittest.TestCase):
    def test_real_build_run_and_seal_are_one_source_bound_controller_execution(self) -> None:
        source_lock = provenance.arb_source_lock_v1()
        archive_names = (
            "LABCOLORS_GMP_ARCHIVE",
            "LABCOLORS_MPFR_ARCHIVE",
            "LABCOLORS_FLINT_ARCHIVE",
        )
        safe = tuple(
            provenance.admit_source_archive(lock, Path(os.environ[name]).read_bytes())
            for lock, name in zip(source_lock.sources, archive_names, strict=True)
        )
        admitted = provenance.admit_arb_sources(source_lock, safe)
        cgroup_parent = Path(os.environ["LABCOLORS_EXECUTOR_CGROUP_V1"])
        result = receipt.SourceBoundArbControllerV1(
            Path(os.environ["LABCOLORS_ARB_PIPELINE_DOCKER"]),
            cgroup_parent,
        ).execute(_request(source_lock=source_lock, admitted_sources=admitted))

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1, result)
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(result.evidence))
        self.assertIs(result.evidence.build.binary, result.executable)
        self.assertIs(result.evidence.invocation.executable, result.executable)
        first, second = result.evidence.build.build_processes
        self.assertEqual(
            first.input_transfer.bundle_identity,
            second.input_transfer.bundle_identity,
        )

        # Runtime characterization reuses receipt bytes; no third build or host
        # output pathname participates in the receipt.
        (cgroup_parent.parent / "tasks" / "cgroup.procs").write_text(
            str(os.getpid()),
            encoding="ascii",
        )
        repo = PROOF.parents[2]
        with tempfile.TemporaryDirectory(prefix="labcolors-source-bound-runtime-") as temporary:
            executable = Path(temporary) / pipeline.EVALUATOR_OUTPUT_NAME_V1
            executable.write_bytes(result.executable)
            executable.chmod(0o500)
            runtime = subprocess.run(
                (sys.executable, str(ARB / "tests" / "runtime_gate.py")),
                check=False,
                capture_output=True,
                cwd=repo,
                env={
                    "LABCOLORS_ARB_EVALUATOR": str(executable),
                    "LC_ALL": "C",
                    "PATH": os.environ.get("PATH", ""),
                    "PYTHONDONTWRITEBYTECODE": "1",
                    "PYTHONHASHSEED": "0",
                    "TZ": "UTC",
                },
                timeout=300,
            )
        self.assertEqual(
            runtime.returncode,
            0,
            (runtime.stdout + runtime.stderr).decode("utf-8", "replace"),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
