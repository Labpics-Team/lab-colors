#!/usr/bin/env python3
"""Independent byte oracle for explicit WCAG22 feasibility identities.

This verifier imports no Rust code and performs no colour evaluation. It fixes
only the versioned cross-language byte grammar used by #296-A: exact UTF-8
candidate ordering, the inherited relation grammar, explicit-domain identity
and explicit evaluation identity.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / (
    "crates/labcolors-core/contracts/"
    "wcag22-explicit-feasibility-identity-v1.json"
)

DOMAIN_SEPARATOR = (
    b"labcolors/wcag22-feasibility/domain/explicit-srgb8-set/v1\0"
)
RELATION_SEPARATOR = b"labcolors/wcag22-feasibility/relations/v1\0"
EVALUATION_SEPARATOR = (
    b"labcolors/wcag22-feasibility/evaluation/explicit-srgb8-set/v1\0"
)
DOMAIN_KIND = b"explicit-srgb8-set-v1"

PROFILE_KEY = b"wcag22-srgb8-contrast-v1"
ARTIFACT_KEY = b"wcag22-srgb8-luminance-q55-v1"
BOUND_KEY = b"wcag22-srgb8-outward-q55-v1"
PROOF_KEY = b"wcag22-srgb8-full-domain-q55-v1"
PROOF_SHA256 = bytes.fromhex(
    "d269e9de689009bb955788bf8762fce56680bf616fc0459b6526a367875a6a08"
)

LAYOUT_FIELDS = (
    "canonicalRelations",
    "applicableRelations",
    "notApplicableRelations",
    "applicableEdges",
    "candidateCount",
    "logicalAssessments",
    "failureMatrixBytes",
    "partitionBytes",
    "packedResultBytes",
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def u64(value: int) -> bytes:
    require(type(value) is int and 0 <= value < 1 << 64,
            f"not an unsigned 64-bit value: {value!r}")
    return value.to_bytes(8, "big")


def field(value: bytes) -> bytes:
    return u64(len(value)) + value


def sha256(value: bytes) -> bytes:
    return hashlib.sha256(value).digest()


def rgb(value: Any, label: str) -> tuple[int, int, int]:
    require(
        isinstance(value, (list, tuple))
        and len(value) == 3
        and all(type(channel) is int and 0 <= channel <= 255 for channel in value),
        f"{label} must be exactly three sRGB8 octets",
    )
    return (value[0], value[1], value[2])


def candidate_id_bytes(candidate: dict[str, Any]) -> bytes:
    value = candidate.get("candidateId")
    require(isinstance(value, str) and value != "",
            "candidateId must be a non-empty UTF-8 string")
    return value.encode("utf-8")


def canonical_candidates(
    candidates: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    require(candidates != [], "explicit candidate set must be non-empty")
    canonical = sorted(copy.deepcopy(candidates), key=candidate_id_bytes)
    previous: bytes | None = None
    for index, candidate in enumerate(canonical):
        identity = candidate_id_bytes(candidate)
        require(identity != previous, "duplicate exact candidate ID bytes")
        candidate["emitted"] = list(rgb(candidate.get("emitted"), f"candidate[{index}]"))
        previous = identity
    return canonical


def domain_preimage_from_canonical(
    candidates: list[dict[str, Any]],
) -> bytes:
    preimage = bytearray(DOMAIN_SEPARATOR)
    preimage += field(DOMAIN_KIND)
    preimage += u64(len(candidates))
    for candidate in candidates:
        preimage += field(candidate_id_bytes(candidate))
        preimage += bytes(rgb(candidate["emitted"], "canonical candidate emitted"))
    return bytes(preimage)


def canonical_relation(relation: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(relation)
    relation_id = result.get("relationId")
    occurrence_id = result.get("occurrenceId")
    require(isinstance(relation_id, str) and relation_id != "",
            "relationId must be non-empty")
    require(isinstance(occurrence_id, str) and occurrence_id != "",
            "occurrenceId must be non-empty")
    kind = result.get("kind")
    if kind == "applicable":
        criterion = result.get("criterion")
        require(isinstance(criterion, str) and criterion != "",
                "applicable criterion key must be non-empty")
        adjacent = result.get("adjacent")
        require(isinstance(adjacent, list) and adjacent != [],
                "applicable relation needs adjacency")
        result["adjacent"] = [
            list(value)
            for value in sorted({rgb(value, "adjacent") for value in adjacent})
        ]
        result.pop("reasonId", None)
    elif kind == "notApplicable":
        reason = result.get("reasonId")
        require(isinstance(reason, str) and reason != "",
                "NotApplicable reasonId must be non-empty")
        result.pop("criterion", None)
        result.pop("adjacent", None)
    else:
        raise ValueError(f"unknown relation kind: {kind!r}")
    return result


def canonical_relations(
    relations: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    require(relations != [], "relation set must be non-empty")
    by_id: dict[bytes, dict[str, Any]] = {}
    for relation in relations:
        canonical = canonical_relation(relation)
        identity = canonical["relationId"].encode("utf-8")
        previous = by_id.get(identity)
        require(previous is None or previous == canonical,
                "same relation ID has conflicting canonical declarations")
        by_id[identity] = canonical
    return [by_id[identity] for identity in sorted(by_id)]


def relation_preimage(relations: list[dict[str, Any]]) -> bytes:
    preimage = bytearray(RELATION_SEPARATOR)
    preimage += u64(len(relations))
    for relation in relations:
        kind = relation["kind"]
        if kind == "applicable":
            preimage += b"\x01"
            preimage += field(relation["relationId"].encode("utf-8"))
            preimage += field(relation["occurrenceId"].encode("utf-8"))
            preimage += field(relation["criterion"].encode("utf-8"))
            preimage += u64(len(relation["adjacent"]))
            for adjacent in relation["adjacent"]:
                preimage += bytes(rgb(adjacent, "canonical adjacent"))
        else:
            require(kind == "notApplicable", "canonical relation kind drifted")
            preimage += b"\x02"
            preimage += field(relation["relationId"].encode("utf-8"))
            preimage += field(relation["occurrenceId"].encode("utf-8"))
            preimage += field(relation["reasonId"].encode("utf-8"))
    return bytes(preimage)


def evaluation_preimage(
    domain_digest: bytes,
    relation_digest: bytes,
    layout: dict[str, int],
    matrix: bytes,
    partition: bytes,
) -> bytes:
    require(len(domain_digest) == 32 and len(relation_digest) == 32,
            "content digests must be SHA-256 values")
    preimage = bytearray(EVALUATION_SEPARATOR)
    preimage += domain_digest
    preimage += relation_digest
    for key in (PROFILE_KEY, ARTIFACT_KEY, BOUND_KEY, PROOF_KEY):
        preimage += field(key)
    preimage += PROOF_SHA256
    for name in LAYOUT_FIELDS:
        require(name in layout, f"missing layout field {name}")
        preimage += u64(layout[name])
    preimage += sha256(matrix)
    preimage += field(partition)
    return bytes(preimage)


def bit(value: bytes, index: int) -> bool:
    return value[index // 8] & (1 << (index % 8)) != 0


def validate_complete_fixture(
    candidates: list[dict[str, Any]],
    relations: list[dict[str, Any]],
    layout: dict[str, int],
    matrix: bytes,
    partition: bytes,
) -> None:
    candidate_count = len(candidates)
    applicable = [value for value in relations if value["kind"] == "applicable"]
    not_applicable = [
        value for value in relations if value["kind"] == "notApplicable"
    ]
    edges = sum(len(value["adjacent"]) for value in applicable)
    work = candidate_count * edges
    matrix_bytes = (work + 7) // 8
    partition_bytes = (candidate_count + 7) // 8
    expected = {
        "canonicalRelations": len(relations),
        "applicableRelations": len(applicable),
        "notApplicableRelations": len(not_applicable),
        "applicableEdges": edges,
        "candidateCount": candidate_count,
        "logicalAssessments": work,
        "failureMatrixBytes": matrix_bytes,
        "partitionBytes": partition_bytes,
        "packedResultBytes": matrix_bytes + partition_bytes,
    }
    require(layout == expected, "fixture layout drifted from exact C x E laws")
    require(len(matrix) == matrix_bytes, "matrix byte length is not ceil(C*E/8)")
    require(len(partition) == partition_bytes,
            "partition byte length is not ceil(C/8)")
    if work % 8:
        require(matrix[-1] >> (work % 8) == 0, "matrix tail bits must be zero")
    if candidate_count % 8:
        require(partition[-1] >> (candidate_count % 8) == 0,
                "partition tail bits must be zero")
    for candidate in range(candidate_count):
        row_passes = all(not bit(matrix, candidate * edges + edge)
                         for edge in range(edges))
        require(bit(partition, candidate) == row_passes,
                "partition must be the exact row-wise matrix reduction")


def fixture_model() -> dict[str, Any]:
    # Deliberately shuffled. U+0065 U+0301 and U+00E9 look alike but are two
    # exact, non-normalized client IDs and even carry the same physical bytes.
    candidates = [
        {"candidateId": "🎨", "emitted": [255, 128, 1]},
        {"candidateId": "é", "emitted": [18, 52, 86]},
        {"candidateId": "海", "emitted": [0, 0, 0]},
        {"candidateId": "e\u0301", "emitted": [18, 52, 86]},
    ]
    relations = [
        {
            "relationId": "zeta",
            "occurrenceId": "ornament",
            "kind": "notApplicable",
            "reasonId": "client/не-применимо",
        },
        {
            "relationId": "alpha",
            "occurrenceId": "hover/🎨",
            "kind": "applicable",
            "criterion": "sc-1.4.3-text-default",
            "adjacent": [[255, 255, 255], [0, 0, 0], [118, 118, 118], [0, 0, 0]],
        },
    ]
    return {
        "candidates": candidates,
        "relations": relations,
        "layout": {
            "canonicalRelations": 2,
            "applicableRelations": 1,
            "notApplicableRelations": 1,
            "applicableEdges": 3,
            "candidateCount": 4,
            "logicalAssessments": 12,
            "failureMatrixBytes": 2,
            "partitionBytes": 1,
            "packedResultBytes": 3,
        },
        # Actual public WCAG path, candidate-major LSB0 rows:
        # F,F,P | F,F,P | F,P,P | P,F,F. No candidate passes every edge.
        "matrix": bytes.fromhex("5b0c"),
        "partition": bytes.fromhex("00"),
    }


def identities(model: dict[str, Any]) -> tuple[bytes, bytes, bytes]:
    candidates = canonical_candidates(model["candidates"])
    relations = canonical_relations(model["relations"])
    domain = sha256(domain_preimage_from_canonical(candidates))
    relation = sha256(relation_preimage(relations))
    evaluation = sha256(
        evaluation_preimage(
            domain,
            relation,
            model["layout"],
            model["matrix"],
            model["partition"],
        )
    )
    return domain, relation, evaluation


def build_fixture() -> dict[str, Any]:
    model = fixture_model()
    candidates = canonical_candidates(model["candidates"])
    relations = canonical_relations(model["relations"])
    validate_complete_fixture(
        candidates,
        relations,
        model["layout"],
        model["matrix"],
        model["partition"],
    )
    domain, relation, evaluation = identities(model)
    return {
        "schemaVersion": 1,
        "artifactId": "wcag22-explicit-feasibility-identity-v1",
        "encoding": {
            "integer": "u64-big-endian",
            "byteString": "u64-length-then-exact-bytes",
            "candidateOrder": "lexicographic-exact-utf8-bytes-no-normalization",
            "candidateRecord": "length-prefixed-id-then-three-srgb8-octets",
            "relationGrammar": "wcag22-feasibility-relations-v1",
            "matrixBitOrder": "candidate-major-contiguous-lsb0",
            "partitionBitOrder": "canonical-candidate-index-lsb0",
            "evaluationLayoutOrder": list(LAYOUT_FIELDS),
            "partitionInEvaluation": "u64-length-then-exact-bytes",
        },
        "fixture": {
            "domainKind": DOMAIN_KIND.decode(),
            "declaredCandidates": model["candidates"],
            "canonicalCandidates": [
                {
                    **candidate,
                    "candidateIdUtf8Hex": candidate_id_bytes(candidate).hex(),
                }
                for candidate in candidates
            ],
            "canonicalRelations": relations,
            "layout": model["layout"],
            "failureMatrixHex": model["matrix"].hex(),
            "matrixSha256": sha256(model["matrix"]).hex(),
            "partitionHex": model["partition"].hex(),
        },
        "expected": {
            "domainDigestSha256": domain.hex(),
            "relationSetDigestSha256": relation.hex(),
            "evaluationIdSha256": evaluation.hex(),
        },
    }


def canonical_bytes(value: dict[str, Any]) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def mutation_self_tests() -> int:
    baseline_model = fixture_model()
    baseline = identities(baseline_model)
    checks = 0

    def changed(label: str, mutate: Any, expected_indices: tuple[int, ...]) -> None:
        nonlocal checks
        candidate = copy.deepcopy(baseline_model)
        mutate(candidate)
        observed = identities(candidate)
        require(
            all(observed[index] != baseline[index] for index in expected_indices),
            f"mutation survived identity oracle: {label}",
        )
        checks += 1

    changed(
        "candidate ID bytes",
        lambda value: value["candidates"][1].__setitem__("candidateId", "É"),
        (0, 2),
    )
    changed(
        "candidate emitted RGB",
        lambda value: value["candidates"][1].__setitem__("emitted", [19, 52, 86]),
        (0, 2),
    )
    changed(
        "layout count",
        lambda value: value["layout"].__setitem__("logicalAssessments", 13),
        (2,),
    )
    changed(
        "matrix byte",
        lambda value: value.__setitem__(
            "matrix", bytes([value["matrix"][0] ^ 1, value["matrix"][1]])
        ),
        (2,),
    )
    changed(
        "partition byte",
        lambda value: value.__setitem__("partition", bytes([value["partition"][0] ^ 1])),
        (2,),
    )
    changed(
        "relation occurrence",
        lambda value: value["relations"][1].__setitem__("occurrenceId", "hover/other"),
        (1, 2),
    )

    # A caller permutation is intentionally invariant because Core sorts exact
    # UTF-8 bytes. Hashing the reversed canonical records directly, however,
    # is a wrong preimage and must not match the canonical domain identity.
    permuted = copy.deepcopy(baseline_model)
    permuted["candidates"].reverse()
    require(identities(permuted) == baseline,
            "declared candidate permutation changed canonical identity")
    canonical = canonical_candidates(baseline_model["candidates"])
    wrong_order = sha256(domain_preimage_from_canonical(list(reversed(canonical))))
    require(wrong_order != baseline[0],
            "non-canonical candidate record order matched canonical digest")
    checks += 1

    decomposed = "e\u0301".encode("utf-8")
    composed = "é".encode("utf-8")
    require(decomposed != composed, "normalization witness collapsed exact UTF-8 bytes")
    require(
        [candidate_id_bytes(value) for value in canonical]
        == sorted([candidate_id_bytes(value) for value in canonical]),
        "canonical candidate order is not exact byte order",
    )
    checks += 1
    return checks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print", action="store_true", dest="print_fixture")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    expected = canonical_bytes(build_fixture())
    if args.print_fixture:
        print(expected.decode("utf-8"), end="")
        return 0
    actual = FIXTURE.read_bytes()
    require(actual == expected,
            "explicit feasibility identity fixture drift; run --print and review exact bytes")
    payload = json.loads(actual)
    mutation_checks = mutation_self_tests() if args.self_test else 0
    print(
        "WCAG22 explicit feasibility identity oracle: PASS; "
        f"domain={payload['expected']['domainDigestSha256']}; "
        f"relations={payload['expected']['relationSetDigestSha256']}; "
        f"evaluation={payload['expected']['evaluationIdSha256']}; "
        f"fixture_sha256={sha256(actual).hex()}; "
        f"mutation_checks={mutation_checks}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        raise SystemExit(f"WCAG22 explicit feasibility identity oracle: {error}") from error
