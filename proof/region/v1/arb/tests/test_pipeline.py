#!/usr/bin/env python3
"""Causal, hostile tests for the controlled Arb BUILD/RUN pipeline."""

from __future__ import annotations

import gzip
import hashlib
import io
import os
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from dataclasses import fields as dataclass_fields, replace
from functools import cache
from pathlib import Path
from types import MethodType
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
REPO = PROOF.parents[2]
sys.path.insert(0, str(PROOF))
sys.path.insert(0, str(ARB))

import executor  # noqa: E402
import pipeline  # noqa: E402
import provenance  # noqa: E402
from region_proof_protocol import (  # noqa: E402
    ComparatorKindV1,
    ComparatorManifestV1,
    ContentResolvedComparatorManifestV1,
    DecisionTranscriptV1,
    DecisionV1,
    ProofJobV1,
    ProtocolErrorV1,
)


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


def _static_elf(payload: bytes = b"fixture") -> bytes:
    """Return a parseable static ELF64/x86-64 object for executor admission."""

    code_offset = 64 + 56
    body = payload or b"x"
    file_size = code_offset + len(body)
    ident = b"\x7fELF\x02\x01\x01" + bytes(9)
    header = ident + struct.pack(
        "<HHIQQQIHHHHHH",
        2,
        62,
        1,
        0x400000 + code_offset,
        64,
        0,
        0,
        64,
        56,
        1,
        0,
        0,
        0,
    )
    program = struct.pack(
        "<IIQQQQQQ",
        1,
        5,
        0,
        0x400000,
        0x400000,
        file_size,
        file_size,
        0x1000,
    )
    return header + program + body


def _tar(root: str, files: tuple[tuple[str, bytes, int], ...]) -> tuple[bytes, int]:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        directories = {root}
        for relative, _body, _mode in files:
            parent = Path(relative).parent
            while str(parent) not in ("", "."):
                directories.add(f"{root}/{parent.as_posix()}")
                parent = parent.parent
        for name in sorted(directories, key=lambda item: (item.count("/"), item)):
            member = tarfile.TarInfo(f"{name}/")
            member.type = tarfile.DIRTYPE
            member.mode = 0o755
            member.mtime = 0
            archive.addfile(member)
        for relative, body, mode in files:
            member = tarfile.TarInfo(f"{root}/{relative}")
            member.mode = mode
            member.size = len(body)
            member.mtime = 0
            archive.addfile(member, io.BytesIO(body))
    encoded = gzip.compress(raw.getvalue(), compresslevel=9, mtime=0)
    return encoded, len(raw.getvalue())


@cache
def _source_fixture() -> tuple[
    provenance.ArbSourceLockV1,
    provenance.AdmittedArbSourcesV1,
]:
    locks: list[provenance.SourceReleaseLockV1] = []
    safe: list[provenance.SafeSourceArchiveV1] = []
    coordinates = (
        (provenance.SourceRoleV1.GMP, "gmp-6.3.0", False),
        (provenance.SourceRoleV1.MPFR, "mpfr-4.2.2", False),
        (provenance.SourceRoleV1.FLINT_ARB, "flint-3.6.0", True),
    )
    for index, (role, root, git) in enumerate(coordinates, start=1):
        files = (("LICENSE", f"license-{index}".encode(), 0o644),)
        if git:
            files += (("configure", b"generated", 0o755),)
        archive, raw_length = _tar(root, files)
        if git:
            integrity: provenance.SourceIntegrityPolicyV1 = provenance.GitContentRelationPolicyV1(
                "https://example.invalid/flint.git",
                "v1",
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                1,
                ("ci/omitted",),
                (
                    provenance.ProjectPinnedReleaseOnlyFileV1(
                        "configure",
                        0o755,
                        len(b"generated"),
                        hashlib.sha256(b"generated").digest(),
                    ),
                ),
            )
        else:
            integrity = provenance.DetachedSignaturePolicyV1(
                f"https://example.invalid/{root}.tar.gz.sig",
                3,
                _digest(f"signature-{index}"),
                _digest(f"public-key-{index}"),
                bytes((index,)) * 20,
            )
        lock = provenance.SourceReleaseLockV1(
            role,
            "1",
            f"https://example.invalid/{root}.tar.gz",
            provenance.ArchiveFormatV1.TAR_GZIP,
            len(archive),
            hashlib.sha256(archive).digest(),
            raw_length,
            f"{root}/",
            len(files),
            sum(len(body) for _name, body, _mode in files),
            (
                provenance.LegalFileV1(
                    "LICENSE",
                    len(files[0][1]),
                    hashlib.sha256(files[0][1]).digest(),
                ),
            ),
            integrity,
        )
        locks.append(lock)
        safe.append(provenance.admit_source_archive(lock, archive))
    source_lock = provenance.ArbSourceLockV1(tuple(locks))
    admitted = provenance.admit_arb_sources(source_lock, tuple(safe))
    return source_lock, admitted


@cache
def _generated_formula() -> bytes:
    result = subprocess.run(
        (
            sys.executable,
            str(ARB / "evaluator/formula.py"),
            str(REPO / "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"),
        ),
        check=False,
        capture_output=True,
        env={"PYTHONDONTWRITEBYTECODE": "1", "PYTHONHASHSEED": "0"},
    )
    if result.returncode != 0:
        raise AssertionError(result.stderr.decode("utf-8", "replace"))
    return result.stdout


