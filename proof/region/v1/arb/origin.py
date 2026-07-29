#!/usr/bin/env python3
"""Pure admission primitives for scoped source-integrity observations."""

from __future__ import annotations

import base64
import binascii
import hashlib
import os
import stat
import tempfile
from dataclasses import dataclass
from datetime import UTC, date, datetime
from enum import StrEnum
from functools import cmp_to_key
from pathlib import Path
from typing import NoReturn, Protocol

import provenance


OPENPGP_V4_FINGERPRINT_BYTES = 20
ARMOUR_BEGIN = b"-----BEGIN PGP PUBLIC KEY BLOCK-----"
ARMOUR_END = b"-----END PGP PUBLIC KEY BLOCK-----"
CRC24_INITIAL = 0xB704CE
CRC24_POLYNOMIAL = 0x1864CFB


class OriginReasonV1(StrEnum):
    INVALID_ARMOUR = "invalid_armour"
    ARMOUR_CRC_MISMATCH = "armour_crc_mismatch"
    INVALID_FINGERPRINT = "invalid_fingerprint"
    INVALID_STATUS = "invalid_status"
    SIGNATURE_REJECTED = "signature_rejected"
    COORDINATE_MISMATCH = "coordinate_mismatch"
    VERIFIER_FAILED = "verifier_failed"
    VERIFIER_UNAVAILABLE = "verifier_unavailable"
    VERIFIER_OUTPUT_LIMIT = "verifier_output_limit"
    VERIFIER_TIMEOUT = "verifier_timeout"
    CONTENT_RELATION_MISMATCH = "content_relation_mismatch"


@dataclass(frozen=True)
class OriginErrorV1(ValueError):
    reason: OriginReasonV1
    detail: str

    def __str__(self) -> str:
        return f"{self.reason}: {self.detail}"


def _fail(reason: OriginReasonV1, detail: str) -> NoReturn:
    raise OriginErrorV1(reason, detail)


def _crc24(payload: bytes) -> bytes:
    value = CRC24_INITIAL
    for byte in payload:
        value ^= byte << 16
        for _ in range(8):
            value <<= 1
            if value & 0x1000000:
                value ^= CRC24_POLYNOMIAL
    return (value & 0xFFFFFF).to_bytes(3, "big")


def decode_public_key_armour(armour: bytes) -> bytes:
    """Decode the one canonical ASCII-armour shape stored by this proof lane."""

    if type(armour) is not bytes or not armour.endswith(b"\n") or b"\r" in armour:
        _fail(OriginReasonV1.INVALID_ARMOUR, "armour must be LF-terminated bytes")
    try:
        text = armour.decode("ascii")
    except UnicodeDecodeError:
        _fail(OriginReasonV1.INVALID_ARMOUR, "armour is not ASCII")
    lines = text.split("\n")
    if (
        len(lines) < 7
        or lines[0] != ARMOUR_BEGIN.decode("ascii")
        or lines[1] != ""
        or lines[-2] != ARMOUR_END.decode("ascii")
        or lines[-1] != ""
    ):
        _fail(OriginReasonV1.INVALID_ARMOUR, "unexpected armour envelope")
    body = lines[2:-3]
    checksum = lines[-3]
    if (
        not body
        or any(len(line) != 64 for line in body[:-1])
        or not 1 <= len(body[-1]) <= 64
        or len(body[-1]) % 4
        or not checksum.startswith("=")
        or len(checksum) != 5
    ):
        _fail(OriginReasonV1.INVALID_ARMOUR, "noncanonical base64 body")
    try:
        packets = base64.b64decode("".join(body), validate=True)
        expected_crc = base64.b64decode(checksum[1:], validate=True)
    except (binascii.Error, ValueError):
        _fail(OriginReasonV1.INVALID_ARMOUR, "invalid base64")
    if not packets or len(expected_crc) != 3:
        _fail(OriginReasonV1.INVALID_ARMOUR, "empty packets or invalid CRC")
    if _crc24(packets) != expected_crc:
        _fail(OriginReasonV1.ARMOUR_CRC_MISMATCH, "CRC-24 mismatch")
    return packets


@dataclass(frozen=True)
class AcceptedHistoricalSignatureStatusV1:
    signer_fingerprint: bytes
    signature_unix_time: int

    def __post_init__(self) -> None:
        if (
            type(self.signer_fingerprint) is not bytes
            or len(self.signer_fingerprint) != OPENPGP_V4_FINGERPRINT_BYTES
            or self.signer_fingerprint == bytes(OPENPGP_V4_FINGERPRINT_BYTES)
        ):
            raise TypeError("invalid signer fingerprint")
        if type(self.signature_unix_time) is not int or self.signature_unix_time <= 0:
            raise TypeError("invalid signature time")


_ALLOWED_STATUS_TAGS = frozenset(
    (
        "NEWSIG",
        "KEYEXPIRED",
        "KEY_CONSIDERED",
        "SIG_ID",
        "EXPKEYSIG",
        "GOODSIG",
        "VALIDSIG",
    )
)
_REJECTED_STATUS_TAGS = frozenset(
    (
        "BADSIG",
        "ERRSIG",
        "REVKEYSIG",
        "KEYREVOKED",
        "NO_PUBKEY",
        "NODATA",
        "FAILURE",
        "ERROR",
    )
)


