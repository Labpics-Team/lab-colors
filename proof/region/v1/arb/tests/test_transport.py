#!/usr/bin/env python3
"""Behavioral contract for the causal controller-to-Docker BUILD transport."""

from __future__ import annotations

import dataclasses
import hashlib
import io
import inspect
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

import pipeline  # noqa: E402
from test_pipeline import _request  # noqa: E402


BUILD_RECIPE = ARB / "build.sh"
NATIVE_GATE = ARB / "tests" / "native_gate.py"


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


def _bundle(length: int = 1024 * 1024) -> pipeline.SealedBuildInputBundleV1:
    contents = (b"0123456789abcdef" * ((length + 15) // 16))[:length]
    return pipeline.SealedBuildInputBundleV1(
        _digest("source"),
        _digest("build-input"),
        contents,
        _token=pipeline._BUILD_INPUT_BUNDLE_TOKEN,
    )


def _backend() -> pipeline.NativeDockerBuildBackendV1:
    return pipeline.NativeDockerBuildBackendV1(
        Path("/bin/true"),
        platform_name="linux",
        machine_name="x86_64",
    )


def _observe(
    source: str,
    bundle: pipeline.SealedBuildInputBundleV1,
    *,
    stdout_limit: int = 2 * 1024 * 1024,
    stderr_limit: int = 2 * 1024 * 1024,
    timeout_ns: int = 5_000_000_000,
) -> pipeline.DockerBuildProcessObservationV1:
    return _backend()._observe_command(
        (sys.executable, "-c", source),
        stdout_limit=stdout_limit,
        stderr_limit=stderr_limit,
        timeout_ns=timeout_ns,
        cid_file=None,
        input_bundle=bundle,
    )


class CanonicalBuildBundleTests(unittest.TestCase):
    def test_bundle_is_reproducible_normalized_ustar_with_no_host_authority(self) -> None:
        request = _request()
        first = pipeline._seal_build_input_bundle_v1(request)
        second = pipeline._seal_build_input_bundle_v1(request)

        self.assertIsNot(first, second)
        self.assertIs(first._contents, first._contents)
        self.assertEqual(first._contents, second._contents)
        self.assertEqual(first.sha256, second.sha256)
        self.assertEqual(first.identity, second.identity)
        self.assertTrue(pipeline.sealed_build_input_bundle_is_well_bound_v1(first))

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

        with tarfile.open(fileobj=io.BytesIO(first._contents), mode="r:") as archive:
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
        entries = (("a/b", 0o644, b"x"), ("c", 0o755, b"y"))
        encoded = pipeline._canonical_tar_v1(entries)
        self.assertEqual(
            hashlib.sha256(encoded).hexdigest(),
            "11bc313cba907e89535876eb8ce46194472367007053ab58b723338676f99427",
        )
        with self.assertRaises(TypeError):
            pipeline._canonical_tar_v1(tuple(reversed(entries)))
        with self.assertRaises(TypeError):
            pipeline._canonical_tar_v1((("a", 0o644, b"x"), ("a/b", 0o644, b"y")))
        with self.assertRaises(TypeError):
            pipeline._canonical_tar_v1((("A", 0o644, b"x"), ("a", 0o644, b"y")))
        with self.assertRaises((TypeError, ValueError)):
            pipeline._canonical_tar_v1((("a" * 256, 0o644, b"x"),))

    def test_omission_or_content_mutation_changes_bundle_identity(self) -> None:
        entries = (("a", 0o644, b"x"), ("b", 0o644, b"y"))
        original = pipeline._canonical_tar_v1(entries)
        omitted = pipeline._canonical_tar_v1(entries[:1])
        mutated = pipeline._canonical_tar_v1(
            (("a", 0o644, b"x"), ("b", 0o644, b"z"))
        )
        identities = {
            pipeline.SealedBuildInputBundleV1(
                _digest("source"),
                _digest("build-input"),
                body,
                _token=pipeline._BUILD_INPUT_BUNDLE_TOKEN,
            ).identity
            for body in (original, omitted, mutated)
        }
        self.assertEqual(len(identities), 3)

    def test_replayed_source_coordinates_must_match_the_admitted_capability(self) -> None:
        request = _request()
        admitted = request.admitted_sources.sources[0]
        original = admitted.tree_identity
        object.__setattr__(admitted, "tree_identity", _digest("mutated-tree"))
        try:
            with self.assertRaises(TypeError):
                pipeline._normalized_source_entries_v1(
                    request.source_lock.sources[0],
                    admitted,
                )
        finally:
            object.__setattr__(admitted, "tree_identity", original)

    def test_transport_authorities_cannot_be_directly_forged(self) -> None:
        bundle = _bundle(1024)
        with self.assertRaises(TypeError):
            pipeline.BuildInputTransferProgressV1(
                bundle.identity,
                bundle.length,
                bundle.sha256,
                bundle.length,
                bundle.sha256,
            )
        with self.assertRaises(TypeError):
            pipeline.BuildInputTransferV1(object())
        with self.assertRaises(TypeError):
            pipeline.DockerBuildExitedV1(0, b"binary", b"", object())


class BuildInputObserverTests(unittest.TestCase):
    def test_positive_partial_writes_are_normal_and_commit_exact_transfer(self) -> None:
        bundle = _bundle(256 * 1024)
        real_write = os.write

        def partial_write(descriptor: int, contents: object) -> int:
            return real_write(descriptor, contents[:997])

        with mock.patch.object(pipeline.os, "write", side_effect=partial_write):
            result = _observe(
                "import hashlib,sys; d=sys.stdin.buffer.read(); "
                "sys.stdout.buffer.write(hashlib.sha256(d).digest()); "
                "sys.stderr.buffer.write(b'observed')",
                bundle,
            )

        self.assertIs(type(result), pipeline.DockerBuildExitedV1, result)
        self.assertEqual(result.stdout, bundle.sha256)
        self.assertEqual(result.stderr, b"observed")
        self.assertEqual(result.input_transfer.bundle_identity, bundle.identity)
        self.assertEqual(result.input_transfer.expected_length, bundle.length)
        self.assertEqual(result.input_transfer.expected_sha256, bundle.sha256)
        self.assertEqual(result.input_transfer.written_length, bundle.length)
        self.assertEqual(result.input_transfer.written_sha256, bundle.sha256)

    def test_zero_write_and_epipe_are_typed_with_exact_partial_progress(self) -> None:
        bundle = _bundle()
        with mock.patch.object(pipeline.os, "write", return_value=0):
            zero = _observe("import sys; sys.stdin.buffer.read()", bundle)
        self.assertIs(type(zero), pipeline.DockerBuildInputRejectedV1, zero)
        self.assertEqual(zero.written_length, 0)
        self.assertEqual(zero.written_sha256, hashlib.sha256(b"").digest())

        closed = _observe("import os,time; os.close(0); time.sleep(1)", bundle)
        self.assertIs(type(closed), pipeline.DockerBuildInputRejectedV1, closed)
        self.assertLess(closed.written_length, bundle.length)
        self.assertEqual(
            closed.written_sha256,
            hashlib.sha256(bundle._contents[: closed.written_length]).digest(),
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

        with mock.patch.object(pipeline.subprocess, "Popen", side_effect=spawn):
            result = _observe(
                "import time; time.sleep(1)",
                _bundle(2 * 1024 * 1024),
                timeout_ns=100_000_000,
            )

        self.assertIs(type(result), pipeline.DockerBuildObserverFailureV1, result)

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
        self.assertIs(type(result), pipeline.DockerBuildExitedV1, result)
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
        self.assertIs(type(timed), pipeline.DockerBuildTimedOutV1, timed)
        self.assertIs(type(timed.input_progress), pipeline.BuildInputTransferProgressV1)
        self.assertGreater(timed.input_progress.written_length, 0)
        self.assertLess(timed.input_progress.written_length, bundle.length)

        limited = _observe(
            "import os,time; os.write(1,b'x'*65536); time.sleep(2)",
            bundle,
            stdout_limit=8,
        )
        self.assertIs(type(limited), pipeline.DockerBuildOutputLimitV1, limited)
        self.assertEqual(limited.stream, pipeline.DockerOutputStreamV1.STDOUT)
        self.assertEqual(limited.stdout, b"x" * 8)
        self.assertIs(type(limited.input_progress), pipeline.BuildInputTransferProgressV1)

    def test_cleanup_failure_preserves_input_trigger_and_progress(self) -> None:
        bundle = _bundle()
        backend = _backend()
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            backend,
            "_cleanup_container",
            return_value="forced cleanup failure",
        ):
            result = backend._observe_command(
                (sys.executable, "-c", "import os,time; os.close(0); time.sleep(1)"),
                stdout_limit=1024,
                stderr_limit=1024,
                timeout_ns=5_000_000_000,
                cid_file=Path(temporary).resolve() / "container.cid",
                container_name="labcolors-arb-build-v1-transport-test",
                input_bundle=bundle,
            )
        self.assertIs(type(result), pipeline.DockerBuildCleanupFailureV1, result)
        self.assertEqual(result.trigger, pipeline.DockerCleanupTriggerV1.INPUT_TRANSFER)
        self.assertEqual(result.detail, "forced cleanup failure")
        self.assertIs(type(result.input_progress), pipeline.BuildInputTransferProgressV1)
        self.assertLess(result.input_progress.written_length, bundle.length)


class SealedBuildTransportContractTests(unittest.TestCase):
    def test_controller_owns_one_sealed_bundle_for_both_builds(self) -> None:
        build_source = inspect.getsource(pipeline.ControlledPipelineV1.build)
        self.assertEqual(build_source.count("_seal_build_input_bundle_v1("), 1)
        self.assertIn("for attempt in (1, 2)", build_source)

    def test_docker_request_has_no_semantic_host_path_authority(self) -> None:
        fields = {item.name for item in dataclasses.fields(pipeline.DockerBuildRequestV1)}
        self.assertEqual(
            fields,
            {"attempt", "input_bundle", "max_executable_bytes", "cid_file", "container_name"},
        )
        command = pipeline.NativeDockerBuildBackendV1(
            Path("/usr/bin/docker"),
            platform_name="linux",
            machine_name="x86_64",
        ).command_for(
            pipeline.DockerBuildRequestV1(
                1,
                _bundle(1024),
                1024,
                Path("/tmp/container.cid"),
                "labcolors-arb-build-v1-command-test",
            )
        )
        self.assertNotIn("--mount", command)
        self.assertEqual(command.count("--tmpfs"), 2)
        self.assertIn(pipeline._BUILD_TMPFS_SPEC_V1, command)
        self.assertIn(pipeline._BUILD_STATE_TMPFS_SPEC_V1, command)
        self.assertIn("--interactive", command)
        self.assertIn("/usr/bin/env", command)
        self.assertIn("-i", command)

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
