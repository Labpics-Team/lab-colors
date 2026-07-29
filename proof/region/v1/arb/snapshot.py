#!/usr/bin/env python3
"""Materialize admitted source bytes into a new normalized build snapshot."""

from __future__ import annotations

import hashlib
import io
import os
import stat
import tarfile
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import NoReturn

import provenance


# Archive timestamps are deliberately outside source admission.  One epoch for
# every materialized node prevents Make-style freshness checks from observing
# extraction order; changing it is therefore a versioned snapshot-policy change.
SOURCE_SNAPSHOT_MTIME_NS_V1 = 0


class SnapshotReasonV1(StrEnum):
    FOREIGN_CAPABILITY = "foreign_capability"
    INVALID_DESTINATION = "invalid_destination"
    MATERIALIZATION_MISMATCH = "materialization_mismatch"
    IO_FAILURE = "io_failure"


@dataclass(frozen=True)
class SnapshotErrorV1(RuntimeError):
    reason: SnapshotReasonV1
    detail: str

    def __str__(self) -> str:
        return f"{self.reason}: {self.detail}"


def _fail(reason: SnapshotReasonV1, detail: str) -> NoReturn:
    raise SnapshotErrorV1(reason, detail)


@dataclass(frozen=True)
class MaterializedSourceTreeV1:
    tree_identity: bytes
    regular_file_count: int
    regular_file_bytes: int


def _write_all(descriptor: int, payload: bytes) -> None:
    offset = 0
    while offset < len(payload):
        try:
            written = os.write(descriptor, payload[offset:])
        except OSError:
            _fail(SnapshotReasonV1.IO_FAILURE, "source write failed")
        if written <= 0:
            _fail(SnapshotReasonV1.IO_FAILURE, "short source write")
        offset += written


def _ensure_parent(root: Path, relative_parent: Path) -> None:
    current = root
    for component in relative_parent.parts:
        current = current / component
        try:
            os.mkdir(current, 0o755)
        except FileExistsError:
            try:
                metadata = current.lstat()
            except OSError:
                _fail(SnapshotReasonV1.IO_FAILURE, "cannot inspect source directory")
            if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
                _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, "parent collision")
        except OSError:
            _fail(SnapshotReasonV1.IO_FAILURE, "cannot create source directory")
        try:
            os.chmod(current, 0o755, follow_symlinks=False)
        except OSError:
            _fail(SnapshotReasonV1.IO_FAILURE, "cannot normalize source directory")


def _normalize_snapshot_times(root: Path, relative_paths: set[str]) -> None:
    directories = {root}
    try:
        for relative in relative_paths:
            target = root / relative
            metadata = target.lstat()
            if not stat.S_ISREG(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
                _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, relative)
            os.utime(
                target,
                ns=(SOURCE_SNAPSHOT_MTIME_NS_V1, SOURCE_SNAPSHOT_MTIME_NS_V1),
                follow_symlinks=False,
            )
            parent = target.parent
            while parent != root:
                directories.add(parent)
                parent = parent.parent
        for directory in sorted(
            directories,
            key=lambda item: len(item.relative_to(root).parts),
            reverse=True,
        ):
            metadata = directory.lstat()
            if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
                _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, "parent collision")
            os.utime(
                directory,
                ns=(SOURCE_SNAPSHOT_MTIME_NS_V1, SOURCE_SNAPSHOT_MTIME_NS_V1),
                follow_symlinks=False,
            )
    except SnapshotErrorV1:
        raise
    except OSError:
        _fail(SnapshotReasonV1.IO_FAILURE, "cannot normalize source timestamps")


def materialize_source_archive(
    expected: provenance.SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
    destination: Path,
) -> MaterializedSourceTreeV1:
    """Write only regular files from the exact bytes owned by `admitted`."""

    if type(expected) is not provenance.SourceReleaseLockV1:
        raise TypeError("expected must be SourceReleaseLockV1")
    if type(admitted) is not provenance.SafeSourceArchiveV1:
        raise TypeError("admitted must be SafeSourceArchiveV1")
    if not isinstance(destination, Path):
        raise TypeError("destination must be Path")
    if admitted.source_lock_identity != expected.identity:
        _fail(SnapshotReasonV1.FOREIGN_CAPABILITY, "source lock identity")

    root_name = expected.root_prefix[:-1]
    if destination.name != root_name or destination.exists() or destination.is_symlink():
        _fail(SnapshotReasonV1.INVALID_DESTINATION, "destination must be a new release root")
    try:
        parent = destination.parent.resolve(strict=True)
    except (OSError, RuntimeError):
        _fail(SnapshotReasonV1.INVALID_DESTINATION, "destination parent unavailable")
    if not parent.is_dir():
        _fail(SnapshotReasonV1.INVALID_DESTINATION, "destination parent is not a directory")
    destination = parent / destination.name

    replayed = provenance.admit_source_archive(expected, admitted.archive_bytes)
    if (
        replayed.tree_identity != admitted.tree_identity
        or replayed.archive_sha256 != admitted.archive_sha256
        or replayed.files != admitted.files
    ):
        _fail(SnapshotReasonV1.FOREIGN_CAPABILITY, "archive replay drift")
    raw_tar = provenance._decompress_exact(  # same parser as admission
        admitted.archive_bytes,
        expected.archive_format,
        expected.tar_stream_length,
    )
    expected_files = {item.path: item for item in admitted.files}
    seen: set[str] = set()
    try:
        os.mkdir(destination, 0o755)
        with tarfile.open(fileobj=io.BytesIO(raw_tar), mode="r:") as archive:
            for member in archive:
                if not member.isreg():
                    continue
                relative = member.name[len(expected.root_prefix) :]
                coordinate = expected_files.get(relative)
                if coordinate is None or relative in seen:
                    _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, relative)
                stream = archive.extractfile(member)
                if stream is None:
                    _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, relative)
                target = destination / relative
                _ensure_parent(destination, Path(relative).parent)
                flags = (
                    os.O_WRONLY
                    | os.O_CREAT
                    | os.O_EXCL
                    | getattr(os, "O_CLOEXEC", 0)
                    | getattr(os, "O_NOFOLLOW", 0)
                )
                descriptor = os.open(target, flags, coordinate.mode)
                try:
                    hasher = hashlib.sha256()
                    length = 0
                    while True:
                        chunk = stream.read(provenance.READ_CHUNK_BYTES)
                        if not chunk:
                            break
                        length += len(chunk)
                        if length > coordinate.length:
                            _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, relative)
                        hasher.update(chunk)
                        _write_all(descriptor, chunk)
                    if length != coordinate.length or hasher.digest() != coordinate.sha256:
                        _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, relative)
                    os.fchmod(descriptor, coordinate.mode)
                finally:
                    os.close(descriptor)
                seen.add(relative)
    except SnapshotErrorV1:
        raise
    except (OSError, tarfile.TarError):
        _fail(SnapshotReasonV1.IO_FAILURE, "materialization failed")
    if seen != set(expected_files):
        _fail(SnapshotReasonV1.MATERIALIZATION_MISMATCH, "missing source file")
    _normalize_snapshot_times(destination, seen)
    return MaterializedSourceTreeV1(
        admitted.tree_identity,
        admitted.regular_file_count,
        admitted.regular_file_bytes,
    )
