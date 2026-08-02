#!/usr/bin/env python3
"""Fail-closed Linux process boundary for proof evaluators.

This module returns process observations only.  A caller must bind those
observations to source/build evidence elsewhere; no value here can certify that
provenance relationship.
"""

from __future__ import annotations

import ctypes
import errno as errno_module
import fcntl
import hashlib
import itertools
import os
import platform
import posixpath
import resource
import selectors
import signal
import stat
import struct
import sys
import threading
import time
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Callable, NoReturn, Protocol, TypeAlias


EXECUTION_PLATFORM_V1 = "linux-x86_64"
SANDBOX_POLICY_RELEASE_V1 = "labcolors.proof-region.executor.linux-x86_64.v1"

_INVOCATION_ID_LABEL_V1 = b"labcolors.proof-region.execution-invocation.v1\0"
_PLATFORM_ID_LABEL_V1 = b"labcolors.proof-region.execution-platform.v1\0"

# Linux UAPI values are fixed by fcntl.h.  Requiring F_SEAL_EXEC makes an older
# kernel an explicit Unsupported host instead of silently weakening the object.
F_SEAL_SEAL_V1 = 0x0001
F_SEAL_SHRINK_V1 = 0x0002
F_SEAL_GROW_V1 = 0x0004
F_SEAL_WRITE_V1 = 0x0008
F_SEAL_EXEC_V1 = 0x0020
REQUIRED_FILE_SEALS_V1 = (
    F_SEAL_SEAL_V1
    | F_SEAL_SHRINK_V1
    | F_SEAL_GROW_V1
    | F_SEAL_WRITE_V1
    | F_SEAL_EXEC_V1
)

_MFD_CLOEXEC = 0x0001
_MFD_ALLOW_SEALING = 0x0002
_MFD_EXEC = 0x0010
_F_ADD_SEALS = 1033
_F_GET_SEALS = 1034
_AT_EMPTY_PATH = 0x1000

_SYS_SECCOMP_X86_64 = 317
_SYS_EXECVEAT_X86_64 = 322
_SYS_CLOSE_RANGE_X86_64 = 436
_SYS_PRLIMIT64_X86_64 = 302

_CLONE_NEWNS = 0x00020000
_CLONE_NEWUSER = 0x10000000
_CLONE_NEWNET = 0x40000000
_MS_REC = 0x4000
_MS_PRIVATE = 1 << 18

_PR_SET_NO_NEW_PRIVS = 38
_SECCOMP_SET_MODE_FILTER = 1
_SECCOMP_FILTER_FLAG_TSYNC = 1
_SECCOMP_RET_KILL_PROCESS = 0x80000000
_SECCOMP_RET_ALLOW = 0x7FFF0000
_AUDIT_ARCH_X86_64 = 0xC000003E

_BPF_LD_W_ABS = 0x20
_BPF_JMP_JEQ_K = 0x15
_BPF_RET_K = 0x06

_CHILD_PACKET = struct.Struct(">4sBBI")
_CHILD_PACKET_MAGIC = b"LCXE"
_ELF_HEADER = struct.Struct("<16sHHIQQQIHHHHHH")
_ELF_PROGRAM_HEADER = struct.Struct("<IIQQQQQQ")
_ELF_DYNAMIC_ENTRY = struct.Struct("<QQ")
# Invocation wire uses four bytes for argv/environment cardinalities.
_U32_CARDINALITY_LIMIT_V1 = 1 << 32


def _execution_identity_v1(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(len(chunk).to_bytes(8, "big") + chunk for chunk in chunks)
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def _sequence_count_v1(value: tuple[object, ...]) -> int:
    return len(value)


def enter_observer_cgroup_v1(parent: Path) -> None:
    """Move this dedicated one-shot controller into its observer cgroup.

    Placement is executor infrastructure, not an engine semantic concern;
    every evaluator lane shares the exact ownership and no-follow boundary.
    """

    if not isinstance(parent, Path) or not parent.is_absolute():
        raise TypeError("cgroup parent must be an absolute Path")
    directory_fd = os.open(
        os.fsencode(parent / "observer"),
        os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
    )
    try:
        metadata = os.fstat(directory_fd)
        if not stat.S_ISDIR(metadata.st_mode):
            raise OSError("observer cgroup is not a directory")
        procs_fd = os.open(
            b"cgroup.procs",
            os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW,
            dir_fd=directory_fd,
        )
        try:
            payload = str(os.getpid()).encode("ascii")
            if os.write(procs_fd, payload) != len(payload):
                raise OSError("short cgroup placement write")
        finally:
            os.close(procs_fd)
    finally:
        os.close(directory_fd)


class RequestReasonV1(str, Enum):
    WRONG_TYPE = "wrong_type"
    INVALID_LIMIT = "invalid_limit"
    EMPTY_ARGV_ZERO = "empty_argv_zero"
    NUL_BYTE = "nul_byte"
    INVALID_ENVIRONMENT_KEY = "invalid_environment_key"
    DUPLICATE_ENVIRONMENT = "duplicate_environment"
    NONCANONICAL_ENVIRONMENT = "noncanonical_environment"
    RELATIVE_CWD = "relative_cwd"
    NONCANONICAL_CWD = "noncanonical_cwd"
    LIMIT_EXCEEDED = "limit_exceeded"
    INVALID_ELF = "invalid_elf"
    DYNAMIC_EXECUTABLE = "dynamic_executable"


class ExecutionRequestErrorV1(ValueError):
    def __init__(self, reason: RequestReasonV1, field: str) -> None:
        super().__init__(f"{field}: {reason.value}")
        self.reason = reason
        self.field = field


class ExecutionIdentityReasonV1(str, Enum):
    WRONG_REQUEST_TYPE = "wrong_request_type"
    REQUEST_NOT_ADMITTED = "request_not_admitted"
    FOREIGN_PLATFORM = "foreign_platform"


@dataclass(frozen=True)
class ExecutionIdentityRejectedV1:
    reason: ExecutionIdentityReasonV1

    def __post_init__(self) -> None:
        if type(self.reason) is not ExecutionIdentityReasonV1:
            raise TypeError("reason must be ExecutionIdentityReasonV1")


ExecutionIdentityResultV1: TypeAlias = bytes | ExecutionIdentityRejectedV1


class CapabilityReasonV1(str, Enum):
    HOST_NOT_LINUX = "host_not_linux"
    ARCHITECTURE_NOT_SUPPORTED = "architecture_not_supported"
    CGROUP_PARENT_NOT_DECLARED = "cgroup_parent_not_declared"
    CGROUP_V2_UNAVAILABLE = "cgroup_v2_unavailable"
    EXECUTABLE_MEMFD_UNAVAILABLE = "executable_memfd_unavailable"
    FILE_SEALS_UNAVAILABLE = "file_seals_unavailable"
    EXECVEAT_UNAVAILABLE = "execveat_unavailable"
    CLOSE_RANGE_UNAVAILABLE = "close_range_unavailable"
    NETWORK_NAMESPACE_UNAVAILABLE = "network_namespace_unavailable"
    SECCOMP_FILTER_UNAVAILABLE = "seccomp_filter_unavailable"
    STANDARD_FDS_UNAVAILABLE = "standard_fds_unavailable"
    OBSERVER_NOT_SINGLE_THREADED = "observer_not_single_threaded"
    OBSERVER_TASK_BUDGET_UNAVAILABLE = "observer_task_budget_unavailable"
    OBSERVATION_INVALIDATED = "observation_invalidated"
    KERNEL_API_UNAVAILABLE = "kernel_api_unavailable"


@dataclass(frozen=True)
class CapabilityFailureV1:
    reason: CapabilityReasonV1
    errno: int | None

    def __post_init__(self) -> None:
        if type(self.reason) is not CapabilityReasonV1:
            raise TypeError("reason must be CapabilityReasonV1")
        if self.errno is not None and (type(self.errno) is not int or self.errno <= 0):
            raise TypeError("errno must be a positive int or None")


@dataclass(frozen=True)
class UnsupportedV1:
    failures: tuple[CapabilityFailureV1, ...]

    def __post_init__(self) -> None:
        if (
            type(self.failures) is not tuple
            or not self.failures
            or any(type(item) is not CapabilityFailureV1 for item in self.failures)
            or len(set(self.failures)) != len(self.failures)
        ):
            raise TypeError("failures must be a nonempty unique tuple")


class SupportedV1(tuple):
    """Exact immutable coordinates of one supported execution platform."""

    __slots__ = ()

    def __new__(
        cls,
        platform: str,
        sandbox_policy_release: str,
    ) -> SupportedV1:
        if type(platform) is not str or platform != EXECUTION_PLATFORM_V1:
            raise TypeError("unknown execution platform")
        if (
            type(sandbox_policy_release) is not str
            or sandbox_policy_release != SANDBOX_POLICY_RELEASE_V1
        ):
            raise TypeError("unknown sandbox policy release")
        return tuple.__new__(cls, (platform, sandbox_policy_release))

    @property
    def platform(self) -> str:
        return self[0]

    @property
    def sandbox_policy_release(self) -> str:
        return self[1]


CapabilityReportV1: TypeAlias = SupportedV1 | UnsupportedV1


def _invalidated_capability_report_v1() -> UnsupportedV1:
    return UnsupportedV1(
        (
            CapabilityFailureV1(
                CapabilityReasonV1.OBSERVATION_INVALIDATED,
                errno_module.EBUSY,
            ),
        )
    )


def _kernel_api_unavailable_report_v1() -> UnsupportedV1:
    return UnsupportedV1(
        (
            CapabilityFailureV1(
                CapabilityReasonV1.KERNEL_API_UNAVAILABLE,
                None,
            ),
        )
    )


_EXECUTION_LIMIT_FIELDS_V1 = (
    "max_executable_bytes",
    "max_stdin_bytes",
    "max_argument_bytes",
    "max_stdout_bytes",
    "max_stderr_bytes",
    "wall_timeout_ns",
    "memory_max_bytes",
    "pids_max",
)


class ExecutionLimitsV1(tuple):
    """Immutable resource coordinates admitted by the execution wire."""

    __slots__ = ()

    def __new__(
        cls,
        max_executable_bytes: int,
        max_stdin_bytes: int,
        max_argument_bytes: int,
        max_stdout_bytes: int,
        max_stderr_bytes: int,
        wall_timeout_ns: int,
        memory_max_bytes: int,
        pids_max: int,
    ) -> ExecutionLimitsV1:
        values = (
            max_executable_bytes,
            max_stdin_bytes,
            max_argument_bytes,
            max_stdout_bytes,
            max_stderr_bytes,
            wall_timeout_ns,
            memory_max_bytes,
            pids_max,
        )
        # Every limit is encoded as u64 in the invocation identity. Admission
        # owns that representability boundary so identity derivation is total.
        positive = frozenset((0, 1, 2, 5, 6, 7))
        for index, (field_name, value) in enumerate(
            zip(_EXECUTION_LIMIT_FIELDS_V1, values, strict=True)
        ):
            minimum = 1 if index in positive else 0
            if type(value) is not int or value < minimum or value >= 1 << 64:
                raise ExecutionRequestErrorV1(RequestReasonV1.INVALID_LIMIT, field_name)
        # V1's syscall policy denies clone/fork/vfork; a larger cgroup task
        # budget would advertise a concurrency capability the executor lacks.
        if pids_max != 1:
            raise ExecutionRequestErrorV1(RequestReasonV1.INVALID_LIMIT, "pids_max")
        return tuple.__new__(cls, values)

    max_executable_bytes = property(lambda self: self[0])
    max_stdin_bytes = property(lambda self: self[1])
    max_argument_bytes = property(lambda self: self[2])
    max_stdout_bytes = property(lambda self: self[3])
    max_stderr_bytes = property(lambda self: self[4])
    wall_timeout_ns = property(lambda self: self[5])
    memory_max_bytes = property(lambda self: self[6])
    pids_max = property(lambda self: self[7])


class ExecutionRequestV1(tuple):
    """Deeply immutable invocation coordinates admitted as one value."""

    __slots__ = ()

    def __new__(
        cls,
        executable: bytes,
        argv: tuple[bytes, ...],
        environment: tuple[tuple[bytes, bytes], ...],
        cwd: bytes,
        stdin: bytes,
        umask: int,
        limits: ExecutionLimitsV1,
    ) -> ExecutionRequestV1:
        if type(limits) is not ExecutionLimitsV1:
            _request_fail(RequestReasonV1.WRONG_TYPE, "limits")
        try:
            limits = ExecutionLimitsV1(*limits)
        except ExecutionRequestErrorV1:
            raise
        except Exception:
            _request_fail(RequestReasonV1.WRONG_TYPE, "limits")
        if type(executable) is not bytes:
            _request_fail(RequestReasonV1.WRONG_TYPE, "executable")
        if not executable or len(executable) > limits.max_executable_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "executable")
        require_static_x86_64_elf_v1(executable)

        if type(argv) is not tuple or not argv:
            _request_fail(RequestReasonV1.WRONG_TYPE, "argv")
        if _sequence_count_v1(argv) >= _U32_CARDINALITY_LIMIT_V1:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "argv")
        for index, item in enumerate(argv):
            _require_bytes_without_nul(item, f"argv[{index}]")
        if not argv[0]:
            _request_fail(RequestReasonV1.EMPTY_ARGV_ZERO, "argv[0]")

        if type(environment) is not tuple:
            _request_fail(RequestReasonV1.WRONG_TYPE, "environment")
        if _sequence_count_v1(environment) >= _U32_CARDINALITY_LIMIT_V1:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "environment")
        previous: bytes | None = None
        argument_bytes = sum(len(item) + 1 for item in argv)
        for index, item in enumerate(environment):
            if type(item) is not tuple or len(item) != 2:
                _request_fail(RequestReasonV1.WRONG_TYPE, f"environment[{index}]")
            key, value = item
            _require_bytes_without_nul(key, f"environment[{index}].key")
            _require_bytes_without_nul(value, f"environment[{index}].value")
            if not key or b"=" in key:
                _request_fail(
                    RequestReasonV1.INVALID_ENVIRONMENT_KEY,
                    f"environment[{index}].key",
                )
            if previous == key:
                _request_fail(RequestReasonV1.DUPLICATE_ENVIRONMENT, "environment")
            if previous is not None and previous > key:
                _request_fail(RequestReasonV1.NONCANONICAL_ENVIRONMENT, "environment")
            previous = key
            argument_bytes += len(key) + len(value) + 2
        if argument_bytes > limits.max_argument_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "argv+environment")

        _require_bytes_without_nul(cwd, "cwd")
        if not cwd.startswith(b"/"):
            _request_fail(RequestReasonV1.RELATIVE_CWD, "cwd")
        if (
            posixpath.normpath(cwd) != cwd
            or cwd.startswith(b"//")
            or (cwd != b"/" and cwd.endswith(b"/"))
        ):
            _request_fail(RequestReasonV1.NONCANONICAL_CWD, "cwd")

        if type(stdin) is not bytes:
            _request_fail(RequestReasonV1.WRONG_TYPE, "stdin")
        if len(stdin) > limits.max_stdin_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "stdin")
        if type(umask) is not int or not 0 <= umask <= 0o777:
            _request_fail(RequestReasonV1.INVALID_LIMIT, "umask")
        return tuple.__new__(
            cls,
            (executable, argv, environment, cwd, stdin, umask, limits),
        )

    executable = property(lambda self: self[0])
    argv = property(lambda self: self[1])
    environment = property(lambda self: self[2])
    cwd = property(lambda self: self[3])
    stdin = property(lambda self: self[4])
    umask = property(lambda self: self[5])
    limits = property(lambda self: self[6])


