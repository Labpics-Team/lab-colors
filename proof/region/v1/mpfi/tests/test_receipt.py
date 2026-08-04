#!/usr/bin/env python3
"""Hostile contract for the MPFI source-bound BUILD → RUN receipt."""

from __future__ import annotations

import hashlib
import os
import sys
import unittest
from contextlib import ExitStack
from dataclasses import replace
from functools import cache
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
TESTS = PROOF / "tests"
ARB_TESTS = PROOF / "arb/tests"
MPFI_TESTS = Path(__file__).resolve().parent
sys.path[:0] = [str(PROOF), str(TESTS), str(ARB_TESTS), str(MPFI_TESTS)]

import executor  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
from build import transport as build_transport  # noqa: E402
import provenance  # noqa: E402
from mpfi import build as mpfi_build  # noqa: E402
from mpfi import receipt, runtime as mpfi_runtime  # noqa: E402
from test_mpfi_build import (  # noqa: E402
    _generated_formula,
    _limits_for_bundle,
    _workspace_sources,
)
from test_mpfi_input import _admitted_closure  # noqa: E402
from test_pipeline import (  # noqa: E402
    _BuildBackend,
    _docker_capability,
    _job,
    _static_elf,
)
try:
    from .skip_contract import NATIVE_RECEIPT_SKIP_REASON_V1  # noqa: E402
except ImportError:  # direct gate discovery imports this module as a top-level test
    from skip_contract import NATIVE_RECEIPT_SKIP_REASON_V1  # noqa: E402


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


@cache
def _request() -> receipt.MpfiPipelineRequestV1:
    source_lock, admitted, _entries = _admitted_closure()
    sources = _workspace_sources()
    generated = _generated_formula()
    build_limits = _limits_for_bundle(source_lock, admitted, sources, generated)
    runtime_limits = executor.ExecutionLimitsV1(
        16 * 1024 * 1024,
        16 * 1024 * 1024,
        4096,
        16 * 1024 * 1024,
        64 * 1024,
        60_000_000_000,
        1024 * 1024 * 1024,
        1,
    )
    runtime_binding = mpfi_runtime.MpfiRuntimeBindingV1(
        mpfi_runtime.mpfi_runtime_profile_v1(),
        runtime_limits,
    )
    return receipt.MpfiPipelineRequestV1(
        source_lock,
        admitted,
        sources,
        generated,
        build_limits,
        _job(),
        runtime_binding,
    )


@cache
def _comparator() -> protocol.ContentResolvedComparatorManifestV2:
    contents = tuple(f"mpfi-fixture-coordinate-{index}".encode() for index in range(10))
    manifest = protocol.ComparatorManifestV2(
        protocol.ComparatorKindV1.MPFI,
        *(hashlib.sha256(content).digest() for content in contents),
    )
    by_digest = {
        hashlib.sha256(content).digest(): content for content in contents
    }
    return protocol.ContentResolvedComparatorManifestV2.admit(manifest, by_digest.get)


class _NativeRunBackend:
    def __init__(self, result: executor.ExecutionResultV1 | None = None) -> None:
        self.result = result
        self.requests: list[executor.ExecutionRequestV1] = []

    def probe(self, guard: object) -> executor.SupportedV1:
        if not guard.is_current():
            raise AssertionError("controller supplied a stale probe guard")
        return executor.SupportedV1(
            executor.EXECUTION_PLATFORM_V1,
            executor.SANDBOX_POLICY_RELEASE_V1,
        )

    def run(
        self,
        request: executor.ExecutionRequestV1,
        _capability: executor.SupportedV1,
    ) -> executor.ExecutionResultV1:
        self.requests.append(request)
        if self.result is not None:
            return self.result
        job = _job()
        transcript = protocol.DecisionTranscriptV1.from_decisions(
            job,
            _comparator(),
            tuple(protocol.DecisionV1.OUTSIDE for _ in range(job.domain.point_count)),
            (),
            _digest("accounting"),
        )
        marker = b"--manifest-identity"
        try:
            marker_index = request.argv.index(marker)
        except ValueError as error:
            raise AssertionError("manifest identity marker is missing") from error
        if marker_index + 1 >= len(request.argv):
            raise AssertionError("manifest identity value is missing")
        transcript = replace(
            transcript,
            comparator_identity=bytes.fromhex(
                request.argv[marker_index + 1].decode("ascii")
            ),
        )
        return executor.CompletedV1(
            hashlib.sha256(request.executable).digest(),
            transcript.encode(),
            b"",
        )


