#!/usr/bin/env python3
"""RED contract for an identity-preserving engine-neutral BUILD leaf."""

from __future__ import annotations

import ast
import dis
import gc
import hashlib
import importlib
import os
import select
import subprocess
import sys
import threading
import unittest
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[1]
ARB = PROOF / "arb"
ARB_TESTS = ARB / "tests"
REPO = PROOF.parents[2]
sys.path[:0] = (str(REPO), str(PROOF), str(ARB), str(ARB_TESTS))

import pipeline  # noqa: E402
from proof.region.v1.arb.tests import gate as arb_gate  # noqa: E402
from test_pipeline import (  # noqa: E402
    _docker_capability,
    _probe_native_backend,
    _request,
)
from test_receipt import _execute  # noqa: E402


# These literals are an independent outer oracle for the Arb gate: importing
# its expected hash here would let a coordinated gate edit hide inventory drift.
# A deliberate test-set change updates both values from fresh enumeration.
ARB_INVENTORY_SHA256_V1 = (
    "cbacd035c919cb7a18a3f05d41319ae5bf6c93bbcca9612312357e9be23aedd5"
)
ARB_ORDER_SHA256_V1 = (
    "506d3d1f82102affc23e846b9500fbe95334134552800a141147a0595b476aea"
)
ARB_TEST_COUNT_V1 = 181

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

# Both shared leaves must remain unaware of engine semantics; separate names
# keep the two contracts legible without making their import policy diverge.
FORBIDDEN_TRANSPORT_IMPORTS_V1 = FORBIDDEN_INPUT_IMPORTS_V1


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


