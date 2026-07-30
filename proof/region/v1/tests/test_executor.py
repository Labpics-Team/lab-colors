#!/usr/bin/env python3
"""Hostile tests for the shared Linux-only proof process boundary."""

from __future__ import annotations

import errno
import fcntl
import hashlib
import os
import signal
import struct
import sys
import tempfile
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import executor  # noqa: E402


def _static_elf(*, interpreter: bool = False, needed: bool = False) -> bytes:
    """Return a parseable ELF64/x86-64 shape; it is not intended to run."""

    program_headers: list[bytes] = []
    body = bytearray(64)
    program_headers.append(
        struct.pack("<IIQQQQQQ", 1, 5, 0, 0, 0, 0, 0, 0x1000)
    )
    if interpreter:
        program_headers.append(
            struct.pack("<IIQQQQQQ", 3, 4, 0, 0, 0, 0, 0, 1)
        )
    if needed:
        dynamic_offset = 64 + 56 * (len(program_headers) + 1)
        dynamic = struct.pack("<QQQQ", 1, 1, 0, 0)
        program_headers.append(
            struct.pack(
                "<IIQQQQQQ",
                2,
                4,
                dynamic_offset,
                0,
                0,
                len(dynamic),
                len(dynamic),
                8,
            )
        )
    else:
        dynamic = b""

    phnum = len(program_headers)
    body[0:16] = b"\x7fELF\x02\x01\x01" + bytes(9)
    body[16:64] = struct.pack(
        "<HHIQQQIHHHHHH",
        2,
        62,
        1,
        0,
        64,
        0,
        0,
        64,
        56,
        phnum,
        0,
        0,
        0,
    )
    return bytes(body) + b"".join(program_headers) + dynamic


def _program_headers(elf: bytes) -> tuple[tuple[int, ...], ...]:
    count = struct.unpack_from("<H", elf, 56)[0]
    return tuple(
        struct.unpack_from("<IIQQQQQQ", elf, 64 + 56 * index)
        for index in range(count)
    )


def _linux_executable_elf(code: bytes) -> bytes:
    """Create a literal static ELF64 fixture with one RX load segment."""

    code_offset = 64 + 56
    file_size = code_offset + len(code)
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
    return header + program + code


_LINUX_EXIT_ZERO = bytes.fromhex("bf00000000b83c0000000f05")
_LINUX_BUSY_LOOP = bytes.fromhex("ebfe")
_LINUX_SIGILL = bytes.fromhex("0f0b")
_LINUX_FOREIGN_PRLIMIT = bytes.fromhex(
    "48c7c02e010000"  # mov rax, 302 (prlimit64)
    "48c7c701000000"  # mov rdi, 1 (foreign PID)
    "4831f6"          # xor rsi, rsi
    "4831d2"          # xor rdx, rdx
    "4d31d2"          # xor r10, r10
    "0f05"            # syscall
    "bf4d000000"      # mov edi, 77 (must be unreachable)
    "b83c000000"      # mov eax, 60 (exit)
    "0f05"            # syscall
)
_LINUX_ECHO_FOUR = bytes.fromhex(
    "4883ec0831c031ff4889e6ba040000000f05"
    "b801000000bf010000004889e6ba040000000f05"
    "31ffb83c0000000f05"
)
_LINUX_WRITE_FIVE_AND_LOOP = (
    bytes.fromhex("b801000000bf01000000488d3509000000ba050000000f05ebfe")
    + b"12345"
)
_LINUX_ALLOCATE_UNTIL_OOM = bytes.fromhex(
    "31ffbe00001000ba0300000041ba2200000049c7c0ffffffff4531c9"
    "b8090000000f054885c07819b900001000c60001480500100000"
    "4881e90010000075eeebbf"
    "bf49000000b83c0000000f05"
)


def _limits(**changes: int) -> executor.ExecutionLimitsV1:
    values = {
        "max_executable_bytes": 4096,
        "max_stdin_bytes": 4096,
        "max_argument_bytes": 4096,
        "max_stdout_bytes": 16,
        "max_stderr_bytes": 16,
        "wall_timeout_ns": 1_000_000_000,
        "memory_max_bytes": 64 * 1024 * 1024,
        "pids_max": 1,
    }
    values.update(changes)
    return executor.ExecutionLimitsV1(**values)


def _request(**changes: object) -> executor.ExecutionRequestV1:
    values: dict[str, object] = {
        "executable": _static_elf(),
        "argv": (
            b"proof-evaluator",
            b"--manifest-identity",
            b"1" * 64,
            b"--job",
            b"/dev/stdin",
        ),
        "environment": ((b"LC_ALL", b"C"), (b"TZ", b"UTC")),
        "cwd": b"/work",
        "stdin": b"LCJOB1\0\0",
        "umask": 0o077,
        "limits": _limits(),
    }
    values.update(changes)
    return executor.ExecutionRequestV1(**values)


class _Backend:
    def __init__(
        self,
        probe_result: executor.CapabilityReportV1,
        run_result: executor.ExecutionResultV1 | None = None,
    ) -> None:
        self.probe_result = probe_result
        self.run_result = run_result
        self.probe_calls = 0
        self.received: list[
            tuple[executor.ExecutionRequestV1, executor.SupportedV1]
        ] = []

    def probe(self, guard: object) -> executor.CapabilityReportV1:
        self.probe_calls += 1
        if not guard.is_current():  # type: ignore[attr-defined]
            raise AssertionError("controller supplied a stale probe guard")
        return self.probe_result

    def run(
        self,
        request: executor.ExecutionRequestV1,
        capability: executor.SupportedV1,
    ) -> executor.ExecutionResultV1:
        self.received.append((request, capability))
        if self.run_result is None:
            raise AssertionError("unsupported backend must not be run")
        return self.run_result


