#!/usr/bin/env python3
"""Canonical source declarations and fail-closed archive admission for proof V1.

This module deliberately stops before cryptographic origin verification, build
execution and evaluator replay.  Observations remain claims; only archive bytes
that were hashed and structurally scanned become ``SafeSourceArchiveV1``.
"""

from __future__ import annotations

import hashlib
import io
import lzma
import tarfile
import zlib
from dataclasses import dataclass, field
from enum import IntEnum, StrEnum
from pathlib import PurePosixPath
from typing import NoReturn, TypeAlias
from urllib.parse import urlsplit


SOURCE_LOCK_MAGIC_V1 = b"LCSRC1\0\0"
SOURCE_LOCK_ID_LABEL_V1 = b"labcolors.proof-region.source-lock.v1\0"
SOURCE_TREE_ID_LABEL_V1 = b"labcolors.proof-region.safe-source-tree.v1\0"
ADMITTED_ARB_SOURCES_ID_LABEL_V1 = b"labcolors.proof-region.admitted-arb-sources.v1\0"
ADMITTED_MPFI_SOURCES_ID_LABEL_V1 = b"labcolors.proof-region.admitted-mpfi-sources.v1\0"
SOURCE_LOCK_RELEASE_V1 = 1
SOURCE_CLOSURE_COUNT_V1 = 3
SHA256_BYTES = 32
SHA1_BYTES = 20
OPENPGP_V4_FINGERPRINT_BYTES = 20
TAR_BLOCK_BYTES = 512
TAR_END_MARKER_BYTES = TAR_BLOCK_BYTES * 2
READ_CHUNK_BYTES = 64 * 1024
ALLOWED_REGULAR_MODES_V1 = frozenset((0o644, 0o700, 0o755))
ALLOWED_DIRECTORY_MODE_V1 = 0o755

# Derived identities are capability coordinates, never cache state: a frozen
# dataclass still has a writable __dict__ through hostile object mutation.


class ProvenanceReasonV1(StrEnum):
    BAD_MAGIC = "bad_magic"
    TRUNCATED = "truncated"
    TRAILING_BYTES = "trailing_bytes"
    UNKNOWN_RELEASE = "unknown_release"
    UNKNOWN_ENUM = "unknown_enum"
    INVALID_FIELD = "invalid_field"
    INVALID_DIGEST = "invalid_digest"
    NONCANONICAL_ORDER = "noncanonical_order"
    DUPLICATE_PATH = "duplicate_path"
    CASE_COLLISION = "case_collision"
    ABSOLUTE_PATH = "absolute_path"
    UNSAFE_PATH = "unsafe_path"
    UNSAFE_LINK = "unsafe_link"
    UNSAFE_MEMBER_TYPE = "unsafe_member_type"
    UNSAFE_MODE = "unsafe_mode"
    ARCHIVE_LENGTH_MISMATCH = "archive_length_mismatch"
    ARCHIVE_DIGEST_MISMATCH = "archive_digest_mismatch"
    DECOMPRESSION_FAILED = "decompression_failed"
    TAR_STREAM_LENGTH_MISMATCH = "tar_stream_length_mismatch"
    TRAILING_COMPRESSED_DATA = "trailing_compressed_data"
    NONCANONICAL_TAR = "noncanonical_tar"
    ROOT_MISMATCH = "root_mismatch"
    FILE_COUNT_MISMATCH = "file_count_mismatch"
    FILE_BYTES_MISMATCH = "file_bytes_mismatch"
    FILE_CONTENT_MISMATCH = "file_content_mismatch"
    LEGAL_FILES_MISMATCH = "legal_files_mismatch"
    CONTENT_RELATION_MISMATCH = "content_relation_mismatch"
    FOREIGN_BINDING = "foreign_binding"
    INTEGRITY_KIND_MISMATCH = "integrity_kind_mismatch"


@dataclass(frozen=True)
class ProvenanceErrorV1(ValueError):
    artifact: str
    reason: ProvenanceReasonV1
    detail: str

    def __str__(self) -> str:
        return f"{self.artifact}: {self.reason}: {self.detail}"


def _fail(artifact: str, reason: ProvenanceReasonV1, detail: str) -> NoReturn:
    raise ProvenanceErrorV1(artifact, reason, detail)


def _identity(label: bytes, encoded: bytes) -> bytes:
    return hashlib.sha256(label + len(encoded).to_bytes(8, "big") + encoded).digest()


def _digest(value: bytes, artifact: str, field_name: str, length: int = SHA256_BYTES) -> bytes:
    if type(value) is not bytes or len(value) != length or value == bytes(length):
        _fail(artifact, ProvenanceReasonV1.INVALID_DIGEST, f"invalid {field_name}")
    return value


def _positive(value: int, artifact: str, field_name: str) -> int:
    if type(value) is not int or value <= 0 or value >= 1 << 64:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"invalid {field_name}")
    return value


def _ascii(value: str, artifact: str, field_name: str, maximum: int) -> bytes:
    if type(value) is not str or not value or "\0" in value:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"invalid {field_name}")
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"non-ASCII {field_name}")
    if any(byte < 0x20 or byte == 0x7F for byte in encoded):
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"control byte in {field_name}")
    if len(encoded) > maximum:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"oversized {field_name}")
    return encoded


def _relative_path(value: str, artifact: str, field_name: str) -> bytes:
    encoded = _ascii(value, artifact, field_name, 4096)
    if value.startswith("/"):
        _fail(artifact, ProvenanceReasonV1.ABSOLUTE_PATH, f"absolute {field_name}")
    if "\\" in value:
        _fail(artifact, ProvenanceReasonV1.UNSAFE_PATH, f"backslash in {field_name}")
    parts = value.split("/")
    if not parts or any(part in ("", ".", "..") for part in parts):
        _fail(artifact, ProvenanceReasonV1.UNSAFE_PATH, f"unsafe {field_name}")
    return encoded


def _root_prefix(value: str, artifact: str) -> bytes:
    if type(value) is not str or not value.endswith("/") or value.count("/") != 1:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, "root prefix is one directory")
    return _relative_path(value[:-1], artifact, "root_prefix") + b"/"


def _https_url(value: str, artifact: str, field_name: str) -> bytes:
    encoded = _ascii(value, artifact, field_name, 2048)
    try:
        parsed = urlsplit(value)
        hostname = parsed.hostname
        _ = parsed.port
    except ValueError:
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"malformed {field_name}")
    if (
        parsed.scheme != "https"
        or not hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or parsed.path in ("", "/")
    ):
        _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, f"noncanonical {field_name}")
    return encoded


def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(4, "big") + value


class _Reader:
    def __init__(self, data: bytes, artifact: str):
        if type(data) is not bytes:
            raise TypeError("canonical wire input must be bytes")
        self.data = data
        self.artifact = artifact
        self.offset = 0

    def exact(self, length: int) -> bytes:
        if length < 0 or self.offset + length > len(self.data):
            _fail(self.artifact, ProvenanceReasonV1.TRUNCATED, "wire is truncated")
        start = self.offset
        self.offset += length
        return self.data[start : self.offset]

    def u8(self) -> int:
        return self.exact(1)[0]

    def u16(self) -> int:
        return int.from_bytes(self.exact(2), "big")

    def u32(self) -> int:
        return int.from_bytes(self.exact(4), "big")

    def u64(self) -> int:
        return int.from_bytes(self.exact(8), "big")

    def blob(self, maximum: int) -> bytes:
        length = self.u32()
        if length == 0 or length > maximum:
            _fail(self.artifact, ProvenanceReasonV1.INVALID_FIELD, "invalid blob length")
        return self.exact(length)

    def text(self, maximum: int, field_name: str) -> str:
        raw = self.blob(maximum)
        try:
            return raw.decode("ascii")
        except UnicodeDecodeError:
            _fail(self.artifact, ProvenanceReasonV1.INVALID_FIELD, f"non-ASCII {field_name}")

    def finish(self) -> None:
        if self.offset != len(self.data):
            _fail(self.artifact, ProvenanceReasonV1.TRAILING_BYTES, "wire has trailing bytes")


class ArchiveFormatV1(IntEnum):
    TAR_XZ = 1
    TAR_GZIP = 2


class SourceRoleV1(IntEnum):
    GMP = 1
    MPFR = 2
    FLINT_ARB = 3
    MPFI = 4


class IntegrityKindV1(IntEnum):
    DETACHED_SIGNATURE = 1
    GIT_CONTENT_RELATION = 2
    PROJECT_PINNED_ARCHIVE_DIGEST = 3


@dataclass(frozen=True)
class LegalFileV1:
    path: str
    length: int
    sha256: bytes

    def __post_init__(self) -> None:
        _relative_path(self.path, "legal-file-v1", "path")
        _positive(self.length, "legal-file-v1", "length")
        _digest(self.sha256, "legal-file-v1", "sha256")

    def encode(self) -> bytes:
        return _blob(self.path.encode("ascii")) + self.length.to_bytes(8, "big") + self.sha256

    @classmethod
    def parse_from(cls, reader: _Reader) -> "LegalFileV1":
        return cls(reader.text(4096, "legal file path"), reader.u64(), reader.exact(SHA256_BYTES))


@dataclass(frozen=True)
class ProjectPinnedReleaseOnlyFileV1:
    path: str
    mode: int
    length: int
    sha256: bytes

    def __post_init__(self) -> None:
        _relative_path(self.path, "project-pinned-release-only-file-v1", "path")
        if type(self.mode) is not int or self.mode not in ALLOWED_REGULAR_MODES_V1:
            _fail(
                "project-pinned-release-only-file-v1",
                ProvenanceReasonV1.UNSAFE_MODE,
                "invalid mode",
            )
        _positive(self.length, "project-pinned-release-only-file-v1", "length")
        _digest(self.sha256, "project-pinned-release-only-file-v1", "sha256")

    def encode(self) -> bytes:
        return (
            _blob(self.path.encode("ascii"))
            + self.mode.to_bytes(4, "big")
            + self.length.to_bytes(8, "big")
            + self.sha256
        )

    @classmethod
    def parse_from(cls, reader: _Reader) -> "ProjectPinnedReleaseOnlyFileV1":
        return cls(
            reader.text(4096, "project-pinned release-only path"),
            reader.u32(),
            reader.u64(),
            reader.exact(SHA256_BYTES),
        )


