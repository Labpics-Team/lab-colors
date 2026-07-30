#!/usr/bin/env python3
"""Hostile MPFI source-lock and ordered-capability tests for proof V1."""

from __future__ import annotations

import hashlib
import io
import lzma
import sys
import tarfile
import unittest
from dataclasses import replace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import provenance  # noqa: E402
from provenance import (  # noqa: E402
    AdmittedMpfiSourcesV1,
    ArchiveFormatV1,
    DetachedSignaturePolicyV1,
    LegalFileV1,
    MpfiSourceLockV1,
    ProjectPinnedArchiveDigestPolicyV1,
    ProvenanceErrorV1,
    ProvenanceReasonV1,
    SourceReleaseLockV1,
    SourceRoleV1,
    admit_mpfi_sources,
    admit_source_archive,
    arb_source_lock_v1,
    mpfi_source_lock_v1,
)


def sha256(value: bytes) -> bytes:
    return hashlib.sha256(value).digest()


def fixture_archive() -> bytes:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        root = tarfile.TarInfo("fixture-1/")
        root.type = tarfile.DIRTYPE
        root.mode = 0o755
        archive.addfile(root)
        for name, body in (("LICENSE", b"license"), ("value", b"data")):
            member = tarfile.TarInfo(f"fixture-1/{name}")
            member.mode = 0o644
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))
    return lzma.compress(raw.getvalue(), format=lzma.FORMAT_XZ)


def fixture_release(
    role: SourceRoleV1,
    archive: bytes,
    integrity: DetachedSignaturePolicyV1 | ProjectPinnedArchiveDigestPolicyV1,
) -> SourceReleaseLockV1:
    raw_tar = lzma.decompress(archive)
    return SourceReleaseLockV1(
        role,
        "1",
        "https://example.invalid/fixture-1.tar.xz",
        ArchiveFormatV1.TAR_XZ,
        len(archive),
        sha256(archive),
        len(raw_tar),
        "fixture-1/",
        2,
        11,
        (LegalFileV1("LICENSE", 7, sha256(b"license")),),
        integrity,
    )


def detached_policy() -> DetachedSignaturePolicyV1:
    return DetachedSignaturePolicyV1(
        "https://example.invalid/fixture-1.tar.xz.sig",
        3,
        sha256(b"sig"),
        sha256(b"packets"),
        bytes.fromhex("00112233445566778899aabbccddeeff00112233"),
    )