class _MemfdOperations:
    def __init__(self) -> None:
        self.fd = 41
        self.bytes_by_fd: dict[int, bytes] = {}
        self.seals_by_fd: dict[int, int] = {}
        self.exec_calls: list[tuple[int, tuple[bytes, ...], tuple[tuple[bytes, bytes], ...]]] = []
        self.events: list[str] = []

    def create_executable_memfd(self) -> int:
        self.events.append("create")
        return self.fd

    def pipe_cloexec(self) -> tuple[int, int]:
        read_fd, write_fd = os.pipe()
        for descriptor in (read_fd, write_fd):
            flags = fcntl.fcntl(descriptor, fcntl.F_GETFD)
            fcntl.fcntl(descriptor, fcntl.F_SETFD, flags | fcntl.FD_CLOEXEC)
        return read_fd, write_fd

    def write_all(self, fd: int, data: bytes) -> None:
        self.events.append("write")
        self.bytes_by_fd[fd] = data

    def make_executable(self, fd: int) -> None:
        self.events.append("chmod")
        self.assert_known(fd)

    def add_seals(self, fd: int, seals: int) -> None:
        self.events.append("seal")
        self.assert_known(fd)
        self.seals_by_fd[fd] = seals

    def get_seals(self, fd: int) -> int:
        self.events.append("get_seals")
        self.assert_known(fd)
        return self.seals_by_fd[fd]

    def pread(self, fd: int, size: int, offset: int) -> bytes:
        self.events.append("pread")
        self.assert_known(fd)
        return self.bytes_by_fd[fd][offset : offset + size]

    def execveat(
        self,
        fd: int,
        argv: tuple[bytes, ...],
        environment: tuple[tuple[bytes, bytes], ...],
    ) -> None:
        self.events.append("execveat")
        self.assert_known(fd)
        self.exec_calls.append((fd, argv, environment))

    def close(self, fd: int) -> None:
        self.assert_known(fd)

    def assert_known(self, fd: int) -> None:
        if fd != self.fd:
            raise AssertionError(f"unexpected file descriptor: {fd}")


class _ProbeOperations(_MemfdOperations):
    def __init__(self) -> None:
        super().__init__()
        self.probes: list[str] = []

    def probe_execveat(self) -> None:
        self.probes.append("execveat")

    def probe_standard_fds(self) -> None:
        self.probes.append("standard_fds")

    def probe_single_threaded(self) -> None:
        self.probes.append("single_threaded")

    def probe_close_range(self) -> None:
        self.probes.append("close_range")

    def probe_namespaces(self) -> None:
        self.probes.append("namespaces")

    def probe_seccomp(self) -> None:
        self.probes.append("seccomp")


class _LateThreadOperations(_ProbeOperations):
    def probe_single_threaded(self) -> None:
        self.probes.append("single_threaded")
        raise OSError(errno.EBUSY, "late thread")


class _OverlapAfterSingleThreadOperations(_ProbeOperations):
    def __init__(self) -> None:
        super().__init__()
        self.single_thread_passed = threading.Event()
        self.release_outer_probe = threading.Event()
        self.blocked_once = False

    def probe_single_threaded(self) -> None:
        super().probe_single_threaded()
        if not self.blocked_once:
            self.blocked_once = True
            self.single_thread_passed.set()
            if not self.release_outer_probe.wait(timeout=1):
                raise AssertionError("overlap test did not release the outer probe")


class _SecondSingleThreadFailureOperations(_ProbeOperations):
    def __init__(self) -> None:
        super().__init__()
        self.single_thread_probes = 0

    def probe_single_threaded(self) -> None:
        super().probe_single_threaded()
        self.single_thread_probes += 1
        if self.single_thread_probes == 2:
            raise OSError(errno.EBUSY, "thread appeared before fork")


class _CgroupFactory:
    def __init__(self) -> None:
        self.observer_budgets: list[Path] = []
        self.probed: list[Path] = []

    def probe_observer_task_budget(self, parent: Path) -> None:
        self.observer_budgets.append(parent)

    def probe(self, parent: Path) -> None:
        self.probed.append(parent)


class _ObserverCgroup:
    def __init__(self, pid: int) -> None:
        self.pid = pid

    def kill_all(self) -> None:
        try:
            os.kill(self.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    def oom_kill_count(self) -> int:
        return 0

    def populated(self) -> bool:
        return False


class _ReadbackCgroup(executor._CgroupV2V1):
    def __init__(self, values: dict[bytes, bytes]) -> None:
        self.values = values

    def _read_required(self, name: bytes) -> bytes:
        return self.values[name]


class SharedExecutorBoundaryTests(unittest.TestCase):
    def test_executor_is_one_shared_leaf_outside_engine_packages(self) -> None:
        shared = PROOF / "executor.py"

        self.assertTrue(shared.is_file())
        self.assertFalse((PROOF / "arb/executor.py").exists())
        self.assertEqual(Path(executor.__file__).resolve(), shared.resolve())
        source = shared.read_text(encoding="utf-8")
        self.assertNotIn("Arb", source)
        self.assertNotIn("labcolors-arb", source.lower())
        self.assertNotIn("mpfi", source.lower())
        for engine_import in (
            "import arb",
            "from arb",
            ".arb",
            "import mpfi",
            "from mpfi",
            ".mpfi",
        ):
            with self.subTest(engine_import=engine_import):
                self.assertNotIn(engine_import, source.lower())

    def test_execution_identities_match_independent_literal_goldens(self) -> None:
        request = _request()
        platform_value = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )

        self.assertEqual(
            executor.invocation_identity_v1(request).hex(),
            "4c5bf676852b086fe7909a572bdbcc497ea52cec8d5affbe162fcd776605c384",
        )
        self.assertEqual(
            executor.platform_identity_v1(platform_value).hex(),
            "0e37fa87cadce6814466528b2ea419e964554339bcbcb8d27c2da66c95dabc51",
        )

        object.__setattr__(platform_value, "sandbox_policy_release", "foreign")
        with self.assertRaises(TypeError):
            executor.platform_identity_v1(platform_value)

    def test_invocation_identity_binds_every_representable_coordinate(self) -> None:
        request = _request()
        baseline = executor.invocation_identity_v1(request)
        limit_mutations = (
            {"max_executable_bytes": request.limits.max_executable_bytes + 1},
            {"max_stdin_bytes": request.limits.max_stdin_bytes + 1},
            {"max_argument_bytes": request.limits.max_argument_bytes + 1},
            {"max_stdout_bytes": request.limits.max_stdout_bytes + 1},
            {"max_stderr_bytes": request.limits.max_stderr_bytes + 1},
            {"wall_timeout_ns": request.limits.wall_timeout_ns + 1},
            {"memory_max_bytes": request.limits.memory_max_bytes + 1},
        )
        mutants = (
            _request(executable=request.executable + b"x"),
            _request(argv=request.argv + (b"--strict",)),
            _request(
                environment=((b"LC_ALL", b"POSIX"), (b"TZ", b"UTC")),
            ),
            _request(cwd=b"/"),
            _request(stdin=request.stdin + b"x"),
            _request(umask=0o022),
            *(
                _request(limits=replace(request.limits, **changes))
                for changes in limit_mutations
            ),
        )

        self.assertEqual(len(mutants), 13)
        identities = {executor.invocation_identity_v1(item) for item in mutants}
        self.assertEqual(len(identities), 13)
        self.assertTrue(
            all(executor.invocation_identity_v1(item) != baseline for item in mutants)
        )

    def test_invalidated_request_cannot_be_reidentified(self) -> None:
        request = _request()
        object.__setattr__(request.limits, "pids_max", 2)

        with self.assertRaises(executor.ExecutionRequestErrorV1):
            executor.invocation_identity_v1(request)


