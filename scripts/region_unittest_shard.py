#!/usr/bin/env python3
"""Run one exact deterministic shard of the contextual-region unittest corpus."""

from __future__ import annotations

import argparse
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
TEST_DIR = ROOT / "proof/region/v1/tests"

# Scheduling hints from the 2026-10-02 exact-head normal run. They do not decide
# test membership: every test_*.py module is discovered below, and unknown/new
# modules are still admitted with weight 1.
MEASURED_SECONDS = {
    "test_semantic_folding": 70,
    "test_corpus_shards": 33,
    "test_semantic_replay": 31,
    "test_dual_proof": 31,
    "test_corpus_assembly": 25,
    "test_verification_assembly": 15,
    "test_corpus_lanes": 13,
}


def flatten_suite(suite: unittest.TestSuite) -> list[unittest.TestCase]:
    cases: list[unittest.TestCase] = []
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            cases.extend(flatten_suite(item))
        else:
            cases.append(item)
    return cases

def discover_modules() -> dict[str, list[unittest.TestCase]]:
    modules: dict[str, list[unittest.TestCase]] = {}
    all_ids: list[str] = []
    for path in sorted(TEST_DIR.glob("test_*.py")):
        suite = unittest.defaultTestLoader.discover(str(TEST_DIR), pattern=path.name)
        cases = flatten_suite(suite)
        if not cases:
            raise ValueError(f"empty region test module: {path.name}")
        ids = [case.id() for case in cases]
        if len(ids) != len(set(ids)):
            raise ValueError(f"duplicate test id inside {path.name}")
        modules[path.stem] = cases
        all_ids.extend(ids)

    if len(all_ids) != len(set(all_ids)):
        raise ValueError("duplicate region test id across modules")
    if len(all_ids) < 25:
        raise ValueError(f"region-proof anti-vacuum floor failed: {len(all_ids)} < 25")
    return modules


def partition_modules(names: list[str], count: int) -> tuple[tuple[str, ...], ...]:
    if count < 1:
        raise ValueError("region shard count must be positive")
    if count > len(names):
        raise ValueError("region shard count exceeds module count")

    bins: list[list[str]] = [[] for _ in range(count)]
    loads = [0 for _ in range(count)]
    weighted = sorted(names, key=lambda name: (-MEASURED_SECONDS.get(name, 1), name))
    for name in weighted:
        target = min(range(count), key=lambda index: (loads[index], index))
        bins[target].append(name)
        loads[target] += MEASURED_SECONDS.get(name, 1)

    flattened = [name for bucket in bins for name in bucket]
    if len(flattened) != len(set(flattened)) or set(flattened) != set(names):
        raise ValueError("region shard plan is not an exact partition")
    return tuple(tuple(sorted(bucket)) for bucket in bins)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shard-index", type=int, required=True)
    parser.add_argument("--shard-count", type=int, required=True)
    args = parser.parse_args()

    modules = discover_modules()
    plan = partition_modules(list(modules), args.shard_count)
    if args.shard_index < 0 or args.shard_index >= len(plan):
        raise ValueError("invalid region shard index")

    selected_modules = plan[args.shard_index]
    selected_cases = [
        case
        for module in selected_modules
        for case in modules[module]
    ]
    total = sum(len(cases) for cases in modules.values())
    print(
        f"region-proof shard {args.shard_index + 1}/{args.shard_count}: "
        f"{len(selected_cases)}/{total} tests"
    )

    print("modules=" + ",".join(selected_modules))
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.TestSuite(selected_cases)
    )
    if not result.wasSuccessful():
        raise SystemExit(1)


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        raise SystemExit(f"region-proof shard FAILED: {error}") from error
