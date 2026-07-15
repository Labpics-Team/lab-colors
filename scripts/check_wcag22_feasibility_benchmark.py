#!/usr/bin/env python3
"""Validate WCAG22 feasibility raw benchmark identity and capacity algebra.

This checker intentionally does not admit or reject elapsed time. Native timing
and allocator observations stay raw evidence; WebAssembly memory and serialized
size are explicitly outside this artifact's claim boundary.

Generation admission replays the measured revision while it is addressable.
Durable repository verification instead proves that every current subject is
byte-identical to the admitted measurement, so squash merging and deleting the
source branch cannot turn valid evidence into an unreachable Git reference.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
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
HISTORICAL_ONLY_PATH = "Cargo.lock"
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
    HISTORICAL_ONLY_PATH,
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
        revision: str,
        rustc_release: str,
        target_triple: str,
        target_arch: str,
        target_os: str,
        pointer_width_bits: int,
        package_version: str,
        sample_count: int,
    ) -> None:
        self.revision = revision
        self.rustc_release = rustc_release
        self.target_triple = target_triple
        self.target_arch = target_arch
        self.target_os = target_os
        self.pointer_width_bits = pointer_width_bits
        self.package_version = package_version
        self.sample_count = sample_count


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


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
    payload: dict[str, Any], durable_current_subjects: bool
) -> None:
    check_subject_git_bindings(SUBJECT_PATHS, SOURCE_OBJECTS)
    manifest = payload.get("subjectManifest")
    require(isinstance(manifest, list) and len(manifest) == len(SUBJECT_PATHS),
            "subjectManifest must bind every exact dependency-cone path")
    for index, path in enumerate(SUBJECT_PATHS):
        entry = manifest[index]
        require(isinstance(entry, dict) and entry.get("path") == path,
                f"subjectManifest[{index}] path/order drifted")
        if current_byte_identity_required(path, durable_current_subjects):
            expected = hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
            require(entry.get("sha256") == expected,
                    f"subjectManifest source drift: {path}")


def current_byte_identity_required(path: str, durable_current_subjects: bool) -> bool:
    """Return whether this verification mode owns exact current bytes for path."""
    return not (durable_current_subjects and path == HISTORICAL_ONLY_PATH)


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


def check_current_subjects_clean() -> None:
    result = subprocess.run(
        [
            "git",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
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
    require_current_subjects_clean(result.stdout)


def require_current_subjects_clean(status: str) -> None:
    require(status == "", "current dependency-cone worktree is dirty")


def check_source_objects(
    environment: dict[str, Any],
    revision: str,
    tree: str,
    verify_current_subjects: bool,
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
            object_id == "unavailable"
            or (isinstance(object_id, str) and GIT_OBJECT.fullmatch(object_id) is not None),
            f"environment.sourceObjects.{name}.gitObject is invalid",
        )

    if revision == "unavailable":
        require(tree == "unavailable",
                "gitTree must be unavailable when complete revision provenance is unavailable")
        require(not verify_current_subjects,
                "current-subject verification requires complete measured provenance")
        return

    if verify_current_subjects:
        require(tree != "unavailable",
                "current-subject verification requires a recorded measurement tree")
        check_current_subjects_clean()
        for name, path in SOURCE_OBJECTS:
            if current_byte_identity_required(path, verify_current_subjects):
                require(values[name]["gitObject"] == git_rev_parse(f"HEAD:{path}"),
                        f"current Git subject differs from admitted measurement: {path}")
        return

    require(tree == git_rev_parse(f"{revision}^{{tree}}"),
            "recorded gitTree differs from gitRevision")
    for name, path in SOURCE_OBJECTS:
        require(values[name]["gitObject"] == git_rev_parse(f"{revision}:{path}"),
                f"recorded Git object differs from gitRevision: {path}")


def check_environment(
    payload: dict[str, Any],
    protocol: AdmissionProtocol | None,
    verify_current_subjects: bool,
) -> None:
    environment = payload.get("environment")
    require(isinstance(environment, dict) and environment.get("execution") == "native-process",
            "environment must identify native-process execution")
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
    require(type(environment.get("sourceTreeClean")) is bool,
            "environment.sourceTreeClean must preserve dirty-tree truth")
    require(type(environment.get("sampleCountExplicit")) is bool,
            "environment.sampleCountExplicit must preserve protocol truth")
    rustc_verbose = environment.get("rustcVerbose")
    require(isinstance(rustc_verbose, str),
            "environment.rustcVerbose must be present even when unavailable")
    pointer_width_bits = exact_nonnegative_int(
        environment.get("pointerWidthBits"),
        "environment.pointerWidthBits",
    )
    require(pointer_width_bits > 0,
            "environment.pointerWidthBits must be positive")
    package_version = environment.get("packageVersion")
    require(isinstance(package_version, str) and package_version != "",
            "environment.packageVersion must be a non-empty string")

    revision = environment.get("gitRevision")
    tree = environment.get("gitTree")
    require(
        revision == "unavailable"
        or (isinstance(revision, str) and GIT_OBJECT.fullmatch(revision) is not None),
        "environment.gitRevision must be unavailable or one exact Git object ID",
    )
    require(
        tree == "unavailable"
        or (isinstance(tree, str) and GIT_OBJECT.fullmatch(tree) is not None),
        "environment.gitTree must be unavailable or one exact Git object ID",
    )
    check_source_objects(environment, revision, tree, verify_current_subjects)

    if protocol is None:
        return
    require(protocol.sample_count >= 5,
            "admission protocol requires at least five raw observations")
    require(environment["sourceTreeClean"] is True,
            "admission requires a clean source tree")
    require(environment["sampleCountExplicit"] is True,
            "admission requires an explicitly pinned raw sample count")
    require(environment.get("debugAssertions") is False,
            "admission requires an optimized release-profile binary")
    require(revision == protocol.revision,
            "measured Git revision differs from the pinned admission revision")
    require(tree != "unavailable", "admission requires an exact Git tree identity")
    require(
        all(
            environment["sourceObjects"][name]["gitObject"] != "unavailable"
            for name, _ in SOURCE_OBJECTS
        ),
        "admission requires every exact Git subject object",
    )
    require(environment.get("targetArch") == protocol.target_arch,
            "measured target architecture differs from the pinned reference target")
    require(environment.get("targetOs") == protocol.target_os,
            "measured target OS differs from the pinned reference target")
    require(pointer_width_bits == protocol.pointer_width_bits,
            "measured pointer width differs from the pinned reference target")
    require(package_version == protocol.package_version,
            "measured package version differs from the pinned admission version")
    require(
        rustc_verbose.startswith(f"rustc {protocol.rustc_release} "),
        "measured rustc differs from the pinned admission toolchain",
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
    verify_current_subjects: bool = False,
) -> None:
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
    check_environment(payload, protocol, verify_current_subjects)

    sample_count = exact_nonnegative_int(payload.get("sampleCount"), "sampleCount")
    require(sample_count > 0, "sampleCount must be positive")
    require(payload.get("warmupSamples") == 0,
            "raw protocol must retain cold observations instead of hiding warmup calls")
    require(payload.get("scenarioOrder") == "as-emitted",
            "scenario order must remain explicit and replayable")
    check_hard_slo(payload)
    check_bounded_envelope_model(payload)
    check_profile_limits(payload)
    check_subject_manifest(payload, verify_current_subjects)

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


def run_mutation_self_tests(
    payload: dict[str, Any],
    protocol: AdmissionProtocol | None,
    verify_current_subjects: bool,
) -> int:
    mutation_checks = 0

    require(
        not current_byte_identity_required(HISTORICAL_ONLY_PATH, True),
        "durable verification must delegate historical-only Cargo.lock compatibility",
    )
    require(
        current_byte_identity_required(HISTORICAL_ONLY_PATH, False),
        "admission verification must retain exact measured Cargo.lock bytes",
    )
    require(
        all(
            current_byte_identity_required(path, True)
            for path in SUBJECT_PATHS
            if path != HISTORICAL_ONLY_PATH
        ),
        "durable verification must retain every non-historical subject byte",
    )

    def rejected(mutator: Any, label: str) -> None:
        nonlocal mutation_checks
        candidate = copy.deepcopy(payload)
        mutator(candidate)
        try:
            check(candidate, protocol, verify_current_subjects)
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
    rejected(
        lambda value: value["subjectManifest"][0].__setitem__("sha256", "0" * 64),
        "subject SHA-256",
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
    if verify_current_subjects:
        rejected(
            lambda value: value["environment"]["sourceObjects"]["coreCargo"].__setitem__(
                "gitObject", "0" * 40
            ),
            "current source object replay",
        )
    elif payload["environment"]["gitRevision"] == "unavailable":
        rejected(
            lambda value: value["environment"].__setitem__(
                "gitTree", "0" * 40
            ),
            "unavailable revision/tree consistency",
        )
    else:
        rejected(
            lambda value: value["environment"].__setitem__("gitTree", "0" * 40),
            "revision tree replay",
        )

    timing = copy.deepcopy(payload)
    timing["scenarios"][0]["samples"][0]["elapsedNs"] = 10**30
    check(timing, protocol, verify_current_subjects)
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
        require_current_subjects_clean("?? crates/labcolors-core/src/unbound.rs\n")
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
    parser.add_argument("--admit-revision")
    parser.add_argument("--admit-rustc-release")
    parser.add_argument("--admit-target-triple")
    parser.add_argument("--admit-target-arch")
    parser.add_argument("--admit-target-os")
    parser.add_argument("--admit-pointer-width-bits", type=int)
    parser.add_argument("--admit-package-version")
    parser.add_argument("--admit-sample-count", type=int)
    parser.add_argument(
        "--verify-current-subjects",
        action="store_true",
        help=(
            "verify current dependency-cone objects instead of resolving the "
            "recorded measurement commit; requires all admission pins and "
            "--artifact-sha256"
        ),
    )
    parser.add_argument("--artifact-sha256")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    admission_values = (
        args.admit_revision,
        args.admit_rustc_release,
        args.admit_target_triple,
        args.admit_target_arch,
        args.admit_target_os,
        args.admit_pointer_width_bits,
        args.admit_package_version,
        args.admit_sample_count,
    )
    require(
        all(value is None for value in admission_values)
        or all(value is not None for value in admission_values),
        "exact admission requires all --admit-* protocol pins together",
    )
    require(
        not args.verify_current_subjects or all(value is not None for value in admission_values),
        "current-subject verification requires all --admit-* protocol pins",
    )
    require(
        not args.verify_current_subjects or args.artifact_sha256 is not None,
        "current-subject verification requires --artifact-sha256",
    )
    require(
        args.artifact_sha256 is None or args.verify_current_subjects,
        "--artifact-sha256 is reserved for durable current-subject verification",
    )
    protocol = None
    if args.admit_revision is not None:
        require(GIT_OBJECT.fullmatch(args.admit_revision) is not None,
                "--admit-revision must be one lowercase 40-hex Git object ID")
        require(args.admit_sample_count >= 5,
                "--admit-sample-count must pin at least five raw observations")
        require(args.admit_pointer_width_bits > 0,
                "--admit-pointer-width-bits must be positive")
        require(args.admit_package_version != "",
                "--admit-package-version must be non-empty")
        protocol = AdmissionProtocol(
            revision=args.admit_revision,
            rustc_release=args.admit_rustc_release,
            target_triple=args.admit_target_triple,
            target_arch=args.admit_target_arch,
            target_os=args.admit_target_os,
            pointer_width_bits=args.admit_pointer_width_bits,
            package_version=args.admit_package_version,
            sample_count=args.admit_sample_count,
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
    payload = json.loads(payload_bytes.decode("utf-8"))
    check(payload, protocol, args.verify_current_subjects)
    mutation_checks = (
        run_mutation_self_tests(payload, protocol, args.verify_current_subjects)
        if args.self_test
        else 0
    )
    mutation_checks += digest_checks
    print(
        "WCAG22 feasibility benchmark artifact: PASS; "
        f"scenarios={len(REQUIRED_SCENARIOS)}; "
        f"samples={payload['sampleCount']}; "
        f"mode={'current-subjects' if args.verify_current_subjects else ('admission' if protocol is not None else 'measurement')}; "
        f"mutation_checks={mutation_checks}; timing_thresholds=none"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