def _fingerprint(value: bytes) -> bytes:
    if (
        type(value) is not bytes
        or len(value) != OPENPGP_V4_FINGERPRINT_BYTES
        or value == bytes(OPENPGP_V4_FINGERPRINT_BYTES)
    ):
        _fail(OriginReasonV1.INVALID_FINGERPRINT, "expected fingerprint length")
    return value


def parse_gpgv_status(
    status: bytes, expected_fingerprint: bytes
) -> AcceptedHistoricalSignatureStatusV1:
    """Accept one historical machine-status shape; stderr has no authority."""

    expected = _fingerprint(expected_fingerprint)
    if (
        type(status) is not bytes
        or not status.endswith(b"\n")
        or b"\r" in status
        or b"\0" in status
    ):
        _fail(OriginReasonV1.INVALID_STATUS, "status must be LF-terminated bytes")
    lines = status[:-1].split(b"\n")
    if not lines:
        _fail(OriginReasonV1.INVALID_STATUS, "empty status")

    newsig_count = 0
    valid: list[tuple[bytes, int]] = []
    prefix = b"[GNUPG:] "
    for line in lines:
        if not line.startswith(prefix):
            _fail(OriginReasonV1.INVALID_STATUS, "unframed output")
        payload = line[len(prefix) :]
        tag_bytes, separator, arguments = payload.partition(b" ")
        try:
            tag = tag_bytes.decode("ascii")
        except UnicodeDecodeError:
            _fail(OriginReasonV1.INVALID_STATUS, "non-ASCII tag")
        if tag in _REJECTED_STATUS_TAGS:
            _fail(OriginReasonV1.SIGNATURE_REJECTED, tag)
        if tag not in _ALLOWED_STATUS_TAGS:
            _fail(OriginReasonV1.INVALID_STATUS, f"unknown tag {tag}")
        if tag == "NEWSIG":
            newsig_count += 1
            continue
        if not separator:
            _fail(OriginReasonV1.INVALID_STATUS, f"missing arguments for {tag}")
        if tag != "VALIDSIG":
            continue

        fields = arguments.split(b" ")
        if len(fields) != 10 or any(not item for item in fields):
            _fail(OriginReasonV1.INVALID_STATUS, "invalid VALIDSIG fields")
        try:
            signer = bytes.fromhex(fields[0].decode("ascii"))
            primary = bytes.fromhex(fields[9].decode("ascii"))
            signature_time = int(fields[2], 10)
            date_text = fields[1].decode("ascii")
            parsed_date = date.fromisoformat(date_text)
        except (UnicodeDecodeError, ValueError, OverflowError):
            _fail(OriginReasonV1.INVALID_STATUS, "invalid VALIDSIG coordinate")
        try:
            timestamp_date = datetime.fromtimestamp(signature_time, UTC).date()
        except (OverflowError, OSError, ValueError):
            _fail(OriginReasonV1.INVALID_STATUS, "invalid VALIDSIG time range")
        if (
            signer != expected
            or primary != expected
            or signature_time <= 0
            or parsed_date.isoformat() != date_text
            or timestamp_date != parsed_date
        ):
            _fail(OriginReasonV1.SIGNATURE_REJECTED, "foreign signer or time")
        valid.append((signer, signature_time))

    if newsig_count != 1 or len(valid) != 1:
        _fail(OriginReasonV1.SIGNATURE_REJECTED, "expected exactly one signature")
    return AcceptedHistoricalSignatureStatusV1(valid[0][0], valid[0][1])


def _digest(value: bytes, field: str) -> bytes:
    if type(value) is not bytes or len(value) != 32 or value == bytes(32):
        raise TypeError(f"invalid {field}")
    return value


_GPGV_PROCESS_TOKEN = object()
_SIGNATURE_RELATION_TOKEN = object()


