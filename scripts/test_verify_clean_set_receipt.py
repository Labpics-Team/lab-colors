#!/usr/bin/env python3
"""Hostile-тесты точного clean-set receipt и замыкания Cargo-пакета."""

from __future__ import annotations

import copy
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest import mock

import verify_clean_set_receipt as verifier

from verify_clean_set_receipt import (
    CODEC_PATH,
    CORE_LICENSE_EXPRESSION,
    EXCLUDED_CLAIMS,
    PRODUCT_ARTIFACT_PATHS,
    RECEIPT_PIN_PATH,
    RECEIPT_PATH,
    VerificationError,
    VerifierPolicy,
    canonical_json_bytes,
    verify_core_package,
    verify_product_receipt,
    verify_receipt,
)


REPO_ROOT = Path(__file__).resolve().parent.parent
CANONICAL_CODEC = (REPO_ROOT / CODEC_PATH).read_bytes()
TRANSITIVE_EXECUTOR_ROLES = (
    "appearance_executor_source",
    "composition_executor_source",
    "content_digest_source",
    "joint_selection_source",
    "observation_runtime_source",
    "point_attachment_source",
    "session_runtime_source",
    "signal_transport_source",
)
ATTACHMENT_PROOF_ROLES = (
    "point_attachment_allocator_oracle",
    "point_attachment_test_support",
    "point_attachment_tests",
)


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _decode_raw(codec: bytes) -> bytes:
    header_bytes = len(b"LPCC\x01\x01\x00\x00")
    index_entries = 256 + 1
    index_entry_bytes = 2
    body_offset = header_bytes + index_entries * index_entry_bytes
    offsets = [
        int.from_bytes(
            codec[
                header_bytes + index * index_entry_bytes :
                header_bytes + (index + 1) * index_entry_bytes
            ],
            "big",
        )
        for index in range(index_entries)
    ]
    body = codec[body_offset:]
    columns: list[list[tuple[int, int, int]]] = []
    for green in range(256):
        columns.append(
            [
                tuple(body[index * 3 : index * 3 + 3])
                for index in range(offsets[green], offsets[green + 1])
            ]
        )

    raw = bytearray()
    for red in range(256):
        for green in range(256):
            record = columns[green][0]
            for candidate in columns[green][1:]:
                if candidate[0] > red:
                    break
                record = candidate
            raw.extend(record[1:])
    return bytes(raw)


CANONICAL_RAW = _decode_raw(CANONICAL_CODEC)


def _git(root: Path, *args: str) -> str:
    return subprocess.check_output(
        ["git", "-C", str(root), *args],
        text=True,
        env={
            **os.environ,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "LC_ALL": "C",
        },
    ).strip()


def _write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def _artifact(path: str, role: str, license_id: str, data: bytes) -> dict[str, object]:
    return {
        "bytes": len(data),
        "license": license_id,
        "path": path,
        "role": role,
        "sha256": _sha256(data),
    }


@dataclass
class ReceiptFixture:
    product: Path
    research: Path
    receipt: dict[str, object]
    policy: VerifierPolicy

    @property
    def receipt_path(self) -> Path:
        return self.product / RECEIPT_PATH

    def write_receipt(self) -> str:
        data = canonical_json_bytes(self.receipt)
        _write(self.receipt_path, data)
        return _sha256(data)

    def write_pin(self) -> str:
        digest = self.write_receipt()
        _write(
            self.product / RECEIPT_PIN_PATH,
            f"{digest}  receipt-v1.json\n".encode("ascii"),
        )
        return digest

    def verify(self) -> None:
        verify_receipt(
            self.product,
            self.research,
            self.write_receipt(),
            policy=self.policy,
        )


