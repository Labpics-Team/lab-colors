#!/usr/bin/env python3
"""RED contract for an identity-preserving engine-neutral BUILD leaf."""

from __future__ import annotations

import ast
import hashlib
import importlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[1]
ARB = PROOF / "arb"
ARB_TESTS = ARB / "tests"
sys.path[:0] = (str(PROOF), str(ARB), str(ARB_TESTS))

import pipeline  # noqa: E402
from proof.region.v1.arb.tests import gate as arb_gate  # noqa: E402
from test_pipeline import (  # noqa: E402
    _docker_capability,
    _probe_native_backend,
    _request,
)
from test_receipt import _execute  # noqa: E402


# These inventory goldens describe the complete Arb gate. Identity-version
# tests deliberately change this inventory and must update both values from the
# gate's independent enumeration in the same slice.
ARB_INVENTORY_SHA256_V1 = (
    "383672bd1ac2a2d472fdba33d3ec4c770a897ecd13192ec41e7c12dc1e563219"
)
ARB_ORDER_SHA256_V1 = (
    "c020118a926070e36f14757f0b281ac05dc460b8affa03947f827baa73d5d172"
)

MOVED_INPUT_SURFACE_V1 = (
    "CanonicalInputLimitsV1",
    "SealedInputV1",
    "seal_input_v1",
    "sealed_input_is_intact_v1",
    "canonical_ustar_v1",
)

MOVED_TRANSPORT_SURFACE_V1 = (
    "DockerBuildPolicyV1",
    "DockerUserModeV1",
    "DockerBlockerReasonV1",
    "DockerUnsupportedV1",
    "DockerSupportedV1",
    "DockerDaemonObservationV1",
    "NativeCommandCoordinateV1",
    "DockerBuildRequestV1",
    "transport_policy_identity_v1",
    "native_command_contract_identity_v1",
    "native_command_coordinate_v1",
    "docker_capability_identity_v1",
    "BuildInputTransferProgressV1",
    "BuildInputTransferV1",
    "DockerBuildExitedV1",
    "DockerBuildTimedOutV1",
    "DockerOutputStreamV1",
    "DockerBuildOutputLimitV1",
    "DockerBuildObserverFailureV1",
    "DockerBuildInputRejectedV1",
    "DockerCleanupTriggerV1",
    "CleanupResourceV1",
    "CleanupFailureRecordV1",
    "DockerBuildCleanupFailureV1",
    "BuildCleanupFailureV1",
    "DockerBuildBackendV1",
    "NativeDockerBuildBackendV1",
    "ControlledBuildTransportV1",
    "BuildFailureReasonV1",
    "BuildRejectedV1",
    "BuildByteRelationV1",
    "TwoBuildObservationV1",
    "build_process_bytes_v1",
)

REMOVED_TRANSPORT_SURFACE_V1 = (
    "NonReproducibleBuildV1",
    "ReproducibleBuildV1",
    "docker_report_matches_policy_v1",
)

FORBIDDEN_INPUT_IMPORTS_V1 = (
    "arb",
    "mpfi",
    "pipeline",
    "formula",
    "comparator",
    "receipt",
    "region_proof_protocol",
    "provenance",
)

FORBIDDEN_TRANSPORT_IMPORTS_V1 = FORBIDDEN_INPUT_IMPORTS_V1 + ("provenance",)


