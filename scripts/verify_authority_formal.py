#!/usr/bin/env python3
"""Kani-доказательства AUTH: обязательные свойства, достижимость и предметный RED."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
KANI_VERSION = "0.68.0"
PREFIX = "authority::proofs::"
SOURCE = ROOT / "crates/labcolors-core/src/authority.rs"
PROOFS = ROOT / "crates/labcolors-core/src/authority/proofs.rs"
# Это перечень обязательств, не счётчик внутренних проверок конкретного rustc.
CONTRACTS = {
    "auth_permit_iff_exact_current_proof": (
        {"permit must match the full current proof binding"},
        {"valid permit remains reachable", "missing proof", "foreign lane", "stale release",
         "foreign applicability", "foreign provenance", "unobserved renderer"},
    ),
    "auth_admission_is_exact_atomic_and_lane_local": (
        {"admission must obey exact authorization and CAS", "only the addressed lane may change",
         "every refusal must preserve all lanes"},
        {"install", "replace", "duplicate", "refusal"},
    ),
    "auth_require_iff_full_binding": (
        {"require must compare every byte of every identity", "read must address the contract lane"},
        {"exact binding", "empty lane", "release mismatch", "applicability mismatch", "provenance mismatch"},
    ),
    "auth_issue_admit_require_composes": (
        {"fresh owner authorization must commit", "accepted binding must remain usable",
         "duplicate must remain a true no-op"},
        {"complete authorized path"},
    ),
}
MUTANT_HARNESS = PREFIX + "auth_require_iff_full_binding"
MUTANT_ASSERTION = '"require must compare every byte of every identity"'
MUTANT_BEFORE = "        if current.applicability != expected.applicability {"
MUTANT_AFTER = "        if current.applicability.0[0] != expected.applicability.0[0] {"


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
    expected = {PREFIX + name for name in CONTRACTS}
    if len(results) != count or {r["harness_id"] for r in results} != expected:
        raise ValueError("missing, duplicate or unexpected harness")
    for result in results:
        if result["status"] != "Success":
            raise ValueError("unsuccessful harness")
        assertions, covers = CONTRACTS[result["harness_id"].removeprefix(PREFIX)]
        seen_assertions, seen_covers = set(), set()
        for check in result["checks"]:
            category = check["category"]
            wanted = "Satisfied" if category == "cover" else "Success"
            if check["status"] != wanted:
                raise ValueError(f"non-passing property: {check['description']}")
            if category == "cover":
                seen_covers.add(check["description"])
            if category == "assertion":
                seen_assertions.add(check["description"])
        if not {json.dumps(a) for a in assertions} <= seen_assertions:
            raise ValueError("missing semantic assertion")
        if seen_covers != covers:
            raise ValueError("missing or unexpected reachability witness")


def validate_mutant(report: dict, returncode: int) -> None:
    """Ошибка сборки, timeout или посторонний отказ не засчитываются как RED."""
    verification = report["verification_results"]
    results = verification["results"]
    summary = verification["summary"]
    if (returncode != 1 or len(results) != 1 or results[0]["harness_id"] != MUTANT_HARNESS
            or results[0]["status"] != "Failure" or summary["status"] != "completed"
            or summary["total_harnesses"] != 1 or summary["executed"] != 1
            or summary["failed"] != 1 or summary["successful"] != 0):
        raise ValueError("mutant did not complete the required harness")
    failed = [c for c in results[0]["checks"] if c["status"] != ("Satisfied" if c["category"] == "cover" else "Success")]
    if not failed or any(c["status"] != "Failure" or c["category"] != "assertion"
                         or c["description"] != MUTANT_ASSERTION for c in failed):
        raise ValueError("mutant failed outside the protected semantic property")


def run_kani(output: Path, *, mutant: bool = False) -> tuple[dict, int]:
    output.unlink(missing_ok=True)
    command = ["cargo", "kani", "-p", "labcolors-core", "--lib", "--harness",
               MUTANT_HARNESS if mutant else PREFIX, "--output-format", "terse",
               "-Z", "unstable-options", "--export-json", str(output)]
    if mutant:
        command.extend(["--exact", "-Z", "concrete-playback", "--concrete-playback", "print"])
    # Обычный код и cfg(kani) исполняются без stubs и отключения safety/reach checks.
    log = output.with_suffix(".log")
    with log.open("w", encoding="utf-8") as stream:
        process = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                 timeout=1200, check=False)
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
    source_files = [SOURCE, PROOFS, lock, ROOT / "crates/labcolors-core/Cargo.toml", Path(__file__)]
    identities = {str(p.relative_to(ROOT)): digest(p) for p in source_files}
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    status = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True)
    subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
                   cwd=ROOT, stdout=subprocess.DEVNULL, check=True)
    report, code = run_kani(output / "positive.json")
    if code != 0:
        raise ValueError("positive verification returned failure")
    validate_report(report)
    original = SOURCE.read_text(encoding="utf-8")
    if original.count(MUTANT_BEFORE) != 1:
        raise ValueError("mutation anchor drift")
    try:
        SOURCE.write_text(original.replace(MUTANT_BEFORE, MUTANT_AFTER), encoding="utf-8")
        mutant, code = run_kani(output / "negative.json", mutant=True)
        validate_mutant(mutant, code)
    finally:
        SOURCE.write_text(original, encoding="utf-8")
    if identities != {str(p.relative_to(ROOT)): digest(p) for p in source_files}:
        raise ValueError("source or dependency identity changed during verification")
    if commit != subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip():
        raise ValueError("checkout changed during verification")
    receipt = {
        "scope": "AUTH Linux x86_64; current owner proof assumed; symbolic 256-bit identities",
        "schema": 1, "kani_version": KANI_VERSION,
        "source_commit": commit if not status else None,
        "checkout_commit": commit,
        "working_tree_clean": not status,
        "verified_source_sha256": identities,
        "positive_sha256": digest(output / "positive.json"),
        "negative_sha256": digest(output / "negative.json"),
        "harnesses": sorted(PREFIX + name for name in CONTRACTS),
        "semantic_mutant_rejected": True,
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("AUTH formal gate: four proofs, all witnesses reachable, semantic mutant rejected")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"AUTH formal gate FAILED: {error}") from error
