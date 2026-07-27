#!/usr/bin/env python3
"""Офлайн-верификатор точного product receipt для clean-set sRGB8.

Receipt намеренно вынесен из исследовательского репозитория: он связывает один
source-cone продукта с неизменяемым исследовательским коммитом, не копируя в
продукт исходную таблицу, сертификаты, датасеты и исследовательские программы.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, NoReturn


RECEIPT_PATH = "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json"
RECEIPT_PIN_PATH = "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256"
CODEC_PATH = (
    "crates/labcolors-core/contracts/clean-set-srgb8-v1/"
    "point-clean-set-srgb8-column-rle-v1.bin"
)
RESEARCH_RELEASE_PATH = "evidence/point-clean-set-srgb8/release-v1.json"
RESEARCH_COMMIT = "ac6d9654fc722334d8bc2054afb903770f2aad80"
RESEARCH_RELEASE_SHA256 = (
    "67cadaae38bbaea3096dba69142b5bf3d7776b7574ec224022abbcd119c45ce6"
)

RELEASE_ID = "exact-nominal-srgb8-point-clean-set-v1"
RECEIPT_SCHEMA = "labcolors-exact-point-clean-set-product-receipt/1"
CORE_LICENSE_EXPRESSION = "MIT AND CC-BY-4.0 AND CC-BY-SA-4.0"
DATA_LICENSE_EXPRESSION = "CC-BY-4.0 AND CC-BY-SA-4.0"

CODEC_SHA256 = "aa6aa7c0b630437f1c1ba8c2ceafb0dadf6551c42331559504076a6cd44e6331"
RAW_SHA256 = "97bcc9f793adb7f13bd70c89e9788c8ab61baf8c77e9f8cd80335ad767d71ae2"
CODEC_HEADER = b"LPCC\x01\x01\x00\x00"
# LPCC v1 связывает 256 green-колонок конечным смещением и трёхбайтовыми
# записями; эти величины меняются только вместе с версией формата в заголовке.
CODEC_INDEX_ENTRIES = 256 + 1
CODEC_INDEX_ENTRY_BYTES = 2
CODEC_RECORD_BYTES = 3
CODEC_BODY_OFFSET = len(CODEC_HEADER) + CODEC_INDEX_ENTRIES * CODEC_INDEX_ENTRY_BYTES
CODEC_BYTES = 11_370
CODEC_RECORDS = 3_616
RAW_BYTES = 131_072
DOMAIN_POINTS = 16_777_216
ACCEPTED_POINTS = 8_232_849

# Здесь Git читает только локальные неизменяемые объекты. 30 секунд — принятый
# операционный предел быстрого отказа, а не замер скорости; менять его следует
# по замеру самого медленного поддерживаемого репозитория и runner с явным запасом.
GIT_LOOKUP_TIMEOUT_SECONDS = 30

EXCLUDED_CLAIMS = (
    "ideal algebraic IEC 61966-2-1 transfer semantics",
    "chromatic adaptation",
    "physical applicability of object-colour geometry to self-luminous display",
    "human cleanliness law or population guarantee",
)

# Первый точный product receipt намеренно связывает файлы целиком. После
# разделения исходников схему надо выпустить заново, а не ослаблять замыкание
# незаметно.
PRODUCT_ARTIFACT_PATHS = {
    "appearance_executor_source": "crates/labcolors-core/src/appearance.rs",
    "classifier_source": "crates/labcolors-core/src/clean_set.rs",
    "classifier_tests": "crates/labcolors-core/src/clean_set_tests.rs",
    "composition_executor_source": "crates/labcolors-core/src/composition.rs",
    "content_digest_source": "crates/labcolors-core/src/sha256.rs",
    "joint_selection_source": "crates/labcolors-core/src/joint.rs",
    "module_registration_source": "crates/labcolors-core/src/lib.rs",
    "observation_runtime_source": "crates/labcolors-core/src/observation.rs",
    "point_attachment_allocator_oracle": "crates/labcolors-core/src/test_support.rs",
    "point_attachment_source": "crates/labcolors-core/src/program/attachment.rs",
    "point_attachment_test_support": "crates/labcolors-core/src/program/attachment/support.rs",
    "point_attachment_tests": "crates/labcolors-core/src/program/attachment/tests.rs",
    "program_facade_source": "crates/labcolors-core/src/program.rs",
    "program_identity_source": "crates/labcolors-core/src/program_identity.rs",
    "program_source": "crates/labcolors-core/src/program_session.rs",
    "program_tests": "crates/labcolors-core/src/program_clean_set_tests.rs",
    "runtime_codec": CODEC_PATH,
    "session_runtime_source": "crates/labcolors-core/src/session.rs",
    "signal_transport_source": "crates/labcolors-core/src/lcs_occurrence.rs",
    "srgb8_source": "crates/labcolors-core/src/srgb8.rs",
    "verifier_source": "scripts/verify_clean_set_receipt.py",
    "verifier_tests": "scripts/test_verify_clean_set_receipt.py",
}

PRODUCT_ARTIFACT_LICENSES = {
    role: DATA_LICENSE_EXPRESSION if role == "runtime_codec" else "MIT"
    for role in PRODUCT_ARTIFACT_PATHS
}

LEGAL_FILE_PATHS = {
    "attribution_notice": "crates/labcolors-core/NOTICE.md",
    "cc_by_4_0_text": "crates/labcolors-core/LICENSES/CC-BY-4.0.txt",
    "cc_by_sa_4_0_text": "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt",
    "core_manifest": "crates/labcolors-core/Cargo.toml",
    "mit_text": "LICENSE",
}

RESEARCH_ARTIFACT_LICENSES = {
    "semantic_profile": "CC-BY-SA-4.0",
    "policy_frontier": "CC-BY-SA-4.0",
    "cie_1931_2deg_source": "CC-BY-SA-4.0",
    "cie_d65_source": "CC-BY-SA-4.0",
    "canonical_raw_table": "CC-BY-SA-4.0",
    "runtime_codec_table": "CC-BY-SA-4.0",
    "boundary_certificates": "CC-BY-SA-4.0",
    "proof": "CC-BY-SA-4.0",
    "generator": "MIT",
    "generator_cie_reader": "MIT",
    "independent_verifier": "MIT",
    "verifier_cie_reader": "MIT",
    "data_notice": "CC-BY-SA-4.0",
}

NOTICE_TOKENS = (
    "10.7717/peerj.2751",
    "CC-BY-4.0",
    "10.25039/CIE.DS.xvudnb9b",
    "10.25039/CIE.DS.hjfjmt59",
    "CC-BY-SA-4.0",
)

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
PORTABLE_PATH_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]*$")


class VerificationError(RuntimeError):
    """Единый fail-closed исход для receipt, provenance, кодека и лицензий."""


@dataclass(frozen=True)
class VerifierPolicy:
    research_commit: str
    research_release_sha256: str


PRODUCTION_POLICY = VerifierPolicy(RESEARCH_COMMIT, RESEARCH_RELEASE_SHA256)


@dataclass(frozen=True)
class VerificationResult:
    receipt_sha256: str
    research_replayed: bool


def _fail(message: str) -> NoReturn:
    raise VerificationError(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    try:
        source = json.dumps(
            value,
            allow_nan=False,
            ensure_ascii=True,
            indent=2,
            sort_keys=True,
        )
    except (TypeError, ValueError) as error:
        _fail(f"value is not canonical JSON: {error}")
    return f"{source}\n".encode("ascii")


def _reject_duplicate_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            _fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _reject_float(source: str) -> NoReturn:
    _fail(f"floating JSON number is unsupported: {source}")


def _reject_constant(source: str) -> NoReturn:
    _fail(f"non-finite JSON number is unsupported: {source}")


def _parse_json(data: bytes, label: str, *, canonical: bool) -> Any:
    if data.startswith(b"\xef\xbb\xbf"):
        _fail(f"{label} has a UTF-8 BOM")
    try:
        source = data.decode("utf-8", errors="strict")
        value = json.loads(
            source,
            object_pairs_hook=_reject_duplicate_pairs,
            parse_float=_reject_float,
            parse_constant=_reject_constant,
        )
    except VerificationError:
        raise
    except (UnicodeError, json.JSONDecodeError) as error:
        _fail(f"{label} is not strict JSON: {error}")
    if canonical and data != canonical_json_bytes(value):
        _fail(f"{label} is not canonical sorted LF JSON")
    return value


def _exact_keys(value: Any, expected: tuple[str, ...], label: str) -> dict[str, Any]:
    if type(value) is not dict:
        _fail(f"{label} must be an object")
    actual = tuple(sorted(value))
    canonical = tuple(sorted(expected))
    if actual != canonical:
        _fail(f"{label} fields {actual!r} differ from {canonical!r}")
    return value


def _exact_string(value: Any, expected: str, label: str) -> None:
    if type(value) is not str or value != expected:
        _fail(f"{label} must equal {expected!r}")


def _exact_int(value: Any, expected: int, label: str) -> None:
    if type(value) is not int or value != expected:
        _fail(f"{label} must equal integer {expected}")


def _positive_int(value: Any, label: str) -> int:
    if type(value) is not int or value <= 0 or value > 2**53 - 1:
        _fail(f"{label} must be a positive interoperable integer")
    return value


def _sha256_string(value: Any, label: str) -> str:
    if type(value) is not str or SHA256_RE.fullmatch(value) is None:
        _fail(f"{label} must be one lower-case SHA-256")
    return value


def _portable_path(value: Any, label: str) -> str:
    if type(value) is not str or PORTABLE_PATH_RE.fullmatch(value) is None:
        _fail(f"{label} must be a portable relative path")
    if "\\" in value or "\x00" in value:
        _fail(f"{label} must be a portable relative path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        _fail(f"{label} must be a portable relative path")
    if path.as_posix() != value:
        _fail(f"{label} must be a canonical portable relative path")
    return value


def _read_regular_file_once(root: Path, relative_path: str, label: str) -> bytes:
    path_text = _portable_path(relative_path, label)
    try:
        canonical_root = root.resolve(strict=True)
    except OSError as error:
        _fail(f"{label} root is unavailable: {error}")
    if not canonical_root.is_dir():
        _fail(f"{label} root is not a directory")

    current = canonical_root
    for part in PurePosixPath(path_text).parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except OSError as error:
            _fail(f"{label} is unavailable: {error}")
        if stat.S_ISLNK(mode):
            _fail(f"{label} contains a symlink: {path_text}")

    try:
        mode = current.stat().st_mode
        if not stat.S_ISREG(mode):
            _fail(f"{label} is not a regular file: {path_text}")
        with current.open("rb") as source:
            data = source.read()
    except OSError as error:
        _fail(f"{label} cannot be read: {error}")
    if not data:
        _fail(f"{label} is empty: {path_text}")
    return data


def _verify_file_metadata(
    entry: Any,
    root: Path,
    label: str,
    *,
    expected_role: str,
    expected_path: str,
    expected_license: str | None,
) -> bytes:
    fields = ("bytes", "path", "role", "sha256")
    if expected_license is not None:
        fields = ("bytes", "license", "path", "role", "sha256")
    item = _exact_keys(entry, fields, label)
    _exact_string(item["role"], expected_role, f"{label}.role")
    actual_path = _portable_path(item["path"], f"{label}.path")
    _exact_string(actual_path, expected_path, f"{label}.path")
    if expected_license is not None:
        _exact_string(item["license"], expected_license, f"{label}.license")
    expected_bytes = _positive_int(item["bytes"], f"{label}.bytes")
    expected_sha = _sha256_string(item["sha256"], f"{label}.sha256")
    data = _read_regular_file_once(root, expected_path, label)
    if len(data) != expected_bytes or sha256(data) != expected_sha:
        _fail(f"{label} bytes do not match receipt metadata")
    return data


def _validate_notice(data: bytes, label: str) -> None:
    if b"\r" in data:
        _fail(f"{label} must use LF line endings")
    try:
        source = data.decode("utf-8", errors="strict")
    except UnicodeError as error:
        _fail(f"{label} is not UTF-8: {error}")
    for token in NOTICE_TOKENS:
        if token not in source:
            _fail(f"{label} lacks required attribution token {token}")


def _parse_receipt_pin(data: bytes, label: str) -> str:
    try:
        source = data.decode("ascii", errors="strict")
    except UnicodeError as error:
        _fail(f"{label} is not ASCII: {error}")
    expected_suffix = "  receipt-v1.json\n"
    if not source.endswith(expected_suffix):
        _fail(f"{label} must name receipt-v1.json with one terminal LF")
    digest = source[: -len(expected_suffix)]
    return _sha256_string(digest, f"{label} digest")


def _receipt_pin(product_root: Path) -> str:
    data = _read_regular_file_once(product_root, RECEIPT_PIN_PATH, "product receipt pin")
    return _parse_receipt_pin(data, "product receipt pin")


def _verify_admission(value: Any, label: str) -> None:
    admission = _exact_keys(
        value,
        ("authority", "convention", "production_auto_minted"),
        label,
    )
    _exact_string(admission["authority"], "ExactTechnicalDerivation", f"{label}.authority")
    _exact_string(
        admission["convention"],
        "DeclaredPackagePolicyCandidate",
        f"{label}.convention",
    )
    if type(admission["production_auto_minted"]) is not bool:
        _fail(f"{label}.production_auto_minted must be a boolean")
    if admission["production_auto_minted"]:
        _fail(f"{label}.production_auto_minted must remain false")


def _verify_excluded_claims(value: Any, label: str) -> None:
    if type(value) is not list or tuple(value) != EXCLUDED_CLAIMS:
        _fail(f"{label} must equal the exact ordered excluded-claim boundary")


def _decode_codec(codec: bytes, expected_records: int = CODEC_RECORDS) -> tuple[bytes, int]:
    expected_bytes = CODEC_BODY_OFFSET + expected_records * CODEC_RECORD_BYTES
    if len(codec) != expected_bytes:
        _fail(f"runtime codec has {len(codec)} bytes, expected {expected_bytes}")
    if codec[: len(CODEC_HEADER)] != CODEC_HEADER:
        _fail("runtime codec header differs from LPCC v1")

    offsets = [
        int.from_bytes(
            codec[
                len(CODEC_HEADER) + index * CODEC_INDEX_ENTRY_BYTES :
                len(CODEC_HEADER) + (index + 1) * CODEC_INDEX_ENTRY_BYTES
            ],
            "big",
        )
        for index in range(CODEC_INDEX_ENTRIES)
    ]
    if offsets[0] != 0 or offsets[-1] != expected_records:
        _fail("runtime codec offsets do not bind the complete record body")
    if any(left >= right for left, right in zip(offsets, offsets[1:])):
        _fail("runtime codec must contain one non-empty canonical run list per green column")

    body = codec[CODEC_BODY_OFFSET:]
    columns: list[list[tuple[int, int, int]]] = []
    for green in range(256):
        records = [
            tuple(body[index * 3 : index * 3 + 3])
            for index in range(offsets[green], offsets[green + 1])
        ]
        if records[0][0] != 0:
            _fail(f"runtime codec green={green} does not start at red=0")
        previous_start = -1
        previous_interval: tuple[int, int] | None = None
        for red_start, lo, hi in records:
            if red_start <= previous_start:
                _fail(f"runtime codec green={green} has non-increasing red starts")
            interval = (lo, hi)
            if lo > hi and interval != (255, 0):
                _fail(f"runtime codec green={green} has a non-canonical reversed interval")
            if previous_interval == interval:
                _fail(f"runtime codec green={green} splits one canonical run")
            previous_start = red_start
            previous_interval = interval
        columns.append(records)

    raw = bytearray()
    accepted = 0
    rejected = 0
    for red in range(256):
        for green in range(256):
            record = columns[green][0]
            for candidate in columns[green][1:]:
                if candidate[0] > red:
                    break
                record = candidate
            lo, hi = record[1], record[2]
            raw.extend((lo, hi))
            rejected_here = 0 if (lo, hi) == (255, 0) else hi - lo + 1
            accepted_here = 256 - rejected_here
            if red == green and rejected_here and lo <= red <= hi:
                accepted_here += 1
                rejected_here -= 1
            accepted += accepted_here
            rejected += rejected_here

    if accepted == 0 or rejected == 0:
        _fail("runtime codec replay is vacuous")
    return bytes(raw), accepted


def _git(root: Path, args: list[str], label: str) -> bytes:
    environment = {
        **os.environ,
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_NO_REPLACE_OBJECTS": "1",
        "GIT_OPTIONAL_LOCKS": "0",
        "LC_ALL": "C",
    }
    try:
        return subprocess.check_output(
            ["git", "--no-replace-objects", "-C", str(root), *args],
            stderr=subprocess.STDOUT,
            env=environment,
            timeout=GIT_LOOKUP_TIMEOUT_SECONDS,
        )
    except (OSError, subprocess.SubprocessError) as error:
        output: bytes = getattr(error, "output", None) or b""
        detail = output.decode("utf-8", errors="replace").strip()
        _fail(f"{label} Git lookup failed{': ' + detail if detail else ''}")


def _git_blob(root: Path, commit: str, path: str, label: str) -> bytes:
    relative = _portable_path(path, label)
    listing = _git(root, ["ls-tree", "-z", commit, "--", relative], label)
    records = [record for record in listing.split(b"\x00") if record]
    if len(records) != 1 or not records[0].startswith((b"100644 blob ", b"100755 blob ")):
        _fail(f"{label} is not one committed regular blob")
    return _git(root, ["cat-file", "blob", f"{commit}:{relative}"], label)


def _verify_research(
    research_root: Path,
    research: Any,
    runtime: dict[str, Any],
    product_codec: bytes,
    policy: VerifierPolicy,
) -> None:
    value = _exact_keys(
        research,
        ("commit", "object_format", "release_path", "release_sha256"),
        "receipt.research",
    )
    commit = value["commit"]
    if type(commit) is not str or GIT_SHA1_RE.fullmatch(commit) is None:
        _fail("receipt research commit must be one lower-case 40-hex Git commit")
    if commit != policy.research_commit:
        _fail("receipt research commit differs from the admitted immutable commit")
    _exact_string(value["object_format"], "sha1", "receipt.research.object_format")
    _exact_string(value["release_path"], RESEARCH_RELEASE_PATH, "receipt.research.release_path")
    _exact_string(
        value["release_sha256"],
        policy.research_release_sha256,
        "receipt.research.release_sha256",
    )

    object_type = _git(research_root, ["cat-file", "-t", commit], "research commit").strip()
    if object_type != b"commit":
        _fail("receipt research commit does not name a commit object")
    peeled = _git(
        research_root,
        ["rev-parse", "--verify", f"{commit}^{{commit}}"],
        "research commit",
    ).decode("ascii", errors="strict").strip()
    if peeled != commit:
        _fail("receipt research commit does not resolve to its exact object identity")

    release_bytes = _git_blob(research_root, commit, RESEARCH_RELEASE_PATH, "research release")
    if sha256(release_bytes) != policy.research_release_sha256:
        _fail("research release blob differs from the admitted release SHA-256")
    release = _exact_keys(
        _parse_json(release_bytes, "research release", canonical=False),
        ("artifacts", "bundle_root", "codec_id", "encoding_id", "license", "release_id", "schema"),
        "research release",
    )
    _exact_string(
        release["bundle_root"],
        "cleanliness-repository-v1",
        "research release.bundle_root",
    )
    _exact_string(release["codec_id"], runtime["codec"]["id"], "research release.codec_id")
    _exact_string(release["encoding_id"], runtime["raw"]["id"], "research release.encoding_id")
    _exact_string(release["license"], "CC-BY-SA-4.0", "research release.license")
    _exact_string(release["release_id"], RELEASE_ID, "research release.release_id")
    _exact_string(
        release["schema"],
        "lab-point-clean-set-srgb8-release/1",
        "research release.schema",
    )

    artifacts = release["artifacts"]
    if type(artifacts) is not list or len(artifacts) != len(RESEARCH_ARTIFACT_LICENSES):
        _fail("research release has an incomplete artifact closure")
    by_role: dict[str, tuple[dict[str, Any], bytes]] = {}
    paths: set[str] = set()
    casefold_paths: set[str] = set()
    for index, entry in enumerate(artifacts):
        item = _exact_keys(
            entry,
            ("bytes", "license", "path", "role", "sha256"),
            f"research release artifact[{index}]",
        )
        role = item["role"]
        if type(role) is not str or role not in RESEARCH_ARTIFACT_LICENSES or role in by_role:
            _fail("research release has an unknown or duplicate artifact role")
        path = _portable_path(item["path"], f"research release artifact[{index}].path")
        if path in paths or path.casefold() in casefold_paths:
            _fail("research release has a duplicate or case-colliding artifact path")
        paths.add(path)
        casefold_paths.add(path.casefold())
        _exact_string(
            item["license"],
            RESEARCH_ARTIFACT_LICENSES[role],
            f"research release artifact[{index}].license",
        )
        expected_bytes = _positive_int(
            item["bytes"],
            f"research release artifact[{index}].bytes",
        )
        expected_sha = _sha256_string(
            item["sha256"],
            f"research release artifact[{index}].sha256",
        )
        data = _git_blob(research_root, commit, path, f"research artifact {role}")
        if len(data) != expected_bytes or sha256(data) != expected_sha:
            _fail(f"research artifact {role} differs from release metadata")
        by_role[role] = (item, data)
    if set(by_role) != set(RESEARCH_ARTIFACT_LICENSES):
        _fail("research release artifact roles differ from the admitted closure")

    profile = _exact_keys(
        _parse_json(by_role["semantic_profile"][1], "research profile", canonical=False),
        (
            "admission",
            "excluded_claims",
            "geometry",
            "neutral_axis",
            "nominal_bridge",
            "output_release",
            "policy",
            "release_id",
            "schema",
        ),
        "research profile",
    )
    proof = _exact_keys(
        _parse_json(by_role["proof"][1], "research proof", canonical=False),
        (
            "admission",
            "artifacts",
            "certificate_encoding",
            "codec_encoding",
            "cone_certificates",
            "counts",
            "excluded_claims",
            "generator_sha256",
            "inputs",
            "law",
            "release_id",
            "schema",
            "witnesses",
        ),
        "research proof",
    )
    for label, document in (("research profile", profile), ("research proof", proof)):
        _verify_admission(document["admission"], f"{label}.admission")
        _verify_excluded_claims(document["excluded_claims"], f"{label}.excluded_claims")
        _exact_string(document["release_id"], RELEASE_ID, f"{label}.release_id")

    proof_artifacts = _exact_keys(
        proof["artifacts"],
        (
            "certificates_bytes",
            "certificates_sha256",
            "codec_bytes",
            "codec_sha256",
            "profile_sha256",
            "table_bytes",
            "table_sha256",
        ),
        "research proof.artifacts",
    )
    _exact_int(proof_artifacts["codec_bytes"], CODEC_BYTES, "research proof codec bytes")
    _exact_string(proof_artifacts["codec_sha256"], CODEC_SHA256, "research proof codec SHA-256")
    _exact_int(proof_artifacts["table_bytes"], RAW_BYTES, "research proof raw bytes")
    _exact_string(proof_artifacts["table_sha256"], RAW_SHA256, "research proof raw SHA-256")
    _exact_string(
        proof_artifacts["profile_sha256"],
        sha256(by_role["semantic_profile"][1]),
        "research proof profile SHA-256",
    )

    counts = proof["counts"]
    if type(counts) is not dict:
        _fail("research proof.counts must be an object")
    _exact_int(counts.get("cube_points"), DOMAIN_POINTS, "research proof cube points")
    _exact_int(counts.get("neutral_points"), 256, "research proof neutral points")
    accepted_chromatic = counts.get("accepted_chromatic")
    neutral_points = counts.get("neutral_points")
    if type(accepted_chromatic) is not int or type(neutral_points) is not int:
        _fail("research proof accepted points must be integers")
    _exact_int(
        accepted_chromatic + neutral_points,
        ACCEPTED_POINTS,
        "research proof accepted points",
    )

    codec_encoding = proof["codec_encoding"]
    if type(codec_encoding) is not dict:
        _fail("research proof.codec_encoding must be an object")
    _exact_int(codec_encoding.get("records"), CODEC_RECORDS, "research proof codec records")
    _exact_string(
        codec_encoding.get("header_hex"),
        CODEC_HEADER.hex(),
        "research proof codec header",
    )
    _exact_string(codec_encoding.get("id"), runtime["codec"]["id"], "research proof codec ID")

    research_codec = by_role["runtime_codec_table"][1]
    research_raw = by_role["canonical_raw_table"][1]
    if research_codec != product_codec:
        _fail("product codec bytes differ from the committed research codec")
    if sha256(research_raw) != RAW_SHA256 or len(research_raw) != RAW_BYTES:
        _fail("committed research raw table identity drifted")


def verify_receipt(
    product_root: Path | str,
    research_root: Path | str | None,
    expected_receipt_sha256: str,
    *,
    policy: VerifierPolicy = PRODUCTION_POLICY,
) -> VerificationResult:
    product = Path(product_root)
    research = Path(research_root) if research_root is not None else None
    expected_receipt = _sha256_string(expected_receipt_sha256, "expected receipt SHA-256")
    receipt_bytes = _read_regular_file_once(product, RECEIPT_PATH, "product receipt")
    if sha256(receipt_bytes) != expected_receipt:
        _fail("product receipt differs from the caller-pinned SHA-256")
    receipt = _exact_keys(
        _parse_json(receipt_bytes, "product receipt", canonical=True),
        (
            "admission",
            "artifacts",
            "excluded_claims",
            "license_scope",
            "release_id",
            "research",
            "runtime_contract",
            "schema",
        ),
        "receipt",
    )
    _exact_string(receipt["schema"], RECEIPT_SCHEMA, "receipt.schema")
    _exact_string(receipt["release_id"], RELEASE_ID, "receipt.release_id")
    _verify_admission(receipt["admission"], "receipt.admission")
    _verify_excluded_claims(receipt["excluded_claims"], "receipt.excluded_claims")

    artifacts = receipt["artifacts"]
    if type(artifacts) is not list:
        _fail("receipt.artifacts must be an array")
    expected_roles = tuple(sorted(PRODUCT_ARTIFACT_PATHS))
    actual_roles = tuple(item.get("role") if type(item) is dict else None for item in artifacts)
    if actual_roles != expected_roles:
        _fail("receipt artifact roles must equal the exact sorted product source cone")
    paths: set[str] = set()
    casefold_paths: set[str] = set()
    product_codec: bytes | None = None
    for index, (role, item) in enumerate(zip(expected_roles, artifacts)):
        data = _verify_file_metadata(
            item,
            product,
            f"receipt artifact[{index}]",
            expected_role=role,
            expected_path=PRODUCT_ARTIFACT_PATHS[role],
            expected_license=PRODUCT_ARTIFACT_LICENSES[role],
        )
        path = PRODUCT_ARTIFACT_PATHS[role]
        if path in paths or path.casefold() in casefold_paths:
            _fail("receipt has a duplicate or case-colliding product path")
        paths.add(path)
        casefold_paths.add(path.casefold())
        if role == "runtime_codec":
            product_codec = data
    if product_codec is None:
        _fail("receipt lacks the runtime codec")

    license_scope = _exact_keys(
        receipt["license_scope"],
        ("codec_spdx", "core_package_spdx", "legal_files", "receipt_spdx", "software_spdx"),
        "receipt.license_scope",
    )
    _exact_string(license_scope["codec_spdx"], DATA_LICENSE_EXPRESSION, "receipt codec SPDX")
    _exact_string(
        license_scope["core_package_spdx"],
        CORE_LICENSE_EXPRESSION,
        "receipt core package SPDX",
    )
    _exact_string(license_scope["receipt_spdx"], DATA_LICENSE_EXPRESSION, "receipt SPDX")
    _exact_string(license_scope["software_spdx"], "MIT", "receipt software SPDX")
    legal_files = license_scope["legal_files"]
    expected_legal_roles = tuple(sorted(LEGAL_FILE_PATHS))
    if type(legal_files) is not list or tuple(
        item.get("role") if type(item) is dict else None for item in legal_files
    ) != expected_legal_roles:
        _fail("receipt legal files must equal the exact sorted license closure")
    core_manifest: bytes | None = None
    notice: bytes | None = None
    for index, (role, item) in enumerate(zip(expected_legal_roles, legal_files)):
        data = _verify_file_metadata(
            item,
            product,
            f"receipt legal file[{index}]",
            expected_role=role,
            expected_path=LEGAL_FILE_PATHS[role],
            expected_license=None,
        )
        if role == "attribution_notice":
            notice = data
        elif role == "core_manifest":
            core_manifest = data
    if core_manifest is None:
        _fail("receipt lacks the core package manifest")
    _package_license(core_manifest)
    if notice is None:
        _fail("receipt lacks the attribution notice")
    _validate_notice(notice, "product attribution notice")

    runtime = _exact_keys(
        receipt["runtime_contract"],
        (
            "accepted_points",
            "codec",
            "domain_id",
            "domain_points",
            "law_id",
            "neutral_axis_id",
            "raw",
        ),
        "receipt.runtime_contract",
    )
    _exact_int(runtime["accepted_points"], ACCEPTED_POINTS, "receipt accepted points")
    _exact_string(runtime["domain_id"], "encoded-srgb8-u8-cube-v1", "receipt domain ID")
    _exact_int(runtime["domain_points"], DOMAIN_POINTS, "receipt domain points")
    _exact_string(
        runtime["law_id"],
        "neutral-or-blue-outside-closed-dirty-interval-v1",
        "receipt law ID",
    )
    _exact_string(
        runtime["neutral_axis_id"],
        "srgb8-output-neutral-axis-v1",
        "receipt neutral-axis ID",
    )
    codec_contract = _exact_keys(
        runtime["codec"],
        ("bytes", "header_hex", "id", "records", "sha256"),
        "receipt runtime codec",
    )
    _exact_int(codec_contract["bytes"], CODEC_BYTES, "receipt codec bytes")
    _exact_string(codec_contract["header_hex"], CODEC_HEADER.hex(), "receipt codec header")
    _exact_string(codec_contract["id"], "green-column-red-run-start-rle-v1", "receipt codec ID")
    _exact_int(codec_contract["records"], CODEC_RECORDS, "receipt codec records")
    _exact_string(codec_contract["sha256"], CODEC_SHA256, "receipt codec SHA-256")
    if len(product_codec) != CODEC_BYTES or sha256(product_codec) != CODEC_SHA256:
        _fail("product runtime codec identity differs from the admitted codec")

    raw_contract = _exact_keys(runtime["raw"], ("bytes", "id", "sha256"), "receipt raw table")
    _exact_int(raw_contract["bytes"], RAW_BYTES, "receipt raw bytes")
    _exact_string(raw_contract["id"], "raw-dirty-blue-interval-u8-pair-v1", "receipt raw ID")
    _exact_string(raw_contract["sha256"], RAW_SHA256, "receipt raw SHA-256")
    decoded_raw, accepted = _decode_codec(product_codec)
    if len(decoded_raw) != RAW_BYTES or sha256(decoded_raw) != RAW_SHA256:
        _fail("runtime codec does not decode to the admitted raw table identity")
    if accepted != ACCEPTED_POINTS:
        _fail("runtime codec accepted-point count differs from the admitted finite domain")

    research_descriptor = _exact_keys(
        receipt["research"],
        ("commit", "object_format", "release_path", "release_sha256"),
        "receipt.research",
    )
    commit = research_descriptor["commit"]
    if type(commit) is not str or GIT_SHA1_RE.fullmatch(commit) is None:
        _fail("receipt research commit must be one lower-case 40-hex Git commit")
    _exact_string(commit, policy.research_commit, "receipt research commit")
    _exact_string(research_descriptor["object_format"], "sha1", "receipt research object format")
    _exact_string(
        research_descriptor["release_path"],
        RESEARCH_RELEASE_PATH,
        "receipt research release path",
    )
    _exact_string(
        research_descriptor["release_sha256"],
        policy.research_release_sha256,
        "receipt research release SHA-256",
    )

    if research is not None:
        _verify_research(research, research_descriptor, runtime, product_codec, policy)
    return VerificationResult(expected_receipt, research is not None)


def verify_product_receipt(
    product_root: Path | str,
    *,
    policy: VerifierPolicy = PRODUCTION_POLICY,
) -> VerificationResult:
    product = Path(product_root)
    expected_receipt = _receipt_pin(product)
    return verify_receipt(product, None, expected_receipt, policy=policy)


def _package_license(cargo_toml: bytes) -> str:
    if b"\r" in cargo_toml:
        _fail("packaged Cargo.toml must use LF line endings")
    try:
        source = cargo_toml.decode("utf-8", errors="strict")
    except UnicodeError as error:
        _fail(f"packaged Cargo.toml is not UTF-8: {error}")
    current_table = ""
    licenses: list[str] = []
    for line in source.splitlines():
        # Любой заголовок таблицы завершает область `[package]`: иначе поле из
        # массива таблиц вроде `[[bin]]` было бы ошибочно засчитано пакету.
        array_table = re.fullmatch(r"\s*\[\[([^][]+)]]\s*(?:#.*)?", line)
        if array_table:
            current_table = ""
            continue
        table = re.fullmatch(r"\s*\[([^][]+)]\s*(?:#.*)?", line)
        if table:
            current_table = table.group(1).strip()
            continue
        if current_table != "package":
            continue
        if re.match(r"\s*license-file(?:\s|=)", line):
            _fail("packaged Cargo.toml must not use license-file beside SPDX metadata")
        match = re.fullmatch(r'\s*license\s*=\s*"([^"]+)"\s*', line)
        if match:
            licenses.append(match.group(1))
        if re.match(r"\s*license\.workspace\s*=", line):
            _fail("packaged Cargo.toml must resolve the inherited package license")
    if licenses != [CORE_LICENSE_EXPRESSION]:
        _fail(f"package license must equal {CORE_LICENSE_EXPRESSION!r}")
    return licenses[0]


def verify_core_package(source_root: Path | str, package_root: Path | str) -> None:
    source = Path(source_root)
    package = Path(package_root)
    copies = {
        "LICENSE": "LICENSE",
        "LICENSES/CC-BY-4.0.txt": "crates/labcolors-core/LICENSES/CC-BY-4.0.txt",
        "LICENSES/CC-BY-SA-4.0.txt": "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt",
        "NOTICE.md": "crates/labcolors-core/NOTICE.md",
        "contracts/clean-set-srgb8-v1/point-clean-set-srgb8-column-rle-v1.bin": CODEC_PATH,
        "contracts/clean-set-srgb8-v1/receipt-v1.json": RECEIPT_PATH,
        "contracts/clean-set-srgb8-v1/receipt-v1.sha256": RECEIPT_PIN_PATH,
    }
    packaged: dict[str, bytes] = {}
    for package_path, source_path in copies.items():
        canonical = _read_regular_file_once(source, source_path, f"canonical {source_path}")
        actual = _read_regular_file_once(package, package_path, f"packaged {package_path}")
        if actual != canonical:
            _fail(f"packaged {package_path} differs from canonical product bytes")
        packaged[package_path] = actual

    codec = packaged["contracts/clean-set-srgb8-v1/point-clean-set-srgb8-column-rle-v1.bin"]
    if len(codec) != CODEC_BYTES or sha256(codec) != CODEC_SHA256:
        _fail("packaged runtime codec identity differs from the admitted codec")
    receipt = packaged["contracts/clean-set-srgb8-v1/receipt-v1.json"]
    receipt_pin = packaged["contracts/clean-set-srgb8-v1/receipt-v1.sha256"]
    if sha256(receipt) != _parse_receipt_pin(receipt_pin, "packaged receipt pin"):
        _fail("packaged receipt differs from its external pin")
    _validate_notice(packaged["NOTICE.md"], "packaged NOTICE.md")
    cargo_toml = _read_regular_file_once(package, "Cargo.toml", "packaged Cargo.toml")
    _package_license(cargo_toml)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    product_parser = subparsers.add_parser(
        "product",
        help="verify the caller-pinned product cone without claiming research replay",
    )
    product_parser.add_argument("--product-root", required=True, type=Path)

    full_parser = subparsers.add_parser(
        "full",
        help="verify product plus committed research closure",
    )
    full_parser.add_argument("--product-root", required=True, type=Path)
    full_parser.add_argument("--research-root", required=True, type=Path)

    package_parser = subparsers.add_parser(
        "core-package",
        help="verify the extracted labcolors-core license and codec closure",
    )
    package_parser.add_argument("--source-root", required=True, type=Path)
    package_parser.add_argument("--package-root", required=True, type=Path)

    arguments = parser.parse_args(argv)
    try:
        if arguments.command == "product":
            result = verify_product_receipt(arguments.product_root)
            print(
                "clean-set product receipt: PRODUCT_IDENTITY_VERIFIED; "
                "RESEARCH_REPLAY_NOT_EXECUTED; "
                f"receipt_sha256={result.receipt_sha256}"
            )
        elif arguments.command == "full":
            expected_receipt = _receipt_pin(arguments.product_root)
            result = verify_receipt(
                arguments.product_root,
                arguments.research_root,
                expected_receipt,
            )
            print(
                "clean-set product receipt: PRODUCT_AND_RESEARCH_VERIFIED; "
                f"receipt_sha256={result.receipt_sha256}"
            )
        else:
            verify_core_package(arguments.source_root, arguments.package_root)
            print("labcolors-core license and codec closure: VERIFIED")
    except VerificationError as error:
        print(f"clean-set verification: FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