@dataclass(frozen=True, init=False)
class GpgvProcessObservationV1:
    returncode: int
    status: bytes
    stderr: bytes
    source_tree_identity: bytes
    archive_sha256: bytes
    signature_sha256: bytes
    public_key_packets_sha256: bytes
    executable_sha256: bytes
    version_sha256: bytes

    def __init__(
        self,
        returncode: int,
        status: bytes,
        stderr: bytes,
        source_tree_identity: bytes,
        archive_sha256: bytes,
        signature_sha256: bytes,
        public_key_packets_sha256: bytes,
        executable_sha256: bytes,
        version_sha256: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _GPGV_PROCESS_TOKEN:
            raise TypeError("GpgvProcessObservationV1 is created only by run_gpgv")
        if type(returncode) is not int or returncode < 0:
            raise TypeError("invalid gpgv returncode")
        if type(status) is not bytes or len(status) > 64 * 1024:
            raise TypeError("invalid gpgv status")
        if type(stderr) is not bytes or len(stderr) > 64 * 1024:
            raise TypeError("invalid gpgv stderr")
        _digest(source_tree_identity, "source tree identity")
        _digest(archive_sha256, "source archive digest")
        _digest(signature_sha256, "detached signature digest")
        _digest(public_key_packets_sha256, "public key packets digest")
        _digest(executable_sha256, "gpgv executable digest")
        _digest(version_sha256, "gpgv version digest")
        object.__setattr__(self, "returncode", returncode)
        object.__setattr__(self, "status", status)
        object.__setattr__(self, "stderr", stderr)
        object.__setattr__(self, "source_tree_identity", source_tree_identity)
        object.__setattr__(self, "archive_sha256", archive_sha256)
        object.__setattr__(self, "signature_sha256", signature_sha256)
        object.__setattr__(
            self,
            "public_key_packets_sha256",
            public_key_packets_sha256,
        )
        object.__setattr__(self, "executable_sha256", executable_sha256)
        object.__setattr__(self, "version_sha256", version_sha256)


@dataclass(frozen=True, init=False)
class _SignatureRelationObservationV1:
    archive_sha256: bytes
    source_tree_identity: bytes
    signature_sha256: bytes
    public_key_packets_sha256: bytes
    signer_fingerprint: bytes
    signature_unix_time: int
    verifier_executable_sha256: bytes
    verifier_version_sha256: bytes

    def __init__(
        self,
        archive_sha256: bytes,
        source_tree_identity: bytes,
        signature_sha256: bytes,
        public_key_packets_sha256: bytes,
        signer_fingerprint: bytes,
        signature_unix_time: int,
        verifier_executable_sha256: bytes,
        verifier_version_sha256: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _SIGNATURE_RELATION_TOKEN:
            raise TypeError("signature relation is created only by admission")
        for field in (
            "archive_sha256",
            "source_tree_identity",
            "signature_sha256",
            "public_key_packets_sha256",
            "verifier_executable_sha256",
            "verifier_version_sha256",
        ):
            _digest(locals()[field], field)
        AcceptedHistoricalSignatureStatusV1(
            signer_fingerprint,
            signature_unix_time,
        )
        for field in self.__dataclass_fields__:
            object.__setattr__(self, field, locals()[field])


class HistoricalPathRecheckedSignatureDiagnosticV1(_SignatureRelationObservationV1):
    """Historical signature diagnostic; no current publisher trust is implied."""


def admit_detached_signature_observation(
    *,
    expected: provenance.SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
    signature: bytes,
    public_key_armour: bytes,
    process: GpgvProcessObservationV1,
) -> HistoricalPathRecheckedSignatureDiagnosticV1:
    """Replay one historical signature relation as a path-rechecked diagnostic.

    The result records what the invoked verifier reported for project-pinned
    bytes.  It does not establish current publisher identity, key status, or an
    exact sealed verifier execution.
    """

    if type(expected) is not provenance.SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    if type(admitted) is not provenance.SafeSourceArchiveV1:
        raise TypeError("admitted must be SafeSourceArchiveV1")
    if type(expected.integrity) is not provenance.DetachedSignaturePolicyV1:
        raise TypeError("source must declare a detached signature policy")
    if type(signature) is not bytes:
        raise TypeError("signature must be bytes")
    if type(process) is not GpgvProcessObservationV1:
        raise TypeError("process must be a sealed GpgvProcessObservationV1")
    archive = admitted.archive_bytes
    actual_archive_sha256 = hashlib.sha256(archive).digest()
    actual_signature_sha256 = hashlib.sha256(signature).digest()
    key_packets = decode_public_key_armour(public_key_armour)
    actual_key_packets_sha256 = hashlib.sha256(key_packets).digest()
    if (
        admitted.source_lock_identity != expected.identity
        or admitted.archive_sha256 != expected.archive_sha256
        or actual_archive_sha256 != expected.archive_sha256
        or len(signature) != expected.integrity.signature_length
        or actual_signature_sha256 != expected.integrity.signature_sha256
        or actual_key_packets_sha256
        != expected.integrity.public_key_packets_sha256
        or process.source_tree_identity != admitted.tree_identity
        or process.archive_sha256 != actual_archive_sha256
        or process.signature_sha256 != actual_signature_sha256
        or process.public_key_packets_sha256 != actual_key_packets_sha256
    ):
        _fail(OriginReasonV1.COORDINATE_MISMATCH, "source, signature, key, or replay")
    if process.returncode != 0:
        _fail(OriginReasonV1.VERIFIER_FAILED, f"gpgv exit {process.returncode}")
    signature_observation = parse_gpgv_status(
        process.status,
        expected.integrity.signer_fingerprint,
    )
    return HistoricalPathRecheckedSignatureDiagnosticV1(
        actual_archive_sha256,
        admitted.tree_identity,
        actual_signature_sha256,
        actual_key_packets_sha256,
        signature_observation.signer_fingerprint,
        signature_observation.signature_unix_time,
        process.executable_sha256,
        process.version_sha256,
        _token=_SIGNATURE_RELATION_TOKEN,
    )


def _read_regular_file_descriptor(descriptor: int) -> bytes:
    metadata = os.fstat(descriptor)
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_size <= 0:
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "gpgv is not a regular file")
    chunks: list[bytes] = []
    offset = 0
    while offset < metadata.st_size:
        chunk = os.pread(descriptor, min(64 * 1024, metadata.st_size - offset), offset)
        if not chunk:
            _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "short gpgv read")
        chunks.append(chunk)
        offset += len(chunk)
    return b"".join(chunks)

