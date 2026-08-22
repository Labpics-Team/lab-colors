#!/usr/bin/env python3
"""Hostile source-lock and archive-admission tests for proof tooling V1."""

from __future__ import annotations

import gzip
import hashlib
import io
import lzma
import sys
import tarfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import provenance  # noqa: E402
from provenance import (  # noqa: E402
    AdmittedArbSourcesV1,
    AdmittedMpfiSourcesV1,
    ArchiveFormatV1,
    DetachedSignaturePolicyV1,
    GitContentRelationPolicyV1,
    LegalFileV1,
    MpfiSourceLockV1,
    ProjectPinnedArchiveDigestPolicyV1,
    ProjectPinnedReleaseOnlyFileV1,
    ProvenanceErrorV1,
    ProvenanceReasonV1,
    SourceReleaseLockV1,
    SourceRoleV1,
    admit_source_archive,
    admit_arb_sources,
    admit_mpfi_sources,
    arb_source_lock_v1,
)


def sha256(value: bytes) -> bytes:
    return hashlib.sha256(value).digest()


def canonical_identity(label: bytes, encoded: bytes) -> bytes:
    """Independent literal oracle for identity values in hostile cache tests."""

    return hashlib.sha256(
        label + len(encoded).to_bytes(8, "big") + encoded
    ).digest()


def admitted_closure_identity(
    label: bytes,
    source_lock_identity: bytes,
    sources: tuple[provenance.SafeSourceArchiveV1, ...],
) -> bytes:
    """Keep the cache-poisoning oracle independent of production preimages."""

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
    return canonical_identity(label, b"".join(chunks))


def _replay_source(
    lock: SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
) -> provenance.ReplayedSourceMaterializationV1:
    """Exercise the one source materialization contract from a fresh replay."""

    return provenance.replay_materialize_admitted_source_v1(lock, admitted)


def _source_files(
    lock: SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
) -> tuple[tuple[str, int, bytes], ...]:
    return _replay_source(lock, admitted).files


def _source_coordinates(
    lock: SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
) -> tuple[bytes, ...]:
    return provenance.source_archive_replay_coordinates_v1(lock, admitted)


def tar_gz(
    entries: tuple[tuple[str, bytes | None, bytes | None], ...],
) -> bytes:
    """Build deterministic hostile fixtures; linkname is the third item."""

    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, body, linkname in entries:
            member = tarfile.TarInfo(name)
            member.mtime = 0
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            if linkname is not None:
                member.type = tarfile.SYMTYPE
                member.linkname = linkname.decode("ascii")
                member.mode = 0o777
                archive.addfile(member)
            elif body is None:
                member.type = tarfile.DIRTYPE
                member.mode = 0o755
                archive.addfile(member)
            else:
                member.type = tarfile.REGTYPE
                member.mode = 0o644
                member.size = len(body)
                archive.addfile(member, io.BytesIO(body))
    return gzip.compress(raw.getvalue(), compresslevel=9, mtime=0)


def raw_ustar(
    entries: tuple[tuple[str, bytes, int], ...],
) -> bytes:
    """Build a replay fixture without reusing admission's compressed input."""

    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        root = tarfile.TarInfo("fixture-1/")
        root.type = tarfile.DIRTYPE
        root.mode = 0o755
        archive.addfile(root)
        for name, body, mode in entries:
            member = tarfile.TarInfo(f"fixture-1/{name}")
            member.mode = mode
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))
    return raw.getvalue()


def fixture_lock(
    archive: bytes,
    *,
    root: str = "fixture-1/",
    file_count: int = 2,
    unpacked_bytes: int = 11,
    tar_stream_bytes: int | None = None,
    archive_format: ArchiveFormatV1 = ArchiveFormatV1.TAR_GZIP,
) -> SourceReleaseLockV1:
    if tar_stream_bytes is None:
        tar_stream_bytes = len(
            gzip.decompress(archive)
            if archive_format is ArchiveFormatV1.TAR_GZIP
            else lzma.decompress(archive)
        )
    return SourceReleaseLockV1(
        role=SourceRoleV1.GMP,
        version="1",
        archive_url="https://example.invalid/fixture-1.tar.gz",
        archive_format=archive_format,
        archive_length=len(archive),
        archive_sha256=sha256(archive),
        tar_stream_length=tar_stream_bytes,
        root_prefix=root,
        regular_file_count=file_count,
        regular_file_bytes=unpacked_bytes,
        legal_files=(LegalFileV1("LICENSE", 7, sha256(b"license")),),
        integrity=DetachedSignaturePolicyV1(
            signature_url="https://example.invalid/fixture-1.tar.gz.sig",
            signature_length=3,
            signature_sha256=sha256(b"sig"),
            public_key_packets_sha256=sha256(b"packets"),
            signer_fingerprint=bytes.fromhex(
                "00112233445566778899aabbccddeeff00112233"
            ),
        ),
    )


GOOD_ARCHIVE = tar_gz(
    (
        ("fixture-1/", None, None),
        ("fixture-1/LICENSE", b"license", None),
        ("fixture-1/value", b"data", None),
    )
)