@dataclass(frozen=True)
class DetachedSignaturePolicyV1:
    signature_url: str
    signature_length: int
    signature_sha256: bytes
    public_key_packets_sha256: bytes
    signer_fingerprint: bytes

    kind: IntegrityKindV1 = field(
        init=False,
        default=IntegrityKindV1.DETACHED_SIGNATURE,
    )

    def __post_init__(self) -> None:
        _https_url(self.signature_url, "signature-policy-v1", "signature_url")
        _positive(self.signature_length, "signature-policy-v1", "signature_length")
        _digest(self.signature_sha256, "signature-policy-v1", "signature_sha256")
        _digest(
            self.public_key_packets_sha256,
            "signature-policy-v1",
            "public_key_packets_sha256",
        )
        _digest(
            self.signer_fingerprint,
            "signature-policy-v1",
            "signer_fingerprint",
            OPENPGP_V4_FINGERPRINT_BYTES,
        )

    def encode_payload(self) -> bytes:
        return (
            _blob(self.signature_url.encode("ascii"))
            + self.signature_length.to_bytes(8, "big")
            + self.signature_sha256
            + self.public_key_packets_sha256
            + self.signer_fingerprint
        )

    @classmethod
    def parse_from(cls, reader: _Reader) -> "DetachedSignaturePolicyV1":
        return cls(
            reader.text(2048, "signature URL"),
            reader.u64(),
            reader.exact(SHA256_BYTES),
            reader.exact(SHA256_BYTES),
            reader.exact(OPENPGP_V4_FINGERPRINT_BYTES),
        )


@dataclass(frozen=True)
class GitContentRelationPolicyV1:
    repository_url: str
    tag: str
    commit: bytes
    tree: bytes
    common_file_count: int
    omitted_paths: tuple[str, ...]
    project_pinned_release_only_files: tuple[ProjectPinnedReleaseOnlyFileV1, ...]

    kind: IntegrityKindV1 = field(
        init=False,
        default=IntegrityKindV1.GIT_CONTENT_RELATION,
    )

    def __post_init__(self) -> None:
        artifact = "git-content-relation-policy-v1"
        _https_url(self.repository_url, artifact, "repository_url")
        _ascii(self.tag, artifact, "tag", 128)
        _digest(self.commit, artifact, "commit", SHA1_BYTES)
        _digest(self.tree, artifact, "tree", SHA1_BYTES)
        _positive(self.common_file_count, artifact, "common_file_count")
        if (
            type(self.omitted_paths) is not tuple
            or not self.omitted_paths
            or len(self.omitted_paths) > 4096
        ):
            _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, "omission count")
        if (
            type(self.project_pinned_release_only_files) is not tuple
            or not self.project_pinned_release_only_files
            or len(self.project_pinned_release_only_files) > 4096
        ):
            _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, "release-only file count")
        for path in self.omitted_paths:
            _relative_path(path, artifact, "omitted path")
        if self.omitted_paths != tuple(sorted(set(self.omitted_paths))):
            _fail(artifact, ProvenanceReasonV1.NONCANONICAL_ORDER, "omissions")
        if any(
            type(value) is not ProjectPinnedReleaseOnlyFileV1
            for value in self.project_pinned_release_only_files
        ):
            _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, "release-only file type")
        release_only_paths = tuple(
            value.path for value in self.project_pinned_release_only_files
        )
        if release_only_paths != tuple(sorted(set(release_only_paths))):
            _fail(
                artifact,
                ProvenanceReasonV1.NONCANONICAL_ORDER,
                "project-pinned release-only files",
            )
        if set(release_only_paths) & set(self.omitted_paths):
            _fail(artifact, ProvenanceReasonV1.INVALID_FIELD, "relation overlap")

    def encode_payload(self) -> bytes:
        chunks = [
            _blob(self.repository_url.encode("ascii")),
            _blob(self.tag.encode("ascii")),
            self.commit,
            self.tree,
            self.common_file_count.to_bytes(8, "big"),
            len(self.omitted_paths).to_bytes(4, "big"),
        ]
        chunks.extend(_blob(path.encode("ascii")) for path in self.omitted_paths)
        chunks.append(len(self.project_pinned_release_only_files).to_bytes(4, "big"))
        chunks.extend(value.encode() for value in self.project_pinned_release_only_files)
        return b"".join(chunks)

    @classmethod
    def parse_from(cls, reader: _Reader) -> "GitContentRelationPolicyV1":
        repository = reader.text(2048, "repository URL")
        tag = reader.text(128, "tag")
        commit = reader.exact(SHA1_BYTES)
        tree = reader.exact(SHA1_BYTES)
        common_file_count = reader.u64()
        omitted_count = reader.u32()
        if omitted_count == 0 or omitted_count > 4096:
            _fail(reader.artifact, ProvenanceReasonV1.INVALID_FIELD, "omission count")
        omitted = tuple(reader.text(4096, "omitted path") for _ in range(omitted_count))
        release_only_count = reader.u32()
        if release_only_count == 0 or release_only_count > 4096:
            _fail(reader.artifact, ProvenanceReasonV1.INVALID_FIELD, "release-only file count")
        release_only = tuple(
            ProjectPinnedReleaseOnlyFileV1.parse_from(reader)
            for _ in range(release_only_count)
        )
        return cls(repository, tag, commit, tree, common_file_count, omitted, release_only)


@dataclass(frozen=True)
class ProjectPinnedArchiveDigestPolicyV1:
    """State that the project pins archive bytes without upstream authentication.

    The digest and exact archive coordinates live in ``SourceReleaseLockV1``.
    This marker prevents an HTTPS download plus a project-chosen digest from
    being misreported as a publisher signature or a verified Git relation.
    """

    kind: IntegrityKindV1 = field(
        init=False,
        default=IntegrityKindV1.PROJECT_PINNED_ARCHIVE_DIGEST,
    )

    def encode_payload(self) -> bytes:
        return b""

    @classmethod
    def parse_from(cls, _reader: _Reader) -> ProjectPinnedArchiveDigestPolicyV1:
        return cls()


SourceIntegrityPolicyV1: TypeAlias = (
    DetachedSignaturePolicyV1
    | GitContentRelationPolicyV1
    | ProjectPinnedArchiveDigestPolicyV1
)


def _parse_integrity_policy(reader: _Reader) -> SourceIntegrityPolicyV1:
    kind_value = reader.u8()
    try:
        kind = IntegrityKindV1(kind_value)
    except ValueError:
        _fail(reader.artifact, ProvenanceReasonV1.UNKNOWN_ENUM, "integrity kind")
    if kind is IntegrityKindV1.DETACHED_SIGNATURE:
        return DetachedSignaturePolicyV1.parse_from(reader)
    if kind is IntegrityKindV1.GIT_CONTENT_RELATION:
        return GitContentRelationPolicyV1.parse_from(reader)
    if kind is IntegrityKindV1.PROJECT_PINNED_ARCHIVE_DIGEST:
        return ProjectPinnedArchiveDigestPolicyV1.parse_from(reader)
    _fail(reader.artifact, ProvenanceReasonV1.UNKNOWN_ENUM, "integrity kind")