@dataclass(frozen=True)
class DiagnosticProcessRequestV1:
    """Client-owned diagnostic execution request with no authority semantics."""

    argv: tuple[str, ...]
    stdin: bytes | None
    cwd: Path
    environment: dict[str, str]
    pass_fds: tuple[int, ...]
    timeout_seconds: int | float
    stdout_limit: int
    stderr_limit: int

    def __post_init__(self) -> None:
        if (
            type(self.argv) is not tuple
            or not self.argv
            or any(type(item) is not str or not item for item in self.argv)
            or (self.stdin is not None and type(self.stdin) is not bytes)
            or not isinstance(self.cwd, Path)
            or type(self.environment) is not dict
            or any(
                type(key) is not str or type(value) is not str
                for key, value in self.environment.items()
            )
            or type(self.pass_fds) is not tuple
            or any(type(fd) is not int or fd < 0 for fd in self.pass_fds)
            or type(self.timeout_seconds) not in (int, float)
            or self.timeout_seconds <= 0
            or type(self.stdout_limit) is not int
            or self.stdout_limit < 0
            or type(self.stderr_limit) is not int
            or self.stderr_limit < 0
        ):
            raise TypeError("invalid diagnostic process request")


@dataclass(frozen=True)
class DiagnosticProcessObservationV1:
    """Untrusted bytes returned by client-owned diagnostic execution."""

    returncode: int
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        if (
            type(self.returncode) is not int
            or not -(1 << 31) <= self.returncode < 1 << 31
            or type(self.stdout) is not bytes
            or type(self.stderr) is not bytes
        ):
            raise TypeError("invalid diagnostic process observation")


class DiagnosticProcessRunnerV1(Protocol):
    """Client-owned resource runner; this interface grants no sandbox claim."""

    def run(
        self,
        request: DiagnosticProcessRequestV1,
    ) -> DiagnosticProcessObservationV1: ...


def _observe_diagnostic_process_v1(
    runner: DiagnosticProcessRunnerV1,
    request: DiagnosticProcessRequestV1,
) -> DiagnosticProcessObservationV1:
    try:
        observed = runner.run(request)
    except Exception:
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "diagnostic runner failed")
    if type(observed) is not DiagnosticProcessObservationV1:
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "foreign diagnostic observation")
    if (
        len(observed.stdout) > request.stdout_limit
        or len(observed.stderr) > request.stderr_limit
    ):
        _fail(OriginReasonV1.VERIFIER_OUTPUT_LIMIT, "diagnostic output exceeded policy")
    return observed


def _run_diagnostic_v1(
    runner: DiagnosticProcessRunnerV1,
    argv: tuple[str, ...],
    *,
    stdin: bytes | None,
    cwd: Path,
    environment: dict[str, str],
    pass_fds: tuple[int, ...],
    timeout_seconds: int | float,
    stdout_limit: int,
    stderr_limit: int,
) -> DiagnosticProcessObservationV1:
    return _observe_diagnostic_process_v1(
        runner,
        DiagnosticProcessRequestV1(
            argv,
            stdin,
            cwd,
            environment,
            pass_fds,
            timeout_seconds,
            stdout_limit,
            stderr_limit,
        ),
    )


def run_gpgv(
    source: provenance.SafeSourceArchiveV1,
    signature: bytes,
    public_key_armour: bytes,
    *,
    executable: Path,
    runner: DiagnosticProcessRunnerV1,
) -> GpgvProcessObservationV1:
    """Request a client-owned gpgv diagnostic; never mint execution authority."""

    if type(source) is not provenance.SafeSourceArchiveV1:
        raise TypeError("source must be SafeSourceArchiveV1")
    if any(type(value) is not bytes for value in (signature, public_key_armour)):
        raise TypeError("gpgv signature and key must be bytes")
    archive = source.archive_bytes
    key_packets = decode_public_key_armour(public_key_armour)
    signature_sha256 = hashlib.sha256(signature).digest()
    public_key_packets_sha256 = hashlib.sha256(key_packets).digest()
    try:
        resolved = executable.resolve(strict=True)
        descriptor = os.open(
            resolved,
            os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0),
        )
    except (OSError, RuntimeError):
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "cannot open gpgv")
    try:
        executable_bytes = _read_regular_file_descriptor(descriptor)
        executable_sha256 = hashlib.sha256(executable_bytes).digest()
        descriptor_exec_supported = Path("/proc/self/fd").is_dir()
        descriptor_path = (
            f"/proc/self/fd/{descriptor}"
            if descriptor_exec_supported
            else str(resolved)
        )
        inherited_descriptors = (descriptor,) if descriptor_exec_supported else ()
        with tempfile.TemporaryDirectory(prefix="labcolors-gpgv-") as temporary:
            root = Path(temporary)
            keyring = root / "keyring.gpg"
            detached = root / "signature.bin"
            keyring.write_bytes(key_packets)
            detached.write_bytes(signature)
            os.chmod(keyring, 0o400)
            os.chmod(detached, 0o400)
            environment = {
                "HOME": "/nonexistent",
                "LANG": "C",
                "LC_ALL": "C",
                "TZ": "UTC",
            }
            version = _run_diagnostic_v1(
                runner,
                (descriptor_path, "--version"),
                stdin=None,
                cwd=root,
                environment=environment,
                pass_fds=inherited_descriptors,
                timeout_seconds=10,
                stdout_limit=64 * 1024,
                stderr_limit=64 * 1024,
            )
            verified = _run_diagnostic_v1(
                runner,
                (
                    descriptor_path,
                    "--homedir",
                    str(root),
                    "--keyring",
                    str(keyring),
                    "--status-fd",
                    "1",
                    str(detached),
                    "-",
                ),
                stdin=archive,
                cwd=root,
                environment=environment,
                pass_fds=inherited_descriptors,
                timeout_seconds=60,
                stdout_limit=64 * 1024,
                stderr_limit=64 * 1024,
            )
        if version.returncode != 0 or not version.stdout or version.stderr:
            _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "gpgv version failed")
        if verified.returncode < 0:
            _fail(
                OriginReasonV1.VERIFIER_FAILED,
                f"gpgv terminated by signal {-verified.returncode}",
            )
        if not descriptor_exec_supported:
            try:
                if hashlib.sha256(resolved.read_bytes()).digest() != executable_sha256:
                    _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "gpgv changed during replay")
            except OSError:
                _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "cannot re-read gpgv")
        return GpgvProcessObservationV1(
            verified.returncode,
            verified.stdout,
            verified.stderr,
            source.tree_identity,
            source.archive_sha256,
            signature_sha256,
            public_key_packets_sha256,
            executable_sha256,
            hashlib.sha256(version.stdout).digest(),
            _token=_GPGV_PROCESS_TOKEN,
        )
    finally:
        os.close(descriptor)


