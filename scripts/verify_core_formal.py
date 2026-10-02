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
FRAGMENT_SCHEMA = 1


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
        if (check["category"] == "cover" and check["status"] == "Unsatisfiable"
                and check["description"] in CONTRACTS[harness][1]):
            continue
        if not (check["status"] == "Failure" and check["category"] == "assertion"
                and check["description"] in allowed_assertions):
            raise ValueError("mutant failed outside its declared semantic properties")


def run_kani(output: Path, *, harness: str | None = None) -> tuple[dict, int]:
    output.unlink(missing_ok=True)
    command = ["cargo", "kani", "-p", "labcolors-core", "--lib", "--output-format", "terse",
               "-Z", "unstable-options", "--harness-timeout", "300s", "--export-json", str(output)]
    if harness is None:
        # Kani defaults to a one-thread verifier even with many independent harnesses.
        # The public GitHub runner provides four vCPUs; keep one canonical report while
        # letting Kani schedule the 41 proofs across those cores.
        command.append("--jobs=4")
    else:
        # Concrete playback is intentionally single-harness and Kani rejects it
        # together with multi-threaded --jobs.
        command.extend(["--harness", harness, "--exact", "-Z", "concrete-playback", "--concrete-playback", "print"])
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
        ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256",
    ])
    return files


def checkout_snapshot() -> tuple[dict[str, str], str, bool]:
    files = formal_source_files()
    identities = {str(path.relative_to(ROOT)): digest(path) for path in files}
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    clean = not subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True
    )
    subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        cwd=ROOT, stdout=subprocess.DEVNULL, check=True,
    )
    return identities, commit, clean


def assert_snapshot(identities: dict[str, str], commit: str) -> bool:
    current = {str(path.relative_to(ROOT)): digest(path) for path in formal_source_files()}
    if identities != current:
        raise ValueError("source or dependency identity changed during verification")
    actual_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if commit != actual_commit:
        raise ValueError("checkout changed during verification")
    return not subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True
    )


def fragment_base(identities: dict[str, str], commit: str, clean: bool) -> dict:
    return {
        "fragment_schema": FRAGMENT_SCHEMA,
        "kani_version": KANI_VERSION,
        "source_commit": commit if clean else None,
        "checkout_commit": commit,
        "working_tree_clean": clean,
        "verified_source_sha256": identities,
    }


def run_positive_phase(output: Path) -> None:
    identities, commit, initially_clean = checkout_snapshot()
    report, code = run_kani(output / "positive.json")
    if code != 0:
        raise ValueError("positive verification returned failure")
    validate_report(report)
    finally_clean = assert_snapshot(identities, commit)
    fragment = fragment_base(identities, commit, initially_clean and finally_clean)
    fragment.update({
        "kind": "positive",
        "positive_sha256": digest(output / "positive.json"),
        "harnesses": sorted(CONTRACTS),
    })
    (output / "fragment-positive.json").write_text(
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


def assemble_fragments(output: Path, input_dir: Path) -> None:
    fragments = [json.loads(path.read_text(encoding="utf-8"))
                 for path in sorted(input_dir.glob("fragment-*.json"))]
    positives = [item for item in fragments if item.get("kind") == "positive"]
    mutants = [item for item in fragments if item.get("kind") == "mutants"]
    if len(positives) != 1 or not mutants:
        raise ValueError("missing or duplicate formal proof fragments")
    positive = positives[0]
    if positive.get("harnesses") != sorted(CONTRACTS):
        raise ValueError("positive fragment lost a required harness")
    if digest(input_dir / "positive.json") != positive.get("positive_sha256"):
        raise ValueError("positive evidence digest mismatch")

    common = ("fragment_schema", "kani_version", "checkout_commit", "source_commit",
              "working_tree_clean", "verified_source_sha256")
    for fragment in fragments:
        if fragment.get("fragment_schema") != FRAGMENT_SCHEMA or fragment.get("kani_version") != KANI_VERSION:
            raise ValueError("foreign formal fragment")
        if any(fragment.get(key) != positive.get(key) for key in common):
            raise ValueError("formal fragments describe different source states")

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
        "source_commit": positive["source_commit"],
        "checkout_commit": positive["checkout_commit"],
        "working_tree_clean": positive["working_tree_clean"],
        "verified_source_sha256": positive["verified_source_sha256"],
        "positive_sha256": positive["positive_sha256"],
        "negative_sha256": negative,
        "harnesses": sorted(CONTRACTS),
        "semantic_mutant_rejected": True,
    }
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
        run_positive_phase(output)
    elif args.phase == "mutants":
        run_mutant_phase(output, args.shard_index, args.shard_count)
    elif args.phase == "assemble":
        if args.input_dir is None:
            parser.error("--input-dir is required for assemble")
        assemble_fragments(output, args.input_dir.resolve())
    else:
        run_positive_phase(output)
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