def _imported_modules(source: str) -> tuple[str, ...]:
    modules: list[str] = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Import):
            modules.extend(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            modules.append(node.module or "")
    return tuple(modules)


def _digest(label: str) -> bytes:
    return hashlib.sha256(label.encode("ascii")).digest()


def _sealed_input() -> object:
    build_input = importlib.import_module("build.input")
    return build_input.seal_input_v1(_digest("generic-build-binding"), b"input")


def _docker_capability_fixture(policy: object) -> object:
    return _docker_capability(policy)


def _completed_process(
    transport: object,
    input_value: object,
    stdout: bytes,
    *,
    returncode: int = 0,
    stderr: bytes = b"",
) -> object:
    transfer = transport._completed_build_input_transfer_v1(
        input_value,
        input_value.length,
        input_value.sha256,
    )
    return transport._docker_build_exited_v1(
        returncode,
        stdout,
        stderr,
        transfer,
    )


def _initial_progress(transport: object, input_value: object) -> object:
    return transport._build_input_progress_v1(
        input_value,
        0,
        hashlib.sha256(b"").digest(),
    )


def _forged_exact_type(value_type: type[object]) -> object:
    if issubclass(value_type, tuple):
        return tuple.__new__(value_type, ())
    return object.__new__(value_type)


class _ScriptedBuildBackend:
    def __init__(self, report: object, observations: tuple[object, ...]) -> None:
        self._report = report
        self._observations = list(observations)
        self.requests: list[object] = []

    def probe(self) -> object:
        return self._report

    def run_build(self, request: object) -> object:
        self.requests.append(request)
        return self._observations.pop(0)


def _controlled_build(
    transport: object,
    policy: object,
    observations: tuple[object, ...],
    *,
    max_output_bytes: int = 64,
    input_value: object | None = None,
) -> tuple[object, _ScriptedBuildBackend, object, object]:
    if input_value is None:
        input_value = _sealed_input()
    capability = _docker_capability_fixture(policy)
    backend = _ScriptedBuildBackend(capability, observations)
    controller = transport.ControlledBuildTransportV1(
        policy=policy,
        backend=backend,
    )
    owned_capability = controller.probe()
    result = controller.build(
        owned_capability,
        input_value,
        max_output_bytes,
        input_admission=lambda _value: True,
        output_admission=lambda _value: True,
    )
    return result, backend, owned_capability, input_value


class ExistingArbGateTests(unittest.TestCase):
    def test_existing_arb_suite_keeps_exact_count_order_and_inventory(self) -> None:
        tests = tuple(arb_gate._iter_tests_v1(arb_gate.full_suite_v1()))
        identifiers = tuple(test.id() for test in tests)
        ordered_preimage = b"".join(
            identifier.encode("utf-8") + b"\n" for identifier in identifiers
        )

        self.assertEqual(len(identifiers), 160)
        self.assertEqual(len(set(identifiers)), 160)
        self.assertEqual(
            arb_gate.test_inventory_sha256_v1(arb_gate.full_suite_v1()),
            ARB_INVENTORY_SHA256_V1,
        )
        self.assertEqual(
            hashlib.sha256(ordered_preimage).hexdigest(),
            ARB_ORDER_SHA256_V1,
        )


class ArbBuildIdentityCharacterizationTests(unittest.TestCase):
    def test_arb_input_and_process_stay_exact_while_capability_identities_move_to_v2(self) -> None:
        transport = importlib.import_module("build.transport")
        request = _request()
        result, _backend = _execute()
        observed = result.evidence.build
        process_bytes = transport.build_process_bytes_v1(
            observed.build_processes[0]
        )

        self.assertEqual(observed.input_bundle_length, 174_080)
        self.assertEqual(
            observed.input_bundle_sha256.hex(),
            "5d6e789a721aeed1a8ff023f0af5389711f85f6fe95294d8290b20301235f4df",
        )
        self.assertEqual(
            observed.input_bundle_identity.hex(),
            "6e88d9105d581ef1898dd1b0ac2ee6362c1bf15e8495990e1518fba35e7a8bd0",
        )
        self.assertEqual(
            pipeline.pipeline_policy_identity_v2(
                request.host_trust,
                observed.docker_capability.policy,
            ).hex(),
            "66af6f844dda8eae548eac026f277845ccde1842c14c39824c5108f027247f39",
        )
        self.assertEqual(len(process_bytes), 196)
        self.assertEqual(
            hashlib.sha256(process_bytes).hexdigest(),
            "401aaf23753b09b35482080e6046499e6a8a0a4ea2cea6c658ed377efebac58c",
        )
        self.assertEqual(
            result.comparator.identity.hex(),
            "e4e8e4dd47ddda5585531f67bfe3112f157a032cb728cd1c61766edf26de6c6c",
        )
        self.assertEqual(
            result.evidence.source_identity.hex(),
            "07d85ad695ec17104bdb34f6e9819d25be08afb3aa485918c44a363d7679f7c9",
        )
        self.assertEqual(
            result.evidence.build_identity.hex(),
            "dfe01f51132d938be3f8a8fad32c91d99fc7b22d69c4c9f488c07f2d00806412",
        )
        self.assertEqual(
            result.evidence.run_identity.hex(),
            "0033f6e70d0090ff2839d364cccaf1a3f5bf79eb2c857ce762b236cbbc730542",
        )
        self.assertEqual(
            result.evidence.identity.hex(),
            "80dd866e156d882a749a78b768cfc66838f92f6608baaf9ddfbf3f3a31870324",
        )
        self.assertEqual(
            result.claim.identity.hex(),
            "c0c200282fc3cd800bb0aa53a8e3c2d3fa2edf1a6350185aeff86f410ccef1bb",
        )


class SharedBuildExtractionTests(unittest.TestCase):
    def test_build_namespace_has_two_focused_shared_leaves(self) -> None:
        package = importlib.import_module("build")
        build_input = importlib.import_module("build.input")
        transport = importlib.import_module("build.transport")
        namespace = PROOF / "build"

        self.assertEqual(
            Path(package.__file__).resolve(),
            (namespace / "__init__.py").resolve(),
        )
        self.assertEqual(
            tuple(Path(item).resolve() for item in package.__path__),
            (namespace.resolve(),),
        )
        self.assertFalse((PROOF / "build.py").exists())
        self.assertEqual(
            Path(build_input.__file__).resolve(),
            (namespace / "input.py").resolve(),
        )
        self.assertEqual(
            Path(transport.__file__).resolve(),
            (namespace / "transport.py").resolve(),
        )
        self.assertFalse((ARB / "build").exists())
        self.assertFalse((PROOF / "mpfi/build").exists())
        self.assertIs(transport.input, build_input)
        self.assertFalse(hasattr(build_input, "normalized_source_entries_v1"))
        for name in MOVED_INPUT_SURFACE_V1:
            with self.subTest(name=name):
                self.assertTrue(hasattr(build_input, name))
        for name in MOVED_TRANSPORT_SURFACE_V1:
            with self.subTest(name=name):
                self.assertTrue(hasattr(transport, name))
        for name in REMOVED_TRANSPORT_SURFACE_V1:
            with self.subTest(removed=name):
                self.assertFalse(hasattr(transport, name))

    def test_shared_leaves_import_no_engine_or_proof_semantics(self) -> None:
        surfaces = (
            (
                importlib.import_module("build.input"),
                FORBIDDEN_INPUT_IMPORTS_V1,
            ),
            (
                importlib.import_module("build.transport"),
                FORBIDDEN_TRANSPORT_IMPORTS_V1,
            ),
        )
        for surface, forbidden_imports in surfaces:
            source = Path(surface.__file__).read_text(encoding="utf-8")
            for module in _imported_modules(source):
                with self.subTest(surface=surface.__name__, module=module):
                    top_level = module.lstrip(".").split(".", 1)[0].lower()
                    self.assertNotIn(top_level, forbidden_imports, module)

    def test_observation_contract_uses_current_non_claiming_language(self) -> None:
        transport_source = (PROOF / "build" / "transport.py").read_text(
            encoding="utf-8"
        )
        pipeline_source = (ARB / "pipeline.py").read_text(encoding="utf-8")
        self.assertNotIn(
            "backend-contract rejection cannot retain authority",
            transport_source,
        )
        self.assertNotIn("invalid reproducible-build digests", pipeline_source)

    def test_arb_consumers_move_atomically_without_compatibility_reexports(self) -> None:
        build_input = importlib.import_module("build.input")
        transport = importlib.import_module("build.transport")
        arb_pipeline = pipeline
        arb_receipt = importlib.import_module("receipt")
        request = _request()
        bundle = arb_pipeline._seal_build_input_bundle_v1(request)

        self.assertIs(arb_pipeline.build_input, build_input)
        self.assertIs(arb_pipeline.build_transport, transport)
        self.assertIs(arb_receipt.build_transport, transport)
        self.assertFalse(hasattr(arb_receipt, "build_input"))
        self.assertIs(type(bundle), build_input.SealedInputV1)
        for name in MOVED_INPUT_SURFACE_V1 + MOVED_TRANSPORT_SURFACE_V1:
            with self.subTest(consumer=arb_pipeline.__name__, name=name):
                self.assertFalse(hasattr(arb_pipeline, name))
        for name in MOVED_TRANSPORT_SURFACE_V1:
            with self.subTest(consumer=arb_receipt.__name__, name=name):
                self.assertFalse(hasattr(arb_receipt, name))
        for name in REMOVED_TRANSPORT_SURFACE_V1:
            with self.subTest(consumer=arb_pipeline.__name__, removed=name):
                self.assertFalse(hasattr(arb_pipeline, name))
            with self.subTest(consumer=arb_receipt.__name__, removed=name):
                self.assertFalse(hasattr(arb_receipt, name))

    def test_shared_input_and_policy_are_deeply_immutable_coordinates(self) -> None:
        build_input = importlib.import_module("build.input")
        transport = importlib.import_module("build.transport")
        sealed = build_input.seal_input_v1(
            hashlib.sha256(b"binding").digest(),
            b"exact bytes",
        )
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1

        for value in (sealed, policy):
            with self.subTest(value=type(value).__name__):
                self.assertFalse(hasattr(value, "__dict__"))
                with self.assertRaises((AttributeError, TypeError)):
                    value[0] = value[0]
                with self.assertRaises((AttributeError, TypeError)):
                    object.__setattr__(value, "foreign", object())
        self.assertIs(type(sealed), build_input.SealedInputV1)
        self.assertIs(type(policy), transport.DockerBuildPolicyV1)

    def test_forged_source_authorities_fail_in_the_arb_taxonomy(self) -> None:
        build_input = importlib.import_module("build.input")
        provenance = importlib.import_module("provenance")
        request = _request()
        lock = request.source_lock.sources[0]
        admitted = request.admitted_sources.sources[0]

        for hostile_lock, hostile_admitted in (
            (object.__new__(provenance.SourceReleaseLockV1), admitted),
            (lock, object.__new__(provenance.SafeSourceArchiveV1)),
        ):
            with self.subTest(authority=type(hostile_lock).__name__):
                with self.assertRaises(pipeline.PipelineInputErrorV1) as raised:
                    pipeline._normalized_source_entries_v1(
                        hostile_lock,
                        hostile_admitted,
                    )
                self.assertEqual(
                    raised.exception.reason,
                    pipeline.PipelineInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
                )


class SharedBuildTransportTargetTests(unittest.TestCase):
    def test_public_build_contract_violations_are_typed_before_backend(self) -> None:
        build_input = importlib.import_module("build.input")
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        cases = (
            ("wrong type", None, lambda _value: True),
            (
                "forged sealed input",
                tuple.__new__(build_input.SealedInputV1, ()),
                lambda _value: True,
            ),
            ("lane admission", _sealed_input(), lambda _value: False),
        )
        for name, hostile, admission in cases:
            with self.subTest(case=name):
                capability = _docker_capability_fixture(policy)
                backend = _ScriptedBuildBackend(capability, ())
                controller = transport.ControlledBuildTransportV1(
                    policy=policy,
                    backend=backend,
                )
                owned_capability = controller.probe()
                result = controller.build(
                    owned_capability,
                    hostile,
                    64,
                    input_admission=admission,
                    output_admission=lambda _value: True,
                )
                self.assertIs(type(result), transport.BuildRejectedV1)
                self.assertEqual(
                    result.reason,
                    transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
                )
                self.assertEqual(backend.requests, [])

    def test_capability_owns_injected_host_user_and_rejects_surrogate_coordinates(self) -> None:
        transport = importlib.import_module("build.transport")
        self.assertTrue(hasattr(transport, "DockerUserModeV1"))
        user_mode = transport.DockerUserModeV1.HOST_EFFECTIVE_IDS
        shipped = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        coordinates = {
            "image_reference": shipped.image_reference,
            "platform": shipped.platform,
            "hostname": shipped.hostname,
            "container_name_prefix": shipped.container_name_prefix,
            "bootstrap": shipped.bootstrap,
            "bootstrap_argv0": shipped.bootstrap_argv0,
            "tmpfs_specs": shipped.tmpfs_specs,
            "user_mode": user_mode,
            "stdout_limit": shipped.stdout_limit,
            "stderr_limit": shipped.stderr_limit,
            "build_timeout_ns": shipped.build_timeout_ns,
            "probe_output_limit": shipped.probe_output_limit,
            "probe_timeout_ns": shipped.probe_timeout_ns,
        }
        policy = transport.DockerBuildPolicyV1(**coordinates)
        input_value = _sealed_input()
        backends = tuple(
            transport.NativeDockerBuildBackendV1(
                Path("/usr/bin/true"),
                policy,
                host_user=(501, 20),
                platform_name="linux",
                machine_name="x86_64",
            )
            for _ in range(2)
        )
        capabilities = tuple(
            _probe_native_backend(backend, policy) for backend in backends
        )
        requests = tuple(
            transport.DockerBuildRequestV1(
                1,
                capability,
                input_value,
                64,
                Path("/tmp/lab-colors-red-user.cid"),
                policy.container_name_prefix + "red-user",
            )
            for capability in capabilities
        )
        with mock.patch.object(
            transport.os,
            "geteuid",
            side_effect=AssertionError("command_for performed ambient uid IO"),
        ), mock.patch.object(
            transport.os,
            "getegid",
            side_effect=AssertionError("command_for performed ambient gid IO"),
        ):
            commands = tuple(
                backend.command_for(request)
                for backend, request in zip(backends, requests, strict=True)
            )
        self.assertEqual(commands[0], commands[1])
        user_index = commands[0].index("--user")
        self.assertEqual(commands[0][user_index + 1], "501:20")

        for field_name, value in (
            ("bootstrap", "\ud800"),
            ("tmpfs_specs", ("/tmp/\ud800:rw",)),
        ):
            with self.subTest(field=field_name):
                hostile = dict(coordinates)
                hostile[field_name] = value
                with self.assertRaises(TypeError):
                    transport.DockerBuildPolicyV1(**hostile)
        with self.assertRaises(TypeError):
            transport.NativeDockerBuildBackendV1(
                Path("/tmp/\ud800"),
                policy,
                host_user=(501, 20),
                platform_name="linux",
                machine_name="x86_64",
            )

    def test_native_host_coordinates_are_exact_strings_and_oci_ports_are_ascii(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1

        class StringSubclass(str):
            pass

        for field_name, value in (
            ("platform_name", StringSubclass("linux")),
            ("machine_name", StringSubclass("x86_64")),
            ("platform_name", 7),
            ("machine_name", object()),
        ):
            with self.subTest(field=field_name, value_type=type(value).__name__):
                coordinates = {
                    "platform_name": "linux",
                    "machine_name": "x86_64",
                }
                coordinates[field_name] = value
                with self.assertRaises(TypeError):
                    transport.NativeDockerBuildBackendV1(
                        Path("/usr/bin/docker"),
                        policy,
                        host_user=(501, 20),
                        **coordinates,
                    )

        hostile_policy = {
            "image_reference": (
                "registry.example:\u0661/toolchain@sha256:" + "a" * 64
            ),
            "platform": policy.platform,
            "hostname": policy.hostname,
            "container_name_prefix": policy.container_name_prefix,
            "bootstrap": policy.bootstrap,
            "bootstrap_argv0": policy.bootstrap_argv0,
            "tmpfs_specs": policy.tmpfs_specs,
            "user_mode": policy.user_mode,
            "stdout_limit": policy.stdout_limit,
            "stderr_limit": policy.stderr_limit,
            "build_timeout_ns": policy.build_timeout_ns,
            "probe_output_limit": policy.probe_output_limit,
            "probe_timeout_ns": policy.probe_timeout_ns,
        }
        with self.assertRaises(TypeError):
            transport.DockerBuildPolicyV1(**hostile_policy)

    def test_stream_close_failure_fallback_closes_fd_and_retains_evidence(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        real_close = os.close
        spawned: list[subprocess.Popen[bytes]] = []
        fallback_closed: list[int] = []
        wrapped_streams: list[object] = []

        class CloseRaises:
            def __init__(self, wrapped: object) -> None:
                self.wrapped = wrapped
                self.descriptor = wrapped.fileno()
                self.close_calls = 0

            @property
            def closed(self) -> bool:
                return False

            def fileno(self) -> int:
                return self.descriptor

            def close(self) -> None:
                self.close_calls += 1
                raise OSError("forced close failure")

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            wrapped = CloseRaises(process.stdout)
            wrapped_streams.append(wrapped)
            process.stdout = wrapped
            return process

        def close(descriptor: int) -> None:
            fallback_closed.append(descriptor)
            real_close(descriptor)

        input_value = _sealed_input()
        command = (
            sys.executable,
            "-c",
            (
                "import sys; sys.stdin.buffer.read(); "
                "sys.stdout.buffer.write(b'evidence')"
            ),
        )
        try:
            with mock.patch.object(
                transport.subprocess,
                "Popen",
                side_effect=spawn,
            ), mock.patch.object(transport.os, "close", side_effect=close):
                result = backend._observe_command(
                    command,
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1_000_000_000,
                    cid_file=None,
                    input_bundle=input_value,
                )
        finally:
            for process in spawned:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                for stream in (process.stdin, process.stderr):
                    if stream is not None and not stream.closed:
                        stream.close()
                original = wrapped_streams[0].wrapped
                try:
                    original.close()
                except OSError:
                    pass

        self.assertIs(type(result), transport.DockerBuildObserverFailureV1)
        self.assertEqual(result.stdout, b"evidence")
        self.assertEqual(result.stderr, b"")
        self.assertIsNotNone(result.input_progress)
        self.assertEqual(result.input_progress.written_length, input_value.length)
        self.assertEqual(result.input_progress.written_sha256, input_value.sha256)
        self.assertIn(wrapped_streams[0].descriptor, fallback_closed)
        self.assertEqual(wrapped_streams[0].close_calls, 1)

    def test_post_popen_failures_always_close_streams_and_cleanup_once(self) -> None:
        transport = importlib.import_module("build.transport")
        real_popen = subprocess.Popen

        def exercise(
            *,
            selector_failure: bool,
        ) -> tuple[object, bool, int]:
            backend = transport.NativeDockerBuildBackendV1(
                Path("/bin/true"),
                pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
                host_user=(501, 20),
                platform_name="linux",
                machine_name="x86_64",
            )
            spawned: list[subprocess.Popen[bytes]] = []
            cleanup_calls: list[tuple[Path, str]] = []

            def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
                process = real_popen(*args, **kwargs)
                spawned.append(process)
                return process

            def cleanup(cid_file: Path, container_name: str) -> None:
                cleanup_calls.append((cid_file, container_name))
                return None

            def stop_raises(process: subprocess.Popen[bytes]) -> None:
                process.kill()
                process.wait(timeout=5)
                raise RuntimeError("forced stop failure")

            selector_patch = (
                mock.patch.object(
                    transport.selectors,
                    "DefaultSelector",
                    side_effect=RuntimeError("forced selector failure"),
                )
                if selector_failure
                else mock.patch.object(
                    backend,
                    "_stop_process",
                    side_effect=stop_raises,
                )
            )
            command = (
                sys.executable,
                "-c",
                "pass" if selector_failure else "import time; time.sleep(5)",
            )
            with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
                transport.subprocess,
                "Popen",
                side_effect=spawn,
            ), mock.patch.object(
                backend,
                "_cleanup_container",
                side_effect=cleanup,
            ), selector_patch:
                try:
                    result: object = backend._observe_command(
                        command,
                        stdout_limit=64,
                        stderr_limit=64,
                        timeout_ns=1 if not selector_failure else 1_000_000_000,
                        cid_file=Path(temporary).resolve() / "container.cid",
                        container_name=(
                            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1.container_name_prefix
                            + ("selector-red" if selector_failure else "stop-red")
                        ),
                    )
                except Exception as error:
                    result = error
                self.assertEqual(len(spawned), 1)
                process = spawned[0]
                streams_closed = bool(
                    process.stdout is not None
                    and process.stdout.closed
                    and process.stderr is not None
                    and process.stderr.closed
                )
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                for stream in (process.stdin, process.stdout, process.stderr):
                    if stream is not None and not stream.closed:
                        stream.close()
            return result, streams_closed, len(cleanup_calls)

        selector_result, selector_closed, selector_cleanups = exercise(
            selector_failure=True
        )
        self.assertIs(type(selector_result), transport.DockerBuildObserverFailureV1)
        self.assertTrue(selector_closed)
        self.assertEqual(selector_cleanups, 1)

        stop_result, stop_closed, stop_cleanups = exercise(selector_failure=False)
        self.assertIn(
            type(stop_result),
            (
                transport.DockerBuildObserverFailureV1,
                transport.DockerBuildCleanupFailureV1,
            ),
        )
        self.assertTrue(stop_closed)
        self.assertEqual(stop_cleanups, 1)

    def test_process_and_container_cleanup_failures_are_both_retained_in_order(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen

        def stop(process: subprocess.Popen[bytes]) -> str:
            process.kill()
            process.wait(timeout=5)
            return "process stop failed"

        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=real_popen,
        ), mock.patch.object(
            backend,
            "_stop_process",
            side_effect=stop,
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            return_value="container cleanup failed",
        ):
            result = backend._observe_command(
                (sys.executable, "-c", "import time; time.sleep(5)"),
                stdout_limit=64,
                stderr_limit=64,
                timeout_ns=1,
                cid_file=Path(temporary).resolve() / "container.cid",
                container_name=(
                    pipeline.ARB_BUILD_TRANSPORT_POLICY_V1.container_name_prefix
                    + "cleanup-records"
                ),
                input_bundle=_sealed_input(),
            )

        self.assertIs(type(result), transport.DockerBuildCleanupFailureV1)
        self.assertEqual(
            tuple(record.resource for record in result.failures),
            (
                transport.CleanupResourceV1.DOCKER_CLI_PROCESS,
                transport.CleanupResourceV1.DOCKER_CONTAINER,
            ),
        )
        self.assertEqual(
            tuple(record.detail for record in result.failures),
            ("process stop failed", "container cleanup failed"),
        )
        self.assertTrue(
            all(
                type(record) is transport.CleanupFailureRecordV1
                for record in result.failures
            )
        )

    def test_impossible_build_failures_without_input_progress_are_contract_violations(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        cleanup = transport.CleanupFailureRecordV1(
            transport.CleanupResourceV1.DOCKER_CONTAINER,
            "container cleanup failed",
        )
        impossible = (
            transport.DockerBuildTimedOutV1(b"", b""),
            transport.DockerBuildOutputLimitV1(
                transport.DockerOutputStreamV1.STDOUT,
                b"x" * 64,
                b"",
            ),
            transport.DockerBuildCleanupFailureV1(
                transport.DockerCleanupTriggerV1.TIMEOUT,
                (cleanup,),
                b"",
                b"",
            ),
        )
        for observation in impossible:
            with self.subTest(observation=type(observation).__name__):
                result, _backend, _report, _input = _controlled_build(
                    transport,
                    policy,
                    (observation,),
                )
                self.assertIs(type(result), transport.BuildRejectedV1)
                self.assertEqual(
                    result.reason,
                    transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
                )
                self.assertIsNone(result.process)

    def test_temporary_root_cleanup_failure_retains_current_process_generically(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        first = _completed_process(transport, input_value, b"first")
        current = _completed_process(transport, input_value, b"second")
        real_temporary_directory = tempfile.TemporaryDirectory
        allocated: list[object] = []
        allocations = 0

        class CleanupFailsOnce:
            def __init__(self, *args: object, **kwargs: object) -> None:
                self._inner = real_temporary_directory(*args, **kwargs)
                self.name = self._inner.name
                self._failed = False
                allocated.append(self)

            def __enter__(self) -> str:
                return self.name

            def __exit__(self, *_args: object) -> None:
                self.cleanup()

            def cleanup(self) -> None:
                if not self._failed:
                    self._failed = True
                    raise OSError("forced temporary-root cleanup failure")
                self._inner.cleanup()

        def temporary_directory(*args: object, **kwargs: object) -> object:
            nonlocal allocations
            allocations += 1
            if allocations == 1:
                return real_temporary_directory(*args, **kwargs)
            return CleanupFailsOnce(*args, **kwargs)

        try:
            with mock.patch.object(
                transport.tempfile,
                "TemporaryDirectory",
                side_effect=temporary_directory,
            ):
                result, _backend, _report, _input = _controlled_build(
                    transport,
                    policy,
                    (first, current),
                    input_value=input_value,
                )
        finally:
            for temporary in allocated:
                temporary.cleanup()

        self.assertIs(type(result), transport.BuildRejectedV1)
        self.assertEqual(
            result.reason,
            transport.BuildFailureReasonV1.CLEANUP_FAILED,
        )
        self.assertEqual(result.attempt, 2)
        self.assertEqual(result.completed_processes, (first,))
        self.assertIs(type(result.process), transport.BuildCleanupFailureV1)
        self.assertIs(result.process.current_process, current)
        self.assertEqual(len(result.process.failures), 1)
        self.assertEqual(
            result.process.failures[0].resource,
            transport.CleanupResourceV1.TEMPORARY_ROOT,
        )

    def test_temporary_root_cleanup_failure_is_typed_without_a_process(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        real_temporary_directory = tempfile.TemporaryDirectory
        inner = real_temporary_directory()

        class CleanupFailsOnce:
            name = inner.name

            def __init__(self) -> None:
                self.failed = False

            def cleanup(self) -> None:
                if not self.failed:
                    self.failed = True
                    raise OSError("forced temporary-root cleanup failure")
                inner.cleanup()

        temporary = CleanupFailsOnce()
        try:
            with mock.patch.object(
                transport.tempfile,
                "TemporaryDirectory",
                return_value=temporary,
            ):
                result, _backend, _capability, _input = _controlled_build(
                    transport,
                    policy,
                    (),
                )
        finally:
            temporary.cleanup()

        self.assertIs(type(result), transport.BuildRejectedV1)
        self.assertEqual(
            result.reason,
            transport.BuildFailureReasonV1.CLEANUP_FAILED,
        )
        self.assertIs(type(result.process), transport.BuildCleanupFailureV1)
        self.assertIsNone(result.process.current_process)
        self.assertEqual(
            tuple(record.resource for record in result.process.failures),
            (transport.CleanupResourceV1.TEMPORARY_ROOT,),
        )

    def test_forged_backend_failures_canonicalize_to_contract_violation(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        for failure_type in (
            transport.DockerBuildTimedOutV1,
            transport.DockerBuildOutputLimitV1,
            transport.DockerBuildObserverFailureV1,
            transport.DockerBuildInputRejectedV1,
            transport.DockerBuildCleanupFailureV1,
            transport.BuildCleanupFailureV1,
        ):
            with self.subTest(failure=failure_type.__name__):
                forged = _forged_exact_type(failure_type)
                if hasattr(forged, "__dict__"):
                    object.__setattr__(forged, "foreign", object())
                result, _backend, _report, _input = _controlled_build(
                    transport,
                    policy,
                    (forged,),
                )
                self.assertIs(type(result), transport.BuildRejectedV1)
                self.assertEqual(
                    result.reason,
                    transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
                )
                self.assertIsNone(result.process)
                self.assertEqual(result.completed_processes, ())

    def test_observer_failure_evidence_is_bounded_and_progress_is_exact(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        foreign_input = importlib.import_module("build.input").seal_input_v1(
            _digest("foreign-build-binding"),
            input_value.contents,
        )
        cases = (
            transport.DockerBuildObserverFailureV1(
                "oversized stdout",
                b"x" * 65,
                b"",
                _initial_progress(transport, input_value),
            ),
            transport.DockerBuildObserverFailureV1(
                "foreign progress",
                b"",
                b"",
                _initial_progress(transport, foreign_input),
            ),
        )
        for observation in cases:
            with self.subTest(detail=observation.detail):
                result, _backend, _report, _input = _controlled_build(
                    transport,
                    policy,
                    (observation,),
                    input_value=input_value,
                )
                self.assertIs(type(result), transport.BuildRejectedV1)
                self.assertEqual(
                    result.reason,
                    transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
                )
                self.assertIsNone(result.process)

    def test_top_level_failure_reason_preserves_observer_outcome(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        expected_reasons = (
            transport.BuildFailureReasonV1.TIMEOUT,
            transport.BuildFailureReasonV1.OUTPUT_LIMIT,
            transport.BuildFailureReasonV1.OBSERVER_FAILURE,
            transport.BuildFailureReasonV1.PROCESS_FAILED,
        )
        input_value = _sealed_input()
        progress = _initial_progress(transport, input_value)
        observations = (
            transport.DockerBuildTimedOutV1(b"", b"", progress),
            transport.DockerBuildOutputLimitV1(
                transport.DockerOutputStreamV1.STDOUT,
                b"x" * 64,
                b"",
                progress,
            ),
            transport.DockerBuildObserverFailureV1(
                "observer failed",
                b"observer stdout",
                b"observer stderr",
                progress,
            ),
            _completed_process(
                transport,
                input_value,
                b"",
                returncode=7,
            ),
        )
        for observation, reason in zip(
            observations,
            expected_reasons,
            strict=True,
        ):
            with self.subTest(reason=reason):
                result, _backend, _report, _input = _controlled_build(
                    transport,
                    policy,
                    (observation,),
                    input_value=input_value,
                )
                self.assertIs(type(result), transport.BuildRejectedV1)
                self.assertEqual(result.reason, reason)

    def test_second_attempt_failure_retains_first_completed_process(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        first = _completed_process(transport, input_value, b"first")
        progress = _initial_progress(transport, input_value)
        result, _backend, _report, _input = _controlled_build(
            transport,
            policy,
            (first, transport.DockerBuildTimedOutV1(b"", b"", progress)),
            input_value=input_value,
        )
        self.assertIs(type(result), transport.BuildRejectedV1)
        self.assertEqual(result.attempt, 2)
        self.assertTrue(hasattr(result, "completed_processes"))
        self.assertIs(type(result.completed_processes), tuple)
        self.assertEqual(result.completed_processes, (first,))
        self.assertIs(result.completed_processes[0], first)
        self.assertFalse(hasattr(result, "__dict__"))
        with self.assertRaises((AttributeError, TypeError)):
            object.__setattr__(result, "completed_processes", ())

    def test_two_build_observation_derives_byte_relation_and_binds_session(self) -> None:
        transport = importlib.import_module("build.transport")
        self.assertTrue(hasattr(transport, "BuildByteRelationV1"))
        self.assertTrue(hasattr(transport, "TwoBuildObservationV1"))
        self.assertFalse(hasattr(transport, "ReproducibleBuildV1"))
        self.assertFalse(hasattr(transport, "NonReproducibleBuildV1"))
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        for outputs, relation in (
            ((b"same", b"same"), transport.BuildByteRelationV1.IDENTICAL),
            ((b"first", b"second"), transport.BuildByteRelationV1.DIFFERENT),
        ):
            with self.subTest(relation=relation):
                input_value = _sealed_input()
                processes = tuple(
                    _completed_process(transport, input_value, output)
                    for output in outputs
                )
                result, _backend, capability, _input = _controlled_build(
                    transport,
                    policy,
                    processes,
                    max_output_bytes=64,
                    input_value=input_value,
                )
                self.assertIs(type(result), transport.TwoBuildObservationV1)
                self.assertEqual(result.relation, relation)
                self.assertEqual(result.policy, policy)
                self.assertEqual(result.capability, capability)
                self.assertIs(result.input_value, input_value)
                self.assertEqual(result.max_output_bytes, 64)
                self.assertEqual(result.processes, processes)
                self.assertEqual(
                    result.relation,
                    (
                        transport.BuildByteRelationV1.IDENTICAL
                        if processes[0].stdout == processes[1].stdout
                        else transport.BuildByteRelationV1.DIFFERENT
                    ),
                )

    def test_build_process_encoding_is_total_and_keeps_exact_golden(self) -> None:
        transport = importlib.import_module("build.transport")
        result, _backend = _execute()
        process = result.evidence.build.build_processes[0]
        encoded = transport.build_process_bytes_v1(process)
        self.assertEqual(len(encoded), 196)
        self.assertEqual(
            hashlib.sha256(encoded).hexdigest(),
            "401aaf23753b09b35482080e6046499e6a8a0a4ea2cea6c658ed377efebac58c",
        )

        forged = tuple.__new__(transport.DockerBuildExitedV1, ())
        with self.assertRaises(TypeError):
            transport.build_process_bytes_v1(forged)
        overflow = tuple.__new__(
            transport.DockerBuildExitedV1,
            (
                1 << 40,
                process.stdout,
                process.stderr,
                process.input_transfer,
            ),
        )
        with self.assertRaises(TypeError):
            transport.build_process_bytes_v1(overflow)


if __name__ == "__main__":
    unittest.main(verbosity=2)