def _sha1(value: bytes, field: str) -> bytes:
    if type(value) is not bytes or len(value) != 20 or value == bytes(20):
        raise TypeError(f"invalid {field}")
    return value


def _source_path(value: str) -> bytes:
    if type(value) is not str or not value or value.startswith("/") or "\\" in value:
        raise TypeError("invalid source path")
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError:
        raise TypeError("source path must be ASCII") from None
    if (
        len(encoded) > 4096
        or any(byte < 0x20 or byte == 0x7F for byte in encoded)
        or any(part in ("", ".", "..") for part in value.split("/"))
    ):
        raise TypeError("invalid source path")
    return encoded


@dataclass(frozen=True)
class FileCoordinateV1:
    path: str
    mode: int
    length: int
    sha256: bytes

    def __post_init__(self) -> None:
        _source_path(self.path)
        if type(self.mode) is not int or self.mode not in (0o644, 0o700, 0o755):
            raise TypeError("invalid source mode")
        if type(self.length) is not int or self.length < 0 or self.length >= 1 << 64:
            raise TypeError("invalid source length")
        _digest(self.sha256, "source file digest")


def _canonical_files(value: tuple[FileCoordinateV1, ...], field: str) -> None:
    if type(value) is not tuple or any(type(item) is not FileCoordinateV1 for item in value):
        raise TypeError(f"invalid {field}")
    paths = tuple(item.path for item in value)
    if paths != tuple(sorted(set(paths))):
        raise TypeError(f"noncanonical {field}")


def _file_set_digest(files: tuple[FileCoordinateV1, ...], label: bytes) -> bytes:
    hasher = hashlib.sha256(label)
    hasher.update(len(files).to_bytes(8, "big"))
    for item in files:
        path = item.path.encode("ascii")
        hasher.update(len(path).to_bytes(4, "big"))
        hasher.update(path)
        hasher.update(item.mode.to_bytes(4, "big"))
        hasher.update(item.length.to_bytes(8, "big"))
        hasher.update(item.sha256)
    return hasher.digest()


_GIT_PROCESS_TOKEN = object()
_GIT_RELATION_TOKEN = object()


