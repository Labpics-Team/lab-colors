#!/usr/bin/env python3
"""Fail-closed доказательство полноты report-only mutation run."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
from typing import Any, Iterable


SCHEMA_MANIFEST = "lab-colors-mutation-population-v2"
SCHEMA_SHARD = "lab-colors-mutation-shard-v2"
SCHEMA_AGGREGATE = "lab-colors-mutation-aggregate-v2"
SHARD_COUNT = 32
SHARD_ALGORITHM = "round-robin"
TOOL_VERSION = "25.3.1"
TOOL_VERSION_OUTPUT = f"cargo-mutants {TOOL_VERSION}"
TOOL_RELEASE_TAG = "v25.3.1"
TOOL_RELEASE_TAG_OBJECT = "e6113423fb6e94bd7d9e70fedca058eb8468b92c"
TOOL_RELEASE_COMMIT = "49940940bd9846a25e4c2db1c4f00e39a668ed0a"
TOOL_ARCHIVE_URL = (
    "https://github.com/sourcefrog/cargo-mutants/releases/download/"
    "v25.3.1/cargo-mutants-x86_64-unknown-linux-gnu.tar.gz"
)
TOOL_ARCHIVE_SHA256 = (
    "be41e6f74b633452fb17ef3b6b6113e180130f7b5693863b400c58b39e476726"
)
CONFIG_RELPATH = ".cargo/mutants.toml"
CARGO_TOOLCHAIN_ID = "1.96.0-x86_64-unknown-linux-gnu"
CARGO_BINARY_SUFFIX = f"toolchains/{CARGO_TOOLCHAIN_ID}/bin/cargo"
EXECUTION_COMMANDS = {
    "Build": ["test", "--no-run", "--verbose"],
    "Test": ["test", "--verbose"],
}
HEX40 = re.compile(r"[0-9a-f]{40}\Z")
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")
PACKAGE_NAME = re.compile(r"[A-Za-z0-9_.-]+\Z")
PACKAGE_VERSION = re.compile(r"[0-9][0-9A-Za-z.+-]*\Z")
MUTANT_GENRES = {"FnValue", "BinaryOperator", "UnaryOperator", "MatchArm", "MatchArmGuard"}
MUTANT_SUMMARIES = {"CaughtMutant", "MissedMutant", "Timeout", "Unviable"}
COUNT_FIELDS = {
    "CaughtMutant": "caught",
    "MissedMutant": "missed",
    "Timeout": "timeout",
    "Unviable": "unviable",
}


class ContractError(RuntimeError):
    """Артефакты не доказывают полноту scheduled-прогона."""


def _reject_constant(value: str) -> None:
    raise ContractError(f"invalid JSON numeric constant: {value}")


def _object_without_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"invalid JSON: duplicate key {key!r}")
        result[key] = value
    return result


def read_json(path: Path) -> Any:
    try:
        return json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=_object_without_duplicate_keys,
            parse_constant=_reject_constant,
        )
    except (OSError, UnicodeError, json.JSONDecodeError, ContractError) as error:
        if isinstance(error, ContractError) and str(error).startswith("invalid JSON"):
            raise
        raise ContractError(f"invalid JSON in {path}: {error}") from error


def _canonical_bytes(value: Any) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise ContractError(f"value is not canonical JSON: {error}") from error


def _digest_value(value: Any) -> str:
    return hashlib.sha256(_canonical_bytes(value)).hexdigest()


def _digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
    except OSError as error:
        raise ContractError(f"cannot hash {path}: {error}") from error
    return digest.hexdigest()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        path.write_text(
            json.dumps(
                value,
                ensure_ascii=False,
                allow_nan=False,
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
    except (OSError, TypeError, ValueError) as error:
        raise ContractError(f"cannot write JSON {path}: {error}") from error


def _expect_dict(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be an object")
    return value


def _expect_list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ContractError(f"{label} must be an array")
    return value


def _expect_exact_keys(value: dict[str, Any], keys: set[str], label: str) -> None:
    actual = set(value)
    if actual != keys:
        missing = sorted(keys - actual)
        extra = sorted(actual - keys)
        raise ContractError(f"{label} schema mismatch; missing={missing}, extra={extra}")


def _positive_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ContractError(f"{label} must be a positive integer")
    return value


def _nonnegative_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ContractError(f"{label} must be a nonnegative integer")
    return value


def _safe_relative(path_text: Any, label: str, first_component: str | None = None) -> PurePosixPath:
    if not isinstance(path_text, str) or not path_text or "\\" in path_text or "\x00" in path_text:
        raise ContractError(f"unsafe {label}: {path_text!r}")
    path = PurePosixPath(path_text)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise ContractError(f"unsafe {label}: {path_text!r}")
    if path.as_posix() != path_text:
        raise ContractError(f"unsafe {label}: {path_text!r}")
    if first_component is not None and (not path.parts or path.parts[0] != first_component):
        raise ContractError(f"unsafe {label}: {path_text!r}")
    return path


def _validate_span(value: Any, label: str) -> None:
    span = _expect_dict(value, label)
    _expect_exact_keys(span, {"start", "end"}, label)
    points: list[tuple[int, int]] = []
    for name in ("start", "end"):
        point = _expect_dict(span[name], f"{label}.{name}")
        _expect_exact_keys(point, {"line", "column"}, f"{label}.{name}")
        points.append(
            (
                _positive_int(point["line"], f"{label}.{name}.line"),
                _positive_int(point["column"], f"{label}.{name}.column"),
            )
        )
    if points[0] > points[1]:
        raise ContractError(f"{label} start must not follow end")


def _validate_mutant(value: Any, label: str, repo_root: Path | None = None) -> dict[str, Any]:
    item = _expect_dict(value, label)
    _expect_exact_keys(
        item,
        {"package", "file", "function", "span", "replacement", "genre"},
        label,
    )
    if not isinstance(item["package"], str) or not PACKAGE_NAME.fullmatch(item["package"]):
        raise ContractError(f"{label}.package must be a non-empty string")
    relative = _safe_relative(item["file"], "mutant file")
    if relative.suffix != ".rs":
        raise ContractError(f"unsafe mutant file: {item['file']!r}")
    if repo_root is not None:
        source = repo_root.joinpath(*relative.parts)
        try:
            resolved_source = source.resolve(strict=True)
        except OSError as error:
            raise ContractError(
                f"mutant source does not exist as a regular file: {item['file']}"
            ) from error
        try:
            resolved_source.relative_to(repo_root)
        except ValueError as error:
            raise ContractError(f"unsafe mutant file: {item['file']!r}") from error
        if not source.is_file() or source.is_symlink():
            raise ContractError(f"mutant source does not exist as a regular file: {item['file']}")
    function = item["function"]
    if function is not None:
        function = _expect_dict(function, f"{label}.function")
        _expect_exact_keys(
            function,
            {"function_name", "return_type", "span"},
            f"{label}.function",
        )
        if not isinstance(function["function_name"], str) or not function["function_name"]:
            raise ContractError(f"{label}.function.function_name must be non-empty")
        if not isinstance(function["return_type"], str):
            raise ContractError(f"{label}.function.return_type must be a string")
        _validate_span(function["span"], f"{label}.function.span")
    _validate_span(item["span"], f"{label}.span")
    if not isinstance(item["replacement"], str):
        raise ContractError(f"{label}.replacement must be a string")
    if item["genre"] not in MUTANT_GENRES:
        raise ContractError(f"{label}.genre is unknown: {item['genre']!r}")
    return item


def _run_bytes(argv: list[str], *, cwd: Path, label: str) -> bytes:
    try:
        result = subprocess.run(
            argv,
            cwd=cwd,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        raise ContractError(f"cannot execute {label}: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ContractError(f"{label} failed with exit {result.returncode}: {detail}")
    return result.stdout


def _git_output(repo_root: Path, arguments: list[str], label: str) -> str:
    raw = _run_bytes(["git", *arguments], cwd=repo_root, label=label)
    try:
        return raw.decode("utf-8").strip()
    except UnicodeDecodeError as error:
        raise ContractError(f"{label} returned non-UTF-8 output") from error


def _tracked_git_tree(repo_root: Path, revision: str) -> str:
    repo_root = repo_root.resolve()
    if not repo_root.is_dir():
        raise ContractError("repo root is not a directory")
    top_level = Path(
        _git_output(repo_root, ["rev-parse", "--show-toplevel"], "git top-level lookup")
    ).resolve()
    if top_level != repo_root:
        raise ContractError("repo root is not the exact Git worktree root")
    head = _git_output(repo_root, ["rev-parse", "--verify", "HEAD"], "git HEAD lookup")
    if head != revision:
        raise ContractError(f"Git HEAD mismatch: expected {revision}, got {head}")
    tracked_status = _run_bytes(
        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
        cwd=repo_root,
        label="tracked worktree status",
    )
    if tracked_status:
        raise ContractError("tracked worktree is dirty; source snapshot is not exact")
    tree = _git_output(
        repo_root,
        ["rev-parse", "--verify", f"{revision}^{{tree}}"],
        "git tree lookup",
    )
    if not HEX40.fullmatch(tree):
        raise ContractError(f"invalid Git tree identity: {tree!r}")
    return tree


def _source_snapshot_payload(snapshot: dict[str, Any]) -> dict[str, Any]:
    payload = dict(snapshot)
    payload.pop("sha256", None)
    return payload


def _build_source_snapshot(
    repo_root: Path,
    revision: str,
    source_paths: Iterable[str],
) -> dict[str, Any]:
    repo_root = repo_root.resolve()
    git_tree = _tracked_git_tree(repo_root, revision)
    files: list[dict[str, str]] = []
    for path_text in sorted(set(source_paths)):
        relative = _safe_relative(path_text, "source snapshot path")
        _run_bytes(
            ["git", "ls-files", "--error-unmatch", "--", relative.as_posix()],
            cwd=repo_root,
            label=f"tracked source lookup for {relative.as_posix()}",
        )
        source = repo_root.joinpath(*relative.parts)
        if not source.is_file() or source.is_symlink():
            raise ContractError(
                f"source snapshot path is not a regular tracked file: {relative.as_posix()}"
            )
        files.append({"path": relative.as_posix(), "sha256": _digest_file(source)})
    if not files:
        raise ContractError("source snapshot must contain at least one tracked source")
    snapshot: dict[str, Any] = {"files": files, "git_tree": git_tree}
    snapshot["sha256"] = _digest_value(_source_snapshot_payload(snapshot))
    return snapshot


def _build_execution_contract(
    specs: list[dict[str, Any]],
    package_versions: dict[str, str],
) -> dict[str, Any]:
    package_names = sorted({spec["package"] for spec in specs})
    if set(package_versions) != set(package_names):
        raise ContractError(
            "package-version inventory does not exactly cover mutation population packages"
        )
    packages: list[dict[str, str]] = []
    for name in package_names:
        version = package_versions[name]
        if not isinstance(version, str) or not PACKAGE_VERSION.fullmatch(version):
            raise ContractError(f"invalid Cargo package version for {name!r}: {version!r}")
        packages.append(
            {
                "argument": f"--package={name}@{version}",
                "name": name,
                "version": version,
            }
        )
    return {
        "baseline": "run",
        "cargo_binary_suffix": CARGO_BINARY_SUFFIX,
        "commands": EXECUTION_COMMANDS,
        "mutant_test_scope": "mutated-package",
        "packages": packages,
        "phases": ["Build", "Test"],
        "test_tool": "cargo",
    }


def _cargo_package_versions(
    repo_root: Path,
    population: list[dict[str, Any]],
) -> dict[str, str]:
    required = {
        _validate_mutant(spec, f"population[{index}]", repo_root)["package"]
        for index, spec in enumerate(population)
    }
    cargo = os.environ.get("CARGO") or shutil.which("cargo")
    if not cargo:
        raise ContractError("cargo executable is unavailable for package provenance")
    raw = _run_bytes(
        [cargo, "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=repo_root,
        label="cargo metadata",
    )
    try:
        metadata = json.loads(
            raw,
            object_pairs_hook=_object_without_duplicate_keys,
            parse_constant=_reject_constant,
        )
    except (UnicodeError, json.JSONDecodeError, ContractError) as error:
        raise ContractError(f"cargo metadata is invalid JSON: {error}") from error
    metadata = _expect_dict(metadata, "cargo metadata")
    packages = _expect_list(metadata.get("packages"), "cargo metadata.packages")
    workspace_members = set(
        _expect_list(metadata.get("workspace_members"), "cargo metadata.workspace_members")
    )
    by_name: dict[str, str] = {}
    for index, raw_package in enumerate(packages):
        package = _expect_dict(raw_package, f"cargo metadata.packages[{index}]")
        if package.get("id") not in workspace_members:
            continue
        name = package.get("name")
        version = package.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            raise ContractError("cargo metadata package identity is invalid")
        if name in by_name:
            raise ContractError(f"duplicate workspace package name: {name!r}")
        by_name[name] = version
    missing = sorted(required - set(by_name))
    if missing:
        raise ContractError(f"mutation packages are absent from Cargo metadata: {missing}")
    return {name: by_name[name] for name in sorted(required)}


def _tool_provenance() -> dict[str, Any]:
    return {
        "archive_sha256": TOOL_ARCHIVE_SHA256,
        "archive_url": TOOL_ARCHIVE_URL,
        "name": "cargo-mutants",
        "release_commit": TOOL_RELEASE_COMMIT,
        "release_tag": TOOL_RELEASE_TAG,
        "release_tag_object": TOOL_RELEASE_TAG_OBJECT,
        "version": TOOL_VERSION,
        "version_output": TOOL_VERSION_OUTPUT,
    }


def verify_tool_archive(archive: Path, version_output: str) -> dict[str, Any]:
    actual_digest = _digest_file(archive)
    if actual_digest != TOOL_ARCHIVE_SHA256:
        raise ContractError(
            f"tool archive digest mismatch: expected {TOOL_ARCHIVE_SHA256}, got {actual_digest}"
        )
    if version_output.strip() != TOOL_VERSION_OUTPUT:
        raise ContractError(
            f"tool version output mismatch: expected {TOOL_VERSION_OUTPUT!r}, "
            f"got {version_output.strip()!r}"
        )
    return _tool_provenance()


def _manifest_payload(manifest: dict[str, Any]) -> dict[str, Any]:
    payload = dict(manifest)
    payload.pop("manifest_sha256", None)
    return payload


def build_manifest(
    population: list[dict[str, Any]],
    *,
    repo_root: Path,
    config_path: Path,
    package_versions: dict[str, str],
    repository: str,
    revision: str,
    run_id: int,
    run_attempt: int,
) -> dict[str, Any]:
    repo_root = repo_root.resolve()
    config_path = config_path.resolve()
    if config_path != repo_root / CONFIG_RELPATH or not config_path.is_file():
        raise ContractError(f"config must be the regular file {CONFIG_RELPATH}")
    if not REPOSITORY.fullmatch(repository):
        raise ContractError(f"invalid repository identity: {repository!r}")
    if not HEX40.fullmatch(revision):
        raise ContractError(f"invalid revision: {revision!r}")
    run_id = _positive_int(run_id, "run_id")
    run_attempt = _positive_int(run_attempt, "run_attempt")
    if not isinstance(population, list) or not population:
        raise ContractError("discovered mutation population must be non-empty")
    entries: list[dict[str, Any]] = []
    seen: set[str] = set()
    for index, value in enumerate(population):
        spec = _validate_mutant(value, f"population[{index}]", repo_root)
        mutant_id = _digest_value(spec)
        if mutant_id in seen:
            raise ContractError(f"duplicate mutant ID in population: {mutant_id}")
        seen.add(mutant_id)
        entries.append({"id": mutant_id, "spec": spec})
    specs = [entry["spec"] for entry in entries]
    source_snapshot = _build_source_snapshot(
        repo_root,
        revision,
        (spec["file"] for spec in specs),
    )
    manifest: dict[str, Any] = {
        "config": {
            "path": CONFIG_RELPATH,
            "sha256": _digest_file(config_path),
        },
        "execution": _build_execution_contract(specs, package_versions),
        "population": {
            "count": len(entries),
            "mutants": entries,
            "sha256": _digest_value(specs),
        },
        "repository": repository,
        "revision": revision,
        "run": {"attempt": run_attempt, "id": run_id},
        "schema": SCHEMA_MANIFEST,
        "sharding": {"algorithm": SHARD_ALGORITHM, "count": SHARD_COUNT},
        "source_snapshot": source_snapshot,
        "tool": _tool_provenance(),
    }
    manifest["manifest_sha256"] = _digest_value(_manifest_payload(manifest))
    validate_manifest(manifest)
    return manifest


def _validate_source_snapshot(value: Any, expected_paths: set[str]) -> dict[str, Any]:
    snapshot = _expect_dict(value, "manifest.source_snapshot")
    _expect_exact_keys(
        snapshot,
        {"files", "git_tree", "sha256"},
        "manifest.source_snapshot",
    )
    if not isinstance(snapshot["git_tree"], str) or not HEX40.fullmatch(snapshot["git_tree"]):
        raise ContractError("manifest source Git tree is invalid")
    if not isinstance(snapshot["sha256"], str) or not HEX64.fullmatch(snapshot["sha256"]):
        raise ContractError("manifest source snapshot digest is invalid")
    files = _expect_list(snapshot["files"], "manifest.source_snapshot.files")
    observed_paths: list[str] = []
    for index, raw_file in enumerate(files):
        label = f"manifest.source_snapshot.files[{index}]"
        source = _expect_dict(raw_file, label)
        _expect_exact_keys(source, {"path", "sha256"}, label)
        relative = _safe_relative(source["path"], "source snapshot path")
        if (
            relative.suffix != ".rs"
            or not isinstance(source["sha256"], str)
            or not HEX64.fullmatch(source["sha256"])
        ):
            raise ContractError(f"{label} is invalid")
        observed_paths.append(relative.as_posix())
    if observed_paths != sorted(expected_paths) or len(observed_paths) != len(set(observed_paths)):
        raise ContractError("source snapshot does not exactly cover population source files")
    if snapshot["sha256"] != _digest_value(_source_snapshot_payload(snapshot)):
        raise ContractError("manifest source snapshot digest mismatch")
    return snapshot


def _validate_execution_contract(
    value: Any,
    specs: list[dict[str, Any]],
) -> dict[str, Any]:
    execution = _expect_dict(value, "manifest.execution")
    _expect_exact_keys(
        execution,
        {
            "baseline",
            "cargo_binary_suffix",
            "commands",
            "mutant_test_scope",
            "packages",
            "phases",
            "test_tool",
        },
        "manifest.execution",
    )
    fixed = {
        "baseline": "run",
        "cargo_binary_suffix": CARGO_BINARY_SUFFIX,
        "commands": EXECUTION_COMMANDS,
        "mutant_test_scope": "mutated-package",
        "phases": ["Build", "Test"],
        "test_tool": "cargo",
    }
    for field, expected in fixed.items():
        if execution[field] != expected:
            raise ContractError(f"manifest execution contract mismatch for {field}")
    packages = _expect_list(execution["packages"], "manifest.execution.packages")
    observed_names: list[str] = []
    for index, raw_package in enumerate(packages):
        label = f"manifest.execution.packages[{index}]"
        package = _expect_dict(raw_package, label)
        _expect_exact_keys(package, {"argument", "name", "version"}, label)
        name = package["name"]
        version = package["version"]
        if not isinstance(name, str) or not PACKAGE_NAME.fullmatch(name):
            raise ContractError(f"{label}.name is invalid")
        if not isinstance(version, str) or not PACKAGE_VERSION.fullmatch(version):
            raise ContractError(f"{label}.version is invalid")
        if package["argument"] != f"--package={name}@{version}":
            raise ContractError(f"{label}.argument is invalid")
        observed_names.append(name)
    expected_names = sorted({spec["package"] for spec in specs})
    if observed_names != expected_names or len(observed_names) != len(set(observed_names)):
        raise ContractError("execution packages do not exactly cover mutation packages")
    return execution


def validate_manifest(value: Any) -> dict[str, Any]:
    manifest = _expect_dict(value, "manifest")
    _expect_exact_keys(
        manifest,
        {
            "config",
            "execution",
            "manifest_sha256",
            "population",
            "repository",
            "revision",
            "run",
            "schema",
            "sharding",
            "source_snapshot",
            "tool",
        },
        "manifest",
    )
    if manifest["schema"] != SCHEMA_MANIFEST:
        raise ContractError(f"manifest schema mismatch: {manifest['schema']!r}")
    if not REPOSITORY.fullmatch(manifest["repository"]):
        raise ContractError("manifest repository is invalid")
    if not HEX40.fullmatch(manifest["revision"]):
        raise ContractError("manifest revision is invalid")
    run = _expect_dict(manifest["run"], "manifest.run")
    _expect_exact_keys(run, {"attempt", "id"}, "manifest.run")
    _positive_int(run["attempt"], "manifest.run.attempt")
    _positive_int(run["id"], "manifest.run.id")
    if manifest["tool"] != _tool_provenance():
        raise ContractError("manifest tool provenance mismatch")
    config = _expect_dict(manifest["config"], "manifest.config")
    _expect_exact_keys(config, {"path", "sha256"}, "manifest.config")
    if config["path"] != CONFIG_RELPATH or not HEX64.fullmatch(config["sha256"]):
        raise ContractError("manifest config provenance mismatch")
    if manifest["sharding"] != {"algorithm": SHARD_ALGORITHM, "count": SHARD_COUNT}:
        raise ContractError("manifest sharding contract mismatch")
    population = _expect_dict(manifest["population"], "manifest.population")
    _expect_exact_keys(population, {"count", "mutants", "sha256"}, "manifest.population")
    count = _positive_int(population["count"], "manifest.population.count")
    entries = _expect_list(population["mutants"], "manifest.population.mutants")
    if count != len(entries):
        raise ContractError("manifest population count mismatch")
    seen: set[str] = set()
    specs: list[dict[str, Any]] = []
    for index, raw_entry in enumerate(entries):
        entry = _expect_dict(raw_entry, f"manifest.population.mutants[{index}]")
        _expect_exact_keys(entry, {"id", "spec"}, f"manifest.population.mutants[{index}]")
        spec = _validate_mutant(entry["spec"], f"manifest.population.mutants[{index}].spec")
        expected_id = _digest_value(spec)
        if entry["id"] != expected_id or not HEX64.fullmatch(entry["id"]):
            raise ContractError(f"manifest mutant ID mismatch at index {index}")
        if entry["id"] in seen:
            raise ContractError(f"duplicate mutant ID in manifest: {entry['id']}")
        seen.add(entry["id"])
        specs.append(spec)
    if population["sha256"] != _digest_value(specs):
        raise ContractError("manifest population digest mismatch")
    _validate_source_snapshot(
        manifest["source_snapshot"],
        {spec["file"] for spec in specs},
    )
    _validate_execution_contract(manifest["execution"], specs)
    expected_manifest_digest = _digest_value(_manifest_payload(manifest))
    if manifest["manifest_sha256"] != expected_manifest_digest:
        raise ContractError("manifest digest mismatch")
    return manifest


def _validate_checkout(
    manifest: dict[str, Any],
    *,
    repo_root: Path,
    config_path: Path,
    observed_revision: str,
) -> None:
    repo_root = repo_root.resolve()
    config_path = config_path.resolve()
    if config_path != repo_root / CONFIG_RELPATH or not config_path.is_file():
        raise ContractError(f"config must be the regular file {CONFIG_RELPATH}")
    if observed_revision != manifest["revision"]:
        raise ContractError(
            f"revision mismatch: expected {manifest['revision']}, got {observed_revision}"
        )
    actual_config = _digest_file(config_path)
    if actual_config != manifest["config"]["sha256"]:
        raise ContractError(
            f"config digest mismatch: expected {manifest['config']['sha256']}, got {actual_config}"
        )
    expected_snapshot = _build_source_snapshot(
        repo_root,
        observed_revision,
        (entry["spec"]["file"] for entry in manifest["population"]["mutants"]),
    )
    if expected_snapshot != manifest["source_snapshot"]:
        raise ContractError("source snapshot does not match exact tracked checkout bytes")


def expected_shard_entries(manifest: dict[str, Any], shard_index: int) -> list[dict[str, Any]]:
    validate_manifest(manifest)
    if isinstance(shard_index, bool) or not isinstance(shard_index, int):
        raise ContractError("shard index must be an integer")
    if not 0 <= shard_index < SHARD_COUNT:
        raise ContractError(f"shard index outside 0..{SHARD_COUNT - 1}: {shard_index}")
    return manifest["population"]["mutants"][shard_index::SHARD_COUNT]


def expected_shard_specs(manifest: dict[str, Any], shard_index: int) -> list[dict[str, Any]]:
    return [entry["spec"] for entry in expected_shard_entries(manifest, shard_index)]


def _process_status_kind(value: Any, label: str) -> str:
    if isinstance(value, str) and value in {"Success", "Timeout", "Other"}:
        return value
    if isinstance(value, dict) and len(value) == 1:
        kind, code = next(iter(value.items()))
        if (
            kind in {"Failure", "Signalled"}
            and isinstance(code, int)
            and not isinstance(code, bool)
            and code != 0
        ):
            return kind
    raise ContractError(f"{label} has invalid process_status: {value!r}")


def _execution_package_arguments(execution: dict[str, Any]) -> dict[str, str]:
    return {package["name"]: package["argument"] for package in execution["packages"]}


def _validate_cargo_command(
    argv_value: Any,
    *,
    phase_name: str,
    package_names: list[str],
    execution: dict[str, Any],
    label: str,
) -> str:
    argv = _expect_list(argv_value, f"{label}.argv")
    if not argv or any(not isinstance(argument, str) or not argument for argument in argv):
        raise ContractError(f"{label}.argv is invalid")
    cargo_binary = argv[0]
    cargo_path = PurePosixPath(cargo_binary)
    if (
        not cargo_path.is_absolute()
        or cargo_path.as_posix() != cargo_binary
        or not cargo_binary.endswith(f"/{execution['cargo_binary_suffix']}")
    ):
        raise ContractError(f"{label} command identity has an unexpected Cargo executable")
    package_arguments = _execution_package_arguments(execution)
    try:
        selected_packages = [package_arguments[name] for name in package_names]
    except KeyError as error:
        raise ContractError(f"{label} command identity references an unknown package") from error
    expected = [cargo_binary, *execution["commands"][phase_name], *selected_packages]
    if argv != expected:
        raise ContractError(
            f"{label} command identity mismatch: expected {expected[1:]!r}, got {argv[1:]!r}"
        )
    return cargo_binary


def _derive_summary(
    scenario: str,
    phase_results: Any,
    label: str,
    *,
    package_names: list[str],
    execution: dict[str, Any],
) -> tuple[str, str]:
    phases = _expect_list(phase_results, f"{label}.phase_results")
    if not phases:
        raise ContractError(f"{label}.phase_results is empty")
    parsed: list[tuple[str, str, str]] = []
    for index, raw_phase in enumerate(phases):
        phase_label = f"{label}.phase_results[{index}]"
        item = _expect_dict(raw_phase, phase_label)
        _expect_exact_keys(
            item,
            {"argv", "duration", "phase", "process_status"},
            phase_label,
        )
        phase_name = item["phase"]
        if phase_name not in execution["phases"]:
            raise ContractError(f"{label} phase automaton contains forbidden phase {phase_name!r}")
        duration = item["duration"]
        if (
            isinstance(duration, bool)
            or not isinstance(duration, (int, float))
            or not math.isfinite(duration)
            or duration < 0
        ):
            raise ContractError(f"{phase_label}.duration is invalid")
        cargo_binary = _validate_cargo_command(
            item["argv"],
            phase_name=phase_name,
            package_names=package_names,
            execution=execution,
            label=phase_label,
        )
        status = _process_status_kind(item["process_status"], phase_label)
        parsed.append((phase_name, status, cargo_binary))
    phase_names = [phase_name for phase_name, _, _ in parsed]
    if phase_names not in (["Build"], ["Build", "Test"]):
        raise ContractError(f"{label} phase automaton is not a canonical Build -> Test prefix")
    cargo_binaries = {cargo_binary for _, _, cargo_binary in parsed}
    if len(cargo_binaries) != 1:
        raise ContractError(f"{label} command identity changed within one scenario")
    build_status = parsed[0][1]
    if build_status == "Success" and phase_names != ["Build", "Test"]:
        raise ContractError(f"{label} phase automaton stopped after a successful Build")
    if build_status != "Success" and phase_names != ["Build"]:
        raise ContractError(f"{label} phase automaton continued after a terminal Build")
    cargo_binary = parsed[0][2]
    if build_status != "Success":
        if build_status == "Timeout":
            return "Timeout", cargo_binary
        if build_status == "Failure":
            return ("Failure" if scenario == "Baseline" else "Unviable"), cargo_binary
        raise ContractError(f"{label} phase automaton has unsupported Build status")
    test_status = parsed[1][1]
    if test_status == "Timeout":
        return "Timeout", cargo_binary
    if test_status == "Success":
        return ("Success" if scenario == "Baseline" else "MissedMutant"), cargo_binary
    if test_status == "Failure":
        return ("Failure" if scenario == "Baseline" else "CaughtMutant"), cargo_binary
    raise ContractError(f"{label} phase automaton has unsupported Test status")


def _artifact_file(root: Path, path_text: Any, label: str, first_component: str) -> Path:
    relative = _safe_relative(path_text, label, first_component)
    path = root.joinpath(*relative.parts)
    try:
        path.resolve().relative_to(root.resolve())
    except (OSError, ValueError) as error:
        raise ContractError(f"unsafe {label}: {path_text!r}") from error
    if not path.is_file() or path.is_symlink():
        raise ContractError(f"{label} does not name a regular artifact file: {path_text!r}")
    return path


def _artifact_evidence(
    root: Path,
    path_text: Any,
    label: str,
    first_component: str,
) -> dict[str, str]:
    path = _artifact_file(root, path_text, label, first_component)
    return {"path": path_text, "sha256": _digest_file(path)}


def _validate_outcomes(
    value: Any,
    expected_specs: list[dict[str, Any]],
    out_dir: Path,
    execution: dict[str, Any],
) -> tuple[dict[str, int], list[str], list[dict[str, str]], str]:
    report = _expect_dict(value, "outcomes.json")
    _expect_exact_keys(
        report,
        {"caught", "missed", "outcomes", "success", "timeout", "total_mutants", "unviable"},
        "outcomes.json",
    )
    raw_outcomes = _expect_list(report["outcomes"], "outcomes.json.outcomes")
    if len(raw_outcomes) != len(expected_specs) + 1:
        raise ContractError("premature/partial run: outcomes do not cover the shard slice")
    counts = {field: 0 for field in COUNT_FIELDS.values()}
    seen_logs: set[str] = set()
    seen_diffs: set[str] = set()
    observed_specs: list[dict[str, Any]] = []
    referenced_artifacts: list[dict[str, str]] = []
    observed_cargo_binary: str | None = None
    baseline_packages = sorted({spec["package"] for spec in expected_specs})
    for index, raw_outcome in enumerate(raw_outcomes):
        label = f"outcomes.json.outcomes[{index}]"
        outcome = _expect_dict(raw_outcome, label)
        _expect_exact_keys(
            outcome,
            {"diff_path", "log_path", "phase_results", "scenario", "summary"},
            label,
        )
        if index == 0:
            if outcome["scenario"] != "Baseline" or outcome["diff_path"] is not None:
                raise ContractError("outcomes must begin with exactly one baseline")
            derived, cargo_binary = _derive_summary(
                "Baseline",
                outcome["phase_results"],
                label,
                package_names=baseline_packages,
                execution=execution,
            )
            if outcome["summary"] != derived or derived != "Success":
                raise ContractError("baseline summary is not a completed success")
        else:
            scenario = _expect_dict(outcome["scenario"], f"{label}.scenario")
            _expect_exact_keys(scenario, {"Mutant"}, f"{label}.scenario")
            spec = _validate_mutant(scenario["Mutant"], f"{label}.scenario.Mutant")
            observed_specs.append(spec)
            derived, cargo_binary = _derive_summary(
                "Mutant",
                outcome["phase_results"],
                label,
                package_names=[spec["package"]],
                execution=execution,
            )
            if outcome["summary"] != derived or derived not in MUTANT_SUMMARIES:
                raise ContractError(
                    f"invalid mutant summary at outcome {index}: "
                    f"declared={outcome['summary']!r}, derived={derived!r}"
                )
            counts[COUNT_FIELDS[derived]] += 1
            if not isinstance(outcome["diff_path"], str):
                raise ContractError(f"{label}.diff_path must be a path")
            referenced_artifacts.append(
                _artifact_evidence(out_dir, outcome["diff_path"], "diff path", "diff")
            )
            if outcome["diff_path"] in seen_diffs:
                raise ContractError(f"duplicate diff path: {outcome['diff_path']}")
            seen_diffs.add(outcome["diff_path"])
        if not isinstance(outcome["log_path"], str):
            raise ContractError(f"{label}.log_path must be a path")
        referenced_artifacts.append(
            _artifact_evidence(out_dir, outcome["log_path"], "log path", "log")
        )
        if outcome["log_path"] in seen_logs:
            raise ContractError(f"duplicate log path: {outcome['log_path']}")
        seen_logs.add(outcome["log_path"])
        if observed_cargo_binary is None:
            observed_cargo_binary = cargo_binary
        elif cargo_binary != observed_cargo_binary:
            raise ContractError("command identity changed across shard scenarios")
    if _canonical_bytes(observed_specs) != _canonical_bytes(expected_specs):
        raise ContractError("outcome mutant order/content does not match exact shard slice")
    expected_fields = {
        "total_mutants": len(expected_specs),
        "caught": counts["caught"],
        "missed": counts["missed"],
        "timeout": counts["timeout"],
        "unviable": counts["unviable"],
        "success": 0,
    }
    for field, expected in expected_fields.items():
        actual = _nonnegative_int(report[field], f"outcomes.json.{field}")
        if actual != expected:
            raise ContractError(
                f"outcomes count mismatch for {field}: expected {expected}, got {actual}"
            )
    summaries: list[str] = []
    for summary, field in COUNT_FIELDS.items():
        summaries.extend([summary] * counts[field])
    if observed_cargo_binary is None:
        raise ContractError("shard has no observed Cargo command identity")
    return counts, summaries, referenced_artifacts, observed_cargo_binary


def expected_exit_code(summaries: Iterable[str]) -> int:
    summary_set = set(summaries)
    if not summary_set.issubset(MUTANT_SUMMARIES):
        raise ContractError(f"cannot derive exit code from summaries: {sorted(summary_set)}")
    if "Timeout" in summary_set:
        return 3
    if "MissedMutant" in summary_set:
        return 2
    return 0


def _shard_payload(record: dict[str, Any]) -> dict[str, Any]:
    payload = dict(record)
    payload.pop("record_sha256", None)
    return payload


def validate_and_record_shard(
    manifest_value: Any,
    *,
    repo_root: Path,
    config_path: Path,
    output_parent: Path,
    shard_index: int,
    observed_revision: str,
    tool_version_output: str,
    exit_code: int,
) -> dict[str, Any]:
    manifest = validate_manifest(manifest_value)
    _validate_checkout(
        manifest,
        repo_root=repo_root,
        config_path=config_path,
        observed_revision=observed_revision,
    )
    if tool_version_output.strip() != TOOL_VERSION_OUTPUT:
        raise ContractError(
            f"tool version output mismatch: expected {TOOL_VERSION_OUTPUT!r}, "
            f"got {tool_version_output.strip()!r}"
        )
    expected_entries = expected_shard_entries(manifest, shard_index)
    expected_specs = [entry["spec"] for entry in expected_entries]
    output_parent = output_parent.resolve()
    out_dir = output_parent / "mutants.out"
    if not out_dir.is_dir() or out_dir.is_symlink():
        raise ContractError("mutants.out is missing or is not a regular directory")
    lock_path = out_dir / "lock.json"
    lock = _expect_dict(read_json(lock_path), "lock.json")
    _expect_exact_keys(
        lock,
        {"cargo_mutants_version", "hostname", "start_time", "username"},
        "lock.json",
    )
    if lock["cargo_mutants_version"] != TOOL_VERSION:
        raise ContractError(
            f"tool version mismatch in lock.json: expected {TOOL_VERSION}, "
            f"got {lock['cargo_mutants_version']!r}"
        )
    for field in ("hostname", "start_time", "username"):
        if not isinstance(lock[field], str) or not lock[field]:
            raise ContractError(f"lock.json.{field} must be a non-empty string")
    mutants_path = out_dir / "mutants.json"
    observed_specs = _expect_list(read_json(mutants_path), "mutants.json")
    for index, spec in enumerate(observed_specs):
        _validate_mutant(spec, f"mutants.json[{index}]")
    if _canonical_bytes(observed_specs) != _canonical_bytes(expected_specs):
        raise ContractError("mutants.json does not match the exact round-robin shard slice")
    observed_ids = [_digest_value(spec) for spec in observed_specs]
    if len(observed_ids) != len(set(observed_ids)):
        raise ContractError("duplicate mutant IDs inside shard")
    outcomes_path = out_dir / "outcomes.json"
    counts, summaries, referenced_artifacts, cargo_binary = _validate_outcomes(
        read_json(outcomes_path),
        expected_specs,
        out_dir,
        manifest["execution"],
    )
    derived_exit = expected_exit_code(summaries)
    if isinstance(exit_code, bool) or not isinstance(exit_code, int) or exit_code != derived_exit:
        raise ContractError(
            f"cargo-mutants exit code mismatch: expected {derived_exit}, got {exit_code!r}"
        )
    record: dict[str, Any] = {
        "config_sha256": manifest["config"]["sha256"],
        "counts": counts,
        "exit_code": exit_code,
        "execution": {
            "cargo_binary": cargo_binary,
            "contract_sha256": _digest_value(manifest["execution"]),
        },
        "files": {
            "lock_json_sha256": _digest_file(lock_path),
            "mutants_json_sha256": _digest_file(mutants_path),
            "outcomes_json_sha256": _digest_file(outcomes_path),
            "referenced_artifacts": referenced_artifacts,
        },
        "manifest_sha256": manifest["manifest_sha256"],
        "population_sha256": manifest["population"]["sha256"],
        "revision": manifest["revision"],
        "schema": SCHEMA_SHARD,
        "shard": {
            "algorithm": SHARD_ALGORITHM,
            "count": SHARD_COUNT,
            "index": shard_index,
        },
        "slice": {
            "count": len(observed_ids),
            "mutant_ids": observed_ids,
            "sha256": _digest_value(observed_specs),
        },
        "source_snapshot_sha256": manifest["source_snapshot"]["sha256"],
        "tool": _tool_provenance(),
    }
    record["record_sha256"] = _digest_value(_shard_payload(record))
    return record


def _validate_record_header(value: Any) -> dict[str, Any]:
    record = _expect_dict(value, "shard.json")
    _expect_exact_keys(
        record,
        {
            "config_sha256",
            "counts",
            "exit_code",
            "execution",
            "files",
            "manifest_sha256",
            "population_sha256",
            "record_sha256",
            "revision",
            "schema",
            "shard",
            "slice",
            "source_snapshot_sha256",
            "tool",
        },
        "shard.json",
    )
    if record["schema"] != SCHEMA_SHARD:
        raise ContractError("shard record schema mismatch")
    if record["record_sha256"] != _digest_value(_shard_payload(record)):
        raise ContractError("shard record digest mismatch")
    execution = _expect_dict(record["execution"], "shard.json.execution")
    _expect_exact_keys(
        execution,
        {"cargo_binary", "contract_sha256"},
        "shard.json.execution",
    )
    cargo_binary = execution["cargo_binary"]
    if (
        not isinstance(cargo_binary, str)
        or not PurePosixPath(cargo_binary).is_absolute()
        or not cargo_binary.endswith(f"/{CARGO_BINARY_SUFFIX}")
        or not isinstance(execution["contract_sha256"], str)
        or not HEX64.fullmatch(execution["contract_sha256"])
    ):
        raise ContractError("shard command identity is invalid")
    files = _expect_dict(record["files"], "shard.json.files")
    _expect_exact_keys(
        files,
        {
            "lock_json_sha256",
            "mutants_json_sha256",
            "outcomes_json_sha256",
            "referenced_artifacts",
        },
        "shard.json.files",
    )
    for field in ("lock_json_sha256", "mutants_json_sha256", "outcomes_json_sha256"):
        if not isinstance(files[field], str) or not HEX64.fullmatch(files[field]):
            raise ContractError(f"shard file digest is invalid: {field}")
    referenced = _expect_list(files["referenced_artifacts"], "shard referenced artifacts")
    seen_paths: set[str] = set()
    for index, raw_artifact in enumerate(referenced):
        label = f"shard referenced artifacts[{index}]"
        artifact = _expect_dict(raw_artifact, label)
        _expect_exact_keys(artifact, {"path", "sha256"}, label)
        relative = _safe_relative(artifact["path"], "referenced artifact")
        if (
            not relative.parts
            or relative.parts[0] not in {"diff", "log"}
            or not isinstance(artifact["sha256"], str)
            or not HEX64.fullmatch(artifact["sha256"])
            or relative.as_posix() in seen_paths
        ):
            raise ContractError(f"{label} is invalid")
        seen_paths.add(relative.as_posix())
    if not isinstance(record["source_snapshot_sha256"], str) or not HEX64.fullmatch(
        record["source_snapshot_sha256"]
    ):
        raise ContractError("shard source snapshot identity is invalid")
    shard = _expect_dict(record["shard"], "shard.json.shard")
    _expect_exact_keys(shard, {"algorithm", "count", "index"}, "shard.json.shard")
    if shard["algorithm"] != SHARD_ALGORITHM or shard["count"] != SHARD_COUNT:
        raise ContractError("shard record sharding mismatch")
    if isinstance(shard["index"], bool) or not isinstance(shard["index"], int):
        raise ContractError("shard record index is invalid")
    return record


def _aggregate_payload(report: dict[str, Any]) -> dict[str, Any]:
    payload = dict(report)
    payload.pop("aggregate_sha256", None)
    return payload


def aggregate(
    manifest_value: Any,
    *,
    repo_root: Path,
    config_path: Path,
    shards_root: Path,
    observed_revision: str,
) -> dict[str, Any]:
    manifest = validate_manifest(manifest_value)
    _validate_checkout(
        manifest,
        repo_root=repo_root,
        config_path=config_path,
        observed_revision=observed_revision,
    )
    shards_root = shards_root.resolve()
    if not shards_root.is_dir() or shards_root.is_symlink():
        raise ContractError("shards root is missing or unsafe")
    expected_names = {f"mutation-shard-{index}" for index in range(SHARD_COUNT)}
    entries = list(shards_root.iterdir())
    actual_names = {entry.name for entry in entries}
    foreign = sorted(actual_names - expected_names)
    missing_names = sorted(expected_names - actual_names)
    if foreign:
        raise ContractError(f"foreign shard artifacts: {foreign}")
    if missing_names:
        raise ContractError(f"missing shard artifacts: {missing_names}")
    if len(entries) != SHARD_COUNT or any(
        not entry.is_dir() or entry.is_symlink() for entry in entries
    ):
        raise ContractError("shard artifacts must be exactly 32 regular directories")
    headers: dict[int, tuple[Path, dict[str, Any]]] = {}
    for directory in sorted(entries, key=lambda entry: entry.name):
        record = _validate_record_header(read_json(directory / "shard.json"))
        index = record["shard"]["index"]
        if index in headers:
            raise ContractError(f"duplicate shard index: {index}")
        headers[index] = (directory, record)
    missing_indices = sorted(set(range(SHARD_COUNT)) - set(headers))
    if missing_indices:
        raise ContractError(f"missing shard indices: {missing_indices}")
    totals = {field: 0 for field in COUNT_FIELDS.values()}
    all_ids: list[str] = []
    shard_digests: list[str] = []
    for index in range(SHARD_COUNT):
        directory, stored = headers[index]
        if directory.name != f"mutation-shard-{index}":
            raise ContractError(
                f"shard directory/index mismatch: {directory.name!r} declares {index}"
            )
        if stored["manifest_sha256"] != manifest["manifest_sha256"]:
            raise ContractError(f"manifest provenance mismatch in shard {index}")
        if stored["population_sha256"] != manifest["population"]["sha256"]:
            raise ContractError(f"population provenance mismatch in shard {index}")
        if stored["revision"] != manifest["revision"]:
            raise ContractError(f"revision provenance mismatch in shard {index}")
        if stored["config_sha256"] != manifest["config"]["sha256"]:
            raise ContractError(f"config provenance mismatch in shard {index}")
        if stored["source_snapshot_sha256"] != manifest["source_snapshot"]["sha256"]:
            raise ContractError(f"source snapshot provenance mismatch in shard {index}")
        if stored["execution"]["contract_sha256"] != _digest_value(manifest["execution"]):
            raise ContractError(f"execution provenance mismatch in shard {index}")
        if stored["tool"] != _tool_provenance():
            raise ContractError(f"tool provenance mismatch in shard {index}")
        regenerated = validate_and_record_shard(
            manifest,
            repo_root=repo_root,
            config_path=config_path,
            output_parent=directory,
            shard_index=index,
            observed_revision=observed_revision,
            tool_version_output=TOOL_VERSION_OUTPUT,
            exit_code=stored["exit_code"],
        )
        if stored != regenerated:
            raise ContractError(f"shard record does not bind exact artifact bytes: {index}")
        for field in totals:
            totals[field] += stored["counts"][field]
        all_ids.extend(stored["slice"]["mutant_ids"])
        shard_digests.append(stored["record_sha256"])
    expected_ids = [entry["id"] for entry in manifest["population"]["mutants"]]
    if len(all_ids) != len(set(all_ids)):
        raise ContractError("overlap/duplicate mutant IDs across shards")
    if set(all_ids) != set(expected_ids) or len(all_ids) != len(expected_ids):
        raise ContractError("aggregate does not exactly cover the discovered population")
    report: dict[str, Any] = {
        "complete": True,
        "counts": totals,
        "execution_sha256": _digest_value(manifest["execution"]),
        "manifest_sha256": manifest["manifest_sha256"],
        "population": {
            "count": manifest["population"]["count"],
            "sha256": manifest["population"]["sha256"],
        },
        "quality_policy": "report-only",
        "revision": manifest["revision"],
        "schema": SCHEMA_AGGREGATE,
        "shards": {
            "algorithm": SHARD_ALGORITHM,
            "count": SHARD_COUNT,
            "record_sha256": shard_digests,
        },
        "source_snapshot_sha256": manifest["source_snapshot"]["sha256"],
        "tool": _tool_provenance(),
    }
    report["aggregate_sha256"] = _digest_value(_aggregate_payload(report))
    return report


def _manifest_command(arguments: argparse.Namespace) -> None:
    verify_tool_archive(Path(arguments.tool_archive), arguments.tool_version_output)
    population = _expect_list(read_json(Path(arguments.population_json)), "population listing")
    repo_root = Path(arguments.repo_root)
    manifest = build_manifest(
        population,
        repo_root=repo_root,
        config_path=Path(arguments.config),
        package_versions=_cargo_package_versions(repo_root, population),
        repository=arguments.repository,
        revision=arguments.revision,
        run_id=arguments.run_id,
        run_attempt=arguments.run_attempt,
    )
    write_json(Path(arguments.output), manifest)


def _record_command(arguments: argparse.Namespace) -> None:
    verify_tool_archive(Path(arguments.tool_archive), arguments.tool_version_output)
    manifest = read_json(Path(arguments.manifest))
    record = validate_and_record_shard(
        manifest,
        repo_root=Path(arguments.repo_root),
        config_path=Path(arguments.config),
        output_parent=Path(arguments.output_parent),
        shard_index=arguments.shard_index,
        observed_revision=arguments.observed_revision,
        tool_version_output=arguments.tool_version_output,
        exit_code=arguments.exit_code,
    )
    write_json(Path(arguments.output_parent) / "shard.json", record)


def _aggregate_command(arguments: argparse.Namespace) -> None:
    manifest = read_json(Path(arguments.manifest))
    report = aggregate(
        manifest,
        repo_root=Path(arguments.repo_root),
        config_path=Path(arguments.config),
        shards_root=Path(arguments.shards_root),
        observed_revision=arguments.observed_revision,
    )
    write_json(Path(arguments.output), report)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Проверяет полноту scheduled mutation run. PR1 — report-only: "
            "missed/timeout публикуются, но не становятся quality "
            "threshold."
        )
    )
    commands = parser.add_subparsers(dest="command", required=True)

    manifest = commands.add_parser(
        "manifest",
        help="связать exact discovered population с revision/config/tool provenance",
    )
    manifest.add_argument("--population-json", required=True)
    manifest.add_argument("--repo-root", required=True)
    manifest.add_argument("--config", required=True)
    manifest.add_argument("--repository", required=True)
    manifest.add_argument("--revision", required=True)
    manifest.add_argument("--run-id", required=True, type=int)
    manifest.add_argument("--run-attempt", required=True, type=int)
    manifest.add_argument("--tool-archive", required=True)
    manifest.add_argument("--tool-version-output", required=True)
    manifest.add_argument("--output", required=True)
    manifest.set_defaults(handler=_manifest_command)

    record = commands.add_parser(
        "record-shard",
        help=(
            "fail-closed проверить один complete round-robin shard "
            "и его baseline"
        ),
    )
    record.add_argument("--manifest", required=True)
    record.add_argument("--repo-root", required=True)
    record.add_argument("--config", required=True)
    record.add_argument("--output-parent", required=True)
    record.add_argument("--shard-index", required=True, type=int)
    record.add_argument("--observed-revision", required=True)
    record.add_argument("--tool-archive", required=True)
    record.add_argument("--tool-version-output", required=True)
    record.add_argument("--exit-code", required=True, type=int)
    record.set_defaults(handler=_record_command)

    aggregate_parser = commands.add_parser(
        "aggregate",
        help="доказать exact non-overlapping coverage всех 32 shards",
    )
    aggregate_parser.add_argument("--manifest", required=True)
    aggregate_parser.add_argument("--repo-root", required=True)
    aggregate_parser.add_argument("--config", required=True)
    aggregate_parser.add_argument("--shards-root", required=True)
    aggregate_parser.add_argument("--observed-revision", required=True)
    aggregate_parser.add_argument("--output", required=True)
    aggregate_parser.set_defaults(handler=_aggregate_command)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    arguments = parser.parse_args(argv)
    try:
        arguments.handler(arguments)
    except ContractError as error:
        print(f"mutation truth error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
