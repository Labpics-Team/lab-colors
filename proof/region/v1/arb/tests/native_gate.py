#!/usr/bin/env python3
"""Require one exact native integration lane without skips."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))

from proof.region.v1.arb.tests import gate  # noqa: E402
from proof.region.v1.arb.tests.test_executor import (  # noqa: E402
    NativeLinuxIntegrationTests,
)
from proof.region.v1.arb.tests.test_pipeline import (  # noqa: E402
    NativeBuildIntegrationTests,
    NativePipelineIntegrationTests,
)


_MODES = {
    "build": (
        (NativeBuildIntegrationTests,),
        "a6f8057d55a19bee9e924fa3bea2f082455ece0b8a9be5caf022b4a61aa9d15e",
    ),
    "executor": (
        (NativeLinuxIntegrationTests, NativePipelineIntegrationTests),
        "0a7135fc2c259f125aa3cb692ea480550549d3aed5fdb95c47a3ddc999969a4d",
    ),
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in _MODES:
        print("usage: native_gate.py {build|executor}", file=sys.stderr)
        return 64
    test_cases, inventory = _MODES[sys.argv[1]]
    suite = unittest.TestSuite(
        unittest.defaultTestLoader.loadTestsFromTestCase(test_case)
        for test_case in test_cases
    )
    return gate.run_exact_suite_v1(
        suite,
        expected_inventory_sha256=inventory,
        expected_skips=frozenset(),
    )


if __name__ == "__main__":
    raise SystemExit(main())
