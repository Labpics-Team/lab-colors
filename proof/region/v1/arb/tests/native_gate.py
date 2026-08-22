#!/usr/bin/env python3
"""Require one exact native integration lane without skips."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))

from proof.region.v1.arb.tests import gate  # noqa: E402
from proof.region.v1.arb.tests.full_domain_receipt import (  # noqa: E402
    NativeSourceBoundFullDomainReceiptIntegrationTests,
)
from proof.region.v1.tests.test_executor import (  # noqa: E402
    NativeLinuxIntegrationTests,
)
from proof.region.v1.arb.tests.test_receipt import (  # noqa: E402
    NativeSourceBoundReceiptIntegrationTests,
)


_MODES = {
    "executor": (
        (NativeLinuxIntegrationTests,),
        "276f45bd831c26288eaa34f1846821a6b8cec3b6d58b9f2c8a6f3136f8ad7869",
    ),
    "receipt": (
        (NativeSourceBoundReceiptIntegrationTests,),
        "d5092e566c23b45f4b81ef850ca8abc8f003fa1a98c15030643660a636b04c6a",
    ),
    # The full 2^24 RUN is a dispatch-only long lane; the fast gates never
    # execute it, but its inventory pin is still recomputed by the quick
    # contract so a stale literal fails fast instead of after a full run.
    "full-domain-receipt": (
        (NativeSourceBoundFullDomainReceiptIntegrationTests,),
        "4696c3f271f070ce67d63446c8ba8b9ce6e85de4cd7a396e43f401a6fff1e3c9",
    ),
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in _MODES:
        print(
            "usage: native_gate.py {executor|receipt|full-domain-receipt}",
            file=sys.stderr,
        )
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
