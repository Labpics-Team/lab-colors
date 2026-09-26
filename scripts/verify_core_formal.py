#!/usr/bin/env python3
"""Kani-доказательства ядра: обязательные свойства, достижимость и предметный RED."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
from pathlib import Path
import subprocess

from formal_contracts import CONTRACTS, MUTANTS, SOURCES

ROOT = Path(__file__).resolve().parents[1]
KANI_VERSION = "0.68.0"

CORE = ROOT / "crates/labcolors-core/src"


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
        if result["status"] != "Success":
            raise ValueError("unsuccessful harness")
        assertions, covers = CONTRACTS[result["harness_id"]]
        seen_assertions, seen_covers = set(), set()
        for check in result["checks"]:
            category = check["category"]
            wanted = "Satisfied" if category == "cover" else "Success"
            if check["status"] != wanted:
                # Недостижимый compiler-generated panic не является дефектом.
                # Именованное обязательство и любой cover обязаны быть достижимы.
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
        if not (check["status"] == "Failure" and check["category"] == "assertion"
                and check["description"] in allowed_assertions):
            raise ValueError("mutant failed outside its declared semantic properties")


def run_kani(output: Path, *, harness: str | None = None) -> tuple[dict, int]:
    output.unlink(missing_ok=True)
    command = ["cargo", "kani", "-p", "labcolors-core", "--lib", "--output-format", "terse",
               "-Z", "unstable-options", "--harness-timeout", "300s", "--export-json", str(output)]
    if harness is not None:
        command.extend(["--harness", harness, "--exact", "-Z", "concrete-playback", "--concrete-playback", "print"])
    # Обычный код и cfg(kani) исполняются без stubs и отключения safety/reach checks.
    log = output.with_suffix(".log")
    with log.open("w", encoding="utf-8") as stream:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            process.wait(timeout=1200)
        finally:
            # Kani может завершить CBMC по внутреннему timeout, оставив SMT-
            # процесс живым. Группа очищается и после штатного выхода обёртки.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    print(log.read_text(encoding="utf-8"), end="", flush=True)
    return json.loads(output.read_text(encoding="utf-8")), process.returncode


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kani-version", action="store_true")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    if args.kani_version:
        print(KANI_VERSION)
        return
    if args.output_dir is None:
        parser.error("--output-dir is required")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "receipt.json").unlink(missing_ok=True)
    lock = ROOT / "Cargo.lock"
    source_files = [ROOT / "Cargo.toml", lock, ROOT / "crates/labcolors-core/Cargo.toml",
                    ROOT / "crates/labcolors-core/build.rs", CORE / "lib.rs", CORE / "program_wire.rs",
                    Path(__file__), ROOT / "scripts/formal_contracts.py"]
    for source in SOURCES:
        source_files.extend([CORE / source, CORE / source.removesuffix(".rs") / "proofs.rs"])
    identities = {str(p.relative_to(ROOT)): digest(p) for p in source_files}
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    status = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True)
    subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
                   cwd=ROOT, stdout=subprocess.DEVNULL, check=True)
    report, code = run_kani(output / "positive.json")
    if code != 0:
        raise ValueError("positive verification returned failure")
    validate_report(report)
    negative_results = {}
    for mutant in MUTANTS:
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
    if identities != {str(p.relative_to(ROOT)): digest(p) for p in source_files}:
        raise ValueError("source or dependency identity changed during verification")
    if commit != subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip():
        raise ValueError("checkout changed during verification")
    final_status = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True)
    clean = not status and not final_status
    receipt = {
        "scope": "Core Linux x86_64; per-harness domains documented in docs/how-to/formal-core.md",
        "schema": 1, "kani_version": KANI_VERSION,
        "source_commit": commit if clean else None,
        "checkout_commit": commit,
        "working_tree_clean": clean,
        "verified_source_sha256": identities,
        "positive_sha256": digest(output / "positive.json"),
        "negative_sha256": negative_results,
        "harnesses": sorted(CONTRACTS),
        "semantic_mutant_rejected": True,
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Core formal gate: {len(CONTRACTS)} proofs, all required witnesses reachable, {len(MUTANTS)} semantic mutants rejected")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Core formal gate FAILED: {error}") from error