@cache
def _build_sources() -> pipeline.AdmittedBuildSourcesV1:
    files = []
    for logical_path, mode in pipeline.REQUIRED_BUILD_SOURCE_MODES_V1:
        if logical_path == pipeline.GENERATED_FORMULA_PATH_V1:
            body = _generated_formula()
        else:
            body = (REPO / logical_path).read_bytes()
        files.append(pipeline.BuildSourceFileV1(logical_path, mode, body))
    return pipeline.admit_build_sources_v1(tuple(files))


@cache
def _job() -> ProofJobV1:
    return ProofJobV1.parse((PROOF / "fixtures/proof-job-v1.bin").read_bytes())


@cache
def _foreign_comparator() -> ContentResolvedComparatorManifestV1:
    content = tuple(f"manifest-coordinate-{index}".encode() for index in range(10))
    manifest = ComparatorManifestV1(
        ComparatorKindV1.ARB,
        *(hashlib.sha256(item).digest() for item in content),
    )
    by_digest = {hashlib.sha256(item).digest(): item for item in content}
    return ContentResolvedComparatorManifestV1.admit(manifest, by_digest.get)


@cache
def _transcript(
    manifest_identity: bytes = _digest("foreign-manifest-identity"),
) -> bytes:
    job = _job()
    transcript = DecisionTranscriptV1.from_decisions(
        job,
        _foreign_comparator(),
        (DecisionV1.OUTSIDE for _ in range(job.domain.point_count)),
        (),
        _digest("accounting"),
    )
    encoded = bytearray(transcript.encode())
    encoded[72:104] = manifest_identity
    return bytes(encoded)


def _limits() -> executor.ExecutionLimitsV1:
    return executor.ExecutionLimitsV1(
        max_executable_bytes=16 * 1024 * 1024,
        max_stdin_bytes=16 * 1024 * 1024,
        max_argument_bytes=4096,
        max_stdout_bytes=16 * 1024 * 1024,
        max_stderr_bytes=64 * 1024,
        wall_timeout_ns=60_000_000_000,
        memory_max_bytes=1024 * 1024 * 1024,
        pids_max=1,
    )


def _request(**changes: object) -> pipeline.PipelineRequestV1:
    source_lock, admitted = _source_fixture()
    values: dict[str, object] = {
        "source_lock": source_lock,
        "admitted_sources": admitted,
        "build_sources": _build_sources(),
        "job": _job(),
        "execution_limits": _limits(),
        "host_trust": pipeline.HostTrustBoundaryV1.PERSISTENT_SELF_HOSTED_DOCKER,
    }
    values.update(changes)
    return pipeline.PipelineRequestV1(**values)


class _BuildBackend:
    def __init__(
        self,
        outputs: tuple[bytes, ...],
        *,
        probe: pipeline.DockerCapabilityReportV1 | None = None,
        mutate_inputs: bool = False,
        hardlink_input: bool = False,
        symlink_output: bool = False,
        reported_stdout: bytes | None = None,
    ) -> None:
        self.outputs = list(outputs)
        self.probe_result = probe or pipeline.DockerSupportedV1(
            pipeline.OCI_IMAGE_REFERENCE_V1,
            pipeline.OCI_PLATFORM_V1,
            _digest("docker-daemon"),
        )
        self.mutate_inputs = mutate_inputs
        self.hardlink_input = hardlink_input
        self.symlink_output = symlink_output
        self.reported_stdout = reported_stdout
        self.requests: list[pipeline.DockerBuildRequestV1] = []

    def probe(self) -> pipeline.DockerCapabilityReportV1:
        return self.probe_result

    def run_build(
        self,
        request: pipeline.DockerBuildRequestV1,
    ) -> pipeline.DockerBuildProcessObservationV1:
        self.requests.append(request)
        output = self.outputs.pop(0)
        target = request.output_directory / pipeline.EVALUATOR_OUTPUT_NAME_V1
        if self.symlink_output:
            outside = request.root_directory / "outside"
            outside.write_bytes(output)
            target.symlink_to(outside)
        else:
            target.write_bytes(output)
            target.chmod(0o555)
        if self.mutate_inputs:
            victim = request.workspace_directory / "proof/region/v1/arb/evaluator/main.c"
            victim.chmod(0o644)
            victim.write_bytes(b"mutated")
        if self.hardlink_input:
            victim = request.workspace_directory / "proof/region/v1/arb/evaluator/main.c"
            outside = request.root_directory / "input-hardlink"
            outside.write_bytes(victim.read_bytes())
            outside.chmod(0o644)
            victim.unlink()
            os.link(outside, victim)
        return pipeline.DockerBuildExitedV1(
            0,
            self.reported_stdout
            if self.reported_stdout is not None
            else b"sha256:" + _digest("self-reported-output").hex().encode(),
            b"",
        )


