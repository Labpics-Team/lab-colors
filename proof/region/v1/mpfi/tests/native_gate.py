#!/usr/bin/env python3
"""Run the MPFI source-bound receipt integrations without skip allowances."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))
from proof.region.v1.arb.tests import gate
from proof.region.v1.mpfi.tests.full_domain_receipt import (
    NativeMpfiSourceBoundFullDomainReceiptIntegrationTests,
)
from proof.region.v1.mpfi.tests.test_receipt import (
    NativeMpfiSourceBoundReceiptIntegrationTests,
)


_MODES = {
    "receipt": (
        NativeMpfiSourceBoundReceiptIntegrationTests,
        "940e82f266c5b3d07962bcbb792c3e34d47f75b7f410c41896d062fa5b2f0f05",
    ),
    # The full 2^24 RUN is a dispatch-only long lane; the fast gates never
    # execute it, so its inventory pin guards the gate's own invocation.
    "full-domain-receipt": (
        NativeMpfiSourceBoundFullDomainReceiptIntegrationTests,
        "894caa5ee033b123b87538f4a121b15dd735785818ca95387f662b79dc5d4a3f",
    ),
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in _MODES:
        print(
            "usage: native_gate.py {receipt|full-domain-receipt}",
            file=sys.stderr,
        )
        return 64
    test_case, inventory = _MODES[sys.argv[1]]
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(test_case)
    return gate.run_exact_suite_v1(
        suite,
        expected_inventory_sha256=inventory,
        expected_skips=frozenset(),
    )


if __name__ == "__main__":
    raise SystemExit(main())