@dataclass(frozen=True)
class SourceReleaseLockV1:
    role: SourceRoleV1
    version: str
    archive_url: str
    archive_format: ArchiveFormatV1
    archive_length: int
    archive_sha256: bytes
    tar_stream_length: int
    root_prefix: str
    regular_file_count: int
    regular_file_bytes: int
    legal_files: tuple[LegalFileV1, ...]
    integrity: SourceIntegrityPolicyV1

    def __post_init__(self) -> None:
        if type(self.role) is not SourceRoleV1 or type(self.archive_format) is not ArchiveFormatV1:
            _fail("source-release-lock-v1", ProvenanceReasonV1.UNKNOWN_ENUM, "role or format")
        _ascii(self.version, "source-release-lock-v1", "version", 128)
        _https_url(self.archive_url, "source-release-lock-v1", "archive_url")
        _positive(self.archive_length, "source-release-lock-v1", "archive_length")
        _digest(self.archive_sha256, "source-release-lock-v1", "archive_sha256")
        _positive(self.tar_stream_length, "source-release-lock-v1", "tar_stream_length")
        if self.tar_stream_length % TAR_BLOCK_BYTES:
            _fail("source-release-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "unaligned tar length")
        _root_prefix(self.root_prefix, "source-release-lock-v1")
        _positive(self.regular_file_count, "source-release-lock-v1", "regular_file_count")
        _positive(self.regular_file_bytes, "source-release-lock-v1", "regular_file_bytes")
        if (
            type(self.legal_files) is not tuple
            or not self.legal_files
            or len(self.legal_files) > 4096
        ):
            _fail("source-release-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "legal file count")
        if any(type(value) is not LegalFileV1 for value in self.legal_files):
            _fail("source-release-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "legal file type")
        paths = tuple(value.path for value in self.legal_files)
        if paths != tuple(sorted(set(paths))):
            _fail("source-release-lock-v1", ProvenanceReasonV1.NONCANONICAL_ORDER, "legal files")
        if type(self.integrity) not in (
            DetachedSignaturePolicyV1,
            GitContentRelationPolicyV1,
            ProjectPinnedArchiveDigestPolicyV1,
        ):
            _fail(
                "source-release-lock-v1",
                ProvenanceReasonV1.UNKNOWN_ENUM,
                "integrity policy",
            )
        if isinstance(self.integrity, GitContentRelationPolicyV1):
            if (
                self.integrity.common_file_count
                + len(self.integrity.project_pinned_release_only_files)
                != self.regular_file_count
            ):
                _fail(
                    "source-release-lock-v1",
                    ProvenanceReasonV1.CONTENT_RELATION_MISMATCH,
                    "common plus project-pinned release-only count does not cover archive",
                )

    def encode(self) -> bytes:
        chunks = [
            bytes((self.role,)),
            _blob(self.version.encode("ascii")),
            _blob(self.archive_url.encode("ascii")),
            bytes((self.archive_format,)),
            self.archive_length.to_bytes(8, "big"),
            self.archive_sha256,
            self.tar_stream_length.to_bytes(8, "big"),
            _blob(self.root_prefix.encode("ascii")),
            self.regular_file_count.to_bytes(8, "big"),
            self.regular_file_bytes.to_bytes(8, "big"),
            len(self.legal_files).to_bytes(2, "big"),
        ]
        chunks.extend(value.encode() for value in self.legal_files)
        chunks.append(bytes((self.integrity.kind,)))
        chunks.append(self.integrity.encode_payload())
        return b"".join(chunks)

    @classmethod
    def parse_from(cls, reader: _Reader) -> "SourceReleaseLockV1":
        role_value = reader.u8()
        try:
            role = SourceRoleV1(role_value)
        except ValueError:
            _fail(reader.artifact, ProvenanceReasonV1.UNKNOWN_ENUM, "source role")
        version = reader.text(128, "version")
        archive_url = reader.text(2048, "archive URL")
        archive_format_value = reader.u8()
        try:
            archive_format = ArchiveFormatV1(archive_format_value)
        except ValueError:
            _fail(reader.artifact, ProvenanceReasonV1.UNKNOWN_ENUM, "archive format")
        archive_length = reader.u64()
        archive_sha256 = reader.exact(SHA256_BYTES)
        tar_stream_length = reader.u64()
        root_prefix = reader.text(4096, "root prefix")
        regular_file_count = reader.u64()
        regular_file_bytes = reader.u64()
        legal_file_count = reader.u16()
        if legal_file_count == 0 or legal_file_count > 4096:
            _fail(reader.artifact, ProvenanceReasonV1.INVALID_FIELD, "legal file count")
        legal_files = tuple(
            LegalFileV1.parse_from(reader) for _ in range(legal_file_count)
        )
        integrity = _parse_integrity_policy(reader)
        return cls(
            role,
            version,
            archive_url,
            archive_format,
            archive_length,
            archive_sha256,
            tar_stream_length,
            root_prefix,
            regular_file_count,
            regular_file_bytes,
            legal_files,
            integrity,
        )

    @classmethod
    def parse(cls, data: bytes) -> SourceReleaseLockV1:
        """Rebuild one source declaration before it crosses a replay boundary."""

        reader = _Reader(data, "source-release-lock-v1")
        result = cls.parse_from(reader)
        reader.finish()
        if result.encode() != data:
            _fail(
                "source-release-lock-v1",
                ProvenanceReasonV1.FOREIGN_BINDING,
                "re-encode drift",
            )
        return result

    @property
    def identity(self) -> bytes:
        return _identity(b"labcolors.proof-region.source-release-lock.v1\0", self.encode())


_SourceClosureV1: TypeAlias = tuple[
    SourceReleaseLockV1,
    SourceReleaseLockV1,
    SourceReleaseLockV1,
]


def _encode_source_closure_v1(sources: _SourceClosureV1) -> bytes:
    return (
        SOURCE_LOCK_MAGIC_V1
        + bytes((SOURCE_LOCK_RELEASE_V1, len(sources)))
        + b"".join(source.encode() for source in sources)
    )


def _parse_source_closure_v1(data: bytes, artifact: str) -> _SourceClosureV1:
    reader = _Reader(data, artifact)
    if reader.exact(len(SOURCE_LOCK_MAGIC_V1)) != SOURCE_LOCK_MAGIC_V1:
        _fail(reader.artifact, ProvenanceReasonV1.BAD_MAGIC, "source lock magic")
    if reader.u8() != SOURCE_LOCK_RELEASE_V1:
        _fail(reader.artifact, ProvenanceReasonV1.UNKNOWN_RELEASE, "source lock release")
    if reader.u8() != SOURCE_CLOSURE_COUNT_V1:
        _fail(reader.artifact, ProvenanceReasonV1.INVALID_FIELD, "source count")
    result = (
        SourceReleaseLockV1.parse_from(reader),
        SourceReleaseLockV1.parse_from(reader),
        SourceReleaseLockV1.parse_from(reader),
    )
    reader.finish()
    return result


@dataclass(frozen=True)
class ArbSourceLockV1:
    sources: _SourceClosureV1

    def __post_init__(self) -> None:
        if type(self.sources) is not tuple or len(self.sources) != SOURCE_CLOSURE_COUNT_V1:
            _fail("arb-source-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "source count")
        if any(type(value) is not SourceReleaseLockV1 for value in self.sources):
            _fail("arb-source-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "source type")
        if tuple(value.role for value in self.sources) != (
            SourceRoleV1.GMP,
            SourceRoleV1.MPFR,
            SourceRoleV1.FLINT_ARB,
        ):
            _fail("arb-source-lock-v1", ProvenanceReasonV1.NONCANONICAL_ORDER, "GMP, MPFR, FLINT")
        if any(
            not isinstance(value.integrity, DetachedSignaturePolicyV1)
            for value in self.sources[:2]
        ) or not isinstance(
            self.sources[2].integrity,
            GitContentRelationPolicyV1,
        ):
            _fail(
                "arb-source-lock-v1",
                ProvenanceReasonV1.INTEGRITY_KIND_MISMATCH,
                "integrity policy",
            )

    def encode(self) -> bytes:
        return _encode_source_closure_v1(self.sources)

    @classmethod
    def parse(cls, data: bytes) -> ArbSourceLockV1:
        result = cls(_parse_source_closure_v1(data, "arb-source-lock-v1"))
        if result.encode() != data:
            _fail("arb-source-lock-v1", ProvenanceReasonV1.FOREIGN_BINDING, "re-encode drift")
        return result

    @property
    def identity(self) -> bytes:
        return _identity(SOURCE_LOCK_ID_LABEL_V1, self.encode())


@dataclass(frozen=True)
class MpfiSourceLockV1:
    sources: _SourceClosureV1

    def __post_init__(self) -> None:
        if type(self.sources) is not tuple or len(self.sources) != SOURCE_CLOSURE_COUNT_V1:
            _fail("mpfi-source-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "source count")
        if any(type(value) is not SourceReleaseLockV1 for value in self.sources):
            _fail("mpfi-source-lock-v1", ProvenanceReasonV1.INVALID_FIELD, "source type")
        if tuple(value.role for value in self.sources) != (
            SourceRoleV1.GMP,
            SourceRoleV1.MPFR,
            SourceRoleV1.MPFI,
        ):
            _fail(
                "mpfi-source-lock-v1",
                ProvenanceReasonV1.NONCANONICAL_ORDER,
                "GMP, MPFR, MPFI",
            )
        if any(
            not isinstance(value.integrity, DetachedSignaturePolicyV1)
            for value in self.sources[:2]
        ) or not isinstance(
            self.sources[2].integrity,
            ProjectPinnedArchiveDigestPolicyV1,
        ):
            _fail(
                "mpfi-source-lock-v1",
                ProvenanceReasonV1.INTEGRITY_KIND_MISMATCH,
                "integrity policy",
            )

    def encode(self) -> bytes:
        return _encode_source_closure_v1(self.sources)

    @classmethod
    def parse(cls, data: bytes) -> MpfiSourceLockV1:
        result = cls(_parse_source_closure_v1(data, "mpfi-source-lock-v1"))
        if result.encode() != data:
            _fail("mpfi-source-lock-v1", ProvenanceReasonV1.FOREIGN_BINDING, "re-encode drift")
        return result

    @property
    def identity(self) -> bytes:
        return _identity(SOURCE_LOCK_ID_LABEL_V1, self.encode())


def _rebuild_legal_file_v1(value: object) -> LegalFileV1:
    if type(value) is not LegalFileV1:
        raise TypeError("legal file must be LegalFileV1")
    return LegalFileV1(value.path, value.length, value.sha256)


def _rebuild_project_pinned_release_only_file_v1(
    value: object,
) -> ProjectPinnedReleaseOnlyFileV1:
    if type(value) is not ProjectPinnedReleaseOnlyFileV1:
        raise TypeError("release-only file must be ProjectPinnedReleaseOnlyFileV1")
    return ProjectPinnedReleaseOnlyFileV1(
        value.path,
        value.mode,
        value.length,
        value.sha256,
    )


def _rebuild_integrity_policy_v1(value: object) -> SourceIntegrityPolicyV1:
    """Copies only primitive policy coordinates; never dispatches caller methods."""

    if type(value) is DetachedSignaturePolicyV1:
        return DetachedSignaturePolicyV1(
            value.signature_url,
            value.signature_length,
            value.signature_sha256,
            value.public_key_packets_sha256,
            value.signer_fingerprint,
        )
    if type(value) is GitContentRelationPolicyV1:
        omitted_paths = value.omitted_paths
        release_only = value.project_pinned_release_only_files
        if type(omitted_paths) is not tuple or type(release_only) is not tuple:
            raise TypeError("git policy collections must be exact tuples")
        return GitContentRelationPolicyV1(
            value.repository_url,
            value.tag,
            value.commit,
            value.tree,
            value.common_file_count,
            tuple(omitted_paths),
            tuple(
                _rebuild_project_pinned_release_only_file_v1(item)
                for item in release_only
            ),
        )
    if type(value) is ProjectPinnedArchiveDigestPolicyV1:
        return ProjectPinnedArchiveDigestPolicyV1()
    raise TypeError("unknown source integrity policy")


def _rebuild_source_release_lock_v1(expected: object) -> SourceReleaseLockV1:
    """Makes a fresh lock before a boundary can derive an authority identity."""

    if type(expected) is not SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    legal_files = expected.legal_files
    if type(legal_files) is not tuple:
        raise TypeError("legal files must be an exact tuple")
    return SourceReleaseLockV1(
        expected.role,
        expected.version,
        expected.archive_url,
        expected.archive_format,
        expected.archive_length,
        expected.archive_sha256,
        expected.tar_stream_length,
        expected.root_prefix,
        expected.regular_file_count,
        expected.regular_file_bytes,
        tuple(_rebuild_legal_file_v1(item) for item in legal_files),
        _rebuild_integrity_policy_v1(expected.integrity),
    )


def _rebuild_source_closure_lock_v1(
    expected: object,
) -> ArbSourceLockV1 | MpfiSourceLockV1:
    if type(expected) not in (ArbSourceLockV1, MpfiSourceLockV1):
        raise TypeError("expected must be an exact three-source lock")
    sources = expected.sources
    if type(sources) is not tuple or len(sources) != SOURCE_CLOSURE_COUNT_V1:
        raise TypeError("source closure must be an exact three-source tuple")
    first, second, third = sources
    rebuilt = (
        _rebuild_source_release_lock_v1(first),
        _rebuild_source_release_lock_v1(second),
        _rebuild_source_release_lock_v1(third),
    )
    if type(expected) is ArbSourceLockV1:
        return ArbSourceLockV1(rebuilt)
    return MpfiSourceLockV1(rebuilt)


@dataclass(frozen=True)
class ArchiveFileV1:
    path: str
    mode: int
    length: int
    sha256: bytes


_SAFE_ARCHIVE_TOKEN = object()
_ADMITTED_ARB_SOURCES_TOKEN = object()
_ADMITTED_MPFI_SOURCES_TOKEN = object()
_REPLAYED_SOURCE_MATERIALIZATION_TOKEN = object()
_REPLAYED_SOURCE_CLOSURE_TOKEN = object()


@dataclass(frozen=True, init=False)
class SafeSourceArchiveV1:
    """Owned structural capability; it is neither origin nor build evidence.

    A materializer must consume archive_bytes from this value, never reopen a
    pathname, and derive normalized directories from admitted regular files.
    Empty archive directories intentionally carry no tree semantics.
    """

    source_lock_identity: bytes
    archive_sha256: bytes
    tree_identity: bytes
    regular_file_count: int
    regular_file_bytes: int
    files: tuple[ArchiveFileV1, ...]
    _archive_bytes: bytes = field(repr=False, compare=False)

    def __init__(
        self,
        source_lock_identity: bytes,
        archive_sha256: bytes,
        tree_identity: bytes,
        regular_file_count: int,
        regular_file_bytes: int,
        files: tuple[ArchiveFileV1, ...],
        archive_bytes: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _SAFE_ARCHIVE_TOKEN:
            raise TypeError("SafeSourceArchiveV1 is created only by archive admission")
        object.__setattr__(self, "source_lock_identity", source_lock_identity)
        object.__setattr__(self, "archive_sha256", archive_sha256)
        object.__setattr__(self, "tree_identity", tree_identity)
        object.__setattr__(self, "regular_file_count", regular_file_count)
        object.__setattr__(self, "regular_file_bytes", regular_file_bytes)
        object.__setattr__(self, "files", files)
        object.__setattr__(self, "_archive_bytes", archive_bytes)

    @property
    def archive_bytes(self) -> bytes:
        """Return the immutable snapshot admitted by this capability."""

        return self._archive_bytes


@dataclass(frozen=True, init=False)
class ReplayedSourceMaterializationV1:
    """One private replay snapshot: lock, archive metadata and file bytes move together.

    Public callers may mutate a nominally frozen input after admission.  This
    value therefore owns a freshly parsed lock and re-admitted archive before
    any downstream identity or USTAR layout is derived from it.
    """

    source_lock: SourceReleaseLockV1
    source: SafeSourceArchiveV1
    files: tuple[tuple[str, int, bytes], ...]

    def __init__(
        self,
        source_lock: SourceReleaseLockV1,
        source: SafeSourceArchiveV1,
        files: tuple[tuple[str, int, bytes], ...],
        *,
        _token: object,
    ) -> None:
        if _token is not _REPLAYED_SOURCE_MATERIALIZATION_TOKEN:
            raise TypeError(
                "ReplayedSourceMaterializationV1 is created only by source replay"
            )
        if (
            type(source_lock) is not SourceReleaseLockV1
            or type(source) is not SafeSourceArchiveV1
            or type(files) is not tuple
            or not files
            or any(
                type(path) is not str
                or type(mode) is not int
                or type(contents) is not bytes
                for path, mode, contents in files
            )
        ):
            raise TypeError("invalid replayed source materialization")
        object.__setattr__(self, "source_lock", source_lock)
        object.__setattr__(self, "source", source)
        object.__setattr__(self, "files", files)


def archive_file_manifest_bytes_v1(
    files_value: tuple[ArchiveFileV1, ...],
) -> bytes:
    """Encode the one canonical retained-file manifest owned by provenance."""

    if type(files_value) is not tuple or any(
        type(item) is not ArchiveFileV1 for item in files_value
    ):
        raise TypeError("invalid archive file manifest")
    paths = tuple(item.path for item in files_value)
    if (
        any(type(path) is not str for path in paths)
        or paths != tuple(sorted(paths))
        or len(paths) != len(set(paths))
        or len(paths) != len({path.lower() for path in paths})
    ):
        raise TypeError("invalid archive file manifest")
    chunks: list[bytes] = [len(files_value).to_bytes(8, "big")]
    for item in files_value:
        path = _relative_path(item.path, "archive-file-manifest-v1", "path")
        if (
            type(item.mode) is not int
            or item.mode not in ALLOWED_REGULAR_MODES_V1
            or type(item.length) is not int
            or item.length < 0
            or item.length >= 1 << 64
        ):
            raise TypeError("invalid archive file coordinate")
        _digest(item.sha256, "archive-file-manifest-v1", "sha256")
        chunks.extend(
            (
                path,
                item.mode.to_bytes(4, "big"),
                item.length.to_bytes(8, "big"),
                item.sha256,
            )
        )
    return b"".join(_blob(chunk) for chunk in chunks)


def _source_archive_coordinates_from_replayed_v1(
    source_lock: SourceReleaseLockV1,
    replayed: SafeSourceArchiveV1,
) -> tuple[bytes, ...]:
    """Encode the sole coordinate tuple shared by replay and owned snapshots.

    This is deliberately a leaf: callers establish whether their snapshot is
    fresh or retained.  Keeping only the wire projection here prevents those
    two ownership paths from quietly acquiring different source identities.
    """

    if (
        type(source_lock) is not SourceReleaseLockV1
        or type(replayed) is not SafeSourceArchiveV1
    ):
        raise TypeError("invalid replayed source snapshot")
    archive = replayed.archive_bytes
    if type(archive) is not bytes:
        raise TypeError("invalid replayed source archive")
    manifest = archive_file_manifest_bytes_v1(replayed.files)
    return (
        bytes((int(source_lock.role),)),
        source_lock.encode(),
        replayed.source_lock_identity,
        replayed.archive_sha256,
        replayed.tree_identity,
        replayed.regular_file_count.to_bytes(8, "big"),
        replayed.regular_file_bytes.to_bytes(8, "big"),
        manifest,
        len(archive).to_bytes(8, "big"),
        replayed.archive_sha256,
    )


def source_archive_replay_coordinates_v1(
    expected: SourceReleaseLockV1,
    admitted: SafeSourceArchiveV1,
) -> tuple[bytes, ...]:
    """Replay coordinates without creating separate extracted file-byte buffers."""

    source_lock, replayed, _raw_tar = _replay_admitted_source_archive_snapshot_v1(
        expected,
        admitted,
    )
    return _source_archive_coordinates_from_replayed_v1(source_lock, replayed)


def _materialized_source_coordinates_v1(
    value: ReplayedSourceMaterializationV1,
) -> tuple[bytes, ...]:
    """Encode coordinates already owned by one operation without replaying it."""

    if type(value) is not ReplayedSourceMaterializationV1:
        raise TypeError("value must be ReplayedSourceMaterializationV1")
    return _source_archive_coordinates_from_replayed_v1(
        value.source_lock,
        value.source,
    )


_SafeSourceClosureV1: TypeAlias = tuple[
    SafeSourceArchiveV1,
    SafeSourceArchiveV1,
    SafeSourceArchiveV1,
]


def _validate_admitted_source_closure_v1(
    source_lock_identity: bytes,
    sources: _SafeSourceClosureV1,
    artifact: str,
) -> None:
    _digest(source_lock_identity, artifact, "source_lock_identity")
    if (
        type(sources) is not tuple
        or len(sources) != SOURCE_CLOSURE_COUNT_V1
        or any(type(source) is not SafeSourceArchiveV1 for source in sources)
    ):
        raise TypeError(f"invalid admitted source tuple for {artifact}")


def _admitted_source_closure_identity_v1(
    label: bytes,
    source_lock_identity: bytes,
    sources: _SafeSourceClosureV1,
) -> bytes:
    chunks = [source_lock_identity]
    for ordinal, source in enumerate(sources):
        chunks.extend(
            (
                bytes((ordinal,)),
                source.source_lock_identity,
                source.archive_sha256,
                source.tree_identity,
            )
        )
    return _identity(label, b"".join(chunks))


@dataclass(frozen=True, init=False)
class AdmittedArbSourcesV1:
    """One ordered capability for the complete locked Arb dependency closure."""

    source_lock_identity: bytes
    sources: _SafeSourceClosureV1

    def __init__(
        self,
        source_lock_identity: bytes,
        sources: _SafeSourceClosureV1,
        *,
        _token: object,
    ) -> None:
        if _token is not _ADMITTED_ARB_SOURCES_TOKEN:
            raise TypeError("AdmittedArbSourcesV1 is created only by source admission")
        _validate_admitted_source_closure_v1(
            source_lock_identity,
            sources,
            "admitted-arb-sources-v1",
        )
        object.__setattr__(self, "source_lock_identity", source_lock_identity)
        object.__setattr__(self, "sources", sources)

    @property
    def identity(self) -> bytes:
        return _admitted_source_closure_identity_v1(
            ADMITTED_ARB_SOURCES_ID_LABEL_V1,
            self.source_lock_identity,
            self.sources,
        )


@dataclass(frozen=True, init=False)
class AdmittedMpfiSourcesV1:
    """One ordered capability for the complete locked MPFI dependency closure."""

    source_lock_identity: bytes
    sources: _SafeSourceClosureV1

    def __init__(
        self,
        source_lock_identity: bytes,
        sources: _SafeSourceClosureV1,
        *,
        _token: object,
    ) -> None:
        if _token is not _ADMITTED_MPFI_SOURCES_TOKEN:
            raise TypeError("AdmittedMpfiSourcesV1 is created only by source admission")
        _validate_admitted_source_closure_v1(
            source_lock_identity,
            sources,
            "admitted-mpfi-sources-v1",
        )
        object.__setattr__(self, "source_lock_identity", source_lock_identity)
        object.__setattr__(self, "sources", sources)

    @property
    def identity(self) -> bytes:
        return _admitted_source_closure_identity_v1(
            ADMITTED_MPFI_SOURCES_ID_LABEL_V1,
            self.source_lock_identity,
            self.sources,
        )


@dataclass(frozen=True, init=False)
class ReplayedSourceClosureV1:
    """One operation-owned three-source snapshot without caller-held refs."""

    source_lock: ArbSourceLockV1 | MpfiSourceLockV1
    admitted_sources: AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1
    sources: tuple[
        ReplayedSourceMaterializationV1,
        ReplayedSourceMaterializationV1,
        ReplayedSourceMaterializationV1,
    ]

    def __init__(
        self,
        source_lock: ArbSourceLockV1 | MpfiSourceLockV1,
        admitted_sources: AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1,
        sources: tuple[
            ReplayedSourceMaterializationV1,
            ReplayedSourceMaterializationV1,
            ReplayedSourceMaterializationV1,
        ],
        *,
        _token: object,
    ) -> None:
        if _token is not _REPLAYED_SOURCE_CLOSURE_TOKEN:
            raise TypeError("ReplayedSourceClosureV1 is created only by closure replay")
        if (
            type(source_lock) not in (ArbSourceLockV1, MpfiSourceLockV1)
            or type(admitted_sources)
            not in (AdmittedArbSourcesV1, AdmittedMpfiSourcesV1)
            or type(sources) is not tuple
            or len(sources) != SOURCE_CLOSURE_COUNT_V1
            or any(type(source) is not ReplayedSourceMaterializationV1 for source in sources)
            or tuple(source.source for source in sources) != admitted_sources.sources
        ):
            raise TypeError("invalid replayed source closure")
        object.__setattr__(self, "source_lock", source_lock)
        object.__setattr__(self, "admitted_sources", admitted_sources)
        object.__setattr__(self, "sources", sources)


def _decompress_exact(
    archive: bytes,
    archive_format: ArchiveFormatV1,
    expected_length: int,
) -> bytes:
    try:
        if archive_format is ArchiveFormatV1.TAR_GZIP:
            decompressor = zlib.decompressobj(16 + zlib.MAX_WBITS)
            output = decompressor.decompress(archive, expected_length + 1)
            if len(output) > expected_length:
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                    "expanded beyond lock",
                )
            while not decompressor.eof and decompressor.unconsumed_tail:
                remaining = expected_length + 1 - len(output)
                if remaining <= 0:
                    _fail(
                        "source-archive-v1",
                        ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                        "expanded beyond lock",
                    )
                output += decompressor.decompress(
                    decompressor.unconsumed_tail,
                    remaining,
                )
                if len(output) > expected_length:
                    break
            if not decompressor.eof:
                if len(output) > expected_length:
                    _fail(
                        "source-archive-v1",
                        ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                        "expanded beyond lock",
                    )
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.DECOMPRESSION_FAILED,
                    "truncated gzip stream",
                )
            if decompressor.unused_data:
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.TRAILING_COMPRESSED_DATA,
                    "concatenated or trailing gzip data",
                )
        else:
            decompressor_xz = lzma.LZMADecompressor(format=lzma.FORMAT_XZ)
            output = decompressor_xz.decompress(archive, max_length=expected_length + 1)
            if len(output) > expected_length:
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                    "expanded beyond lock",
                )
            while not decompressor_xz.eof and not decompressor_xz.needs_input:
                remaining = expected_length + 1 - len(output)
                if remaining <= 0:
                    _fail(
                        "source-archive-v1",
                        ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                        "expanded beyond lock",
                    )
                output += decompressor_xz.decompress(
                    b"", max_length=remaining
                )
                if len(output) > expected_length:
                    break
            if not decompressor_xz.eof:
                if len(output) > expected_length:
                    _fail(
                        "source-archive-v1",
                        ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                        "expanded beyond lock",
                    )
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.DECOMPRESSION_FAILED,
                    "truncated xz stream",
                )
            if decompressor_xz.unused_data:
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.TRAILING_COMPRESSED_DATA,
                    "concatenated or trailing xz data",
                )
    except (zlib.error, lzma.LZMAError, EOFError):
        _fail("source-archive-v1", ProvenanceReasonV1.DECOMPRESSION_FAILED, "invalid compressed stream")
    if len(output) != expected_length:
        _fail(
            "source-archive-v1",
            ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
            "tar stream length",
        )
    return output


def _tree_identity(files: tuple[ArchiveFileV1, ...]) -> bytes:
    chunks = [len(files).to_bytes(8, "big")]
    for item in files:
        chunks.extend(
            (
                _blob(item.path.encode("ascii")),
                item.mode.to_bytes(4, "big"),
                item.length.to_bytes(8, "big"),
                item.sha256,
            )
        )
    encoded = b"".join(chunks)
    return _identity(SOURCE_TREE_ID_LABEL_V1, encoded)


def _scan_tar(expected: SourceReleaseLockV1, raw_tar: bytes) -> tuple[ArchiveFileV1, ...]:
    files: list[ArchiveFileV1] = []
    seen: set[str] = set()
    folded: set[str] = set()
    directories: set[str] = set()
    root = expected.root_prefix[:-1]
    last_payload_end = 0
    admitted_file_bytes = 0
    try:
        with tarfile.open(fileobj=io.BytesIO(raw_tar), mode="r:") as archive:
            if archive.pax_headers:
                _fail("source-archive-v1", ProvenanceReasonV1.NONCANONICAL_TAR, "global pax headers")
            for member in archive:
                if member.pax_headers:
                    _fail("source-archive-v1", ProvenanceReasonV1.NONCANONICAL_TAR, "member pax headers")
                name = member.name
                try:
                    name.encode("ascii")
                except UnicodeEncodeError:
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_PATH, "non-ASCII member")
                if name.startswith("/"):
                    _fail("source-archive-v1", ProvenanceReasonV1.ABSOLUTE_PATH, name)
                if "\\" in name or any(part in ("", ".", "..") for part in name.split("/")):
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_PATH, name)
                if name in seen:
                    _fail("source-archive-v1", ProvenanceReasonV1.DUPLICATE_PATH, name)
                casefolded = name.lower()
                if casefolded in folded:
                    _fail("source-archive-v1", ProvenanceReasonV1.CASE_COLLISION, name)
                seen.add(name)
                folded.add(casefolded)
                last_payload_end = max(
                    last_payload_end,
                    member.offset_data + ((member.size + TAR_BLOCK_BYTES - 1) // TAR_BLOCK_BYTES) * TAR_BLOCK_BYTES,
                )
                if member.issym() or member.islnk():
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_LINK, name)
                if not (member.isdir() or member.isreg()):
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_MEMBER_TYPE, name)
                if member.isdir():
                    if member.mode != ALLOWED_DIRECTORY_MODE_V1:
                        _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_MODE, name)
                    if name != root and not name.startswith(expected.root_prefix):
                        _fail("source-archive-v1", ProvenanceReasonV1.ROOT_MISMATCH, name)
                    parent = str(PurePosixPath(name).parent)
                    if name != root and parent not in directories:
                        _fail(
                            "source-archive-v1",
                            ProvenanceReasonV1.UNSAFE_PATH,
                            f"undeclared parent of {name}",
                        )
                    directories.add(name)
                    continue
                if not name.startswith(expected.root_prefix):
                    _fail("source-archive-v1", ProvenanceReasonV1.ROOT_MISMATCH, name)
                relative = name[len(expected.root_prefix) :]
                _relative_path(relative, "source-archive-v1", "member path")
                parent = str(PurePosixPath(name).parent)
                if parent not in directories:
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_PATH, f"undeclared parent of {name}")
                if member.mode not in ALLOWED_REGULAR_MODES_V1:
                    _fail("source-archive-v1", ProvenanceReasonV1.UNSAFE_MODE, name)
                if len(files) >= expected.regular_file_count:
                    _fail("source-archive-v1", ProvenanceReasonV1.FILE_COUNT_MISMATCH, "too many files")
                if member.size > expected.regular_file_bytes - admitted_file_bytes:
                    _fail("source-archive-v1", ProvenanceReasonV1.FILE_BYTES_MISMATCH, "declared bytes exceed lock")
                stream = archive.extractfile(member)
                if stream is None:
                    _fail("source-archive-v1", ProvenanceReasonV1.FILE_CONTENT_MISMATCH, name)
                hasher = hashlib.sha256()
                length = 0
                while True:
                    chunk = stream.read(READ_CHUNK_BYTES)
                    if not chunk:
                        break
                    length += len(chunk)
                    if length > member.size:
                        _fail("source-archive-v1", ProvenanceReasonV1.FILE_CONTENT_MISMATCH, name)
                    hasher.update(chunk)
                if length != member.size:
                    _fail("source-archive-v1", ProvenanceReasonV1.FILE_CONTENT_MISMATCH, name)
                files.append(ArchiveFileV1(relative, member.mode, length, hasher.digest()))
                admitted_file_bytes += length
    except tarfile.TarError:
        _fail("source-archive-v1", ProvenanceReasonV1.NONCANONICAL_TAR, "invalid tar stream")
    if root not in directories:
        _fail("source-archive-v1", ProvenanceReasonV1.ROOT_MISMATCH, "missing root directory")
    trailing = raw_tar[last_payload_end:]
    if len(trailing) < TAR_END_MARKER_BYTES or any(trailing):
        _fail("source-archive-v1", ProvenanceReasonV1.NONCANONICAL_TAR, "nonzero or missing tar terminator")
    return tuple(sorted(files, key=lambda item: item.path))


