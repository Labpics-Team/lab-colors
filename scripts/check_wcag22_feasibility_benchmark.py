#!/usr/bin/env python3
"""Validate WCAG22 feasibility raw benchmark identity and capacity algebra.

This checker intentionally does not admit or reject elapsed time. Native timing
and allocator observations stay raw evidence; WebAssembly memory and serialized
size are explicitly outside this artifact's claim boundary.

Generation admission and durable repository verification share one provenance
SSOT: the exact Git objects and SHA-256 manifest of the measured dependency
cone. A whole-commit identity is deliberately absent because a pre-merge
measurement commit is not durable across squash merge. ``Cargo.lock`` is an
ordinary exact member of that cone; dependency drift requires a new measured
artifact instead of a second compatibility law.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import tempfile
from decimal import Decimal
from pathlib import Path
from typing import Any


DEFAULT_ARTIFACT = Path(
    "/private/tmp/labcolors-wcag22-feasibility-admission-raw-v3.json"
)
HEX_256 = re.compile(r"[0-9a-f]{64}")
GIT_OBJECT = re.compile(r"[0-9a-f]{40}")

CANDIDATE_COUNT = 256
PAGE_BYTES = 65_536
DECISION_SLOT_BYTES = 32
PARTITION_BYTES = 32
MAX_APPLICABLE_EDGES = PAGE_BYTES // DECISION_SLOT_BYTES - 1
MAX_LOGICAL_ASSESSMENTS = CANDIDATE_COUNT * MAX_APPLICABLE_EDGES
ROOT = Path(__file__).resolve().parents[1]
SUBJECT_PATHS = (
    "Cargo.toml",
    "crates/labcolors-core/src/lib.rs",
    "crates/labcolors-core/src/wcag22_feasibility.rs",
    "crates/labcolors-core/src/wcag22_feasibility/explicit.rs",
    "crates/labcolors-core/src/srgb8.rs",
    "crates/labcolors-core/src/sha256.rs",
    "crates/labcolors-core/src/wcag22.rs",
    "crates/labcolors-core/src/wcag22/kernel.rs",
    "crates/labcolors-core/src/wcag22/q55_data.rs",
    "crates/labcolors-core/src/wcag22_evidence.rs",
    "crates/labcolors-core/src/numerics.rs",
    "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
    "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
    "crates/labcolors-core/Cargo.toml",
    "Cargo.lock",
    "crates/labcolors-core/benches/wcag22_feasibility_admission.rs",
    "scripts/check_wcag22_feasibility_benchmark.py",
)
SOURCE_OBJECTS = (
    ("workspaceCargo", "Cargo.toml"),
    ("workspaceLock", "Cargo.lock"),
    ("coreCargo", "crates/labcolors-core/Cargo.toml"),
    ("coreSourceTree", "crates/labcolors-core/src"),
    (
        "wcag22Srgb8Contract",
        "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
    ),
    (
        "wcag22Q55ProofContract",
        "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
    ),
    (
        "benchmarkHarness",
        "crates/labcolors-core/benches/wcag22_feasibility_admission.rs",
    ),
    ("benchmarkChecker", "scripts/check_wcag22_feasibility_benchmark.py"),
)
COMPILER_RECIPE_ID = "closed-cargo-bench-v1"
EXPLICIT_EMPTY_BUILD_INPUTS = (
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
)
ENVIRONMENT_FIELDS = frozenset(
    {
        "execution",
        "targetArch",
        "targetOs",
        "pointerWidthBits",
        "debugAssertions",
        "packageVersion",
        "allocator",
        "allocatorInstrumentationIncludedInElapsedTime",
        "timer",
        "measurementThreads",
        "requestConstructionMeasured",
        "rustcVerbose",
        "cargoVerbose",
        "activeCoreFeatures",
        "explicitEmptyBuildInputs",
        "rustcBinarySha256",
        "cargoBinarySha256",
        "sourceConeClean",
        "sampleCountExplicit",
        "sourceObjects",
    }
)

REQUIRED_SCENARIOS: dict[str, dict[str, Any]] = {
    "minimum-evaluated": {
        "shape": (1, 1, 2, 1, 1, 1),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-applicable-edges": {
        "shape": (1, 2_047, 19, 1, 1, 2_047),
        "terminal": "infeasible",
        "feasibleCandidates": 0,
    },
    "maximum-raw-duplicate-relations": {
        "shape": (2_047, 2_047, 26_611, 1, 1, 1),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-raw-adjacent-duplicates": {
        "shape": (1, 2_047, 2, 1, 1, 1),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-canonical-applicable-relations": {
        "shape": (2_047, 2_047, 20_470, 2_047, 2_047, 2_047),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-combined-applicable-envelope": {
        "shape": (2_047, 2_047, 65_536, 2_047, 2_047, 2_047),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-canonical-not-applicable-relations": {
        "shape": (2_047, 0, 36_846, 2_047, 0, 0),
        "terminal": "not-evaluated",
        "feasibleCandidates": None,
    },
    "maximum-combined-not-applicable-envelope": {
        "shape": (2_047, 0, 65_536, 2_047, 0, 0),
        "terminal": "not-evaluated",
        "feasibleCandidates": None,
    },
    "maximum-mixed-relations": {
        "shape": (2_047, 1_023, 28_662, 2_047, 1_023, 1_023),
        "terminal": "feasible",
        "feasibleCandidates": 7,
    },
    "maximum-opaque-utf8-bytes": {
        "shape": (1, 0, 65_536, 1, 0, 0),
        "terminal": "not-evaluated",
        "feasibleCandidates": None,
    },
}

SHAPE_KEYS = (
    "rawRelations",
    "rawAdjacentEntries",
    "opaqueUtf8Bytes",
    "canonicalRelations",
    "applicableRelations",
    "applicableEdges",
)


class AdmissionProtocol:
    def __init__(
        self,
        *,
        rustc_release: str,
        cargo_release: str,
        rustc_binary_sha256: str,
        cargo_binary_sha256: str,
        benchmark_binary_sha256: str,
        target_triple: str,
        target_arch: str,
        target_os: str,
        pointer_width_bits: int,
        package_version: str,
        sample_count: int,
    ) -> None:
        self.rustc_release = rustc_release
        self.cargo_release = cargo_release
        self.rustc_binary_sha256 = rustc_binary_sha256
        self.cargo_binary_sha256 = cargo_binary_sha256
        self.benchmark_binary_sha256 = benchmark_binary_sha256
        self.target_triple = target_triple
        self.target_arch = target_arch
        self.target_os = target_os
        self.pointer_width_bits = pointer_width_bits
        self.package_version = package_version
        self.sample_count = sample_count


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def toolchain_binary(toolchain: str, binary: str) -> Path:
    result = subprocess.run(
        ["rustup", "which", "--toolchain", toolchain, binary],
        check=True,
        capture_output=True,
        text=True,
    )
    path = Path(result.stdout.strip()).resolve(strict=True)
    require(path.is_file() and path.is_absolute(),
            f"rustup did not resolve an absolute {binary} binary")
    return path


def require_empty_cargo_config_hierarchy(cwd: Path, cargo_home: Path) -> None:
    candidates = [
        *(parent / ".cargo" / name
          for parent in (cwd, *cwd.parents)
          for name in ("config", "config.toml")),
        cargo_home / "config",
        cargo_home / "config.toml",
    ]
    for candidate in candidates:
        require(not candidate.exists(),
                f"closed record recipe rejects Cargo config: {candidate}")


def closed_record_environment(
    temporary_root: Path,
    output: Path,
    rustc: Path,
    cargo: Path,
    sample_count: int,
    path_value: str,
) -> dict[str, str]:
    home = temporary_root / "home"
    cargo_home = temporary_root / "cargo-home"
    target = temporary_root / "target"
    for directory in (home, cargo_home, target):
        directory.mkdir()
    require_empty_cargo_config_hierarchy(temporary_root, cargo_home)
    return {
        "PATH": path_value,
        "HOME": str(home),
        "LANG": "C",
        "LC_ALL": "C",
        "CARGO": str(cargo),
        "RUSTC": str(rustc),
        "CARGO_HOME": str(cargo_home),
        "CARGO_TARGET_DIR": str(target),
        "CARGO_CACHE_RUSTC_INFO": "0",
        "CARGO_ENCODED_RUSTFLAGS": "",
        "RUSTC_WRAPPER": "",
        "RUSTC_WORKSPACE_WRAPPER": "",
        "LABCOLORS_WCAG22_BENCH_SAMPLES": str(sample_count),
        "LABCOLORS_WCAG22_BENCH_OUTPUT": str(output),
    }


def record_command(cargo: Path) -> list[str]:
    return [
        str(cargo),
        "bench",
        "--locked",
        "--frozen",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
        "--no-default-features",
        "--features",
        "wcag22-feasibility,wcag22-explicit-feasibility",
        "-p",
        "labcolors-core",
        "--bench",
        "wcag22_feasibility_admission",
    ]


def fetch_command(cargo: Path) -> list[str]:
    return [
        str(cargo),
        "fetch",
        "--locked",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
    ]


def source_snapshot_sha256(
    snapshot: tuple[dict[str, str], dict[str, str]],
) -> str:
    canonical = json.dumps(
        snapshot,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def recorded_benchmark_binary(target: Path) -> Path:
    candidates = [
        path
        for path in (target / "release" / "deps").glob(
            "wcag22_feasibility_admission-*"
        )
        if path.is_file() and os.access(path, os.X_OK)
    ]
    require(len(candidates) == 1,
            "closed record recipe did not produce exactly one benchmark binary")
    return candidates[0]


def bind_record_provenance(
    output: Path,
    source_snapshot: tuple[dict[str, str], dict[str, str]],
    target: Path,
) -> None:
    payload = decode_benchmark_artifact(output.read_bytes())
    require("recordProvenance" not in payload,
            "raw harness output must not self-assert record provenance")
    binary = recorded_benchmark_binary(target)
    provenance = {
        "recipeId": COMPILER_RECIPE_ID,
        "sourceSnapshotSha256": source_snapshot_sha256(source_snapshot),
        "benchmarkBinarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    enriched: dict[str, Any] = {}
    for key, value in payload.items():
        enriched[key] = value
        if key == "artifactId":
            enriched["recordProvenance"] = provenance
    output.write_text(
        json.dumps(enriched, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def record_artifact(
    output: Path,
    toolchain: str,
    sample_count: int,
) -> None:
    require(toolchain != "", "--record-toolchain must be non-empty")
    require(sample_count >= 5,
            "--record-sample-count must request at least five observations")
    require(output.parent.is_dir(), "record output parent directory must exist")
    rustc = toolchain_binary(toolchain, "rustc")
    cargo = toolchain_binary(toolchain, "cargo")
    source_before = dependency_cone_snapshot()
    with tempfile.TemporaryDirectory(prefix="labcolors-wcag22-record-") as raw_root:
        temporary_root = Path(raw_root)
        environment = closed_record_environment(
            temporary_root,
            output.resolve(),
            rustc,
            cargo,
            sample_count,
            os.defpath,
        )
        subprocess.run(
            fetch_command(cargo),
            cwd=temporary_root,
            env=environment,
            check=True,
        )
        subprocess.run(
            record_command(cargo),
            cwd=temporary_root,
            env=environment,
            check=True,
        )
        require(dependency_cone_snapshot() == source_before,
                "dependency cone changed between pre-build snapshot and benchmark completion")
        bind_record_provenance(
            output,
            source_before,
            temporary_root / "target",
        )
    require(dependency_cone_snapshot() == source_before,
            "dependency cone changed while record provenance was bound")
    require(output.is_file(), "closed record recipe did not produce the artifact")


def run_record_recipe_self_tests() -> int:
    with tempfile.TemporaryDirectory(prefix="labcolors-wcag22-recipe-test-") as raw_root:
        temporary_root = Path(raw_root)
        output = temporary_root / "artifact.json"
        rustc = Path("/toolchain/rustc")
        cargo = Path("/toolchain/cargo")
        environment = closed_record_environment(
            temporary_root,
            output,
            rustc,
            cargo,
            5,
            "/controlled/bin",
        )
        require(
            all(environment[name] == "" for name in EXPLICIT_EMPTY_BUILD_INPUTS),
            "closed recipe must explicitly empty every compiler override",
        )
        hostile_names = (
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_PROFILE_BENCH_OPT_LEVEL",
            "RUSTFLAGS",
            "RUSTC_BOOTSTRAP",
        )
        require(
            all(name not in environment for name in hostile_names),
            "closed recipe leaked an ambient semantic build input",
        )
        command = record_command(cargo)
        require(
            command[1:] == [
                "bench",
                "--locked",
                "--frozen",
                "--manifest-path",
                str(ROOT / "Cargo.toml"),
                "--no-default-features",
                "--features",
                "wcag22-feasibility,wcag22-explicit-feasibility",
                "-p",
                "labcolors-core",
                "--bench",
                "wcag22_feasibility_admission",
            ],
            "closed recipe command drifted",
        )
        require(
            fetch_command(cargo)[1:] == [
                "fetch",
                "--locked",
                "--manifest-path",
                str(ROOT / "Cargo.toml"),
            ],
            "closed fetch command drifted",
        )
        cargo_directory = temporary_root / ".cargo"
        cargo_directory.mkdir()
        (cargo_directory / "config.toml").write_text("[build]\n", encoding="utf-8")
        try:
            require_empty_cargo_config_hierarchy(
                temporary_root,
                Path(environment["CARGO_HOME"]),
            )
        except ValueError:
            pass
        else:
            raise ValueError("record recipe mutation survived: Cargo config hierarchy")
    return 5


def reject_duplicate_json_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def reject_non_json_constant(value: str) -> None:
    raise ValueError(f"non-JSON numeric constant: {value}")


def decode_benchmark_artifact(payload_bytes: bytes) -> dict[str, Any]:
    try:
        payload = json.loads(
            payload_bytes.decode("utf-8"),
            object_pairs_hook=reject_duplicate_json_keys,
            parse_constant=reject_non_json_constant,
            # Preserve valid JSON numbers such as 1e400 without Python silently
            # turning them into non-finite binary floats. Schema checks below
            # admit only the exact integer fields the artifact declares.
            parse_float=Decimal,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        raise ValueError(f"benchmark artifact is not strict JSON: {error}") from error
    require(isinstance(payload, dict), "benchmark artifact root must be an object")
    return payload


def run_json_parser_self_tests() -> int:
    require(
        decode_benchmark_artifact(b'{"outer":{"integer":1}}')
        == {"outer": {"integer": 1}},
        "strict JSON object must remain admissible",
    )
    large_exponent = decode_benchmark_artifact(b'{"value":1e400}')["value"]
    require(
        isinstance(large_exponent, Decimal) and large_exponent.is_finite(),
        "valid large-exponent JSON must not become a non-finite binary float",
    )
    hostile = (
        b'{"outer":{"key":1,"key":2}}',
        b'{"value":NaN}',
        b'{"value":Infinity}',
        b'{"value":-Infinity}',
        b'\xff\xfe{\x00}\x00',
        b"[]",
        b"null",
        b'"scalar"',
        b"0",
        b"true",
    )
    rejected = 0
    for candidate in hostile:
        try:
            decode_benchmark_artifact(candidate)
        except ValueError:
            rejected += 1
        else:
            raise ValueError("checker mutation survived: ambiguous or invalid artifact JSON")
    return rejected


def exact_nonnegative_int(value: Any, field: str) -> int:
    require(type(value) is int and value >= 0, f"{field} must be a non-negative integer")
    return value


def require_digest(value: Any, field: str) -> None:
    require(isinstance(value, str) and HEX_256.fullmatch(value) is not None,
            f"{field} must be 64 lowercase hex characters")


def checked_packed_bytes(applicable_relations: int, applicable_edges: int) -> int:
    if applicable_relations == 0:
        return 0
    return DECISION_SLOT_BYTES * (applicable_edges + 1)


def check_bounded_envelope_model(payload: dict[str, Any]) -> None:
    model = payload.get("boundedEnvelopeModel")
    require(isinstance(model, dict), "boundedEnvelopeModel must be an object")
    expected = {
        "scope": "product-policy-capacity-arithmetic-not-total-memory",
        "referenceBoundedBytes": PAGE_BYTES,
        "candidateCount": CANDIDATE_COUNT,
        "decisionSlotBytes": DECISION_SLOT_BYTES,
        "partitionBytes": PARTITION_BYTES,
        "reservedPartitionSlots": 1,
        "maximumCardinality": MAX_APPLICABLE_EDGES,
        "maximumLogicalAssessments": MAX_LOGICAL_ASSESSMENTS,
        "maximumPackedResultBytes": PAGE_BYTES,
    }
    require(model == expected, "boundedEnvelopeModel drifted from exact 65536/32 algebra")
    require(
        DECISION_SLOT_BYTES * (MAX_APPLICABLE_EDGES + 1) == PAGE_BYTES,
        "one partition slot plus 2047 edge slots must occupy exactly 65536 bytes",
    )


def check_profile_limits(payload: dict[str, Any]) -> None:
    expected = {
        "profileId": "compile-v1",
        "rawRelations": 2_047,
        "rawAdjacentEntries": 2_047,
        "opaqueUtf8Bytes": PAGE_BYTES,
        "canonicalRelations": 2_047,
        "applicableEdges": MAX_APPLICABLE_EDGES,
        "logicalAssessments": MAX_LOGICAL_ASSESSMENTS,
        "packedResultBytes": PAGE_BYTES,
    }
    require(payload.get("profileLimits") == expected,
            "public Compile profile limits drifted from the measured admission envelope")


def check_hard_slo(payload: dict[str, Any]) -> None:
    expected = {
        "class": "deterministic-work-and-storage",
        "logicalAssessmentLaw": "W=256E",
        "packedStorageLaw": "B=0 if A=0, otherwise B=32(E+1)",
        "partialTerminalAllowed": False,
        "timingThresholdNs": None,
        "allRequiredShapesMustComplete": True,
    }
    require(payload.get("hardSlo") == expected,
            "hard SLO must remain deterministic work/storage, never a guessed timing limit")


def check_subject_manifest(
    payload: dict[str, Any], current_digests: dict[str, str]
) -> None:
    check_subject_git_bindings(SUBJECT_PATHS, SOURCE_OBJECTS)
    manifest = payload.get("subjectManifest")
    require(isinstance(manifest, list) and len(manifest) == len(SUBJECT_PATHS),
            "subjectManifest must bind every exact dependency-cone path")
    for index, path in enumerate(SUBJECT_PATHS):
        entry = manifest[index]
        require(isinstance(entry, dict) and set(entry) == {"path", "sha256"},
                f"subjectManifest[{index}] has unexpected fields")
        require(entry.get("path") == path,
                f"subjectManifest[{index}] path/order drifted")
        require_digest(entry.get("sha256"), f"subjectManifest[{index}].sha256")
        require(entry["sha256"] == current_digests[path],
                f"subjectManifest source drift: {path}")


def check_subject_git_bindings(
    subject_paths: tuple[str, ...],
    source_objects: tuple[tuple[str, str], ...],
) -> None:
    bound_paths = tuple(path for _, path in source_objects)
    for subject in subject_paths:
        require(
            any(subject == bound or subject.startswith(f"{bound}/") for bound in bound_paths),
            f"subject path lacks historical Git-object binding: {subject}",
        )


def check_samples(name: str, samples: Any, sample_count: int) -> None:
    require(isinstance(samples, list) and len(samples) == sample_count,
            f"{name}: samples must contain exactly sampleCount raw observations")
    required_metrics = (
        "elapsedNs",
        "allocationCalls",
        "allocatedBytes",
        "deallocationCalls",
        "deallocatedBytes",
        "baselineLiveBytes",
        "endLiveBytes",
        "peakLiveBytes",
        "peakAdditionalLiveBytes",
    )
    for index, sample in enumerate(samples):
        require(isinstance(sample, dict), f"{name}: sample {index} must be an object")
        require(sample.get("index") == index, f"{name}: sample indices must be contiguous")
        for metric in required_metrics:
            exact_nonnegative_int(sample.get(metric), f"{name}.samples[{index}].{metric}")
        baseline = sample["baselineLiveBytes"]
        allocated = sample["allocatedBytes"]
        deallocated = sample["deallocatedBytes"]
        end = sample["endLiveBytes"]
        peak = sample["peakLiveBytes"]
        require(peak >= baseline, f"{name}: peak live bytes precede the sample baseline")
        require(peak >= end, f"{name}: peak live bytes are below the sample end")
        require(
            baseline + allocated - deallocated == end,
            f"{name}: allocator live-byte conservation failed",
        )
        require(
            sample["peakAdditionalLiveBytes"] == peak - baseline,
            f"{name}: peakAdditionalLiveBytes is not the exact baseline delta",
        )


def check_scenario(value: Any, sample_count: int) -> tuple[str, str]:
    require(isinstance(value, dict), "every scenario must be an object")
    name = value.get("name")
    require(name in REQUIRED_SCENARIOS, f"unknown or duplicate scenario identity: {name!r}")
    required = REQUIRED_SCENARIOS[name]

    shape = value.get("shape")
    require(isinstance(shape, dict), f"{name}: shape must be an object")
    observed_shape = tuple(
        exact_nonnegative_int(shape.get(key), f"{name}.shape.{key}") for key in SHAPE_KEYS
    )
    require(observed_shape == required["shape"], f"{name}: exact scenario shape drifted")
    raw_relations, raw_adjacent, opaque_bytes, canonical, applicable, edges = observed_shape
    require(canonical <= raw_relations, f"{name}: canonical relations exceed raw relations")
    require(edges <= raw_adjacent, f"{name}: canonical edges exceed raw adjacency")
    require(applicable <= canonical, f"{name}: applicable relations exceed canonical relations")
    require((applicable == 0) == (edges == 0),
            f"{name}: applicable relation/edge zero law failed")
    require(applicable == 0 or edges >= applicable,
            f"{name}: every applicable relation must retain at least one edge")
    require(raw_relations <= 2_047 and raw_adjacent <= 2_047,
            f"{name}: raw dimensions exceed the admitted profile")
    require(opaque_bytes <= PAGE_BYTES and canonical <= 2_047 and edges <= 2_047,
            f"{name}: canonical dimensions exceed the admitted profile")

    logical_assessments = CANDIDATE_COUNT * edges
    packed_result_bytes = checked_packed_bytes(applicable, edges)
    expected = value.get("expected")
    require(isinstance(expected, dict), f"{name}: expected must be an object")
    require(expected.get("terminal") == required["terminal"], f"{name}: terminal drifted")
    require(expected.get("logicalAssessments") == logical_assessments,
            f"{name}: W must equal 256E")
    require(expected.get("packedResultBytes") == packed_result_bytes,
            f"{name}: B must equal 0 or 32(E + 1)")
    require(expected.get("feasibleCandidates") == required["feasibleCandidates"],
            f"{name}: feasible partition cardinality drifted")

    identity = value.get("observedIdentity")
    require(isinstance(identity, dict), f"{name}: observedIdentity must be an object")
    require(identity.get("terminal") == expected["terminal"],
            f"{name}: observed terminal differs from the bound scenario")
    require(identity.get("logicalAssessments") == logical_assessments,
            f"{name}: proof count differs from W")
    require(identity.get("assessmentIteratorLen") == logical_assessments,
            f"{name}: public full-matrix iterator differs from W")
    require(identity.get("derivedPackedResultBytes") == packed_result_bytes,
            f"{name}: public-shape packed derivation differs from B")
    require(identity.get("feasibleCandidates") == expected["feasibleCandidates"],
            f"{name}: observed feasible count differs from the bound partition")
    require_digest(identity.get("domainDigestSha256"), f"{name}.domainDigestSha256")
    require_digest(identity.get("relationSetDigestSha256"), f"{name}.relationSetDigestSha256")
    evaluation_id = identity.get("evaluationIdSha256")
    if applicable == 0:
        require(evaluation_id is None, f"{name}: NotEvaluated must not forge an evaluation ID")
    else:
        require_digest(evaluation_id, f"{name}.evaluationIdSha256")

    check_samples(name, value.get("samples"), sample_count)
    return name, identity["domainDigestSha256"]


def git_rev_parse(specification: str) -> str:
    result = subprocess.run(
        ["git", "rev-parse", specification],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    require(result.returncode == 0,
            f"recorded Git provenance is not addressable: {specification}")
    value = result.stdout.strip()
    require(GIT_OBJECT.fullmatch(value) is not None,
            f"git rev-parse returned a non-object ID: {specification}")
    return value


def dependency_cone_snapshot() -> tuple[dict[str, str], dict[str, str]]:
    result = subprocess.run(
        [
            "git",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
            "--",
            *(path for _, path in SOURCE_OBJECTS),
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    require(result.returncode == 0,
            "cannot inspect the current dependency-cone worktree")
    require_cone_clean(result.stdout)
    objects = {
        name: git_rev_parse(f"HEAD:{path}") for name, path in SOURCE_OBJECTS
    }
    digests = {
        path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
        for path in SUBJECT_PATHS
    }
    return objects, digests


def require_cone_clean(status: str) -> None:
    require(status == "", "current dependency-cone worktree is dirty")


def check_source_objects(
    environment: dict[str, Any],
    current_objects: dict[str, str],
) -> None:
    values = environment.get("sourceObjects")
    require(isinstance(values, dict), "environment.sourceObjects must be an object")
    require(set(values) == {name for name, _ in SOURCE_OBJECTS},
            "environment.sourceObjects names drifted from the exact provenance cone")
    for name, path in SOURCE_OBJECTS:
        entry = values[name]
        require(isinstance(entry, dict) and set(entry) == {"path", "gitObject"},
                f"environment.sourceObjects.{name} has unexpected fields")
        require(entry.get("path") == path,
                f"environment.sourceObjects.{name}.path drifted")
        object_id = entry.get("gitObject")
        require(
            isinstance(object_id, str) and GIT_OBJECT.fullmatch(object_id) is not None,
            f"environment.sourceObjects.{name}.gitObject is invalid",
        )
        require(object_id == current_objects[name],
                f"current Git subject differs from admitted measurement: {path}")


def check_record_provenance(
    payload: dict[str, Any],
    current_snapshot: tuple[dict[str, str], dict[str, str]],
    protocol: AdmissionProtocol | None,
) -> None:
    provenance = payload.get("recordProvenance")
    require(
        isinstance(provenance, dict)
        and set(provenance)
        == {"recipeId", "sourceSnapshotSha256", "benchmarkBinarySha256"},
        "recordProvenance must be the exact source-bound recorder receipt",
    )
    require(provenance.get("recipeId") == COMPILER_RECIPE_ID,
            "recordProvenance.recipeId drifted from the closed native recipe")
    source_digest = provenance.get("sourceSnapshotSha256")
    benchmark_digest = provenance.get("benchmarkBinarySha256")
    require_digest(source_digest, "recordProvenance.sourceSnapshotSha256")
    require_digest(benchmark_digest, "recordProvenance.benchmarkBinarySha256")
    require(source_digest == source_snapshot_sha256(current_snapshot),
            "recorded source snapshot differs from the current dependency cone")
    if protocol is not None:
        require(benchmark_digest == protocol.benchmark_binary_sha256,
                "recorded benchmark binary differs from the pinned executable")


def check_environment(
    payload: dict[str, Any],
    protocol: AdmissionProtocol | None,
    current_objects: dict[str, str],
) -> None:
    environment = payload.get("environment")
    require(isinstance(environment, dict) and environment.get("execution") == "native-process",
            "environment must identify native-process execution")
    require(set(environment) == ENVIRONMENT_FIELDS,
            "environment fields drifted from the exact V3 schema")
    require(environment.get("allocator") == "std::alloc::System",
            "allocator provenance must identify the measured global allocator")
    require(environment.get("allocatorInstrumentationIncludedInElapsedTime") is True,
            "allocator instrumentation overhead must remain explicit")
    require(environment.get("timer") == "std::time::Instant",
            "timer provenance must remain explicit")
    require(environment.get("measurementThreads") == 1,
            "this raw protocol is intentionally single-threaded")
    require(environment.get("requestConstructionMeasured") is False,
            "request construction must remain outside the compiler-operation sample")
    require(environment.get("sourceConeClean") is True,
            "environment.sourceConeClean must attest a clean measured cone")
    require(type(environment.get("sampleCountExplicit")) is bool,
            "environment.sampleCountExplicit must preserve protocol truth")
    rustc_verbose = environment.get("rustcVerbose")
    require(isinstance(rustc_verbose, str),
            "environment.rustcVerbose must be present even when unavailable")
    cargo_verbose = environment.get("cargoVerbose")
    require(isinstance(cargo_verbose, str),
            "environment.cargoVerbose must be present even when unavailable")
    require(
        environment.get("activeCoreFeatures")
        == ["wcag22-feasibility", "wcag22-explicit-feasibility"],
        "environment.activeCoreFeatures drifted from the measured feature set",
    )
    require(
        environment.get("explicitEmptyBuildInputs")
        == list(EXPLICIT_EMPTY_BUILD_INPUTS),
        "native admission requires the exact explicit-empty compiler overrides",
    )
    rustc_binary_sha256 = environment.get("rustcBinarySha256")
    cargo_binary_sha256 = environment.get("cargoBinarySha256")
    require_digest(rustc_binary_sha256, "environment.rustcBinarySha256")
    require_digest(cargo_binary_sha256, "environment.cargoBinarySha256")
    pointer_width_bits = exact_nonnegative_int(
        environment.get("pointerWidthBits"),
        "environment.pointerWidthBits",
    )
    require(pointer_width_bits > 0,
            "environment.pointerWidthBits must be positive")
    package_version = environment.get("packageVersion")
    require(isinstance(package_version, str) and package_version != "",
            "environment.packageVersion must be a non-empty string")

    check_source_objects(environment, current_objects)

    if protocol is None:
        return
    require(protocol.sample_count >= 5,
            "admission protocol requires at least five raw observations")
    require(environment["sampleCountExplicit"] is True,
            "admission requires an explicitly pinned raw sample count")
    require(environment.get("debugAssertions") is False,
            "admission requires an optimized release-profile binary")
    require(environment.get("targetArch") == protocol.target_arch,
            "measured target architecture differs from the pinned reference target")
    require(environment.get("targetOs") == protocol.target_os,
            "measured target OS differs from the pinned reference target")
    require(rustc_binary_sha256 == protocol.rustc_binary_sha256,
            "measured rustc binary differs from the pinned toolchain")
    require(cargo_binary_sha256 == protocol.cargo_binary_sha256,
            "measured cargo binary differs from the pinned toolchain")
    require(pointer_width_bits == protocol.pointer_width_bits,
            "measured pointer width differs from the pinned reference target")
    require(package_version == protocol.package_version,
            "measured package version differs from the pinned admission version")
    require(
        rustc_verbose.startswith(f"rustc {protocol.rustc_release} "),
        "measured rustc differs from the pinned admission toolchain",
    )
    require(
        cargo_verbose.startswith(f"cargo {protocol.cargo_release} "),
        "measured cargo differs from the pinned admission toolchain",
    )
    require(
        f"host: {protocol.target_triple}" in rustc_verbose.splitlines(),
        "measured native target triple differs from the pinned reference target",
    )
    require(payload.get("sampleCount") == protocol.sample_count,
            "measured sample count differs from the pinned admission protocol")


def check(
    payload: Any,
    protocol: AdmissionProtocol | None = None,
) -> None:
    source_before = dependency_cone_snapshot()
    require(isinstance(payload, dict), "artifact root must be an object")
    require(payload.get("schemaVersion") == 1, "unsupported benchmark schemaVersion")
    require(payload.get("artifactId") == "wcag22-feasibility-admission-raw-v3",
            "unexpected benchmark artifactId")
    require(
        payload.get("claimBoundary")
        == "native-process-observations-and-page-slot-arithmetic-only",
        "claim boundary must remain explicit",
    )
    require(
        payload.get("notMeasured")
        == ["webassembly-runtime-memory", "serialized-output-size", "client-latency"],
        "unmeasured target/adapter claims must remain explicit",
    )
    require(
        payload.get("admissionStatus")
        == "measurement-only-unless-admission-check-passes",
        "raw artifacts must not claim admission without an exact protocol check",
    )
    check_record_provenance(payload, source_before, protocol)
    check_environment(payload, protocol, source_before[0])

    sample_count = exact_nonnegative_int(payload.get("sampleCount"), "sampleCount")
    require(sample_count > 0, "sampleCount must be positive")
    require(payload.get("warmupSamples") == 0,
            "raw protocol must retain cold observations instead of hiding warmup calls")
    require(payload.get("scenarioOrder") == "as-emitted",
            "scenario order must remain explicit and replayable")
    check_hard_slo(payload)
    check_bounded_envelope_model(payload)
    check_profile_limits(payload)
    check_subject_manifest(payload, source_before[1])

    scenarios = payload.get("scenarios")
    require(isinstance(scenarios, list), "scenarios must be an array")
    require(
        [scenario.get("name") if isinstance(scenario, dict) else None for scenario in scenarios]
        == list(REQUIRED_SCENARIOS),
        "scenario order drifted from the exact cold-observation protocol",
    )
    seen: set[str] = set()
    domain_digest: str | None = None
    for scenario in scenarios:
        name, scenario_domain = check_scenario(scenario, sample_count)
        require(name not in seen, f"duplicate scenario: {name}")
        seen.add(name)
        if domain_digest is None:
            domain_digest = scenario_domain
        else:
            require(scenario_domain == domain_digest,
                    "all scenarios must bind the same registered domain digest")
    require(seen == set(REQUIRED_SCENARIOS), "required scenario set is incomplete")
    require(dependency_cone_snapshot() == source_before,
            "current dependency cone changed during verification")


def run_mutation_self_tests(
    payload: dict[str, Any],
    protocol: AdmissionProtocol | None,
) -> int:
    mutation_checks = 0

    def rejected(mutator: Any, label: str) -> None:
        nonlocal mutation_checks
        candidate = copy.deepcopy(payload)
        mutator(candidate)
        try:
            check(candidate, protocol)
        except ValueError:
            mutation_checks += 1
            return
        raise ValueError(f"checker mutation survived: {label}")

    rejected(
        lambda value: value["scenarios"][0]["expected"].__setitem__(
            "logicalAssessments",
            value["scenarios"][0]["expected"]["logicalAssessments"] - 1,
        ),
        "W formula",
    )
    rejected(
        lambda value: value["scenarios"].reverse(),
        "cold scenario order",
    )
    rejected(
        lambda value: value["scenarios"][0]["samples"][0].__setitem__(
            "endLiveBytes",
            value["scenarios"][0]["samples"][0]["endLiveBytes"] + 1,
        ),
        "allocator live-byte conservation",
    )
    for index, path in enumerate(SUBJECT_PATHS):
        rejected(
            lambda value, entry_index=index: value["subjectManifest"][entry_index].__setitem__(
                "sha256", "0" * 64
            ),
            f"subject SHA-256: {path}",
        )
    rejected(
        lambda value: value["subjectManifest"][0].__setitem__("extra", True),
        "subject manifest entry shape",
    )
    rejected(
        lambda value: value["subjectManifest"].reverse(),
        "subject manifest order",
    )
    rejected(
        lambda value: value["environment"]["sourceObjects"].pop("coreSourceTree"),
        "source object set",
    )
    rejected(
        lambda value: value["environment"]["sourceObjects"]["coreCargo"].__setitem__(
            "path", "crates/labcolors-core/README.md"
        ),
        "source object path",
    )
    rejected(
        lambda value: value["environment"]["sourceObjects"]["workspaceLock"].__setitem__(
            "gitObject", "not-an-object"
        ),
        "source object ID shape",
    )
    rejected(
        lambda value: value["environment"]["sourceObjects"]["coreCargo"].__setitem__(
            "extra", True
        ),
        "source object entry shape",
    )
    if protocol is not None:
        rejected(
            lambda value: value["environment"].__setitem__(
                "pointerWidthBits", protocol.pointer_width_bits + 1
            ),
            "admitted pointer width",
        )
        rejected(
            lambda value: value["environment"].__setitem__(
                "packageVersion", f"{protocol.package_version}-mutated"
            ),
            "admitted package version",
        )
    for name, path in SOURCE_OBJECTS:
        rejected(
            lambda value, source_name=name: value["environment"]["sourceObjects"][
                source_name
            ].__setitem__("gitObject", "0" * 40),
            f"source object replay: {path}",
        )
    for field in ("gitRevision", "gitTree", "unknownEnvironmentField"):
        rejected(
            lambda value, field_name=field: value["environment"].__setitem__(
                field_name, "0" * 40
            ),
            f"unexpected environment field: {field}",
        )
    rejected(
        lambda value: value["environment"].__setitem__("sourceConeClean", False),
        "dirty measured source cone",
    )
    rejected(
        lambda value: value["environment"].__setitem__("activeCoreFeatures", []),
        "active Core feature set",
    )
    rejected(
        lambda value: value["environment"].__setitem__(
            "explicitEmptyBuildInputs",
            list(reversed(EXPLICIT_EMPTY_BUILD_INPUTS)),
        ),
        "explicit-empty compiler input order",
    )
    rejected(
        lambda value: value["environment"]["explicitEmptyBuildInputs"].pop(),
        "missing explicit-empty compiler input",
    )
    rejected(
        lambda value: value["environment"].__setitem__(
            "rustcBinarySha256", "not-a-digest"
        ),
        "rustc binary identity shape",
    )
    rejected(
        lambda value: value["environment"].__setitem__(
            "cargoBinarySha256", "not-a-digest"
        ),
        "cargo binary identity shape",
    )
    if protocol is not None:
        rejected(
            lambda value: value["environment"].__setitem__(
                "rustcBinarySha256", "0" * 64
            ),
            "admitted rustc binary identity",
        )
        rejected(
            lambda value: value["environment"].__setitem__(
                "cargoBinarySha256", "0" * 64
            ),
            "admitted cargo binary identity",
        )
    rejected(
        lambda value: value.pop("recordProvenance"),
        "source-bound record provenance",
    )
    rejected(
        lambda value: value["recordProvenance"].__setitem__(
            "recipeId", "ambient-cargo"
        ),
        "closed compiler recipe",
    )
    rejected(
        lambda value: value["recordProvenance"].__setitem__(
            "sourceSnapshotSha256", "0" * 64
        ),
        "recorded source snapshot",
    )
    rejected(
        lambda value: value["recordProvenance"].__setitem__(
            "benchmarkBinarySha256", "not-a-digest"
        ),
        "recorded benchmark binary shape",
    )
    if protocol is not None:
        rejected(
            lambda value: value["recordProvenance"].__setitem__(
                "benchmarkBinarySha256", "0" * 64
            ),
            "admitted benchmark binary identity",
        )
    rejected(
        lambda value: (
            value["environment"]["sourceObjects"]["workspaceCargo"].__setitem__(
                "gitObject", "0" * 40
            ),
            value["subjectManifest"][0].__setitem__("sha256", "0" * 64),
        ),
        "coordinated source-object and manifest drift",
    )

    timing = copy.deepcopy(payload)
    timing["scenarios"][0]["samples"][0]["elapsedNs"] = 10**30
    check(timing, protocol)
    try:
        check_subject_git_bindings(
            (*SUBJECT_PATHS, "unbound-subject"),
            SOURCE_OBJECTS,
        )
    except ValueError:
        mutation_checks += 1
    else:
        raise ValueError("checker mutation survived: subject Git binding")
    try:
        require_cone_clean("?? crates/labcolors-core/src/unbound.rs\n")
    except ValueError:
        mutation_checks += 1
    else:
        raise ValueError("checker mutation survived: dirty current subject")
    return mutation_checks


def check_artifact_digest(payload_bytes: bytes, expected: str) -> None:
    require_digest(expected, "--artifact-sha256")
    require(hashlib.sha256(payload_bytes).hexdigest() == expected,
            "artifact bytes differ from the admitted SHA-256")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "artifact",
        nargs="?",
        type=Path,
        default=DEFAULT_ARTIFACT,
        help="raw JSON emitted by wcag22_feasibility_admission",
    )
    parser.add_argument("--admit-rustc-release")
    parser.add_argument("--admit-cargo-release")
    parser.add_argument("--admit-rustc-binary-sha256")
    parser.add_argument("--admit-cargo-binary-sha256")
    parser.add_argument("--admit-benchmark-binary-sha256")
    parser.add_argument("--admit-target-triple")
    parser.add_argument("--admit-target-arch")
    parser.add_argument("--admit-target-os")
    parser.add_argument("--admit-pointer-width-bits", type=int)
    parser.add_argument("--admit-package-version")
    parser.add_argument("--admit-sample-count", type=int)
    parser.add_argument("--artifact-sha256")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--record-toolchain")
    parser.add_argument("--record-sample-count", type=int)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    admission_values = (
        args.admit_rustc_release,
        args.admit_cargo_release,
        args.admit_rustc_binary_sha256,
        args.admit_cargo_binary_sha256,
        args.admit_benchmark_binary_sha256,
        args.admit_target_triple,
        args.admit_target_arch,
        args.admit_target_os,
        args.admit_pointer_width_bits,
        args.admit_package_version,
        args.admit_sample_count,
    )
    record_values = (args.record_toolchain, args.record_sample_count)
    require(
        (args.record and all(value is not None for value in record_values))
        or (not args.record and all(value is None for value in record_values)),
        "--record requires both --record-toolchain and --record-sample-count",
    )
    require(
        not args.record or all(value is None for value in admission_values),
        "recording and pre-declared admission pins are separate operations",
    )
    require(
        not args.record or args.artifact_sha256 is None,
        "a new recording cannot have a pre-declared artifact SHA-256",
    )
    require(
        all(value is None for value in admission_values)
        or all(value is not None for value in admission_values),
        "exact admission requires all --admit-* protocol pins together",
    )
    require(
        args.artifact_sha256 is None or all(value is not None for value in admission_values),
        "durable artifact verification requires all --admit-* protocol pins",
    )
    protocol = None
    if args.admit_rustc_release is not None:
        require(args.admit_sample_count >= 5,
                "--admit-sample-count must pin at least five raw observations")
        require(args.admit_pointer_width_bits > 0,
                "--admit-pointer-width-bits must be positive")
        require(args.admit_package_version != "",
                "--admit-package-version must be non-empty")
        require_digest(args.admit_rustc_binary_sha256,
                       "--admit-rustc-binary-sha256")
        require_digest(args.admit_cargo_binary_sha256,
                       "--admit-cargo-binary-sha256")
        require_digest(args.admit_benchmark_binary_sha256,
                       "--admit-benchmark-binary-sha256")
        protocol = AdmissionProtocol(
            rustc_release=args.admit_rustc_release,
            cargo_release=args.admit_cargo_release,
            rustc_binary_sha256=args.admit_rustc_binary_sha256,
            cargo_binary_sha256=args.admit_cargo_binary_sha256,
            benchmark_binary_sha256=args.admit_benchmark_binary_sha256,
            target_triple=args.admit_target_triple,
            target_arch=args.admit_target_arch,
            target_os=args.admit_target_os,
            pointer_width_bits=args.admit_pointer_width_bits,
            package_version=args.admit_package_version,
            sample_count=args.admit_sample_count,
        )
    if args.record:
        record_artifact(
            args.artifact,
            args.record_toolchain,
            args.record_sample_count,
        )
    payload_bytes = args.artifact.read_bytes()
    digest_checks = 0
    if args.artifact_sha256 is not None:
        check_artifact_digest(payload_bytes, args.artifact_sha256)
        if args.self_test:
            try:
                check_artifact_digest(payload_bytes + b"\n", args.artifact_sha256)
            except ValueError:
                digest_checks = 1
            else:
                raise ValueError("checker mutation survived: artifact SHA-256")
    parser_checks = run_json_parser_self_tests() if args.self_test else 0
    recipe_checks = run_record_recipe_self_tests() if args.self_test else 0
    payload = decode_benchmark_artifact(payload_bytes)
    check(payload, protocol)
    mutation_checks = (
        run_mutation_self_tests(payload, protocol)
        if args.self_test
        else 0
    )
    mutation_checks += digest_checks + parser_checks + recipe_checks
    print(
        "WCAG22 feasibility benchmark artifact: PASS; "
        f"scenarios={len(REQUIRED_SCENARIOS)}; "
        f"samples={payload['sampleCount']}; "
        f"mode={'durable-admission' if args.artifact_sha256 is not None else ('admission' if protocol is not None else 'measurement')}; "
        f"mutation_checks={mutation_checks}; timing_thresholds=none"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
