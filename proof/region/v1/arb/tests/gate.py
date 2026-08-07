#!/usr/bin/env python3
"""Запускает обязательные быстрые proof-контракты с точным manifest skips."""

from __future__ import annotations

import hashlib
import sys
import unittest
from collections.abc import Iterator
from pathlib import Path


TEST_DIRECTORY = Path(__file__).resolve().parent
SHARED_TEST_DIRECTORY = TEST_DIRECTORY.parents[1] / "tests"
REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))
SHARED_FAST_TEST_PATTERNS_V1 = (
    "test_executor.py",
    "test_mpfi_input.py",
)
EXPECTED_TEST_INVENTORY_SHA256 = (
    "cc497572c7613c7fad2c765ba0e9901795c54900d30bd202fe06cbff13f8415d"
)
_EVALUATOR_REASON = "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary"
EXPECTED_SKIPS = frozenset(
    {
        (
            f"test_evaluator_source.ExactBoundaryRuntimeTests.{name}",
            _EVALUATOR_REASON,
        )
        for name in (
            "test_allocation_profile_boundaries_are_enforced_by_the_native_parser",
            "test_black_exact_zero_runs_through_job_parser_formula_and_closed_driver",
            "test_cli_requires_one_nonzero_lowercase_manifest_identity",
            "test_closed_stdout_is_a_versioned_io_exit_not_an_untyped_signal",
            "test_frozen_seam_cube_resolves_one_inside_and_511_outside",
            "test_global_pregrant_is_never_transferred_between_points",
            "test_job_transport_limit_precedes_wire_parsing",
            "test_multisegment_exact_trace_selects_first_canonical_branch",
            "test_aggregate_transcript_output_limit_is_exact",
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
            "test_receipt.NativeSourceBoundReceiptIntegrationTests."
            "test_real_build_run_and_seal_are_one_source_bound_controller_execution",
            "requires Linux, Docker, a delegated cgroup, and all three exact source archives",
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


def iter_tests_v1(suite: unittest.TestSuite) -> Iterator[unittest.TestCase]:
    """Expose test enumeration without leaking the gate's private helper."""

    return _iter_tests_v1(suite)


def _inventory_preimage_v1(test_ids: tuple[str, ...]) -> bytes:
    return b"".join(test_id.encode("utf-8") + b"\n" for test_id in sorted(test_ids))


def test_inventory_sha256_v1(suite: unittest.TestSuite) -> str:
    test_ids = tuple(test.id() for test in _iter_tests_v1(suite))
    return hashlib.sha256(_inventory_preimage_v1(test_ids)).hexdigest()


def full_suite_v1() -> unittest.TestSuite:
    """Собирает обязательные общие proof-контракты и Arb-only contract."""

    # Явный top_level_dir фиксирует загрузку модулей по простому имени и
    # делает discovery независимой от версии python: на 3.12 неявный обход
    # пространства имён упирается в регулярный пакет arb/ и падает с
    # ImportError "Start directory is not importable".
    return unittest.TestSuite(
        tuple(
            unittest.defaultTestLoader.discover(
                str(SHARED_TEST_DIRECTORY),
                pattern=pattern,
                top_level_dir=str(SHARED_TEST_DIRECTORY),
            )
            for pattern in SHARED_FAST_TEST_PATTERNS_V1
        )
        + (
            unittest.defaultTestLoader.discover(
                str(TEST_DIRECTORY),
                pattern="test_*.py",
                top_level_dir=str(TEST_DIRECTORY),
            ),
        )
    )


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
            "Proof fast gate inventory drift: "
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
        f"Proof fast gate: {len(tests)} tests, "
        f"inventory {actual_inventory_sha256}, "
        f"exact {len(actual_skips)}-skip manifest"
    )
    return 0


def main() -> int:
    suite = full_suite_v1()
    return run_exact_suite_v1(
        suite,
        expected_inventory_sha256=EXPECTED_TEST_INVENTORY_SHA256,
        expected_skips=EXPECTED_SKIPS,
    )


if __name__ == "__main__":
    raise SystemExit(main())