class RequestAdmissionTests(unittest.TestCase):
    def test_combined_dynamic_fixture_points_after_its_full_header_table(self) -> None:
        elf = _static_elf(interpreter=True, needed=True)
        headers = _program_headers(elf)
        dynamic = next(header for header in headers if header[0] == 2)

        self.assertEqual(dynamic[2], 64 + 56 * len(headers))
        self.assertEqual(
            elf[dynamic[2] : dynamic[2] + dynamic[5]],
            struct.pack("<QQQQ", 1, 1, 0, 0),
        )

    def assert_rejected(
        self,
        reason: executor.RequestReasonV1,
        **changes: object,
    ) -> None:
        with self.assertRaises(executor.ExecutionRequestErrorV1) as caught:
            _request(**changes)
        self.assertEqual(caught.exception.reason, reason)

    def test_request_preserves_exact_invocation_without_mapping_or_inheritance(self) -> None:
        request = _request()

        self.assertEqual(
            request.argv,
            (
                b"proof-evaluator",
                b"--manifest-identity",
                b"1" * 64,
                b"--job",
                b"/dev/stdin",
            ),
        )
        self.assertEqual(request.environment, ((b"LC_ALL", b"C"), (b"TZ", b"UTC")))
        self.assertEqual(request.cwd, b"/work")
        self.assertEqual(request.stdin, b"LCJOB1\0\0")
        self.assertNotIn("network_isolated", request.__dataclass_fields__)
        self.assertNotIn("cgroup_isolated", request.__dataclass_fields__)

    def test_argv_environment_cwd_and_stdin_are_strict_bytes(self) -> None:
        cases = (
            ({"argv": [b"proof-evaluator"]}, executor.RequestReasonV1.WRONG_TYPE),
            ({"argv": (b"",)}, executor.RequestReasonV1.EMPTY_ARGV_ZERO),
            ({"argv": (b"arb\0evil",)}, executor.RequestReasonV1.NUL_BYTE),
            ({"environment": {b"LC_ALL": b"C"}}, executor.RequestReasonV1.WRONG_TYPE),
            (
                {"environment": ((b"TZ", b"UTC"), (b"LC_ALL", b"C"))},
                executor.RequestReasonV1.NONCANONICAL_ENVIRONMENT,
            ),
            (
                {"environment": ((b"LC_ALL", b"C"), (b"LC_ALL", b"POSIX"))},
                executor.RequestReasonV1.DUPLICATE_ENVIRONMENT,
            ),
            ({"environment": ((b"A=B", b"C"),)}, executor.RequestReasonV1.INVALID_ENVIRONMENT_KEY),
            ({"cwd": b"relative"}, executor.RequestReasonV1.RELATIVE_CWD),
            ({"cwd": b"/work/../tmp"}, executor.RequestReasonV1.NONCANONICAL_CWD),
            ({"stdin": "not-bytes"}, executor.RequestReasonV1.WRONG_TYPE),
        )
        for changes, reason in cases:
            with self.subTest(changes=changes):
                self.assert_rejected(reason, **changes)

    def test_explicit_limits_reject_oversized_inputs_and_bool_numbers(self) -> None:
        self.assert_rejected(
            executor.RequestReasonV1.LIMIT_EXCEEDED,
            stdin=b"12345",
            limits=_limits(max_stdin_bytes=4),
        )
        self.assert_rejected(
            executor.RequestReasonV1.LIMIT_EXCEEDED,
            executable=_static_elf() + b"x" * 4096,
        )
        with self.assertRaises(executor.ExecutionRequestErrorV1) as caught:
            replace(_limits(), pids_max=True)  # type: ignore[arg-type]
        self.assertEqual(caught.exception.reason, executor.RequestReasonV1.INVALID_LIMIT)
        with self.assertRaises(executor.ExecutionRequestErrorV1) as caught:
            replace(_limits(), pids_max=2)
        self.assertEqual(caught.exception.reason, executor.RequestReasonV1.INVALID_LIMIT)

    def test_only_static_x86_64_elf_is_admitted(self) -> None:
        self.assert_rejected(executor.RequestReasonV1.INVALID_ELF, executable=b"#!/bin/sh\n")
        self.assert_rejected(
            executor.RequestReasonV1.DYNAMIC_EXECUTABLE,
            executable=_static_elf(interpreter=True),
        )
        self.assert_rejected(
            executor.RequestReasonV1.DYNAMIC_EXECUTABLE,
            executable=_static_elf(needed=True),
        )

    def test_cross_module_verifiers_are_explicit_versioned_api(self) -> None:
        self.assertTrue(callable(executor.require_static_x86_64_elf_v1))
        self.assertTrue(callable(executor.result_matches_request_v1))
        self.assertFalse(hasattr(executor, "_require_static_x86_64_elf"))
        self.assertFalse(hasattr(executor, "_result_matches_request"))