def _research_fixture(root: Path) -> tuple[str, str]:
    files: dict[str, tuple[str, str, bytes]] = {}

    profile = {
        "admission": {
            "authority": "ExactTechnicalDerivation",
            "convention": "DeclaredPackagePolicyCandidate",
            "production_auto_minted": False,
        },
        "excluded_claims": list(EXCLUDED_CLAIMS),
        "geometry": {},
        "neutral_axis": {"id": "srgb8-output-neutral-axis-v1"},
        "nominal_bridge": {},
        "output_release": {},
        "policy": {},
        "release_id": "exact-nominal-srgb8-point-clean-set-v1",
        "schema": "lab-point-clean-set-srgb8-profile/1",
    }
    profile_bytes = canonical_json_bytes(profile)
    proof = {
        "admission": copy.deepcopy(profile["admission"]),
        "artifacts": {
            "certificates_bytes": 8,
            "certificates_sha256": _sha256(b"certificate"),
            "codec_bytes": len(CANONICAL_CODEC),
            "codec_sha256": _sha256(CANONICAL_CODEC),
            "profile_sha256": _sha256(profile_bytes),
            "table_bytes": len(CANONICAL_RAW),
            "table_sha256": _sha256(CANONICAL_RAW),
        },
        "certificate_encoding": {},
        "codec_encoding": {
            "body_offset": 522,
            "column_axis": "green",
            "empty_interval": [255, 0],
            "header_hex": "4c50434301010000",
            "id": "green-column-red-run-start-rle-v1",
            "index": {},
            "record": {},
            "records": 3616,
            "run_axis": "red",
        },
        "cone_certificates": [],
        "counts": {
            "accepted_chromatic": 8_232_593,
            "boundary_unproven": 0,
            "chromatic_points": 16_776_960,
            "continuous_nonempty_discrete_empty_columns": 0,
            "cube_points": 16_777_216,
            "empty_columns": 21_379,
            "full_columns": 0,
            "neutral_points": 256,
            "no_positive_ray": 0,
            "rejected_chromatic": 8_544_367,
            "singleton_columns": 1,
        },
        "excluded_claims": list(EXCLUDED_CLAIMS),
        "generator_sha256": "1" * 64,
        "inputs": [],
        "law": {
            "chromatic_accept": "q / T_policy not in Z",
            "equality": "reject",
            "neutral_outer_union": "red == green == blue",
            "runtime": "neutral or blue outside closed dirty interval",
        },
        "release_id": "exact-nominal-srgb8-point-clean-set-v1",
        "schema": "lab-point-clean-set-srgb8-proof/1",
        "witnesses": {},
    }
    proof_bytes = canonical_json_bytes(proof)

    definitions = [
        (
            "evidence/point-clean-set-srgb8/profile-v1.json",
            "semantic_profile",
            "CC-BY-SA-4.0",
            profile_bytes,
        ),
        (
            "evidence/frontier/artifact-v1.json",
            "policy_frontier",
            "CC-BY-SA-4.0",
            b"frontier",
        ),
        (
            "evidence/cie-2019/CIE_xyz_1931_2deg.csv",
            "cie_1931_2deg_source",
            "CC-BY-SA-4.0",
            b"cmf",
        ),
        (
            "evidence/cie-2019/CIE_std_illum_D65.csv",
            "cie_d65_source",
            "CC-BY-SA-4.0",
            b"d65",
        ),
        (
            "evidence/point-clean-set-srgb8/artifact-v1.bin",
            "canonical_raw_table",
            "CC-BY-SA-4.0",
            CANONICAL_RAW,
        ),
        (
            "evidence/point-clean-set-srgb8/point-clean-set-srgb8-column-rle-v1.bin",
            "runtime_codec_table",
            "CC-BY-SA-4.0",
            CANONICAL_CODEC,
        ),
        (
            "evidence/point-clean-set-srgb8/certificates-v1.bin",
            "boundary_certificates",
            "CC-BY-SA-4.0",
            b"certificate",
        ),
        ("evidence/point-clean-set-srgb8/proof-v1.json", "proof", "CC-BY-SA-4.0", proof_bytes),
        ("evidence/point-clean-set-srgb8/generate.py", "generator", "MIT", b"generator"),
        ("evidence/cie/ciegen.py", "generator_cie_reader", "MIT", b"cie generator"),
        ("evidence/point-clean-set-srgb8/verify.py", "independent_verifier", "MIT", b"verifier"),
        ("evidence/cie/ciever.py", "verifier_cie_reader", "MIT", b"cie verifier"),
        ("evidence/point-clean-set-srgb8/NOTICE.md", "data_notice", "CC-BY-SA-4.0", b"notice"),
    ]
    for path, role, license_id, data in definitions:
        files[path] = (role, license_id, data)
        _write(root / path, data)

    release = {
        "artifacts": [
            _artifact(path, role, license_id, data)
            for path, (role, license_id, data) in files.items()
        ],
        "bundle_root": "cleanliness-repository-v1",
        "codec_id": "green-column-red-run-start-rle-v1",
        "encoding_id": "raw-dirty-blue-interval-u8-pair-v1",
        "license": "CC-BY-SA-4.0",
        "release_id": "exact-nominal-srgb8-point-clean-set-v1",
        "schema": "lab-point-clean-set-srgb8-release/1",
    }
    release_bytes = canonical_json_bytes(release)
    _write(root / "evidence/point-clean-set-srgb8/release-v1.json", release_bytes)

    _git(root, "init", "--quiet")
    _git(root, "config", "user.name", "Receipt fixture")
    _git(root, "config", "user.email", "receipt@example.invalid")
    _git(root, "add", ".")
    _git(root, "commit", "--quiet", "-m", "fixture")
    return _git(root, "rev-parse", "HEAD"), _sha256(release_bytes)


