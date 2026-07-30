#!/usr/bin/env python3
"""Causal, hostile tests for the controlled Arb BUILD/RUN pipeline."""

from __future__ import annotations

import gzip
import hashlib
import io
import inspect
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
    ComparatorManifestV2,
    ContentResolvedComparatorManifestV2,
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
        timeout=30,
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
def _foreign_comparator() -> ContentResolvedComparatorManifestV2:
    content = tuple(f"manifest-coordinate-{index}".encode() for index in range(10))
    manifest = ComparatorManifestV2(
        ComparatorKindV1.ARB,
        *(hashlib.sha256(item).digest() for item in content),
    )
    by_digest = {hashlib.sha256(item).digest(): item for item in content}
    return ContentResolvedComparatorManifestV2.admit(manifest, by_digest.get)


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
        "host_trust": pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST,
    }
    values.update(changes)
    return pipeline.PipelineRequestV1(**values)


class _BuildBackend:
    def __init__(
        self,
        outputs: tuple[bytes, ...],
        *,
        probe: pipeline.DockerCapabilityReportV1 | None = None,
        reject_input: bool = False,
        omit_transfer: bool = False,
        foreign_transfer: bool = False,
        reported_stderr: bytes = b"",
    ) -> None:
        self.outputs = list(outputs)
        self.probe_result = probe or pipeline.DockerSupportedV1(
            pipeline.OCI_IMAGE_REFERENCE_V1,
            pipeline.OCI_PLATFORM_V1,
            _digest("docker-daemon"),
        )
        self.reject_input = reject_input
        self.omit_transfer = omit_transfer
        self.foreign_transfer = foreign_transfer
        self.reported_stderr = reported_stderr
        self.requests: list[pipeline.DockerBuildRequestV1] = []

    def probe(self) -> pipeline.DockerCapabilityReportV1:
        return self.probe_result

    def run_build(
        self,
        request: pipeline.DockerBuildRequestV1,
    ) -> pipeline.DockerBuildProcessObservationV1:
        self.requests.append(request)
        output = self.outputs.pop(0)
        if self.reject_input:
            return pipeline.DockerBuildInputRejectedV1(
                pipeline._build_input_progress_v1(
                    request.input_bundle,
                    1,
                    hashlib.sha256(request.input_bundle._contents[:1]).digest(),
                ),
                b"",
                b"",
            )
        if self.omit_transfer:
            return pipeline._docker_command_exited_v1(0, output, self.reported_stderr)
        transfer = pipeline._completed_build_input_transfer_v1(
            request.input_bundle,
            request.input_bundle.length,
            request.input_bundle.sha256,
        )
        if self.foreign_transfer:
            object.__setattr__(
                transfer,
                "bundle_identity",
                _digest("foreign-bundle"),
            )
        return pipeline._docker_build_exited_v1(
            0,
            output,
            self.reported_stderr,
            transfer,
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
    def _result(self) -> pipeline.DiagnosticBuildObservationV1:
        binary = _static_elf(b"derived-comparator")
        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
        ).build(_request())
        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
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
        self.assertIn(
            b"gap:host-and-docker-daemon-not-source-bound",
            admitted.preimages.exclusions,
        )
        self.assertNotIn(b"persistent", admitted.preimages.exclusions)
        self.assertNotIn(b"github-hosted", admitted.preimages.exclusions)

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
                    ContentResolvedComparatorManifestV2.admit(
                        manifest,
                        resolver.get,
                    )

    def test_operator_coordinate_is_the_exact_ordered_formula_contract(self) -> None:
        original = _build_sources().formula_spec
        lines = original.splitlines()
        self.assertIn(
            b"operators 20",
            lines,
            "registered formula must retain the exact 20-operator contract",
        )
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
                reported_stderr=report,
            ),
        ).build(_request())

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
        self.assertNotEqual(result.comparator.identity, foreign.identity)
        coordinates = tuple(
            getattr(result.comparator.manifest.manifest, field.name)
            for field in dataclass_fields(result.comparator.manifest.manifest)
            if field.name != "kind"
        )
        self.assertNotIn(foreign.identity, coordinates)
        self.assertEqual(result.build_processes[0].stdout, binary)
        self.assertEqual(result.build_processes[0].stderr, report)

    def test_diagnostic_comparator_has_no_public_constructor(self) -> None:
        with self.assertRaises(TypeError):
            pipeline.DiagnosticArbComparatorV1()


