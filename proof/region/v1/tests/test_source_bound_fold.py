#!/usr/bin/env python3
"""The public fold refuses from outside, or its refusals are decoration.

`source_bound_identity_v2` validates its kind, its arity and every coordinate.
Both callers in the tree validate before they call, so none of that is
reachable by accident — and unreachable by accident is how a guard rots into
decoration: removing the whole block leaves the suite green.

These tests are the third caller the module does not have yet.  They are here
rather than in `test_region_proof_protocol.py` because that module carries
domain separators with delicate bytes, and a mechanical edit to it once
introduced corruption that only the parser noticed.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import region_proof_protocol as protocol  # noqa: E402


def _coordinates() -> tuple[bytes, ...]:
    """One distinct digest per source-bound coordinate, in declaration order.

    Numbered from one: the all-zero digest is refused as noncanonical, which
    is a separate rule and not the one these tests are about.
    """

    return tuple(
        bytes([index + 1]) * 32
        for index in range(len(protocol.source_bound_coordinates_v2()))
    )


class SourceBoundFoldGuardTests(unittest.TestCase):
    def test_a_foreign_kind_is_an_unknown_release(self) -> None:
        with self.assertRaises(protocol.ProtocolErrorV1) as caught:
            protocol.source_bound_identity_v2(1, _coordinates())
        self.assertEqual(
            caught.exception.reason, protocol.ProtocolReasonV1.UNKNOWN_RELEASE
        )

    def test_a_wrong_sized_coordinate_set_is_an_invalid_manifest(self) -> None:
        exact = _coordinates()
        for hostile in (exact[:-1], exact + (b"\x00" * 32,), list(exact)):
            with self.subTest(kind=type(hostile).__name__, count=len(hostile)):
                with self.assertRaises(protocol.ProtocolErrorV1) as caught:
                    protocol.source_bound_identity_v2(
                        protocol.ComparatorKindV1.ARB, hostile
                    )
                self.assertEqual(
                    caught.exception.reason,
                    protocol.ProtocolReasonV1.INVALID_MANIFEST,
                )

    def test_a_coordinate_that_is_not_a_digest_is_refused(self) -> None:
        exact = _coordinates()
        for index, bad in ((0, b""), (3, b"\x00" * 31), (7, "x" * 32)):
            with self.subTest(index=index):
                hostile = exact[:index] + (bad,) + exact[index + 1 :]
                with self.assertRaises(protocol.ProtocolErrorV1):
                    protocol.source_bound_identity_v2(
                        protocol.ComparatorKindV1.ARB, hostile
                    )

    def test_the_exact_shape_still_folds(self) -> None:
        # Anti-vacuity: a fold that refused everything would satisfy the three
        # tests above and be worthless.  It must also separate the kinds — the
        # kind is the first byte of the preimage for exactly that reason.
        arb = protocol.source_bound_identity_v2(
            protocol.ComparatorKindV1.ARB, _coordinates()
        )
        mpfi = protocol.source_bound_identity_v2(
            protocol.ComparatorKindV1.MPFI, _coordinates()
        )
        self.assertIs(type(arb), bytes)
        self.assertEqual(len(arb), 32)
        self.assertNotEqual(arb, mpfi)


if __name__ == "__main__":
    unittest.main(verbosity=2)
