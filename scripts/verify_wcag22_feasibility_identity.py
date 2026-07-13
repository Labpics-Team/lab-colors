#!/usr/bin/env python3
"""Independent byte oracle for WCAG22 feasibility V1 identity preimages.

This script contains no Rust imports and performs no colour evaluation. It
fixes only the cross-language byte grammar: separators, tags, length prefixes,
integer endianness, canonical fixture order and SHA-256 outputs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "crates/labcolors-core/contracts/wcag22-feasibility-identity-v1.json"

DOMAIN_SEPARATOR = b"labcolors/wcag22-feasibility/domain/v1\0"
RELATION_SEPARATOR = b"labcolors/wcag22-feasibility/relations/v1\0"
EVALUATION_SEPARATOR = b"labcolors/wcag22-feasibility/evaluation/v1\0"

PROFILE_KEY = b"wcag22-srgb8-contrast-v1"
ARTIFACT_KEY = b"wcag22-srgb8-luminance-q55-v1"
BOUND_KEY = b"wcag22-srgb8-outward-q55-v1"
PROOF_KEY = b"wcag22-srgb8-full-domain-q55-v1"
PROOF_SHA256 = bytes.fromhex(
    "d269e9de689009bb955788bf8762fce56680bf616fc0459b6526a367875a6a08"
)


def u64(value: int) -> bytes:
    if not 0 <= value < 1 << 64:
        raise ValueError(f"not an unsigned 64-bit value: {value}")
    return value.to_bytes(8, "big")


def field(value: bytes) -> bytes:
    return u64(len(value)) + value


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def build_fixture() -> dict[str, object]:
    domain_preimage = bytearray(DOMAIN_SEPARATOR)
    domain_preimage += field(b"srgb8-neutral-axis-v1")
    domain_preimage += u64(256)
    for candidate in range(256):
        domain_preimage += bytes([candidate, candidate, candidate])
    domain_digest = hashlib.sha256(domain_preimage).digest()

    # Exact canonical relation order: alpha (Applicable), then zeta
    # (NotApplicable). Adjacent bytes are already sorted.
    relation_preimage = bytearray(RELATION_SEPARATOR)
    relation_preimage += u64(2)
    relation_preimage += b"\x01"
    relation_preimage += field("alpha".encode())
    relation_preimage += field("hover/🎨".encode())
    relation_preimage += field(b"sc-1.4.3-text-default")
    relation_preimage += u64(3)
    relation_preimage += bytes([0, 0, 0, 118, 118, 118, 255, 255, 255])
    relation_preimage += b"\x02"
    relation_preimage += field("zeta".encode())
    relation_preimage += field("ornament".encode())
    relation_preimage += field("client/не-применимо".encode())
    relation_digest = hashlib.sha256(relation_preimage).digest()

    canonical_relations = 2
    applicable_relations = 1
    not_applicable_relations = 1
    applicable_edges = 3
    logical_assessments = 256 * applicable_edges
    packed_result_bytes = 32 * (applicable_edges + 1)
    matrix = bytes((index * 37 + 11) % 256 for index in range(32 * applicable_edges))
    partition = bytes((255 - index * 3) % 256 for index in range(32))

    evaluation_preimage = bytearray(EVALUATION_SEPARATOR)
    evaluation_preimage += domain_digest
    evaluation_preimage += relation_digest
    for key in (PROFILE_KEY, ARTIFACT_KEY, BOUND_KEY, PROOF_KEY):
        evaluation_preimage += field(key)
    evaluation_preimage += PROOF_SHA256
    for count in (
        canonical_relations,
        applicable_relations,
        not_applicable_relations,
        applicable_edges,
        logical_assessments,
        packed_result_bytes,
    ):
        evaluation_preimage += u64(count)
    evaluation_preimage += hashlib.sha256(matrix).digest()
    evaluation_preimage += partition

    return {
        "schemaVersion": 1,
        "artifactId": "wcag22-feasibility-identity-v1",
        "encoding": {
            "integer": "u64-big-endian",
            "byteString": "u64-length-then-exact-bytes",
            "matrixBitOrder": "candidate-major-lsb0",
            "partitionBitOrder": "candidate-index-lsb0",
            "applicableTag": 1,
            "notApplicableTag": 2,
        },
        "fixture": {
            "canonicalRelations": canonical_relations,
            "applicableRelations": applicable_relations,
            "notApplicableRelations": not_applicable_relations,
            "applicableEdges": applicable_edges,
            "logicalAssessments": logical_assessments,
            "packedResultBytes": packed_result_bytes,
            "matrixSha256": sha256(matrix),
            "partitionHex": partition.hex(),
        },
        "expected": {
            "domainDigestSha256": domain_digest.hex(),
            "relationSetDigestSha256": relation_digest.hex(),
            "evaluationIdSha256": sha256(evaluation_preimage),
        },
    }


def canonical_bytes(value: dict[str, object]) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print", action="store_true", dest="print_fixture")
    args = parser.parse_args()
    expected = canonical_bytes(build_fixture())
    if args.print_fixture:
        print(expected.decode(), end="")
        return 0
    actual = FIXTURE.read_bytes()
    if actual != expected:
        raise SystemExit(
            "WCAG22 feasibility identity fixture drift; run with --print and review exact bytes"
        )
    payload = json.loads(actual)
    print(
        "WCAG22 feasibility identity oracle: PASS; "
        f"domain={payload['expected']['domainDigestSha256']}; "
        f"relations={payload['expected']['relationSetDigestSha256']}; "
        f"evaluation={payload['expected']['evaluationIdSha256']}; "
        f"fixture_sha256={sha256(actual)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
