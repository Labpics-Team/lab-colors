#!/usr/bin/env python3
"""Run the complete fast Arb contract with an exact skip manifest."""

from __future__ import annotations

import hashlib
import sys
import unittest
from collections.abc import Iterator
from pathlib import Path


TEST_DIRECTORY = Path(__file__).resolve().parent
REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))
EXPECTED_TEST_INVENTORY_SHA256 = (
    "ab21c48b5e3347c55a3c69ff2c76dee93f8e23f28eab54220729252fd2f6f1fc"
)
_EVALUATOR_REASON = "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary"
EXPECTED_SKIPS = frozenset(
    {
        (
            f"test_evaluator_source.ExactBoundaryRuntimeTests.{name}",
            _EVALUATOR_REASON,
        )
        for name in (
            "test_black_exact_zero_runs_through_job_parser_formula_and_closed_driver",
            "test_cli_requires_one_nonzero_lowercase_manifest_identity",
            "test_frozen_seam_cube_resolves_one_inside_and_511_outside",
            "test_global_pregrant_is_never_transferred_between_points",
            "test_multisegment_exact_trace_selects_first_canonical_branch",
            "test_resource_witness_accounts_for_work_consumed_on_earlier_rungs",
            "test_spd_admission_is_exact_across_the_full_binary64_exponent_range",
            "test_subminimum_precision_is_unresolved_and_a_later_valid_rung_recovers",
            "test_zero_grant_emits_canonical_resource_witnesses",
        )
    }
    | {
        (
            "test_executor.NativeLinuxIntegrationTests."
            "test_real_kernel_success_output_timeout_signal_oom_and_cleanup",
            "requires Linux and an explicit delegated cgroup v2 parent",
        ),
        (
            "test_pipeline.NativeBuildIntegrationTests."
            "test_real_two_builds_and_ephemeral_evaluator_runtime_tests",
            "requires Linux, Docker, the native binary path, and all three exact source archives",
        ),
        (
            "test_pipeline.NativePipelineIntegrationTests."
            "test_prepared_two_build_binary_runs_through_controlled_pipeline",
            "requires Linux and an explicit delegated cgroup v2 parent",
        ),
    }
)


def _iter_tests_v1(suite: unittest.TestSuite) -> Iterator[unittest.TestCase]:
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from _iter_tests_v1(item)
        elif isinstance(item, unittest.TestCase):
            yield item
        else:
            raise TypeError("suite contains a non-test object")


def _inventory_preimage_v1(test_ids: tuple[str, ...]) -> bytes:
    return b"".join(test_id.encode("utf-8") + b"\n" for test_id in sorted(test_ids))


def test_inventory_sha256_v1(suite: unittest.TestSuite) -> str:
    test_ids = tuple(test.id() for test in _iter_tests_v1(suite))
    return hashlib.sha256(_inventory_preimage_v1(test_ids)).hexdigest()


def run_exact_suite_v1(
    suite: unittest.TestSuite,
    *,
    expected_inventory_sha256: str,
    expected_skips: frozenset[tuple[str, str]],
    verbosity: int = 2,
) -> int:
    tests = tuple(_iter_tests_v1(suite))
    test_ids = tuple(test.id() for test in tests)
    actual_inventory_sha256 = hashlib.sha256(
        _inventory_preimage_v1(test_ids)
    ).hexdigest()
    if (
        not tests
        or len(set(test_ids)) != len(test_ids)
        or actual_inventory_sha256 != expected_inventory_sha256
    ):
        print(
            "Arb test inventory drift: "
            f"count={len(tests)} sha256={actual_inventory_sha256} "
            f"expected={expected_inventory_sha256}",
            file=sys.stderr,
        )
        return 1
    result = unittest.TextTestRunner(verbosity=verbosity).run(suite)
    actual_skips = frozenset((test.id(), reason) for test, reason in result.skipped)
    if actual_skips != expected_skips:
        print(f"unexpected skips: {sorted(actual_skips - expected_skips)!r}", file=sys.stderr)
        print(f"missing skips: {sorted(expected_skips - actual_skips)!r}", file=sys.stderr)
        return 1
    if (
        result.failures
        or result.errors
        or result.expectedFailures
        or result.unexpectedSuccesses
        or not result.wasSuccessful()
    ):
        print(
            "proof suite contains failures, errors, expected failures, or "
            "unexpected successes",
            file=sys.stderr,
        )
        return 1
    print(
        f"Arb fast gate: {len(tests)} tests, "
        f"inventory {actual_inventory_sha256}, "
        f"exact {len(actual_skips)}-skip manifest"
    )
    return 0


def main() -> int:
    suite = unittest.defaultTestLoader.discover(
        str(TEST_DIRECTORY),
        pattern="test_*.py",
    )
    return run_exact_suite_v1(
        suite,
        expected_inventory_sha256=EXPECTED_TEST_INVENTORY_SHA256,
        expected_skips=EXPECTED_SKIPS,
    )


if __name__ == "__main__":
    raise SystemExit(main())
