#!/usr/bin/env python3
"""Require the exact evaluator runtime suite with no vacuous outcomes."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))

from proof.region.v1.arb.tests import gate  # noqa: E402
from proof.region.v1.arb.tests.test_evaluator_source import (  # noqa: E402
    ExactBoundaryRuntimeTests,
)


EXPECTED_RUNTIME_INVENTORY_SHA256 = (
    "bc169a72a472a67e206250f755006085fa204646ee76fe7c6e8752db072aa73a"
)


def main() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        ExactBoundaryRuntimeTests
    )
    return gate.run_exact_suite_v1(
        suite,
        expected_inventory_sha256=EXPECTED_RUNTIME_INVENTORY_SHA256,
        expected_skips=frozenset(),
    )


if __name__ == "__main__":
    raise SystemExit(main())
