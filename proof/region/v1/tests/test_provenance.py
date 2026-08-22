#!/usr/bin/env python3
"""Hostile raw/provenance boundary tests for region proof V1."""

from __future__ import annotations

import hashlib
import sys
import unittest
from dataclasses import replace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import region_proof_protocol as protocol  # noqa: E402


class ProvenanceClaimTests(unittest.TestCase):
    def test_run_is_a_structural_claim_not_a_receipt(self) -> None:
        self.assertFalse(hasattr(protocol, "RunReceiptV1"))

        coordinates = tuple(bytes((value,)) * 32 for value in range(1, 7))
        expected = b"LCRUN1\0\0" + b"".join(coordinates)
        claim = protocol.RunClaimV1(*coordinates)

        self.assertEqual(len(expected), 200)
        self.assertEqual(
            hashlib.sha256(expected).hexdigest(),
            "bbe225505a3015b351e2a74ad164bcbc23bc86b7f8b2b588a015217887dc1759",
        )
        self.assertEqual(claim.encode(), expected)
        self.assertEqual(protocol.RunClaimV1.parse(expected), claim)
        self.assertEqual(
            claim.identity.hex(),
            "3c5f8ad117cb8c61d987d1ff08880f1f3388a5f423ab0da6a0f3f640f441f903",
        )

    def test_provenance_claim_has_a_literal_canonical_wire_oracle(self) -> None:
        coordinates = tuple(bytes((value,)) * 32 for value in range(1, 4))
        expected = b"LCPRV1\0\0" + b"".join(coordinates)
        claim = protocol.EvaluatorProvenanceClaimV1(*coordinates)

        self.assertEqual(len(expected), 104)
        self.assertEqual(
            hashlib.sha256(expected).hexdigest(),
            "aff9c702aa1c9c8e7548085da918de9708d81a2d3613c0f4de684629ae2d5097",
        )
        self.assertEqual(claim.encode(), expected)
        self.assertEqual(protocol.EvaluatorProvenanceClaimV1.parse(expected), claim)
        self.assertEqual(
            claim.identity.hex(),
            "755b0c2cc4c2a336579beab3a432840893e849671de98b719633cbf056582b02",
        )

    def test_raw_provenance_never_mints_a_source_bound_receipt(self) -> None:
        raw = b"LCPRV1\0\0" + b"".join(
            bytes((value,)) * 32 for value in range(11, 14)
        )
        claim = protocol.EvaluatorProvenanceClaimV1.parse(raw)

        self.assertIs(type(claim), protocol.EvaluatorProvenanceClaimV1)
        self.assertFalse(hasattr(protocol, "SourceBoundEvaluatorReceiptV1"))
        self.assertFalse(hasattr(protocol.EvaluatorProvenanceClaimV1, "admit"))
        self.assertFalse(hasattr(protocol.EvaluatorProvenanceClaimV1, "resolve"))

    def test_structural_diversity_never_claims_independence(self) -> None:
        self.assertFalse(hasattr(protocol.ProtocolReasonV1, "NOT_INDEPENDENT"))
        self.assertEqual(
            protocol.ProtocolReasonV1.SHARED_DIVERSITY_COORDINATE,
            "shared_diversity_coordinate",
        )

        documentation = (ROOT / "PROTOCOL.md").read_text(encoding="utf-8")
        for overclaim in (
            "RunReceiptV1",
            "фактически наблюдаемого run",
            "неподтверждённая independence",
            "не используют общий код",
        ):
            with self.subTest(overclaim=overclaim):
                self.assertNotIn(overclaim, documentation)

    def test_provenance_parser_rejects_noncanonical_wire(self) -> None:
        valid = b"LCPRV1\0\0" + b"\1" * 32 + b"\2" * 32 + b"\3" * 32
        cases = (
            (b"BADMAGIC" + valid[8:], protocol.ProtocolReasonV1.BAD_MAGIC),
            (valid[:-1], protocol.ProtocolReasonV1.TRUNCATED),
            (valid + b"\0", protocol.ProtocolReasonV1.TRAILING_BYTES),
            (valid[:8] + bytes(32) + valid[40:], protocol.ProtocolReasonV1.INVALID_DIGEST),
        )
        for encoded, reason in cases:
            with self.subTest(reason=reason):
                with self.assertRaises(protocol.ProtocolErrorV1) as caught:
                    protocol.EvaluatorProvenanceClaimV1.parse(encoded)
                self.assertEqual(caught.exception.reason, reason)

    def test_each_provenance_coordinate_is_nonzero_and_identity_bound(self) -> None:
        claim = protocol.EvaluatorProvenanceClaimV1(
            provenance_policy_identity=b"\1" * 32,
            run_claim_identity=b"\2" * 32,
            replay_evidence_identity=b"\3" * 32,
        )
        for field_name in (
            "provenance_policy_identity",
            "run_claim_identity",
            "replay_evidence_identity",
        ):
            with self.subTest(field_name=field_name):
                with self.assertRaises(protocol.ProtocolErrorV1) as caught:
                    replace(claim, **{field_name: bytes(32)})
                self.assertEqual(
                    caught.exception.reason,
                    protocol.ProtocolReasonV1.INVALID_DIGEST,
                )

                changed = replace(claim, **{field_name: b"\xff" * 32})
                self.assertNotEqual(changed.encode(), claim.encode())
                self.assertNotEqual(changed.identity, claim.identity)
                self.assertEqual(
                    protocol.EvaluatorProvenanceClaimV1.parse(changed.encode()),
                    changed,
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
