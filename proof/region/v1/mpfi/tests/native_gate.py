#!/usr/bin/env python3
"""Run the MPFI source-bound receipt integration without skip allowances."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))
from proof.region.v1.arb.tests import gate
from proof.region.v1.mpfi.tests.test_receipt import (
    NativeMpfiSourceBoundReceiptIntegrationTests,
)


EXPECTED_INVENTORY_SHA256 = (
    "940e82f266c5b3d07962bcbb792c3e34d47f75b7f410c41896d062fa5b2f0f05"
)


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] != "receipt":
        print("usage: native_gate.py receipt", file=sys.stderr)
        return 64
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        NativeMpfiSourceBoundReceiptIntegrationTests,
    )
    return gate.run_exact_suite_v1(
        suite,
        expected_inventory_sha256=EXPECTED_INVENTORY_SHA256,
        expected_skips=frozenset(),
    )


if __name__ == "__main__":
    raise SystemExit(main())
