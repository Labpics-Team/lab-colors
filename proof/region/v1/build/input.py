#!/usr/bin/env python3
"""Canonical BUILD input bytes without engine or recipe semantics."""

from __future__ import annotations

import hashlib
import io
import tarfile
from dataclasses import dataclass
from enum import StrEnum
from typing import NoReturn


_SEALED_INPUT_TOKEN = object()
_USTAR_BLOCK_BYTES = 512
_USTAR_RECORD_BYTES = 20 * _USTAR_BLOCK_BYTES
_USTAR_EOF_BLOCKS = 2


def _valid_digest(value: object) -> bool:
    return type(value) is bytes and len(value) == 32 and value != bytes(32)


class InputReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    INVALID_VALUE = "invalid_value"
    INVALID_PATH = "invalid_path"
    INVALID_MODE = "invalid_mode"
    NONCANONICAL_SET = "noncanonical_set"
    RESOURCE_LIMIT = "resource_limit"


@dataclass(frozen=True)
class InputErrorV1(ValueError):
    reason: InputReasonV1
    field: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.field}"


def _fail(reason: InputReasonV1, field_name: str) -> NoReturn:
    raise InputErrorV1(reason, field_name)


def _positive_u64(value: object, field_name: str) -> int:
    if type(value) is not int:
        _fail(InputReasonV1.WRONG_TYPE, field_name)
    if value <= 0 or value >= 1 << 64:
        _fail(InputReasonV1.INVALID_VALUE, field_name)
    return value


def _logical_path(value: object) -> str:
    if type(value) is not str or not value or value.startswith("/") or "\\" in value:
        _fail(InputReasonV1.INVALID_PATH, "path")
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError:
        _fail(InputReasonV1.INVALID_PATH, "path")
    if (
        len(encoded) > 4096
        or any(byte < 0x20 or byte == 0x7F for byte in encoded)
        or any(part in ("", ".", "..") for part in value.split("/"))
    ):
        _fail(InputReasonV1.INVALID_PATH, "path")
    return value


class CanonicalInputLimitsV1(tuple):
    """Caller-owned resource bounds for one in-memory canonical archive."""

    __slots__ = ()

    def __new__(
        cls,
        max_members: int,
        max_file_bytes: int,
        max_payload_bytes: int,
        max_encoded_bytes: int | None = None,
    ) -> CanonicalInputLimitsV1:
        max_members = _positive_u64(max_members, "max_members")
        max_file_bytes = _positive_u64(max_file_bytes, "max_file_bytes")
        max_payload_bytes = _positive_u64(max_payload_bytes, "max_payload_bytes")
        if max_encoded_bytes is None:
            # USTAR adds one header block per member, at most one partial data
            # block per member, two EOF blocks, then pads to one record. This
            # is derived from the caller's bounds, not a fixture-specific cap.
            maximum_unpadded = (
                max_payload_bytes
                + (2 * _USTAR_BLOCK_BYTES - 1) * max_members
                + _USTAR_EOF_BLOCKS * _USTAR_BLOCK_BYTES
            )
            max_encoded_bytes = _round_up(
                maximum_unpadded,
                _USTAR_RECORD_BYTES,
            )
        max_encoded_bytes = _positive_u64(max_encoded_bytes, "max_encoded_bytes")
        return tuple.__new__(
            cls,
            (
                max_members,
                max_file_bytes,
                max_payload_bytes,
                max_encoded_bytes,
            ),
        )

    @property
    def max_members(self) -> int:
        return self[0]

    @property
    def max_file_bytes(self) -> int:
        return self[1]

    @property
    def max_payload_bytes(self) -> int:
        return self[2]

    @property
    def max_encoded_bytes(self) -> int:
        return self[3]


def _limits_are_valid(value: object) -> bool:
    if type(value) is not CanonicalInputLimitsV1:
        return False
    try:
        return tuple(CanonicalInputLimitsV1(*tuple(value))) == tuple(value)
    except Exception:
        return False


def _ustar_path_is_encodable(path: str, *, directory: bool = False) -> bool:
    encoded = path.encode("ascii")
    if directory:
        # tarfile writes DIRTYPE without a trailing separator as one with it;
        # the USTAR prefix split must validate the exact emitted header name.
        encoded += b"/"
    if len(encoded) <= 100:
        return True
    return any(
        0 < index <= 155 and len(encoded) - index - 1 <= 100
        for index, byte in enumerate(encoded)
        if byte == ord("/")
    )


