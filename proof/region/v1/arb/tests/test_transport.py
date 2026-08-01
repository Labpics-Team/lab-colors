#!/usr/bin/env python3
"""Behavioral contract for the causal controller-to-Docker BUILD transport."""

from __future__ import annotations

import hashlib
import io
import inspect
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
TESTS = ARB / "tests"
sys.path.insert(0, str(PROOF))
sys.path.insert(0, str(ARB))
sys.path.insert(0, str(TESTS))

from build import input as build_input  # noqa: E402
from build import transport as build_transport  # noqa: E402
import pipeline  # noqa: E402
from test_pipeline import (  # noqa: E402
    _docker_capability,
    _probe_native_backend,
    _request,
)


BUILD_RECIPE = ARB / "build.sh"
NATIVE_GATE = ARB / "tests" / "native_gate.py"
_TEST_CANONICAL_LIMITS = build_input.CanonicalInputLimitsV1(64, 1024, 4096)


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


def _bundle(length: int = 1024 * 1024) -> build_input.SealedInputV1:
    contents = (b"0123456789abcdef" * ((length + 15) // 16))[:length]
    return build_input.seal_input_v1(_digest("opaque-binding"), contents)


def _backend() -> build_transport.NativeDockerBuildBackendV1:
    return build_transport.NativeDockerBuildBackendV1(
        Path("/bin/true"),
        pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
        platform_name="linux",
        machine_name="x86_64",
    )


def _observe(
    source: str,
    bundle: build_input.SealedInputV1,
    *,
    stdout_limit: int = 2 * 1024 * 1024,
    stderr_limit: int = 2 * 1024 * 1024,
    timeout_ns: int = 5_000_000_000,
) -> build_transport.DockerBuildProcessObservationV1:
    return _backend()._observe_command(
        (sys.executable, "-c", source),
        stdout_limit=stdout_limit,
        stderr_limit=stderr_limit,
        timeout_ns=timeout_ns,
        input_bundle=bundle,
    )


class CanonicalBuildBundleTests(unittest.TestCase):
    def test_bundle_is_reproducible_normalized_ustar_with_no_host_authority(self) -> None:
        request = _request()
        first = pipeline._seal_build_input_bundle_v1(request)
        second = pipeline._seal_build_input_bundle_v1(request)

        self.assertIsNot(first, second)
        self.assertIs(first.contents, first.contents)
        self.assertEqual(first.contents, second.contents)
        self.assertEqual(first.sha256, second.sha256)
        self.assertEqual(first.binding_identity, second.binding_identity)
        self.assertTrue(pipeline.arb_input_is_bound_v1(request, first))

        source_entries = tuple(
            entry
            for lock, admitted in zip(
                request.source_lock.sources,
                request.admitted_sources.sources,
                strict=True,
            )
            for entry in pipeline._normalized_source_entries_v1(lock, admitted)
        )
        workspace_entries = tuple(
            (
                "inputs/formula.generated.c"
                if item.path == pipeline.GENERATED_FORMULA_PATH_V1
                else f"workspace/{item.path}",
                item.mode,
                item.contents,
            )
            for item in request.build_sources.files
            if item.path
            not in (pipeline.FORMULA_SPEC_PATH_V1, pipeline.FORMULA_GENERATOR_PATH_V1)
        )
        expected_entries = tuple(sorted(source_entries + workspace_entries))
        expected_files = {path: (mode, body) for path, mode, body in expected_entries}
        expected_directories = {
            "/".join(path.split("/")[:index])
            for path in expected_files
            for index in range(1, len(path.split("/")))
        }
        expected_order = tuple(
            sorted(expected_directories, key=lambda value: (value.count("/"), value))
        ) + tuple(sorted(expected_files))

        with tarfile.open(fileobj=io.BytesIO(first.contents), mode="r:") as archive:
            members = tuple(archive)
            self.assertFalse(archive.pax_headers)
            self.assertEqual(tuple(member.name for member in members), expected_order)
            self.assertEqual(len({member.name for member in members}), len(members))
            for member in members:
                with self.subTest(path=member.name):
                    self.assertEqual(member.uid, 0)
                    self.assertEqual(member.gid, 0)
                    self.assertEqual(member.uname, "")
                    self.assertEqual(member.gname, "")
                    self.assertEqual(member.mtime, 0)
                    self.assertFalse(member.pax_headers)
                    self.assertFalse(member.issym() or member.islnk())
                    if member.isdir():
                        self.assertIn(member.name, expected_directories)
                        self.assertEqual(member.mode, 0o755)
                        self.assertEqual(member.size, 0)
                    else:
                        self.assertTrue(member.isreg())
                        mode, body = expected_files[member.name]
                        self.assertEqual(member.mode, mode)
                        self.assertEqual(member.size, len(body))
                        stream = archive.extractfile(member)
                        self.assertIsNotNone(stream)
                        self.assertEqual(stream.read(), body)

    def test_canonical_encoder_rejects_reorder_collision_and_unencodable_path(self) -> None:
        def reject(
            values: object,
            reason: build_input.InputReasonV1,
            field: str,
            limits: build_input.CanonicalInputLimitsV1 = _TEST_CANONICAL_LIMITS,
        ) -> None:
            with self.assertRaises(build_input.InputErrorV1) as caught:
                build_input.canonical_ustar_v1(values, limits)
            self.assertEqual(caught.exception.reason, reason)
            self.assertEqual(caught.exception.field, field)

        entries = (("a/b", 0o644, b"x"), ("c", 0o755, b"y"))
        encoded = build_input.canonical_ustar_v1(entries, _TEST_CANONICAL_LIMITS)
        self.assertEqual(
            hashlib.sha256(encoded).hexdigest(),
            "11bc313cba907e89535876eb8ce46194472367007053ab58b723338676f99427",
        )
        for hostile, reason, field in (
            (
                tuple(reversed(entries)),
                build_input.InputReasonV1.NONCANONICAL_SET,
                "entries",
            ),
            (
                (("a", 0o644, b"x"), ("a/b", 0o644, b"y")),
                build_input.InputReasonV1.NONCANONICAL_SET,
                "a",
            ),
            (
                (("A", 0o644, b"x"), ("a", 0o644, b"y")),
                build_input.InputReasonV1.NONCANONICAL_SET,
                "a",
            ),
            (
                (("a" * 256, 0o644, b"x"),),
                build_input.InputReasonV1.INVALID_PATH,
                "a" * 256,
            ),
            (
                ((1, 0o644, b"x"),),
                build_input.InputReasonV1.INVALID_PATH,
                "path",
            ),
            (
                ((["a"], 0o644, b"x"),),
                build_input.InputReasonV1.INVALID_PATH,
                "path",
            ),
            (
                (("a" * 101, 0o644, b"x"),),
                build_input.InputReasonV1.INVALID_PATH,
                "a" * 101,
            ),
            (
                (("A", 0o644, b"x"), ("a/b", 0o644, b"y")),
                build_input.InputReasonV1.NONCANONICAL_SET,
                "A",
            ),
        ):
            with self.subTest(hostile=repr(hostile)):
                reject(hostile, reason, field)

        resource_cases = (
            (
                (("a", 0o644, b"x"), ("b", 0o644, b"y")),
                build_input.CanonicalInputLimitsV1(1, 1, 2),
                "max_members",
            ),
            (
                (("a", 0o644, b"xy"),),
                build_input.CanonicalInputLimitsV1(1, 1, 2),
                "max_file_bytes",
            ),
            (
                (("a", 0o644, b"x"), ("b", 0o644, b"y")),
                build_input.CanonicalInputLimitsV1(2, 1, 1),
                "max_payload_bytes",
            ),
            (
                (("a/b", 0o644, b"x"),),
                build_input.CanonicalInputLimitsV1(1, 1, 1),
                "max_members",
            ),
            (
                (("a", 0o644, b"x"),),
                build_input.CanonicalInputLimitsV1(1, 1, 1, 10_239),
                "max_encoded_bytes",
            ),
        )
        for values, limits, field in resource_cases:
            with self.subTest(resource=field):
                reject(
                    values,
                    build_input.InputReasonV1.RESOURCE_LIMIT,
                    field,
                    limits,
                )
        exact_cap = build_input.CanonicalInputLimitsV1(1, 1, 1, 10_240)
        self.assertEqual(
            len(build_input.canonical_ustar_v1((("a", 0o644, b"x"),), exact_cap)),
            exact_cap.max_encoded_bytes,
        )

    def test_long_implicit_directory_uses_the_ustar_trailing_separator(self) -> None:
        directory = "d" * 155
        entries = ((f"{directory}/f", 0o644, b"x"),)
        limits = build_input.CanonicalInputLimitsV1(2, 1, 1)

        encoded = build_input.canonical_ustar_v1(entries, limits)

        with tarfile.open(fileobj=io.BytesIO(encoded), mode="r:") as archive:
            self.assertEqual(
                tuple(member.name for member in archive),
                (directory, f"{directory}/f"),
            )
        with self.assertRaises(build_input.InputErrorV1) as caught:
            build_input.canonical_ustar_v1(
                ((f"{'d' * 156}/f", 0o644, b"x"),),
                limits,
            )
        self.assertEqual(caught.exception.reason, build_input.InputReasonV1.INVALID_PATH)
        self.assertEqual(caught.exception.field, f"{'d' * 156}/f")

    def test_omission_or_content_mutation_changes_bundle_identity(self) -> None:
        entries = (("a", 0o644, b"x"), ("b", 0o644, b"y"))
        original = build_input.canonical_ustar_v1(entries, _TEST_CANONICAL_LIMITS)
        omitted = build_input.canonical_ustar_v1(
            entries[:1],
            _TEST_CANONICAL_LIMITS,
        )
        mutated = build_input.canonical_ustar_v1(
            (("a", 0o644, b"x"), ("b", 0o644, b"z")),
            _TEST_CANONICAL_LIMITS,
        )
        identities = {
            (
                sealed.binding_identity,
                sealed.sha256,
            )
            for body in (original, omitted, mutated)
            for sealed in (
                build_input.seal_input_v1(_digest("opaque-binding"), body),
            )
        }
        self.assertEqual(len(identities), 3)

    def test_replayed_source_coordinates_must_match_the_admitted_capability(self) -> None:
        request = _request()
        admitted = request.admitted_sources.sources[0]
        original = admitted.tree_identity
        object.__setattr__(admitted, "tree_identity", _digest("mutated-tree"))
        try:
            with self.assertRaises(pipeline.PipelineInputErrorV1):
                pipeline._normalized_source_entries_v1(
                    request.source_lock.sources[0],
                    admitted,
                )
        finally:
            object.__setattr__(admitted, "tree_identity", original)

    def test_transport_authorities_cannot_be_directly_forged(self) -> None:
        bundle = _bundle(1024)
        with self.assertRaises(TypeError):
            build_transport.BuildInputTransferProgressV1(
                bundle.binding_identity,
                bundle.length,
                bundle.sha256,
                bundle.length,
                bundle.sha256,
            )
        with self.assertRaises(TypeError):
            build_transport.BuildInputTransferV1(object())
        with self.assertRaises(TypeError):
            build_transport.DockerBuildExitedV1(0, b"binary", b"", object())
        with self.assertRaises(TypeError):
            build_transport.DockerBuildPolicyV1(
                "gcc@sha256:bad@sha256:" + "0" * 64,
                *pipeline.ARB_BUILD_TRANSPORT_POLICY_V1[1:],
            )
        report = _docker_capability()
        self.assertFalse(hasattr(report, "__dict__"))
        with self.assertRaises((AttributeError, TypeError)):
            object.__setattr__(report, "host_user", (0, 0))
        forged_policy = tuple.__new__(build_transport.DockerBuildPolicyV1, ())
        with self.assertRaises(TypeError):
            build_transport.ControlledBuildTransportV1(
                policy=forged_policy,
                backend=object(),
            )

        class ForgedProbeBackend:
            def probe(self) -> object:
                return tuple.__new__(build_transport.DockerSupportedV1, ())

        probed = build_transport.ControlledBuildTransportV1(
            policy=pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            backend=ForgedProbeBackend(),
        ).probe()
        self.assertIs(type(probed), build_transport.DockerUnsupportedV1)
        self.assertEqual(
            probed.reason,
            build_transport.DockerBlockerReasonV1.BACKEND_CONTRACT,
        )

        class ForgedProcessBackend:
            def probe(self) -> object:
                return report

            def run_build(self, _request: object) -> object:
                return tuple.__new__(build_transport.DockerBuildExitedV1, ())

        controller = build_transport.ControlledBuildTransportV1(
            policy=pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            backend=ForgedProcessBackend(),
        )
        observed_report = controller.probe()
        self.assertIs(type(observed_report), build_transport.DockerSupportedV1)
        rejected = controller.build(
            observed_report,
            bundle,
            1,
            input_admission=lambda _value: True,
            output_admission=lambda _value: True,
        )
        self.assertIs(type(rejected), build_transport.BuildRejectedV1)
        self.assertEqual(
            rejected.reason,
            build_transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
        )


class BuildInputObserverTests(unittest.TestCase):
    def test_positive_partial_writes_are_normal_and_commit_exact_transfer(self) -> None:
        bundle = _bundle(256 * 1024)
        real_write = os.write

        def partial_write(descriptor: int, contents: object) -> int:
            return real_write(descriptor, contents[:997])

        with mock.patch.object(build_transport.os, "write", side_effect=partial_write):
            result = _observe(
                "import hashlib,sys; d=sys.stdin.buffer.read(); "
                "sys.stdout.buffer.write(hashlib.sha256(d).digest()); "
                "sys.stderr.buffer.write(b'observed')",
                bundle,
            )

        self.assertIs(type(result), build_transport.DockerBuildExitedV1, result)
        self.assertEqual(result.stdout, bundle.sha256)
        self.assertEqual(result.stderr, b"observed")
        self.assertEqual(result.input_transfer.bundle_identity, bundle.binding_identity)
        self.assertEqual(result.input_transfer.expected_length, bundle.length)
        self.assertEqual(result.input_transfer.expected_sha256, bundle.sha256)
        self.assertEqual(result.input_transfer.written_length, bundle.length)
        self.assertEqual(result.input_transfer.written_sha256, bundle.sha256)

    def test_zero_write_and_epipe_are_typed_with_exact_partial_progress(self) -> None:
        bundle = _bundle()
        with mock.patch.object(build_transport.os, "write", return_value=0):
            zero = _observe("import sys; sys.stdin.buffer.read()", bundle)
        self.assertIs(type(zero), build_transport.DockerBuildInputRejectedV1, zero)
        self.assertEqual(zero.written_length, 0)
        self.assertEqual(zero.written_sha256, hashlib.sha256(b"").digest())

        closed = _observe("import os,time; os.close(0); time.sleep(1)", bundle)
        self.assertIs(type(closed), build_transport.DockerBuildInputRejectedV1, closed)
        self.assertLess(closed.written_length, bundle.length)
        self.assertEqual(
            closed.written_sha256,
            hashlib.sha256(bundle.contents[: closed.written_length]).digest(),
        )

    def test_final_stdin_close_failure_is_a_typed_observer_failure(self) -> None:
        real_popen = subprocess.Popen

        class CloseFailsOnce:
            def __init__(self, stream: object) -> None:
                self._stream = stream

            @property
            def closed(self) -> bool:
                return self._stream.closed

            def fileno(self) -> int:
                return self._stream.fileno()

            def close(self) -> None:
                self._stream.close()
                raise BrokenPipeError("forced close failure")

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            self.assertIsNotNone(process.stdin)
            process.stdin = CloseFailsOnce(process.stdin)
            return process

        with mock.patch.object(build_transport.subprocess, "Popen", side_effect=spawn):
            result = _observe(
                "import time; time.sleep(1)",
                _bundle(2 * 1024 * 1024),
                timeout_ns=100_000_000,
            )

        self.assertIs(type(result), build_transport.DockerBuildObserverFailureV1, result)

    def test_full_duplex_backpressure_does_not_deadlock_or_drop_bytes(self) -> None:
        bundle = _bundle(512 * 1024)
        result = _observe(
            "import os\n"
            "while True:\n"
            " d=os.read(0,4096)\n"
            " if not d: break\n"
            " os.write(1,b'o'*len(d))\n"
            " os.write(2,b'e'*len(d))\n",
            bundle,
        )
        self.assertIs(type(result), build_transport.DockerBuildExitedV1, result)
        self.assertEqual(len(result.stdout), bundle.length)
        self.assertEqual(len(result.stderr), bundle.length)
        self.assertEqual(result.input_transfer.written_sha256, bundle.sha256)

    def test_timeout_and_output_limit_preserve_input_progress(self) -> None:
        bundle = _bundle()
        timed = _observe(
            "import os,time; os.read(0,1); time.sleep(2)",
            bundle,
            timeout_ns=100_000_000,
        )
        self.assertIs(type(timed), build_transport.DockerBuildTimedOutV1, timed)
        self.assertIs(type(timed.input_progress), build_transport.BuildInputTransferProgressV1)
        self.assertGreater(timed.input_progress.written_length, 0)
        self.assertLess(timed.input_progress.written_length, bundle.length)

        limited = _observe(
            "import os,time; os.write(1,b'x'*65536); time.sleep(2)",
            bundle,
            stdout_limit=8,
        )
        self.assertIs(type(limited), build_transport.DockerBuildOutputLimitV1, limited)
        self.assertEqual(limited.stream, build_transport.DockerOutputStreamV1.STDOUT)
        self.assertEqual(limited.stdout, b"x" * 8)
        self.assertIs(type(limited.input_progress), build_transport.BuildInputTransferProgressV1)

    def test_cleanup_failure_preserves_input_trigger_and_progress(self) -> None:
        bundle = _bundle()
        backend = _backend()
        lease = backend._next_run_lease_v1(
            _docker_capability(
                pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
                docker_path=Path("/bin/true"),
            )
        )
        try:
            with mock.patch.object(
                backend,
                "_cleanup_container",
                return_value="forced cleanup failure",
            ):
                result = backend._observe_command(
                    (sys.executable, "-c", "import os,time; os.close(0); time.sleep(1)"),
                    stdout_limit=1024,
                    stderr_limit=1024,
                    timeout_ns=5_000_000_000,
                    lease=lease,
                    input_bundle=bundle,
                )
        finally:
            backend._release_run_lease_v1(lease)
        self.assertIs(type(result), build_transport.DockerBuildCleanupFailureV1, result)
        self.assertEqual(result.trigger, build_transport.DockerCleanupTriggerV1.INPUT_TRANSFER)
        self.assertEqual(result.detail, "forced cleanup failure")
        self.assertIs(type(result.input_progress), build_transport.BuildInputTransferProgressV1)
        self.assertLess(result.input_progress.written_length, bundle.length)


class SealedBuildTransportContractTests(unittest.TestCase):
    def test_successful_probe_keeps_machine_readable_stdout_despite_cli_warning(self) -> None:
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        with tempfile.TemporaryDirectory() as temporary:
            docker_path = Path(temporary) / "docker"
            docker_path.write_bytes(b"fixture")
            docker_path.chmod(0o755)
            backend = build_transport.NativeDockerBuildBackendV1(
                docker_path,
                policy,
                platform_name="linux",
                machine_name="x86_64",
                host_user=(501, 20),
            )
            image = json.dumps(
                [
                    {
                        "Os": "linux",
                        "Architecture": "amd64",
                        "RepoDigests": [policy.image_reference],
                    }
                ],
                separators=(",", ":"),
            ).encode("ascii")
            with mock.patch.object(
                backend,
                "_observe_command",
                side_effect=(
                    build_transport._docker_command_exited_v1(
                        0,
                        b'{"Version":"fixture"}',
                        b"warning: CLI hint\n",
                    ),
                    build_transport._docker_command_exited_v1(
                        0,
                        image,
                        b"warning: local metadata\n",
                    ),
                ),
            ):
                capability = backend.probe()

        self.assertIs(type(capability), build_transport.DockerSupportedV1)
        self.assertEqual(
            capability.daemon_observation.server_stdout,
            b'{"Version":"fixture"}',
        )

    def test_controller_owns_one_sealed_bundle_for_both_builds(self) -> None:
        pipeline_source = inspect.getsource(pipeline.ControlledPipelineV1.build)
        transport_source = inspect.getsource(
            build_transport.ControlledBuildTransportV1.build
        )
        self.assertEqual(pipeline_source.count("_seal_build_input_bundle_v1("), 1)
        self.assertIn("for attempt in (1, 2)", transport_source)

    def test_docker_request_carries_only_semantic_build_coordinates(self) -> None:
        fields = set(inspect.signature(build_transport.DockerBuildRequestV1).parameters)
        self.assertEqual(
            fields,
            {
                "attempt",
                "capability",
                "input_bundle",
                "max_output_bytes",
            },
        )
        backend = build_transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            platform_name="linux",
            machine_name="x86_64",
            host_user=(501, 20),
        )
        capability = _probe_native_backend(
            backend,
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
        )
        request = build_transport.DockerBuildRequestV1(
            1,
            capability,
            _bundle(1024),
            1024,
        )
        self.assertFalse(hasattr(request, "cid_file"))
        self.assertFalse(hasattr(request, "container_name"))
        # The request has no positional slot for adapter-owned host cleanup
        # authority; extra values cannot smuggle a CID path or container name.
        with self.assertRaises(TypeError):
            build_transport.DockerBuildRequestV1(
                1,
                capability,
                _bundle(1024),
                1024,
                Path("/tmp/foreign.cid"),
                "labcolors-arb-build-v1-foreign",
            )

    def test_native_adapter_mints_private_docker_issued_cleanup_authority(self) -> None:
        backend = build_transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            platform_name="linux",
            machine_name="x86_64",
            host_user=(501, 20),
        )
        capability = _probe_native_backend(
            backend,
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
        )
        request = build_transport.DockerBuildRequestV1(
            1,
            capability,
            _bundle(1024),
            1024,
        )
        lease = backend._next_run_lease_v1(capability)
        try:
            command = backend._command_for_v1(request, lease)

            self.assertIn("--cidfile", command)
            self.assertNotIn("--name", command)
            self.assertFalse(hasattr(lease, "container_name"))
            self.assertTrue(lease.cid_file.is_absolute())
            self.assertFalse(lease.cid_file.exists())
        finally:
            backend._release_run_lease_v1(lease)

    def test_recipe_is_transport_agnostic_and_bootstrap_owns_binary_stdout(self) -> None:
        source = BUILD_RECIPE.read_text(encoding="utf-8")
        self.assertIn("readonly inputs=/build/snapshot/inputs", source)
        self.assertIn("readonly workspace=/build/snapshot/workspace", source)
        self.assertIn("readonly build=/build/work", source)
        self.assertNotIn("/out", source)
        self.assertNotIn(">&3", source)
        self.assertIn("exec 3>&1", pipeline._BUILD_BOOTSTRAP_V1)
        self.assertIn("/build/work/arb-evaluator-v1 >&3", pipeline._BUILD_BOOTSTRAP_V1)

    def test_native_gate_executes_the_one_shot_receipt_controller(self) -> None:
        gate_source = NATIVE_GATE.read_text(encoding="utf-8")
        receipt_source = (TESTS / "test_receipt.py").read_text(encoding="utf-8")
        self.assertIn('"receipt"', gate_source)
        self.assertIn("NativeSourceBoundReceiptIntegrationTests", gate_source)
        self.assertIn("SourceBoundArbControllerV1", receipt_source)
        self.assertIn("SourceBoundEvaluatorReceiptV1", receipt_source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