def _admit_source_archive_once(
    expected: SourceReleaseLockV1,
    archive: bytes,
) -> tuple[SafeSourceArchiveV1, bytes]:

    if type(expected) is not SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    if type(archive) is not bytes:
        raise TypeError("archive must be owned bytes")
    if len(archive) != expected.archive_length:
        _fail("source-archive-v1", ProvenanceReasonV1.ARCHIVE_LENGTH_MISMATCH, "archive length")
    archive_sha256 = hashlib.sha256(archive).digest()
    if archive_sha256 != expected.archive_sha256:
        _fail("source-archive-v1", ProvenanceReasonV1.ARCHIVE_DIGEST_MISMATCH, "archive digest")
    raw_tar = _decompress_exact(archive, expected.archive_format, expected.tar_stream_length)
    files = _scan_tar(expected, raw_tar)
    if len(files) != expected.regular_file_count:
        _fail("source-archive-v1", ProvenanceReasonV1.FILE_COUNT_MISMATCH, "regular file count")
    total_bytes = sum(item.length for item in files)
    if total_bytes != expected.regular_file_bytes:
        _fail("source-archive-v1", ProvenanceReasonV1.FILE_BYTES_MISMATCH, "regular file bytes")
    by_path = {item.path: item for item in files}
    for legal_file in expected.legal_files:
        actual = by_path.get(legal_file.path)
        if (
            actual is None
            or actual.length != legal_file.length
            or actual.sha256 != legal_file.sha256
        ):
            _fail(
                "source-archive-v1",
                ProvenanceReasonV1.LEGAL_FILES_MISMATCH,
                legal_file.path,
            )
    if isinstance(expected.integrity, GitContentRelationPolicyV1):
        for path in expected.integrity.omitted_paths:
            if path in by_path:
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.CONTENT_RELATION_MISMATCH,
                    f"omitted path present: {path}",
                )
        for release_only in expected.integrity.project_pinned_release_only_files:
            actual = by_path.get(release_only.path)
            if (
                actual is None
                or actual.mode != release_only.mode
                or actual.length != release_only.length
                or actual.sha256 != release_only.sha256
            ):
                _fail(
                    "source-archive-v1",
                    ProvenanceReasonV1.CONTENT_RELATION_MISMATCH,
                    release_only.path,
                )
    tree_identity = _tree_identity(files)
    admitted = SafeSourceArchiveV1(
        expected.identity,
        archive_sha256,
        tree_identity,
        len(files),
        total_bytes,
        files,
        archive,
        _token=_SAFE_ARCHIVE_TOKEN,
    )
    return admitted, raw_tar


