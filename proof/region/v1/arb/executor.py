#!/usr/bin/env python3
"""Fail-closed Linux process boundary for the Arb evaluator.

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
import struct
import sys
import time
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Protocol, TypeAlias


SANDBOX_POLICY_RELEASE_V1 = "labcolors.arb.executor.linux-x86_64.v1"

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


@dataclass(frozen=True)
class SupportedV1:
    platform: str
    sandbox_policy_release: str

    def __post_init__(self) -> None:
        if type(self.platform) is not str or not self.platform:
            raise TypeError("platform must be a nonempty str")
        if self.sandbox_policy_release != SANDBOX_POLICY_RELEASE_V1:
            raise TypeError("unknown sandbox policy release")


CapabilityReportV1: TypeAlias = SupportedV1 | UnsupportedV1


@dataclass(frozen=True)
class ExecutionLimitsV1:
    max_executable_bytes: int
    max_stdin_bytes: int
    max_argument_bytes: int
    max_stdout_bytes: int
    max_stderr_bytes: int
    wall_timeout_ns: int
    memory_max_bytes: int
    pids_max: int

    def __post_init__(self) -> None:
        positive = (
            "max_executable_bytes",
            "max_stdin_bytes",
            "max_argument_bytes",
            "wall_timeout_ns",
            "memory_max_bytes",
            "pids_max",
        )
        nonnegative = ("max_stdout_bytes", "max_stderr_bytes")
        for field_name in positive:
            value = getattr(self, field_name)
            if type(value) is not int or value <= 0:
                raise ExecutionRequestErrorV1(RequestReasonV1.INVALID_LIMIT, field_name)
        for field_name in nonnegative:
            value = getattr(self, field_name)
            if type(value) is not int or value < 0:
                raise ExecutionRequestErrorV1(RequestReasonV1.INVALID_LIMIT, field_name)
        # V1's syscall policy denies clone/fork/vfork; a larger cgroup task
        # budget would advertise a concurrency capability the executor lacks.
        if self.pids_max != 1:
            raise ExecutionRequestErrorV1(RequestReasonV1.INVALID_LIMIT, "pids_max")


@dataclass(frozen=True)
class ExecutionRequestV1:
    executable: bytes
    argv: tuple[bytes, ...]
    environment: tuple[tuple[bytes, bytes], ...]
    cwd: bytes
    stdin: bytes
    umask: int
    limits: ExecutionLimitsV1

    def __post_init__(self) -> None:
        if type(self.limits) is not ExecutionLimitsV1:
            _request_fail(RequestReasonV1.WRONG_TYPE, "limits")
        if type(self.executable) is not bytes:
            _request_fail(RequestReasonV1.WRONG_TYPE, "executable")
        if not self.executable or len(self.executable) > self.limits.max_executable_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "executable")
        _require_static_x86_64_elf(self.executable)

        if type(self.argv) is not tuple or not self.argv:
            _request_fail(RequestReasonV1.WRONG_TYPE, "argv")
        for index, item in enumerate(self.argv):
            _require_bytes_without_nul(item, f"argv[{index}]")
        if not self.argv[0]:
            _request_fail(RequestReasonV1.EMPTY_ARGV_ZERO, "argv[0]")

        if type(self.environment) is not tuple:
            _request_fail(RequestReasonV1.WRONG_TYPE, "environment")
        previous: bytes | None = None
        argument_bytes = sum(len(item) + 1 for item in self.argv)
        for index, item in enumerate(self.environment):
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
        if argument_bytes > self.limits.max_argument_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "argv+environment")

        _require_bytes_without_nul(self.cwd, "cwd")
        if not self.cwd.startswith(b"/"):
            _request_fail(RequestReasonV1.RELATIVE_CWD, "cwd")
        if (
            posixpath.normpath(self.cwd) != self.cwd
            or self.cwd.startswith(b"//")
            or (self.cwd != b"/" and self.cwd.endswith(b"/"))
        ):
            _request_fail(RequestReasonV1.NONCANONICAL_CWD, "cwd")

        if type(self.stdin) is not bytes:
            _request_fail(RequestReasonV1.WRONG_TYPE, "stdin")
        if len(self.stdin) > self.limits.max_stdin_bytes:
            _request_fail(RequestReasonV1.LIMIT_EXCEEDED, "stdin")
        if type(self.umask) is not int or not 0 <= self.umask <= 0o777:
            _request_fail(RequestReasonV1.INVALID_LIMIT, "umask")


def _request_fail(reason: RequestReasonV1, field: str) -> None:
    raise ExecutionRequestErrorV1(reason, field)


def _require_bytes_without_nul(value: object, field: str) -> None:
    if type(value) is not bytes:
        _request_fail(RequestReasonV1.WRONG_TYPE, field)
    if b"\0" in value:
        _request_fail(RequestReasonV1.NUL_BYTE, field)


def _require_static_x86_64_elf(data: bytes) -> None:
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


class ExecutionBackendV1(Protocol):
    def probe(self) -> CapabilityReportV1: ...

    def run(self, request: ExecutionRequestV1) -> ExecutionResultV1: ...


class ControlledExecutorV1:
    def __init__(self, backend: ExecutionBackendV1 | None = None) -> None:
        self._backend = backend if backend is not None else NativeLinuxBackendV1()

    def probe(self) -> CapabilityReportV1:
        try:
            report = self._backend.probe()
        except Exception:
            return UnsupportedV1(
                (
                    CapabilityFailureV1(
                        CapabilityReasonV1.KERNEL_API_UNAVAILABLE,
                        None,
                    ),
                )
            )
        if type(report) not in (SupportedV1, UnsupportedV1):
            return UnsupportedV1(
                (
                    CapabilityFailureV1(
                        CapabilityReasonV1.KERNEL_API_UNAVAILABLE,
                        None,
                    ),
                )
            )
        return report

    def execute(self, request: ExecutionRequestV1) -> ExecutionResultV1:
        if type(request) is not ExecutionRequestV1:
            raise ExecutionRequestErrorV1(RequestReasonV1.WRONG_TYPE, "request")
        report = self.probe()
        if type(report) is UnsupportedV1:
            return report
        try:
            result = self._backend.run(request)
        except Exception:
            return ObserverFailureV1(ObserverReasonV1.BACKEND_EXCEPTION)
        if not _result_matches_request(result, request):
            return ObserverFailureV1(ObserverReasonV1.BACKEND_CONTRACT)
        return result


def _result_matches_request(result: object, request: ExecutionRequestV1) -> bool:
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
    if type(result) in (ObserverFailureV1, UnsupportedV1):
        return True

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
        if result.binary_sha256 is not None and result.binary_sha256 != expected_digest:
            return False
        return (
            type(result.stage) is SetupStageV1
            and type(result.errno) is int
            and result.errno > 0
        )
    if result.binary_sha256 != expected_digest:
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
        return result.deadline_ns == request.limits.wall_timeout_ns
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
        captured = result.stdout if result.stream is OutputStreamV1.STDOUT else result.stderr
        return expected_limit is not None and result.limit == expected_limit and len(captured) == expected_limit
    return True


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
        302,  # prlimit64
        334,  # rseq
    )

    def __init__(self) -> None:
        self._libc = ctypes.CDLL(None, use_errno=True)

    def create_executable_memfd(self) -> int:
        if not hasattr(os, "memfd_create"):
            raise OSError(errno_module.ENOSYS, "memfd_create unavailable")
        return os.memfd_create(
            "labcolors-arb-evaluator",
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
        for index in (6, 8, 10, 12):
            distance = final_kill - index - 1
            instructions[index].jf = distance

        # A plain write rule is safe only because close_range leaves fd 1, 2
        # and the CLOEXEC setup fd.  No executable or filesystem fd survives.
        if setup_error_fd not in (4,):
            raise OSError(errno_module.EINVAL, "noncanonical setup fd")
        if generic_start != 14:
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


class _CgroupV2V1:
    def __init__(self, parent_fd: int, directory_fd: int, name: bytes) -> None:
        self._parent_fd = parent_fd
        self._directory_fd = directory_fd
        self._name = name

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

    The task-count checks are fail-closed observations that minimise, but do not
    eliminate, the observation-to-fork race.  Correctness therefore requires a
    dedicated process in which thread creation is architecturally forbidden.
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

    def probe(self) -> CapabilityReportV1:
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
                return UnsupportedV1(
                    (
                        CapabilityFailureV1(
                            CapabilityReasonV1.KERNEL_API_UNAVAILABLE,
                            None,
                        ),
                    )
                )
            operations = _NativeLinuxOperationsV1()
            self._operations = operations

        failures: list[CapabilityFailureV1] = []
        _probe_operation(
            operations.probe_standard_fds,
            CapabilityReasonV1.STANDARD_FDS_UNAVAILABLE,
            failures,
        )
        _probe_operation(
            operations.probe_single_threaded,
            CapabilityReasonV1.OBSERVER_NOT_SINGLE_THREADED,
            failures,
        )
        self._probe_sealed_memfd(operations, failures)
        _probe_operation(operations.probe_execveat, CapabilityReasonV1.EXECVEAT_UNAVAILABLE, failures)
        _probe_operation(operations.probe_close_range, CapabilityReasonV1.CLOSE_RANGE_UNAVAILABLE, failures)
        _probe_operation(operations.probe_namespaces, CapabilityReasonV1.NETWORK_NAMESPACE_UNAVAILABLE, failures)
        _probe_operation(operations.probe_seccomp, CapabilityReasonV1.SECCOMP_FILTER_UNAVAILABLE, failures)
        _probe_operation(
            lambda: self._cgroup_factory.probe(self._cgroup_parent),
            CapabilityReasonV1.CGROUP_V2_UNAVAILABLE,
            failures,
        )
        if failures:
            return UnsupportedV1(tuple(failures))
        return SupportedV1("linux-x86_64", SANDBOX_POLICY_RELEASE_V1)

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

    def run(self, request: ExecutionRequestV1) -> ExecutionResultV1:
        report = self.probe()
        if type(report) is UnsupportedV1:
            return report
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
            # Keep this as the final operation before fork to minimise the
            # observation-to-fork window.  It is a fail-closed observation, not
            # an atomic proof; the backend must run in a dedicated process where
            # thread creation is architecturally forbidden.
            operations.probe_single_threaded()
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