class ArbSourceLockTests(unittest.TestCase):
    def test_old_overclaiming_vocabulary_is_not_public(self) -> None:
        for name in (
            "GeneratedFileV1",
            "GitReleasePolicyV1",
            "LicenseFileV1",
            "OriginKindV1",
            "OriginPolicyV1",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(provenance, name))
        self.assertFalse(
            hasattr(ProvenanceReasonV1, "LICENSE_CLOSURE_MISMATCH")
        )
        self.assertFalse(
            hasattr(ProvenanceReasonV1, "RELEASE_RELATION_MISMATCH")
        )
        self.assertFalse(
            hasattr(provenance.IntegrityKindV1, "GIT_RELEASE_RELATION")
        )
        self.assertFalse(hasattr(ProvenanceReasonV1, "ORIGIN_KIND_MISMATCH"))
        self.assertFalse(hasattr(provenance, "_parse_origin_policy"))
        self.assertNotIn("origin", SourceReleaseLockV1.__dataclass_fields__)
        self.assertIn("integrity", SourceReleaseLockV1.__dataclass_fields__)

    def test_exact_primary_coordinates_are_canonical_and_round_trip(self) -> None:
        lock = arb_source_lock_v1()
        self.assertEqual(tuple(item.role for item in lock.sources), (
            SourceRoleV1.GMP,
            SourceRoleV1.MPFR,
            SourceRoleV1.FLINT_ARB,
        ))

        gmp, mpfr, flint = lock.sources
        self.assertEqual(gmp.version, "6.3.0")
        self.assertEqual(gmp.archive_length, 2_094_196)
        self.assertEqual(
            gmp.archive_sha256.hex(),
            "a3c2b80201b89e68616f4ad30bc66aee4927c3ce50e33929ca819d5c43538898",
        )
        self.assertIsInstance(gmp.integrity, DetachedSignaturePolicyV1)
        self.assertEqual(
            gmp.integrity.signer_fingerprint.hex(),
            "343c2ff0fbee5ec2edbef399f3599ff828c67298",
        )
        self.assertEqual(
            gmp.integrity.public_key_packets_sha256.hex(),
            "928ac84aa0e2134bbb335cd439110dc3f9b967eb04caff4a44dd5d04a3f13474",
        )

        self.assertEqual(mpfr.version, "4.2.2")
        self.assertEqual(
            mpfr.archive_url,
            "https://www.mpfr.org/mpfr-4.2.2/mpfr-4.2.2.tar.xz",
        )
        self.assertEqual(mpfr.archive_length, 1_505_596)
        self.assertEqual(
            mpfr.archive_sha256.hex(),
            "b67ba0383ef7e8a8563734e2e889ef5ec3c3b898a01d00fa0a6869ad81c6ce01",
        )
        self.assertIsInstance(mpfr.integrity, DetachedSignaturePolicyV1)
        self.assertEqual(
            mpfr.integrity.signer_fingerprint.hex(),
            "a534be3f83e241d918280aeb5831d11a0d4db02a",
        )
        self.assertEqual(
            mpfr.integrity.public_key_packets_sha256.hex(),
            "3fe00f68bbf3888ae185b950d4db0f708dd01b6159cb03dec77296f9045b6372",
        )

        self.assertEqual(flint.version, "3.6.0")
        self.assertEqual(flint.archive_length, 9_313_139)
        self.assertEqual(
            flint.archive_sha256.hex(),
            "b95e2c7792f5eea4a1c8d2d42c4098434756832e57a094b295eb5dfdc9b4c36b",
        )
        self.assertIsInstance(flint.integrity, GitContentRelationPolicyV1)
        self.assertEqual(
            flint.integrity.commit.hex(),
            "8d5454b96761fafe4d5a9da76a369a602f500f49",
        )
        self.assertEqual(
            flint.integrity.tree.hex(),
            "18d57417a96227b27dd5336881403dee6fdc851b",
        )
        self.assertEqual(flint.integrity.common_file_count, 10_108)
        self.assertEqual(len(flint.integrity.omitted_paths), 20)
        self.assertEqual(
            len(flint.integrity.project_pinned_release_only_files),
            4,
        )

        encoded = lock.encode()
        self.assertEqual(len(encoded), 2_286)
        self.assertEqual(
            lock.identity.hex(),
            "a4948c57ed0f9bb066a285b17d7990415cad22ff8d03b5f91900b73da5d2b8cc",
        )
        self.assertEqual(type(lock).parse(encoded).encode(), encoded)
        self.assertEqual(type(lock).parse(encoded).identity, lock.identity)

    def test_every_expected_coordinate_is_identity_bound(self) -> None:
        lock = arb_source_lock_v1()
        seen: set[bytes] = set()
        for index, source in enumerate(lock.sources):
            if isinstance(source.integrity, GitContentRelationPolicyV1):
                count_mutation = replace(
                    source,
                    regular_file_count=source.regular_file_count + 1,
                    integrity=replace(
                        source.integrity,
                        common_file_count=source.integrity.common_file_count + 1,
                    ),
                )
            else:
                count_mutation = replace(
                    source,
                    regular_file_count=source.regular_file_count + 1,
                )
            source_mutations = (
                replace(source, version=source.version + "x"),
                replace(source, archive_url=source.archive_url + ".invalid"),
                replace(source, archive_length=source.archive_length + 1),
                replace(source, archive_sha256=sha256(source.archive_sha256)),
                replace(source, tar_stream_length=source.tar_stream_length + 512),
                replace(source, root_prefix="x-" + source.root_prefix),
                count_mutation,
                replace(source, regular_file_bytes=source.regular_file_bytes + 1),
                replace(
                    source,
                    legal_files=(
                        replace(
                            source.legal_files[0],
                            length=source.legal_files[0].length + 1,
                        ),
                    )
                    + source.legal_files[1:],
                ),
            )
            if isinstance(source.integrity, DetachedSignaturePolicyV1):
                integrity_mutations = (
                    replace(source.integrity, signature_url=source.integrity.signature_url + ".invalid"),
                    replace(source.integrity, signature_length=source.integrity.signature_length + 1),
                    replace(source.integrity, signature_sha256=sha256(source.integrity.signature_sha256)),
                    replace(
                        source.integrity,
                        public_key_packets_sha256=sha256(
                            source.integrity.public_key_packets_sha256
                        ),
                    ),
                    replace(
                        source.integrity,
                        signer_fingerprint=bytes(
                            reversed(source.integrity.signer_fingerprint)
                        ),
                    ),
                )
            else:
                integrity_mutations = (
                    replace(source.integrity, repository_url=source.integrity.repository_url + ".invalid"),
                    replace(source.integrity, tag=source.integrity.tag + "x"),
                    replace(source.integrity, commit=bytes(reversed(source.integrity.commit))),
                    replace(source.integrity, tree=bytes(reversed(source.integrity.tree))),
                    replace(
                        source.integrity,
                        omitted_paths=(source.integrity.omitted_paths[0] + "x",)
                        + source.integrity.omitted_paths[1:],
                    ),
                    replace(
                        source.integrity,
                        project_pinned_release_only_files=(
                            replace(
                                source.integrity.project_pinned_release_only_files[0],
                                sha256=sha256(
                                    source.integrity.project_pinned_release_only_files[0].sha256
                                ),
                            ),
                        )
                        + source.integrity.project_pinned_release_only_files[1:],
                    ),
                )
            for mutation in (
                *source_mutations,
                *(replace(source, integrity=value) for value in integrity_mutations),
            ):
                sources = list(lock.sources)
                sources[index] = mutation
                changed = type(lock)(tuple(sources))
                self.assertNotEqual(changed.identity, lock.identity)
                self.assertNotIn(changed.identity, seen)
                seen.add(changed.identity)

    def test_parser_rejects_malleability_and_arbitrary_order(self) -> None:
        lock = arb_source_lock_v1()
        encoded = lock.encode()
        for hostile in (encoded[:-1], encoded + b"\0", b"wrong!!!" + encoded[8:]):
            with self.assertRaises(ProvenanceErrorV1):
                type(lock).parse(hostile)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            type(lock)((lock.sources[1], lock.sources[0], lock.sources[2]))
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.NONCANONICAL_ORDER)

        with self.assertRaises(ProvenanceErrorV1) as caught:
            type(lock).parse(encoded[:10])
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.TRUNCATED)

    def test_malformed_url_is_a_typed_input_failure(self) -> None:
        for url in (
            "https://[invalid/signature",
            "https://example.invalid:bad/signature",
            "https://example.invalid/\nsignature",
        ):
            with self.subTest(url=url):
                with self.assertRaises(ProvenanceErrorV1) as caught:
                    DetachedSignaturePolicyV1(
                        url,
                        3,
                        sha256(b"sig"),
                        sha256(b"packets"),
                        bytes.fromhex("00112233445566778899aabbccddeeff00112233"),
                    )
                self.assertEqual(
                    caught.exception.reason,
                    ProvenanceReasonV1.INVALID_FIELD,
                )

    def test_constructor_cardinality_limits_match_the_wire_parser(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            replace(lock, legal_files=lock.legal_files * 4_097)
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.INVALID_FIELD)

        flint_integrity = arb_source_lock_v1().sources[2].integrity
        self.assertIsInstance(flint_integrity, GitContentRelationPolicyV1)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            replace(
                flint_integrity,
                omitted_paths=flint_integrity.omitted_paths * 205,
            )
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.INVALID_FIELD)