@dataclass(frozen=True, init=False)
class GitTreeProcessObservationV1:
    commit: bytes
    tree: bytes
    commit_object_sha256: bytes
    files: tuple[FileCoordinateV1, ...]
    executable_sha256: bytes
    version_sha256: bytes

    def __init__(
        self,
        commit: bytes,
        tree: bytes,
        commit_object_sha256: bytes,
        files: tuple[FileCoordinateV1, ...],
        executable_sha256: bytes,
        version_sha256: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _GIT_PROCESS_TOKEN:
            raise TypeError("GitTreeProcessObservationV1 is created only by run_git_tree")
        _sha1(commit, "Git commit")
        _sha1(tree, "Git tree")
        _digest(commit_object_sha256, "Git commit object digest")
        _canonical_files(files, "Git files")
        if not files or any(item.mode == 0o700 for item in files):
            raise TypeError("invalid Git tree")
        _digest(executable_sha256, "Git executable digest")
        _digest(version_sha256, "Git version digest")
        object.__setattr__(self, "commit", commit)
        object.__setattr__(self, "tree", tree)
        object.__setattr__(self, "commit_object_sha256", commit_object_sha256)
        object.__setattr__(self, "files", files)
        object.__setattr__(self, "executable_sha256", executable_sha256)
        object.__setattr__(self, "version_sha256", version_sha256)


@dataclass(frozen=True, init=False)
class RecomputedGitContentRelationV1:
    archive_sha256: bytes
    source_tree_identity: bytes
    commit: bytes
    tree: bytes
    commit_object_sha256: bytes
    git_files_identity: bytes
    archive_files_identity: bytes
    common_file_count: int
    omitted_file_count: int
    project_pinned_release_only_file_count: int

    def __init__(
        self,
        archive_sha256: bytes,
        source_tree_identity: bytes,
        commit: bytes,
        tree: bytes,
        commit_object_sha256: bytes,
        git_files_identity: bytes,
        archive_files_identity: bytes,
        common_file_count: int,
        omitted_file_count: int,
        project_pinned_release_only_file_count: int,
        *,
        _token: object,
    ) -> None:
        if _token is not _GIT_RELATION_TOKEN:
            raise TypeError("Git relation is created only by admission")
        _sha1(commit, "Git commit")
        _sha1(tree, "Git tree")
        for field in (
            "archive_sha256",
            "source_tree_identity",
            "git_files_identity",
            "archive_files_identity",
            "commit_object_sha256",
        ):
            _digest(locals()[field], field)
        for field in (
            "common_file_count",
            "omitted_file_count",
            "project_pinned_release_only_file_count",
        ):
            value = locals()[field]
            if type(value) is not int or value <= 0:
                raise TypeError(f"invalid {field}")
        for field in self.__dataclass_fields__:
            object.__setattr__(self, field, locals()[field])


def admit_git_content_relation_observation(
    *,
    expected: provenance.SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
    process: GitTreeProcessObservationV1,
) -> RecomputedGitContentRelationV1:
    """Relate archive bytes to a project-pinned, independently replayed graph.

    Git supplies bytes and diagnostics only.  The admitted relation derives
    from locally recomputed commit, tree, and blob identities, so executable
    metadata is intentionally absent from its identity and authority surface.
    """

    if type(expected) is not provenance.SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    if type(admitted) is not provenance.SafeSourceArchiveV1:
        raise TypeError("admitted must be SafeSourceArchiveV1")
    if type(expected.integrity) is not provenance.GitContentRelationPolicyV1:
        raise TypeError("source must declare a Git content relation policy")
    if type(process) is not GitTreeProcessObservationV1:
        raise TypeError("process must be a sealed GitTreeProcessObservationV1")
    if (
        admitted.source_lock_identity != expected.identity
        or admitted.archive_sha256 != expected.archive_sha256
        or process.commit != expected.integrity.commit
        or process.tree != expected.integrity.tree
    ):
        _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "source, commit, or tree")
    expected_common_file_count = expected.integrity.common_file_count
    omitted_paths = expected.integrity.omitted_paths
    project_pinned_release_only_files = tuple(
        FileCoordinateV1(item.path, item.mode, item.length, item.sha256)
        for item in expected.integrity.project_pinned_release_only_files
    )
    archive_files = tuple(
        FileCoordinateV1(item.path, item.mode, item.length, item.sha256)
        for item in admitted.files
    )
    _canonical_files(
        project_pinned_release_only_files,
        "project-pinned release-only files",
    )
    _canonical_files(archive_files, "archive files")
    if not project_pinned_release_only_files or not archive_files:
        raise TypeError("empty content relation")
    if type(omitted_paths) is not tuple or not omitted_paths:
        raise TypeError("empty omitted paths")
    for path in omitted_paths:
        _source_path(path)
    if omitted_paths != tuple(sorted(set(omitted_paths))):
        raise TypeError("noncanonical omitted paths")

    git_by_path = {item.path: item for item in process.files}
    archive_by_path = {item.path: item for item in archive_files}
    release_only_by_path = {
        item.path: item for item in project_pinned_release_only_files
    }
    omitted = set(omitted_paths)
    release_only = set(release_only_by_path)
    git_paths = set(git_by_path)
    archive_paths = set(archive_by_path)
    common_paths = git_paths - omitted
    if (
        len(common_paths) != expected_common_file_count
        or not omitted <= git_paths
        or omitted & archive_paths
        or release_only & git_paths
        or not release_only <= archive_paths
        or archive_paths != common_paths | release_only
    ):
        _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "path partition")
    if any(archive_by_path[path] != git_by_path[path] for path in common_paths):
        _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "common file content")
    if any(
        archive_by_path[path] != release_only_by_path[path]
        for path in release_only
    ):
        _fail(
            OriginReasonV1.CONTENT_RELATION_MISMATCH,
            "project-pinned release-only file content",
        )
    return RecomputedGitContentRelationV1(
        admitted.archive_sha256,
        admitted.tree_identity,
        process.commit,
        process.tree,
        process.commit_object_sha256,
        _file_set_digest(process.files, b"labcolors.git-tree-files.v1\0"),
        _file_set_digest(archive_files, b"labcolors.release-archive-files.v1\0"),
        len(common_paths),
        len(omitted),
        len(release_only),
        _token=_GIT_RELATION_TOKEN,
    )


def _git_object_id(value: bytes) -> bytes:
    if len(value) != 40:
        _fail(OriginReasonV1.INVALID_STATUS, "invalid Git object id")
    try:
        decoded = bytes.fromhex(value.decode("ascii"))
    except (UnicodeDecodeError, ValueError):
        _fail(OriginReasonV1.INVALID_STATUS, "invalid Git object id")
    if decoded == bytes(20):
        _fail(OriginReasonV1.INVALID_STATUS, "zero Git object id")
    return decoded