class CapabilityAndExecutionTests(unittest.TestCase):
    def test_non_linux_host_fails_closed_before_any_run(self) -> None:
        native = executor.NativeLinuxBackendV1(
            cgroup_parent=None,
            platform_name="darwin",
            machine_name="arm64",
        )
        controller = executor.ControlledExecutorV1(native)
        report = controller.probe()

        self.assertIs(type(report), executor.UnsupportedV1)
        self.assertEqual(
            report.failures,
            (
                executor.CapabilityFailureV1(
                    executor.CapabilityReasonV1.HOST_NOT_LINUX,
                    None,
                ),
            ),
        )
        result = controller.execute(_request())
        self.assertEqual(result, report)

    def test_linux_without_an_explicit_delegated_cgroup_is_unsupported(self) -> None:
        native = executor.NativeLinuxBackendV1(
            cgroup_parent=None,
            platform_name="linux",
            machine_name="x86_64",
        )
        report = executor.ControlledExecutorV1(native).probe()

        self.assertIs(type(report), executor.UnsupportedV1)
        self.assertIn(
            executor.CapabilityFailureV1(
                executor.CapabilityReasonV1.CGROUP_PARENT_NOT_DECLARED,
                None,
            ),
            report.failures,
        )

    def test_supported_probe_executes_every_required_mechanism(self) -> None:
        operations = _ProbeOperations()
        cgroups = _CgroupFactory()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=cgroups,
        )

        report = executor.ControlledExecutorV1(native).probe()

        self.assertEqual(
            report,
            executor.SupportedV1(
                "linux-x86_64",
                executor.SANDBOX_POLICY_RELEASE_V1,
            ),
        )
        self.assertEqual(
            operations.probes,
            [
                "standard_fds",
                "single_threaded",
                "execveat",
                "close_range",
                "single_threaded",
                "namespaces",
                "single_threaded",
                "seccomp",
            ],
        )
        self.assertEqual(
            cgroups.observer_budgets,
            [Path("/delegated-proof-cgroup")],
        )
        self.assertEqual(cgroups.probed, [Path("/delegated-proof-cgroup")])

    def test_final_probe_cannot_outlive_its_controller_lease(self) -> None:
        current = True

        class InvalidatingCgroupFactory(_CgroupFactory):
            def probe(self, parent: Path) -> None:
                nonlocal current
                super().probe(parent)
                current = False

        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=_ProbeOperations(),
            cgroup_factory=InvalidatingCgroupFactory(),
        )

        report = native._probe_capability_v1(
            executor._ProbeGuardV1(lambda: current)
        )

        self.assertEqual(report, executor._invalidated_capability_report_v1())

    def test_overlap_after_single_thread_gate_cancels_before_fork_and_revokes_authority(self) -> None:
        operations = _OverlapAfterSingleThreadOperations()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=_CgroupFactory(),
        )
        controller = executor.ControlledExecutorV1(native)
        reports: list[executor.CapabilityReportV1] = []
        worker = threading.Thread(
            target=lambda: reports.append(controller.probe()),
            daemon=True,
        )

        worker.start()
        self.assertTrue(
            operations.single_thread_passed.wait(timeout=1),
            "outer probe did not reach the single-thread gate",
        )
        overlap = controller.probe()
        operations.release_outer_probe.set()
        worker.join(timeout=1)

        self.assertFalse(worker.is_alive(), "overlapping probe deadlocked")
        self.assertEqual(len(reports), 1)
        invalidated = executor.UnsupportedV1(
            (
                executor.CapabilityFailureV1(
                    executor.CapabilityReasonV1.OBSERVATION_INVALIDATED,
                    errno.EBUSY,
                ),
            )
        )
        self.assertEqual(overlap, invalidated)
        self.assertEqual(reports[0], invalidated)
        self.assertEqual(operations.probes, ["standard_fds", "single_threaded"])
        self.assertIsNone(controller._issued_capability)
        self.assertEqual(
            controller.probe(),
            executor.SupportedV1(
                "linux-x86_64",
                executor.SANDBOX_POLICY_RELEASE_V1,
            ),
        )

    def test_failed_single_thread_gate_suppresses_every_forking_probe(self) -> None:
        operations = _LateThreadOperations()
        cgroups = _CgroupFactory()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=cgroups,
        )

        report = executor.ControlledExecutorV1(native).probe()

        self.assertEqual(
            report,
            executor.UnsupportedV1(
                (
                    executor.CapabilityFailureV1(
                        executor.CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
                        errno.EBUSY,
                    ),
                )
            ),
        )
        self.assertEqual(operations.probes, ["standard_fds", "single_threaded"])
        self.assertEqual(cgroups.observer_budgets, [])
        self.assertEqual(cgroups.probed, [])

    def test_second_single_thread_gate_suppresses_forking_probe(self) -> None:
        operations = _SecondSingleThreadFailureOperations()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=_CgroupFactory(),
        )

        report = executor.ControlledExecutorV1(native).probe()

        self.assertEqual(
            report,
            executor.UnsupportedV1(
                (
                    executor.CapabilityFailureV1(
                        executor.CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
                        errno.EBUSY,
                    ),
                )
            ),
        )
        self.assertEqual(
            operations.probes,
            ["standard_fds", "single_threaded", "execveat", "close_range", "single_threaded"],
        )

    def test_one_probe_capability_is_forwarded_to_exactly_one_run(self) -> None:
        capability = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )
        expected = executor.CompletedV1(
            binary_sha256=hashlib.sha256(_static_elf()).digest(),
            stdout=b"answer",
            stderr=b"",
        )
        backend = _Backend(capability, expected)
        request = _request()

        actual = executor.ControlledExecutorV1(backend).execute(request)

        self.assertEqual(actual, expected)
        self.assertEqual(backend.probe_calls, 1)
        self.assertEqual(len(backend.received), 1)
        received_request, received_capability = backend.received[0]
        self.assertIs(received_request, request)
        self.assertEqual(received_capability, capability)
        self.assertIsNot(received_capability, capability)

    def test_preprobed_capability_is_consumed_without_a_second_probe(self) -> None:
        capability = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )
        request = _request()
        expected = executor.CompletedV1(
            binary_sha256=hashlib.sha256(request.executable).digest(),
            stdout=b"answer",
            stderr=b"",
        )
        backend = _Backend(capability, expected)
        controller = executor.ControlledExecutorV1(backend)
        issued = controller.probe()
        self.assertEqual(issued, capability)
        self.assertIsNot(issued, capability)

        with mock.patch.object(
            backend,
            "probe",
            side_effect=AssertionError("execute must consume the supplied observation"),
        ):
            actual = controller.execute(request, issued)

        self.assertEqual(actual, expected)
        self.assertEqual(backend.probe_calls, 1)
        self.assertEqual(
            controller.execute(request, issued),
            executor.ObserverFailureV1(executor.ObserverReasonV1.PROBE_FAILED),
        )

    def test_backend_reused_report_cannot_renew_a_stale_controller_lease(self) -> None:
        backend_report = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )
        request = _request()
        expected = executor.CompletedV1(
            binary_sha256=hashlib.sha256(request.executable).digest(),
            stdout=b"answer",
            stderr=b"",
        )
        backend = _Backend(backend_report, expected)
        controller = executor.ControlledExecutorV1(backend)

        stale = controller.probe()
        fresh = controller.probe()

        self.assertIs(type(stale), executor.SupportedV1)
        self.assertIs(type(fresh), executor.SupportedV1)
        self.assertIsNot(stale, fresh)
        self.assertEqual(
            controller.execute(request, stale),
            executor.ObserverFailureV1(executor.ObserverReasonV1.PROBE_FAILED),
        )
        self.assertEqual(backend.received, [])
        self.assertEqual(controller.execute(request, fresh), expected)
        self.assertEqual(len(backend.received), 1)

    def test_controller_capability_cannot_be_duplicated_across_fork(self) -> None:
        backend_report = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )
        request = _request()
        expected = executor.CompletedV1(
            binary_sha256=hashlib.sha256(request.executable).digest(),
            stdout=b"answer",
            stderr=b"",
        )
        backend = _Backend(backend_report, expected)
        controller = executor.ControlledExecutorV1(backend)
        capability = controller.probe()
        read_descriptor, write_descriptor = os.pipe()

        child = os.fork()
        if child == 0:
            os.close(read_descriptor)
            try:
                result = controller.execute(request, capability)
                payload = (
                    b"blocked"
                    if result
                    == executor.ObserverFailureV1(
                        executor.ObserverReasonV1.PROBE_FAILED
                    )
                    else b"executed"
                )
                os.write(write_descriptor, payload)
                status = 0
            except BaseException:
                status = 1
            finally:
                os.close(write_descriptor)
            os._exit(status)

        os.close(write_descriptor)
        try:
            payload = os.read(read_descriptor, 32)
        finally:
            os.close(read_descriptor)
        waited, status = os.waitpid(child, 0)

        self.assertEqual(waited, child)
        self.assertTrue(os.WIFEXITED(status))
        self.assertEqual(os.WEXITSTATUS(status), 0)
        self.assertEqual(payload, b"blocked")
        self.assertEqual(controller.execute(request, capability), expected)
        self.assertEqual(len(backend.received), 1)

    def test_capability_cannot_cross_a_backend_replacement(self) -> None:
        request = _request()
        capability = executor.SupportedV1(
            "linux-x86_64",
            executor.SANDBOX_POLICY_RELEASE_V1,
        )
        expected = executor.CompletedV1(
            binary_sha256=hashlib.sha256(request.executable).digest(),
            stdout=b"answer",
            stderr=b"",
        )
        original = _Backend(capability, expected)
        replacement = _Backend(capability, expected)
        controller = executor.ControlledExecutorV1(original)
        issued = controller.probe()
        self.assertEqual(issued, capability)
        self.assertIsNot(issued, capability)
        controller._backend = replacement

        result = controller.execute(request, issued)

        self.assertEqual(
            result,
            executor.ObserverFailureV1(executor.ObserverReasonV1.PROBE_FAILED),
        )
        self.assertEqual(original.received, [])
        self.assertEqual(replacement.received, [])

    def test_native_run_consumes_capability_without_reprobe_and_rejects_foreign(self) -> None:
        operations = _ProbeOperations()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=_CgroupFactory(),
        )
        controller = executor.ControlledExecutorV1(native)
        capability = controller.probe()
        self.assertIs(type(capability), executor.SupportedV1)
        creates_after_probe = operations.events.count("create")
        request = _request(cwd=b"/definitely-missing-labcolors-cwd")

        with mock.patch.object(
            native,
            "probe",
            side_effect=AssertionError("run must consume, not repeat, capability probe"),
        ):
            result = controller.execute(request, capability)

        self.assertIs(type(result), executor.SandboxSetupFailedV1)
        self.assertEqual(result.stage, executor.SetupStageV1.CWD)
        self.assertEqual(operations.events.count("create"), creates_after_probe + 1)

        equal_but_foreign = executor.SupportedV1(
            capability.platform,
            capability.sandbox_policy_release,
        )
        self.assertEqual(equal_but_foreign, capability)
        self.assertIsNot(equal_but_foreign, capability)
        for invalid in (capability, equal_but_foreign, object()):
            with self.subTest(invalid=invalid):
                with mock.patch.object(
                    executor,
                    "_seal_executable_v1",
                    side_effect=AssertionError("foreign capability must not execute"),
                ):
                    rejected = controller.execute(  # type: ignore[arg-type]
                        request,
                        invalid,
                    )
                self.assertEqual(
                    rejected,
                    executor.ObserverFailureV1(
                        executor.ObserverReasonV1.PROBE_FAILED,
                    ),
                )

    def test_native_capability_has_one_atomic_consumer(self) -> None:
        operations = _ProbeOperations()
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=operations,
            cgroup_factory=_CgroupFactory(),
        )
        controller = executor.ControlledExecutorV1(native)
        capability = controller.probe()
        self.assertIs(type(capability), executor.SupportedV1)
        creates_after_probe = operations.events.count("create")
        request = _request(cwd=b"/definitely-missing-labcolors-cwd")

        with ThreadPoolExecutor(max_workers=2) as pool:
            results = tuple(
                pool.map(
                    lambda _index: controller.execute(request, capability),
                    range(2),
                )
            )

        self.assertEqual(operations.events.count("create"), creates_after_probe + 1)
        self.assertEqual(
            sum(type(result) is executor.SandboxSetupFailedV1 for result in results),
            1,
        )
        self.assertEqual(
            sum(
                result
                == executor.ObserverFailureV1(executor.ObserverReasonV1.PROBE_FAILED)
                for result in results
            ),
            1,
        )

    def test_failed_native_probe_revokes_earlier_capability(self) -> None:
        native = executor.NativeLinuxBackendV1(
            cgroup_parent="/delegated-proof-cgroup",
            platform_name="linux",
            machine_name="x86_64",
            operations=_ProbeOperations(),
            cgroup_factory=_CgroupFactory(),
        )
        controller = executor.ControlledExecutorV1(native)
        stale = controller.probe()
        self.assertIs(type(stale), executor.SupportedV1)
        native._platform_name = "darwin"

        failed = controller.probe()

        self.assertIs(type(failed), executor.UnsupportedV1)
        with mock.patch.object(
            executor,
            "_seal_executable_v1",
            side_effect=AssertionError("failed probe must revoke earlier capability"),
        ):
            rejected = controller.execute(_request(), stale)
        self.assertEqual(
            rejected,
            executor.ObserverFailureV1(executor.ObserverReasonV1.PROBE_FAILED),
        )

    def test_untrusted_backend_cannot_return_unbounded_output(self) -> None:
        class ExplosiveEquality:
            def __eq__(self, _other: object) -> bool:
                raise RuntimeError("comparison escaped")

        invalid_results = (
            executor.CompletedV1(
                binary_sha256=b"x" * 32,
                stdout=b"17 bytes overflow",
                stderr=b"",
            ),
            executor.CompletedV1(ExplosiveEquality(), b"", b""),
            executor.ObserverFailureV1(ExplosiveEquality()),
        )
        for invalid in invalid_results:
            with self.subTest(invalid=type(invalid).__name__):
                self.assertFalse(executor.result_matches_request_v1(invalid, _request()))
                backend = _Backend(
                    executor.SupportedV1(
                        "linux-x86_64",
                        executor.SANDBOX_POLICY_RELEASE_V1,
                    ),
                    invalid,
                )

                result = executor.ControlledExecutorV1(backend).execute(_request())

                self.assertIs(type(result), executor.ObserverFailureV1)
                self.assertEqual(
                    result.reason,
                    executor.ObserverReasonV1.BACKEND_CONTRACT,
                )
                self.assertFalse(hasattr(result, "stdout"))

    def test_process_failures_remain_distinct_from_evaluator_resource_outcome(self) -> None:
        digest = hashlib.sha256(_static_elf()).digest()
        results: tuple[executor.ExecutionResultV1, ...] = (
            executor.ExitNonZeroV1(digest, b"", b"bad", 17),
            executor.SignaledV1(digest, b"", b"", 11, True),
            executor.TimedOutV1(digest, b"", b"", 1_000_000_000),
            executor.OomKilledV1(digest, b"", b"", 1),
            executor.OutputLimitExceededV1(
                digest,
                b"x" * 16,
                b"",
                executor.OutputStreamV1.STDOUT,
                16,
            ),
        )
        for expected in results:
            with self.subTest(result_type=type(expected).__name__):
                backend = _Backend(
                    executor.SupportedV1(
                        "linux-x86_64", executor.SANDBOX_POLICY_RELEASE_V1
                    ),
                    expected,
                )
                actual = executor.ControlledExecutorV1(backend).execute(_request())
                self.assertEqual(actual, expected)
                self.assertNotIn("ResourceLimit", type(actual).__name__)

    def test_executor_exports_observations_but_no_receipt_mint(self) -> None:
        self.assertFalse(any("Receipt" in name for name in dir(executor)))
        self.assertFalse(hasattr(executor.ControlledExecutorV1, "mint"))
        self.assertFalse(hasattr(executor.ControlledExecutorV1, "admit"))