def _invocation_identity_from_fields_v1(request: ExecutionRequestV1) -> bytes:
    chunks: list[bytes] = [hashlib.sha256(request.executable).digest()]
    chunks.append(len(request.argv).to_bytes(4, "big"))
    chunks.extend(request.argv)
    chunks.append(len(request.environment).to_bytes(4, "big"))
    for key, value in request.environment:
        chunks.extend((key, value))
    chunks.extend(
        (
            request.cwd,
            hashlib.sha256(request.stdin).digest(),
            len(request.stdin).to_bytes(8, "big"),
            request.umask.to_bytes(4, "big"),
        )
    )
    for value in request.limits:
        chunks.append(value.to_bytes(8, "big"))
    return _execution_identity_v1(_INVOCATION_ID_LABEL_V1, tuple(chunks))


def invocation_identity_v1(request: object) -> ExecutionIdentityResultV1:
    """Bind exactly the invocation state that passed request admission."""

    if type(request) is not ExecutionRequestV1:
        return ExecutionIdentityRejectedV1(
            ExecutionIdentityReasonV1.WRONG_REQUEST_TYPE
        )
    try:
        if type(request.limits) is not ExecutionLimitsV1:
            raise TypeError("foreign execution limits")
        replayed_limits = ExecutionLimitsV1(*request.limits)
        replayed = ExecutionRequestV1(
            request.executable,
            request.argv,
            request.environment,
            request.cwd,
            request.stdin,
            request.umask,
            replayed_limits,
        )
    except Exception:
        return ExecutionIdentityRejectedV1(
            ExecutionIdentityReasonV1.REQUEST_NOT_ADMITTED
        )
    if replayed != request:
        return ExecutionIdentityRejectedV1(
            ExecutionIdentityReasonV1.REQUEST_NOT_ADMITTED
        )
    return _invocation_identity_from_fields_v1(replayed)


def platform_identity_v1(report: object) -> ExecutionIdentityResultV1:
    """Bind the exact admitted execution platform and sandbox policy."""

    if type(report) is not SupportedV1:
        return ExecutionIdentityRejectedV1(
            ExecutionIdentityReasonV1.FOREIGN_PLATFORM
        )
    try:
        replayed = SupportedV1(report.platform, report.sandbox_policy_release)
        if replayed != report:
            raise TypeError("platform coordinates did not replay")
        return _execution_identity_v1(
            _PLATFORM_ID_LABEL_V1,
            (
                replayed.platform.encode("ascii"),
                replayed.sandbox_policy_release.encode("ascii"),
            ),
        )
    except Exception:
        return ExecutionIdentityRejectedV1(
            ExecutionIdentityReasonV1.FOREIGN_PLATFORM
        )


def _request_fail(reason: RequestReasonV1, field: str) -> NoReturn:
    raise ExecutionRequestErrorV1(reason, field)


def _require_bytes_without_nul(value: object, field: str) -> None:
    if type(value) is not bytes:
        _request_fail(RequestReasonV1.WRONG_TYPE, field)
    if b"\0" in value:
        _request_fail(RequestReasonV1.NUL_BYTE, field)