def _racing_build_transport(
    transport: object,
    *,
    policy: object,
    backend: object,
) -> object:
    """Force the former unlocked check→consume race without scheduler guesses."""

    class TrackingLock:
        def __init__(self) -> None:
            self._lock = threading.Lock()
            self._owner: int | None = None

        def __enter__(self) -> TrackingLock:
            self._lock.acquire()
            self._owner = threading.get_ident()
            return self

        def __exit__(
            self,
            _exception_type: object,
            _exception: object,
            _traceback: object,
        ) -> None:
            self._owner = None
            self._lock.release()

        def held_by_current_thread(self) -> bool:
            return self._owner == threading.get_ident()

    class RacingController(transport.ControlledBuildTransportV1):
        def __init__(self) -> None:
            self._consume_barrier = threading.Barrier(2)
            self._race_armed = False
            super().__init__(policy=policy, backend=backend)
            self._lease_lock = TrackingLock()

        def arm_consume_race(self) -> None:
            self._race_armed = True

        def __getattribute__(self, name: str) -> object:
            if (
                name == "_consumed"
                and object.__getattribute__(self, "_race_armed")
                and not object.__getattribute__(
                    self,
                    "_lease_lock",
                ).held_by_current_thread()
            ):
                object.__getattribute__(self, "_consume_barrier").wait(timeout=2)
            return super().__getattribute__(name)

    return RacingController()


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

        self.assertEqual(len(identifiers), ARB_TEST_COUNT_V1)
        self.assertEqual(len(set(identifiers)), ARB_TEST_COUNT_V1)
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
            "5ff9cac8af5fee7ffb05d18da33721842150dafe43edd6f0e356566c7be12144",
        )
        self.assertEqual(len(process_bytes), 196)
        self.assertEqual(
            hashlib.sha256(process_bytes).hexdigest(),
            "401aaf23753b09b35482080e6046499e6a8a0a4ea2cea6c658ed377efebac58c",
        )
        self.assertEqual(
            result.comparator.identity.hex(),
            "965004e9a45d4ff724f2ca39043086adf29bf860efc9b47367f67473ba6c52ac",
        )
        self.assertEqual(
            result.evidence.source_identity.hex(),
            "07d85ad695ec17104bdb34f6e9819d25be08afb3aa485918c44a363d7679f7c9",
        )
        self.assertEqual(
            result.evidence.build_identity.hex(),
            "5200b47ecae538174dea9f9c67e487859af70f59dd77eb46ca870d337b866bf9",
        )
        self.assertEqual(
            result.evidence.run_identity.hex(),
            "3036f9f4e49d0822d48447eaa08a0a2aaf052923e2f6cdb9362585dd044acc8e",
        )
        self.assertEqual(
            result.evidence.identity.hex(),
            "5a3041c6462401a919940d3a7ad1ed99039c7654d3d6b946901e44dd69c9dc53",
        )
        self.assertEqual(
            result.claim.identity.hex(),
            "71d1e5d6580404cd8ff4fef677d7664ba18e4fc99cbacb0a942756d56d59eb25",
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
        self.assertFalse(hasattr(transport, "input"))
        self.assertFalse(hasattr(transport, "build_input"))
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
        protocol_source = (PROOF / "PROTOCOL.md").read_text(encoding="utf-8")
        self.assertNotIn(
            "backend-contract rejection cannot retain authority",
            transport_source,
        )
        self.assertNotIn("invalid reproducible-build digests", pipeline_source)
        self.assertIn("fresh one-job VM workflow Arb", protocol_source)
        self.assertIn("same-UID writer", protocol_source)
        self.assertIn("Popen construction", protocol_source)
        self.assertNotIn("cleanup выполняет только по его точному имени", protocol_source)

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

    def test_arb_policy_reuses_generic_observer_ceiling_ssot(self) -> None:
        transport = importlib.import_module("build.transport")
        pipeline_source = (ARB / "pipeline.py").read_text(encoding="utf-8")
        for name in (
            "BUILD_STDOUT_LIMIT_V1",
            "BUILD_STDERR_LIMIT_V1",
            "BUILD_TIMEOUT_NS_V1",
            "DOCKER_PROBE_OUTPUT_LIMIT_V1",
            "DOCKER_PROBE_TIMEOUT_NS_V1",
        ):
            with self.subTest(name=name):
                self.assertIs(getattr(pipeline, name), getattr(transport, name))
                self.assertIn(f"build_transport.{name}", pipeline_source)

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
    def test_stream_close_fallback_requires_current_stream_ownership(self) -> None:
        transport = importlib.import_module("build.transport")
        real_close = os.close

        descriptor = os.open(os.devnull, os.O_RDONLY)

        class ClosesThenRaises:
            closed = False
            replacement: int | None = None
            close_calls = 0

            def close(self) -> None:
                self.close_calls += 1
                real_close(descriptor)
                self.closed = True
                self.replacement = os.open(os.devnull, os.O_RDONLY)
                raise OSError("stream close released its descriptor")

        stream = ClosesThenRaises()
        try:
            with mock.patch.object(
                transport.os,
                "close",
                side_effect=AssertionError("helper closed a numeric descriptor"),
            ) as direct_close:
                failed, interruption = (
                    transport.NativeDockerBuildBackendV1._close_owned_stream(
                        stream,
                    )
                )

            self.assertTrue(failed)
            self.assertIsNone(interruption)
            self.assertEqual(stream.replacement, descriptor)
            self.assertEqual(stream.close_calls, 1)
            direct_close.assert_not_called()
            os.fstat(descriptor)
        finally:
            if stream.replacement is not None:
                try:
                    real_close(stream.replacement)
                except OSError:
                    pass

        descriptor = os.open(os.devnull, os.O_RDONLY)

        class RaisesBeforeClose:
            closed = False
            close_calls = 0

            def close(self) -> None:
                self.close_calls += 1
                if self.close_calls == 1:
                    raise OSError("stream close kept its descriptor")
                real_close(descriptor)
                self.closed = True

        stream = RaisesBeforeClose()
        try:
            with mock.patch.object(
                transport.os,
                "close",
                side_effect=AssertionError("helper closed a numeric descriptor"),
            ) as direct_close:
                failed, interruption = (
                    transport.NativeDockerBuildBackendV1._close_owned_stream(
                        stream,
                    )
                )

            self.assertTrue(failed)
            self.assertIsNone(interruption)
            self.assertEqual(stream.close_calls, 2)
            direct_close.assert_not_called()
            with self.assertRaises(OSError):
                os.fstat(descriptor)
        finally:
            try:
                real_close(descriptor)
            except OSError:
                pass

    def test_session_property_is_pure_while_boundary_validator_rejects_forgery(self) -> None:
        transport = importlib.import_module("build.transport")
        build_input = importlib.import_module("build.input")
        forged_input = tuple.__new__(build_input.SealedInputV1, ())
        session = tuple.__new__(
            transport.BuildSessionV1,
            (
                _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1),
                forged_input,
                64,
            ),
        )

        self.assertIs(session.input_value, forged_input)
        self.assertFalse(transport._build_session_is_valid_v1(session))

    def test_overlapping_probe_is_rejected_without_a_second_backend_probe(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        capability = _docker_capability_fixture(policy)

        class BlockingBackend:
            def __init__(self) -> None:
                self.entered = threading.Event()
                self.release = threading.Event()
                self.calls = 0

            def probe(self) -> object:
                self.calls += 1
                self.entered.set()
                self.release.wait(timeout=2)
                return capability

            def run_build(self, _request: object) -> object:
                raise AssertionError("probe-only test reached build")

        backend = BlockingBackend()
        controller = transport.ControlledBuildTransportV1(
            policy=policy,
            backend=backend,
        )
        first_results: list[object] = []
        second_results: list[object] = []
        second_done = threading.Event()
        first = threading.Thread(target=lambda: first_results.append(controller.probe()))

        def second_probe() -> None:
            try:
                second_results.append(controller.probe())
            finally:
                second_done.set()

        second = threading.Thread(target=second_probe)
        first.start()
        self.assertTrue(backend.entered.wait(timeout=1))
        second.start()
        try:
            self.assertTrue(second_done.wait(timeout=1))
        finally:
            backend.release.set()
        first.join(timeout=2)
        second.join(timeout=2)

        self.assertFalse(first.is_alive())
        self.assertFalse(second.is_alive())
        self.assertEqual(backend.calls, 1)
        self.assertEqual(first_results, [capability])
        self.assertEqual(len(second_results), 1)
        self.assertIs(type(second_results[0]), transport.DockerUnsupportedV1)
        self.assertEqual(
            second_results[0].reason,
            transport.DockerBlockerReasonV1.BACKEND_CONTRACT,
        )

    def test_one_probe_lease_cannot_start_two_concurrent_two_build_sessions(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        capability = _docker_capability_fixture(policy)
        backend = _ScriptedBuildBackend(
            capability,
            tuple(
                _completed_process(transport, input_value, b"same executable")
                for _ in range(4)
            ),
        )
        controller = _racing_build_transport(
            transport,
            policy=policy,
            backend=backend,
        )
        owned_capability = controller.probe()
        controller.arm_consume_race()
        start = threading.Barrier(3)
        results: list[object] = []
        failures: list[BaseException] = []

        def build() -> None:
            try:
                start.wait(timeout=2)
                results.append(
                    controller.build(
                        owned_capability,
                        input_value,
                        64,
                        input_admission=lambda _value: True,
                        output_admission=lambda _value: True,
                    )
                )
            except BaseException as error:
                failures.append(error)

        workers = tuple(threading.Thread(target=build) for _ in range(2))
        for worker in workers:
            worker.start()
        start.wait(timeout=2)
        for worker in workers:
            worker.join(timeout=3)
            self.assertFalse(worker.is_alive())

        self.assertEqual(failures, [])
        self.assertEqual(len(results), 2)
        self.assertEqual(
            sum(type(result) is transport.TwoBuildObservationV1 for result in results),
            1,
        )
        rejections = tuple(
            result
            for result in results
            if type(result) is transport.BuildRejectedV1
        )
        self.assertEqual(len(rejections), 1)
        self.assertEqual(
            rejections[0].reason,
            transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
        )
        self.assertEqual(len(backend.requests), 2)

    def test_rejected_preflight_preserves_the_unconsumed_build_lease(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        capability = _docker_capability_fixture(policy)
        backend = _ScriptedBuildBackend(
            capability,
            (
                _completed_process(transport, input_value, b"same executable"),
                _completed_process(transport, input_value, b"same executable"),
            ),
        )
        controller = transport.ControlledBuildTransportV1(
            policy=policy,
            backend=backend,
        )
        owned_capability = controller.probe()

        rejected = controller.build(
            owned_capability,
            input_value,
            64,
            input_admission=lambda _value: False,
            output_admission=lambda _value: True,
        )
        self.assertIs(type(rejected), transport.BuildRejectedV1)
        self.assertEqual(
            rejected.reason,
            transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
        )
        self.assertEqual(backend.requests, [])

        admitted = controller.build(
            owned_capability,
            input_value,
            64,
            input_admission=lambda _value: True,
            output_admission=lambda _value: True,
        )
        self.assertIs(type(admitted), transport.TwoBuildObservationV1)
        self.assertEqual(len(backend.requests), 2)

    @unittest.skipUnless(hasattr(os, "fork"), "requires POSIX fork")
    def test_forked_child_cannot_wait_on_or_duplicate_a_build_lease(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        capability = _docker_capability_fixture(policy)
        backend = _ScriptedBuildBackend(
            capability,
            (
                _completed_process(transport, input_value, b"same executable"),
                _completed_process(transport, input_value, b"same executable"),
            ),
        )
        controller = transport.ControlledBuildTransportV1(
            policy=policy,
            backend=backend,
        )
        owned_capability = controller.probe()
        read_fd, write_fd = os.pipe()
        controller._lease_lock.acquire()
        child_pid: int | None = None
        child_reaped = False
        try:
            child_pid = os.fork()
            if child_pid == 0:
                os.close(read_fd)
                try:
                    child_result = controller.build(
                        owned_capability,
                        input_value,
                        64,
                        input_admission=lambda _value: True,
                        output_admission=lambda _value: True,
                    )
                    os.write(
                        write_fd,
                        (
                            f"{type(child_result).__name__}:"
                            f"{len(backend.requests)}"
                        ).encode("ascii"),
                    )
                finally:
                    os.close(write_fd)
                    os._exit(0)
            os.close(write_fd)
            ready, _write_ready, _errors = select.select([read_fd], [], [], 1)
            self.assertEqual(ready, [read_fd])
            child_message = os.read(read_fd, 128).decode("ascii")
            _waited_pid, status = os.waitpid(child_pid, 0)
            child_reaped = True
        finally:
            controller._lease_lock.release()
            if child_pid is not None and not child_reaped:
                try:
                    os.kill(child_pid, 9)
                except ProcessLookupError:
                    pass
                try:
                    os.waitpid(child_pid, 0)
                except ChildProcessError:
                    pass
            try:
                os.close(read_fd)
            except OSError:
                pass

        self.assertTrue(os.WIFEXITED(status))
        self.assertEqual(child_message, "BuildRejectedV1:0")
        parent_result = controller.build(
            owned_capability,
            input_value,
            64,
            input_admission=lambda _value: True,
            output_admission=lambda _value: True,
        )
        self.assertIs(type(parent_result), transport.TwoBuildObservationV1)
        self.assertEqual(len(backend.requests), 2)

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
            )
            for capability in capabilities
        )
        commands: list[tuple[str, ...]] = []

        def observe(
            command: tuple[str, ...],
            **_kwargs: object,
        ) -> object:
            commands.append(command)
            return _completed_process(transport, input_value, b"")

        with mock.patch.object(
            transport.os,
            "geteuid",
            side_effect=AssertionError("command_for performed ambient uid IO"),
        ), mock.patch.object(
            transport.os,
            "getegid",
            side_effect=AssertionError("command_for performed ambient gid IO"),
        ):
            for backend, request in zip(backends, requests, strict=True):
                with mock.patch.object(
                    backend,
                    "_observe_command",
                    side_effect=observe,
                ):
                    self.assertIs(
                        type(backend.run_build(request)),
                        transport.DockerBuildExitedV1,
                    )
        self.assertEqual(len(commands), 2)

        def without_native_cid_path(command: tuple[str, ...]) -> tuple[str, ...]:
            index = command.index("--cidfile")
            return command[: index + 1] + command[index + 2 :]

        self.assertEqual(
            without_native_cid_path(commands[0]),
            without_native_cid_path(commands[1]),
        )
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

    def test_native_backend_defers_host_user_observation_to_supported_probe(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1

        with mock.patch.object(
            transport.os,
            "geteuid",
            side_effect=AttributeError("not available on this host"),
        ), mock.patch.object(
            transport.os,
            "getegid",
            side_effect=AttributeError("not available on this host"),
        ):
            unsupported_backend = transport.NativeDockerBuildBackendV1(
                Path("/usr/bin/true"),
                policy,
                platform_name="windows",
                machine_name="amd64",
            )
            unsupported = unsupported_backend.probe()

        self.assertIs(type(unsupported), transport.DockerUnsupportedV1)
        self.assertEqual(
            unsupported.reason,
            transport.DockerBlockerReasonV1.HOST_NOT_LINUX_AMD64,
        )

        supported_backend = transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            policy,
            platform_name="linux",
            machine_name="x86_64",
        )
        with mock.patch.object(
            transport.os,
            "geteuid",
            side_effect=AttributeError("not available on this host"),
        ):
            unavailable = supported_backend.probe()

        self.assertIs(type(unavailable), transport.DockerUnsupportedV1)
        self.assertEqual(
            unavailable.reason,
            transport.DockerBlockerReasonV1.HOST_USER_UNAVAILABLE,
        )

    def test_native_probe_observes_unconfigured_host_user_each_time(self) -> None:
        """Ambient uid/gid belong to a capability observation, never backend cache."""

        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        backend = transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            policy,
            platform_name="linux",
            machine_name="x86_64",
        )

        with mock.patch.object(
            transport.os,
            "geteuid",
            side_effect=(501, 502),
        ), mock.patch.object(
            transport.os,
            "getegid",
            side_effect=(20, 21),
        ):
            first = _probe_native_backend(backend, policy)
            second = _probe_native_backend(backend, policy)

        self.assertEqual(first.host_user, (501, 20))
        self.assertEqual(second.host_user, (502, 21))

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

    def test_stream_close_failure_retries_owner_and_retains_evidence(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []
        wrapped_streams: list[object] = []

        class CloseRaises:
            def __init__(self, wrapped: object) -> None:
                self.wrapped = wrapped
                self.descriptor = wrapped.fileno()
                self.close_calls = 0

            @property
            def closed(self) -> bool:
                return self.wrapped.closed

            def fileno(self) -> int:
                return self.descriptor

            def close(self) -> None:
                self.close_calls += 1
                if self.close_calls == 1:
                    raise OSError("forced close failure")
                self.wrapped.close()

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            wrapped = CloseRaises(process.stdout)
            wrapped_streams.append(wrapped)
            process.stdout = wrapped
            return process

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
            ):
                result = backend._observe_command(
                    command,
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1_000_000_000,
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
                if not original.closed:
                    original.close()

        self.assertIs(type(result), transport.DockerBuildObserverFailureV1)
        self.assertEqual(result.stdout, b"evidence")
        self.assertEqual(result.stderr, b"")
        self.assertIsNotNone(result.input_progress)
        self.assertEqual(result.input_progress.written_length, input_value.length)
        self.assertEqual(result.input_progress.written_sha256, input_value.sha256)
        self.assertTrue(wrapped_streams[0].wrapped.closed)
        self.assertEqual(wrapped_streams[0].close_calls, 2)

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
            cleanup_calls: list[object] = []

            def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
                process = real_popen(*args, **kwargs)
                spawned.append(process)
                return process

            def cleanup(lease: object, **_kwargs: object) -> None:
                cleanup_calls.append(lease)
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
            lease = backend._next_run_lease_v1(
                _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
            )
            self.addCleanup(backend._release_run_lease_v1, lease)
            with mock.patch.object(
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
                        lease=lease,
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

    def test_base_exception_during_stop_still_reaps_streams_and_container(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []
        cleanup_calls: list[object] = []

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            return process

        def cleanup(lease: object, **_kwargs: object) -> None:
            cleanup_calls.append(lease)

        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_stop_process",
            side_effect=KeyboardInterrupt("interrupt during stop"),
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            side_effect=cleanup,
        ):
            with self.assertRaises(KeyboardInterrupt):
                backend._observe_command(
                    (sys.executable, "-c", "import time; time.sleep(5)"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1,
                    lease=lease,
                )
            process = spawned[0]
            running_before_test_cleanup = process.poll() is None
            streams_closed = bool(
                process.stdout is not None
                and process.stdout.closed
                and process.stderr is not None
                and process.stderr.closed
            )
            if running_before_test_cleanup:
                process.kill()
                process.wait(timeout=5)
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None and not stream.closed:
                    stream.close()

        self.assertFalse(running_before_test_cleanup)
        self.assertTrue(streams_closed)
        self.assertEqual(len(cleanup_calls), 1)

    def test_interrupt_after_spawn_still_reaps_and_attempts_cid_cleanup(self) -> None:
        """A post-spawn interruption cannot bypass the native finalizer."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            return process

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_mark_run_lease_launched_v1",
            side_effect=KeyboardInterrupt("interrupt after spawn"),
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            return_value=None,
        ) as cleanup:
            with self.assertRaisesRegex(KeyboardInterrupt, "after spawn"):
                backend._observe_command(
                    (sys.executable, "-c", "import time; time.sleep(5)"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1,
                    lease=lease,
                )

        process = spawned[0]
        self.assertIsNotNone(process.poll())
        cleanup.assert_called_once_with(lease, spawn_may_have_started=True)

    def test_interrupt_during_post_spawn_state_initialization_reaps_and_cleans(self) -> None:
        """No allocation between Popen and the finalizer may leak a child."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            return process

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            transport,
            "bytearray",
            side_effect=KeyboardInterrupt("interrupt during post-spawn allocation"),
            create=True,
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            return_value=None,
        ) as cleanup:
            with self.assertRaisesRegex(
                KeyboardInterrupt,
                "post-spawn allocation",
            ):
                backend._observe_command(
                    (sys.executable, "-c", "import time; time.sleep(5)"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1,
                    lease=lease,
                )

        process = spawned[0]
        try:
            self.assertIsNotNone(process.poll())
            cleanup.assert_called_once_with(lease, spawn_may_have_started=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None and not stream.closed:
                    stream.close()

    def test_post_popen_handler_gap_cannot_bypass_finalizer(self) -> None:
        """An interrupt at the first bytecode after Popen still owns its child."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)
        observe = backend._observe_command
        instructions = tuple(dis.Bytecode(observe))
        process_store = next(
            (
                index
                for index, instruction in enumerate(instructions)
                if (
                    instruction.opname == "STORE_FAST"
                    and instruction.argval == "process"
                    and index > 0
                    and instructions[index - 1].opname == "CALL_FUNCTION_EX"
                )
            ),
            None,
        )
        if process_store is None:
            self.fail(
                "CPython bytecode no longer exposes CALL_FUNCTION_EX before "
                f"STORE_FAST process (Python {sys.version})"
            )
        interruption_offset = instructions[process_store + 1].offset
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []
        injected = False

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            return process

        def tracer(frame: object, event: str, _arg: object) -> object:
            nonlocal injected
            if getattr(frame, "f_code", None) is observe.__code__:
                frame.f_trace_opcodes = True
                if (
                    not injected
                    and event == "opcode"
                    and frame.f_lasti == interruption_offset
                ):
                    injected = True
                    raise KeyboardInterrupt("interrupt in post-Popen handler gap")
            return tracer

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            return_value=None,
        ) as cleanup:
            previous = sys.gettrace()
            sys.settrace(tracer)
            try:
                with self.assertRaisesRegex(KeyboardInterrupt, "handler gap"):
                    observe(
                        (sys.executable, "-c", "import time; time.sleep(5)"),
                        stdout_limit=64,
                        stderr_limit=64,
                        timeout_ns=1,
                        lease=lease,
                    )
            finally:
                sys.settrace(previous)

        self.assertTrue(injected)
        process = spawned[0]
        try:
            self.assertIsNotNone(process.poll())
            cleanup.assert_called_once_with(lease, spawn_may_have_started=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None and not stream.closed:
                    stream.close()

    def test_popen_construction_interrupt_attempts_cid_cleanup_without_a_handle(self) -> None:
        """The pre-handle boundary retains the interruption and tries CID cleanup."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=KeyboardInterrupt("interrupt during Popen construction"),
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            return_value=None,
        ) as cleanup:
            with self.assertRaisesRegex(KeyboardInterrupt, "Popen construction"):
                backend._observe_command(
                    (sys.executable, "-c", "pass"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1,
                    lease=lease,
                )

        cleanup.assert_called_once_with(lease, spawn_may_have_started=True)

    @unittest.skipUnless(hasattr(os, "fork"), "requires POSIX fork")
    def test_forked_child_gc_cannot_delete_parent_cid_root(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        root = lease.cid_file.parent
        try:
            child = os.fork()
            if child == 0:
                del lease
                gc.collect()
                os._exit(0)
            _pid, status = os.waitpid(child, 0)
            self.assertEqual(os.waitstatus_to_exitcode(status), 0)
            self.assertTrue(root.is_dir())
        finally:
            backend._release_run_lease_v1(lease)

    def test_interrupted_cid_root_release_remains_retryable(self) -> None:
        """A failed root release must not permanently consume its cleanup lease."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        root = lease.cid_file.parent
        real_rmtree = transport.shutil.rmtree
        try:
            with mock.patch.object(
                transport.shutil,
                "rmtree",
                side_effect=KeyboardInterrupt("interrupt during CID-root release"),
            ):
                with self.assertRaisesRegex(KeyboardInterrupt, "CID-root release"):
                    backend._release_run_lease_v1(lease)

            self.assertTrue(root.is_dir())
            self.assertFalse(lease._released)
            self.assertIsNone(backend._release_run_lease_v1(lease))
            self.assertFalse(root.exists())
        finally:
            if root.exists():
                real_rmtree(root)

    def test_stop_interrupt_survives_container_cleanup_failure(self) -> None:
        """A later cleanup error cannot replace the caller's interruption."""

        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            return process

        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)

        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_stop_process",
            side_effect=KeyboardInterrupt("interrupt during stop"),
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            side_effect=OSError("cleanup failed after interruption"),
        ) as cleanup:
            with self.assertRaisesRegex(KeyboardInterrupt, "interrupt during stop"):
                backend._observe_command(
                    (sys.executable, "-c", "import time; time.sleep(5)"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1,
                    lease=lease,
                )
            process = spawned[0]
            running_before_test_cleanup = process.poll() is None
            streams_closed = bool(
                process.stdout is not None
                and process.stdout.closed
                and process.stderr is not None
                and process.stderr.closed
            )
            if running_before_test_cleanup:
                process.kill()
                process.wait(timeout=5)
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None and not stream.closed:
                    stream.close()

        self.assertFalse(running_before_test_cleanup)
        self.assertTrue(streams_closed)
        self.assertEqual(cleanup.call_count, 1)

    def test_stream_close_interrupt_still_closes_siblings_and_cleans_container(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []
        wrapped_stdout: list[object] = []
        cleanup_calls: list[object] = []

        class CloseInterrupts:
            def __init__(self, wrapped: object) -> None:
                self.wrapped = wrapped
                self.descriptor = wrapped.fileno()
                self.close_calls = 0

            @property
            def closed(self) -> bool:
                return self.wrapped.closed

            def fileno(self) -> int:
                return self.descriptor

            def close(self) -> None:
                self.close_calls += 1
                if self.close_calls == 1:
                    raise KeyboardInterrupt("interrupt during stdout close")
                self.wrapped.close()

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            spawned.append(process)
            wrapper = CloseInterrupts(process.stdout)
            wrapped_stdout.append(wrapper)
            process.stdout = wrapper
            return process

        def cleanup(lease: object, **_kwargs: object) -> None:
            cleanup_calls.append(lease)

        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)
        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            side_effect=cleanup,
        ):
            with self.assertRaisesRegex(KeyboardInterrupt, "stdout close"):
                backend._observe_command(
                    (sys.executable, "-c", "pass"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1_000_000_000,
                    lease=lease,
                )
            process = spawned[0]
            stderr_closed = process.stderr is not None and process.stderr.closed
            try:
                os.fstat(wrapped_stdout[0].descriptor)
            except OSError:
                stdout_descriptor_closed = True
            else:
                stdout_descriptor_closed = False
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            for stream in (process.stdin, process.stderr):
                if stream is not None and not stream.closed:
                    stream.close()
            try:
                wrapped_stdout[0].wrapped.close()
            except OSError:
                pass

        self.assertTrue(stderr_closed)
        self.assertTrue(stdout_descriptor_closed)
        self.assertEqual(wrapped_stdout[0].close_calls, 2)
        self.assertEqual(cleanup_calls, [lease])

    def test_persistent_stream_close_interrupt_keeps_release_failure_honest(self) -> None:
        transport = importlib.import_module("build.transport")
        backend = transport.NativeDockerBuildBackendV1(
            Path("/bin/true"),
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
            host_user=(501, 20),
            platform_name="linux",
            machine_name="x86_64",
        )
        real_popen = subprocess.Popen
        spawned: list[subprocess.Popen[bytes]] = []
        wrapped_stdout: list[object] = []
        cleanup_calls: list[object] = []

        class CloseAlwaysInterrupts:
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
                raise KeyboardInterrupt("persistent stdout close interrupt")

        def cleanup_spawned(
            process: subprocess.Popen[bytes],
            original_stdout: object,
        ) -> None:
            if process.poll() is None:
                try:
                    process.kill()
                except ProcessLookupError:
                    pass
            process.wait(timeout=5)
            for stream in (process.stdin, process.stderr, original_stdout):
                if stream is not None and not stream.closed:
                    stream.close()

        def spawn(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
            process = real_popen(*args, **kwargs)
            # Fixture setup may fail after Popen; ownership starts with its
            # original stream, before the hostile wrapper exists.
            self.addCleanup(cleanup_spawned, process, process.stdout)
            spawned.append(process)
            wrapper = CloseAlwaysInterrupts(process.stdout)
            wrapped_stdout.append(wrapper)
            process.stdout = wrapper
            return process

        def cleanup(lease: object, **_kwargs: object) -> None:
            cleanup_calls.append(lease)

        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)
        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=spawn,
        ), mock.patch.object(
            backend,
            "_cleanup_container",
            side_effect=cleanup,
        ):
            with self.assertRaisesRegex(KeyboardInterrupt, "persistent stdout"):
                backend._observe_command(
                    (sys.executable, "-c", "pass"),
                    stdout_limit=64,
                    stderr_limit=64,
                    timeout_ns=1_000_000_000,
                    lease=lease,
                )
            process = spawned[0]
            stderr_closed = process.stderr is not None and process.stderr.closed
            os.fstat(wrapped_stdout[0].descriptor)

        self.assertTrue(stderr_closed)
        self.assertEqual(wrapped_stdout[0].close_calls, 2)
        self.assertEqual(cleanup_calls, [lease])

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

        lease = backend._next_run_lease_v1(
            _docker_capability_fixture(pipeline.ARB_BUILD_TRANSPORT_POLICY_V1)
        )
        self.addCleanup(backend._release_run_lease_v1, lease)

        with mock.patch.object(
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
                lease=lease,
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

    def test_controller_passes_only_semantic_build_request_to_its_backend(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        observations = (
            _completed_process(transport, input_value, b"first"),
            _completed_process(transport, input_value, b"second"),
        )
        result, backend, _capability, _input = _controlled_build(
            transport,
            policy,
            observations,
            input_value=input_value,
        )

        self.assertIs(type(result), transport.TwoBuildObservationV1)
        self.assertEqual(len(backend.requests), 2)
        for request in backend.requests:
            self.assertEqual(len(tuple(request)), 4)
            self.assertFalse(hasattr(request, "cid_file"))
            self.assertFalse(hasattr(request, "container_name"))

    def test_backend_interrupt_propagates_without_controller_cleanup_authority(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        input_value = _sealed_input()
        capability = _docker_capability_fixture(policy)

        class InterruptingBackend:
            def __init__(self) -> None:
                self.requests: list[object] = []

            def probe(self) -> object:
                return capability

            def run_build(self, request: object) -> object:
                self.requests.append(request)
                raise KeyboardInterrupt("interrupt during build observation")

        backend = InterruptingBackend()
        controller = transport.ControlledBuildTransportV1(
            policy=policy,
            backend=backend,
        )
        owned_capability = controller.probe()
        with self.assertRaisesRegex(KeyboardInterrupt, "interrupt during build observation"):
            controller.build(
                owned_capability,
                input_value,
                64,
                input_admission=lambda _value: True,
                output_admission=lambda _value: True,
            )

        self.assertEqual(len(backend.requests), 1)
        self.assertEqual(len(tuple(backend.requests[0])), 4)

    def test_forged_backend_failures_canonicalize_to_contract_violation(self) -> None:
        transport = importlib.import_module("build.transport")
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        for failure_type in (
            transport.DockerBuildTimedOutV1,
            transport.DockerBuildOutputLimitV1,
            transport.DockerBuildObserverFailureV1,
            transport.DockerBuildInputRejectedV1,
            transport.DockerBuildCleanupFailureV1,
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
