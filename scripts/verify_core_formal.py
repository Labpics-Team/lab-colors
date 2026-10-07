#!/usr/bin/env python3
"""Kani-доказательства ядра: обязательные свойства, достижимость и предметный RED."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
import stat
from pathlib import Path
import subprocess

from formal_contracts import CONTRACTS, MUTANTS, SOURCES

ROOT = Path(__file__).resolve().parents[1]
KANI_VERSION = "0.68.0"
CORE = ROOT / "crates/labcolors-core/src"
FRAGMENT_SCHEMA = 1

# Five shards are a measured scheduling hint, not a proof assumption. The partition
# was balanced from the 2026-10-02 exact-head baseline; completeness below is
# checked against CONTRACTS so a new or renamed harness fails closed until placed.
POSITIVE_SHARDS = (
    (
        "field_effect::proofs::premultiplied_source_over_has_unique_nearest_integer_output",
    ),
    (
        "wcag22::kernel::proofs::interval_default_text_is_sound_for_every_enclosed_point",
        "field_effect::proofs::premultiplied_admission_and_lighter_are_exact",
        "certificate::proofs::reader_bounds_and_failure_atomicity",
        "composition::proofs::opacity_domain_preserves_all_boundaries",
        "authority::clean_convention::proofs::selection_admits_exactly_declared_modeled_point_release",
        "composition::proofs::opacity_admission_is_exact_and_canonical",
    ),
    (
        "authority::proofs::auth_admission_is_exact_atomic_and_lane_local",
        "wcag22::kernel::proofs::interval_graphical_object_is_sound_for_every_enclosed_point",
        "joint::proofs::joint_order_is_complete_unique_and_authored",
        "certificate::proofs::revision_is_exact_lowercase_hex",
        "composition::proofs::source_over_endpoints_preserve_channel_values",
        "srgb8::proofs::parser_preserves_all_srgb8_values",
        "program::wire::proofs::fixed_reads_are_exact_and_atomic",
        "composition::proofs::opacity_multiplication_preserves_identity_and_zero",
        "composition::proofs::identical_channels_are_fixed_for_every_opacity",
        "program::attachment::proofs::mutation_stamp_preserves_epoch_and_cannot_wrap",
        "composition::proofs::multiply_preserves_the_entire_admitted_domain",
    ),
    (
        "wcag22::kernel::proofs::exact_points_are_total_symmetric_and_correct",
        "wcag22::kernel::proofs::interval_ui_component_is_sound_for_every_enclosed_point",
        "observation::proofs::equality_requires_the_same_observation_allocation",
        "program::attachment::handoff::proofs::preparation_is_exact_and_has_no_publication_effect",
        "srgb8::proofs::hex_admits_exactly_six_hex_digits",
        "program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host",
        "field_effect::proofs::gaussian_sampling_clamps_without_coordinate_wrap",
        "certificate::proofs::reader_big_endian_and_length_prefix",
        "field_effect::proofs::rectangles_admit_exactly_nonempty_in_bounds_geometry",
        "program::wire::proofs::floating_wire_keeps_every_bit",
        "session::proofs::state_displacement_conserves_every_owned_payload",
        "certificate::proofs::wire_resource_length_is_exact",
    ),
    (
        "authority::proofs::auth_issue_admit_require_composes",
        "wcag22::kernel::proofs::interval_large_text_is_sound_for_every_enclosed_point",
        "authority::proofs::auth_require_iff_full_binding",
        "program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only",
        "authority::proofs::auth_permit_iff_exact_current_proof",
        "joint::proofs::doubling_cardinality_cannot_wrap_into_empty_success",
        "field_effect::proofs::expanded_rectangles_preserve_exact_clipped_influence",
        "composition::proofs::source_over_is_bounded_before_quantization",
        "program::wire::proofs::section_count_preserves_resource_limit",
        "certificate::proofs::ledger_budget_cannot_overflow_or_exceed_capacity",
        "session::proofs::current_evidence_never_promotes_historical_state",
    ),
)


def validate_result(result: dict) -> None:
    if result["status"] != "Success":
        raise ValueError("unsuccessful harness")
    harness = result["harness_id"]
    if harness not in CONTRACTS:
        raise ValueError("unexpected harness")
    assertions, covers = CONTRACTS[harness]
    seen_assertions, seen_covers = set(), set()
    for check in result["checks"]:
        category = check["category"]
        wanted = "Satisfied" if category == "cover" else "Success"
        if check["status"] != wanted:
            required = category == "cover" or check["description"] in {json.dumps(a) for a in assertions}
            if check["status"] != "Unreachable" or required:
                raise ValueError(f"non-passing property: {check['description']}")
        if category == "cover":
            seen_covers.add(check["description"])
        if category == "assertion":
            seen_assertions.add(check["description"])
    if not {json.dumps(a) for a in assertions} <= seen_assertions:
        raise ValueError("missing semantic assertion")
    if seen_covers != covers:
        raise ValueError("missing or unexpected reachability witness")


def validate_report(report: dict) -> None:
    """Неполный, пустой, недостижимый или чужой результат не означает успех."""
    if report["metadata"]["kani_version"] != KANI_VERSION:
        raise ValueError("unexpected Kani version")
    if report["metadata"]["target"] != "x86_64-unknown-linux-gnu":
        raise ValueError("unexpected verification target")
    verification = report["verification_results"]
    summary = verification["summary"]
    count = len(CONTRACTS)
    if any(summary[key] != count for key in ("total_harnesses", "executed", "successful")):
        raise ValueError("incomplete harness execution")
    if summary["failed"] != 0 or summary["status"] != "completed":
        raise ValueError("verification did not complete successfully")
    results = verification["results"]
    expected = set(CONTRACTS)
    if len(results) != count or {r["harness_id"] for r in results} != expected:
        raise ValueError("missing, duplicate or unexpected harness")
    for result in results:
        validate_result(result)


def validate_positive_subset(report: dict, harnesses: tuple[str, ...]) -> None:
    if report["metadata"]["kani_version"] != KANI_VERSION:
        raise ValueError("unexpected Kani version")
    if report["metadata"]["target"] != "x86_64-unknown-linux-gnu":
        raise ValueError("unexpected verification target")
    verification = report["verification_results"]
    summary = verification["summary"]
    count = len(harnesses)
    if any(summary[key] != count for key in ("total_harnesses", "executed", "successful")):
        raise ValueError("positive shard did not execute its complete harness set")
    if summary["failed"] != 0 or summary["status"] != "completed":
        raise ValueError("positive shard did not complete successfully")
    results = verification["results"]
    if len(results) != count or {result["harness_id"] for result in results} != set(harnesses):
        raise ValueError("positive shard returned a different harness set")
    for result in results:
        validate_result(result)


def validate_mutant(report: dict, returncode: int, mutant: tuple) -> None:
    """Ошибка сборки, timeout или посторонний отказ не засчитываются как RED."""
    _, _, harness, assertion, _, _ = mutant
    verification = report["verification_results"]
    results = verification["results"]
    summary = verification["summary"]
    if (returncode != 1 or len(results) != 1 or results[0]["harness_id"] != harness
            or results[0]["status"] != "Failure" or summary["status"] != "completed"
            or summary["total_harnesses"] != 1 or summary["executed"] != 1
            or summary["failed"] != 1 or summary["successful"] != 0):
        raise ValueError("mutant did not complete the required harness")
    failures = [c for c in results[0]["checks"] if c["status"] == "Failure"]
    allowed_assertions = {json.dumps(a) for a in CONTRACTS[harness][0]}
    if not any(c["description"] == json.dumps(assertion) for c in failures):
        raise ValueError("mutant did not falsify its required semantic assertion")
    for check in results[0]["checks"]:
        if check["status"] in ("Success", "Satisfied", "Unreachable"):
            continue
        if (check["category"] == "cover" and check["status"] == "Unsatisfiable"
                and check["description"] in CONTRACTS[harness][1]):
            continue
        if not (check["status"] == "Failure" and check["category"] == "assertion"
                and check["description"] in allowed_assertions):
            raise ValueError("mutant failed outside its declared semantic properties")


def run_kani(
    output: Path,
    *,
    harness: str | None = None,
    positive_harnesses: tuple[str, ...] | None = None,
) -> tuple[dict, int]:
    output.unlink(missing_ok=True)
    if harness is not None and positive_harnesses is not None:
        raise ValueError("mutant and positive harness filters are mutually exclusive")
    command = ["cargo", "kani", "-p", "labcolors-core", "--lib", "--output-format", "terse",
               "-Z", "unstable-options", "--harness-timeout", "300s", "--export-json", str(output)]
    if harness is not None:
        command.extend(["--harness", harness, "--exact", "-Z", "concrete-playback", "--concrete-playback", "print"])
    else:
        command.append("--jobs=4")
        if positive_harnesses is not None:
            if not positive_harnesses:
                raise ValueError("empty positive harness shard")
            for selected in positive_harnesses:
                command.extend(["--harness", selected])
            command.append("--exact")
    log = output.with_suffix(".log")
    with log.open("w", encoding="utf-8") as stream:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            process.wait(timeout=1200)
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    print(log.read_text(encoding="utf-8"), end="", flush=True)
    return json.loads(output.read_text(encoding="utf-8")), process.returncode


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def formal_source_files() -> list[Path]:
    files = [
        ROOT / "Cargo.toml",
        ROOT / "Cargo.lock",
        ROOT / "crates/labcolors-core/Cargo.toml",
        ROOT / "crates/labcolors-core/build.rs",
        CORE / "lib.rs",
        CORE / "program_wire.rs",
        Path(__file__),
        ROOT / "scripts/formal_contracts.py",
    ]
    for source in SOURCES:
        files.extend([CORE / source, CORE / source.removesuffix(".rs") / "proofs.rs"])
    files.extend([
        CORE / "clean_set.rs",
        ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v2/receipt-v2.sha256",
    ])
    return files


def git_checkout_environment() -> dict[str, str]:
    environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    environment.update({
        "GIT_NO_REPLACE_OBJECTS": "1",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_OPTIONAL_LOCKS": "0",
        "LC_ALL": "C",
    })
    return environment


def git_checkout_output(*args: str) -> str:
    return subprocess.check_output(
        ["git", "-c", "core.fsmonitor=false", *args],
        cwd=ROOT, text=True, env=git_checkout_environment(),
    ).strip()


def core_tree_has_no_physical_extras(paths: set[str]) -> bool:
    core_relative = Path("crates/labcolors-core")
    core_root = ROOT / core_relative
    prefix = core_relative.as_posix() + "/"
    expected_files = {path for path in paths if path.startswith(prefix)}
    if not expected_files or core_root.is_symlink() or not core_root.is_dir():
        return False

    expected_directories: set[str] = set()
    for file in expected_files:
        parent = Path(file).parent
        while parent != core_relative:
            expected_directories.add(parent.as_posix())
            parent = parent.parent
    expected = expected_files | expected_directories

    # Cargo сверяет физическое поддерево Core, включая игнорируемые Git записи.
    observed: set[str] = set()
    pending = [core_root]
    while pending:
        for entry in pending.pop().iterdir():
            relative = entry.relative_to(ROOT).as_posix()
            if relative not in expected:
                return False
            observed.add(relative)
            if relative in expected_directories:
                if entry.is_symlink() or not entry.is_dir():
                    return False
                pending.append(entry)
    return observed == expected


def tracked_tree_matches_head(commit: str, identities: dict[str, str]) -> bool:
    # Сначала удостоверяем достижимые объекты Git: cat-file может отдать повреждённый blob под прежним OID.
    environment = git_checkout_environment()
    command = ["git", "-c", "core.fsmonitor=false"]
    try:
        subprocess.check_output(
            [*command, "fsck", "--strict", "--no-reflogs", "--no-progress", "--no-dangling", commit],
            cwd=ROOT, env=environment, stderr=subprocess.DEVNULL,
        )
        tree = subprocess.check_output(
            [*command, "ls-tree", "-rz", "--full-tree", commit], cwd=ROOT, env=environment,
        )
        entries = []
        paths = set()
        for record in tree.split(b"\0"):
            if not record:
                continue
            metadata, path_bytes = record.split(b"\t", 1)
            mode, kind, oid = metadata.split(b" ")
            if kind != b"blob" or mode not in {b"100644", b"100755", b"120000"}:
                return False
            relative = os.fsdecode(path_bytes)
            entries.append((mode, oid, ROOT / relative))
            paths.add(Path(relative).as_posix())
        formal_paths = {Path(path).as_posix() for path in identities}
        if formal_paths - paths:
            return False
        blobs = subprocess.check_output(
            [*command, "cat-file", "--batch"],
            input=b"".join(oid + b"\n" for _, oid, _ in entries), cwd=ROOT, env=environment,
        )
        cursor = 0
        for mode, oid, path in entries:
            if mode == b"120000":
                # Git связывает только текст ссылки; формальный вход может читать изменяемую цель.
                if (path.is_relative_to(ROOT / "crates/labcolors-core")
                        or path.relative_to(ROOT).as_posix() in formal_paths):
                    return False
                if not path.is_symlink():
                    return False
                actual = os.fsencode(os.readlink(path))
            else:
                if path.is_symlink() or not path.is_file():
                    return False
                actual = path.read_bytes()
                if os.name != "nt" and bool(path.stat().st_mode & stat.S_IXUSR) != (mode == b"100755"):
                    return False
            end = blobs.find(b"\n", cursor)
            if end < 0 or blobs[cursor:end] != oid + b" blob " + str(len(actual)).encode("ascii"):
                return False
            cursor = end + 1
            if blobs[cursor:cursor + len(actual)] != actual or blobs[cursor + len(actual):cursor + len(actual) + 1] != b"\n":
                return False
            cursor += len(actual) + 1
        return cursor == len(blobs) and core_tree_has_no_physical_extras(paths)
    except (OSError, ValueError, subprocess.CalledProcessError):
        return False


def checkout_is_clean(identities: dict[str, str] | None = None, commit: str | None = None) -> bool:
    # Git status скрывает правки с флагами индекса; для связи квитанции с HEAD этого недостаточно.
    version_text = git_checkout_output("version").split()
    try:
        version = tuple(int(part) for part in version_text[2].split(".")[:3])
    except (IndexError, ValueError) as error:
        raise ValueError("cannot verify Git version for formal checkout") from error
    if len(version) != 3 or version < (2, 35, 2):
        raise ValueError("Git 2.35.2 or newer is required for formal checkout")
    records = git_checkout_output("ls-files", "-v", "-z").split("\0")
    status = git_checkout_output("status", "--porcelain", "--untracked-files=all")
    if status or any(not record.startswith("H ") for record in records if record):
        return False
    if identities is None:
        identities = {str(path.relative_to(ROOT)): digest(path) for path in formal_source_files()}
    if commit is None:
        commit = git_checkout_output("rev-parse", "HEAD")
    return tracked_tree_matches_head(commit, identities)


def checkout_snapshot() -> tuple[dict[str, str], str, bool]:
    files = formal_source_files()
    identities = {str(path.relative_to(ROOT)): digest(path) for path in files}
    commit = git_checkout_output("rev-parse", "HEAD")
    clean = checkout_is_clean(identities, commit)
    subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        cwd=ROOT, stdout=subprocess.DEVNULL, check=True,
    )
    return identities, commit, clean


def assert_snapshot(identities: dict[str, str], commit: str) -> bool:
    current = {str(path.relative_to(ROOT)): digest(path) for path in formal_source_files()}
    if identities != current:
        raise ValueError("source or dependency identity changed during verification")
    actual_commit = git_checkout_output("rev-parse", "HEAD")
    if commit != actual_commit:
        raise ValueError("checkout changed during verification")
    return checkout_is_clean(current, commit)


def fragment_base(identities: dict[str, str], commit: str, clean: bool) -> dict:
    return {
        "fragment_schema": FRAGMENT_SCHEMA,
        "kani_version": KANI_VERSION,
        "source_commit": commit if clean else None,
        "checkout_commit": commit,
        "working_tree_clean": clean,
        "verified_source_sha256": identities,
    }


def selected_positive_harnesses(index: int, count: int) -> tuple[str, ...]:
    if count == 1:
        if index != 0:
            raise ValueError("invalid positive shard coordinates")
        return tuple(sorted(CONTRACTS))
    if count != len(POSITIVE_SHARDS) or index < 0 or index >= count:
        raise ValueError("positive proofs use the measured five-shard partition")
    flattened = [harness for shard in POSITIVE_SHARDS for harness in shard]
    if len(flattened) != len(set(flattened)) or set(flattened) != set(CONTRACTS):
        raise ValueError("positive shard partition does not exactly cover CONTRACTS")
    return POSITIVE_SHARDS[index]


def run_positive_phase(output: Path, index: int, count: int) -> None:
    identities, commit, initially_clean = checkout_snapshot()
    fragment = fragment_base(identities, commit, initially_clean)

    if count == 1:
        report, code = run_kani(output / "positive.json")
        if code != 0:
            raise ValueError("positive verification returned failure")
        validate_report(report)
        finally_clean = assert_snapshot(identities, commit)
        fragment.update({
            "kind": "positive",
            "shard_index": 0,
            "shard_count": 1,
            "positive_sha256": digest(output / "positive.json"),
            "harnesses": sorted(CONTRACTS),
            "working_tree_clean": initially_clean and finally_clean,
        })
    else:
        harnesses = selected_positive_harnesses(index, count)
        output_path = output / f"positive-{index}.json"
        report, code = run_kani(output_path, positive_harnesses=harnesses)
        if code != 0:
            raise ValueError("positive shard verification returned failure")
        validate_positive_subset(report, harnesses)
        finally_clean = assert_snapshot(identities, commit)
        fragment.update({
            "kind": "positive",
            "shard_index": index,
            "shard_count": count,
            "file": output_path.name,
            "sha256": digest(output_path),
            "harnesses": list(harnesses),
            "working_tree_clean": initially_clean and finally_clean,
        })

    (output / f"fragment-positive-{index}.json").write_text(
        json.dumps(fragment, indent=2) + "\n", encoding="utf-8"
    )


def selected_mutants(index: int, count: int) -> tuple:
    if count < 1 or index < 0 or index >= count:
        raise ValueError("invalid mutant shard coordinates")
    selected = tuple(mutant for position, mutant in enumerate(MUTANTS) if position % count == index)
    if not selected:
        raise ValueError("empty mutant shard")
    return selected


def run_mutant_phase(output: Path, index: int, count: int) -> None:
    identities, commit, initially_clean = checkout_snapshot()
    negative_results = {}
    for mutant in selected_mutants(index, count):
        name, relative, harness, _, before, after = mutant
        source = CORE / relative
        original = source.read_bytes()
        if original.count(before.encode()) != 1:
            raise ValueError(f"{name}: mutation anchor drift")
        output_path = output / f"negative-{name}.json"
        try:
            source.write_bytes(original.replace(before.encode(), after.encode()))
            report, code = run_kani(output_path, harness=harness)
            validate_mutant(report, code, mutant)
        finally:
            source.write_bytes(original)
        negative_results[name] = digest(output_path)
    finally_clean = assert_snapshot(identities, commit)
    fragment = fragment_base(identities, commit, initially_clean and finally_clean)
    fragment.update({
        "kind": "mutants",
        "shard_index": index,
        "shard_count": count,
        "negative_sha256": negative_results,
    })
    (output / f"fragment-mutants-{index}.json").write_text(
        json.dumps(fragment, indent=2) + "\n", encoding="utf-8"
    )


def merge_positive_reports(reports: list[dict]) -> dict:
    if not reports:
        raise ValueError("positive evidence is empty")
    expected_keys = {
        "metadata", "project", "tools", "harness_metadata", "error_details",
        "property_details", "cbmc", "verification_results", "coverage",
    }
    first = reports[0]
    if set(first) != expected_keys:
        raise ValueError("unexpected Kani report schema")
    stable_metadata = {key: value for key, value in first["metadata"].items() if key != "timestamp"}
    stable_project = {key: value for key, value in first["project"].items() if key != "output_dir"}
    seen_harnesses: set[str] = set()
    for report in reports:
        if set(report) != expected_keys:
            raise ValueError("inconsistent Kani report schema")
        metadata = {key: value for key, value in report["metadata"].items() if key != "timestamp"}
        project = {key: value for key, value in report["project"].items() if key != "output_dir"}
        if metadata != stable_metadata or project != stable_project:
            raise ValueError("positive shards used different Kani project metadata")
        if report["tools"] != first["tools"] or report["coverage"] != first["coverage"]:
            raise ValueError("positive shards used different verifier configuration")
        ids = {item["harness_id"] for item in report["verification_results"]["results"]}
        if seen_harnesses & ids:
            raise ValueError("duplicate positive harness")
        seen_harnesses |= ids
    if seen_harnesses != set(CONTRACTS):
        raise ValueError("positive evidence is incomplete")

    merged = {
        "metadata": dict(first["metadata"]),
        "project": dict(first["project"]),
        "tools": first["tools"],
        "harness_metadata": [],
        "error_details": [],
        "property_details": [],
        "cbmc": [],
        "verification_results": {
            "summary": {
                "total_harnesses": len(CONTRACTS),
                "executed": len(CONTRACTS),
                "status": "completed",
                "successful": len(CONTRACTS),
                "failed": 0,
                "duration_ms": 0,
            },
            "results": [],
        },
        "coverage": first["coverage"],
    }
    for report in reports:
        for key in ("harness_metadata", "error_details", "property_details", "cbmc"):
            if not isinstance(report[key], list):
                raise ValueError(f"{key} must remain a list")
            merged[key].extend(report[key])
        verification = report["verification_results"]
        merged["verification_results"]["results"].extend(verification["results"])
        merged["verification_results"]["summary"]["duration_ms"] += verification["summary"].get("duration_ms", 0)
    merged["verification_results"]["results"].sort(key=lambda item: item["harness_id"])
    validate_report(merged)
    return merged


def assemble_fragments(output: Path, input_dir: Path) -> None:
    fragments = [json.loads(path.read_text(encoding="utf-8"))
                 for path in sorted(input_dir.glob("fragment-*.json"))]
    for fragment in fragments:
        if fragment.get("kind") not in {"positive", "mutants"}:
            raise ValueError("unknown formal fragment kind")
    positives = [item for item in fragments if item["kind"] == "positive"]
    mutants = [item for item in fragments if item["kind"] == "mutants"]
    if not positives or not mutants:
        raise ValueError("missing formal proof fragments")
    reference = positives[0]

    common = ("fragment_schema", "kani_version", "checkout_commit", "source_commit",
              "working_tree_clean", "verified_source_sha256")
    for fragment in fragments:
        if fragment.get("fragment_schema") != FRAGMENT_SCHEMA or fragment.get("kani_version") != KANI_VERSION:
            raise ValueError("foreign formal fragment")
        if any(fragment.get(key) != reference.get(key) for key in common):
            raise ValueError("formal fragments describe different source states")

    identities, commit, clean = checkout_snapshot()
    if (not clean or reference.get("working_tree_clean") is not True
            or reference.get("checkout_commit") != commit
            or reference.get("source_commit") != commit
            or reference.get("verified_source_sha256") != identities):
        raise ValueError("formal fragments do not bind to the current clean source state")

    positive_counts = {item.get("shard_count") for item in positives}
    if len(positive_counts) != 1:
        raise ValueError("positive shard-count mismatch")
    positive_count = positive_counts.pop()
    positive_indexes = [item.get("shard_index") for item in positives]
    if not isinstance(positive_count, int) or sorted(positive_indexes) != list(range(positive_count)):
        raise ValueError("missing or duplicate positive shard")

    if positive_count == 1 and "positive_sha256" in reference:
        if reference.get("harnesses") != sorted(CONTRACTS):
            raise ValueError("positive fragment lost a required harness")
        positive_path = input_dir / "positive.json"
        if digest(positive_path) != reference.get("positive_sha256"):
            raise ValueError("positive evidence digest mismatch")
        validate_report(json.loads(positive_path.read_text(encoding="utf-8")))
        positive_sha256 = reference["positive_sha256"]
    else:
        if positive_count != len(POSITIVE_SHARDS):
            raise ValueError("unexpected positive shard count")
        reports = []
        seen_harnesses: set[str] = set()
        for fragment in positives:
            index = fragment["shard_index"]
            expected_harnesses = selected_positive_harnesses(index, positive_count)
            if tuple(fragment.get("harnesses", ())) != expected_harnesses:
                raise ValueError("positive shard contains the wrong harness set")
            if seen_harnesses & set(expected_harnesses):
                raise ValueError("duplicate positive harness")
            seen_harnesses |= set(expected_harnesses)
            path = input_dir / fragment.get("file", "")
            if not path.is_file() or digest(path) != fragment.get("sha256"):
                raise ValueError(f"positive shard {index}: evidence digest mismatch")
            report = json.loads(path.read_text(encoding="utf-8"))
            validate_positive_subset(report, expected_harnesses)
            reports.append(report)
        if seen_harnesses != set(CONTRACTS):
            raise ValueError("positive evidence is incomplete")
        merged = merge_positive_reports(reports)
        positive_path = output / "positive.json"
        positive_path.write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")
        positive_sha256 = digest(positive_path)

    counts = {item.get("shard_count") for item in mutants}
    if len(counts) != 1:
        raise ValueError("mutant shard-count mismatch")
    shard_count = counts.pop()
    if not isinstance(shard_count, int) or shard_count < 1:
        raise ValueError("invalid mutant shard count")
    indexes = [item.get("shard_index") for item in mutants]
    if sorted(indexes) != list(range(shard_count)):
        raise ValueError("missing or duplicate mutant shard")

    negative = {}
    for fragment in mutants:
        for name, claimed in fragment.get("negative_sha256", {}).items():
            if name in negative or name not in {item[0] for item in MUTANTS}:
                raise ValueError("duplicate or foreign semantic mutant")
            path = input_dir / f"negative-{name}.json"
            if digest(path) != claimed:
                raise ValueError(f"{name}: evidence digest mismatch")
            negative[name] = claimed
    expected = {item[0] for item in MUTANTS}
    if set(negative) != expected:
        raise ValueError("semantic mutant evidence is incomplete")

    receipt = {
        "scope": "Core Linux x86_64; per-harness domains documented in docs/how-to/formal-core.md",
        "schema": 2,
        "kani_version": KANI_VERSION,
        "source_commit": reference["source_commit"],
        "checkout_commit": reference["checkout_commit"],
        "working_tree_clean": reference["working_tree_clean"],
        "verified_source_sha256": reference["verified_source_sha256"],
        "positive_sha256": positive_sha256,
        "negative_sha256": negative,
        "harnesses": sorted(CONTRACTS),
        "semantic_mutant_rejected": True,
    }
    if not assert_snapshot(identities, commit):
        raise ValueError("source state became dirty during formal assembly")
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kani-version", action="store_true")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--phase", choices=("all", "positive", "mutants", "assemble"), default="all")
    parser.add_argument("--shard-index", type=int, default=0)
    parser.add_argument("--shard-count", type=int, default=1)
    parser.add_argument("--input-dir", type=Path)
    args = parser.parse_args()
    if args.kani_version:
        print(KANI_VERSION)
        return
    if args.output_dir is None:
        parser.error("--output-dir is required")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "receipt.json").unlink(missing_ok=True)

    if args.phase == "positive":
        run_positive_phase(output, args.shard_index, args.shard_count)
    elif args.phase == "mutants":
        run_mutant_phase(output, args.shard_index, args.shard_count)
    elif args.phase == "assemble":
        if args.input_dir is None:
            parser.error("--input-dir is required for assemble")
        assemble_fragments(output, args.input_dir.resolve())
    else:
        run_positive_phase(output, args.shard_index, args.shard_count)
        run_mutant_phase(output, 0, 1)
        assemble_fragments(output, output)
        print(
            f"Core formal gate: {len(CONTRACTS)} proofs, all required witnesses reachable, "
            f"{len(MUTANTS)} semantic mutants rejected"
        )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Core formal gate FAILED: {error}") from error