def _execute(
    *,
    binary: bytes | None = None,
    binaries: tuple[bytes, bytes] | None = None,
    second: bool = False,
    result: executor.ExecutionResultV1 | None = None,
) -> tuple[receipt.MpfiSourceBoundResultV1, _NativeRunBackend]:
    build_binaries = binaries or (
        binary or _static_elf(b"mpfi-source-bound"),
        binary or _static_elf(b"mpfi-source-bound"),
    )
    build_backend = _BuildBackend(
        build_binaries,
        probe=_docker_capability(mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1),
    )
    run_backend = _NativeRunBackend(result)
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
                request,
                capability,
            ),
        ),
        mock.patch.object(receipt.executor, "enter_observer_cgroup_v1"),
    )
    controller = receipt.MpfiSourceBoundControllerV1(
        Path("/usr/bin/docker"),
        Path("/sys/fs/cgroup/labcolors/proof"),
    )
    with ExitStack() as stack:
        for patch in patches:
            stack.enter_context(patch)
        first = controller.execute(_request())
        if second:
            if type(first) is not receipt.MpfiSourceBoundEvaluatorReceiptV1:
                raise AssertionError(f"first execution failed: {first!r}")
            return controller.execute(_request()), run_backend
        return first, run_backend


def _tamper(value: object, field: str, replacement: object) -> object:
    clone = object.__new__(type(value))
    for name, current in vars(value).items():
        object.__setattr__(clone, name, current)
    object.__setattr__(clone, field, replacement)
    return clone