def _round_up(value: int, quantum: int) -> int:
    return ((value + quantum - 1) // quantum) * quantum


def _encoded_ustar_length(
    entries: tuple[tuple[str, int, bytes], ...],
    directory_count: int,
) -> int:
    data_bytes = sum(
        _round_up(len(contents), _USTAR_BLOCK_BYTES)
        for _path, _mode, contents in entries
    )
    raw_bytes = (
        (len(entries) + directory_count + _USTAR_EOF_BLOCKS)
        * _USTAR_BLOCK_BYTES
        + data_bytes
    )
    return _round_up(raw_bytes, _USTAR_RECORD_BYTES)


class SealedInputV1(tuple):
    """Owned exact bytes carrying only integrity and an opaque caller binding."""

    __slots__ = ()

    def __new__(
        cls,
        binding_identity: bytes,
        contents: bytes,
        *,
        _token: object,
    ) -> SealedInputV1:
        if _token is not _SEALED_INPUT_TOKEN:
            raise TypeError("SealedInputV1 is created only by seal_input_v1")
        if not _valid_digest(binding_identity):
            raise TypeError("binding_identity must be one opaque nonzero digest")
        if type(contents) is not bytes or not contents:
            raise TypeError("sealed input must own nonempty exact bytes")
        return tuple.__new__(
            cls,
            (
                binding_identity,
                hashlib.sha256(contents).digest(),
                len(contents),
                contents,
            ),
        )

    @property
    def binding_identity(self) -> bytes:
        return self[0]

    @property
    def sha256(self) -> bytes:
        return self[1]

    @property
    def length(self) -> int:
        return self[2]

    @property
    def contents(self) -> bytes:
        return self[3]


def seal_input_v1(binding_identity: bytes, contents: bytes) -> SealedInputV1:
    """Seal exact bytes while treating their semantic binding as opaque."""

    if type(binding_identity) is not bytes:
        _fail(InputReasonV1.WRONG_TYPE, "binding_identity")
    if not _valid_digest(binding_identity):
        _fail(InputReasonV1.INVALID_VALUE, "binding_identity")
    if type(contents) is not bytes:
        _fail(InputReasonV1.WRONG_TYPE, "contents")
    if not contents:
        _fail(InputReasonV1.INVALID_VALUE, "contents")
    return SealedInputV1(
        binding_identity,
        contents,
        _token=_SEALED_INPUT_TOKEN,
    )


def sealed_input_is_intact_v1(value: object) -> bool:
    """Recheck byte integrity without interpreting the caller-owned binding."""

    if type(value) is not SealedInputV1:
        return False
    try:
        return (
            _valid_digest(value.binding_identity)
            and type(value.contents) is bytes
            and bool(value.contents)
            and value.length == len(value.contents)
            and value.sha256 == hashlib.sha256(value.contents).digest()
        )
    except Exception:
        return False


def canonical_ustar_v1(
    entries: tuple[tuple[str, int, bytes], ...],
    limits: CanonicalInputLimitsV1,
) -> bytes:
    """Encode one canonical normalized USTAR file tree."""

    if (
        type(entries) is not tuple
        or not entries
        or not _limits_are_valid(limits)
    ):
        _fail(InputReasonV1.NONCANONICAL_SET, "entries")
    if len(entries) > limits.max_members:
        _fail(InputReasonV1.RESOURCE_LIMIT, "max_members")
    parsed: list[tuple[str, int, bytes]] = []
    total_bytes = 0
    for entry in entries:
        if type(entry) is not tuple or len(entry) != 3:
            _fail(InputReasonV1.WRONG_TYPE, "entries")
        path, mode, contents = entry
        path = _logical_path(path)
        if not _ustar_path_is_encodable(path):
            _fail(InputReasonV1.INVALID_PATH, path)
        if type(mode) is not int or mode not in (0o644, 0o755):
            _fail(InputReasonV1.INVALID_MODE, path)
        if type(contents) is not bytes:
            _fail(InputReasonV1.WRONG_TYPE, path)
        total_bytes += len(contents)
        if len(contents) > limits.max_file_bytes:
            _fail(InputReasonV1.RESOURCE_LIMIT, "max_file_bytes")
        if total_bytes > limits.max_payload_bytes:
            _fail(InputReasonV1.RESOURCE_LIMIT, "max_payload_bytes")
        parsed.append((path, mode, contents))
    owned = tuple(parsed)
    paths = tuple(path for path, _mode, _contents in owned)
    if paths != tuple(sorted(paths)) or len(set(paths)) != len(entries):
        _fail(InputReasonV1.NONCANONICAL_SET, "entries")
    directories: set[str] = set()
    for path, _mode, _contents in owned:
        parts = path.split("/")[:-1]
        for length in range(1, len(parts) + 1):
            directories.add("/".join(parts[:length]))
    for path in directories:
        if not _ustar_path_is_encodable(path, directory=True):
            _fail(InputReasonV1.INVALID_PATH, path)
    namespace: dict[str, tuple[str, str]] = {}
    for kind, values in (("directory", tuple(sorted(directories))), ("file", paths)):
        for path in values:
            folded = path.lower()
            prior = namespace.get(folded)
            coordinate = (kind, path)
            if prior is not None and prior != coordinate:
                _fail(InputReasonV1.NONCANONICAL_SET, path)
            namespace[folded] = coordinate
    if (
        directories.intersection(paths)
        or len(directories) + len(owned) > limits.max_members
    ):
        if directories.intersection(paths):
            _fail(InputReasonV1.NONCANONICAL_SET, "file-directory collision")
        _fail(InputReasonV1.RESOURCE_LIMIT, "max_members")
    encoded_length = _encoded_ustar_length(owned, len(directories))
    if encoded_length > limits.max_encoded_bytes:
        _fail(InputReasonV1.RESOURCE_LIMIT, "max_encoded_bytes")

    output = io.BytesIO()
    try:
        with tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for path in sorted(directories, key=lambda value: (value.count("/"), value)):
                member = tarfile.TarInfo(path)
                member.type = tarfile.DIRTYPE
                member.mode = 0o755
                member.uid = 0
                member.gid = 0
                member.uname = ""
                member.gname = ""
                member.mtime = 0
                member.size = 0
                archive.addfile(member)
            for path, mode, contents in owned:
                member = tarfile.TarInfo(path)
                member.type = tarfile.REGTYPE
                member.mode = mode
                member.uid = 0
                member.gid = 0
                member.uname = ""
                member.gname = ""
                member.mtime = 0
                member.size = len(contents)
                archive.addfile(member, io.BytesIO(contents))
    except (OSError, OverflowError, tarfile.TarError, ValueError):
        _fail(InputReasonV1.INVALID_PATH, "USTAR encoding")
    encoded = output.getvalue()
    if len(encoded) != encoded_length:
        _fail(InputReasonV1.NONCANONICAL_SET, "USTAR size mismatch")
    return encoded
