"""Fail fast in the quick gate when a native lane inventory pin drifts.

The native containment lane recomputes the exact test inventory at runtime
and refuses any drift, but that verdict arrives only after the disposable
worker rebuilds every sealed archive. A stale literal therefore costs a full
native run before it is visible. This contract recomputes each lane inventory
the same way the native gate does and fails in the quick gate instead.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO))

import gate  # noqa: E402
import native_gate  # noqa: E402


class NativeGateInventoryPinTests(unittest.TestCase):
    def test_every_native_lane_pin_matches_its_exact_runtime_suite(self) -> None:
        for mode, (test_cases, pinned_inventory) in native_gate._MODES.items():
            with self.subTest(mode=mode):
                suite = unittest.TestSuite(
                    unittest.defaultTestLoader.loadTestsFromTestCase(test_case)
                    for test_case in test_cases
                )
                self.assertEqual(
                    gate.test_inventory_sha256_v1(suite),
                    pinned_inventory,
                    f"native lane {mode!r} inventory pin drifted from the "
                    "exact runtime suite; recompute the pin from the loaded "
                    "test ids instead of editing it by hand",
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