def require_static_x86_64_elf_v1(data: bytes) -> None:
    if type(data) is not bytes:
        _request_fail(RequestReasonV1.WRONG_TYPE, "executable")
    if len(data) < _ELF_HEADER.size:
        _request_fail(RequestReasonV1.INVALID_ELF, "executable")
    try:
        (
            ident,
            elf_type,
            machine,
            version,
            _entry,
            program_offset,
            _section_offset,
            _flags,
            header_size,
            program_entry_size,
            program_count,
            _section_entry_size,
            _section_count,
            _section_names,
        ) = _ELF_HEADER.unpack_from(data)
    except struct.error:
        _request_fail(RequestReasonV1.INVALID_ELF, "executable")
    if (
        ident[:7] != b"\x7fELF\x02\x01\x01"
        or ident[7] not in (0, 3)
        or elf_type not in (2, 3)
        or machine != 62
        or version != 1
        or header_size != _ELF_HEADER.size
        or program_entry_size != _ELF_PROGRAM_HEADER.size
        or program_count == 0
        or program_offset < header_size
    ):
        _request_fail(RequestReasonV1.INVALID_ELF, "executable")
    table_end = program_offset + program_count * program_entry_size
    if table_end > len(data):
        _request_fail(RequestReasonV1.INVALID_ELF, "executable")

    saw_load = False
    dynamic_ranges: list[tuple[int, int]] = []
    for index in range(program_count):
        offset = program_offset + index * program_entry_size
        try:
            (
                segment_type,
                _segment_flags,
                file_offset,
                _virtual_address,
                _physical_address,
                file_size,
                memory_size,
                _alignment,
            ) = _ELF_PROGRAM_HEADER.unpack_from(data, offset)
        except struct.error:
            _request_fail(RequestReasonV1.INVALID_ELF, "executable")
        if file_size > memory_size or file_offset + file_size > len(data):
            _request_fail(RequestReasonV1.INVALID_ELF, "executable")
        if segment_type == 1:
            saw_load = True
        elif segment_type == 3:
            _request_fail(RequestReasonV1.DYNAMIC_EXECUTABLE, "executable")
        elif segment_type == 2:
            dynamic_ranges.append((file_offset, file_size))
    if not saw_load:
        _request_fail(RequestReasonV1.INVALID_ELF, "executable")

    for start, size in dynamic_ranges:
        if size % _ELF_DYNAMIC_ENTRY.size != 0:
            _request_fail(RequestReasonV1.INVALID_ELF, "executable")
        saw_terminator = False
        for offset in range(start, start + size, _ELF_DYNAMIC_ENTRY.size):
            tag, _value = _ELF_DYNAMIC_ENTRY.unpack_from(data, offset)
            if tag == 0:
                saw_terminator = True
                break
            if tag == 1:
                _request_fail(RequestReasonV1.DYNAMIC_EXECUTABLE, "executable")
        if not saw_terminator:
            _request_fail(RequestReasonV1.INVALID_ELF, "executable")


class OutputStreamV1(str, Enum):
    STDOUT = "stdout"
    STDERR = "stderr"


class SetupStageV1(int, Enum):
    SEALED_EXECUTABLE = 1
    CGROUP_CREATE = 2
    CGROUP_ATTACH = 3
    CWD = 4
    NAMESPACE = 5
    MOUNT_PROPAGATION = 6
    FILE_DESCRIPTORS = 7
    SIGNAL_STATE = 8
    NO_NEW_PRIVILEGES = 9
    SECCOMP = 10
    EXECVEAT = 11
    OBSERVER_PRECONDITION = 12


class ObserverReasonV1(str, Enum):
    REQUEST_NOT_ADMITTED = "request_not_admitted"
    PROBE_FAILED = "probe_failed"
    BACKEND_EXCEPTION = "backend_exception"
    BACKEND_CONTRACT = "backend_contract"
    CHILD_PROTOCOL = "child_protocol"
    CGROUP_OBSERVATION = "cgroup_observation"
    CLEANUP_FAILED = "cleanup_failed"


@dataclass(frozen=True)
class CompletedV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class ExitNonZeroV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes
    exit_code: int


@dataclass(frozen=True)
class SignaledV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes
    signal_number: int
    core_dumped: bool


@dataclass(frozen=True)
class TimedOutV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes
    deadline_ns: int


@dataclass(frozen=True)
class OomKilledV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes
    oom_kill_delta: int


@dataclass(frozen=True)
class OutputLimitExceededV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes
    stream: OutputStreamV1
    limit: int


@dataclass(frozen=True)
class SandboxSetupFailedV1:
    binary_sha256: bytes | None
    stdout: bytes
    stderr: bytes
    stage: SetupStageV1
    errno: int


@dataclass(frozen=True)
class ResidualProcessesV1:
    binary_sha256: bytes
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class ObserverFailureV1:
    reason: ObserverReasonV1


ExecutionResultV1: TypeAlias = (
    CompletedV1
    | ExitNonZeroV1
    | SignaledV1
    | TimedOutV1
    | OomKilledV1
    | OutputLimitExceededV1
    | SandboxSetupFailedV1
    | ResidualProcessesV1
    | ObserverFailureV1
    | UnsupportedV1
)


@dataclass(frozen=True)
class _ProbeGuardV1:
    """One controller-owned lease; a backend may observe but never renew it."""

    _is_current: Callable[[], bool]

    def is_current(self) -> bool:
        try:
            return self._is_current()
        except Exception:
            return False


class ExecutionBackendV1(Protocol):
    def probe(self, guard: _ProbeGuardV1) -> CapabilityReportV1: ...

    def run(
        self,
        request: ExecutionRequestV1,
        capability: SupportedV1,
    ) -> ExecutionResultV1: ...


class ControlledExecutorV1:
    def __init__(self, backend: ExecutionBackendV1 | None = None) -> None:
        self._backend = backend if backend is not None else NativeLinuxBackendV1()
        # A fork snapshots Python locks and object identity, so an inherited
        # controller cannot share the creator process's one-shot authority.
        self._owner_pid = os.getpid()
        self._capability_lock = threading.Lock()
        self._capability_generation = 0
        self._capability_conflict_generation = 0
        self._active_capability_probes = 0
        self._issued_capability: SupportedV1 | None = None
        self._issued_backend: ExecutionBackendV1 | None = None

    def _probe_is_current_v1(
        self,
        generation: int,
        conflict_generation: int,
        backend: ExecutionBackendV1,
    ) -> bool:
        with self._capability_lock:
            return (
                os.getpid() == self._owner_pid
                and generation == self._capability_generation
                and conflict_generation == self._capability_conflict_generation
                and backend is self._backend
            )

    def probe(self) -> CapabilityReportV1:
        if os.getpid() != self._owner_pid:
            return _invalidated_capability_report_v1()
        with self._capability_lock:
            self._capability_generation += 1
            generation = self._capability_generation
            if self._active_capability_probes != 0:
                self._capability_conflict_generation += 1
                self._issued_capability = None
                self._issued_backend = None
                return _invalidated_capability_report_v1()
            conflict_generation = self._capability_conflict_generation
            self._active_capability_probes += 1
            self._issued_capability = None
            self._issued_backend = None
            backend = self._backend
        guard = _ProbeGuardV1(
            lambda: self._probe_is_current_v1(
                generation,
                conflict_generation,
                backend,
            )
        )
        try:
            report = backend.probe(guard)
        except Exception:
            report = _kernel_api_unavailable_report_v1()
        except BaseException:
            with self._capability_lock:
                self._active_capability_probes -= 1
                self._capability_conflict_generation += 1
                self._issued_capability = None
                self._issued_backend = None
            raise
        if type(report) not in (SupportedV1, UnsupportedV1):
            report = _kernel_api_unavailable_report_v1()
        elif (
            type(report) is SupportedV1
            and type(platform_identity_v1(report)) is not bytes
        ):
            report = _kernel_api_unavailable_report_v1()
        elif type(report) is UnsupportedV1:
            try:
                if not _unsupported_is_well_typed_v1(report):
                    raise TypeError("unsupported capability report did not replay")
                report = UnsupportedV1(
                    tuple(
                        CapabilityFailureV1(failure.reason, failure.errno)
                        for failure in report.failures
                    )
                )
            except Exception:
                report = _kernel_api_unavailable_report_v1()
        with self._capability_lock:
            self._active_capability_probes -= 1
            invalidated = (
                generation != self._capability_generation
                or conflict_generation != self._capability_conflict_generation
                or backend is not self._backend
            )
            if invalidated:
                self._issued_capability = None
                self._issued_backend = None
                return _invalidated_capability_report_v1()
            if type(report) is SupportedV1:
                # The backend reports host facts; it cannot mint authority.
                # A fresh controller-owned object binds this exact successful
                # probe generation, even when a backend reuses its report.
                issued = SupportedV1(
                    report.platform,
                    report.sandbox_policy_release,
                )
                self._issued_capability = issued
                self._issued_backend = backend
                return issued
        return report

    def execute(
        self,
        request: object,
        capability: SupportedV1 | None = None,
    ) -> ExecutionResultV1:
        if os.getpid() != self._owner_pid:
            return ObserverFailureV1(ObserverReasonV1.PROBE_FAILED)
        request_identity = invocation_identity_v1(request)
        if (
            type(request) is not ExecutionRequestV1
            or type(request_identity) is not bytes
        ):
            return ObserverFailureV1(ObserverReasonV1.REQUEST_NOT_ADMITTED)
        if capability is None:
            report = self.probe()
            if type(report) is UnsupportedV1:
                return report
            capability = report
        if type(capability) is not SupportedV1:
            return ObserverFailureV1(ObserverReasonV1.PROBE_FAILED)
        with self._capability_lock:
            backend = self._issued_backend
            if (
                capability is not self._issued_capability
                or backend is None
                or backend is not self._backend
            ):
                return ObserverFailureV1(ObserverReasonV1.PROBE_FAILED)
            # The controller is the sole owner. Consumption precedes every
            # backend operation, so retries and backend replacement fail shut.
            self._issued_capability = None
            self._issued_backend = None
        try:
            result = backend.run(request, capability)
        except Exception:
            return ObserverFailureV1(ObserverReasonV1.BACKEND_EXCEPTION)
        if not result_matches_request_v1(result, request):
            return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
        return result


def _unsupported_is_well_typed_v1(report: UnsupportedV1) -> bool:
    failures = report.failures
    return (
        type(failures) is tuple
        and bool(failures)
        and all(
            type(failure) is CapabilityFailureV1
            and type(failure.reason) is CapabilityReasonV1
            and (
                failure.errno is None
                or (type(failure.errno) is int and failure.errno > 0)
            )
            for failure in failures
        )
    )


