#!/usr/bin/env python3
"""Hostile tests for normalized source snapshots."""

from __future__ import annotations

import gzip
import hashlib
import io
import stat
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
sys.path.insert(0, str(PROOF))
sys.path.insert(0, str(ARB))

import provenance  # noqa: E402
import snapshot  # noqa: E402


def fixture() -> tuple[provenance.SourceReleaseLockV1, bytes]:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name in ("fixture-1/", "fixture-1/src/"):
            member = tarfile.TarInfo(name)
            member.type = tarfile.DIRTYPE
            member.mode = 0o755
            member.mtime = 0
            archive.addfile(member)
        for name, body, mode in (
            ("fixture-1/LICENSE", b"license", 0o644),
            ("fixture-1/src/tool", b"tool", 0o755),
        ):
            member = tarfile.TarInfo(name)
            member.mode = mode
            member.size = len(body)
            member.mtime = 0
            archive.addfile(member, io.BytesIO(body))
    archive_bytes = gzip.compress(raw.getvalue(), compresslevel=9, mtime=0)
    lock = provenance.SourceReleaseLockV1(
        provenance.SourceRoleV1.GMP,
        "1",
        "https://example.invalid/fixture-1.tar.gz",
        provenance.ArchiveFormatV1.TAR_GZIP,
        len(archive_bytes),
        hashlib.sha256(archive_bytes).digest(),
        len(raw.getvalue()),
        "fixture-1/",
        2,
        11,
        (
            provenance.LegalFileV1(
                "LICENSE", 7, hashlib.sha256(b"license").digest()
            ),
        ),
        provenance.DetachedSignaturePolicyV1(
            "https://example.invalid/fixture-1.tar.gz.sig",
            3,
            hashlib.sha256(b"sig").digest(),
            hashlib.sha256(b"packets").digest(),
            bytes.fromhex("11" * 20),
        ),
    )
    return lock, archive_bytes


class SourceSnapshotTests(unittest.TestCase):
    def test_snapshot_depends_only_on_public_provenance_surface(self) -> None:
        source = (ARB / "snapshot.py").read_text(encoding="utf-8")

        self.assertNotIn("provenance._", source)

    def test_only_admitted_regular_files_materialize_with_exact_modes(self) -> None:
        lock, archive_bytes = fixture()
        admitted = provenance.admit_source_archive(lock, archive_bytes)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "fixture-1"
            result = snapshot.materialize_source_archive(lock, admitted, destination)

            self.assertEqual(result.tree_identity, admitted.tree_identity)
            self.assertEqual(result.regular_file_count, 2)
            self.assertEqual((destination / "LICENSE").read_bytes(), b"license")
            self.assertEqual((destination / "src/tool").read_bytes(), b"tool")
            self.assertEqual(stat.S_IMODE((destination / "LICENSE").stat().st_mode), 0o644)
            self.assertEqual(stat.S_IMODE((destination / "src/tool").stat().st_mode), 0o755)
            for path in (
                destination,
                destination / "LICENSE",
                destination / "src",
                destination / "src/tool",
            ):
                with self.subTest(path=path):
                    self.assertEqual(
                        path.stat().st_mtime_ns,
                        snapshot.SOURCE_SNAPSHOT_MTIME_NS_V1,
                    )

    def test_materialization_decompresses_the_owned_archive_once(self) -> None:
        lock, archive_bytes = fixture()
        admitted = provenance.admit_source_archive(lock, archive_bytes)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "fixture-1"
            with mock.patch.object(
                provenance,
                "_decompress_exact",
                wraps=provenance._decompress_exact,
            ) as decompress:
                snapshot.materialize_source_archive(lock, admitted, destination)

        self.assertEqual(decompress.call_count, 1)

    def test_single_pass_replay_rejects_capability_coordinate_drift(self) -> None:
        lock, archive_bytes = fixture()
        original = provenance.admit_source_archive(lock, archive_bytes)
        mutations = (
            ("archive_sha256", bytes.fromhex("ff" * 32)),
            ("tree_identity", bytes.fromhex("ff" * 32)),
            ("regular_file_count", original.regular_file_count + 1),
            ("regular_file_bytes", original.regular_file_bytes + 1),
            ("files", original.files[:-1]),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                with tempfile.TemporaryDirectory() as temporary:
                    admitted = provenance.admit_source_archive(lock, archive_bytes)
                    object.__setattr__(admitted, field, value)
                    destination = Path(temporary) / "fixture-1"
                    with mock.patch.object(
                        provenance,
                        "_decompress_exact",
                        wraps=provenance._decompress_exact,
                    ) as decompress:
                        with self.assertRaises(snapshot.SnapshotErrorV1) as caught:
                            snapshot.materialize_source_archive(
                                lock,
                                admitted,
                                destination,
                            )

                    self.assertEqual(
                        caught.exception.reason,
                        snapshot.SnapshotReasonV1.FOREIGN_CAPABILITY,
                    )
                    self.assertEqual(decompress.call_count, 1)

    def test_destination_must_be_new_exact_release_root(self) -> None:
        lock, archive_bytes = fixture()
        admitted = provenance.admit_source_archive(lock, archive_bytes)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for destination in (root / "wrong", root):
                with self.subTest(destination=destination):
                    with self.assertRaises(snapshot.SnapshotErrorV1):
                        snapshot.materialize_source_archive(lock, admitted, destination)

    def test_timestamp_normalization_must_verify_the_filesystem_postcondition(self) -> None:
        lock, archive_bytes = fixture()
        admitted = provenance.admit_source_archive(lock, archive_bytes)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "fixture-1"
            with mock.patch.object(snapshot.os, "utime", return_value=None):
                with self.assertRaises(snapshot.SnapshotErrorV1) as caught:
                    snapshot.materialize_source_archive(lock, admitted, destination)

        self.assertEqual(caught.exception.reason, snapshot.SnapshotReasonV1.IO_FAILURE)


if __name__ == "__main__":
    unittest.main(verbosity=2)