def _parse_git_listing(raw: bytes) -> tuple[tuple[bytes, str, int], ...]:
    if type(raw) is not bytes or not raw or not raw.endswith(b"\0"):
        _fail(OriginReasonV1.INVALID_STATUS, "invalid Git listing")
    records: list[tuple[bytes, str, int]] = []
    previous: str | None = None
    for encoded in raw[:-1].split(b"\0"):
        metadata, separator, path_raw = encoded.partition(b"\t")
        fields = metadata.split(b" ")
        if not separator or len(fields) != 3 or fields[1] != b"blob":
            _fail(OriginReasonV1.INVALID_STATUS, "non-blob Git entry")
        if fields[0] == b"100644":
            mode = 0o644
        elif fields[0] == b"100755":
            mode = 0o755
        else:
            _fail(OriginReasonV1.INVALID_STATUS, "unsupported Git mode")
        try:
            path = path_raw.decode("ascii")
        except UnicodeDecodeError:
            _fail(OriginReasonV1.INVALID_STATUS, "non-ASCII Git path")
        try:
            _source_path(path)
        except TypeError:
            _fail(OriginReasonV1.INVALID_STATUS, "invalid Git path")
        _git_object_id(fields[2])
        if previous is not None and previous >= path:
            _fail(OriginReasonV1.INVALID_STATUS, "noncanonical Git path order")
        previous = path
        records.append((fields[2], path, mode))
    if not records:
        _fail(OriginReasonV1.INVALID_STATUS, "empty Git tree")
    return tuple(records)


def _recompute_git_tree_identity(
    listing: tuple[tuple[bytes, str, int], ...]
) -> bytes:
    """Rebuild recursive Git tree objects without trusting `git ls-tree` IDs."""

    if type(listing) is not tuple or not listing:
        _fail(OriginReasonV1.INVALID_STATUS, "empty Git listing")
    root: dict[bytes, object] = {}
    for object_id_raw, path, mode in listing:
        object_id = _git_object_id(object_id_raw)
        components = path.encode("ascii").split(b"/")
        node = root
        for component in components[:-1]:
            existing = node.get(component)
            if existing is None:
                child: dict[bytes, object] = {}
                node[component] = child
                node = child
            elif type(existing) is dict:
                node = existing
            else:
                _fail(OriginReasonV1.INVALID_STATUS, "Git file/directory collision")
        leaf = components[-1]
        if leaf in node:
            _fail(OriginReasonV1.INVALID_STATUS, "duplicate Git tree entry")
        node[leaf] = (mode, object_id)

    def compare_entries(
        left: tuple[bytes, bool, bytes, bytes],
        right: tuple[bytes, bool, bytes, bytes],
    ) -> int:
        left_name, left_tree, _left_mode, _left_id = left
        right_name, right_tree, _right_mode, _right_id = right
        common = min(len(left_name), len(right_name))
        if left_name[:common] != right_name[:common]:
            return -1 if left_name[:common] < right_name[:common] else 1
        left_next = left_name[common] if common < len(left_name) else (47 if left_tree else 0)
        right_next = right_name[common] if common < len(right_name) else (47 if right_tree else 0)
        return left_next - right_next

    # Git permits paths deeper than Python's recursion limit.  Explicit
    # post-order traversal keeps the accepted path grammar independent of the
    # host interpreter stack while preserving Git's byte ordering exactly.
    digests: dict[int, bytes] = {}
    stack: list[tuple[dict[bytes, object], bool]] = [(root, False)]
    while stack:
        node, visited = stack.pop()
        if not visited:
            stack.append((node, True))
            for child in node.values():
                if type(child) is dict:
                    stack.append((child, False))
            continue

        entries: list[tuple[bytes, bool, bytes, bytes]] = []
        for name, child in node.items():
            if type(child) is dict:
                entries.append((name, True, b"40000", digests[id(child)]))
            else:
                mode, object_id = child  # type: ignore[misc]
                encoded_mode = b"100644" if mode == 0o644 else b"100755"
                entries.append((name, False, encoded_mode, object_id))
        entries.sort(key=cmp_to_key(compare_entries))
        body = b"".join(
            mode + b" " + name + b"\0" + object_id
            for name, _is_tree, mode, object_id in entries
        )
        digests[id(node)] = hashlib.sha1(
            b"tree " + str(len(body)).encode("ascii") + b"\0" + body
        ).digest()

    return digests[id(root)]


def _admit_git_commit_object(body: bytes, commit: bytes, tree: bytes) -> bytes:
    if type(body) is not bytes or not body:
        _fail(OriginReasonV1.INVALID_STATUS, "empty Git commit object")
    expected_commit = _sha1(commit, "Git commit")
    expected_tree = _sha1(tree, "Git tree")
    header = b"commit " + str(len(body)).encode("ascii") + b"\0"
    if hashlib.sha1(header + body).digest() != expected_commit:
        _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "Git commit identity")
    first_line, separator, _remaining = body.partition(b"\n")
    if not separator or first_line != b"tree " + expected_tree.hex().encode("ascii"):
        _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "commit to tree edge")
    return hashlib.sha256(body).digest()


