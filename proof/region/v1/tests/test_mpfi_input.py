#!/usr/bin/env python3
"""Контракт границы MPFI admitted-source → sealed input."""

from __future__ import annotations

import ast
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
from build import input as build_input  # noqa: E402
from mpfi import input as mpfi_input  # noqa: E402


# Characterization-pins маленького three-source closure. Versioned layout
# меняет их только явным решением, а не silent drift.
SYNTHETIC_MPFI_INPUT_LENGTH_V1 = 10_240
SYNTHETIC_MPFI_INPUT_SHA256_V1 = (
    "fac4761a9018ca55f467328dae238ccea0a280c08277e533a7bbef696eea567f"
)
SYNTHETIC_MPFI_INPUT_BINDING_V1 = (
    "d0adc51b30e68b672efcf7f3a4be4a6ec4171b9a1f053ef52787adeb713b6653"
)


def _sha256(value: bytes) -> bytes:
    return hashlib.sha256(value).digest()


def _detached_policy() -> provenance.DetachedSignaturePolicyV1:
    return provenance.DetachedSignaturePolicyV1(
        "https://example.invalid/source.tar.xz.sig",
        3,
        _sha256(b"signature"),
        _sha256(b"packets"),
        bytes.fromhex("00112233445566778899aabbccddeeff00112233"),
    )


def _fixture_archive(
    *,
    license_body: bytes,
    value_body: bytes,
    value_mode: int = 0o644,
) -> bytes:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        root = tarfile.TarInfo("shared/")
        root.type = tarfile.DIRTYPE
        root.mode = 0o755
        root.uid = 0
        root.gid = 0
        root.mtime = 0
        archive.addfile(root)
        for name, body, mode in (
            ("LICENSE", license_body, 0o644),
            ("value", value_body, value_mode),
        ):
            member = tarfile.TarInfo(f"shared/{name}")
            member.mode = mode
            member.uid = 0
            member.gid = 0
            member.mtime = 0
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))
    return lzma.compress(raw.getvalue(), format=lzma.FORMAT_XZ)


def _fixture_release(
    role: provenance.SourceRoleV1,
    archive: bytes,
    *,
    license_body: bytes,
    value_body: bytes,
) -> provenance.SourceReleaseLockV1:
    raw_tar = lzma.decompress(archive)
    integrity: (
        provenance.DetachedSignaturePolicyV1
        | provenance.ProjectPinnedArchiveDigestPolicyV1
    )
    if role is provenance.SourceRoleV1.MPFI:
        integrity = provenance.ProjectPinnedArchiveDigestPolicyV1()
    else:
        integrity = _detached_policy()
    return provenance.SourceReleaseLockV1(
        role,
        "1",
        f"https://example.invalid/{role.name.lower()}.tar.xz",
        provenance.ArchiveFormatV1.TAR_XZ,
        len(archive),
        _sha256(archive),
        len(raw_tar),
        "shared/",
        2,
        len(license_body) + len(value_body),
        (provenance.LegalFileV1("LICENSE", len(license_body), _sha256(license_body)),),
        integrity,
    )


def _admitted_closure(
    *,
    mpfr_value_mode: int = 0o755,
) -> tuple[
    provenance.MpfiSourceLockV1,
    provenance.AdmittedMpfiSourcesV1,
    tuple[tuple[str, int, bytes], ...],
]:
    specifications = (
        (provenance.SourceRoleV1.GMP, b"gmp-license", b"gmp-source", 0o644),
        (provenance.SourceRoleV1.MPFR, b"mpfr-license", b"mpfr-source", mpfr_value_mode),
        (provenance.SourceRoleV1.MPFI, b"mpfi-license", b"mpfi-source", 0o644),
    )
    releases: list[provenance.SourceReleaseLockV1] = []
    archives: list[bytes] = []
    expected_entries: list[tuple[str, int, bytes]] = []
    for role, license_body, value_body, value_mode in specifications:
        archive = _fixture_archive(
            license_body=license_body,
            value_body=value_body,
            value_mode=value_mode,
        )
        archives.append(archive)
        releases.append(
            _fixture_release(
                role,
                archive,
                license_body=license_body,
                value_body=value_body,
            )
        )
        namespace = role.name.lower()
        expected_entries.extend(
            (
                (f"sources/{namespace}/LICENSE", 0o644, license_body),
                (f"sources/{namespace}/value", value_mode, value_body),
            )
        )
    lock = provenance.MpfiSourceLockV1(tuple(releases))
    sources = tuple(
        provenance.admit_source_archive(release, archive)
        for release, archive in zip(lock.sources, archives, strict=True)
    )
    return (
        lock,
        provenance.admit_mpfi_sources(lock, sources),
        tuple(sorted(expected_entries)),
    )