class _Executor:
    def __init__(self, result_factory: object | None = None) -> None:
        self.requests: list[executor.ExecutionRequestV1] = []
        self.results: list[executor.ExecutionResultV1] = []
        self.result_factory = result_factory

    def probe(self) -> executor.CapabilityReportV1:
        return executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )

    def execute(self, request: executor.ExecutionRequestV1) -> executor.ExecutionResultV1:
        self.requests.append(request)
        if self.result_factory is not None:
            result = self.result_factory(request)
        else:
            manifest_identity = bytes.fromhex(request.argv[2].decode("ascii"))
            result = executor.CompletedV1(
                hashlib.sha256(request.executable).digest(),
                _transcript(manifest_identity),
                b"",
            )
        self.results.append(result)
        return result


class _MasqueradingControlledExecutor(executor.ControlledExecutorV1):
    pass


class _MasqueradingNativeBackend(executor.NativeLinuxBackendV1):
    def probe(self) -> executor.CapabilityReportV1:
        return executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )

    def run(self, request: executor.ExecutionRequestV1) -> executor.ExecutionResultV1:
        manifest_identity = bytes.fromhex(request.argv[2].decode("ascii"))
        return executor.CompletedV1(
            hashlib.sha256(request.executable).digest(),
            _transcript(manifest_identity),
            b"",
        )


class _SelfMutatingExecutionBackend:
    owner: executor.ControlledExecutorV1

    def probe(self) -> executor.CapabilityReportV1:
        return executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )

    def run(self, request: executor.ExecutionRequestV1) -> executor.ExecutionResultV1:
        self.owner._backend = executor.NativeLinuxBackendV1(
            Path("/sys/fs/cgroup/labcolors")
        )
        manifest_identity = bytes.fromhex(request.argv[2].decode("ascii"))
        return executor.CompletedV1(
            hashlib.sha256(request.executable).digest(),
            _transcript(manifest_identity),
            b"",
        )


class BuildSourceAdmissionTests(unittest.TestCase):
    def test_exact_formula_generator_recipe_and_evaluator_bytes_are_admitted(self) -> None:
        admitted = _build_sources()

        self.assertEqual(admitted.formula_spec, _job().formula_spec)
        self.assertEqual(
            hashlib.sha256(admitted.generated_formula).hexdigest(),
            pipeline.GENERATED_FORMULA_SHA256_V1,
        )
        self.assertEqual(
            tuple(item.path for item in admitted.files),
            tuple(path for path, _mode in pipeline.REQUIRED_BUILD_SOURCE_MODES_V1),
        )
        self.assertNotEqual(admitted.build_input_identity, admitted.formula_support_identity)
        self.assertFalse(hasattr(admitted, "source_path"))

    def test_missing_extra_reordered_or_mutated_source_bytes_are_rejected(self) -> None:
        files = _build_sources().files
        mutants = (
            files[:-1],
            files + (pipeline.BuildSourceFileV1("extra.c", 0o644, b"x"),),
            tuple(reversed(files)),
            (replace(files[0], contents=files[0].contents + b"x"),) + files[1:],
        )
        for mutant in mutants:
            with self.subTest(length=len(mutant)):
                with self.assertRaises(pipeline.BuildSourceAdmissionErrorV1):
                    pipeline.admit_build_sources_v1(mutant)

    def test_capabilities_cannot_be_directly_forged(self) -> None:
        with self.assertRaises(TypeError):
            pipeline.AdmittedBuildSourcesV1(
                _build_sources().files,
                _digest("forged"),
                _token=object(),
            )


class FlintSourcePartitionTests(unittest.TestCase):
    def test_partition_is_nonempty_and_separately_binds_both_content_sets(self) -> None:
        source_lock, admitted = _source_fixture()
        flint = source_lock.sources[2]

        partition = pipeline.flint_source_content_partition_v1(
            source_lock,
            admitted,
        )

        self.assertGreater(partition.commit_content_file_count, 0)
        self.assertGreater(partition.project_pinned_release_only_file_count, 0)
        self.assertEqual(
            partition.commit_content_file_count,
            flint.integrity.common_file_count,
        )
        self.assertEqual(
            partition.project_pinned_release_only_file_count,
            len(flint.integrity.project_pinned_release_only_files),
        )
        self.assertEqual(
            partition.commit_content_file_count
            + partition.project_pinned_release_only_file_count,
            admitted.sources[2].regular_file_count,
        )
        self.assertNotEqual(
            partition.commit_content_identity,
            partition.project_pinned_release_only_identity,
        )
        self.assertFalse(hasattr(partition, "commit_derived_identity"))

    def test_partition_rejects_a_foreign_lock_replay(self) -> None:
        source_lock, admitted = _source_fixture()
        foreign_flint = replace(
            source_lock.sources[2],
            version="foreign-release",
        )
        foreign_lock = provenance.ArbSourceLockV1(
            source_lock.sources[:2] + (foreign_flint,)
        )

        with self.assertRaises(pipeline.PipelineInputErrorV1) as caught:
            pipeline.flint_source_content_partition_v1(foreign_lock, admitted)

        self.assertEqual(
            caught.exception.reason,
            pipeline.PipelineInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )


