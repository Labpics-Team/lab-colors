#!/usr/bin/env python3
"""Запускает обязательный MPFI source-contract gate с anti-vacuum inventory."""

from __future__ import annotations

import hashlib
import sys
import unittest
from collections.abc import Iterator
from pathlib import Path


TEST_DIRECTORY = Path(__file__).resolve().parent
EXPECTED_TEST_COUNT = 27
EXPECTED_TEST_INVENTORY_SHA256 = "d7f421024196c07c5b306e2967c9a173a1dc0e7bd98b4a6008c475c9dea0ba63"
_RUNTIME_REASON = "set LABCOLORS_MPFI_EVALUATOR to the controlled C17 binary"
EXPECTED_SKIPS = frozenset(
    {
        (
            "test_evaluator_source.RuntimeTests.test_frozen_fixture_produces_a_canonical_transcript",
            _RUNTIME_REASON,
        ),
        (
            "test_evaluator_source.RuntimeTests.test_black_exact_zero_emits_the_canonical_trace_witness",
            _RUNTIME_REASON,
        ),
        (
            "test_evaluator_source.RuntimeTests.test_input_limit_is_enforced_before_wire_parse",
            _RUNTIME_REASON,
        ),
        (
            "test_receipt.NativeMpfiSourceBoundReceiptIntegrationTests."
            "test_real_build_run_and_seal_are_one_source_bound_controller_execution",
            "requires Linux, Docker, a delegated cgroup, and all three exact MPFI source archives",
        ),
    }
)


def _iter_tests(suite: unittest.TestSuite) -> Iterator[unittest.TestCase]:
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from _iter_tests(item)
        elif isinstance(item, unittest.TestCase):
            yield item
        else:
            raise TypeError("suite contains a non-test object")


def _inventory_digest(test_ids: tuple[str, ...]) -> str:
    preimage = b"".join(test_id.encode("utf-8") + b"\n" for test_id in sorted(test_ids))
    return hashlib.sha256(preimage).hexdigest()


def run_gate() -> int:
    suite = unittest.defaultTestLoader.discover(
        str(TEST_DIRECTORY),
        pattern="test_*.py",
    )
    tests = tuple(_iter_tests(suite))
    test_ids = tuple(test.id() for test in tests)
    actual_digest = _inventory_digest(test_ids)
    if (
        not tests
        or len(test_ids) != EXPECTED_TEST_COUNT
        or len(set(test_ids)) != len(test_ids)
        or actual_digest != EXPECTED_TEST_INVENTORY_SHA256
    ):
        print(
            "MPFI source gate inventory drift: "
            f"count={len(test_ids)} sha256={actual_digest} "
            f"expected_count={EXPECTED_TEST_COUNT} "
            f"expected_sha256={EXPECTED_TEST_INVENTORY_SHA256}",
            file=sys.stderr,
        )
        return 1
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    actual_skips = frozenset((test.id(), reason) for test, reason in result.skipped)
    if actual_skips != EXPECTED_SKIPS:
        print(f"unexpected skips: {sorted(actual_skips - EXPECTED_SKIPS)!r}", file=sys.stderr)
        print(f"missing skips: {sorted(EXPECTED_SKIPS - actual_skips)!r}", file=sys.stderr)
        return 1
    return int(bool(result.failures or result.errors))


if __name__ == "__main__":
    raise SystemExit(run_gate())
