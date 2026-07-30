#!/usr/bin/env python3
"""Hostile contract for the controller-owned Arb provenance receipt."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
TESTS = ARB / "tests"
sys.path[:0] = [str(PROOF), str(ARB), str(TESTS)]

import executor  # noqa: E402
import pipeline  # noqa: E402
import provenance  # noqa: E402
import receipt  # noqa: E402
from region_proof_protocol import (  # noqa: E402
    BoundaryUnprovenWitnessV1,
    DecisionTranscriptV1,
    DecisionV1,
    RunClaimV1,
)
from test_pipeline import (  # noqa: E402
    _BuildBackend,
    _foreign_comparator,
    _job,
    _request,
    _static_elf,
)


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
        _capability: executor.SupportedV1,
    ) -> executor.ExecutionResultV1:
        self.requests.append(request)
        if self.result is not None:
            return self.result
        return executor.CompletedV1(
            hashlib.sha256(request.executable).digest(),
            _transcript_for_request(request, unresolved=self.unresolved),
            b"",
        )


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
            pipeline.NativeDockerBuildBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self: build_backend.probe(),
        ),
        mock.patch.object(
            pipeline.NativeDockerBuildBackendV1,
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
        mock.patch.object(receipt, "_enter_observer_cgroup_v1", return_value=None),
    )


def _execute(
    *,
    unresolved: bool = False,
    process_result: executor.ExecutionResultV1 | None = None,
) -> tuple[receipt.SourceBoundResultV1, _NativeRunBackend]:
    backend = _NativeRunBackend(unresolved=unresolved, result=process_result)
    controller, patches = _controller(_static_elf(b"source-bound-receipt"), backend)
    with patches[0], patches[1], patches[2], patches[3], patches[4]:
        return controller.execute(_request()), backend


def _tamper(value: object, field: str, replacement: object) -> object:
    clone = object.__new__(type(value))
    for name, current in vars(value).items():
        object.__setattr__(clone, name, current)
    object.__setattr__(clone, field, replacement)
    return clone


class SourceBoundReceiptTests(unittest.TestCase):
    def test_only_controller_execution_can_seal_a_receipt(self) -> None:
        result, backend = _execute()

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1)
        self.assertIs(type(result.comparator), pipeline.DiagnosticArbComparatorV1)
        self.assertEqual(result.run_claim.identity, result.claim.run_claim_identity)
        self.assertEqual(result.evidence.identity, result.claim.replay_evidence_identity)
        self.assertEqual(
            result.claim.provenance_policy_identity,
            receipt.source_bound_policy_identity_v1(),
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

    def test_receipt_uses_only_versioned_public_pipeline_verifiers(self) -> None:
        source = (ARB / "receipt.py").read_text(encoding="utf-8")

        self.assertNotIn("pipeline._sealed_build_input_bundle_is_well_bound_v1", source)
        self.assertNotIn("pipeline._build_process_bytes_v1", source)
        self.assertTrue(hasattr(pipeline, "sealed_build_input_bundle_is_well_bound_v1"))
        self.assertTrue(hasattr(pipeline, "build_process_bytes_v1"))

    def test_reference_does_not_describe_shipped_arb_receipt_as_future(self) -> None:
        documentation = (PROOF / "PROTOCOL.md").read_text(encoding="utf-8")
        prose = " ".join(documentation.split())

        self.assertIn("## Source-bound Arb replay", documentation)
        self.assertIn("SourceBoundEvaluatorReceiptV1", documentation)
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
            receipt._build_identity_v1(request, first_source, first_build),
            receipt._build_identity_v1(
                different_request,
                second_source,
                second_build,
            ),
        )

    def test_root_and_build_coordinates_are_recomputed(self) -> None:
        result, _backend = _execute()
        dag = result.evidence
        for field_name in ("source_identity", "build_identity", "run_identity", "_identity"):
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
            "docker_daemon_observation_sha256",
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
        for field_name in ("bundle_identity", "expected_sha256", "written_sha256"):
            with self.subTest(transfer=field_name):
                transfer = _tamper(
                    first.input_transfer,
                    field_name,
                    _digest(field_name),
                )
                process = _tamper(first, "input_transfer", transfer)
                build = _tamper(
                    dag.build,
                    "build_processes",
                    (process, dag.build.build_processes[1]),
                )
                self.assertFalse(
                    receipt.replay_evidence_is_well_bound_v1(
                        _tamper(dag, "build", build)
                    )
                )
        for field_name in ("expected_length", "written_length"):
            transfer = _tamper(
                first.input_transfer,
                field_name,
                first.input_transfer.expected_length + 1,
            )
            process = _tamper(first, "input_transfer", transfer)
            build = _tamper(
                dag.build,
                "build_processes",
                (process, dag.build.build_processes[1]),
            )
            self.assertFalse(
                receipt.replay_evidence_is_well_bound_v1(
                    _tamper(dag, "build", build)
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
        mutants = (
            replace(dag.invocation, executable=equal_executable_copy),
            replace(dag.invocation, argv=dag.invocation.argv + (b"ambient",)),
            replace(
                dag.invocation,
                environment=((b"LC_ALL", b"POSIX"), (b"TZ", b"UTC")),
            ),
            replace(dag.invocation, cwd=b"/tmp"),
            replace(dag.invocation, stdin=dag.invocation.stdin + b"x"),
            replace(dag.invocation, umask=0o022),
        )
        for invocation in mutants:
            self.assertFalse(
                receipt.replay_evidence_is_well_bound_v1(
                    _tamper(dag, "invocation", invocation)
                )
            )
            forged_claim = RunClaimV1.for_transcript(
                dag.request.job,
                dag.build.comparator.manifest,
                dag.transcript,
                dag.build.binary_sha256,
                pipeline.invocation_identity_v1(invocation),
                pipeline.platform_identity_v1(dag.platform),
            )
            with self.assertRaises(TypeError):
                receipt.ContentResolvedEvaluatorReplayV1(
                    dag.request,
                    dag.build,
                    invocation,
                    dag.platform,
                    dag.process,
                    dag.transcript,
                    forged_claim,
                    _token=receipt._EVIDENCE_TOKEN,
                )
        mutated_limits = replace(
            dag.request.execution_limits,
            wall_timeout_ns=dag.request.execution_limits.wall_timeout_ns - 1,
        )
        request = _tamper(dag.request, "execution_limits", mutated_limits)
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
                    _tamper(dag.platform, "platform", "foreign-linux-x86_64"),
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

    def test_crash_signal_timeout_and_oom_remain_typed_failures(self) -> None:
        binary_digest = hashlib.sha256(_static_elf(b"source-bound-receipt")).digest()
        outcomes = (
            executor.ExitNonZeroV1(binary_digest, b"", b"crash", 70),
            executor.SignaledV1(binary_digest, b"", b"", 11, True),
            executor.TimedOutV1(binary_digest, b"", b"", 60_000_000_000),
            executor.OomKilledV1(binary_digest, b"", b"", 1),
        )
        for outcome in outcomes:
            with self.subTest(outcome=type(outcome).__name__):
                result, _backend = _execute(process_result=outcome)
                self.assertIs(type(result), pipeline.ExecutionRejectedV1)
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
                os.close(read_fd)
                try:
                    child = controller.execute(request)
                    payload = (
                        f"{type(child).__name__}:"
                        f"{child.reason.value}:"
                        f"{child.detail}"
                    ).encode("utf-8")
                    os.write(write_fd, payload)
                    exit_status = 0
                except BaseException as error:
                    os.write(
                        write_fd,
                        f"ERROR:{type(error).__name__}:{error}".encode("utf-8"),
                    )
                    exit_status = 1
                finally:
                    os.close(write_fd)
                    os._exit(exit_status)
            os.close(write_fd)
            child_payload = b""
            while chunk := os.read(read_fd, 4096):
                child_payload += chunk
            os.close(read_fd)
            waited_pid, status = os.waitpid(child_pid, 0)
            parent = controller.execute(request)

        self.assertEqual(waited_pid, child_pid)
        self.assertTrue(os.WIFEXITED(status), status)
        self.assertEqual(os.WEXITSTATUS(status), 0)
        self.assertEqual(
            child_payload.decode("utf-8"),
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