class ComparatorDerivationTests(unittest.TestCase):
    def _result(self) -> pipeline.DiagnosticPipelineObservationV1:
        binary = _static_elf(b"derived-comparator")
        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=_Executor(),
        ).execute(_request())
        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        return result

    def test_request_cannot_supply_an_arbitrary_comparator(self) -> None:
        with self.assertRaises(TypeError):
            _request(comparator=_foreign_comparator())

    def test_all_ten_coordinates_replay_exact_named_preimages(self) -> None:
        result = self._result()
        admitted = result.comparator
        manifest = admitted.manifest.manifest

        names = tuple(field.name for field in dataclass_fields(admitted.preimages))
        self.assertEqual(
            names,
            (
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
            ),
        )
        for name in names:
            with self.subTest(name=name):
                preimage = getattr(admitted.preimages, name)
                self.assertGreater(len(preimage), len(name))
                self.assertNotEqual(preimage, name.encode("ascii"))
                self.assertEqual(
                    getattr(manifest, name),
                    hashlib.sha256(preimage).digest(),
                )
        self.assertEqual(admitted.identity, admitted.manifest.identity)
        self.assertEqual(
            admitted.structural_source_identity,
            result.structural_source_identity,
        )
        self.assertEqual(admitted.build_input_identity, result.build_input_identity)
        self.assertEqual(admitted.pipeline_policy_identity, result.pipeline_policy_identity)
        self.assertEqual(admitted.binary_sha256, result.binary_sha256)
        self.assertEqual(admitted.rebuild_sha256s, result.rebuild_sha256s)

    def test_mutated_or_reordered_preimages_cannot_replay_the_manifest(self) -> None:
        admitted = self._result().comparator
        manifest = admitted.manifest.manifest
        original = {
            getattr(manifest, field.name): getattr(admitted.preimages, field.name)
            for field in dataclass_fields(admitted.preimages)
        }
        variants = []
        mutated = dict(original)
        mutated[manifest.evaluator_source] += b"x"
        variants.append(mutated)
        reordered = dict(original)
        reordered[manifest.wrapper_source], reordered[manifest.evaluator_source] = (
            reordered[manifest.evaluator_source],
            reordered[manifest.wrapper_source],
        )
        variants.append(reordered)

        for resolver in variants:
            with self.subTest(variant=variants.index(resolver)):
                with self.assertRaises(ProtocolErrorV1):
                    ContentResolvedComparatorManifestV1.admit(
                        manifest,
                        resolver.get,
                    )

    def test_operator_coordinate_is_the_exact_ordered_formula_contract(self) -> None:
        original = _build_sources().formula_spec
        lines = original.splitlines()
        count_index = lines.index(b"operators 20")
        lines[count_index + 1], lines[count_index + 2] = (
            lines[count_index + 2],
            lines[count_index + 1],
        )
        reordered = b"\n".join(lines) + b"\n"

        original_preimage = pipeline._operation_allowlist_preimage_v1(original)
        reordered_preimage = pipeline._operation_allowlist_preimage_v1(reordered)

        self.assertNotEqual(original_preimage, reordered_preimage)
        self.assertNotEqual(
            hashlib.sha256(original_preimage).digest(),
            hashlib.sha256(reordered_preimage).digest(),
        )

    def test_wrapper_and_evaluator_file_sets_are_exact_and_disjoint(self) -> None:
        result = self._result()
        files = _build_sources().files
        wrapper_paths = frozenset(
            (
                "proof/region/v1/arb/evaluator/formula.h",
                "proof/region/v1/arb/evaluator/interval.c",
                "proof/region/v1/arb/evaluator/interval.h",
            )
        )
        excluded = wrapper_paths | {
            pipeline.FORMULA_SPEC_PATH_V1,
            pipeline.FORMULA_GENERATOR_PATH_V1,
            pipeline.BUILD_RECIPE_PATH_V1,
        }
        wrapper_files = tuple(item for item in files if item.path in wrapper_paths)
        evaluator_files = tuple(item for item in files if item.path not in excluded)

        self.assertFalse({item.path for item in wrapper_files} & {item.path for item in evaluator_files})
        self.assertEqual(
            result.comparator.preimages.wrapper_source,
            pipeline._encoded_build_file_set_v1(
                b"labcolors.proof-region.arb-comparator.wrapper-source.v1\0",
                wrapper_files,
            ),
        )
        self.assertEqual(
            result.comparator.preimages.evaluator_source,
            pipeline._encoded_build_file_set_v1(
                b"labcolors.proof-region.arb-comparator.evaluator-source.v1\0",
                evaluator_files,
            ),
        )

    def test_build_stdout_cannot_supply_a_foreign_manifest_or_coordinate(self) -> None:
        foreign = _foreign_comparator()
        report = b"manifest=" + foreign.identity.hex().encode("ascii")
        binary = _static_elf(b"ignore-build-self-report")

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend(
                (binary, binary),
                reported_stdout=report,
            ),
            executor=_Executor(),
        ).execute(_request())

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        self.assertNotEqual(result.comparator.identity, foreign.identity)
        coordinates = tuple(
            getattr(result.comparator.manifest.manifest, field.name)
            for field in dataclass_fields(result.comparator.manifest.manifest)
            if field.name != "kind"
        )
        self.assertNotIn(foreign.identity, coordinates)
        self.assertEqual(result.build_processes[0].stdout, report)

    def test_foreign_comparator_transcript_is_rejected(self) -> None:
        binary = _static_elf(b"foreign-transcript")
        run = _Executor(
            lambda request: executor.CompletedV1(
                hashlib.sha256(request.executable).digest(),
                _transcript(_foreign_comparator().identity),
                b"",
            )
        )

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=run,
        ).execute(_request())

        self.assertIs(type(result), pipeline.TranscriptRejectedV1)
        self.assertEqual(result.reason, pipeline.TranscriptFailureReasonV1.FOREIGN_BINDING)

    def test_diagnostic_comparator_has_no_public_constructor(self) -> None:
        with self.assertRaises(TypeError):
            pipeline.DiagnosticArbComparatorV1()


