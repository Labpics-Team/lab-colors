#!/usr/bin/env python3
"""Behavioral contract for the causal controller-to-Docker BUILD transport."""

from __future__ import annotations

import ast
from collections.abc import Callable
import hashlib
import io
import inspect
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import textwrap
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
import provenance  # noqa: E402
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


def _native_backend_with_request(
    bundle: build_input.SealedInputV1 | None = None,
) -> tuple[
    build_transport.NativeDockerBuildBackendV1,
    build_transport.DockerBuildRequestV1,
]:
    """One fixture owns the native request shape used by cleanup tests."""

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
    return backend, build_transport.DockerBuildRequestV1(
        1,
        capability,
        _bundle(1024) if bundle is None else bundle,
        1024,
    )


def _report_released_cid_root(
    backend: build_transport.NativeDockerBuildBackendV1,
    detail: object = "forced CID-root cleanup failure",
) -> Callable[[object], object]:
    """Keep failure fixtures honest: a reported cleanup failure follows release."""

    release = backend._release_run_lease_v1

    def release_then_report(lease: object) -> object:
        if release(lease) is not None:
            raise AssertionError("native CID-root fixture lease did not release")
        return detail

    return release_then_report


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
    def test_public_probe_starts_with_a_typed_terminal_outcome(self) -> None:
        """A backend cannot leave the public probe in a non-report state."""

        source = inspect.getsource(build_transport.ControlledBuildTransportV1.probe)
        tree = ast.parse(textwrap.dedent(source))
        function = tree.body[0]
        if not isinstance(function, ast.FunctionDef):
            self.fail("public probe source must remain one function definition")
        typed_initializers = [
            node
            for node in function.body
            if isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id == "outcome"
            and isinstance(node.annotation, ast.Name)
            and node.annotation.id == "DockerCapabilityReportV1"
            and isinstance(node.value, ast.Call)
            and isinstance(node.value.func, ast.Name)
            and node.value.func.id == "DockerUnsupportedV1"
        ]
        self.assertEqual(len(typed_initializers), 1)
        self.assertNotIn('raise RuntimeError("probe outcome was not produced")', source)

    def test_probe_releases_its_transient_lease_after_backend_failure(self) -> None:
        report = _docker_capability(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)

        class FlakyProbeBackend:
            def __init__(self) -> None:
                self.calls = 0

            def probe(self) -> object:
                self.calls += 1
                if self.calls == 1:
                    raise ValueError("forced backend failure")
                return report

        backend = FlakyProbeBackend()
        controller = build_transport.ControlledBuildTransportV1(
            policy=pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            backend=backend,
        )
        first = controller.probe()
        second = controller.probe()

        self.assertIs(type(first), build_transport.DockerUnsupportedV1)
        self.assertEqual(
            first.reason,
            build_transport.DockerBlockerReasonV1.BACKEND_CONTRACT,
        )
        self.assertIs(second, report)
        self.assertEqual(backend.calls, 2)

    def test_public_input_constructors_use_typed_value_errors(self) -> None:
        def reject(
            constructor: Callable[[], object],
            reason: build_input.InputReasonV1,
            field: str,
        ) -> None:
            with self.assertRaises(build_input.InputErrorV1) as caught:
                constructor()
            self.assertEqual(caught.exception.reason, reason)
            self.assertEqual(caught.exception.field, field)

        def limits(
            max_members: object = 1,
            max_file_bytes: object = 1,
            max_payload_bytes: object = 1,
            max_encoded_bytes: object = None,
        ) -> object:
            return build_input.CanonicalInputLimitsV1(
                max_members,
                max_file_bytes,
                max_payload_bytes,
                max_encoded_bytes,
            )

        def seal(
            binding_identity: object = _digest("binding"),
            contents: object = b"x",
        ) -> object:
            return build_input.seal_input_v1(binding_identity, contents)

        limit_fields = (
            "max_members",
            "max_file_bytes",
            "max_payload_bytes",
            "max_encoded_bytes",
        )
        self.assertIs(
            type(build_input.CanonicalInputLimitsV1(1, 1, 1, None)),
            build_input.CanonicalInputLimitsV1,
        )
        for field, value in (
            ("max_members", True),
            ("max_file_bytes", 1.0),
            ("max_payload_bytes", object()),
            ("max_encoded_bytes", b"1"),
        ):
            with self.subTest(kind="wrong_type", field=field):
                reject(
                    lambda field=field, value=value: limits(**{field: value}),
                    build_input.InputReasonV1.WRONG_TYPE,
                    field,
                )
        for field in limit_fields:
            for value in (0, 1 << 64):
                with self.subTest(kind="invalid_limit", field=field, value=value):
                    reject(
                        lambda field=field, value=value: limits(**{field: value}),
                        build_input.InputReasonV1.INVALID_VALUE,
                        field,
                    )
        reject(
            lambda: limits(max_payload_bytes=(1 << 64) - 1),
            build_input.InputReasonV1.INVALID_VALUE,
            "max_encoded_bytes",
        )

        for field, constructor in (
            ("binding_identity", lambda: seal(bytearray(_digest("binding")))),
            ("contents", lambda: seal(contents=bytearray(b"x"))),
        ):
            with self.subTest(kind="wrong_type", field=field):
                reject(constructor, build_input.InputReasonV1.WRONG_TYPE, field)
        for field, constructor in (
            ("binding_identity", lambda: seal(bytes(32))),
            ("binding_identity", lambda: seal(b"x" * 31)),
            ("contents", lambda: seal(contents=b"")),
        ):
            with self.subTest(kind="invalid_value", field=field):
                reject(constructor, build_input.InputReasonV1.INVALID_VALUE, field)

        for constructor in (
            lambda: build_input.CanonicalInputLimitsV1(1, 1),
            lambda: build_input.seal_input_v1(_digest("binding")),
            lambda: build_input.SealedInputV1(
                _digest("binding"),
                b"x",
                _token=object(),
            ),
        ):
            with self.subTest(kind="private_or_call_shape"):
                with self.assertRaises(TypeError):
                    constructor()

    def test_bundle_is_reproducible_normalized_ustar_with_no_host_authority(self) -> None:
        request = _request()
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        first = pipeline._seal_build_input_bundle_v1(request, policy)
        second = pipeline._seal_build_input_bundle_v1(request, policy)

        self.assertIsNot(first, second)
        self.assertIs(first.contents, first.contents)
        self.assertEqual(first.contents, second.contents)
        self.assertEqual(first.sha256, second.sha256)
        self.assertEqual(first.binding_identity, second.binding_identity)
        self.assertTrue(pipeline.arb_input_is_bound_v1(request, policy, first))
        self.assertEqual(
            first.sha256.hex(),
            "5d6e789a721aeed1a8ff023f0af5389711f85f6fe95294d8290b20301235f4df",
        )
        self.assertEqual(first.length, 174_080)

        source_entries = tuple(
            (
                f"inputs/{lock.root_prefix[:-1]}/{relative}",
                mode,
                contents,
            )
            for lock, admitted in zip(
                request.source_lock.sources,
                request.admitted_sources.sources,
                strict=True,
            )
            for relative, mode, contents in provenance.materialize_admitted_source_files_v1(
                lock,
                admitted,
            )
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
            with self.assertRaises(provenance.ProvenanceErrorV1):
                provenance.materialize_admitted_source_files_v1(
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
    def test_diagnostic_details_have_one_strict_admission_law(self) -> None:
        constructors = (
            (
                "unsupported",
                lambda detail: build_transport.DockerUnsupportedV1(
                    build_transport.DockerBlockerReasonV1.BACKEND_CONTRACT,
                    detail,
                ),
            ),
            (
                "observer-failure",
                lambda detail: build_transport.DockerBuildObserverFailureV1(
                    detail,
                    b"",
                    b"",
                ),
            ),
            (
                "cleanup-record",
                lambda detail: build_transport.CleanupFailureRecordV1(
                    build_transport.CleanupResourceV1.DOCKER_CID_ROOT,
                    detail,
                ),
            ),
        )

        class DetailSubclass(str):
            pass

        for constructor_name, constructor in constructors:
            with self.subTest(constructor=constructor_name, detail="valid"):
                self.assertIsNotNone(constructor("valid diagnostic detail"))
            for name, invalid_detail in (
                ("empty", ""),
                ("subclass", DetailSubclass("detail")),
                (
                    "too-long",
                    "x" * (build_transport._DIAGNOSTIC_DETAIL_TEXT_LIMIT_V1 + 1),
                ),
                ("wrong-type", object()),
            ):
                with self.subTest(constructor=constructor_name, detail=name):
                    with self.assertRaises(TypeError):
                        constructor(invalid_detail)

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
        # The field assertion above proves no cleanup coordinate is modeled.
        # This call separately guards the fixed-arity boundary against extras.
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
        backend, request = _native_backend_with_request()
        capability = request.capability
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

    def test_native_adapter_rejects_malformed_nominal_observations_typed(self) -> None:
        raw_observations = (
            ("unknown", object()),
            *(
                (kind.__name__, tuple.__new__(kind, ()))
                for kind in (
                    build_transport.DockerBuildExitedV1,
                    build_transport.DockerBuildTimedOutV1,
                    build_transport.DockerBuildOutputLimitV1,
                    build_transport.DockerBuildObserverFailureV1,
                    build_transport.DockerBuildInputRejectedV1,
                    build_transport.DockerBuildCleanupFailureV1,
                )
            ),
            (
                "DockerBuildObserverFailureV1/invalid-progress",
                tuple.__new__(
                    build_transport.DockerBuildObserverFailureV1,
                    ("forged", b"untrusted stdout", b"untrusted stderr", object()),
                ),
            ),
        )

        def observe(raw: object, *, cleanup_fails: bool) -> object:
            backend, request = _native_backend_with_request()
            if not cleanup_fails:
                with mock.patch.object(
                    backend,
                    "_observe_command",
                    return_value=raw,
                ):
                    return backend.run_build(request)

            with mock.patch.object(
                backend,
                "_observe_command",
                return_value=raw,
            ), mock.patch.object(
                backend,
                "_release_run_lease_v1",
                side_effect=_report_released_cid_root(backend),
            ):
                return backend.run_build(request)

        for name, raw in raw_observations:
            with self.subTest(observation=name, cleanup_fails=False):
                without_cleanup_failure = observe(raw, cleanup_fails=False)
                self.assertIs(
                    type(without_cleanup_failure),
                    build_transport.DockerBuildObserverFailureV1,
                )
                self.assertEqual(
                    without_cleanup_failure.detail,
                    "native Docker build observation is not canonical",
                )
                self.assertEqual(without_cleanup_failure.stdout, b"")
                self.assertEqual(without_cleanup_failure.stderr, b"")
                self.assertIsNone(without_cleanup_failure.input_progress)
            with self.subTest(observation=name, cleanup_fails=True):
                with_cleanup_failure = observe(raw, cleanup_fails=True)
                self.assertIs(
                    type(with_cleanup_failure),
                    build_transport.DockerBuildObserverFailureV1,
                )
                self.assertEqual(
                    with_cleanup_failure.detail,
                    "native Docker build observation is not canonical; "
                    "forced CID-root cleanup failure",
                )
                self.assertEqual(with_cleanup_failure.stdout, b"")
                self.assertEqual(with_cleanup_failure.stderr, b"")
                self.assertIsNone(with_cleanup_failure.input_progress)

    def test_native_adapter_rejects_a_preexisting_cid_root_claim(self) -> None:
        bundle = _bundle(1024)
        backend, request = _native_backend_with_request(bundle)
        raw = build_transport.DockerBuildCleanupFailureV1(
            build_transport.DockerCleanupTriggerV1.PROCESS_EXIT,
            (
                build_transport.CleanupFailureRecordV1(
                    build_transport.CleanupResourceV1.DOCKER_CID_ROOT,
                    "forged prior CID-root cleanup failure",
                ),
            ),
            b"retained stdout",
            b"retained stderr",
            build_transport._build_input_progress_v1(
                bundle,
                bundle.length,
                bundle.sha256,
            ),
        )
        with mock.patch.object(
            backend,
            "_observe_command",
            return_value=raw,
        ):
            without_cleanup_failure = backend.run_build(request)

        self.assertIs(
            type(without_cleanup_failure),
            build_transport.DockerBuildObserverFailureV1,
        )
        self.assertEqual(
            without_cleanup_failure.detail,
            "native Docker build observation already contains a CID-root "
            "cleanup failure",
        )
        self.assertEqual(without_cleanup_failure.stdout, b"retained stdout")
        self.assertEqual(without_cleanup_failure.stderr, b"retained stderr")
        self.assertIs(without_cleanup_failure.input_progress, raw.input_progress)

        with mock.patch.object(
            backend,
            "_observe_command",
            return_value=raw,
        ), mock.patch.object(
            backend,
            "_release_run_lease_v1",
            side_effect=_report_released_cid_root(backend),
        ):
            with_cleanup_failure = backend.run_build(request)

        self.assertIs(
            type(with_cleanup_failure),
            build_transport.DockerBuildCleanupFailureV1,
        )
        self.assertEqual(
            with_cleanup_failure.trigger,
            build_transport.DockerCleanupTriggerV1.OBSERVER_FAILURE,
        )
        self.assertEqual(
            with_cleanup_failure.failures,
            (
                build_transport.CleanupFailureRecordV1(
                    build_transport.CleanupResourceV1.DOCKER_CID_ROOT,
                    "forced CID-root cleanup failure",
                ),
            ),
        )
        self.assertEqual(with_cleanup_failure.stdout, b"retained stdout")
        self.assertEqual(with_cleanup_failure.stderr, b"retained stderr")
        self.assertIs(with_cleanup_failure.input_progress, raw.input_progress)

    def test_native_adapter_bounds_unknown_observation_cleanup_detail(self) -> None:
        prefix = "native Docker build observation is not canonical; "
        for name, detail, expected_detail in (
            (
                "retained",
                "forced CID-root cleanup failure",
                prefix + "forced CID-root cleanup failure",
            ),
            (
                "bounded",
                "x" * build_transport._DIAGNOSTIC_DETAIL_TEXT_LIMIT_V1,
                prefix
                + "x"
                * (
                    build_transport._DIAGNOSTIC_DETAIL_TEXT_LIMIT_V1
                    - len(prefix)
                ),
            ),
        ):
            with self.subTest(detail=name):
                result = (
                    build_transport.NativeDockerBuildBackendV1._with_cid_root_cleanup_failure_v1(
                        object(),
                        detail,
                    )
                )

                self.assertIs(type(result), build_transport.DockerBuildObserverFailureV1)
                self.assertEqual(result.detail, expected_detail)
                self.assertEqual(result.stdout, b"")
                self.assertEqual(result.stderr, b"")
                self.assertIsNone(result.input_progress)

    def test_native_adapter_appends_cid_root_to_canonical_cleanup_prefix(self) -> None:
        bundle = _bundle(1024)
        backend, request = _native_backend_with_request(bundle)
        progress = build_transport._build_input_progress_v1(
            bundle,
            bundle.length,
            bundle.sha256,
        )
        raw = build_transport.DockerBuildCleanupFailureV1(
            build_transport.DockerCleanupTriggerV1.PROCESS_EXIT,
            (
                build_transport.CleanupFailureRecordV1(
                    build_transport.CleanupResourceV1.DOCKER_CLI_PROCESS,
                    "prior CLI cleanup failure",
                ),
                build_transport.CleanupFailureRecordV1(
                    build_transport.CleanupResourceV1.DOCKER_CONTAINER,
                    "prior container cleanup failure",
                ),
            ),
            b"retained stdout",
            b"retained stderr",
            progress,
        )
        with mock.patch.object(
            backend,
            "_observe_command",
            return_value=raw,
        ), mock.patch.object(
            backend,
            "_release_run_lease_v1",
            side_effect=_report_released_cid_root(backend),
        ):
            result = backend.run_build(request)

        self.assertIs(type(result), build_transport.DockerBuildCleanupFailureV1)
        self.assertEqual(result.trigger, build_transport.DockerCleanupTriggerV1.PROCESS_EXIT)
        self.assertEqual(
            tuple(record.resource for record in result.failures),
            (
                build_transport.CleanupResourceV1.DOCKER_CLI_PROCESS,
                build_transport.CleanupResourceV1.DOCKER_CONTAINER,
                build_transport.CleanupResourceV1.DOCKER_CID_ROOT,
            ),
        )
        self.assertEqual(
            tuple(record.detail for record in result.failures),
            (
                "prior CLI cleanup failure",
                "prior container cleanup failure",
                "forced CID-root cleanup failure",
            ),
        )
        self.assertEqual(result.stdout, b"retained stdout")
        self.assertEqual(result.stderr, b"retained stderr")
        self.assertIs(result.input_progress, progress)

    def test_native_adapter_reowns_invalid_cid_cleanup_details(self) -> None:
        bundle = _bundle(1024)
        backend, request = _native_backend_with_request(bundle)
        transfer = build_transport._completed_build_input_transfer_v1(
            bundle,
            bundle.length,
            bundle.sha256,
        )
        raw = build_transport._docker_build_exited_v1(
            0,
            b"built stdout",
            b"built stderr",
            transfer,
        )
        for name, invalid_detail in (
            ("empty", ""),
            ("wrong-type", object()),
            (
                "too-long",
                "x" * (build_transport._DIAGNOSTIC_DETAIL_TEXT_LIMIT_V1 + 1),
            ),
        ):
            with self.subTest(detail=name):

                with mock.patch.object(
                    backend,
                    "_observe_command",
                    return_value=raw,
                ), mock.patch.object(
                    backend,
                    "_release_run_lease_v1",
                    side_effect=_report_released_cid_root(
                        backend,
                        invalid_detail,
                    ),
                ):
                    result = backend.run_build(request)

                self.assertIs(type(result), build_transport.DockerBuildCleanupFailureV1)
                self.assertEqual(
                    result.detail,
                    "native Docker CID root cleanup detail is not canonical",
                )
                self.assertEqual(result.stdout, b"built stdout")
                self.assertEqual(result.stderr, b"built stderr")
                self.assertEqual(
                    result.input_progress,
                    build_transport._build_input_progress_v1(
                        bundle,
                        bundle.length,
                        bundle.sha256,
                    ),
                )

    def test_native_adapter_keeps_dual_failure_typed_at_detail_limit(self) -> None:
        backend, request = _native_backend_with_request()
        raw = build_transport.DockerBuildObserverFailureV1(
            "x" * build_transport._DIAGNOSTIC_DETAIL_TEXT_LIMIT_V1,
            b"retained stdout",
            b"retained stderr",
        )
        with mock.patch.object(
            backend,
            "_observe_command",
            return_value=raw,
        ), mock.patch.object(
            backend,
            "_release_run_lease_v1",
            side_effect=_report_released_cid_root(backend),
        ):
            result = backend.run_build(request)

        self.assertIs(type(result), build_transport.DockerBuildObserverFailureV1)
        self.assertEqual(
            result.detail,
            "native Docker build observation and CID root cleanup both failed",
        )
        self.assertEqual(result.stdout, b"retained stdout")
        self.assertEqual(result.stderr, b"retained stderr")

    def test_native_adapter_releases_before_propagating_first_canonicalization_interruption(
        self,
    ) -> None:
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
        release = backend._release_run_lease_v1
        released: list[None] = []

        class FalseyInterrupt(BaseException):
            def __bool__(self) -> bool:
                return False

        original = FalseyInterrupt("first interruption")

        def release_then_record(lease: object) -> None:
            self.assertIsNone(release(lease))
            released.append(None)
            raise KeyboardInterrupt("later cleanup interruption")

        with mock.patch.object(
            backend,
            "_observe_command",
            return_value=object(),
        ), mock.patch.object(
            build_transport,
            "_canonical_process_observation_v1",
            side_effect=original,
        ), mock.patch.object(
            backend,
            "_release_run_lease_v1",
            side_effect=release_then_record,
        ):
            with self.assertRaises(FalseyInterrupt) as raised:
                backend.run_build(request)

        self.assertIs(raised.exception, original)
        self.assertEqual(released, [None])

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