def _result_matches_request_v1(result: object, request: ExecutionRequestV1) -> bool:
    if type(request) is not ExecutionRequestV1:
        return False
    known = (
        CompletedV1,
        ExitNonZeroV1,
        SignaledV1,
        TimedOutV1,
        OomKilledV1,
        OutputLimitExceededV1,
        SandboxSetupFailedV1,
        ResidualProcessesV1,
        ObserverFailureV1,
        UnsupportedV1,
    )
    if type(result) not in known:
        return False
    if type(result) is ObserverFailureV1:
        return type(result.reason) is ObserverReasonV1
    if type(result) is UnsupportedV1:
        return _unsupported_is_well_typed_v1(result)

    stdout = result.stdout
    stderr = result.stderr
    if (
        type(stdout) is not bytes
        or type(stderr) is not bytes
        or len(stdout) > request.limits.max_stdout_bytes
        or len(stderr) > request.limits.max_stderr_bytes
    ):
        return False
    expected_digest = hashlib.sha256(request.executable).digest()
    if type(result) is SandboxSetupFailedV1:
        if result.binary_sha256 is not None and (
            type(result.binary_sha256) is not bytes
            or len(result.binary_sha256) != len(expected_digest)
            or result.binary_sha256 != expected_digest
        ):
            return False
        return (
            type(result.stage) is SetupStageV1
            and type(result.errno) is int
            and result.errno > 0
        )
    if (
        type(result.binary_sha256) is not bytes
        or len(result.binary_sha256) != len(expected_digest)
        or result.binary_sha256 != expected_digest
    ):
        return False
    if type(result) is ExitNonZeroV1:
        return type(result.exit_code) is int and result.exit_code > 0
    if type(result) is SignaledV1:
        return (
            type(result.signal_number) is int
            and result.signal_number > 0
            and type(result.core_dumped) is bool
        )
    if type(result) is TimedOutV1:
        return (
            type(result.deadline_ns) is int
            and result.deadline_ns == request.limits.wall_timeout_ns
        )
    if type(result) is OomKilledV1:
        return type(result.oom_kill_delta) is int and result.oom_kill_delta > 0
    if type(result) is OutputLimitExceededV1:
        expected_limit = (
            request.limits.max_stdout_bytes
            if result.stream is OutputStreamV1.STDOUT
            else request.limits.max_stderr_bytes
            if result.stream is OutputStreamV1.STDERR
            else None
        )
        captured = (
            result.stdout
            if result.stream is OutputStreamV1.STDOUT
            else result.stderr
        )
        return (
            expected_limit is not None
            and type(result.limit) is int
            and result.limit == expected_limit
            and len(captured) == expected_limit
        )
    return True


def result_matches_request_v1(result: object, request: ExecutionRequestV1) -> bool:
    """Total validation for observations returned by an injected backend."""

    try:
        return _result_matches_request_v1(result, request)
    except Exception:
        return False


class _MemfdOperationsV1(Protocol):
    def create_executable_memfd(self) -> int: ...

    def write_all(self, fd: int, data: bytes) -> None: ...

    def make_executable(self, fd: int) -> None: ...

    def add_seals(self, fd: int, seals: int) -> None: ...

    def get_seals(self, fd: int) -> int: ...

    def pread(self, fd: int, size: int, offset: int) -> bytes: ...

    def execveat(
        self,
        fd: int,
        argv: tuple[bytes, ...],
        environment: tuple[tuple[bytes, bytes], ...],
    ) -> None: ...

    def close(self, fd: int) -> None: ...


@dataclass(frozen=True)
class _SealedExecutableV1:
    fd: int
    size: int
    sha256: bytes

    def execveat(
        self,
        argv: tuple[bytes, ...],
        environment: tuple[tuple[bytes, bytes], ...],
        operations: _MemfdOperationsV1,
    ) -> None:
        operations.execveat(self.fd, argv, environment)


def _seal_executable_v1(
    executable: bytes,
    operations: _MemfdOperationsV1,
) -> _SealedExecutableV1:
    fd = operations.create_executable_memfd()
    try:
        operations.write_all(fd, executable)
        operations.make_executable(fd)
        operations.add_seals(fd, REQUIRED_FILE_SEALS_V1)
        actual_seals = operations.get_seals(fd)
        if actual_seals & REQUIRED_FILE_SEALS_V1 != REQUIRED_FILE_SEALS_V1:
            raise OSError(errno_module.ENOTSUP, "required file seals did not stick")
        digest = hashlib.sha256()
        offset = 0
        while offset < len(executable):
            chunk = operations.pread(fd, min(1 << 20, len(executable) - offset), offset)
            if not chunk:
                raise OSError(errno_module.EIO, "sealed executable shortened while hashing")
            digest.update(chunk)
            offset += len(chunk)
        if offset != len(executable):
            raise OSError(errno_module.EIO, "sealed executable length changed")
        return _SealedExecutableV1(fd, len(executable), digest.digest())
    except BaseException:
        operations.close(fd)
        raise


@dataclass(frozen=True)
class _ChildErrorV1:
    stage: SetupStageV1
    errno: int


class ObserverProtocolErrorV1(ValueError):
    pass


def _encode_child_error_packet_v1(stage: SetupStageV1, error_number: int) -> bytes:
    if type(stage) is not SetupStageV1 or type(error_number) is not int or not 0 < error_number <= 0xFFFFFFFF:
        raise ValueError("invalid child error")
    return _CHILD_PACKET.pack(_CHILD_PACKET_MAGIC, 1, stage.value, error_number)


def _parse_child_error_packet_v1(packet: bytes) -> _ChildErrorV1:
    if type(packet) is not bytes or len(packet) != _CHILD_PACKET.size:
        raise ObserverProtocolErrorV1("noncanonical child packet length")
    try:
        magic, release, raw_stage, error_number = _CHILD_PACKET.unpack(packet)
        stage = SetupStageV1(raw_stage)
    except (struct.error, ValueError) as error:
        raise ObserverProtocolErrorV1("invalid child packet") from error
    if magic != _CHILD_PACKET_MAGIC or release != 1 or error_number == 0:
        raise ObserverProtocolErrorV1("invalid child packet")
    return _ChildErrorV1(stage, error_number)


class _SockFilter(ctypes.Structure):
    _fields_ = (
        ("code", ctypes.c_ushort),
        ("jt", ctypes.c_ubyte),
        ("jf", ctypes.c_ubyte),
        ("k", ctypes.c_uint32),
    )


class _SockFprog(ctypes.Structure):
    _fields_ = (
        ("length", ctypes.c_ushort),
        ("filters", ctypes.POINTER(_SockFilter)),
    )