class CausalPipelineTests(unittest.TestCase):
    def test_pipeline_policy_identity_binds_the_snapshot_timestamp_policy(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.PERSISTENT_SELF_HOSTED_DOCKER
        original = pipeline.pipeline_policy_identity_v1(trust)

        with mock.patch.object(
            pipeline.snapshot,
            "SOURCE_SNAPSHOT_MTIME_NS_V1",
            pipeline.snapshot.SOURCE_SNAPSHOT_MTIME_NS_V1 + 1,
        ):
            changed = pipeline.pipeline_policy_identity_v1(trust)

        self.assertNotEqual(original, changed)

    def test_build_only_does_not_probe_or_execute_run_backend(self) -> None:
        binary = _static_elf(b"build-only")
        controller = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=object(),
        )

        result = controller.build(_request())

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
        self.assertEqual(result.binary, binary)
        self.assertEqual(result.rebuild_sha256s, (result.binary_sha256,) * 2)
        self.assertIs(type(result.comparator), pipeline.DiagnosticArbComparatorV1)

    def test_two_fresh_equal_builds_feed_exact_observed_bytes_to_executor(self) -> None:
        binary = _static_elf(b"observed-output")
        build = _BuildBackend((binary, binary))
        run = _Executor()
        controller = pipeline.ControlledPipelineV1(build_backend=build, executor=run)

        result = controller.execute(_request())

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        self.assertEqual(len(build.requests), 2)
        self.assertEqual(tuple(item.attempt for item in build.requests), (1, 2))
        self.assertNotEqual(
            build.requests[0].root_directory,
            build.requests[1].root_directory,
        )
        self.assertTrue(
            all(not item.root_directory.exists() for item in build.requests),
            "fresh build roots must be removed after post-exit observation",
        )
        self.assertEqual(len(run.requests), 1)
        self.assertIs(run.requests[0].executable, result.binary)
        self.assertEqual(result.binary, binary)
        self.assertEqual(result.binary_sha256, hashlib.sha256(binary).digest())
        self.assertEqual(
            result.rebuild_sha256s,
            (result.binary_sha256, result.binary_sha256),
        )
        self.assertNotEqual(
            result.binary_sha256,
            _digest("self-reported-output"),
        )
        self.assertIs(result.transcript_bytes, run.results[0].stdout)
        self.assertEqual(
            result.transcript_bytes,
            _transcript(result.comparator.identity),
        )
        self.assertEqual(result.transcript.encode(), result.transcript_bytes)
        self.assertEqual(result.run_claim.binary_identity, result.binary_sha256)
        self.assertEqual(result.run_claim.transcript_identity, result.transcript.identity)
        self.assertEqual(
            result.structural_source_identity,
            _request().admitted_sources.identity,
        )
        partition = pipeline.flint_source_content_partition_v1(
            _request().source_lock,
            _request().admitted_sources,
        )
        self.assertEqual(
            result.flint_commit_content_identity,
            partition.commit_content_identity,
        )
        self.assertEqual(
            result.flint_project_pinned_release_only_identity,
            partition.project_pinned_release_only_identity,
        )
        self.assertEqual(
            result.flint_commit_content_file_count,
            partition.commit_content_file_count,
        )
        self.assertEqual(
            result.flint_project_pinned_release_only_file_count,
            partition.project_pinned_release_only_file_count,
        )
        self.assertEqual(result.build_input_identity, _build_sources().build_input_identity)
        self.assertEqual(
            result.formula_support_identity,
            _build_sources().formula_support_identity,
        )
        self.assertEqual(
            result.pipeline_policy_identity,
            pipeline.pipeline_policy_identity_v1(result.host_trust),
        )
        self.assertFalse(hasattr(result, "build_observer_kind"))
        self.assertFalse(hasattr(result, "run_observer_kind"))
        self.assertFalse(hasattr(result, "build_source_identity"))
        self.assertFalse(hasattr(result, "build_policy_identity"))
        self.assertFalse(hasattr(result, "commit_derived_source_identity"))
        self.assertEqual(result.host_trust, pipeline.HostTrustBoundaryV1.PERSISTENT_SELF_HOSTED_DOCKER)
        self.assertEqual(result.oci_image_reference, pipeline.OCI_IMAGE_REFERENCE_V1)
        self.assertEqual(result.oci_platform, pipeline.OCI_PLATFORM_V1)
        self.assertFalse(hasattr(result, "slsa_level"))
        self.assertFalse(hasattr(result, "fresh_vm"))

    def test_builds_must_be_byte_identical_before_any_run(self) -> None:
        first = _static_elf(b"first")
        second = _static_elf(b"second")
        run = _Executor()

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((first, second)),
            executor=run,
        ).execute(_request())

        self.assertEqual(
            result,
            pipeline.NonReproducibleBuildV1(
                hashlib.sha256(first).digest(),
                hashlib.sha256(second).digest(),
            ),
        )
        self.assertEqual(run.requests, [])

    def test_build_input_mutation_or_symlink_output_is_typed_failure(self) -> None:
        binary = _static_elf()
        cases = (
            (
                _BuildBackend((binary,), mutate_inputs=True),
                pipeline.BuildFailureReasonV1.INPUT_CHANGED,
            ),
            (
                _BuildBackend((binary,), hardlink_input=True),
                pipeline.BuildFailureReasonV1.INPUT_CHANGED,
            ),
            (
                _BuildBackend((binary,), symlink_output=True),
                pipeline.BuildFailureReasonV1.INVALID_OUTPUT,
            ),
        )
        for backend, reason in cases:
            with self.subTest(reason=reason):
                run = _Executor()
                result = pipeline.ControlledPipelineV1(
                    build_backend=backend,
                    executor=run,
                ).execute(_request())
                self.assertIs(type(result), pipeline.BuildRejectedV1)
                self.assertEqual(result.attempt, 1)
                self.assertEqual(result.reason, reason)
                self.assertEqual(run.requests, [])

    def test_docker_inability_to_observe_build_edge_is_a_design_blocker(self) -> None:
        build = _BuildBackend(
            (),
            probe=pipeline.DockerUnsupportedV1(
                pipeline.DockerBlockerReasonV1.SAME_OBJECT_OUTPUT_UNAVAILABLE,
                "post-exit owned-byte observation unavailable",
            ),
        )
        run = _Executor()

        result = pipeline.ControlledPipelineV1(
            build_backend=build,
            executor=run,
        ).execute(_request())

        self.assertEqual(
            result,
            pipeline.PipelineBlockedV1(
                pipeline.DockerBlockerReasonV1.SAME_OBJECT_OUTPUT_UNAVAILABLE,
                "post-exit owned-byte observation unavailable",
            ),
        )
        self.assertEqual(build.requests, [])
        self.assertEqual(run.requests, [])

    def test_job_that_exceeds_exact_run_limits_is_rejected_before_build(self) -> None:
        with self.assertRaises(pipeline.PipelineInputErrorV1) as caught:
            _request(
                execution_limits=replace(
                    _limits(),
                    max_stdin_bytes=1,
                )
            )

        self.assertEqual(
            caught.exception.reason,
            pipeline.PipelineInputReasonV1.EXECUTION_LIMIT_MISMATCH,
        )

    def test_snapshot_modes_are_normalized_independently_of_host_umask(self) -> None:
        binary = _static_elf(b"umask-independent")
        previous = os.umask(0o077)
        try:
            result = pipeline.ControlledPipelineV1(
                build_backend=_BuildBackend((binary, binary)),
                executor=_Executor(),
            ).execute(_request())
        finally:
            os.umask(previous)

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)

    def test_binary_digest_from_executor_must_match_the_owned_build_object(self) -> None:
        binary = _static_elf()
        run = _Executor(
            lambda _request: executor.CompletedV1(
                _digest("foreign-binary"),
                _transcript(),
                b"",
            )
        )

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=run,
        ).execute(_request())

        self.assertIs(type(result), pipeline.ExecutionRejectedV1)
        self.assertEqual(result.reason, pipeline.ExecutionFailureReasonV1.BINARY_MISMATCH)

    def test_only_completed_empty_stderr_canonical_bound_transcript_is_admitted(self) -> None:
        binary = _static_elf()
        foreign = bytearray(_transcript())
        foreign[16] ^= 1
        cases = (
            (
                lambda request: executor.ExitNonZeroV1(
                    hashlib.sha256(request.executable).digest(), b"", b"failed", 7
                ),
                pipeline.ExecutionRejectedV1,
            ),
            (
                lambda request: executor.CompletedV1(
                    hashlib.sha256(request.executable).digest(), _transcript(), b"warning"
                ),
                pipeline.ExecutionRejectedV1,
            ),
            (
                lambda request: executor.CompletedV1(
                    hashlib.sha256(request.executable).digest(), bytes(foreign), b""
                ),
                pipeline.TranscriptRejectedV1,
            ),
        )
        for factory, expected_type in cases:
            with self.subTest(expected_type=expected_type):
                result = pipeline.ControlledPipelineV1(
                    build_backend=_BuildBackend((binary, binary)),
                    executor=_Executor(factory),
                ).execute(_request())
                self.assertIs(type(result), expected_type)

    def test_controller_derives_exact_invocation_without_backend_metadata(self) -> None:
        binary = _static_elf()
        run = _Executor()
        request = _request()

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=run,
        ).execute(request)

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        invocation = run.requests[0]
        self.assertEqual(
            invocation.argv,
            (
                b"arb-evaluator",
                b"--manifest-identity",
                result.comparator.identity.hex().encode("ascii"),
                b"--job",
                b"/dev/stdin",
            ),
        )
        self.assertEqual(invocation.environment, ((b"LC_ALL", b"C"), (b"TZ", b"UTC")))
        self.assertEqual(invocation.cwd, b"/")
        self.assertEqual(invocation.stdin, request.job.encode())
        self.assertEqual(invocation.umask, 0o077)
        self.assertEqual(
            result.invocation_identity,
            pipeline.invocation_identity_v1(invocation),
        )

    def test_pipeline_exports_no_receipt_or_self_report_admission_api(self) -> None:
        names = dir(pipeline)
        self.assertFalse(any("Receipt" in name for name in names))
        self.assertFalse(hasattr(pipeline.ControlledPipelineV1, "admit_report"))
        self.assertFalse(hasattr(pipeline.ControlledPipelineV1, "mint"))

    def test_fake_executor_wrapper_or_native_subclass_stays_diagnostic(self) -> None:
        binary = _static_elf(b"run-kind")
        wrapped = _MasqueradingControlledExecutor(
            _MasqueradingNativeBackend()
        )
        exact_executor_with_fake_native = executor.ControlledExecutorV1(
            _MasqueradingNativeBackend()
        )
        for run in (_Executor(), wrapped, exact_executor_with_fake_native):
            with self.subTest(executor_type=type(run).__name__):
                result = pipeline.ControlledPipelineV1(
                    build_backend=_BuildBackend((binary, binary)),
                    executor=run,
                ).execute(_request())

                self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
                self.assertFalse(hasattr(result, "run_observer_kind"))

    def test_native_observer_promotion_is_not_representable_in_v1(self) -> None:
        for name in (
            "BuildObserverKindV1",
            "RunObserverKindV1",
            "build_observer_kind_v1",
            "run_observer_kind_v1",
            "NativePipelineObservationV1",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(pipeline, name))

    def test_self_mutating_executor_cannot_upgrade_fabricated_run(self) -> None:
        binary = _static_elf(b"self-mutating-run")
        backend = _SelfMutatingExecutionBackend()
        run = executor.ControlledExecutorV1(backend)
        backend.owner = run

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
            executor=run,
        ).execute(_request())

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        self.assertFalse(hasattr(result, "run_observer_kind"))

    def test_mutable_exact_native_build_backend_cannot_upgrade_fabricated_build(self) -> None:
        binary = _static_elf(b"self-mutating-build")
        backend = pipeline.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            platform_name="linux",
            machine_name="x86_64",
        )

        def probe(_self: object) -> pipeline.DockerCapabilityReportV1:
            return pipeline.DockerSupportedV1(
                pipeline.OCI_IMAGE_REFERENCE_V1,
                pipeline.OCI_PLATFORM_V1,
                _digest("fabricated-daemon"),
            )

        def run_build(
            _self: object,
            request: pipeline.DockerBuildRequestV1,
        ) -> pipeline.DockerBuildProcessObservationV1:
            target = request.output_directory / pipeline.EVALUATOR_OUTPUT_NAME_V1
            target.write_bytes(binary)
            target.chmod(0o555)
            return pipeline.DockerBuildExitedV1(0, b"self-reported-native", b"")

        backend.probe = MethodType(probe, backend)
        backend.run_build = MethodType(run_build, backend)

        result = pipeline.ControlledPipelineV1(
            build_backend=backend,
            executor=_Executor(),
        ).execute(_request())

        self.assertIs(type(result), pipeline.DiagnosticPipelineObservationV1)
        self.assertFalse(hasattr(result, "build_observer_kind"))