class ControlledPipelineTests(unittest.TestCase):
    def test_admission_uses_only_explicit_cross_module_verification_api(self) -> None:
        source = (ARB / "pipeline.py").read_text(encoding="utf-8")

        for forbidden in (
            "executor._result_matches_request",
            "executor._require_static_x86_64_elf",
            "protocol._validate_witness_alignment",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)
        self.assertTrue(callable(executor.invocation_identity_v1))
        self.assertTrue(callable(executor.platform_identity_v1))
        self.assertFalse(hasattr(pipeline, "invocation_identity_v1"))
        self.assertFalse(hasattr(pipeline, "platform_identity_v1"))

    def test_host_trust_claims_only_backend_observable_facts(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST

        self.assertEqual(tuple(pipeline.HostTrustBoundaryV1), (trust,))
        self.assertEqual(trust.value, "unsealed-linux-x64-docker-host")
        self.assertFalse(
            hasattr(
                pipeline.HostTrustBoundaryV1,
                "PERSISTENT_SELF_HOSTED_DOCKER",
            )
        )

    def test_pipeline_policy_identity_binds_the_stream_bootstrap(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST
        original = pipeline.pipeline_policy_identity_v1(trust)

        with mock.patch.object(
            pipeline,
            "_BUILD_BOOTSTRAP_V1",
            pipeline._BUILD_BOOTSTRAP_V1 + "\nexit 1",
        ):
            changed = pipeline.pipeline_policy_identity_v1(trust)

        self.assertNotEqual(original, changed)

    def test_pipeline_policy_identity_binds_the_private_tmpfs_policy(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST
        original = pipeline.pipeline_policy_identity_v1(trust)

        with mock.patch.object(
            pipeline,
            "_BUILD_TMPFS_SPEC_V1",
            "/tmp:rw,exec,suid,dev,mode=1777",
        ):
            changed = pipeline.pipeline_policy_identity_v1(trust)

        self.assertNotEqual(original, changed)

    def test_build_only_does_not_probe_or_execute_run_backend(self) -> None:
        binary = _static_elf(b"build-only")
        controller = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
        )

        result = controller.build(_request())

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
        self.assertEqual(result.binary, binary)
        self.assertEqual(result.rebuild_sha256s, (result.binary_sha256,) * 2)
        self.assertIs(type(result.comparator), pipeline.DiagnosticArbComparatorV1)

    def test_two_fresh_equal_builds_retain_one_input_and_exact_outputs(self) -> None:
        binary = _static_elf(b"observed-output")
        build = _BuildBackend((binary, binary))
        controller = pipeline.ControlledPipelineV1(build_backend=build)

        result = controller.build(_request())

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
        self.assertEqual(len(build.requests), 2)
        self.assertEqual(tuple(item.attempt for item in build.requests), (1, 2))
        self.assertIs(build.requests[0].input_bundle, build.requests[1].input_bundle)
        self.assertEqual(result.binary, binary)
        self.assertEqual(result.binary_sha256, hashlib.sha256(binary).digest())
        self.assertEqual(
            result.rebuild_sha256s,
            (result.binary_sha256, result.binary_sha256),
        )
        self.assertIs(result.rebuild_binaries[0], result.binary)
        self.assertEqual(result.rebuild_binaries, (binary, binary))
        self.assertEqual(
            result.input_transfers[0].bundle_identity,
            result.input_bundle_identity,
        )
        self.assertEqual(
            result.input_transfers,
            tuple(item.input_transfer for item in result.build_processes),
        )
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
        self.assertFalse(hasattr(result, "build_source_identity"))
        self.assertFalse(hasattr(result, "build_policy_identity"))
        self.assertFalse(hasattr(result, "commit_derived_source_identity"))
        self.assertEqual(
            result.host_trust,
            pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST,
        )
        self.assertEqual(result.oci_image_reference, pipeline.OCI_IMAGE_REFERENCE_V1)
        self.assertEqual(result.oci_platform, pipeline.OCI_PLATFORM_V1)
        self.assertFalse(hasattr(result, "slsa_level"))
        self.assertFalse(hasattr(result, "fresh_vm"))

    def test_builds_must_be_byte_identical(self) -> None:
        first = _static_elf(b"first")
        second = _static_elf(b"second")

        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((first, second)),
        ).build(_request())

        self.assertEqual(
            result,
            pipeline.NonReproducibleBuildV1(
                hashlib.sha256(first).digest(),
                hashlib.sha256(second).digest(),
            ),
        )

    def test_input_transport_or_invalid_binary_is_typed_failure(self) -> None:
        binary = _static_elf()
        cases = (
            (
                _BuildBackend((binary,), reject_input=True),
                pipeline.BuildFailureReasonV1.INPUT_TRANSFER_FAILED,
            ),
            (
                _BuildBackend((binary,), omit_transfer=True),
                pipeline.BuildFailureReasonV1.BACKEND_CONTRACT,
            ),
            (
                _BuildBackend((binary,), foreign_transfer=True),
                pipeline.BuildFailureReasonV1.BACKEND_CONTRACT,
            ),
            (
                _BuildBackend((b"not-an-elf",)),
                pipeline.BuildFailureReasonV1.INVALID_OUTPUT,
            ),
        )
        for backend, reason in cases:
            with self.subTest(reason=reason):
                result = pipeline.ControlledPipelineV1(
                    build_backend=backend,
                ).build(_request())
                self.assertIs(type(result), pipeline.BuildRejectedV1)
                self.assertEqual(result.attempt, 1)
                self.assertEqual(result.reason, reason)

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

    def test_build_output_limit_is_rejected_at_pipeline_admission(self) -> None:
        with self.assertRaises(pipeline.PipelineInputErrorV1) as caught:
            _request(
                execution_limits=replace(
                    _limits(),
                    max_executable_bytes=pipeline.BUILD_STDOUT_LIMIT_V1 + 1,
                )
            )

        self.assertEqual(
            caught.exception.reason,
            pipeline.PipelineInputReasonV1.EXECUTION_LIMIT_MISMATCH,
        )
        self.assertEqual(caught.exception.field, "execution_limits")

    def test_snapshot_modes_are_normalized_independently_of_host_umask(self) -> None:
        binary = _static_elf(b"umask-independent")
        identities: list[tuple[bytes, bytes]] = []
        for mask in (0o077, 0o022):
            previous = os.umask(mask)
            try:
                result = pipeline.ControlledPipelineV1(
                    build_backend=_BuildBackend((binary, binary)),
                ).build(_request())
            finally:
                os.umask(previous)
            self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
            identities.append(
                (result.input_bundle_identity, result.input_bundle_sha256)
            )

        self.assertEqual(identities[0], identities[1])

    def test_pipeline_exports_no_receipt_or_self_report_admission_api(self) -> None:
        names = dir(pipeline)
        source = (ARB / "pipeline.py").read_text(encoding="utf-8")
        self.assertFalse(any("Receipt" in name for name in names))
        self.assertFalse(hasattr(pipeline, "DiagnosticPipelineObservationV1"))
        self.assertFalse(hasattr(pipeline, "PipelineResultV1"))
        self.assertFalse(hasattr(pipeline, "ExecutionControllerV1"))
        self.assertFalse(hasattr(pipeline.ControlledPipelineV1, "execute"))
        self.assertEqual(
            tuple(inspect.signature(pipeline.ControlledPipelineV1).parameters),
            ("build_backend",),
        )
        self.assertFalse(hasattr(pipeline.ControlledPipelineV1, "admit_report"))
        self.assertFalse(hasattr(pipeline.ControlledPipelineV1, "mint"))
        self.assertNotIn("run-observation=diagnostic", source)

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
            transfer = pipeline._completed_build_input_transfer_v1(
                request.input_bundle,
                request.input_bundle.length,
                request.input_bundle.sha256,
            )
            return pipeline._docker_build_exited_v1(0, binary, b"", transfer)

        backend.probe = MethodType(probe, backend)
        backend.run_build = MethodType(run_build, backend)

        result = pipeline.ControlledPipelineV1(
            build_backend=backend,
        ).build(_request())

        self.assertIs(type(result), pipeline.DiagnosticBuildObservationV1)
        self.assertFalse(hasattr(result, "build_observer_kind"))


class DockerCommandContractTests(unittest.TestCase):
    def test_command_is_exact_digest_offline_read_only_and_capability_free(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            request = pipeline.DockerBuildRequestV1(
                1,
                pipeline._seal_build_input_bundle_v1(_request()),
                _limits().max_executable_bytes,
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
            "--interactive",
            "--cap-drop ALL",
            "--security-opt no-new-privileges:true",
            "--name labcolors-arb-build-v1-test",
            "--rm",
        ):
            with self.subTest(fragment=fragment):
                self.assertIn(fragment, joined)
        for forbidden in ("--privileged", "--network host", ":latest"):
            self.assertNotIn(forbidden, joined)

    def test_command_exposes_only_a_private_non_executable_standard_tmp(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            request = pipeline.DockerBuildRequestV1(
                1,
                pipeline._seal_build_input_bundle_v1(_request()),
                _limits().max_executable_bytes,
                root / "container.cid",
                "labcolors-arb-build-v1-test",
            )
            command = pipeline.NativeDockerBuildBackendV1(
                Path("/usr/bin/docker"),
                platform_name="linux",
                machine_name="x86_64",
            ).command_for(request)

        tmpfs_indexes = tuple(
            index for index, item in enumerate(command) if item == "--tmpfs"
        )
        self.assertEqual(len(tmpfs_indexes), 2)
        self.assertEqual(
            tuple(command[index + 1] for index in tmpfs_indexes),
            (pipeline._BUILD_TMPFS_SPEC_V1, pipeline._BUILD_STATE_TMPFS_SPEC_V1),
        )
        self.assertTrue(
            all("src=" not in command[index + 1] for index in tmpfs_indexes)
        )
        mount_indexes = tuple(
            index for index, item in enumerate(command) if item == "--mount"
        )
        self.assertEqual(mount_indexes, ())
        self.assertNotIn("-v", command)
        self.assertNotIn("--volume", command)

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
                        pipeline._docker_command_exited_v1(1, b"", b"not found"),
                        pipeline._docker_command_exited_v1(0, b"", b""),
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