def _product_fixture(root: Path, research_commit: str, release_sha256: str) -> dict[str, object]:
    artifacts = []
    for role, path in PRODUCT_ARTIFACT_PATHS.items():
        data = CANONICAL_CODEC if role == "runtime_codec" else f"{role}\n".encode()
        _write(root / path, data)
        license_id = (
            "CC-BY-4.0 AND CC-BY-SA-4.0" if role == "runtime_codec" else "MIT"
        )
        artifacts.append(_artifact(path, role, license_id, data))
    artifacts.sort(key=lambda artifact: str(artifact["role"]))

    legal_definitions = [
        ("LICENSE", "mit_text", b"MIT fixture\n"),
        (
            "crates/labcolors-core/LICENSES/CC-BY-4.0.txt",
            "cc_by_4_0_text",
            b"CC BY fixture\n",
        ),
        (
            "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt",
            "cc_by_sa_4_0_text",
            b"CC BY-SA fixture\n",
        ),
        (
            "crates/labcolors-core/NOTICE.md",
            "attribution_notice",
            (
                "Sato & Inoue 2016 DOI 10.7717/peerj.2751 CC-BY-4.0\n"
                "CIE 1931 2 Degree DOI 10.25039/CIE.DS.xvudnb9b CC-BY-SA-4.0\n"
                "CIE D65 DOI 10.25039/CIE.DS.hjfjmt59 CC-BY-SA-4.0\n"
            ).encode(),
        ),
        (
            "crates/labcolors-core/Cargo.toml",
            "core_manifest",
            (
                "[package]\n"
                'name = "labcolors-core"\n'
                f'license = "{CORE_LICENSE_EXPRESSION}"\n'
            ).encode(),
        ),
    ]
    legal_files = []
    for path, role, data in legal_definitions:
        _write(root / path, data)
        legal_files.append(
            {
                "bytes": len(data),
                "path": path,
                "role": role,
                "sha256": _sha256(data),
            }
        )
    legal_files.sort(key=lambda artifact: str(artifact["role"]))

    return {
        "admission": {
            "authority": "ExactTechnicalDerivation",
            "convention": "DeclaredPackagePolicyCandidate",
            "production_auto_minted": False,
        },
        "artifacts": artifacts,
        "excluded_claims": list(EXCLUDED_CLAIMS),
        "license_scope": {
            "codec_spdx": "CC-BY-4.0 AND CC-BY-SA-4.0",
            "core_package_spdx": CORE_LICENSE_EXPRESSION,
            "legal_files": legal_files,
            "receipt_spdx": "CC-BY-4.0 AND CC-BY-SA-4.0",
            "software_spdx": "MIT",
        },
        "release_id": "exact-nominal-srgb8-point-clean-set-v1",
        "research": {
            "commit": research_commit,
            "object_format": "sha1",
            "release_path": "evidence/point-clean-set-srgb8/release-v1.json",
            "release_sha256": release_sha256,
        },
        "runtime_contract": {
            "accepted_points": 8_232_849,
            "codec": {
                "bytes": len(CANONICAL_CODEC),
                "header_hex": "4c50434301010000",
                "id": "green-column-red-run-start-rle-v1",
                "records": 3616,
                "sha256": _sha256(CANONICAL_CODEC),
            },
            "domain_id": "encoded-srgb8-u8-cube-v1",
            "domain_points": 16_777_216,
            "law_id": "neutral-or-blue-outside-closed-dirty-interval-v1",
            "neutral_axis_id": "srgb8-output-neutral-axis-v1",
            "raw": {
                "bytes": len(CANONICAL_RAW),
                "id": "raw-dirty-blue-interval-u8-pair-v1",
                "sha256": _sha256(CANONICAL_RAW),
            },
        },
        "schema": "labcolors-exact-point-clean-set-product-receipt/1",
    }