class SameObjectAndObserverProtocolTests(unittest.TestCase):
    @staticmethod
    def _seccomp_verdict(
        program: list[object],
        syscall_number: int,
        *,
        architecture: int = 0xC000003E,
        arguments: tuple[int, ...] = (0, 0, 0, 0, 0, 0),
    ) -> int:
        words = {0: syscall_number, 4: architecture}
        for index, argument in enumerate(arguments):
            words[16 + index * 8] = argument & 0xFFFFFFFF
            words[20 + index * 8] = (argument >> 32) & 0xFFFFFFFF
        accumulator = 0
        pc = 0
        for _ in range(1024):
            instruction = program[pc]
            if instruction.code == 0x20:  # BPF_LD | BPF_W | BPF_ABS
                accumulator = words.get(instruction.k, 0)
                pc += 1
            elif instruction.code == 0x15:  # BPF_JMP | BPF_JEQ | BPF_K
                pc += 1 + (instruction.jt if accumulator == instruction.k else instruction.jf)
            elif instruction.code == 0x06:  # BPF_RET | BPF_K
                return instruction.k
            else:
                raise AssertionError(f"unknown BPF opcode {instruction.code:#x}")
        raise AssertionError("seccomp program did not terminate")

    def test_seccomp_filter_denies_files_network_processes_and_exec_path_swaps(self) -> None:
        operations = executor._NativeLinuxOperationsV1()
        program = operations._seccomp_program(exec_fd=3, setup_error_fd=4)
        killed = 0x80000000
        allowed = 0x7FFF0000

        self.assertEqual(self._seccomp_verdict(program, 1), allowed)  # write
        for syscall_number in (2, 41, 56, 257, 319):
            with self.subTest(syscall_number=syscall_number):
                self.assertEqual(self._seccomp_verdict(program, syscall_number), killed)
        self.assertEqual(
            self._seccomp_verdict(program, 302, arguments=(0, 0, 0, 0, 0, 0)),
            allowed,
        )
        for foreign_pid in (1, 42, 0xFFFFFFFFFFFFFFFF):
            with self.subTest(prlimit_pid=foreign_pid):
                self.assertEqual(
                    self._seccomp_verdict(
                        program,
                        302,
                        arguments=(foreign_pid, 0, 0, 0, 0, 0),
                    ),
                    killed,
                )
        self.assertEqual(
            self._seccomp_verdict(
                program,
                322,
                arguments=(3, 0, 0, 0, 0x1000, 0),
            ),
            allowed,
        )
        self.assertEqual(
            self._seccomp_verdict(
                program,
                322,
                arguments=(5, 0, 0, 0, 0x1000, 0),
            ),
            killed,
        )
        self.assertEqual(
            self._seccomp_verdict(
                program,
                322,
                arguments=(3, 0, 0, 0, 0, 0),
            ),
            killed,
        )
        self.assertEqual(
            self._seccomp_verdict(program, 1, architecture=0x40000003),
            killed,
        )

    def test_hash_and_exec_use_the_same_sealed_memfd(self) -> None:
        operations = _MemfdOperations()
        executable = _static_elf()

        sealed = executor._seal_executable_v1(executable, operations)
        sealed.execveat(
            (b"proof-evaluator",),
            ((b"LC_ALL", b"C"),),
            operations,
        )

        self.assertEqual(sealed.fd, operations.fd)
        self.assertEqual(sealed.sha256, hashlib.sha256(executable).digest())
        self.assertEqual(
            operations.seals_by_fd[sealed.fd] & executor.REQUIRED_FILE_SEALS_V1,
            executor.REQUIRED_FILE_SEALS_V1,
        )
        self.assertEqual(operations.exec_calls[0][0], sealed.fd)
        self.assertEqual(
            operations.events,
            ["create", "write", "chmod", "seal", "get_seals", "pread", "execveat"],
        )

    def test_child_error_packet_rejects_unknown_trailing_and_truncated_bytes(self) -> None:
        valid = executor._encode_child_error_packet_v1(
            executor.SetupStageV1.EXECVEAT,
            8,
        )
        parsed = executor._parse_child_error_packet_v1(valid)
        self.assertEqual(parsed.stage, executor.SetupStageV1.EXECVEAT)
        self.assertEqual(parsed.errno, 8)

        cases = (
            b"BAD!" + valid[4:],
            valid[:-1],
            valid + b"\0",
            valid[:5] + b"\xff" + valid[6:],
        )
        for packet in cases:
            with self.subTest(packet=packet):
                with self.assertRaises(executor.ObserverProtocolErrorV1):
                    executor._parse_child_error_packet_v1(packet)

    def test_capture_keeps_exact_cap_and_detects_only_cap_plus_one(self) -> None:
        captured = bytearray()

        self.assertFalse(executor._append_bounded_v1(captured, b"1234", 4))
        self.assertEqual(bytes(captured), b"1234")
        self.assertTrue(executor._append_bounded_v1(captured, b"5", 4))
        self.assertEqual(bytes(captured), b"1234")

    def test_observer_does_not_infer_oom_from_sigkill(self) -> None:
        digest = hashlib.sha256(_static_elf()).digest()

        signal_only = executor._classify_process_v1(
            digest=digest,
            stdout=b"",
            stderr=b"",
            child_status=9,
            oom_kill_delta=0,
            residual=False,
            setup_packet=b"",
            terminal=None,
            limits=_limits(),
        )
        actual_oom = executor._classify_process_v1(
            digest=digest,
            stdout=b"",
            stderr=b"",
            child_status=9,
            oom_kill_delta=2,
            residual=False,
            setup_packet=b"",
            terminal=None,
            limits=_limits(),
        )

        self.assertEqual(signal_only, executor.SignaledV1(digest, b"", b"", 9, False))
        self.assertEqual(actual_oom, executor.OomKilledV1(digest, b"", b"", 2))

    def test_cgroup_limits_are_read_back_before_execution(self) -> None:
        expected = {
            b"memory.max": b"67108864\n",
            b"memory.swap.max": b"0\n",
            b"memory.oom.group": b"1\n",
            b"pids.max": b"1\n",
        }
        _ReadbackCgroup(expected)._require_applied_limits(
            memory_max=64 * 1024 * 1024,
            pids_max=1,
        )

        for name in expected:
            hostile = dict(expected)
            hostile[name] = b"max\n" if name != b"memory.max" else b"67112960\n"
            with self.subTest(name=name):
                with self.assertRaises(OSError) as caught:
                    _ReadbackCgroup(hostile)._require_applied_limits(
                        memory_max=64 * 1024 * 1024,
                        pids_max=1,
                    )
                self.assertEqual(caught.exception.errno, errno.EPROTO)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent = root / "proof"
            observer = parent / "observer"
            observer.mkdir(parents=True)
            (parent / "pids.max").write_bytes(b"2\n")
            (parent / "pids.current").write_bytes(b"1\n")
            (observer / "pids.current").write_bytes(b"1\n")
            with mock.patch.object(
                executor,
                "_current_unified_cgroup_v1",
                return_value=observer,
            ):
                executor._CgroupV2V1.probe_observer_task_budget(parent)
                for path, hostile in (
                    (parent / "pids.max", b"3\n"),
                    (parent / "pids.current", b"2\n"),
                    (observer / "pids.current", b"2\n"),
                ):
                    original = path.read_bytes()
                    path.write_bytes(hostile)
                    with self.subTest(path=path.name, hostile=hostile):
                        with self.assertRaises(OSError):
                            executor._CgroupV2V1.probe_observer_task_budget(
                                parent
                            )
                    path.write_bytes(original)

    def test_observer_initialization_failure_closes_fds_and_reaps_child(self) -> None:
        stdin_read, stdin_write = os.pipe()
        stdout_read, stdout_write = os.pipe()
        stderr_read, stderr_write = os.pipe()
        setup_read, setup_write = os.pipe()
        pid = os.fork()
        if pid == 0:
            os.close(stdin_write)
            os.close(stdout_read)
            os.close(stderr_read)
            os.close(setup_read)
            while True:
                signal.pause()

        os.close(stdin_read)
        os.close(stdout_write)
        os.close(stderr_write)
        os.close(setup_write)
        observed_fds = (stdin_write, stdout_read, stderr_read, setup_read)
        backend = executor.NativeLinuxBackendV1()
        try:
            with mock.patch.object(
                executor.selectors,
                "DefaultSelector",
                side_effect=OSError(errno.EMFILE, "selector unavailable"),
            ):
                result = backend._observe(
                    _request(),
                    hashlib.sha256(_static_elf()).digest(),
                    pid,
                    _ObserverCgroup(pid),
                    0,
                    *observed_fds,
                )
            self.assertEqual(
                result,
                executor.ObserverFailureV1(executor.ObserverReasonV1.BACKEND_EXCEPTION),
            )
            for descriptor in observed_fds:
                with self.subTest(descriptor=descriptor):
                    with self.assertRaises(OSError) as caught:
                        os.fstat(descriptor)
                    self.assertEqual(caught.exception.errno, errno.EBADF)
            with self.assertRaises(ChildProcessError):
                os.waitpid(pid, os.WNOHANG)
        finally:
            for descriptor in observed_fds:
                try:
                    os.close(descriptor)
                except OSError:
                    pass
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                os.waitpid(pid, 0)
            except ChildProcessError:
                pass

    def test_fork_rechecks_single_thread_precondition_after_all_setup(self) -> None:
        backend = executor.NativeLinuxBackendV1()
        cwd_fd = os.open("/", os.O_RDONLY)
        try:
            with mock.patch.object(
                executor.os,
                "fork",
                side_effect=AssertionError("fork must not run after failed final gate"),
            ):
                result = backend._fork_and_observe(
                    _request(cwd=b"/"),
                    executor._SealedExecutableV1(
                        41,
                        len(_static_elf()),
                        hashlib.sha256(_static_elf()).digest(),
                    ),
                    _LateThreadOperations(),
                    cwd_fd,
                    object(),
                )
        finally:
            os.close(cwd_fd)

        self.assertEqual(
            result,
            executor.SandboxSetupFailedV1(
                hashlib.sha256(_static_elf()).digest(),
                b"",
                b"",
                executor.SetupStageV1.OBSERVER_PRECONDITION,
                errno.EBUSY,
            ),
        )

    def test_controller_timeout_and_output_limit_are_not_child_outcomes(self) -> None:
        digest = hashlib.sha256(_static_elf()).digest()
        limits = _limits()

        timeout = executor._classify_process_v1(
            digest=digest,
            stdout=b"",
            stderr=b"",
            child_status=9,
            oom_kill_delta=1,
            residual=False,
            setup_packet=b"",
            terminal=("timeout", None),
            limits=limits,
        )
        output = executor._classify_process_v1(
            digest=digest,
            stdout=b"x" * limits.max_stdout_bytes,
            stderr=b"",
            child_status=9,
            oom_kill_delta=1,
            residual=False,
            setup_packet=b"",
            terminal=("output", executor.OutputStreamV1.STDOUT),
            limits=limits,
        )

        self.assertEqual(
            timeout,
            executor.TimedOutV1(digest, b"", b"", limits.wall_timeout_ns),
        )
        self.assertEqual(
            output,
            executor.OutputLimitExceededV1(
                digest,
                b"x" * limits.max_stdout_bytes,
                b"",
                executor.OutputStreamV1.STDOUT,
                limits.max_stdout_bytes,
            ),
        )