class MpfiSourceBoundReceiptTests(unittest.TestCase):
    def test_divergent_builds_return_an_explicit_nonidentical_observation(self) -> None:
        result, _backend = _execute(
            binaries=(_static_elf(b"first-build"), _static_elf(b"second-build")),
        )
        self.assertIs(type(result), build_transport.TwoBuildObservationV1)
        self.assertIs(result.relation, build_transport.BuildByteRelationV1.DIFFERENT)

    def test_controller_seals_one_source_bound_receipt_and_consumes_authority(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        self.assertIs(type(result.claim), protocol.EvaluatorProvenanceClaimV1)
        self.assertEqual(result.comparator.manifest.manifest.kind, protocol.ComparatorKindV1.MPFI)
        self.assertTrue(receipt.replay_mpfi_evidence_is_well_bound_v1(result.evidence))

        consumed, _backend = _execute(second=True)
        self.assertIs(type(consumed), receipt.MpfiSourceBoundRejectedV1)
        self.assertEqual(
            consumed.reason,
            receipt.MpfiSourceBoundFailureReasonV1.CONTROLLER_CONSUMED,
        )

    def test_public_receipt_and_evidence_constructors_are_controller_only(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        with self.assertRaises(TypeError):
            receipt.MpfiSourceBoundEvaluatorReceiptV1(
                result.claim,
                result.evidence,
            )
        with self.assertRaises(TypeError):
            receipt.MpfiEvaluatorReplayV1(*tuple(result.evidence))
        with self.assertRaises(TypeError):
            build = result.evidence.build
            receipt.MpfiDiagnosticBuildObservationV1(
                build.source_identity,
                build.build_source_identity,
                build.generated_formula_sha256,
                build.runtime_binding_identity,
                build.docker_capability,
                build.input_bundle,
                build.binary_sha256,
                build.rebuild_sha256s,
                build.processes,
                build.binaries,
                build.comparator,
                _token=object(),
            )
        with self.assertRaises(TypeError):
            receipt.MpfiSourceBoundRejectedV1(
                "foreign-reason",
                "bounded detail",
            )
        with self.assertRaises(TypeError):
            receipt.MpfiSourceBoundRejectedV1(
                receipt.MpfiSourceBoundFailureReasonV1.REQUEST_REJECTED,
                "",
            )
        with self.assertRaises(TypeError):
            receipt.MpfiSourceBoundRejectedV1(
                receipt.MpfiSourceBoundFailureReasonV1.REQUEST_REJECTED,
                "x" * 4097,
            )

        class BadRepr:
            def __repr__(self) -> str:
                raise RuntimeError("repr failed")

        self.assertEqual(
            receipt._failure_detail_v1(BadRepr(), diagnostic_repr=True),
            "MPFI source-bound operation failed",
        )

    def test_replay_rejects_a_changed_build_transfer(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        process = result.evidence.build.processes[0]
        transfer = tuple.__new__(
            type(process.input_transfer),
            (
                process.input_transfer.bundle_identity,
                process.input_transfer.expected_length,
                process.input_transfer.expected_sha256,
                process.input_transfer.written_length - 1,
                process.input_transfer.written_sha256,
            ),
        )
        changed_process = tuple.__new__(
            type(process),
            (process.returncode, process.stdout, process.stderr, transfer),
        )
        changed_processes = (changed_process, result.evidence.build.processes[1])
        changed_build = _tamper(result.evidence.build, "processes", changed_processes)
        changed_evidence = _tamper(result.evidence, "build", changed_build)
        self.assertFalse(receipt.replay_mpfi_evidence_is_well_bound_v1(changed_evidence))

    def test_replay_rejects_a_runtime_limit_switch(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        current = result.evidence.request.runtime_binding
        changed_limits = executor.ExecutionLimitsV1(
            current.limits.max_executable_bytes,
            current.limits.max_stdin_bytes,
            current.limits.max_argument_bytes - 1,
            current.limits.max_stdout_bytes,
            current.limits.max_stderr_bytes,
            current.limits.wall_timeout_ns,
            current.limits.memory_max_bytes,
            current.limits.pids_max,
        )
        changed_binding = mpfi_runtime.MpfiRuntimeBindingV1(
            current.profile,
            changed_limits,
        )
        changed_request = receipt.MpfiPipelineRequestV1(
            result.evidence.request.source_lock,
            result.evidence.request.admitted_sources,
            result.evidence.request.build_sources,
            result.evidence.request.generated_formula,
            result.evidence.request.build_limits,
            result.evidence.request.job,
            changed_binding,
        )
        changed_evidence = _tamper(result.evidence, "request", changed_request)
        self.assertFalse(receipt.replay_mpfi_evidence_is_well_bound_v1(changed_evidence))

    def test_replay_rejects_a_changed_comparator_preimage(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        preimages = result.evidence.build.comparator.preimages
        changed_preimages = replace(preimages, exclusions=preimages.exclusions + b"!")
        changed_comparator = _tamper(
            result.evidence.build.comparator,
            "preimages",
            changed_preimages,
        )
        changed_build = _tamper(
            result.evidence.build,
            "comparator",
            changed_comparator,
        )
        changed_evidence = _tamper(result.evidence, "build", changed_build)
        self.assertFalse(receipt.replay_mpfi_evidence_is_well_bound_v1(changed_evidence))

    def test_replay_rejects_a_raw_build_identity_preimage(self) -> None:
        result, _backend = _execute()
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1)
        preimages = result.evidence.build.comparator.preimages
        changed_preimages = replace(
            preimages,
            build_identity=preimages.build_identity[:-1] + bytes((
                preimages.build_identity[-1] ^ 1,
            )),
        )
        changed_comparator = _tamper(
            result.evidence.build.comparator,
            "preimages",
            changed_preimages,
        )
        changed_build = _tamper(
            result.evidence.build,
            "comparator",
            changed_comparator,
        )
        changed_evidence = _tamper(result.evidence, "build", changed_build)
        self.assertFalse(receipt.replay_mpfi_evidence_is_well_bound_v1(changed_evidence))

    def test_malformed_generated_formula_is_rejected_before_transport(self) -> None:
        request = _request()
        malformed = receipt.MpfiPipelineRequestV1(
            request.source_lock,
            request.admitted_sources,
            request.build_sources,
            request.generated_formula[:-1]
            + bytes((request.generated_formula[-1] ^ 1,)),
            request.build_limits,
            request.job,
            request.runtime_binding,
        )
        controller = receipt.MpfiSourceBoundControllerV1(
            Path("/usr/bin/docker"),
            Path("/sys/fs/cgroup/labcolors/proof"),
        )
        with mock.patch.object(receipt.mpfi_build, "seal_mpfi_build_input_from_snapshot_v1") as seal:
            result = controller.execute(malformed)
        self.assertIs(type(result), receipt.MpfiSourceBoundRejectedV1)
        self.assertEqual(result.reason, receipt.MpfiSourceBoundFailureReasonV1.REQUEST_REJECTED)
        seal.assert_not_called()

        controller = receipt.MpfiSourceBoundControllerV1(
            Path("/usr/bin/docker"),
            Path("/sys/fs/cgroup/labcolors/proof"),
        )
        with mock.patch.object(
            receipt.mpfi_build,
            "seal_mpfi_build_input_from_snapshot_v1",
            side_effect=RuntimeError(),
        ):
            result = controller.execute(_request())
        self.assertIs(type(result), receipt.MpfiSourceBoundRejectedV1)
        self.assertEqual(result.reason, receipt.MpfiSourceBoundFailureReasonV1.REQUEST_REJECTED)
        self.assertEqual(result.detail, "MPFI source-bound operation failed")


@unittest.skipUnless(
    sys.platform == "linux"
    and all(
        os.environ.get(name)
        for name in (
            "LABCOLORS_MPFI_DOCKER",
            "LABCOLORS_EXECUTOR_CGROUP_V1",
            "LABCOLORS_GMP_ARCHIVE",
            "LABCOLORS_MPFR_ARCHIVE",
            "LABCOLORS_MPFI_ARCHIVE",
        )
    ),
    NATIVE_RECEIPT_SKIP_REASON_V1,
)
class NativeMpfiSourceBoundReceiptIntegrationTests(unittest.TestCase):
    def test_real_build_run_and_seal_are_one_source_bound_controller_execution(self) -> None:
        source_lock = provenance.mpfi_source_lock_v1()
        archive_names = (
            "LABCOLORS_GMP_ARCHIVE",
            "LABCOLORS_MPFR_ARCHIVE",
            "LABCOLORS_MPFI_ARCHIVE",
        )
        safe = tuple(
            provenance.admit_source_archive(lock, Path(os.environ[name]).read_bytes())
            for lock, name in zip(source_lock.sources, archive_names, strict=True)
        )
        admitted = provenance.admit_mpfi_sources(source_lock, safe)
        request = _request()
        build_limits = _limits_for_bundle(
            source_lock,
            admitted,
            request.build_sources,
            request.generated_formula,
        )
        request = receipt.MpfiPipelineRequestV1(
            source_lock,
            admitted,
            request.build_sources,
            request.generated_formula,
            build_limits,
            request.job,
            request.runtime_binding,
        )
        result = receipt.MpfiSourceBoundControllerV1(
            Path(os.environ["LABCOLORS_MPFI_DOCKER"]),
            Path(os.environ["LABCOLORS_EXECUTOR_CGROUP_V1"]),
        ).execute(request)
        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1, result)
        self.assertTrue(receipt.replay_mpfi_evidence_is_well_bound_v1(result.evidence))
        self.assertIs(result.evidence.build.binaries[0], result.executable)
        self.assertIs(result.evidence.invocation.executable, result.executable)
        first, second = result.evidence.build.processes
        self.assertEqual(
            first.input_transfer.bundle_identity,
            second.input_transfer.bundle_identity,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