def _limits_for_entries(
    entries: tuple[tuple[str, int, bytes], ...],
) -> build_input.CanonicalInputLimitsV1:
    directories = {
        "/".join(path.split("/")[:length])
        for path, _mode, _contents in entries
        for length in range(1, len(path.split("/")))
    }
    return build_input.CanonicalInputLimitsV1(
        len(entries) + len(directories),
        max(len(contents) for _path, _mode, contents in entries),
        sum(len(contents) for _path, _mode, contents in entries),
    )


def _regular_ustar_entries(value: build_input.SealedInputV1) -> tuple[tuple[str, int, bytes], ...]:
    with tarfile.open(fileobj=io.BytesIO(value.contents), mode="r:") as archive:
        return tuple(
            (member.name, member.mode, archive.extractfile(member).read())
            for member in archive.getmembers()
            if member.isreg()
        )


def _imported_module_names(source: str) -> tuple[str, ...]:
    modules: list[str] = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Import):
            modules.extend(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            prefix = node.module or ""
            modules.extend(
                ".".join(part for part in (prefix, alias.name) if part)
                for alias in node.names
            )
    return tuple(modules)


class MpfiSourceInputTests(unittest.TestCase):
    def test_three_same_root_archives_become_one_deterministic_lane_input(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)

        first = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        second = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)

        self.assertIs(type(first), build_input.SealedInputV1)
        self.assertTrue(build_input.sealed_input_is_intact_v1(first))
        self.assertEqual(first, second)
        self.assertEqual(first.length, SYNTHETIC_MPFI_INPUT_LENGTH_V1)
        self.assertEqual(first.sha256.hex(), SYNTHETIC_MPFI_INPUT_SHA256_V1)
        self.assertEqual(
            first.binding_identity.hex(),
            SYNTHETIC_MPFI_INPUT_BINDING_V1,
        )
        self.assertEqual(_regular_ustar_entries(first), expected_entries)
        self.assertTrue(
            mpfi_input.mpfi_source_input_is_bound_v1(lock, admitted, limits, first)
        )
        relaxed_limits = build_input.CanonicalInputLimitsV1(
            limits.max_members + 1,
            limits.max_file_bytes + 1,
            limits.max_payload_bytes + 1,
        )
        self.assertTrue(
            mpfi_input.mpfi_source_input_is_bound_v1(
                lock,
                admitted,
                relaxed_limits,
                first,
            )
        )
        with tarfile.open(fileobj=io.BytesIO(first.contents), mode="r:") as archive:
            members = archive.getmembers()
            self.assertEqual(
                tuple(sorted(member.name for member in members if member.isdir())),
                ("sources", "sources/gmp", "sources/mpfi", "sources/mpfr"),
            )
            self.assertTrue(all(member.uid == 0 and member.gid == 0 for member in members))
            self.assertTrue(all(member.mtime == 0 for member in members))

    def test_binding_rechecks_the_closure_and_exact_ustar_bytes(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)

        foreign_binding = build_input.seal_input_v1(_sha256(b"foreign"), sealed.contents)
        changed_entries = list(expected_entries)
        path, mode, contents = changed_entries[0]
        changed_entries[0] = (path, mode, contents + b"!")
        changed_contents = build_input.canonical_ustar_v1(
            tuple(changed_entries),
            _limits_for_entries(tuple(changed_entries)),
        )
        stale_binding = build_input.seal_input_v1(sealed.binding_identity, changed_contents)

        self.assertTrue(build_input.sealed_input_is_intact_v1(foreign_binding))
        self.assertTrue(build_input.sealed_input_is_intact_v1(stale_binding))
        self.assertFalse(
            mpfi_input.mpfi_source_input_is_bound_v1(
                lock,
                admitted,
                limits,
                foreign_binding,
            )
        )
        self.assertFalse(
            mpfi_input.mpfi_source_input_is_bound_v1(
                lock,
                admitted,
                limits,
                stale_binding,
            )
        )

    def test_reordered_or_foreign_closure_is_rejected_before_ustar_encoding(
        self,
    ) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        _ = admitted.identity
        original_sources = admitted.sources
        object.__setattr__(
            admitted,
            "sources",
            (original_sources[1], original_sources[0], original_sources[2]),
        )
        try:
            with mock.patch.object(mpfi_input.build_input, "canonical_ustar_v1") as encoder:
                with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                    mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            encoder.assert_not_called()
        finally:
            object.__setattr__(admitted, "sources", original_sources)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )

    def test_lock_identity_cannot_hide_source_or_capability_drift(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        original_version = lock.sources[0].version
        object.__setattr__(lock.sources[0], "version", "2")
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(lock, admitted, limits, sealed)
            )
        finally:
            object.__setattr__(lock.sources[0], "version", original_version)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )

        source = admitted.sources[2]
        original_tree_identity = source.tree_identity
        object.__setattr__(source, "tree_identity", _sha256(b"foreign tree"))
        try:
            with self.assertRaises(provenance.ProvenanceErrorV1):
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(lock, admitted, limits, sealed)
            )
        finally:
            object.__setattr__(source, "tree_identity", original_tree_identity)

    def test_wrong_public_capability_types_are_typed_rejections(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        for hostile_lock, hostile_admitted, hostile_limits, field_name in (
            (object(), admitted, limits, "source_lock"),
            (lock, object(), limits, "admitted_sources"),
            (lock, admitted, object(), "limits"),
        ):
            with self.subTest(field=field_name):
                with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                    mpfi_input.seal_mpfi_source_input_v1(
                        hostile_lock,
                        hostile_admitted,
                        hostile_limits,
                    )
                self.assertEqual(
                    caught.exception.reason,
                    mpfi_input.MpfiSourceInputReasonV1.WRONG_TYPE,
                )
                self.assertEqual(caught.exception.field, field_name)

    def test_locked_mode_is_preserved_without_silent_normalization(self) -> None:
        lock, admitted, expected_entries = _admitted_closure(mpfr_value_mode=0o700)

        sealed = mpfi_input.seal_mpfi_source_input_v1(
            lock,
            admitted,
            _limits_for_entries(expected_entries),
        )

        self.assertEqual(_regular_ustar_entries(sealed), expected_entries)

    def test_limits_reject_declared_closure_before_archive_materialization(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        total_files = len(expected_entries)
        total_payload = sum(len(contents) for _path, _mode, contents in expected_entries)
        max_file = max(len(contents) for _path, _mode, contents in expected_entries)
        cases = (
            (
                build_input.CanonicalInputLimitsV1(total_files - 1, max_file, total_payload),
                "max_members",
            ),
            (
                build_input.CanonicalInputLimitsV1(total_files + 4, max_file, total_payload - 1),
                "max_payload_bytes",
            ),
        )
        for limits, field in cases:
            with self.subTest(limit=field):
                with mock.patch.object(
                    mpfi_input.provenance,
                    "replay_admitted_source_closure_v1",
                ) as replay_closure:
                    with self.assertRaises(build_input.InputErrorV1) as caught:
                        mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
                replay_closure.assert_not_called()
                self.assertEqual(caught.exception.reason, build_input.InputReasonV1.RESOURCE_LIMIT)
                self.assertEqual(caught.exception.field, field)

    def test_final_ustar_limits_remain_typed_rejections(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        cases = (
            (
                build_input.CanonicalInputLimitsV1(
                    limits.max_members,
                    limits.max_file_bytes - 1,
                    limits.max_payload_bytes,
                ),
                "max_file_bytes",
            ),
            (
                build_input.CanonicalInputLimitsV1(
                    limits.max_members,
                    limits.max_file_bytes,
                    limits.max_payload_bytes,
                    sealed.length - 1,
                ),
                "max_encoded_bytes",
            ),
        )
        for constrained, field in cases:
            with self.subTest(limit=field):
                with self.assertRaises(build_input.InputErrorV1) as caught:
                    mpfi_input.seal_mpfi_source_input_v1(lock, admitted, constrained)
                self.assertEqual(caught.exception.reason, build_input.InputReasonV1.RESOURCE_LIMIT)
                self.assertEqual(caught.exception.field, field)
                self.assertFalse(
                    mpfi_input.mpfi_source_input_is_bound_v1(
                        lock,
                        admitted,
                        constrained,
                        sealed,
                    )
                )

    def test_missing_capability_field_is_a_typed_rejection(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        original = admitted.source_lock_identity
        object.__delattr__(admitted, "source_lock_identity")
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(
                    lock,
                    admitted,
                    limits,
                )
        finally:
            object.__setattr__(admitted, "source_lock_identity", original)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )
        self.assertEqual(caught.exception.field, "admitted_sources")

        class ExplodesOnComparison:
            def __ne__(self, _other: object) -> bool:
                raise RuntimeError("comparison ran")

        object.__setattr__(admitted, "source_lock_identity", ExplodesOnComparison())
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        finally:
            object.__setattr__(admitted, "source_lock_identity", original)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )
        self.assertEqual(caught.exception.field, "admitted_sources")

    def test_hostile_exact_source_capability_stays_a_typed_replay_failure(
        self,
    ) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        source = admitted.sources[0]
        original = source.source_lock_identity

        class ExplodesOnComparison:
            def __ne__(self, _other: object) -> bool:
                raise RuntimeError("comparison ran")

        object.__setattr__(source, "source_lock_identity", ExplodesOnComparison())
        try:
            with self.assertRaises(provenance.ProvenanceErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(
                    lock,
                    admitted,
                    limits,
                    sealed,
                )
            )
        finally:
            object.__setattr__(source, "source_lock_identity", original)
        self.assertEqual(
            caught.exception.reason,
            provenance.ProvenanceReasonV1.FOREIGN_BINDING,
        )

    def test_source_input_keeps_one_snapshot_across_reentrant_source_mutation(
        self,
    ) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        source = admitted.sources[0]
        original_tree_identity = source.tree_identity
        real_encoder = build_input.canonical_ustar_v1

        def encode_then_mutate(
            entries: tuple[tuple[str, int, bytes], ...],
            encoder_limits: build_input.CanonicalInputLimitsV1,
        ) -> bytes:
            encoded = real_encoder(entries, encoder_limits)
            object.__setattr__(source, "tree_identity", _sha256(b"reentrant-tree"))
            return encoded

        try:
            with mock.patch.object(
                mpfi_input.build_input,
                "canonical_ustar_v1",
                side_effect=encode_then_mutate,
            ):
                sealed = mpfi_input.seal_mpfi_source_input_v1(
                    lock,
                    admitted,
                    limits,
                )
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(
                    lock,
                    admitted,
                    limits,
                    sealed,
                )
            )
        finally:
            object.__setattr__(source, "tree_identity", original_tree_identity)
        self.assertTrue(
            mpfi_input.mpfi_source_input_is_bound_v1(
                lock,
                admitted,
                limits,
                sealed,
            )
        )

    def test_hostile_replayed_source_stays_a_typed_provenance_failure(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        source = admitted.sources[0]
        original = source.files

        class ExplodesOnComparison:
            def __eq__(self, _other: object) -> bool:
                raise RuntimeError("comparison ran")

        object.__setattr__(source, "files", ExplodesOnComparison())
        try:
            with self.assertRaises(provenance.ProvenanceErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(
                    lock,
                    admitted,
                    limits,
                    sealed,
                )
            )
        finally:
            object.__setattr__(source, "files", original)
        self.assertEqual(
            caught.exception.reason,
            provenance.ProvenanceReasonV1.FOREIGN_BINDING,
        )

    def test_noncanonical_exact_type_lock_is_a_typed_rejection(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        original_version = lock.sources[0].version
        object.__setattr__(lock.sources[0], "version", "\0")
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(
                    lock,
                    admitted,
                    _limits_for_entries(expected_entries),
                )
        finally:
            object.__setattr__(lock.sources[0], "version", original_version)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )

    def test_hostile_exact_lock_cannot_escape_public_boundary(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
        original_sources = lock.sources

        class ExplodesOnEncode:
            def encode(self) -> bytes:
                raise RuntimeError("encode ran")

        object.__setattr__(lock, "sources", (ExplodesOnEncode(),) * 3)
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertFalse(
                mpfi_input.mpfi_source_input_is_bound_v1(
                    lock,
                    admitted,
                    limits,
                    sealed,
                )
            )
        finally:
            object.__setattr__(lock, "sources", original_sources)
        self.assertEqual(
            caught.exception.reason,
            mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )
        self.assertEqual(caught.exception.field, "source_lock")

    def test_source_lock_encoder_shadow_cannot_select_foreign_closure(self) -> None:
        lock, admitted, expected_entries = _admitted_closure()
        limits = _limits_for_entries(expected_entries)
        foreign_releases = list(lock.sources)
        foreign_releases[0] = replace(foreign_releases[0], version="shadowed")
        foreign_lock = provenance.MpfiSourceLockV1(tuple(foreign_releases))
        foreign_sources = tuple(
            provenance.admit_source_archive(release, source.archive_bytes)
            for release, source in zip(
                foreign_lock.sources,
                admitted.sources,
                strict=True,
            )
        )
        foreign_admitted = provenance.admit_mpfi_sources(
            foreign_lock,
            foreign_sources,
        )
        lock.__dict__["encode"] = lambda: provenance.MpfiSourceLockV1.encode(
            foreign_lock
        )
        try:
            with self.assertRaises(mpfi_input.MpfiSourceInputErrorV1) as caught:
                mpfi_input.seal_mpfi_source_input_v1(
                    lock,
                    foreign_admitted,
                    limits,
                )
            self.assertEqual(
                caught.exception.reason,
                mpfi_input.MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            )

            sealed = mpfi_input.seal_mpfi_source_input_v1(lock, admitted, limits)
            self.assertTrue(
                mpfi_input.mpfi_source_input_is_bound_v1(
                    lock,
                    admitted,
                    limits,
                    sealed,
                )
            )
        finally:
            del lock.__dict__["encode"]

    def test_source_input_owner_has_no_engine_dependency(self) -> None:
        source_path = ROOT / "mpfi" / "input.py"
        self.assertTrue(source_path.is_file())
        source = source_path.read_text(encoding="utf-8")
        tree = ast.parse(source)
        imported_modules = _imported_module_names(source)
        forbidden = (
            "arb",
            "pipeline",
            "receipt",
            "executor",
            "transport",
            "formula",
            "controller",
            "region_proof_protocol",
        )
        self.assertFalse(
            any(
                name in module.split(".")
                for module in imported_modules
                for name in forbidden
            ),
        )
        self.assertFalse(
            any(
                isinstance(node, ast.Call)
                and (
                    (
                        isinstance(node.func, ast.Name)
                        and node.func.id == "__import__"
                    )
                    or (
                        isinstance(node.func, ast.Attribute)
                        and node.func.attr == "import_module"
                    )
                )
                for node in ast.walk(tree)
            ),
        )
        self.assertFalse((ROOT / "mpfi" / "build").exists())

    def test_import_guard_resolves_from_import_targets(self) -> None:
        self.assertEqual(
            _imported_module_names(
                "from build import transport\n"
                "from proof.region.v1.arb import pipeline\n"
            ),
            ("build.transport", "proof.region.v1.arb.pipeline"),
        )

    def test_protocol_keeps_the_source_input_boundary_below_build_authority(self) -> None:
        protocol = (ROOT / "PROTOCOL.md").read_text(encoding="utf-8")
        source_input_start = protocol.index("`mpfi/input.py`")
        transport_start = protocol.index(
            "`proof/region/v1/build/transport.py`",
            source_input_start,
        )
        arb_replay_start = protocol.index("## Воспроизведение Arb", transport_start)
        reference = " ".join(protocol[source_input_start:transport_start].split())
        transport_reference = " ".join(protocol[transport_start:arb_replay_start].split())
        source_path = ROOT / "mpfi" / "input.py"
        source_text = source_path.read_text(encoding="utf-8")
        # Тест намеренно запускается с ``-OO``. Явно выключаем оптимизацию
        # parser-а, чтобы контракт документации наблюдался по исходнику, а не
        # случайно зависел от сохранения runtime ``__doc__``.
        source_tree = compile(
            source_text,
            str(source_path),
            "exec",
            flags=ast.PyCF_ONLY_AST,
            optimize=0,
        )
        seal_function = next(
            node
            for node in source_tree.body
            if isinstance(node, ast.FunctionDef)
            and node.name == "seal_mpfi_source_input_v1"
        )
        seal_reference = ast.get_docstring(seal_function)

        self.assertIn("`sources/<role>/<relative>`", reference)
        self.assertIn("не требует уникальности root", reference)
        self.assertIn("Caller передаёт canonical `CanonicalInputLimitsV1`", reference)
        self.assertIn("`MpfiSourceInputErrorV1`", reference)
        self.assertIn("`ProvenanceErrorV1`", reference)
        self.assertIn("`InputErrorV1`", reference)
        self.assertIn("MPFI source-input binding", reference)
        self.assertIn("materialization archive", reference)
        self.assertIn(
            "не вводит recipe, Docker policy, BUILD/RUN authority",
            reference,
        )
        self.assertIn(
            "MPFI sealed source input ещё не является MPFI build policy",
            transport_reference,
        )
        self.assertIn("engine-owned input binding", transport_reference)
        self.assertIsNotNone(seal_reference)
        self.assertIn("failure exact archive replay", seal_reference)


if __name__ == "__main__":
    unittest.main(verbosity=2)