@unittest.skipUnless(
    sys.platform == "linux" and os.environ.get("LABCOLORS_EXECUTOR_CGROUP_V1"),
    "requires Linux and an explicit delegated cgroup v2 parent",
)
class NativeLinuxIntegrationTests(unittest.TestCase):
    def setUp(self) -> None:
        raw_parent = os.environ["LABCOLORS_EXECUTOR_CGROUP_V1"]
        self.cgroup_parent = Path(raw_parent)
        self.backend = executor.NativeLinuxBackendV1(self.cgroup_parent)
        self.controller = executor.ControlledExecutorV1(self.backend)
        report = self.controller.probe()
        self.assertEqual(
            report,
            executor.SupportedV1(
                "linux-x86_64",
                executor.SANDBOX_POLICY_RELEASE_V1,
            ),
        )

    def _native_request(
        self,
        code: bytes,
        *,
        stdin: bytes = b"",
        stdout_limit: int = 4,
        timeout_ns: int = 500_000_000,
        memory_max: int = 64 * 1024 * 1024,
    ) -> executor.ExecutionRequestV1:
        return executor.ExecutionRequestV1(
            executable=_linux_executable_elf(code),
            argv=(b"native-executor-fixture",),
            environment=(),
            cwd=b"/",
            stdin=stdin,
            umask=0o077,
            limits=executor.ExecutionLimitsV1(
                max_executable_bytes=4096,
                max_stdin_bytes=16,
                max_argument_bytes=512,
                max_stdout_bytes=stdout_limit,
                max_stderr_bytes=4,
                wall_timeout_ns=timeout_ns,
                memory_max_bytes=memory_max,
                pids_max=1,
            ),
        )

    def _owned_cgroups(self) -> set[str]:
        prefix = f"labcolors-executor-{os.getpid()}-"
        return {
            child.name
            for child in self.cgroup_parent.iterdir()
            if child.name.startswith(prefix)
        }

    def test_real_kernel_success_output_timeout_signal_oom_and_cleanup(self) -> None:
        before = self._owned_cgroups()
        controlled = self.controller

        exit_request = self._native_request(_LINUX_EXIT_ZERO)
        exit_result = controlled.execute(exit_request)
        self.assertEqual(
            exit_result,
            executor.CompletedV1(
                hashlib.sha256(exit_request.executable).digest(),
                b"",
                b"",
            ),
        )

        echo_request = self._native_request(_LINUX_ECHO_FOUR, stdin=b"PING")
        echo_result = controlled.execute(echo_request)
        self.assertEqual(
            echo_result,
            executor.CompletedV1(
                hashlib.sha256(echo_request.executable).digest(),
                b"PING",
                b"",
            ),
        )

        output_request = self._native_request(_LINUX_WRITE_FIVE_AND_LOOP)
        output_result = controlled.execute(output_request)
        self.assertEqual(
            output_result,
            executor.OutputLimitExceededV1(
                hashlib.sha256(output_request.executable).digest(),
                b"1234",
                b"",
                executor.OutputStreamV1.STDOUT,
                4,
            ),
        )

        timeout_request = self._native_request(
            _LINUX_BUSY_LOOP,
            timeout_ns=50_000_000,
        )
        timeout_result = controlled.execute(timeout_request)
        self.assertEqual(
            timeout_result,
            executor.TimedOutV1(
                hashlib.sha256(timeout_request.executable).digest(),
                b"",
                b"",
                timeout_request.limits.wall_timeout_ns,
            ),
        )

        signal_request = self._native_request(_LINUX_SIGILL)
        signal_result = controlled.execute(signal_request)
        self.assertEqual(
            signal_result,
            executor.SignaledV1(
                hashlib.sha256(signal_request.executable).digest(),
                b"",
                b"",
                signal.SIGILL,
                False,
            ),
        )

        foreign_prlimit_request = self._native_request(_LINUX_FOREIGN_PRLIMIT)
        foreign_prlimit_result = controlled.execute(foreign_prlimit_request)
        self.assertIs(type(foreign_prlimit_result), executor.SignaledV1)
        self.assertEqual(foreign_prlimit_result.signal_number, signal.SIGSYS)

        oom_request = self._native_request(
            _LINUX_ALLOCATE_UNTIL_OOM,
            timeout_ns=5_000_000_000,
            memory_max=16 * 1024 * 1024,
        )
        oom_result = controlled.execute(oom_request)
        self.assertIs(type(oom_result), executor.OomKilledV1)
        self.assertGreater(oom_result.oom_kill_delta, 0)

        self.assertEqual(self._owned_cgroups(), before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