def _parse_git_batch(
    raw: bytes, listing: tuple[tuple[bytes, str, int], ...]
) -> tuple[FileCoordinateV1, ...]:
    if type(raw) is not bytes:
        raise TypeError("Git batch output must be bytes")
    offset = 0
    files: list[FileCoordinateV1] = []
    for object_id_raw, path, mode in listing:
        header_end = raw.find(b"\n", offset)
        if header_end < 0:
            _fail(OriginReasonV1.INVALID_STATUS, "truncated Git batch header")
        header = raw[offset:header_end].split(b" ")
        if len(header) != 3 or header[0] != object_id_raw or header[1] != b"blob":
            _fail(OriginReasonV1.INVALID_STATUS, "foreign Git batch object")
        try:
            length = int(header[2], 10)
        except ValueError:
            _fail(OriginReasonV1.INVALID_STATUS, "invalid Git blob length")
        if (
            length < 0
            or length >= 1 << 64
            or not header[2].isdigit()
            or header[2] != str(length).encode("ascii")
        ):
            _fail(OriginReasonV1.INVALID_STATUS, "invalid Git blob length")
        body_start = header_end + 1
        body_end = body_start + length
        if body_end >= len(raw) or raw[body_end : body_end + 1] != b"\n":
            _fail(OriginReasonV1.INVALID_STATUS, "truncated Git blob")
        body = raw[body_start:body_end]
        object_id = _git_object_id(object_id_raw)
        object_header = b"blob " + str(length).encode("ascii") + b"\0"
        if hashlib.sha1(object_header + body).digest() != object_id:
            _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "Git blob identity")
        files.append(FileCoordinateV1(path, mode, length, hashlib.sha256(body).digest()))
        offset = body_end + 1
    if offset != len(raw):
        _fail(OriginReasonV1.INVALID_STATUS, "trailing Git batch bytes")
    return tuple(files)


def run_git_tree(
    repository: Path,
    expected_commit: bytes,
    expected_tree: bytes,
    *,
    executable: Path,
    runner: DiagnosticProcessRunnerV1,
) -> GitTreeProcessObservationV1:
    """Parse a client-owned Git diagnostic and recompute every content edge."""

    commit = _sha1(expected_commit, "expected Git commit")
    tree = _sha1(expected_tree, "expected Git tree")
    try:
        root = repository.resolve(strict=True)
    except (OSError, RuntimeError):
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "Git repository unavailable")
    if not root.is_dir():
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "Git repository is not a directory")
    try:
        resolved = executable.resolve(strict=True)
        descriptor = os.open(
            resolved,
            os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0),
        )
    except (OSError, RuntimeError):
        _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "cannot open Git")
    try:
        executable_bytes = _read_regular_file_descriptor(descriptor)
        executable_sha256 = hashlib.sha256(executable_bytes).digest()
        descriptor_exec_supported = Path("/proc/self/fd").is_dir()
        executable_path = (
            f"/proc/self/fd/{descriptor}"
            if descriptor_exec_supported
            else str(resolved)
        )
        inherited_descriptors = (descriptor,) if descriptor_exec_supported else ()
        environment = {
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_NO_LAZY_FETCH": "1",
            "GIT_OPTIONAL_LOCKS": "0",
            "GIT_PAGER": "cat",
            "HOME": "/nonexistent",
            "LANG": "C",
            "LC_ALL": "C",
            "PATH": "/usr/bin:/bin",
            "TZ": "UTC",
        }

        def invoke(
            arguments: tuple[str, ...],
            *,
            stdin: bytes | None = None,
            timeout: int = 60,
            stdout_limit: int = 64 * 1024,
        ) -> bytes:
            process = _run_diagnostic_v1(
                runner,
                (executable_path, "-C", str(root), *arguments),
                stdin=stdin,
                cwd=root,
                environment=environment,
                pass_fds=inherited_descriptors,
                timeout_seconds=timeout,
                stdout_limit=stdout_limit,
                stderr_limit=64 * 1024,
            )
            if process.returncode != 0 or process.stderr:
                _fail(OriginReasonV1.VERIFIER_FAILED, "Git command rejected")
            return process.stdout

        version_process = _run_diagnostic_v1(
            runner,
            (executable_path, "--version"),
            stdin=None,
            cwd=root,
            environment=environment,
            pass_fds=inherited_descriptors,
            timeout_seconds=10,
            stdout_limit=64 * 1024,
            stderr_limit=64 * 1024,
        )
        if version_process.returncode != 0 or not version_process.stdout or version_process.stderr:
            _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "Git version failed")

        commit_object = invoke(
            ("cat-file", "commit", commit.hex()),
            stdout_limit=1024 * 1024,
        )
        commit_object_sha256 = _admit_git_commit_object(commit_object, commit, tree)
        listing = _parse_git_listing(
            invoke(
                ("ls-tree", "-r", "-z", "--full-tree", tree.hex()),
                stdout_limit=64 * 1024 * 1024,
            )
        )
        if _recompute_git_tree_identity(listing) != tree:
            _fail(OriginReasonV1.CONTENT_RELATION_MISMATCH, "Git tree identity")
        query = b"".join(object_id + b"\n" for object_id, _path, _mode in listing)
        if len(query) > 1024 * 1024:
            _fail(OriginReasonV1.INVALID_STATUS, "oversized Git query")
        batch = invoke(
            ("cat-file", "--batch"),
            stdin=query,
            timeout=180,
            stdout_limit=128 * 1024 * 1024,
        )
        files = _parse_git_batch(batch, listing)
        if not descriptor_exec_supported:
            try:
                if hashlib.sha256(resolved.read_bytes()).digest() != executable_sha256:
                    _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "Git changed during replay")
            except OSError:
                _fail(OriginReasonV1.VERIFIER_UNAVAILABLE, "cannot re-read Git")
        return GitTreeProcessObservationV1(
            commit,
            tree,
            commit_object_sha256,
            files,
            executable_sha256,
            hashlib.sha256(version_process.stdout).digest(),
            _token=_GIT_PROCESS_TOKEN,
        )
    finally:
        os.close(descriptor)
