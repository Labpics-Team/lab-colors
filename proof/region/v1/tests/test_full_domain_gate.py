#!/usr/bin/env python3
"""Hostile contract for the full-domain mint gate.

The family mint admits exactly one domain: the exact full manifest of the
whole sRGB8 point space, the single canonical range `[0, 2^24)` with
`point_count = 2^24`.  Its content identity is the only domain identity a
full-domain claim may carry; a bare point count proves nothing, because a
parsed raw claim keeps the domain identity as an unverified coordinate.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import dual_proof  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402


def digest(label: int) -> bytes:
    return hashlib.sha256(f"full-domain-gate-{label}".encode("ascii")).digest()


def make_claim(
    *, domain_identity: bytes, point_count: int
) -> protocol.DualComparisonClaimV1:
    return protocol.DualComparisonClaimV1(
        digest(1),
        digest(2),
        domain_identity,
        digest(3),
        point_count,
        (digest(4), digest(5)),
        (digest(6), digest(7)),
        (digest(8), digest(9)),
        digest(10),
    )


class ExactFullDomainManifestTests(unittest.TestCase):
    def test_manifest_is_the_single_canonical_range(self) -> None:
        manifest = protocol.exact_full_domain_manifest_v1()
        self.assertIs(type(manifest), protocol.ReducedDomainManifestV1)
        self.assertEqual(manifest.ranges, ((0, protocol.OUTPUT_CARDINALITY_V1),))
        self.assertEqual(manifest.point_count, protocol.OUTPUT_CARDINALITY_V1)

    def test_manifest_reencode_is_byte_identical(self) -> None:
        manifest = protocol.exact_full_domain_manifest_v1()
        self.assertEqual(
            protocol.ReducedDomainManifestV1.parse(manifest.encode()), manifest
        )

    def test_manifest_content_identity_is_stable(self) -> None:
        first = protocol.exact_full_domain_manifest_v1().identity
        second = protocol.exact_full_domain_manifest_v1().identity
        self.assertEqual(len(first), 32)
        self.assertEqual(first, second)

    def test_split_full_coverage_is_not_a_manifest(self) -> None:
        # Two adjacent ranges covering the whole domain must fail canonical
        # grammar, so the exact single range remains the only full manifest.
        half = protocol.OUTPUT_CARDINALITY_V1 // 2
        with self.assertRaises(protocol.ProtocolErrorV1):
            protocol.ReducedDomainManifestV1(
                ((0, half), (half, protocol.OUTPUT_CARDINALITY_V1)),
                protocol.OUTPUT_CARDINALITY_V1,
            )


class FullDomainGateTests(unittest.TestCase):
    def test_exact_full_claim_spans_the_domain(self) -> None:
        identity = protocol.exact_full_domain_manifest_v1().identity
        claim = make_claim(
            domain_identity=identity, point_count=protocol.OUTPUT_CARDINALITY_V1
        )
        self.assertTrue(dual_proof.claim_spans_full_domain_v1(claim))

    def test_full_count_with_foreign_identity_does_not_span(self) -> None:
        # Count alone cannot authorize a family mint: nothing verifies that a
        # raw claim's domain identity belongs to the claimed point count.
        claim = make_claim(
            domain_identity=digest(42), point_count=protocol.OUTPUT_CARDINALITY_V1
        )
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(claim))

    def test_exact_identity_with_short_count_does_not_span(self) -> None:
        identity = protocol.exact_full_domain_manifest_v1().identity
        claim = make_claim(
            domain_identity=identity, point_count=protocol.OUTPUT_CARDINALITY_V1 - 1
        )
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(claim))

    def test_mutated_identity_does_not_span(self) -> None:
        mutated = bytearray(protocol.exact_full_domain_manifest_v1().identity)
        mutated[0] ^= 0xFF
        claim = make_claim(
            domain_identity=bytes(mutated),
            point_count=protocol.OUTPUT_CARDINALITY_V1,
        )
        self.assertFalse(dual_proof.claim_spans_full_domain_v1(claim))

    def test_foreign_input_is_typed_rejection(self) -> None:
        rejection = dual_proof.claim_spans_full_domain_v1(object())
        self.assertIs(type(rejection), dual_proof.DualProofRejectedV1)
        self.assertEqual(
            rejection.reason, dual_proof.DualProofRejectionReasonV1.FOREIGN_INPUT
        )


if __name__ == "__main__":
    unittest.main()