def admit_source_archive(expected: SourceReleaseLockV1, archive: bytes) -> SafeSourceArchiveV1:
    """Hash then scan one locked archive; this establishes no origin trust."""

    source_lock = _canonical_source_lock_for_replay_v1(expected)
    admitted, _raw_tar = _admit_source_archive_once(source_lock, archive)
    return admitted


def _canonical_source_lock_for_replay_v1(
    expected: SourceReleaseLockV1,
) -> SourceReleaseLockV1:
    if type(expected) is not SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    try:
        return _rebuild_source_release_lock_v1(expected)
    except (
        ProvenanceErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(
            "source-archive-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "invalid retained source lock",
        )


_RetainedSourceArchiveSnapshotV1: TypeAlias = tuple[
    bytes,
    bytes,
    bytes,
    int,
    int,
    bytes,
    bytes,
]


def _retained_source_archive_snapshot_v1(
    admitted: SafeSourceArchiveV1,
) -> _RetainedSourceArchiveSnapshotV1:
    """Copies only exact primitives before a replay can re-enter caller code."""

    if type(admitted) is not SafeSourceArchiveV1:
        raise TypeError("admitted must be SafeSourceArchiveV1")
    try:
        source_lock_identity = admitted.source_lock_identity
        archive_sha256 = admitted.archive_sha256
        tree_identity = admitted.tree_identity
        regular_file_count = admitted.regular_file_count
        regular_file_bytes = admitted.regular_file_bytes
        files = admitted.files
        archive = admitted.archive_bytes
        _digest(
            source_lock_identity,
            "source-archive-replay-v1",
            "source_lock_identity",
        )
        _digest(
            archive_sha256,
            "source-archive-replay-v1",
            "archive_sha256",
        )
        _digest(tree_identity, "source-archive-replay-v1", "tree_identity")
        _positive(
            regular_file_count,
            "source-archive-replay-v1",
            "regular_file_count",
        )
        _positive(
            regular_file_bytes,
            "source-archive-replay-v1",
            "regular_file_bytes",
        )
        if type(archive) is not bytes:
            raise TypeError("archive must be exact bytes")
        manifest = archive_file_manifest_bytes_v1(files)
    except (
        ProvenanceErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(
            "source-archive-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "invalid retained source capability",
        )
    return (
        source_lock_identity,
        archive_sha256,
        tree_identity,
        regular_file_count,
        regular_file_bytes,
        manifest,
        archive,
    )


def _replay_source_archive_from_retained_v1(
    source_lock: SourceReleaseLockV1,
    retained: _RetainedSourceArchiveSnapshotV1,
) -> tuple[SafeSourceArchiveV1, bytes]:
    """Re-admit one copied archive before extracting individual file-byte buffers."""

    (
        retained_source_lock_identity,
        retained_archive_sha256,
        retained_tree_identity,
        retained_file_count,
        retained_file_bytes,
        retained_manifest,
        archive,
    ) = retained

    try:
        replayed, raw_tar = _admit_source_archive_once(source_lock, archive)
        replayed_manifest = archive_file_manifest_bytes_v1(replayed.files)
    except ProvenanceErrorV1:
        raise
    except (AttributeError, TypeError, ValueError, OverflowError):
        _fail(
            "source-archive-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "archive replay failed",
        )
    if (
        retained_source_lock_identity != replayed.source_lock_identity
        or retained_archive_sha256 != replayed.archive_sha256
        or retained_tree_identity != replayed.tree_identity
        or retained_file_count != replayed.regular_file_count
        or retained_file_bytes != replayed.regular_file_bytes
        or retained_manifest != replayed_manifest
    ):
        _fail(
            "source-archive-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "retained source coordinates changed",
        )
    return replayed, raw_tar


def _replay_admitted_source_archive_snapshot_v1(
    expected: SourceReleaseLockV1,
    admitted: SafeSourceArchiveV1,
) -> tuple[SourceReleaseLockV1, SafeSourceArchiveV1, bytes]:
    """Makes the source-owned replay needed by metadata and body consumers."""

    source_lock = _canonical_source_lock_for_replay_v1(expected)
    replayed, raw_tar = _replay_source_archive_from_retained_v1(
        source_lock,
        _retained_source_archive_snapshot_v1(admitted),
    )
    return source_lock, replayed, raw_tar


def _materialize_replayed_source_files_v1(
    source_lock: SourceReleaseLockV1,
    replayed: SafeSourceArchiveV1,
    raw_tar: bytes,
) -> tuple[tuple[str, int, bytes], ...]:
    """Reads only one locally replayed archive and its canonical lock snapshot."""

    expected_by_path = {item.path: item for item in replayed.files}
    values: list[tuple[str, int, bytes]] = []
    seen: set[str] = set()
    try:
        with tarfile.open(fileobj=io.BytesIO(raw_tar), mode="r:") as archive:
            for member in archive:
                if member.isdir():
                    continue
                if not member.isreg() or not member.name.startswith(source_lock.root_prefix):
                    _fail(
                        "source-archive-materialization-v1",
                        ProvenanceReasonV1.FOREIGN_BINDING,
                        "unexpected archive member",
                    )
                relative = member.name[len(source_lock.root_prefix) :]
                coordinate = expected_by_path.get(relative)
                if (
                    coordinate is None
                    or relative in seen
                    or member.mode != coordinate.mode
                    or member.size != coordinate.length
                ):
                    _fail(
                        "source-archive-materialization-v1",
                        ProvenanceReasonV1.FOREIGN_BINDING,
                        "archive file set changed",
                    )
                stream = archive.extractfile(member)
                if stream is None:
                    _fail(
                        "source-archive-materialization-v1",
                        ProvenanceReasonV1.FILE_CONTENT_MISMATCH,
                        relative,
                    )
                chunks: list[bytes] = []
                length = 0
                hasher = hashlib.sha256()
                while True:
                    chunk = stream.read(READ_CHUNK_BYTES)
                    if not chunk:
                        break
                    length += len(chunk)
                    if length > coordinate.length:
                        _fail(
                            "source-archive-materialization-v1",
                            ProvenanceReasonV1.FILE_CONTENT_MISMATCH,
                            relative,
                        )
                    chunks.append(chunk)
                    hasher.update(chunk)
                if (
                    length != coordinate.length
                    or hasher.digest() != coordinate.sha256
                ):
                    _fail(
                        "source-archive-materialization-v1",
                        ProvenanceReasonV1.FILE_CONTENT_MISMATCH,
                        relative,
                    )
                values.append((relative, coordinate.mode, b"".join(chunks)))
                seen.add(relative)
    except ProvenanceErrorV1:
        raise
    except (OSError, tarfile.TarError, ValueError):
        _fail(
            "source-archive-materialization-v1",
            ProvenanceReasonV1.NONCANONICAL_TAR,
            "archive replay failed",
        )
    if seen != set(expected_by_path):
        _fail(
            "source-archive-materialization-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "archive file set is incomplete",
        )
    return tuple(sorted(values))


def replay_materialize_admitted_source_v1(
    expected: SourceReleaseLockV1,
    admitted: SafeSourceArchiveV1,
) -> ReplayedSourceMaterializationV1:
    """Builds one owned replay snapshot for all source-derived consumers.

    The snapshot is deliberately below engine and recipe layers.  Its lock,
    archive identity and materialized bytes originate from the same fresh
    replay, so a caller-held capability cannot relabel already-read bytes.
    """

    source_lock, replayed, raw_tar = _replay_admitted_source_archive_snapshot_v1(
        expected,
        admitted,
    )
    files = _materialize_replayed_source_files_v1(source_lock, replayed, raw_tar)
    return ReplayedSourceMaterializationV1(
        source_lock,
        replayed,
        files,
        _token=_REPLAYED_SOURCE_MATERIALIZATION_TOKEN,
    )


def replay_admitted_source_archive_v1(
    expected: SourceReleaseLockV1,
    admitted: SafeSourceArchiveV1,
) -> tuple[SafeSourceArchiveV1, bytes]:
    """Return a bounded-decompressed replay without extracted file-byte buffers."""

    _source_lock, replayed, raw_tar = _replay_admitted_source_archive_snapshot_v1(
        expected,
        admitted,
    )
    return replayed, raw_tar


def materialize_admitted_source_files_v1(
    expected: SourceReleaseLockV1,
    admitted: SafeSourceArchiveV1,
) -> tuple[tuple[str, int, bytes], ...]:
    """Return exact relative files from one owned replay snapshot."""

    return replay_materialize_admitted_source_v1(expected, admitted).files


def _source_closure_lock_snapshot_v1(
    expected: ArbSourceLockV1 | MpfiSourceLockV1,
) -> ArbSourceLockV1 | MpfiSourceLockV1:
    try:
        return _rebuild_source_closure_lock_v1(expected)
    except (
        ProvenanceErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(
            "source-closure-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "invalid retained source closure lock",
        )


def snapshot_source_closure_lock_v1(
    expected: object,
) -> ArbSourceLockV1 | MpfiSourceLockV1:
    """Return a detached structural lock snapshot before a public replay."""

    return _source_closure_lock_snapshot_v1(expected)


def _source_capability_matches_lock_v1(
    lock: SourceReleaseLockV1,
    source: SafeSourceArchiveV1,
) -> _RetainedSourceArchiveSnapshotV1:
    retained = _retained_source_archive_snapshot_v1(source)
    (
        retained_lock_identity,
        retained_archive_sha256,
        _retained_tree_identity,
        retained_file_count,
        retained_file_bytes,
        _retained_manifest,
        _archive,
    ) = retained
    if (
        retained_lock_identity != lock.identity
        or retained_archive_sha256 != lock.archive_sha256
        or retained_file_count != lock.regular_file_count
        or retained_file_bytes != lock.regular_file_bytes
    ):
        _fail(
            "source-closure-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "source capability does not match ordered lock",
        )
    return retained


def snapshot_admitted_source_closure_v1(
    expected: object,
    admitted: object,
) -> AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1:
    """Return a detached source-closure declaration without extracted file buffers.

    The archive is re-admitted so the retained manifest, tree and compressed
    bytes agree.  Re-admission bounded-decompresses and scans the tar, but
    unlike an operation replay it does not retain separate file-byte buffers;
    consumers that need those buffers must still call
    ``replay_admitted_source_closure_v1``.
    """

    canonical_lock = _source_closure_lock_snapshot_v1(expected)
    if (
        (
            type(canonical_lock) is ArbSourceLockV1
            and type(admitted) is not AdmittedArbSourcesV1
        )
        or (
            type(canonical_lock) is MpfiSourceLockV1
            and type(admitted) is not AdmittedMpfiSourcesV1
        )
    ):
        raise TypeError("admitted sources do not match the source lock kind")
    try:
        retained_lock_identity = admitted.source_lock_identity
        retained_sources = admitted.sources
        _digest(
            retained_lock_identity,
            "source-closure-snapshot-v1",
            "source_lock_identity",
        )
        if retained_lock_identity != canonical_lock.identity:
            _fail(
                "source-closure-snapshot-v1",
                ProvenanceReasonV1.FOREIGN_BINDING,
                "admitted closure lock identity changed",
            )
        sources = _validate_source_replay_arguments_v1(
            canonical_lock.sources,
            retained_sources,
        )
        snapshots = _replay_source_archives_v1(
            canonical_lock.sources,
            sources,
        )
    except ProvenanceErrorV1:
        raise
    except (
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(
            "source-closure-snapshot-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "invalid retained admitted closure",
        )
    return _fresh_admitted_source_closure_v1(canonical_lock, snapshots)


def _validate_source_replay_arguments_v1(
    expected_sources: _SourceClosureV1,
    sources: _SafeSourceClosureV1,
) -> tuple[SafeSourceArchiveV1, SafeSourceArchiveV1, SafeSourceArchiveV1]:
    if (
        type(expected_sources) is not tuple
        or len(expected_sources) != SOURCE_CLOSURE_COUNT_V1
        or any(type(lock) is not SourceReleaseLockV1 for lock in expected_sources)
    ):
        raise TypeError("expected sources must be three SourceReleaseLockV1 values")
    if (
        type(sources) is not tuple
        or len(sources) != SOURCE_CLOSURE_COUNT_V1
        or any(type(source) is not SafeSourceArchiveV1 for source in sources)
    ):
        raise TypeError("sources must be three SafeSourceArchiveV1 values")
    first, second, third = sources
    return first, second, third


def _replay_source_archives_v1(
    expected_sources: _SourceClosureV1,
    sources: _SafeSourceClosureV1,
) -> _SafeSourceClosureV1:
    """Re-admit a closure for metadata without retaining file-byte buffers."""

    admitted_sources = _validate_source_replay_arguments_v1(expected_sources, sources)
    replayed: list[SafeSourceArchiveV1] = []
    for lock, source in zip(expected_sources, admitted_sources, strict=True):
        retained = _source_capability_matches_lock_v1(lock, source)
        fresh, _raw_tar = _replay_source_archive_from_retained_v1(lock, retained)
        replayed.append(fresh)
    first, second, third = replayed
    return first, second, third


def _replay_source_materializations_v1(
    expected_sources: _SourceClosureV1,
    sources: _SafeSourceClosureV1,
) -> tuple[
    ReplayedSourceMaterializationV1,
    ReplayedSourceMaterializationV1,
    ReplayedSourceMaterializationV1,
]:
    """Materialize only the operation path that actually needs source bytes."""

    admitted_sources = _validate_source_replay_arguments_v1(expected_sources, sources)
    materializations: list[ReplayedSourceMaterializationV1] = []
    for lock, source in zip(expected_sources, admitted_sources, strict=True):
        retained = _source_capability_matches_lock_v1(lock, source)
        replayed, raw_tar = _replay_source_archive_from_retained_v1(lock, retained)
        materializations.append(
            ReplayedSourceMaterializationV1(
                lock,
                replayed,
                _materialize_replayed_source_files_v1(lock, replayed, raw_tar),
                _token=_REPLAYED_SOURCE_MATERIALIZATION_TOKEN,
            )
        )
    first, second, third = materializations
    return first, second, third


def _fresh_admitted_source_closure_v1(
    source_lock: ArbSourceLockV1 | MpfiSourceLockV1,
    sources: _SafeSourceClosureV1,
) -> AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1:
    if type(source_lock) is ArbSourceLockV1:
        return AdmittedArbSourcesV1(
            source_lock.identity,
            sources,
            _token=_ADMITTED_ARB_SOURCES_TOKEN,
        )
    if type(source_lock) is MpfiSourceLockV1:
        return AdmittedMpfiSourcesV1(
            source_lock.identity,
            sources,
            _token=_ADMITTED_MPFI_SOURCES_TOKEN,
        )
    raise TypeError("source lock is not a supported closure")


def _admitted_closure_snapshot_v1(
    source_lock: ArbSourceLockV1 | MpfiSourceLockV1,
    admitted: AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1,
) -> ReplayedSourceClosureV1:
    canonical_lock = _source_closure_lock_snapshot_v1(source_lock)
    if (
        (
            type(canonical_lock) is ArbSourceLockV1
            and type(admitted) is not AdmittedArbSourcesV1
        )
        or (
            type(canonical_lock) is MpfiSourceLockV1
            and type(admitted) is not AdmittedMpfiSourcesV1
        )
    ):
        raise TypeError("admitted sources do not match the source lock kind")
    try:
        retained_lock_identity = admitted.source_lock_identity
        retained_sources = admitted.sources
        _digest(
            retained_lock_identity,
            "source-closure-replay-v1",
            "source_lock_identity",
        )
        if retained_lock_identity != canonical_lock.identity:
            _fail(
                "source-closure-replay-v1",
                ProvenanceReasonV1.FOREIGN_BINDING,
                "admitted closure lock identity changed",
            )
        _validate_source_replay_arguments_v1(
            canonical_lock.sources,
            retained_sources,
        )
    except ProvenanceErrorV1:
        raise
    except (
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(
            "source-closure-replay-v1",
            ProvenanceReasonV1.FOREIGN_BINDING,
            "invalid retained admitted closure",
        )
    snapshots = _replay_source_materializations_v1(
        canonical_lock.sources,
        retained_sources,
    )
    fresh = _fresh_admitted_source_closure_v1(
        canonical_lock,
        tuple(snapshot.source for snapshot in snapshots),
    )
    return ReplayedSourceClosureV1(
        canonical_lock,
        fresh,
        snapshots,
        _token=_REPLAYED_SOURCE_CLOSURE_TOKEN,
    )


def admit_arb_sources(
    expected: ArbSourceLockV1,
    sources: _SafeSourceClosureV1,
) -> AdmittedArbSourcesV1:
    """Replay and own three archives before minting one Arb closure capability."""

    if type(expected) is not ArbSourceLockV1:
        raise TypeError("expected must be ArbSourceLockV1")
    canonical_lock = _source_closure_lock_snapshot_v1(expected)
    replayed_sources = _replay_source_archives_v1(
        canonical_lock.sources,
        sources,
    )
    fresh = _fresh_admitted_source_closure_v1(canonical_lock, replayed_sources)
    if type(fresh) is not AdmittedArbSourcesV1:
        raise AssertionError("Arb closure kind changed during admission")
    return fresh


def admit_mpfi_sources(
    expected: MpfiSourceLockV1,
    sources: _SafeSourceClosureV1,
) -> AdmittedMpfiSourcesV1:
    """Replay and own three archives before minting one MPFI closure capability."""

    if type(expected) is not MpfiSourceLockV1:
        raise TypeError("expected must be MpfiSourceLockV1")
    canonical_lock = _source_closure_lock_snapshot_v1(expected)
    replayed_sources = _replay_source_archives_v1(
        canonical_lock.sources,
        sources,
    )
    fresh = _fresh_admitted_source_closure_v1(canonical_lock, replayed_sources)
    if type(fresh) is not AdmittedMpfiSourcesV1:
        raise AssertionError("MPFI closure kind changed during admission")
    return fresh


def replay_admitted_source_closure_v1(
    expected: ArbSourceLockV1 | MpfiSourceLockV1,
    admitted: AdmittedArbSourcesV1 | AdmittedMpfiSourcesV1,
) -> ReplayedSourceClosureV1:
    """Take one local source-closure snapshot for a build-like operation."""

    return _admitted_closure_snapshot_v1(expected, admitted)


def _legal_file(path: str, length: int, digest_hex: str) -> LegalFileV1:
    return LegalFileV1(path, length, bytes.fromhex(digest_hex))


def _gmp_source_release_v1() -> SourceReleaseLockV1:
    return SourceReleaseLockV1(
        SourceRoleV1.GMP,
        "6.3.0",
        "https://ftp.gnu.org/gnu/gmp/gmp-6.3.0.tar.xz",
        ArchiveFormatV1.TAR_XZ,
        2_094_196,
        bytes.fromhex("a3c2b80201b89e68616f4ad30bc66aee4927c3ce50e33929ca819d5c43538898"),
        18_759_680,
        "gmp-6.3.0/",
        2_156,
        16_998_222,
        (
            _legal_file("COPYING", 35_147, "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903"),
            _legal_file("COPYING.LESSERv3", 7_639, "a853c2ffec17057872340eee242ae4d96cbf2b520ae27d903e1b2fef1a5f9d1c"),
            _legal_file("COPYINGv2", 18_092, "8177f97513213526df2cf6184d8ff986c675afb514d4e68a404010521b880643"),
            _legal_file("COPYINGv3", 35_150, "e6037104443f9a7829b2aa7c5370d0789a7bda3ca65a0b904cdc0c2e285d9195"),
            _legal_file("README", 4_051, "5e9f9325fd702bc4bcda27d7a78fea88a2a09fa39b4b15ac7b9b205e0863dc7e"),
        ),
        DetachedSignaturePolicyV1(
            "https://ftp.gnu.org/gnu/gmp/gmp-6.3.0.tar.xz.sig",
            374,
            bytes.fromhex("94def8c1a731854de684689126046ec93589147abd4cd0025f12d741d323aa82"),
            bytes.fromhex("928ac84aa0e2134bbb335cd439110dc3f9b967eb04caff4a44dd5d04a3f13474"),
            bytes.fromhex("343c2ff0fbee5ec2edbef399f3599ff828c67298"),
        ),
    )


def _mpfr_source_release_v1() -> SourceReleaseLockV1:
    return SourceReleaseLockV1(
        SourceRoleV1.MPFR,
        "4.2.2",
        "https://www.mpfr.org/mpfr-4.2.2/mpfr-4.2.2.tar.xz",
        ArchiveFormatV1.TAR_XZ,
        1_505_596,
        bytes.fromhex("b67ba0383ef7e8a8563734e2e889ef5ec3c3b898a01d00fa0a6869ad81c6ce01"),
        10_045_440,
        "mpfr-4.2.2/",
        572,
        9_590_620,
        (
            _legal_file("COPYING", 35_149, "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986"),
            _legal_file("COPYING.LESSER", 7_652, "e3a994d82e644b03a792a930f574002658412f62407f5fee083f2555c5f23118"),
            _legal_file("README", 3_333, "74e733d2cfa1a6f4e6530326ed460f13ac9e4a5d79bb0f682ab67db2c9dc4d5b"),
        ),
        DetachedSignaturePolicyV1(
            "https://www.mpfr.org/mpfr-4.2.2/mpfr-4.2.2.tar.xz.asc",
            228,
            bytes.fromhex("c6264c9a3652bc40775205ce90e7c96cea5058629e2e68f9eede5d8213f23ee6"),
            bytes.fromhex("3fe00f68bbf3888ae185b950d4db0f708dd01b6159cb03dec77296f9045b6372"),
            bytes.fromhex("a534be3f83e241d918280aeb5831d11a0d4db02a"),
        ),
    )


def arb_source_lock_v1() -> ArbSourceLockV1:
    """Return the exact published source declarations for the first Arb lane."""

    gmp = _gmp_source_release_v1()
    mpfr = _mpfr_source_release_v1()
    omitted = (
        ".gitattributes",
        ".github/ISSUE_TEMPLATE/bug_report.md",
        ".github/ISSUE_TEMPLATE/feature_request.md",
        ".github/PULL_REQUEST_TEMPLATE/pull_request_template.md",
        ".github/codecov.yml",
        ".github/workflows/CI.yml",
        ".github/workflows/docs.yml",
        ".github/workflows/push_CI.yml",
        ".github/workflows/release.yml",
        ".gitignore",
        "dev/bench.py",
        "dev/check_examples.sh",
        "dev/check_prototypes",
        "dev/conway/convert_cp_to_new_form.jl",
        "dev/conway/notes.c",
        "dev/find_gmp_mpfr.jl",
        "dev/gen_mul_basecase.jl",
        "dev/gen_mul_basecase.py",
        "dev/gen_mulhigh_basecase.jl",
        "dev/make_dist.sh",
    )
    project_pinned_release_only_files = (
        ProjectPinnedReleaseOnlyFileV1(
            "config/install-sh",
            0o700,
            15_358,
            bytes.fromhex("3d7488bebd0cfc9b5c440c55d5b44f1c6e2e3d3e19894821bae4a27f9307f1d2"),
        ),
        ProjectPinnedReleaseOnlyFileV1(
            "config/ltmain.sh",
            0o755,
            333_053,
            bytes.fromhex("579a1445e6a9a8b0809a44aa9f908387d4a43a2a440c9b84ea979f2b4f17816c"),
        ),
        ProjectPinnedReleaseOnlyFileV1(
            "configure",
            0o755,
            731_646,
            bytes.fromhex("43192d2f63812610726d943ada13bfc25864c39a8555314395d2d459d1502f45"),
        ),
        ProjectPinnedReleaseOnlyFileV1(
            "src/config.h.in",
            0o644,
            6_645,
            bytes.fromhex("af5b88c82a1549585b43a5dc856f3325d3513f423da0880f5459d913a25f9455"),
        ),
    )
    flint = SourceReleaseLockV1(
        SourceRoleV1.FLINT_ARB,
        "3.6.0",
        "https://github.com/flintlib/flint/releases/download/v3.6.0/flint-3.6.0.tar.gz",
        ArchiveFormatV1.TAR_GZIP,
        9_313_139,
        bytes.fromhex("b95e2c7792f5eea4a1c8d2d42c4098434756832e57a094b295eb5dfdc9b4c36b"),
        56_811_520,
        "flint-3.6.0/",
        10_112,
        48_758_775,
        (
            _legal_file("COPYING", 35_149, "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986"),
            _legal_file("COPYING.LESSER", 7_652, "e3a994d82e644b03a792a930f574002658412f62407f5fee083f2555c5f23118"),
            _legal_file("README.md", 3_008, "1a1c629fe32957b0bdf197c6048a83a987e8d28793234aff6feec5e1dcf7633f"),
        ),
        GitContentRelationPolicyV1(
            "https://github.com/flintlib/flint.git",
            "v3.6.0",
            bytes.fromhex("8d5454b96761fafe4d5a9da76a369a602f500f49"),
            bytes.fromhex("18d57417a96227b27dd5336881403dee6fdc851b"),
            10_108,
            omitted,
            project_pinned_release_only_files,
        ),
    )
    return ArbSourceLockV1((gmp, mpfr, flint))


def mpfi_source_lock_v1() -> MpfiSourceLockV1:
    """Return the exact source declarations for the first MPFI lane.

    MPFI 1.5.4 has no verified detached signature or archive-to-Git content
    relation.  Its archive is therefore named honestly as a project-pinned
    byte digest while GMP and MPFR retain their independently signed locks.
    """

    mpfi = SourceReleaseLockV1(
        SourceRoleV1.MPFI,
        "1.5.4",
        "https://perso.ens-lyon.fr/nathalie.revol/softwares/mpfi-1.5.4.tar.xz",
        ArchiveFormatV1.TAR_XZ,
        370_932,
        bytes.fromhex(
            "819e98bc7dad7cf7e67c9ddb592f44545c300de143fe30bc29ca1b422b55306a"
        ),
        3_502_080,
        "mpfi-1.5.4/",
        495,
        3_117_639,
        (
            _legal_file(
                "COPYING",
                35_147,
                "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
            ),
            _legal_file(
                "COPYING.LESSER",
                7_651,
                "da7eabb7bafdf7d3ae5e9f223aa5bdc1eece45ac569dc21b3b037520b4464768",
            ),
            _legal_file(
                "README",
                1_336,
                "dab7a52115f111ff3771dc4311a837919d45ffaa654e64c110af78bd2a003e20",
            ),
        ),
        ProjectPinnedArchiveDigestPolicyV1(),
    )
    return MpfiSourceLockV1(
        (_gmp_source_release_v1(), _mpfr_source_release_v1(), mpfi)
    )