class DockerCommandContractTests(unittest.TestCase):
    def test_command_is_exact_digest_offline_read_only_and_capability_free(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            directories = tuple(root / name for name in ("inputs", "workspace", "build", "out"))
            for directory in directories:
                directory.mkdir()
            request = pipeline.DockerBuildRequestV1(
                1,
                root,
                *directories,
                root / "container.cid",
                "labcolors-arb-build-v1-test",
            )
            backend = pipeline.NativeDockerBuildBackendV1(
                Path("/usr/bin/docker"),
                platform_name="linux",
                machine_name="x86_64",
            )

            command = backend.command_for(request)

        joined = " ".join(command)
        self.assertEqual(command[0], "/usr/bin/docker")
        self.assertIn(pipeline.OCI_IMAGE_REFERENCE_V1, command)
        self.assertNotIn("gcc:latest", joined)
        for fragment in (
            "--pull never",
            "--platform linux/amd64",
            "--network none",
            "--read-only",
            "--cap-drop ALL",
            "--security-opt no-new-privileges:true",
            "--name labcolors-arb-build-v1-test",
            "readonly,bind-propagation=private",
            "dst=/inputs",
            "dst=/workspace",
            "dst=/build",
            "dst=/out",
            "--rm",
        ):
            with self.subTest(fragment=fragment):
                self.assertIn(fragment, joined)
        for forbidden in ("--privileged", "--network host", ":latest"):
            self.assertNotIn(forbidden, joined)

    def test_native_probe_fails_closed_without_linux_or_exact_docker(self) -> None:
        non_linux = pipeline.NativeDockerBuildBackendV1(
            Path("/usr/bin/docker"),
            platform_name="darwin",
            machine_name="arm64",
        ).probe()
        missing = pipeline.NativeDockerBuildBackendV1(
            Path("/definitely/missing/docker"),
            platform_name="linux",
            machine_name="x86_64",
        ).probe()

        self.assertEqual(non_linux.reason, pipeline.DockerBlockerReasonV1.HOST_NOT_LINUX_AMD64)
        self.assertEqual(missing.reason, pipeline.DockerBlockerReasonV1.DOCKER_UNAVAILABLE)

    def test_native_command_observer_caps_probe_output_before_allocation(self) -> None:
        backend = pipeline.NativeDockerBuildBackendV1(
            Path("/bin/sh"),
            platform_name="linux",
            machine_name="x86_64",
        )

        result = backend._observe_command(
            (
                sys.executable,
                "-c",
                "import os; os.write(1, b'x' * 65536)",
            ),
            stdout_limit=8,
            stderr_limit=8,
            timeout_ns=5_000_000_000,
            cid_file=None,
        )

        self.assertEqual(
            result,
            pipeline.DockerBuildOutputLimitV1(
                pipeline.DockerOutputStreamV1.STDOUT,
                b"x" * 8,
                b"",
            ),
        )

    def test_cleanup_falls_back_to_exact_name_for_absent_or_invalid_cidfile(self) -> None:
        backend = pipeline.NativeDockerBuildBackendV1(
            Path("/bin/sh"),
            platform_name="linux",
            machine_name="x86_64",
        )
        name = "labcolors-arb-build-v1-cleanup-test"
        for cid_contents in (None, b"partial-or-foreign"):
            with self.subTest(cid_contents=cid_contents):
                with tempfile.TemporaryDirectory() as temporary:
                    cid_file = Path(temporary) / "container.cid"
                    if cid_contents is not None:
                        cid_file.write_bytes(cid_contents)
                    observations = (
                        pipeline.DockerBuildExitedV1(1, b"", b"not found"),
                        pipeline.DockerBuildExitedV1(0, b"", b""),
                    )
                    with mock.patch.object(
                        backend,
                        "_observe_cleanup_command",
                        side_effect=observations,
                    ) as observe:
                        detail = backend._cleanup_container(cid_file, name)

                self.assertIsNone(detail)
                commands = tuple(call.args[0] for call in observe.call_args_list)
                self.assertEqual(commands[0][-1], name)
                self.assertIn(f"name=^/{name}$", commands[1])
                self.assertNotIn("partial-or-foreign", " ".join(commands[0]))

    def test_unverified_container_removal_is_typed_cleanup_failure(self) -> None:
        backend = pipeline.NativeDockerBuildBackendV1(
            Path("/bin/sh"),
            platform_name="linux",
            machine_name="x86_64",
        )
        with tempfile.TemporaryDirectory() as temporary:
            cid_file = Path(temporary).resolve() / "container.cid"
            with mock.patch.object(
                backend,
                "_cleanup_container",
                return_value="container absence could not be verified",
            ):
                result = backend._observe_command(
                    (sys.executable, "-c", "pass"),
                    stdout_limit=8,
                    stderr_limit=8,
                    timeout_ns=5_000_000_000,
                    cid_file=cid_file,
                    container_name="labcolors-arb-build-v1-cleanup-failure",
                )

        self.assertIs(type(result), pipeline.DockerBuildCleanupFailureV1)
        self.assertEqual(
            result.trigger,
            pipeline.DockerCleanupTriggerV1.PROCESS_EXIT,
        )


@unittest.skipUnless(
    sys.platform == "linux"
    and os.environ.get("LABCOLORS_ARB_PIPELINE_DOCKER")
    and os.environ.get("LABCOLORS_GMP_ARCHIVE")
    and os.environ.get("LABCOLORS_MPFR_ARCHIVE")
    and os.environ.get("LABCOLORS_FLINT_ARCHIVE"),
    "requires Linux, Docker, and all three exact source archives",
)
class NativeBuildIntegrationTests(unittest.TestCase):
    def test_real_two_builds_and_ephemeral_evaluator_runtime_tests(self) -> None:
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
        controller = pipeline.ControlledPipelineV1(
            build_backend=pipeline.NativeDockerBuildBackendV1(
                Path(os.environ["LABCOLORS_ARB_PIPELINE_DOCKER"])
            ),
            executor=object(),
        )

        result = controller.build(
            _request(source_lock=source_lock, admitted_sources=admitted)
        )

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1, result)
        self.assertEqual(result.rebuild_sha256s, (result.binary_sha256,) * 2)

        # This executable is deliberately ephemeral and is never uploaded: a
        # distributable static artifact needs a separate linker/legal gate.
        with tempfile.TemporaryDirectory(prefix="labcolors-arb-evaluator-tests-") as temporary:
            executable = Path(temporary) / pipeline.EVALUATOR_OUTPUT_NAME_V1
            executable.write_bytes(result.binary)
            executable.chmod(0o555)
            environment = {
                "LABCOLORS_ARB_EVALUATOR": str(executable),
                "LC_ALL": "C",
                "PATH": os.environ.get("PATH", ""),
                "PYTHONDONTWRITEBYTECODE": "1",
                "PYTHONHASHSEED": "0",
                "TZ": "UTC",
            }
            runtime = subprocess.run(
                (
                    sys.executable,
                    "-m",
                    "unittest",
                    "-v",
                    "proof.region.v1.arb.tests.test_evaluator_source",
                ),
                check=False,
                capture_output=True,
                cwd=REPO,
                env=environment,
            )

        self.assertEqual(
            runtime.returncode,
            0,
            runtime.stderr.decode("utf-8", "replace"),
        )
        self.assertNotIn(b"skipped", runtime.stderr.lower())


if __name__ == "__main__":
    unittest.main(verbosity=2)