def _fixture(root: Path) -> ReceiptFixture:
    product = root / "product"
    research = root / "research"
    product.mkdir()
    research.mkdir()
    commit, release_sha256 = _research_fixture(research)
    policy = VerifierPolicy(commit, release_sha256)
    return ReceiptFixture(
        product,
        research,
        _product_fixture(product, commit, release_sha256),
        policy,
    )


class ReceiptHostileTests(unittest.TestCase):
    def _assert_product_only_rejects_missing_roles(self, roles: tuple[str, ...]) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_pin()
            for role in roles:
                with self.subTest(role=role):
                    path = fixture.product / PRODUCT_ARTIFACT_PATHS[role]
                    original = path.read_bytes()
                    path.unlink()
                    with self.assertRaisesRegex(VerificationError, "unavailable"):
                        verify_product_receipt(fixture.product, policy=fixture.policy)
                    path.write_bytes(original)

    def _assert_product_only_rejects_mutated_roles(self, roles: tuple[str, ...]) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_pin()
            for role in roles:
                with self.subTest(role=role):
                    path = fixture.product / PRODUCT_ARTIFACT_PATHS[role]
                    original = path.read_bytes()
                    path.write_bytes(original + b"mutant\n")
                    with self.assertRaisesRegex(VerificationError, "receipt metadata"):
                        verify_product_receipt(fixture.product, policy=fixture.policy)
                    path.write_bytes(original)

    def test_valid_receipt_binds_committed_research_and_product_codec(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            _fixture(Path(temporary)).verify()

    def test_product_only_mode_detects_stale_product_source_without_research_repo(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_pin()
            result = verify_product_receipt(fixture.product, policy=fixture.policy)
            self.assertFalse(result.research_replayed)

            source = fixture.product / PRODUCT_ARTIFACT_PATHS["classifier_source"]
            source.write_bytes(source.read_bytes() + b"stale\n")
            with self.assertRaisesRegex(VerificationError, "receipt metadata"):
                verify_product_receipt(fixture.product, policy=fixture.policy)

    def test_product_only_mode_rejects_each_missing_transitive_executor(self) -> None:
        self._assert_product_only_rejects_missing_roles(TRANSITIVE_EXECUTOR_ROLES)

    def test_product_only_mode_rejects_each_mutated_transitive_executor(self) -> None:
        self._assert_product_only_rejects_mutated_roles(TRANSITIVE_EXECUTOR_ROLES)

    def test_product_only_mode_rejects_each_missing_attachment_proof_artifact(self) -> None:
        self._assert_product_only_rejects_missing_roles(ATTACHMENT_PROOF_ROLES)

    def test_product_only_mode_rejects_each_mutated_attachment_proof_artifact(self) -> None:
        self._assert_product_only_rejects_mutated_roles(ATTACHMENT_PROOF_ROLES)

    def test_product_only_mode_rejects_a_receipt_changed_without_external_pin(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_pin()
            fixture.receipt["claim"] = "not admitted"
            _write(fixture.receipt_path, canonical_json_bytes(fixture.receipt))
            with self.assertRaisesRegex(VerificationError, "caller-pinned"):
                verify_product_receipt(fixture.product, policy=fixture.policy)

    def test_product_only_mode_rejects_deleting_both_receipt_and_pin(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_pin()
            fixture.receipt_path.unlink()
            (fixture.product / RECEIPT_PIN_PATH).unlink()
            with self.assertRaisesRegex(VerificationError, "product receipt pin"):
                verify_product_receipt(fixture.product, policy=fixture.policy)

    def test_product_pin_rejects_noncanonical_name(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            digest = fixture.write_receipt()
            _write(
                fixture.product / RECEIPT_PIN_PATH,
                f"{digest}  other.json\n".encode("ascii"),
            )
            with self.assertRaisesRegex(VerificationError, "receipt-v1.json"):
                verify_product_receipt(fixture.product, policy=fixture.policy)

    def test_product_pin_rejects_noncanonical_digest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_receipt()
            _write(
                fixture.product / RECEIPT_PIN_PATH,
                b"A" * 64 + b"  receipt-v1.json\n",
            )
            with self.assertRaisesRegex(VerificationError, "lower-case SHA-256"):
                verify_product_receipt(fixture.product, policy=fixture.policy)

    def test_git_timeout_is_a_verification_error_when_output_is_absent(self) -> None:
        timeout = subprocess.TimeoutExpired(["git"], 1, output=None)
        with mock.patch.object(
            verifier.subprocess,
            "check_output",
            side_effect=timeout,
        ) as check_output:
            with self.assertRaisesRegex(VerificationError, "research commit Git lookup failed"):
                verifier._git(Path("."), ["cat-file", "-t", "0" * 40], "research commit")

        self.assertEqual(
            check_output.call_args.kwargs.get("timeout"),
            verifier.GIT_LOOKUP_TIMEOUT_SECONDS,
        )
        self.assertGreater(verifier.GIT_LOOKUP_TIMEOUT_SECONDS, 0)

    def test_numeric_zero_cannot_impersonate_false(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.receipt["admission"]["production_auto_minted"] = 0
            with self.assertRaisesRegex(VerificationError, "production_auto_minted"):
                fixture.verify()

    def test_duplicate_json_key_is_rejected_before_semantic_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            source = canonical_json_bytes(fixture.receipt)
            duplicate = source[:-2] + b',\n  "schema": "duplicate"\n}\n'
            _write(fixture.receipt_path, duplicate)
            with self.assertRaisesRegex(VerificationError, "duplicate JSON key"):
                verify_receipt(
                    fixture.product,
                    fixture.research,
                    _sha256(duplicate),
                    policy=fixture.policy,
                )

    def test_receipt_requires_an_external_caller_pinned_digest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.write_receipt()
            with self.assertRaisesRegex(VerificationError, "caller-pinned"):
                verify_receipt(
                    fixture.product,
                    fixture.research,
                    "0" * 64,
                    policy=fixture.policy,
                )

    def test_path_traversal_is_rejected_even_when_digest_matches(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.receipt["artifacts"][0]["path"] = "../escape"
            with self.assertRaisesRegex(VerificationError, "portable relative path"):
                fixture.verify()

    def test_dirty_research_worktree_cannot_replace_committed_release(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            release = fixture.research / "evidence/point-clean-set-srgb8/release-v1.json"
            release.write_text("not the committed release\n", encoding="utf-8")
            fixture.verify()

    def test_wrong_research_commit_is_rejected_before_blob_lookup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.receipt["research"]["commit"] = "0" * 40
            with self.assertRaisesRegex(VerificationError, "research commit"):
                fixture.verify()

    def test_new_commit_cannot_rebless_a_different_release_blob(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            release = fixture.research / "evidence/point-clean-set-srgb8/release-v1.json"
            release.write_bytes(release.read_bytes() + b"\n")
            _git(fixture.research, "add", ".")
            _git(fixture.research, "commit", "--quiet", "-m", "mutated release")
            new_commit = _git(fixture.research, "rev-parse", "HEAD")
            fixture.receipt["research"]["commit"] = new_commit
            fixture.policy = VerifierPolicy(
                new_commit,
                fixture.policy.research_release_sha256,
            )
            with self.assertRaisesRegex(VerificationError, "release blob"):
                fixture.verify()

    def test_one_codec_bit_cannot_be_reblessed_by_artifact_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            codec_path = fixture.product / CODEC_PATH
            mutant = bytearray(codec_path.read_bytes())
            mutant[-1] ^= 1
            codec_path.write_bytes(mutant)
            codec_artifact = next(
                item for item in fixture.receipt["artifacts"] if item["role"] == "runtime_codec"
            )
            codec_artifact["sha256"] = _sha256(mutant)
            with self.assertRaisesRegex(VerificationError, "runtime codec identity"):
                fixture.verify()

    def test_unknown_receipt_field_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            fixture.receipt["claim"] = "human cleanliness law"
            with self.assertRaisesRegex(VerificationError, "receipt fields"):
                fixture.verify()

    def test_rehashed_mit_only_core_manifest_cannot_bypass_product_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fixture = _fixture(Path(temporary))
            manifest = b'[package]\nname = "labcolors-core"\nlicense = "MIT"\n'
            manifest_path = fixture.product / "crates/labcolors-core/Cargo.toml"
            manifest_path.write_bytes(manifest)
            manifest_entry = next(
                item
                for item in fixture.receipt["license_scope"]["legal_files"]
                if item["role"] == "core_manifest"
            )
            manifest_entry["bytes"] = len(manifest)
            manifest_entry["sha256"] = _sha256(manifest)
            with self.assertRaisesRegex(VerificationError, "package license"):
                fixture.verify()


class CorePackageLicenseTests(unittest.TestCase):
    def _package_fixture(self, root: Path) -> tuple[Path, Path]:
        source = root / "source"
        package = root / "package"
        source.mkdir()
        package.mkdir()

        receipt = b'{"fixture":true}\n'
        receipt_pin = f"{_sha256(receipt)}  receipt-v1.json\n".encode("ascii")
        files = {
            "LICENSE": b"MIT fixture\n",
            "crates/labcolors-core/LICENSES/CC-BY-4.0.txt": b"CC BY fixture\n",
            "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt": b"CC BY-SA fixture\n",
            "crates/labcolors-core/NOTICE.md": (
                b"Sato & Inoue DOI 10.7717/peerj.2751 CC-BY-4.0\n"
                b"CIE DOI 10.25039/CIE.DS.xvudnb9b CC-BY-SA-4.0\n"
                b"D65 DOI 10.25039/CIE.DS.hjfjmt59 CC-BY-SA-4.0\n"
            ),
            CODEC_PATH: CANONICAL_CODEC,
            RECEIPT_PATH: receipt,
            RECEIPT_PIN_PATH: receipt_pin,
        }
        for path, data in files.items():
            _write(source / path, data)

        packaged = {
            "LICENSE": files["LICENSE"],
            "LICENSES/CC-BY-4.0.txt": files[
                "crates/labcolors-core/LICENSES/CC-BY-4.0.txt"
            ],
            "LICENSES/CC-BY-SA-4.0.txt": files[
                "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt"
            ],
            "NOTICE.md": files["crates/labcolors-core/NOTICE.md"],
            "contracts/clean-set-srgb8-v1/point-clean-set-srgb8-column-rle-v1.bin": CANONICAL_CODEC,
            "contracts/clean-set-srgb8-v1/receipt-v1.json": receipt,
            "contracts/clean-set-srgb8-v1/receipt-v1.sha256": receipt_pin,
            "Cargo.toml": (
                "[package]\n"
                'name = "labcolors-core"\n'
                f'license = "{CORE_LICENSE_EXPRESSION}"\n'
            ).encode(),
        }
        for path, data in packaged.items():
            _write(package / path, data)
        return source, package

    def test_exact_core_package_license_closure_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            verify_core_package(source, package)

    def test_mit_only_package_metadata_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            # write_bytes avoids Windows CRLF translation that triggers the LF
            # line-ending guard before the license check can run.
            (package / "Cargo.toml").write_bytes(
                b'[package]\nname = "labcolors-core"\nlicense = "MIT"\n',
            )
            with self.assertRaisesRegex(VerificationError, "package license"):
                verify_core_package(source, package)

    def test_array_table_cannot_supply_the_package_license(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            # write_bytes avoids Windows CRLF translation that triggers the LF
            # line-ending guard before the license check can run.
            (package / "Cargo.toml").write_bytes(
                (
                    '[package]\nname = "labcolors-core"\n\n'
                    '[[bin]]\nname = "fixture"\npath = "src/main.rs"\n'
                    f'license = "{CORE_LICENSE_EXPRESSION}"\n'
                ).encode(),
            )
            with self.assertRaisesRegex(VerificationError, "package license"):
                verify_core_package(source, package)

    def test_commented_table_headers_cannot_supply_the_package_license(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            for table_header in (
                "[[bin]] # executable target",
                "[dependencies] # package table has ended",
            ):
                with self.subTest(table_header=table_header):
                    # write_bytes avoids Windows CRLF translation that triggers
                    # the LF line-ending guard before the license check can run.
                    (package / "Cargo.toml").write_bytes(
                        (
                            '[package]\nname = "labcolors-core"\n\n'
                            f"{table_header}\n"
                            f'license = "{CORE_LICENSE_EXPRESSION}"\n'
                        ).encode(),
                    )
                    with self.assertRaisesRegex(VerificationError, "package license"):
                        verify_core_package(source, package)

    def test_missing_cc_text_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            (package / "LICENSES/CC-BY-4.0.txt").unlink()
            with self.assertRaisesRegex(VerificationError, "CC-BY-4.0"):
                verify_core_package(source, package)

    def test_missing_packaged_receipt_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            (package / "contracts/clean-set-srgb8-v1/receipt-v1.json").unlink()
            with self.assertRaisesRegex(VerificationError, "receipt-v1.json"):
                verify_core_package(source, package)

    def test_missing_packaged_receipt_pin_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            (package / "contracts/clean-set-srgb8-v1/receipt-v1.sha256").unlink()
            with self.assertRaisesRegex(VerificationError, "receipt-v1.sha256"):
                verify_core_package(source, package)

    def test_coherently_substituted_packaged_receipt_and_pin_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            receipt = b'{"substituted":true}\n'
            (package / "contracts/clean-set-srgb8-v1/receipt-v1.json").write_bytes(receipt)
            (package / "contracts/clean-set-srgb8-v1/receipt-v1.sha256").write_bytes(
                f"{_sha256(receipt)}  receipt-v1.json\n".encode("ascii")
            )
            with self.assertRaisesRegex(VerificationError, "canonical product bytes"):
                verify_core_package(source, package)

    def test_notice_without_one_source_attribution_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            notice = package / "NOTICE.md"
            notice.write_bytes(notice.read_bytes().replace(b"10.7717/peerj.2751", b"missing"))
            source_notice = source / "crates/labcolors-core/NOTICE.md"
            source_notice.write_bytes(
                source_notice.read_bytes().replace(b"10.7717/peerj.2751", b"missing")
            )
            with self.assertRaisesRegex(VerificationError, "10.7717/peerj.2751"):
                verify_core_package(source, package)

    @unittest.skipUnless(hasattr(os, "symlink"), "platform has no symlink support")
    def test_symlinked_notice_is_not_a_packaged_regular_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, package = self._package_fixture(Path(temporary))
            notice = package / "NOTICE.md"
            notice.unlink()
            notice.symlink_to(package / "LICENSE")
            with self.assertRaisesRegex(VerificationError, "symlink"):
                verify_core_package(source, package)


if __name__ == "__main__":
    unittest.main(verbosity=2)