class MpfiSourceLockTests(unittest.TestCase):
    def test_lane_specific_capabilities_have_no_generic_public_aggregate(self) -> None:
        self.assertFalse(hasattr(provenance, "SourceClosureV1"))
        self.assertFalse(hasattr(provenance, "SafeSourceClosureV1"))

    def test_exact_primary_coordinates_are_canonical_and_round_trip(self) -> None:
        lock = mpfi_source_lock_v1()
        self.assertEqual(
            tuple(source.role for source in lock.sources),
            (SourceRoleV1.GMP, SourceRoleV1.MPFR, SourceRoleV1.MPFI),
        )

        mpfi = lock.sources[2]
        self.assertEqual(mpfi.version, "1.5.4")
        self.assertEqual(
            mpfi.archive_url,
            "https://perso.ens-lyon.fr/nathalie.revol/softwares/mpfi-1.5.4.tar.xz",
        )
        self.assertIs(mpfi.archive_format, ArchiveFormatV1.TAR_XZ)
        self.assertEqual(mpfi.archive_length, 370_932)
        self.assertEqual(
            mpfi.archive_sha256.hex(),
            "819e98bc7dad7cf7e67c9ddb592f44545c300de143fe30bc29ca1b422b55306a",
        )
        self.assertEqual(
            mpfi.identity.hex(),
            "66289ae877f4526f992e4ffe5143433f6e17d73acfaeb936482ae4b797b876fb",
        )
        self.assertEqual(mpfi.tar_stream_length, 3_502_080)
        self.assertEqual(mpfi.root_prefix, "mpfi-1.5.4/")
        self.assertEqual(mpfi.regular_file_count, 495)
        self.assertEqual(mpfi.regular_file_bytes, 3_117_639)
        self.assertEqual(
            tuple((item.path, item.length, item.sha256.hex()) for item in mpfi.legal_files),
            (
                (
                    "COPYING",
                    35_147,
                    "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
                ),
                (
                    "COPYING.LESSER",
                    7_651,
                    "da7eabb7bafdf7d3ae5e9f223aa5bdc1eece45ac569dc21b3b037520b4464768",
                ),
                (
                    "README",
                    1_336,
                    "dab7a52115f111ff3771dc4311a837919d45ffaa654e64c110af78bd2a003e20",
                ),
            ),
        )
        self.assertIs(type(mpfi.integrity), ProjectPinnedArchiveDigestPolicyV1)
        encoded = lock.encode()
        self.assertEqual(len(encoded), 1_339)
        self.assertEqual(
            lock.identity.hex(),
            "03636d1f8c6c4950ba74943cf148d65b596d23a078391eedf6c7606c8613f830",
        )
        self.assertEqual(MpfiSourceLockV1.parse(encoded), lock)
        self.assertEqual(MpfiSourceLockV1.parse(encoded).encode(), encoded)

    def test_shared_sources_have_one_declaration_but_aggregate_lanes_differ(self) -> None:
        arb = arb_source_lock_v1()
        mpfi = mpfi_source_lock_v1()

        self.assertEqual(arb.sources[:2], mpfi.sources[:2])
        self.assertNotEqual(arb.sources[2], mpfi.sources[2])
        self.assertNotEqual(arb.identity, mpfi.identity)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            provenance.ArbSourceLockV1.parse(mpfi.encode())
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.NONCANONICAL_ORDER)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            MpfiSourceLockV1.parse(arb.encode())
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.NONCANONICAL_ORDER)

    def test_digest_only_policy_is_explicit_and_identity_bound(self) -> None:
        archive = fixture_archive()
        policy = ProjectPinnedArchiveDigestPolicyV1()
        gmp = fixture_release(SourceRoleV1.GMP, archive, detached_policy())
        mpfr = fixture_release(SourceRoleV1.MPFR, archive, detached_policy())
        mpfi = fixture_release(SourceRoleV1.MPFI, archive, policy)
        lock = MpfiSourceLockV1((gmp, mpfr, mpfi))

        encoded = lock.encode()
        self.assertEqual(MpfiSourceLockV1.parse(encoded), lock)
        for kind, reason in (
            (1, ProvenanceReasonV1.TRUNCATED),
            (2, ProvenanceReasonV1.TRUNCATED),
            (255, ProvenanceReasonV1.UNKNOWN_ENUM),
        ):
            with self.subTest(kind=kind):
                with self.assertRaises(ProvenanceErrorV1) as caught:
                    MpfiSourceLockV1.parse(encoded[:-1] + bytes((kind,)))
                self.assertEqual(caught.exception.reason, reason)
        self.assertNotEqual(
            lock.identity,
            MpfiSourceLockV1((gmp, mpfr, replace(mpfi, version="2"))).identity,
        )
        with self.assertRaises(ProvenanceErrorV1) as caught:
            MpfiSourceLockV1((gmp, mpfr, replace(mpfi, integrity=detached_policy())))
        self.assertEqual(
            caught.exception.reason,
            ProvenanceReasonV1.INTEGRITY_KIND_MISMATCH,
        )
        with self.assertRaises(ProvenanceErrorV1) as caught:
            provenance.ArbSourceLockV1((gmp, mpfr, mpfi))
        self.assertEqual(
            caught.exception.reason,
            ProvenanceReasonV1.NONCANONICAL_ORDER,
        )

    def test_three_locked_sources_become_one_mpfi_capability(self) -> None:
        archive = fixture_archive()
        gmp = fixture_release(SourceRoleV1.GMP, archive, detached_policy())
        mpfr = fixture_release(SourceRoleV1.MPFR, archive, detached_policy())
        mpfi = fixture_release(
            SourceRoleV1.MPFI,
            archive,
            ProjectPinnedArchiveDigestPolicyV1(),
        )
        lock = MpfiSourceLockV1((gmp, mpfr, mpfi))
        sources = tuple(
            admit_source_archive(expected, archive) for expected in lock.sources
        )

        admitted = admit_mpfi_sources(lock, sources)

        self.assertIs(type(admitted), AdmittedMpfiSourcesV1)
        self.assertEqual(admitted.source_lock_identity, lock.identity)
        self.assertEqual(admitted.sources, sources)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            admit_mpfi_sources(lock, (sources[1], sources[0], sources[2]))
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)
        with self.assertRaises(TypeError):
            AdmittedMpfiSourcesV1(lock.identity, sources, _token=object())

    def test_reference_does_not_upgrade_the_mpfi_digest_to_publisher_evidence(self) -> None:
        reference = (ROOT / "PROTOCOL.md").read_text(encoding="utf-8")

        self.assertIn("ProjectPinnedArchiveDigestPolicyV1", reference)
        self.assertIn("не приписывает этот digest издателю", reference)
        self.assertIn("не заявляет publisher authentication", reference)


if __name__ == "__main__":
    unittest.main(verbosity=2)