class SafeArchiveAdmissionTests(unittest.TestCase):
    def test_three_locked_sources_become_one_ordered_capability(self) -> None:
        gmp = fixture_lock(GOOD_ARCHIVE)
        mpfr = replace(gmp, role=SourceRoleV1.MPFR)
        flint = replace(
            gmp,
            role=SourceRoleV1.FLINT_ARB,
            integrity=GitContentRelationPolicyV1(
                "https://example.invalid/fixture.git",
                "v1",
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                1,
                ("missing",),
                (
                    ProjectPinnedReleaseOnlyFileV1(
                        "value",
                        0o644,
                        4,
                        sha256(b"data"),
                    ),
                ),
            ),
        )
        lock = provenance.ArbSourceLockV1((gmp, mpfr, flint))
        sources = tuple(
            admit_source_archive(expected, GOOD_ARCHIVE)
            for expected in lock.sources
        )

        admitted = admit_arb_sources(lock, sources)

        self.assertIs(type(admitted), AdmittedArbSourcesV1)
        self.assertEqual(admitted.source_lock_identity, lock.identity)
        self.assertEqual(admitted.sources, sources)
        self.assertEqual(len(admitted.identity), 32)
        with self.assertRaises((ProvenanceErrorV1, TypeError)):
            admit_arb_sources(lock, (sources[1], sources[0], sources[2]))
        with self.assertRaises(TypeError):
            AdmittedArbSourcesV1(
                lock.identity,
                sources,
                _token=object(),
            )

    def test_aggregate_admission_replays_each_source_before_it_owns_the_closure(
        self,
    ) -> None:
        gmp = fixture_lock(GOOD_ARCHIVE)
        mpfr = replace(gmp, role=SourceRoleV1.MPFR)
        arb_third = replace(
            gmp,
            role=SourceRoleV1.FLINT_ARB,
            integrity=GitContentRelationPolicyV1(
                "https://example.invalid/fixture.git",
                "v1",
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                1,
                ("missing",),
                (
                    ProjectPinnedReleaseOnlyFileV1(
                        "value",
                        0o644,
                        4,
                        sha256(b"data"),
                    ),
                ),
            ),
        )
        mpfi_third = replace(
            gmp,
            role=SourceRoleV1.MPFI,
            integrity=ProjectPinnedArchiveDigestPolicyV1(),
        )

        class CounterfeitDigest(bytes):
            def __eq__(self, _other: object) -> bool:
                return True

            def __ne__(self, _other: object) -> bool:
                return False

        cases = (
            (
                provenance.ArbSourceLockV1((gmp, mpfr, arb_third)),
                admit_arb_sources,
            ),
            (
                MpfiSourceLockV1((gmp, mpfr, mpfi_third)),
                admit_mpfi_sources,
            ),
        )
        for lock, admit in cases:
            with self.subTest(lock_type=type(lock).__name__):
                sources = tuple(
                    admit_source_archive(release, GOOD_ARCHIVE)
                    for release in lock.sources
                )
                original = sources[0].archive_sha256
                object.__setattr__(
                    sources[0],
                    "archive_sha256",
                    CounterfeitDigest(b"\x92" * 32),
                )
                try:
                    with self.assertRaises(ProvenanceErrorV1) as caught:
                        admit(lock, sources)
                finally:
                    object.__setattr__(sources[0], "archive_sha256", original)
                self.assertEqual(
                    caught.exception.reason,
                    ProvenanceReasonV1.FOREIGN_BINDING,
                )

                fresh = admit(lock, sources)
                self.assertIsNot(fresh.sources[0], sources[0])

    def test_archive_is_hash_checked_then_scanned_without_extracting(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)
        self.assertEqual(admitted.source_lock_identity, lock.identity)
        self.assertEqual(admitted.regular_file_count, 2)
        self.assertEqual(admitted.regular_file_bytes, 11)
        self.assertEqual(tuple(item.path for item in admitted.files), ("LICENSE", "value"))
        self.assertEqual(admitted.files[0].sha256, sha256(b"license"))
        self.assertIs(admitted.archive_bytes, GOOD_ARCHIVE)

        with self.assertRaises(TypeError):
            provenance.SafeSourceArchiveV1(
                lock.identity,
                lock.archive_sha256,
                b"t" * 32,
                2,
                11,
                admitted.files,
                GOOD_ARCHIVE,
                _token=object(),
            )

        for changed in (
            replace(lock, archive_length=lock.archive_length + 1),
            replace(lock, archive_sha256=sha256(b"other")),
            replace(lock, tar_stream_length=lock.tar_stream_length + 512),
            replace(lock, root_prefix="other/"),
            replace(lock, regular_file_count=3),
            replace(lock, regular_file_bytes=12),
            replace(
                lock,
                legal_files=(LegalFileV1("LICENSE", 7, sha256(b"wrong")),),
            ),
        ):
            with self.assertRaises(ProvenanceErrorV1):
                admit_source_archive(changed, GOOD_ARCHIVE)

        changed_legal_file = replace(
            lock,
            legal_files=(LegalFileV1("LICENSE", 7, sha256(b"wrong")),),
        )
        with self.assertRaises(ProvenanceErrorV1) as caught:
            admit_source_archive(changed_legal_file, GOOD_ARCHIVE)
        self.assertEqual(
            caught.exception.reason,
            ProvenanceReasonV1.LEGAL_FILES_MISMATCH,
        )

    def test_derived_identities_ignore_injected_instance_caches(self) -> None:
        poison = bytes.fromhex("a5" * 32)
        release = fixture_lock(GOOD_ARCHIVE)
        release_identity = canonical_identity(
            b"labcolors.proof-region.source-release-lock.v1\0",
            release.encode(),
        )
        release.__dict__["identity"] = poison

        self.assertEqual(release.identity, release_identity)
        admitted_release = admit_source_archive(release, GOOD_ARCHIVE)
        self.assertEqual(admitted_release.source_lock_identity, release_identity)
        replay = _replay_source(release, admitted_release)
        self.assertEqual(replay.source.source_lock_identity, release_identity)
        self.assertEqual(
            provenance.source_archive_replay_coordinates_v1(
                release,
                admitted_release,
            )[2],
            release_identity,
        )
        self.assertEqual(
            replay.files,
            (("LICENSE", 0o644, b"license"), ("value", 0o644, b"data")),
        )

        arb_gmp = fixture_lock(GOOD_ARCHIVE)
        arb_mpfr = replace(arb_gmp, role=SourceRoleV1.MPFR)
        arb_flint = replace(
            arb_gmp,
            role=SourceRoleV1.FLINT_ARB,
            integrity=GitContentRelationPolicyV1(
                "https://example.invalid/fixture.git",
                "v1",
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                1,
                ("missing",),
                (
                    ProjectPinnedReleaseOnlyFileV1(
                        "value",
                        0o644,
                        4,
                        sha256(b"data"),
                    ),
                ),
            ),
        )
        arb_lock = provenance.ArbSourceLockV1((arb_gmp, arb_mpfr, arb_flint))
        arb_lock_identity = canonical_identity(
            b"labcolors.proof-region.source-lock.v1\0",
            arb_lock.encode(),
        )
        arb_lock.__dict__["identity"] = poison
        arb_sources = tuple(
            admit_source_archive(source, GOOD_ARCHIVE)
            for source in arb_lock.sources
        )
        arb_admitted = admit_arb_sources(arb_lock, arb_sources)
        self.assertIs(type(arb_admitted), AdmittedArbSourcesV1)
        self.assertEqual(arb_admitted.source_lock_identity, arb_lock_identity)
        arb_admitted_identity = admitted_closure_identity(
            b"labcolors.proof-region.admitted-arb-sources.v1\0",
            arb_lock_identity,
            arb_sources,
        )
        arb_admitted.__dict__["identity"] = poison
        self.assertEqual(arb_admitted.identity, arb_admitted_identity)

        mpfi_gmp = fixture_lock(GOOD_ARCHIVE)
        mpfi_mpfr = replace(mpfi_gmp, role=SourceRoleV1.MPFR)
        mpfi_release = replace(
            mpfi_gmp,
            role=SourceRoleV1.MPFI,
            integrity=ProjectPinnedArchiveDigestPolicyV1(),
        )
        mpfi_lock = MpfiSourceLockV1((mpfi_gmp, mpfi_mpfr, mpfi_release))
        mpfi_lock_identity = canonical_identity(
            b"labcolors.proof-region.source-lock.v1\0",
            mpfi_lock.encode(),
        )
        mpfi_lock.__dict__["identity"] = poison
        mpfi_sources = tuple(
            admit_source_archive(source, GOOD_ARCHIVE)
            for source in mpfi_lock.sources
        )
        mpfi_admitted = admit_mpfi_sources(mpfi_lock, mpfi_sources)
        self.assertIs(type(mpfi_admitted), AdmittedMpfiSourcesV1)
        self.assertEqual(mpfi_admitted.source_lock_identity, mpfi_lock_identity)
        mpfi_admitted_identity = admitted_closure_identity(
            b"labcolors.proof-region.admitted-mpfi-sources.v1\0",
            mpfi_lock_identity,
            mpfi_sources,
        )
        mpfi_admitted.__dict__["identity"] = poison
        self.assertEqual(mpfi_admitted.identity, mpfi_admitted_identity)

    def test_operation_owned_source_coordinates_have_no_public_projection(self) -> None:
        self.assertFalse(hasattr(provenance, "materialized_source_coordinates_v1"))

    def test_replay_rejects_an_instance_encode_shadow(self) -> None:
        """A frozen dataclass can still shadow a method through ``__dict__``."""

        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)
        foreign_lock = replace(lock, version="2")
        foreign_admitted = admit_source_archive(foreign_lock, GOOD_ARCHIVE)
        lock.__dict__["encode"] = lambda: SourceReleaseLockV1.encode(foreign_lock)
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                provenance.replay_materialize_admitted_source_v1(
                    lock,
                    foreign_admitted,
                )
            self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

            replay = provenance.replay_materialize_admitted_source_v1(lock, admitted)
            self.assertEqual(replay.source_lock.version, "1")
        finally:
            del lock.__dict__["encode"]

    def test_replay_rejects_a_nested_encoder_shadow(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)
        foreign_legal_file = LegalFileV1("value", 4, sha256(b"data"))
        foreign_lock = replace(lock, legal_files=(foreign_legal_file,))
        foreign_admitted = admit_source_archive(foreign_lock, GOOD_ARCHIVE)
        legal_file = lock.legal_files[0]
        legal_file.__dict__["encode"] = lambda: LegalFileV1.encode(foreign_legal_file)
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                provenance.replay_materialize_admitted_source_v1(
                    lock,
                    foreign_admitted,
                )
            self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

            replay = provenance.replay_materialize_admitted_source_v1(lock, admitted)
            self.assertEqual(replay.source_lock.legal_files[0].path, "LICENSE")
        finally:
            del legal_file.__dict__["encode"]

    def test_metadata_replays_do_not_materialize_file_bodies(self) -> None:
        gmp = fixture_lock(GOOD_ARCHIVE)
        mpfr = replace(gmp, role=SourceRoleV1.MPFR)
        mpfi = replace(
            gmp,
            role=SourceRoleV1.MPFI,
            integrity=ProjectPinnedArchiveDigestPolicyV1(),
        )
        lock = MpfiSourceLockV1((gmp, mpfr, mpfi))
        sources = tuple(
            admit_source_archive(release, GOOD_ARCHIVE)
            for release in lock.sources
        )

        with mock.patch.object(
            provenance,
            "_materialize_replayed_source_files_v1",
        ) as materialize:
            provenance.source_archive_replay_coordinates_v1(gmp, sources[0])
            admitted = admit_mpfi_sources(lock, sources)

        materialize.assert_not_called()
        self.assertIs(type(admitted), AdmittedMpfiSourcesV1)

    def test_closure_snapshot_replays_metadata_without_materializing_bodies(
        self,
    ) -> None:
        gmp = fixture_lock(GOOD_ARCHIVE)
        mpfr = replace(gmp, role=SourceRoleV1.MPFR)
        flint = replace(
            gmp,
            role=SourceRoleV1.FLINT_ARB,
            integrity=GitContentRelationPolicyV1(
                "https://example.invalid/fixture.git",
                "v1",
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                1,
                ("missing",),
                (
                    ProjectPinnedReleaseOnlyFileV1(
                        "value",
                        0o644,
                        4,
                        sha256(b"data"),
                    ),
                ),
            ),
        )
        lock = provenance.ArbSourceLockV1((gmp, mpfr, flint))
        sources = tuple(
            admit_source_archive(release, GOOD_ARCHIVE)
            for release in lock.sources
        )
        admitted = admit_arb_sources(lock, sources)
        real_admit = provenance._admit_source_archive_once
        real_materialize = provenance._materialize_replayed_source_files_v1

        with (
            mock.patch.object(
                provenance,
                "_admit_source_archive_once",
                wraps=real_admit,
            ) as replay,
            mock.patch.object(
                provenance,
                "_materialize_replayed_source_files_v1",
                wraps=real_materialize,
            ) as materialize,
        ):
            snapshot = provenance.snapshot_admitted_source_closure_v1(lock, admitted)

        self.assertIs(type(snapshot), AdmittedArbSourcesV1)
        self.assertEqual(snapshot.identity, admitted.identity)
        self.assertEqual(replay.call_count, 3)
        self.assertEqual(materialize.call_count, 0)
        self.assertTrue(
            all(
                snapshot_source is not retained_source
                for snapshot_source, retained_source in zip(
                    snapshot.sources,
                    admitted.sources,
                    strict=True,
                )
            )
        )

        flint_source = admitted.sources[2]
        original_files = flint_source.files
        for replacement in (
            list(original_files),
            (replace(original_files[0], path="LICENSE-FORGED"), *original_files[1:]),
        ):
            with self.subTest(retained_manifest=type(replacement).__name__):
                object.__setattr__(flint_source, "files", replacement)
                try:
                    with self.assertRaises(ProvenanceErrorV1) as caught:
                        provenance.snapshot_admitted_source_closure_v1(lock, admitted)
                finally:
                    object.__setattr__(flint_source, "files", original_files)
                self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

        original_archive = flint_source.archive_bytes
        corrupted_archive = bytes((original_archive[0] ^ 1,)) + original_archive[1:]
        object.__setattr__(flint_source, "_archive_bytes", corrupted_archive)
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                provenance.snapshot_admitted_source_closure_v1(lock, admitted)
        finally:
            object.__setattr__(flint_source, "_archive_bytes", original_archive)
        self.assertEqual(
            caught.exception.reason,
            ProvenanceReasonV1.ARCHIVE_DIGEST_MISMATCH,
        )

    def test_public_closure_replay_totalizes_a_mutated_source_tuple(self) -> None:
        gmp = fixture_lock(GOOD_ARCHIVE)
        mpfr = replace(gmp, role=SourceRoleV1.MPFR)
        mpfi = replace(
            gmp,
            role=SourceRoleV1.MPFI,
            integrity=ProjectPinnedArchiveDigestPolicyV1(),
        )
        lock = MpfiSourceLockV1((gmp, mpfr, mpfi))
        sources = tuple(
            admit_source_archive(release, GOOD_ARCHIVE)
            for release in lock.sources
        )
        admitted = admit_mpfi_sources(lock, sources)
        original = admitted.sources
        object.__setattr__(admitted, "sources", object())
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                provenance.replay_admitted_source_closure_v1(lock, admitted)
        finally:
            object.__setattr__(admitted, "sources", original)
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

    def test_shared_materializer_replays_only_the_exact_admitted_archive(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)

        self.assertEqual(
            _source_files(lock, admitted),
            (
                ("LICENSE", 0o644, b"license"),
                ("value", 0o644, b"data"),
            ),
        )

        mutations = (
            ("source_lock_identity", sha256(b"foreign-lock")),
            ("archive_sha256", sha256(b"foreign-archive")),
            ("tree_identity", sha256(b"foreign-tree")),
            ("regular_file_count", admitted.regular_file_count + 1),
            ("regular_file_bytes", admitted.regular_file_bytes + 1),
            ("files", tuple(reversed(admitted.files))),
        )
        for field_name, replacement in mutations:
            with self.subTest(retained_coordinate=field_name):
                original = getattr(admitted, field_name)
                object.__setattr__(admitted, field_name, replacement)
                try:
                    with self.assertRaises(ProvenanceErrorV1) as caught:
                        _source_files(lock, admitted)
                finally:
                    object.__setattr__(admitted, field_name, original)
                self.assertEqual(
                    caught.exception.reason,
                    ProvenanceReasonV1.FOREIGN_BINDING,
                )

        original_archive_bytes = admitted.archive_bytes
        object.__setattr__(admitted, "_archive_bytes", GOOD_ARCHIVE[:-1])
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                _source_files(lock, admitted)
        finally:
            object.__setattr__(admitted, "_archive_bytes", original_archive_bytes)
        self.assertEqual(
            caught.exception.reason,
            ProvenanceReasonV1.ARCHIVE_LENGTH_MISMATCH,
        )

        original_role = lock.role
        object.__setattr__(lock, "role", 999)
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                _source_files(lock, admitted)
        finally:
            object.__setattr__(lock, "role", original_role)
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

        for hostile_lock, hostile_admitted in ((object(), admitted), (lock, object())):
            with self.subTest(hostile=type(hostile_lock).__name__):
                with self.assertRaises(TypeError):
                    _source_files(hostile_lock, hostile_admitted)

    def test_replay_boundary_totalizes_hostile_nominal_coordinates(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)

        class ExplodingCoordinate:
            def __ne__(self, _other: object) -> bool:
                raise RuntimeError("hostile coordinate comparison")

        original_length = lock.archive_length
        object.__setattr__(lock, "archive_length", ExplodingCoordinate())
        try:
            for name, operation in (
                ("atomic-source-snapshot", lambda: _replay_source(lock, admitted)),
            ):
                with self.subTest(operation=name):
                    with self.assertRaises(ProvenanceErrorV1) as caught:
                        operation()
                    self.assertEqual(
                        caught.exception.reason,
                        ProvenanceReasonV1.FOREIGN_BINDING,
                    )
        finally:
            object.__setattr__(lock, "archive_length", original_length)

        class InterruptedCoordinate:
            def to_bytes(self, _length: int, _order: str) -> bytes:
                raise KeyboardInterrupt("source lock interruption")

        object.__setattr__(lock, "archive_length", InterruptedCoordinate())
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                _replay_source(lock, admitted)
        finally:
            object.__setattr__(lock, "archive_length", original_length)
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

    def test_replay_rejects_counterfeit_retained_coordinates_before_equality(
        self,
    ) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)

        class CounterfeitDigest(bytes):
            def __eq__(self, _other: object) -> bool:
                return True

            def __ne__(self, _other: object) -> bool:
                return False

        originals = {
            field_name: getattr(admitted, field_name)
            for field_name in (
                "source_lock_identity",
                "archive_sha256",
                "tree_identity",
            )
        }
        for field_name, original in originals.items():
            with self.subTest(retained_coordinate=field_name):
                object.__setattr__(
                    admitted,
                    field_name,
                    CounterfeitDigest(b"\xa5" * 32),
                )
                try:
                    for operation in (lambda: _replay_source(lock, admitted),):
                        with self.assertRaises(ProvenanceErrorV1) as caught:
                            operation()
                        self.assertEqual(
                            caught.exception.reason,
                            ProvenanceReasonV1.FOREIGN_BINDING,
                        )
                finally:
                    object.__setattr__(admitted, field_name, original)

        original_file = admitted.files[0]
        original_digest = original_file.sha256
        object.__setattr__(
            original_file,
            "sha256",
            CounterfeitDigest(b"\x91" * 32),
        )
        try:
            with self.assertRaises(ProvenanceErrorV1) as caught:
                _replay_source(lock, admitted)
        finally:
            object.__setattr__(original_file, "sha256", original_digest)
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.FOREIGN_BINDING)

    def test_replay_coordinates_keep_one_lock_snapshot_across_reentrancy(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)
        original_root_prefix = lock.root_prefix
        real_admit = provenance._admit_source_archive_once
        mutated = False

        def admit_then_mutate(
            expected: SourceReleaseLockV1,
            archive: bytes,
        ) -> tuple[provenance.SafeSourceArchiveV1, bytes]:
            nonlocal mutated
            replayed = real_admit(expected, archive)
            object.__setattr__(lock, "root_prefix", "other/")
            mutated = True
            return replayed

        try:
            with mock.patch.object(
                provenance,
                "_admit_source_archive_once",
                side_effect=admit_then_mutate,
            ):
                coordinates = _source_coordinates(lock, admitted)
        finally:
            object.__setattr__(lock, "root_prefix", original_root_prefix)
        self.assertTrue(mutated)
        self.assertEqual(
            canonical_identity(
                b"labcolors.proof-region.source-release-lock.v1\0",
                coordinates[1],
            ),
            coordinates[2],
        )

    def test_shared_materializer_is_invariant_under_regular_member_permutation(self) -> None:
        expected = (
            ("LICENSE", 0o644, b"license"),
            ("value", 0o644, b"data"),
        )
        for entries in (
            (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/value", b"data", None),
            ),
            (
                ("fixture-1/", None, None),
                ("fixture-1/value", b"data", None),
                ("fixture-1/LICENSE", b"license", None),
            ),
        ):
            with self.subTest(member_order=entries):
                archive = tar_gz(entries)
                lock = fixture_lock(archive)
                admitted = admit_source_archive(lock, archive)
                self.assertEqual(
                    provenance.materialize_admitted_source_files_v1(lock, admitted),
                    expected,
                )

    def test_shared_materializer_rechecks_the_replayed_tar_before_returning_files(self) -> None:
        lock = fixture_lock(GOOD_ARCHIVE)
        admitted = admit_source_archive(lock, GOOD_ARCHIVE)
        replayed, _ = provenance.replay_admitted_source_archive_v1(lock, admitted)
        cases = (
            (
                "body",
                raw_ustar(
                    (
                        ("LICENSE", b"license", 0o644),
                        ("value", b"evil", 0o644),
                    )
                ),
                ProvenanceReasonV1.FILE_CONTENT_MISMATCH,
            ),
            (
                "mode",
                raw_ustar(
                    (
                        ("LICENSE", b"license", 0o644),
                        ("value", b"data", 0o755),
                    )
                ),
                ProvenanceReasonV1.FOREIGN_BINDING,
            ),
            (
                "duplicate",
                raw_ustar(
                    (
                        ("LICENSE", b"license", 0o644),
                        ("value", b"data", 0o644),
                        ("value", b"data", 0o644),
                    )
                ),
                ProvenanceReasonV1.FOREIGN_BINDING,
            ),
            (
                "missing",
                raw_ustar((("LICENSE", b"license", 0o644),)),
                ProvenanceReasonV1.FOREIGN_BINDING,
            ),
        )
        for name, raw_tar, reason in cases:
            with self.subTest(mutation=name):
                with mock.patch.object(
                    provenance,
                    "_admit_source_archive_once",
                    return_value=(replayed, raw_tar),
                ):
                    with self.assertRaises(ProvenanceErrorV1) as caught:
                        provenance.materialize_admitted_source_files_v1(lock, admitted)
                self.assertEqual(caught.exception.reason, reason)

    def test_unsafe_member_kinds_and_paths_are_rejected(self) -> None:
        fixtures = (
            (ProvenanceReasonV1.UNSAFE_PATH, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/../escape", b"data", None),
            )),
            (ProvenanceReasonV1.UNSAFE_PATH, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/bad\\path", b"data", None),
            )),
            (ProvenanceReasonV1.ABSOLUTE_PATH, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("/absolute", b"data", None),
            )),
            (ProvenanceReasonV1.UNSAFE_LINK, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/link", None, b"../escape"),
            )),
            (ProvenanceReasonV1.CASE_COLLISION, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/license", b"data", None),
            )),
            (ProvenanceReasonV1.DUPLICATE_PATH, (
                ("fixture-1/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/LICENSE", b"data", None),
            )),
            (ProvenanceReasonV1.UNSAFE_PATH, (
                ("fixture-1/", None, None),
                ("fixture-1/a/b/", None, None),
                ("fixture-1/LICENSE", b"license", None),
                ("fixture-1/value", b"data", None),
            )),
        )
        for expected, entries in fixtures:
            with self.subTest(expected=expected):
                archive = tar_gz(entries)
                lock = fixture_lock(
                    archive,
                    file_count=2,
                    unpacked_bytes=11,
                )
                with self.assertRaises(ProvenanceErrorV1) as caught:
                    admit_source_archive(lock, archive)
                self.assertEqual(caught.exception.reason, expected)

    def test_special_member_and_noncanonical_compressed_stream_are_rejected(self) -> None:
        for member_type, reason in (
            (tarfile.FIFOTYPE, ProvenanceReasonV1.UNSAFE_MEMBER_TYPE),
            (tarfile.CHRTYPE, ProvenanceReasonV1.UNSAFE_MEMBER_TYPE),
            (tarfile.BLKTYPE, ProvenanceReasonV1.UNSAFE_MEMBER_TYPE),
            (tarfile.LNKTYPE, ProvenanceReasonV1.UNSAFE_LINK),
        ):
            raw = io.BytesIO()
            with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                root = tarfile.TarInfo("fixture-1/")
                root.type = tarfile.DIRTYPE
                root.mode = 0o755
                archive.addfile(root)
                license_member = tarfile.TarInfo("fixture-1/LICENSE")
                license_member.size = 7
                license_member.mode = 0o644
                archive.addfile(license_member, io.BytesIO(b"license"))
                hostile = tarfile.TarInfo("fixture-1/hostile")
                hostile.type = member_type
                hostile.mode = 0o644
                hostile.linkname = "fixture-1/LICENSE"
                archive.addfile(hostile)
            special = gzip.compress(raw.getvalue(), mtime=0)
            with self.assertRaises(ProvenanceErrorV1) as caught:
                admit_source_archive(
                    fixture_lock(special, file_count=1, unpacked_bytes=7),
                    special,
                )
            self.assertEqual(caught.exception.reason, reason)

        concatenated = GOOD_ARCHIVE + gzip.compress(b"trailing", mtime=0)
        with self.assertRaises(ProvenanceErrorV1) as caught:
            admit_source_archive(
                fixture_lock(
                    concatenated,
                    tar_stream_bytes=len(gzip.decompress(GOOD_ARCHIVE)),
                ),
                concatenated,
            )
        self.assertEqual(caught.exception.reason, ProvenanceReasonV1.TRAILING_COMPRESSED_DATA)

    def test_xz_uses_the_same_bounded_archive_law(self) -> None:
        raw_tar = gzip.decompress(GOOD_ARCHIVE)
        archive = lzma.compress(raw_tar, format=lzma.FORMAT_XZ)
        lock = fixture_lock(archive, archive_format=ArchiveFormatV1.TAR_XZ)
        admitted = admit_source_archive(lock, archive)
        self.assertEqual(admitted.regular_file_count, 2)

    def test_compressed_expansion_cannot_cross_the_locked_tar_bound(self) -> None:
        raw_tar = gzip.decompress(GOOD_ARCHIVE)
        locked_length = len(raw_tar) - 512
        for archive_format, archive in (
            (
                ArchiveFormatV1.TAR_GZIP,
                gzip.compress(raw_tar, compresslevel=9, mtime=0),
            ),
            (
                ArchiveFormatV1.TAR_XZ,
                lzma.compress(raw_tar, format=lzma.FORMAT_XZ),
            ),
        ):
            with self.subTest(archive_format=archive_format):
                lock = fixture_lock(
                    archive,
                    tar_stream_bytes=locked_length,
                    archive_format=archive_format,
                )
                with self.assertRaises(ProvenanceErrorV1) as caught:
                    admit_source_archive(lock, archive)
                self.assertEqual(
                    caught.exception.reason,
                    ProvenanceReasonV1.TAR_STREAM_LENGTH_MISMATCH,
                )

    def test_encoded_mutations_never_reenter_as_the_same_lock(self) -> None:
        lock = arb_source_lock_v1()
        encoded = lock.encode()
        accepted = 0
        for offset in range(len(encoded)):
            mutated = encoded[:offset] + bytes((encoded[offset] ^ 1,)) + encoded[offset + 1 :]
            try:
                parsed = type(lock).parse(mutated)
            except ProvenanceErrorV1:
                continue
            accepted += 1
            self.assertNotEqual(parsed.identity, lock.identity)
            self.assertEqual(parsed.encode(), mutated)
        self.assertGreater(accepted, 0)


if __name__ == "__main__":
    unittest.main()