class _NativeLinuxOperationsV1:
    _runtime_syscalls = (
        0,    # read: only inherited stdin remains readable
        1,    # write: only inherited stdout/stderr remain writable
        3,    # close
        5,    # fstat
        8,    # lseek
        9,    # mmap
        10,   # mprotect
        11,   # munmap
        12,   # brk
        13,   # rt_sigaction
        14,   # rt_sigprocmask
        15,   # rt_sigreturn
        25,   # mremap
        28,   # madvise
        60,   # exit
        131,  # sigaltstack
        158,  # arch_prctl
        202,  # futex
        218,  # set_tid_address
        231,  # exit_group
        273,  # set_robust_list
        334,  # rseq
    )

    def __init__(self) -> None:
        self._libc = ctypes.CDLL(None, use_errno=True)

    def create_executable_memfd(self) -> int:
        if not hasattr(os, "memfd_create"):
            raise OSError(errno_module.ENOSYS, "memfd_create unavailable")
        return os.memfd_create(
            "labcolors-proof-evaluator",
            _MFD_CLOEXEC | _MFD_ALLOW_SEALING | _MFD_EXEC,
        )

    def pipe_cloexec(self) -> tuple[int, int]:
        return os.pipe2(os.O_CLOEXEC)

    def write_all(self, fd: int, data: bytes) -> None:
        view = memoryview(data)
        offset = 0
        while offset < len(view):
            try:
                written = os.write(fd, view[offset:])
            except InterruptedError:
                continue
            if written <= 0:
                raise OSError(errno_module.EIO, "short memfd write")
            offset += written

    def make_executable(self, fd: int) -> None:
        os.fchmod(fd, 0o500)

    def add_seals(self, fd: int, seals: int) -> None:
        fcntl.fcntl(fd, _F_ADD_SEALS, seals)

    def get_seals(self, fd: int) -> int:
        return int(fcntl.fcntl(fd, _F_GET_SEALS))

    def pread(self, fd: int, size: int, offset: int) -> bytes:
        return os.pread(fd, size, offset)

    def close(self, fd: int) -> None:
        os.close(fd)

    def execveat(
        self,
        fd: int,
        argv: tuple[bytes, ...],
        environment: tuple[tuple[bytes, bytes], ...],
    ) -> None:
        argv_array = (ctypes.c_char_p * (len(argv) + 1))(*argv, None)
        environment_bytes = tuple(key + b"=" + value for key, value in environment)
        environment_array = (ctypes.c_char_p * (len(environment_bytes) + 1))(
            *environment_bytes,
            None,
        )
        ctypes.set_errno(0)
        result = self._libc.syscall(
            _SYS_EXECVEAT_X86_64,
            fd,
            ctypes.c_char_p(b""),
            argv_array,
            environment_array,
            _AT_EMPTY_PATH,
        )
        error_number = ctypes.get_errno()
        if result == -1:
            raise OSError(error_number or errno_module.EIO, "execveat failed")
        raise OSError(errno_module.EIO, "execveat unexpectedly returned")

    def probe_execveat(self) -> None:
        ctypes.set_errno(0)
        result = self._libc.syscall(
            _SYS_EXECVEAT_X86_64,
            -1,
            ctypes.c_char_p(b""),
            ctypes.c_void_p(),
            ctypes.c_void_p(),
            _AT_EMPTY_PATH,
        )
        error_number = ctypes.get_errno()
        if result != -1 or error_number != errno_module.EBADF:
            raise OSError(error_number or errno_module.ENOSYS, "execveat unavailable")

    def probe_single_threaded(self) -> None:
        try:
            task_count = len(os.listdir("/proc/self/task"))
        except OSError as error:
            raise OSError(error.errno or errno_module.EIO, "cannot inspect observer tasks") from error
        if task_count != 1:
            raise OSError(errno_module.EBUSY, "observer is not single-threaded")

    def probe_standard_fds(self) -> None:
        for descriptor in (0, 1, 2):
            os.fstat(descriptor)

    def close_range_after_setup(self) -> None:
        ctypes.set_errno(0)
        result = self._libc.syscall(
            _SYS_CLOSE_RANGE_X86_64,
            5,
            ctypes.c_uint(0xFFFFFFFF),
            0,
        )
        if result == -1:
            raise OSError(ctypes.get_errno() or errno_module.EIO, "close_range failed")

    def probe_close_range(self) -> None:
        ctypes.set_errno(0)
        result = self._libc.syscall(
            _SYS_CLOSE_RANGE_X86_64,
            ctypes.c_uint(0xFFFFFFFF),
            ctypes.c_uint(0xFFFFFFFF),
            0,
        )
        if result == -1:
            raise OSError(ctypes.get_errno() or errno_module.ENOSYS, "close_range unavailable")

    def enter_namespaces(self) -> None:
        flags = _CLONE_NEWUSER | _CLONE_NEWNET | _CLONE_NEWNS
        ctypes.set_errno(0)
        if self._libc.unshare(flags) == -1:
            raise OSError(ctypes.get_errno() or errno_module.EIO, "unshare failed")

    def make_mounts_private(self) -> None:
        ctypes.set_errno(0)
        result = self._libc.mount(
            ctypes.c_void_p(),
            ctypes.c_char_p(b"/"),
            ctypes.c_void_p(),
            ctypes.c_ulong(_MS_REC | _MS_PRIVATE),
            ctypes.c_void_p(),
        )
        if result == -1:
            raise OSError(ctypes.get_errno() or errno_module.EIO, "mount propagation failed")

    def probe_namespaces(self) -> None:
        read_fd, write_fd = os.pipe2(os.O_CLOEXEC)
        pid = os.fork()
        if pid == 0:
            os.close(read_fd)
            error_number = 0
            try:
                self.enter_namespaces()
                self.make_mounts_private()
            except OSError as error:
                error_number = error.errno or errno_module.EIO
            try:
                os.write(write_fd, error_number.to_bytes(4, "big"))
            finally:
                os._exit(0 if error_number == 0 else 1)
        os.close(write_fd)
        try:
            packet = _read_exact_fd(read_fd, 4)
        finally:
            os.close(read_fd)
        _wait_exact_child(pid)
        if len(packet) != 4:
            raise OSError(errno_module.EIO, "namespace probe lost")
        error_number = int.from_bytes(packet, "big")
        if error_number:
            raise OSError(error_number, "namespace probe failed")

    def set_no_new_privileges(self) -> None:
        ctypes.set_errno(0)
        if self._libc.prctl(_PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1:
            raise OSError(ctypes.get_errno() or errno_module.EIO, "no_new_privs failed")

    def set_not_dumpable(self) -> None:
        ctypes.set_errno(0)
        if self._libc.prctl(4, 0, 0, 0, 0) == -1:  # PR_SET_DUMPABLE
            raise OSError(ctypes.get_errno() or errno_module.EIO, "PR_SET_DUMPABLE failed")

    def install_seccomp(self, exec_fd: int, setup_error_fd: int) -> None:
        instructions = self._seccomp_program(exec_fd, setup_error_fd)
        array = (_SockFilter * len(instructions))(*instructions)
        program = _SockFprog(len(instructions), array)
        ctypes.set_errno(0)
        result = self._libc.syscall(
            _SYS_SECCOMP_X86_64,
            _SECCOMP_SET_MODE_FILTER,
            _SECCOMP_FILTER_FLAG_TSYNC,
            ctypes.byref(program),
        )
        if result == -1:
            raise OSError(ctypes.get_errno() or errno_module.EIO, "seccomp failed")

    def probe_seccomp(self) -> None:
        pid = os.fork()
        if pid == 0:
            try:
                self.set_no_new_privileges()
                self.install_seccomp(3, 4)
            except OSError as error:
                os._exit(min(error.errno or errno_module.EIO, 255))
            os._exit(0)
        status = _wait_exact_child(pid)
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
            code = os.WEXITSTATUS(status) if os.WIFEXITED(status) else errno_module.EIO
            raise OSError(code or errno_module.EIO, "seccomp probe failed")

    def _seccomp_program(self, exec_fd: int, setup_error_fd: int) -> list[_SockFilter]:
        instructions = [
            _bpf(_BPF_LD_W_ABS, 0, 0, 4),
            _bpf(_BPF_JMP_JEQ_K, 1, 0, _AUDIT_ARCH_X86_64),
            _bpf(_BPF_RET_K, 0, 0, _SECCOMP_RET_KILL_PROCESS),
            _bpf(_BPF_LD_W_ABS, 0, 0, 0),
            _bpf(_BPF_JMP_JEQ_K, 0, 9, _SYS_EXECVEAT_X86_64),
            _bpf(_BPF_LD_W_ABS, 0, 0, 16),
            _bpf(_BPF_JMP_JEQ_K, 0, 0, exec_fd),
            _bpf(_BPF_LD_W_ABS, 0, 0, 20),
            _bpf(_BPF_JMP_JEQ_K, 0, 0, 0),
            _bpf(_BPF_LD_W_ABS, 0, 0, 48),
            _bpf(_BPF_JMP_JEQ_K, 0, 0, _AT_EMPTY_PATH),
            _bpf(_BPF_LD_W_ABS, 0, 0, 52),
            _bpf(_BPF_JMP_JEQ_K, 0, 0, 0),
            _bpf(_BPF_RET_K, 0, 0, _SECCOMP_RET_ALLOW),
        ]
        restricted_start = len(instructions)
        # glibc may query or tighten this process's own limits after exec. A
        # foreign PID would instead give the evaluator authority over another
        # same-UID process, so both words of pid_t must encode the kernel's
        # canonical self selector (zero).
        instructions.extend(
            (
                _bpf(_BPF_JMP_JEQ_K, 0, 5, _SYS_PRLIMIT64_X86_64),
                _bpf(_BPF_LD_W_ABS, 0, 0, 16),
                _bpf(_BPF_JMP_JEQ_K, 0, 0, 0),
                _bpf(_BPF_LD_W_ABS, 0, 0, 20),
                _bpf(_BPF_JMP_JEQ_K, 0, 0, 0),
                _bpf(_BPF_RET_K, 0, 0, _SECCOMP_RET_ALLOW),
            )
        )
        generic_start = len(instructions)
        # setup_error_fd is CLOEXEC, so this write capability disappears at the
        # successful exec boundary together with the descriptor itself.
        generic = tuple(self._runtime_syscalls)
        for syscall_number in generic:
            instructions.extend(
                (
                    _bpf(_BPF_JMP_JEQ_K, 0, 1, syscall_number),
                    _bpf(_BPF_RET_K, 0, 0, _SECCOMP_RET_ALLOW),
                )
            )
        final_kill = len(instructions)
        instructions.append(_bpf(_BPF_RET_K, 0, 0, _SECCOMP_RET_KILL_PROCESS))
        for index in (6, 8, 10, 12, restricted_start + 2, restricted_start + 4):
            distance = final_kill - index - 1
            instructions[index].jf = distance

        # A plain write rule is safe only because close_range leaves fd 1, 2
        # and the CLOEXEC setup fd.  No executable or filesystem fd survives.
        if setup_error_fd not in (4,):
            raise OSError(errno_module.EINVAL, "noncanonical setup fd")
        if restricted_start != 14 or generic_start != 20:
            raise AssertionError("seccomp branch offset drift")
        return instructions


def _bpf(code: int, jt: int, jf: int, value: int) -> _SockFilter:
    if not 0 <= jt <= 255 or not 0 <= jf <= 255:
        raise ValueError("BPF jump exceeds classic filter encoding")
    return _SockFilter(code, jt, jf, value)


def _read_exact_fd(fd: int, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        try:
            chunk = os.read(fd, size - len(chunks))
        except InterruptedError:
            continue
        if not chunk:
            break
        chunks.extend(chunk)
    return bytes(chunks)


def _wait_exact_child(pid: int) -> int:
    while True:
        try:
            waited, status = os.waitpid(pid, 0)
        except InterruptedError:
            continue
        if waited != pid:
            raise OSError(errno_module.ECHILD, "wrong child reaped")
        return status


_CGROUP_NAMES = itertools.count()
_CGROUP_ROOT_V1 = Path("/sys/fs/cgroup")
# One observer plus one child is the whole process tree.  The kernel pids
# controller makes thread creation and fork contend for the same final slot.
_OBSERVER_SUBTREE_TASK_LIMIT_V1 = 2


def _current_unified_cgroup_v1() -> Path:
    descriptor = os.open(
        "/proc/self/cgroup",
        os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW,
    )
    try:
        raw = os.read(descriptor, 4097)
    finally:
        os.close(descriptor)
    if len(raw) > 4096 or not raw.endswith(b"\n") or raw.count(b"\n") != 1:
        raise OSError(errno_module.EPROTO, "noncanonical unified cgroup record")
    prefix = b"0::"
    if not raw.startswith(prefix):
        raise OSError(errno_module.ENOTSUP, "unified cgroup v2 is required")
    try:
        relative = raw[len(prefix) : -1].decode("ascii")
    except UnicodeDecodeError as error:
        raise OSError(errno_module.EPROTO, "non-ASCII cgroup path") from error
    if (
        not relative.startswith("/")
        or relative != posixpath.normpath(relative)
        or any(part in ("", ".", "..") for part in relative[1:].split("/"))
    ):
        raise OSError(errno_module.EPROTO, "noncanonical cgroup path")
    return _CGROUP_ROOT_V1 / relative[1:]


class _CgroupV2V1:
    def __init__(self, parent_fd: int, directory_fd: int, name: bytes) -> None:
        self._parent_fd = parent_fd
        self._directory_fd = directory_fd
        self._name = name

    @classmethod
    def probe_observer_task_budget(cls, parent: Path) -> None:
        parent_fd = os.open(
            os.fsencode(parent),
            os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
        )
        current_fd = -1
        current_parent_fd = -1
        try:
            current = _current_unified_cgroup_v1()
            current_fd = os.open(
                os.fsencode(current),
                os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
            )
            current_parent_fd = os.open(
                os.fsencode(current.parent),
                os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
            )
            parent_stat = os.fstat(parent_fd)
            current_parent_stat = os.fstat(current_parent_fd)
            if (
                parent_stat.st_dev != current_parent_stat.st_dev
                or parent_stat.st_ino != current_parent_stat.st_ino
            ):
                raise OSError(
                    errno_module.EXDEV,
                    "observer must be in a direct child of the delegated parent",
                )
            expected_limit = f"{_OBSERVER_SUBTREE_TASK_LIMIT_V1}\n".encode("ascii")
            if (
                _read_cgroup_file(parent_fd, b"pids.max") != expected_limit
                or _read_cgroup_file(parent_fd, b"pids.current") != b"1\n"
                or _read_cgroup_file(current_fd, b"pids.current") != b"1\n"
            ):
                raise OSError(
                    errno_module.EBUSY,
                    "observer subtree must contain exactly one of two permitted tasks",
                )
        finally:
            if current_parent_fd >= 0:
                os.close(current_parent_fd)
            if current_fd >= 0:
                os.close(current_fd)
            os.close(parent_fd)

    @classmethod
    def create(
        cls,
        parent: Path,
        *,
        memory_max: int | None,
        pids_max: int,
    ) -> "_CgroupV2V1":
        parent_fd = os.open(
            os.fsencode(parent),
            os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
        )
        name = f"labcolors-executor-{os.getpid()}-{next(_CGROUP_NAMES)}".encode("ascii")
        directory_fd = -1
        try:
            controllers = set(_read_cgroup_file(parent_fd, b"cgroup.controllers").split())
            subtree = set(_read_cgroup_file(parent_fd, b"cgroup.subtree_control").split())
            if not {b"memory", b"pids"} <= controllers or not {b"memory", b"pids"} <= subtree:
                raise OSError(errno_module.ENOTSUP, "memory/pids controllers are not delegated")
            os.mkdir(name, mode=0o700, dir_fd=parent_fd)
            directory_fd = os.open(
                name,
                os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
                dir_fd=parent_fd,
            )
            group = cls(parent_fd, directory_fd, name)
            group._write(b"memory.max", b"max" if memory_max is None else str(memory_max).encode("ascii"))
            group._write(b"memory.swap.max", b"0")
            group._write(b"memory.oom.group", b"1")
            group._write(b"pids.max", str(pids_max).encode("ascii"))
            group._require_applied_limits(
                memory_max=memory_max,
                pids_max=pids_max,
            )
            group._require_writable(b"cgroup.kill")
            group.oom_kill_count()
            group.populated()
            return group
        except BaseException:
            if directory_fd >= 0:
                os.close(directory_fd)
            try:
                os.rmdir(name, dir_fd=parent_fd)
            except OSError:
                pass
            os.close(parent_fd)
            raise

    @classmethod
    def probe(cls, parent: Path) -> None:
        group = cls.create(parent, memory_max=None, pids_max=1)
        group.close()

    def attach(self, pid: int) -> None:
        self._write(b"cgroup.procs", str(pid).encode("ascii"))

    def kill_all(self) -> None:
        self._write(b"cgroup.kill", b"1")

    def oom_kill_count(self) -> int:
        values = _parse_cgroup_kv(self._read_required(b"memory.events.local"))
        try:
            return values[b"oom_kill"]
        except KeyError as error:
            raise OSError(errno_module.EPROTO, "oom_kill counter missing") from error

    def populated(self) -> bool:
        values = _parse_cgroup_kv(self._read_required(b"cgroup.events"))
        value = values.get(b"populated")
        if value not in (0, 1):
            raise OSError(errno_module.EPROTO, "invalid populated counter")
        return bool(value)

    def close(self) -> None:
        directory_fd, parent_fd = self._directory_fd, self._parent_fd
        self._directory_fd = -1
        self._parent_fd = -1
        try:
            os.close(directory_fd)
            os.rmdir(self._name, dir_fd=parent_fd)
        finally:
            os.close(parent_fd)

    def _write(self, name: bytes, value: bytes) -> None:
        fd = os.open(name, os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=self._directory_fd)
        try:
            written = os.write(fd, value)
            if written != len(value):
                raise OSError(errno_module.EIO, "short cgroup write")
        finally:
            os.close(fd)

    def _read_required(self, name: bytes) -> bytes:
        return _read_cgroup_file(self._directory_fd, name)

    def _require_writable(self, name: bytes) -> None:
        fd = os.open(
            name,
            os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW,
            dir_fd=self._directory_fd,
        )
        os.close(fd)

    def _require_applied_limits(
        self,
        *,
        memory_max: int | None,
        pids_max: int,
    ) -> None:
        expected = {
            b"memory.max": b"max" if memory_max is None else str(memory_max).encode("ascii"),
            b"memory.swap.max": b"0",
            b"memory.oom.group": b"1",
            b"pids.max": str(pids_max).encode("ascii"),
        }
        for name, value in expected.items():
            if self._read_required(name) != value + b"\n":
                raise OSError(errno_module.EPROTO, f"cgroup rejected exact {name!r}")


def _read_cgroup_file(directory_fd: int, name: bytes) -> bytes:
    fd = os.open(name, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=directory_fd)
    try:
        chunks = bytearray()
        while True:
            chunk = os.read(fd, 4096)
            if not chunk:
                return bytes(chunks)
            chunks.extend(chunk)
            if len(chunks) > 65536:
                raise OSError(errno_module.EOVERFLOW, "cgroup control file too large")
    finally:
        os.close(fd)


def _parse_cgroup_kv(data: bytes) -> dict[bytes, int]:
    result: dict[bytes, int] = {}
    for line in data.splitlines():
        parts = line.split(b" ")
        if len(parts) != 2 or not parts[0] or not parts[1].isdigit() or parts[0] in result:
            raise OSError(errno_module.EPROTO, "invalid cgroup counter file")
        result[parts[0]] = int(parts[1])
    if not result:
        raise OSError(errno_module.EPROTO, "empty cgroup counter file")
    return result


def _append_bounded_v1(captured: bytearray, chunk: bytes, limit: int) -> bool:
    if (
        type(captured) is not bytearray
        or type(chunk) is not bytes
        or type(limit) is not int
        or limit < 0
        or len(captured) > limit
    ):
        raise ValueError("invalid bounded capture state")
    remaining = limit - len(captured)
    captured.extend(chunk[:remaining])
    return len(chunk) > remaining


def _classify_process_v1(
    *,
    digest: bytes,
    stdout: bytes,
    stderr: bytes,
    child_status: int | None,
    oom_kill_delta: int,
    residual: bool,
    setup_packet: bytes,
    terminal: tuple[str, OutputStreamV1 | None] | None,
    limits: ExecutionLimitsV1,
) -> ExecutionResultV1:
    if (
        type(digest) is not bytes
        or len(digest) != 32
        or type(stdout) is not bytes
        or type(stderr) is not bytes
        or len(stdout) > limits.max_stdout_bytes
        or len(stderr) > limits.max_stderr_bytes
        or type(oom_kill_delta) is not int
        or oom_kill_delta < 0
        or type(residual) is not bool
        or type(setup_packet) is not bytes
    ):
        return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
    if terminal is not None:
        if terminal == ("timeout", None):
            return TimedOutV1(digest, stdout, stderr, limits.wall_timeout_ns)
        kind, stream = terminal
        if kind != "output" or type(stream) is not OutputStreamV1:
            return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
        limit = (
            limits.max_stdout_bytes
            if stream is OutputStreamV1.STDOUT
            else limits.max_stderr_bytes
        )
        captured = stdout if stream is OutputStreamV1.STDOUT else stderr
        if len(captured) != limit:
            return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
        return OutputLimitExceededV1(digest, stdout, stderr, stream, limit)
    if setup_packet:
        try:
            child_error = _parse_child_error_packet_v1(setup_packet)
        except ObserverProtocolErrorV1:
            return ObserverFailureV1(ObserverReasonV1.CHILD_PROTOCOL)
        return SandboxSetupFailedV1(
            digest,
            stdout,
            stderr,
            child_error.stage,
            child_error.errno,
        )
    if residual:
        return ResidualProcessesV1(digest, stdout, stderr)
    if oom_kill_delta > 0:
        return OomKilledV1(digest, stdout, stderr, oom_kill_delta)
    if child_status is None:
        return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
    if os.WIFSIGNALED(child_status):
        core_dumped = bool(os.WCOREDUMP(child_status)) if hasattr(os, "WCOREDUMP") else False
        return SignaledV1(
            digest,
            stdout,
            stderr,
            os.WTERMSIG(child_status),
            core_dumped,
        )
    if not os.WIFEXITED(child_status):
        return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
    exit_code = os.WEXITSTATUS(child_status)
    if exit_code:
        return ExitNonZeroV1(digest, stdout, stderr, exit_code)
    return CompletedV1(digest, stdout, stderr)


class NativeLinuxBackendV1:
    """Native backend for a dedicated, single-threaded Linux helper process.

    Correctness requires a dedicated helper whose delegated cgroup permits
    exactly the observer and one controlled child across the whole subtree.
    The kernel pids controller then arbitrates thread creation against fork,
    eliminating the observation-to-fork race rather than timing around it.
    Native threads created outside CPython and instruction-level inputs such as
    CPUID/RDTSC or auxv remain outside this observation boundary, so this result
    alone cannot establish ambient-free reproducibility.
    """

    def __init__(
        self,
        cgroup_parent: str | os.PathLike[str] | None = None,
        *,
        platform_name: str | None = None,
        machine_name: str | None = None,
        operations: _NativeLinuxOperationsV1 | None = None,
        cgroup_factory: object = _CgroupV2V1,
        monotonic_ns: object = time.monotonic_ns,
    ) -> None:
        self._cgroup_parent = None if cgroup_parent is None else Path(cgroup_parent)
        self._platform_name = sys.platform if platform_name is None else platform_name
        self._machine_name = platform.machine() if machine_name is None else machine_name
        self._operations = operations
        self._cgroup_factory = cgroup_factory
        self._monotonic_ns = monotonic_ns

    def probe(self, guard: _ProbeGuardV1) -> CapabilityReportV1:
        if type(guard) is not _ProbeGuardV1 or not guard.is_current():
            return _invalidated_capability_report_v1()
        try:
            return self._probe_capability_v1(guard)
        except Exception:
            return _kernel_api_unavailable_report_v1()

    def _probe_capability_v1(self, guard: _ProbeGuardV1) -> CapabilityReportV1:
        if self._platform_name != "linux":
            return UnsupportedV1(
                (CapabilityFailureV1(CapabilityReasonV1.HOST_NOT_LINUX, None),)
            )
        if self._machine_name.lower() not in ("x86_64", "amd64"):
            return UnsupportedV1(
                (
                    CapabilityFailureV1(
                        CapabilityReasonV1.ARCHITECTURE_NOT_SUPPORTED,
                        None,
                    ),
                )
            )
        if self._cgroup_parent is None:
            return UnsupportedV1(
                (
                    CapabilityFailureV1(
                        CapabilityReasonV1.CGROUP_PARENT_NOT_DECLARED,
                        None,
                    ),
                )
            )
        if not self._cgroup_parent.is_absolute():
            return UnsupportedV1(
                (
                    CapabilityFailureV1(
                        CapabilityReasonV1.CGROUP_V2_UNAVAILABLE,
                        errno_module.EINVAL,
                    ),
                )
            )
        operations = self._operations
        if operations is None:
            if sys.platform != "linux":
                return _kernel_api_unavailable_report_v1()
            operations = _NativeLinuxOperationsV1()
            self._operations = operations

        failures: list[CapabilityFailureV1] = []
        _probe_operation(
            operations.probe_standard_fds,
            CapabilityReasonV1.STANDARD_FDS_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_single_threaded,
            CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            lambda: self._cgroup_factory.probe_observer_task_budget(
                self._cgroup_parent
            ),
            CapabilityReasonV1.OBSERVER_TASK_BUDGET_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        self._probe_sealed_memfd(operations, failures)
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_execveat,
            CapabilityReasonV1.EXECVEAT_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_close_range,
            CapabilityReasonV1.CLOSE_RANGE_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_single_threaded,
            CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_namespaces,
            CapabilityReasonV1.NETWORK_NAMESPACE_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_single_threaded,
            CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            operations.probe_seccomp,
            CapabilityReasonV1.SECCOMP_FILTER_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        _probe_operation(
            lambda: self._cgroup_factory.probe(self._cgroup_parent),
            CapabilityReasonV1.CGROUP_V2_UNAVAILABLE,
            failures,
        )
        if failures or not guard.is_current():
            if not failures:
                return _invalidated_capability_report_v1()
            return UnsupportedV1(tuple(failures))
        return SupportedV1(EXECUTION_PLATFORM_V1, SANDBOX_POLICY_RELEASE_V1)

    def _probe_sealed_memfd(
        self,
        operations: _NativeLinuxOperationsV1,
        failures: list[CapabilityFailureV1],
    ) -> None:
        try:
            fd = operations.create_executable_memfd()
        except OSError as error:
            failures.append(
                CapabilityFailureV1(
                    CapabilityReasonV1.EXECUTABLE_MEMFD_UNAVAILABLE,
                    error.errno or None,
                )
            )
            return
        try:
            operations.write_all(fd, b"probe")
            operations.make_executable(fd)
            operations.add_seals(fd, REQUIRED_FILE_SEALS_V1)
            if operations.get_seals(fd) & REQUIRED_FILE_SEALS_V1 != REQUIRED_FILE_SEALS_V1:
                raise OSError(errno_module.ENOTSUP, "required file seals missing")
        except OSError as error:
            failures.append(
                CapabilityFailureV1(
                    CapabilityReasonV1.FILE_SEALS_UNAVAILABLE,
                    error.errno or None,
                )
            )
        finally:
            operations.close(fd)

    def run(
        self,
        request: ExecutionRequestV1,
        capability: SupportedV1,
    ) -> ExecutionResultV1:
        operations = self._operations
        if operations is None or self._cgroup_parent is None:
            return ObserverFailureV1(ObserverReasonV1.PROBE_FAILED)

        try:
            sealed = _seal_executable_v1(request.executable, operations)
        except OSError as error:
            return SandboxSetupFailedV1(
                None,
                b"",
                b"",
                SetupStageV1.SEALED_EXECUTABLE,
                error.errno or errno_module.EIO,
            )
        try:
            return self._run_sealed(request, sealed, operations)
        finally:
            operations.close(sealed.fd)

    def _run_sealed(
        self,
        request: ExecutionRequestV1,
        sealed: _SealedExecutableV1,
        operations: _NativeLinuxOperationsV1,
    ) -> ExecutionResultV1:
        try:
            cwd_fd = os.open(
                request.cwd,
                os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
            )
        except OSError as error:
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.CWD,
                error.errno or errno_module.EIO,
            )
        try:
            group = self._cgroup_factory.create(
                self._cgroup_parent,
                memory_max=request.limits.memory_max_bytes,
                pids_max=request.limits.pids_max,
            )
        except OSError as error:
            os.close(cwd_fd)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.CGROUP_CREATE,
                error.errno or errno_module.EIO,
            )
        try:
            try:
                result = self._fork_and_observe(request, sealed, operations, cwd_fd, group)
            except Exception:
                result = ObserverFailureV1(ObserverReasonV1.BACKEND_EXCEPTION)
        finally:
            os.close(cwd_fd)
        cleanup_failed = False
        try:
            if group.populated():
                group.kill_all()
                cleanup_deadline = self._clock() + 1_000_000_000
                while group.populated() and self._clock() < cleanup_deadline:
                    time.sleep(0.001)
                if group.populated():
                    cleanup_failed = True
        except OSError:
            cleanup_failed = True
        try:
            group.close()
        except OSError:
            cleanup_failed = True
        if cleanup_failed:
            return ObserverFailureV1(ObserverReasonV1.CLEANUP_FAILED)
        return result

    def _fork_and_observe(
        self,
        request: ExecutionRequestV1,
        sealed: _SealedExecutableV1,
        operations: _NativeLinuxOperationsV1,
        cwd_fd: int,
        group: _CgroupV2V1,
    ) -> ExecutionResultV1:
        all_fds: list[int] = []
        try:
            for _ in range(5):
                all_fds.extend(operations.pipe_cloexec())
        except OSError as error:
            _close_many(all_fds)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.FILE_DESCRIPTORS,
                error.errno or errno_module.EIO,
            )
        (
            stdin_read,
            stdin_write,
            stdout_read,
            stdout_write,
            stderr_read,
            stderr_write,
            setup_read,
            setup_write,
            start_read,
            start_write,
        ) = all_fds
        try:
            # The task count is observational; the delegated pids.max=2
            # subtree is the atomic law.  Once the observer occupies one slot,
            # either a new thread or the controlled child can claim the other,
            # never both.
            operations.probe_single_threaded()
            if self._cgroup_parent is None:
                raise OSError(errno_module.EINVAL, "missing cgroup parent")
            self._cgroup_factory.probe_observer_task_budget(
                self._cgroup_parent
            )
        except OSError as error:
            _close_many(all_fds)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.OBSERVER_PRECONDITION,
                error.errno or errno_module.EIO,
            )
        try:
            pid = os.fork()
        except OSError as error:
            _close_many(all_fds)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.CGROUP_ATTACH,
                error.errno or errno_module.EIO,
            )
        if pid == 0:
            self._child(
                request,
                sealed,
                operations,
                cwd_fd,
                stdin_read,
                stdout_write,
                stderr_write,
                setup_write,
                start_read,
            )
            os._exit(127)

        _close_many((stdin_read, stdout_write, stderr_write, setup_write, start_read))
        try:
            baseline_oom = group.oom_kill_count()
            group.attach(pid)
        except OSError as error:
            _close_many((stdin_write, stdout_read, stderr_read, setup_read, start_write))
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            _wait_exact_child(pid)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.CGROUP_ATTACH,
                error.errno or errno_module.EIO,
            )
        try:
            os.write(start_write, b"1")
        except OSError as error:
            try:
                group.kill_all()
            finally:
                _close_many((stdin_write, stdout_read, stderr_read, setup_read, start_write))
                _wait_exact_child(pid)
            return SandboxSetupFailedV1(
                sealed.sha256,
                b"",
                b"",
                SetupStageV1.CGROUP_ATTACH,
                error.errno or errno_module.EIO,
            )
        os.close(start_write)
        return self._observe(
            request,
            sealed.sha256,
            pid,
            group,
            baseline_oom,
            stdin_write,
            stdout_read,
            stderr_read,
            setup_read,
        )

    def _child(
        self,
        request: ExecutionRequestV1,
        sealed: _SealedExecutableV1,
        operations: _NativeLinuxOperationsV1,
        cwd_fd: int,
        stdin_read: int,
        stdout_write: int,
        stderr_write: int,
        setup_write: int,
        start_read: int,
    ) -> None:
        try:
            if _read_exact_fd(start_read, 1) != b"1":
                _child_fail(setup_write, SetupStageV1.CGROUP_ATTACH, errno_module.EPIPE)
            os.fchdir(cwd_fd)
            os.umask(request.umask)
        except OSError as error:
            _child_fail(setup_write, SetupStageV1.CWD, error.errno or errno_module.EIO)
        try:
            operations.enter_namespaces()
        except OSError as error:
            _child_fail(setup_write, SetupStageV1.NAMESPACE, error.errno or errno_module.EIO)
        try:
            operations.make_mounts_private()
        except OSError as error:
            _child_fail(
                setup_write,
                SetupStageV1.MOUNT_PROPAGATION,
                error.errno or errno_module.EIO,
            )
        try:
            protected = tuple(
                fcntl.fcntl(fd, fcntl.F_DUPFD_CLOEXEC, 10)
                for fd in (sealed.fd, setup_write)
            )
            os.dup2(stdin_read, 0)
            os.dup2(stdout_write, 1)
            os.dup2(stderr_write, 2)
            os.dup2(protected[0], 3, inheritable=False)
            os.dup2(protected[1], 4, inheritable=False)
            operations.close_range_after_setup()
        except OSError as error:
            _child_fail(setup_write, SetupStageV1.FILE_DESCRIPTORS, error.errno or errno_module.EIO)
        try:
            _reset_signal_state()
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
            operations.set_not_dumpable()
        except OSError as error:
            _child_fail(4, SetupStageV1.SIGNAL_STATE, error.errno or errno_module.EIO)
        except (ValueError, RuntimeError):
            _child_fail(4, SetupStageV1.SIGNAL_STATE, errno_module.EINVAL)
        try:
            operations.set_no_new_privileges()
        except OSError as error:
            _child_fail(4, SetupStageV1.NO_NEW_PRIVILEGES, error.errno or errno_module.EIO)
        try:
            operations.install_seccomp(3, 4)
        except OSError as error:
            _child_fail(4, SetupStageV1.SECCOMP, error.errno or errno_module.EIO)
        try:
            _SealedExecutableV1(3, sealed.size, sealed.sha256).execveat(
                request.argv,
                request.environment,
                operations,
            )
        except OSError as error:
            _child_fail(4, SetupStageV1.EXECVEAT, error.errno or errno_module.EIO)

    def _observe(
        self,
        request: ExecutionRequestV1,
        digest: bytes,
        pid: int,
        group: _CgroupV2V1,
        baseline_oom: int,
        stdin_fd: int,
        stdout_fd: int,
        stderr_fd: int,
        setup_fd: int,
    ) -> ExecutionResultV1:
        streams = {
            "stdout": bytearray(),
            "stderr": bytearray(),
            "setup": bytearray(),
        }
        limits = {
            "stdout": request.limits.max_stdout_bytes,
            "stderr": request.limits.max_stderr_bytes,
            "setup": _CHILD_PACKET.size,
        }
        fd_by_tag = {"stdin": stdin_fd, "stdout": stdout_fd, "stderr": stderr_fd, "setup": setup_fd}
        input_offset = 0
        selector: selectors.BaseSelector | None = None
        child_status: int | None = None
        terminal: tuple[str, OutputStreamV1 | None] | None = None
        observer_failure: ObserverReasonV1 | None = None
        killed = False

        try:
            selector = selectors.DefaultSelector()
            for fd in fd_by_tag.values():
                os.set_blocking(fd, False)
            selector.register(stdout_fd, selectors.EVENT_READ, "stdout")
            selector.register(stderr_fd, selectors.EVENT_READ, "stderr")
            selector.register(setup_fd, selectors.EVENT_READ, "setup")
            if request.stdin:
                selector.register(stdin_fd, selectors.EVENT_WRITE, "stdin")
            else:
                os.close(stdin_fd)
                fd_by_tag["stdin"] = -1
            deadline_ns = self._clock() + request.limits.wall_timeout_ns

            while child_status is None or any(fd_by_tag[tag] >= 0 for tag in ("stdout", "stderr", "setup")):
                if child_status is None:
                    waited, status = os.waitpid(pid, os.WNOHANG)
                    if waited == pid:
                        child_status = status
                        if fd_by_tag["stdin"] >= 0:
                            _selector_close(selector, fd_by_tag, "stdin")
                now = self._clock()
                if child_status is None and terminal is None and now >= deadline_ns:
                    terminal = ("timeout", None)
                    try:
                        group.kill_all()
                        killed = True
                    except OSError:
                        observer_failure = ObserverReasonV1.CGROUP_OBSERVATION
                        try:
                            os.kill(pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                wait_seconds = max(0.0, min((deadline_ns - now) / 1_000_000_000, 0.05))
                events = sorted(selector.select(wait_seconds), key=lambda item: str(item[0].data))
                for key, _mask in events:
                    tag = key.data
                    if tag == "stdin":
                        try:
                            written = os.write(stdin_fd, request.stdin[input_offset:])
                        except BlockingIOError:
                            continue
                        except BrokenPipeError:
                            _selector_close(selector, fd_by_tag, "stdin")
                            continue
                        input_offset += written
                        if input_offset == len(request.stdin):
                            _selector_close(selector, fd_by_tag, "stdin")
                        continue

                    limit = limits[tag]
                    remaining = max(0, limit - len(streams[tag]))
                    try:
                        chunk = os.read(key.fd, min(65536, remaining + 1))
                    except BlockingIOError:
                        continue
                    if not chunk:
                        _selector_close(selector, fd_by_tag, tag)
                        continue
                    exceeded = _append_bounded_v1(streams[tag], chunk, limit)
                    if exceeded:
                        if tag == "setup":
                            observer_failure = ObserverReasonV1.CHILD_PROTOCOL
                        elif terminal is None:
                            terminal = (
                                "output",
                                OutputStreamV1.STDOUT if tag == "stdout" else OutputStreamV1.STDERR,
                            )
                        if not killed:
                            try:
                                group.kill_all()
                                killed = True
                            except OSError:
                                observer_failure = ObserverReasonV1.CGROUP_OBSERVATION
                                try:
                                    os.kill(pid, signal.SIGKILL)
                                except ProcessLookupError:
                                    pass
                if observer_failure is not None and child_status is None and not killed:
                    try:
                        group.kill_all()
                        killed = True
                    except OSError:
                        try:
                            os.kill(pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                if child_status is not None and not events:
                    for tag in ("stdout", "stderr", "setup"):
                        if fd_by_tag[tag] >= 0:
                            try:
                                chunk = os.read(fd_by_tag[tag], 1)
                            except BlockingIOError:
                                continue
                            if not chunk:
                                _selector_close(selector, fd_by_tag, tag)
                            else:
                                exceeded = _append_bounded_v1(
                                    streams[tag],
                                    chunk,
                                    limits[tag],
                                )
                                if exceeded and tag == "setup":
                                    observer_failure = ObserverReasonV1.CHILD_PROTOCOL
                                elif exceeded and terminal is None:
                                    terminal = (
                                        "output",
                                        OutputStreamV1.STDOUT if tag == "stdout" else OutputStreamV1.STDERR,
                                    )
        except Exception:
            observer_failure = ObserverReasonV1.BACKEND_EXCEPTION
            if not killed:
                try:
                    group.kill_all()
                    killed = True
                except OSError:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
        finally:
            if selector is not None:
                selector.close()
            _close_many(fd for fd in fd_by_tag.values() if fd >= 0)
            if child_status is None:
                try:
                    waited, status = os.waitpid(pid, os.WNOHANG)
                except ChildProcessError:
                    waited = pid
                    status = 0
                if waited == pid:
                    child_status = status
            if child_status is None:
                try:
                    group.kill_all()
                except OSError:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                child_status = _wait_exact_child(pid)

        stdout = bytes(streams["stdout"])
        stderr = bytes(streams["stderr"])
        if observer_failure is not None:
            return ObserverFailureV1(observer_failure)
        try:
            oom_delta = group.oom_kill_count() - baseline_oom
            residual = group.populated()
        except OSError:
            return ObserverFailureV1(ObserverReasonV1.CGROUP_OBSERVATION)
        if residual:
            try:
                group.kill_all()
            except OSError:
                return ObserverFailureV1(ObserverReasonV1.CGROUP_OBSERVATION)
        return _classify_process_v1(
            digest=digest,
            stdout=stdout,
            stderr=stderr,
            child_status=child_status,
            oom_kill_delta=oom_delta,
            residual=residual,
            setup_packet=bytes(streams["setup"]),
            terminal=terminal,
            limits=request.limits,
        )

    def _clock(self) -> int:
        value = self._monotonic_ns()
        if type(value) is not int or value < 0:
            raise OSError(errno_module.EIO, "invalid monotonic clock")
        return value


def _probe_operation(
    operation: object,
    reason: CapabilityReasonV1,
    failures: list[CapabilityFailureV1],
) -> None:
    try:
        operation()
    except OSError as error:
        failures.append(CapabilityFailureV1(reason, error.errno or None))
    except Exception:
        failures.append(CapabilityFailureV1(reason, None))


def _child_fail(fd: int, stage: SetupStageV1, error_number: int) -> None:
    packet = _encode_child_error_packet_v1(stage, error_number)
    try:
        offset = 0
        while offset < len(packet):
            try:
                written = os.write(fd, packet[offset:])
            except InterruptedError:
                continue
            if written <= 0:
                break
            offset += written
    finally:
        os._exit(127)


def _reset_signal_state() -> None:
    for number in signal.valid_signals():
        if number in (signal.SIGKILL, signal.SIGSTOP):
            continue
        signal.signal(number, signal.SIG_DFL)
    signal.pthread_sigmask(signal.SIG_SETMASK, set())


def _selector_close(
    selector: selectors.BaseSelector,
    fd_by_tag: dict[str, int],
    tag: str,
) -> None:
    fd = fd_by_tag[tag]
    if fd < 0:
        return
    try:
        selector.unregister(fd)
    except KeyError:
        pass
    os.close(fd)
    fd_by_tag[tag] = -1


def _close_many(fds: object) -> None:
    for fd in tuple(fds):
        try:
            os.close(fd)
        except OSError:
            pass
