#!/usr/bin/env python3
"""Independent byte oracle for explicit WCAG22 selection identities.

The oracle imports no Rust code and performs no colour evaluation. It composes
the already independent #296-A feasibility grammar with the new #296-B policy
and selected-row receipt grammars, then checks a fixture produced from exact
UTF-8 bytes, LSB0 feasibility bits and canonical applicable-edge order.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
from typing import Any

import verify_wcag22_explicit_feasibility_identity as feasibility


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / (
    "crates/labcolors-core/contracts/"
    "wcag22-explicit-selection-identity-v1.json"
)

POLICY_SEPARATOR = (
    b"labcolors/wcag22-feasibility/selection/policy/"
    b"first-feasible-in-declared-order/v1\0"
)
RECEIPT_SEPARATOR = (
    b"labcolors/wcag22-feasibility/selection/receipt/"
    b"selected-final-verification/v1\0"
)
POLICY_KIND = b"first-feasible-in-declared-order-v1"
RECEIPT_KIND = b"selected-final-verification-v1"
VERIFIED_PASS_TAG = 1


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def u64(value: int) -> bytes:
    require(
        type(value) is int and 0 <= value < 1 << 64,
        f"not an unsigned 64-bit value: {value!r}",
    )
    return value.to_bytes(8, "big")


def field(value: bytes) -> bytes:
    return u64(len(value)) + value


def sha256(value: bytes) -> bytes:
    return hashlib.sha256(value).digest()


def exact_utf8(value: Any, label: str) -> bytes:
    require(isinstance(value, str) and value != "", f"{label} must be non-empty")
    return value.encode("utf-8")


def exact_rgb(value: Any, label: str) -> bytes:
    return bytes(feasibility.rgb(value, label))


def policy_preimage(
    policy_id: bytes,
    ordered_candidate_ids: list[bytes],
    *,
    separator: bytes = POLICY_SEPARATOR,
    kind: bytes = POLICY_KIND,
    declared_count: int | None = None,
) -> bytes:
    require(policy_id != b"", "policy ID bytes must be non-empty")
    require(ordered_candidate_ids != [], "policy order must be non-empty")
    require(all(value != b"" for value in ordered_candidate_ids),
            "policy candidate ID bytes must be non-empty")
    preimage = bytearray(separator)
    preimage += field(kind)
    preimage += field(policy_id)
    preimage += u64(
        len(ordered_candidate_ids) if declared_count is None else declared_count
    )
    for candidate_id in ordered_candidate_ids:
        preimage += field(candidate_id)
    return bytes(preimage)


def receipt_preimage(
    *,
    evaluation_id: bytes,
    relation_set_digest: bytes,
    policy_digest: bytes,
    selected_policy_ordinal: int,
    selected_candidate_id: bytes,
    selected_emitted: bytes,
    verified_applicable_edges: int,
    relations: list[dict[str, Any]],
    separator: bytes = RECEIPT_SEPARATOR,
    kind: bytes = RECEIPT_KIND,
    profile_key: bytes = feasibility.PROFILE_KEY,
    artifact_key: bytes = feasibility.ARTIFACT_KEY,
    bound_key: bytes = feasibility.BOUND_KEY,
    proof_key: bytes = feasibility.PROOF_KEY,
    proof_sha256: bytes = feasibility.PROOF_SHA256,
) -> bytes:
    for value, label in (
        (evaluation_id, "evaluation ID"),
        (relation_set_digest, "relation-set digest"),
        (policy_digest, "policy digest"),
        (proof_sha256, "proof SHA-256"),
    ):
        require(len(value) == 32, f"{label} must contain exactly 32 bytes")
    require(selected_candidate_id != b"", "selected candidate ID must be non-empty")
    require(len(selected_emitted) == 3, "selected emitted value must be one sRGB8 triple")
    streamed_edges = sum(len(relation["edges"]) for relation in relations)
    require(
        verified_applicable_edges == streamed_edges,
        "verified edge count must equal the exact streamed record count",
    )

    preimage = bytearray(separator)
    preimage += field(kind)
    preimage += evaluation_id
    preimage += relation_set_digest
    preimage += policy_digest
    preimage += u64(selected_policy_ordinal)
    preimage += field(selected_candidate_id)
    preimage += selected_emitted
    for key in (profile_key, artifact_key, bound_key, proof_key):
        preimage += field(key)
    preimage += proof_sha256
    preimage += u64(verified_applicable_edges)
    for relation in relations:
        relation_id = relation["relationId"]
        criterion = relation["criterion"]
        require(relation_id != b"", "verified relation ID must be non-empty")
        require(criterion != b"", "verified criterion key must be non-empty")
        preimage += u64(relation["relationOrdinal"])
        preimage += field(relation_id)
        preimage += field(criterion)
        preimage += u64(len(relation["edges"]))
        for edge in relation["edges"]:
            foreground = edge["foreground"]
            background = edge["background"]
            require(len(foreground) == 3 and len(background) == 3,
                    "verified edge colours must be exact sRGB8 triples")
            decision_tag = edge["decisionTag"]
            require(
                decision_tag == VERIFIED_PASS_TAG,
                "a V1 selected receipt may contain only the verified-Pass tag",
            )
            preimage += u64(edge["edgeOrdinal"])
            preimage += foreground
            preimage += background
            preimage += bytes([decision_tag])
    return bytes(preimage)


def fixture_model() -> dict[str, Any]:
    # Deliberately shuffled. The two visually equivalent Unicode IDs remain
    # byte-distinct and carry the same physical #757575 candidate.
    return {
        "candidates": [
            {"candidateId": "海", "emitted": [0, 0, 0]},
            {"candidateId": "é", "emitted": [117, 117, 117]},
            {"candidateId": "e\u0301", "emitted": [117, 117, 117]},
        ],
        "relations": [
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
                "adjacent": [
                    [255, 255, 255],
                    [0, 0, 0],
                    [255, 255, 255],
                ],
            },
        ],
        "layout": {
            "canonicalRelations": 2,
            "applicableRelations": 1,
            "notApplicableRelations": 1,
            "applicableEdges": 2,
            "candidateCount": 3,
            "logicalAssessments": 6,
            "failureMatrixBytes": 1,
            "partitionBytes": 1,
            "packedResultBytes": 2,
        },
        # Candidate-major LSB0 rows: P,P | P,P | F,P.
        "matrix": bytes.fromhex("10"),
        "partition": bytes.fromhex("03"),
    }


def canonical_model(model: dict[str, Any]) -> tuple[
    list[dict[str, Any]], list[dict[str, Any]], bytes, bytes, bytes
]:
    candidates = feasibility.canonical_candidates(model["candidates"])
    relations = feasibility.canonical_relations(model["relations"])
    feasibility.validate_complete_fixture(
        candidates,
        relations,
        model["layout"],
        model["matrix"],
        model["partition"],
    )
    domain, relation, evaluation = feasibility.identities(model)
    return candidates, relations, domain, relation, evaluation


def evaluate_policy(
    model: dict[str, Any], policy: dict[str, Any]
) -> dict[str, Any]:
    candidates, relations, domain_digest, relation_digest, evaluation_id = (
        canonical_model(model)
    )
    policy_id = exact_utf8(policy.get("policyId"), "policyId")
    order = policy.get("orderedCandidateIds")
    require(isinstance(order, list) and order != [], "policy order must be non-empty")
    ordered_ids = [exact_utf8(value, "ordered candidate ID") for value in order]

    by_id = {
        feasibility.candidate_id_bytes(candidate): index
        for index, candidate in enumerate(candidates)
    }
    seen: set[bytes] = set()
    canonical_indices: list[int] = []
    # Validate the entire order before using a feasible prefix.
    for candidate_id in ordered_ids:
        require(candidate_id in by_id, "foreign candidate ID in policy order")
        require(candidate_id not in seen, "duplicate candidate ID in policy order")
        seen.add(candidate_id)
        canonical_indices.append(by_id[candidate_id])

    digest = sha256(policy_preimage(policy_id, ordered_ids))
    selected_policy_ordinal: int | None = None
    selected_index: int | None = None
    for ordinal, candidate_index in enumerate(canonical_indices):
        if feasibility.bit(model["partition"], candidate_index):
            selected_policy_ordinal = ordinal
            selected_index = candidate_index
            break

    result: dict[str, Any] = {
        "policyId": policy["policyId"],
        "orderedCandidateIds": order,
        "policyDigestSha256": digest,
        "domainDigestSha256": domain_digest,
        "relationSetDigestSha256": relation_digest,
        "evaluationIdSha256": evaluation_id,
    }
    if selected_index is None:
        result["outcome"] = "noSelection"
        result["selectionReceiptDigestSha256"] = None
        return result

    assert selected_policy_ordinal is not None
    selected = candidates[selected_index]
    selected_emitted = exact_rgb(selected["emitted"], "selected emitted")
    verified_relations: list[dict[str, Any]] = []
    edge_ordinal = 0
    for relation_ordinal, relation in enumerate(relations):
        if relation["kind"] != "applicable":
            continue
        relation_edges: list[dict[str, Any]] = []
        for adjacent in relation["adjacent"]:
            logical_index = (
                selected_index * model["layout"]["applicableEdges"] + edge_ordinal
            )
            require(
                not feasibility.bit(model["matrix"], logical_index),
                "selected fixture row must contain only Pass cells",
            )
            relation_edges.append(
                {
                    "edgeOrdinal": edge_ordinal,
                    "foreground": selected_emitted,
                    "background": exact_rgb(adjacent, "adjacent"),
                    "decisionTag": VERIFIED_PASS_TAG,
                }
            )
            edge_ordinal += 1
        verified_relations.append(
            {
                "relationOrdinal": relation_ordinal,
                "relationId": exact_utf8(relation["relationId"], "relationId"),
                "criterion": exact_utf8(relation["criterion"], "criterion"),
                "edges": relation_edges,
            }
        )
    require(
        edge_ordinal == model["layout"]["applicableEdges"],
        "verified fixture traversal must cover every canonical applicable edge",
    )

    receipt_inputs = {
        "evaluation_id": evaluation_id,
        "relation_set_digest": relation_digest,
        "policy_digest": digest,
        "selected_policy_ordinal": selected_policy_ordinal,
        "selected_candidate_id": feasibility.candidate_id_bytes(selected),
        "selected_emitted": selected_emitted,
        "verified_applicable_edges": edge_ordinal,
        "relations": verified_relations,
    }
    result.update(
        {
            "outcome": "selected",
            "selectedCandidateId": selected["candidateId"],
            "selectedEmitted": selected_emitted,
            "selectedPolicyOrdinal": selected_policy_ordinal,
            "verifiedApplicableEdges": edge_ordinal,
            "verifiedRelations": verified_relations,
            "receiptInputs": receipt_inputs,
            "selectionReceiptDigestSha256": sha256(
                receipt_preimage(**receipt_inputs)
            ),
        }
    )
    return result


def json_edge(edge: dict[str, Any]) -> dict[str, Any]:
    return {
        "edgeOrdinal": edge["edgeOrdinal"],
        "foreground": list(edge["foreground"]),
        "background": list(edge["background"]),
        "decisionTag": edge["decisionTag"],
    }


def json_verified_relation(relation: dict[str, Any]) -> dict[str, Any]:
    return {
        "relationOrdinal": relation["relationOrdinal"],
        "relationId": relation["relationId"].decode("utf-8"),
        "criterion": relation["criterion"].decode("utf-8"),
        "edges": [json_edge(edge) for edge in relation["edges"]],
    }


def json_outcome(result: dict[str, Any]) -> dict[str, Any]:
    value: dict[str, Any] = {
        "policyId": result["policyId"],
        "orderedCandidateIds": result["orderedCandidateIds"],
        "outcome": result["outcome"],
        "policyDigestSha256": result["policyDigestSha256"].hex(),
        "selectionReceiptDigestSha256": (
            result["selectionReceiptDigestSha256"].hex()
            if result["selectionReceiptDigestSha256"] is not None
            else None
        ),
    }
    if result["outcome"] == "selected":
        value.update(
            {
                "selectedCandidateId": result["selectedCandidateId"],
                "selectedEmitted": list(result["selectedEmitted"]),
                "selectedPolicyOrdinal": result["selectedPolicyOrdinal"],
                "verifiedApplicableEdges": result["verifiedApplicableEdges"],
                "verifiedRelations": [
                    json_verified_relation(relation)
                    for relation in result["verifiedRelations"]
                ],
            }
        )
    return value


def policy_cases() -> list[dict[str, Any]]:
    policy_id = "brand/выбор/🎨"
    return [
        {
            "policyId": policy_id,
            "orderedCandidateIds": ["海", "é", "e\u0301"],
        },
        {
            "policyId": policy_id,
            "orderedCandidateIds": ["海", "e\u0301", "é"],
        },
        {"policyId": policy_id, "orderedCandidateIds": ["海"]},
    ]


def build_fixture() -> dict[str, Any]:
    model = fixture_model()
    candidates, relations, domain, relation, evaluation = canonical_model(model)
    outcomes = [evaluate_policy(model, policy) for policy in policy_cases()]
    return {
        "schemaVersion": 1,
        "artifactId": "wcag22-explicit-selection-identity-v1",
        "encoding": {
            "integer": "u64-big-endian",
            "byteString": "u64-length-then-exact-bytes",
            "digest": "raw-32-byte-sha256-in-preimage-lowercase-hex-in-json",
            "policyOrder": "client-declared-exact-utf8-bytes-no-normalization",
            "policyPreimageOrder": [
                "separator",
                "length-prefixed-policy-kind",
                "length-prefixed-policy-id",
                "declared-candidate-count",
                "length-prefixed-candidate-ids-in-declared-order",
            ],
            "receiptPreimageOrder": [
                "separator",
                "length-prefixed-receipt-kind",
                "evaluation-id",
                "relation-set-digest",
                "policy-digest",
                "selected-policy-ordinal",
                "length-prefixed-selected-candidate-id",
                "selected-srgb8",
                "length-prefixed-profile-artifact-bound-proof-keys",
                "proof-sha256",
                "verified-applicable-edge-count",
                "canonical-applicable-relation-records-with-edge-records",
            ],
            "verifiedRelationRecord": [
                "canonical-relation-ordinal-u64",
                "length-prefixed-relation-id",
                "length-prefixed-actual-criterion-key",
                "relation-edge-count-u64",
                "verified-edge-records",
            ],
            "verifiedEdgeRecord": [
                "edge-ordinal-u64",
                "actual-foreground-srgb8",
                "actual-background-srgb8",
                "verified-pass-tag-01",
            ],
        },
        "fixture": {
            "declaredCandidates": model["candidates"],
            "canonicalCandidates": [
                {
                    **candidate,
                    "candidateIdUtf8Hex": feasibility.candidate_id_bytes(candidate).hex(),
                }
                for candidate in candidates
            ],
            "canonicalRelations": relations,
            "layout": model["layout"],
            "failureMatrixHex": model["matrix"].hex(),
            "matrixSha256": sha256(model["matrix"]).hex(),
            "partitionHex": model["partition"].hex(),
            "domainDigestSha256": domain.hex(),
            "relationSetDigestSha256": relation.hex(),
            "evaluationIdSha256": evaluation.hex(),
            "policies": [json_outcome(outcome) for outcome in outcomes],
        },
    }


def canonical_bytes(value: dict[str, Any]) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def mutation_self_tests() -> tuple[int, int, int]:
    model = fixture_model()
    baseline = evaluate_policy(model, policy_cases()[0])
    require(baseline["outcome"] == "selected", "baseline must select")
    mutation_checks = 0
    invariance_checks = 0
    rejection_checks = 0

    baseline_policy_id = exact_utf8(baseline["policyId"], "policyId")
    baseline_order = [
        exact_utf8(value, "ordered ID") for value in baseline["orderedCandidateIds"]
    ]
    baseline_policy_digest = baseline["policyDigestSha256"]

    def policy_changed(label: str, **overrides: Any) -> None:
        nonlocal mutation_checks
        observed = sha256(
            policy_preimage(
                overrides.get("policy_id", baseline_policy_id),
                overrides.get("order", baseline_order),
                separator=overrides.get("separator", POLICY_SEPARATOR),
                kind=overrides.get("kind", POLICY_KIND),
                declared_count=overrides.get("declared_count"),
            )
        )
        require(observed != baseline_policy_digest,
                f"policy mutation survived identity oracle: {label}")
        mutation_checks += 1

    policy_changed("policy separator", separator=POLICY_SEPARATOR[:-2] + b"2\0")
    policy_changed("policy kind", kind=POLICY_KIND[:-1] + b"2")
    policy_changed("policy ID", policy_id=baseline_policy_id + b"/changed")
    policy_changed("declared count", declared_count=len(baseline_order) + 1)
    policy_changed("declared order", order=[baseline_order[0], baseline_order[2], baseline_order[1]])
    policy_changed("candidate ID bytes", order=[b"changed", *baseline_order[1:]])
    policy_changed("candidate insertion", order=[*baseline_order, b"inserted"])
    policy_changed("candidate deletion", order=baseline_order[:-1])
    require(
        sha256(policy_preimage(b"tail", [b"selected", b"a", b"b"]))
        != sha256(policy_preimage(b"tail", [b"selected", b"b", b"a"])),
        "unselected policy-tail order was not bound",
    )
    mutation_checks += 1
    require(
        sha256(policy_preimage(b"split", [b"a", b"bc"]))
        != sha256(policy_preimage(b"split", [b"ab", b"c"])),
        "length prefixes failed to distinguish split candidate IDs",
    )
    mutation_checks += 1
    require(
        sha256(policy_preimage(b"unicode", ["é".encode("utf-8")]))
        != sha256(policy_preimage(b"unicode", ["e\u0301".encode("utf-8")])),
        "opaque Unicode policy IDs were normalized",
    )
    mutation_checks += 1

    baseline_receipt = baseline["selectionReceiptDigestSha256"]
    receipt_inputs = baseline["receiptInputs"]

    def receipt_changed(label: str, mutate: Any) -> None:
        nonlocal mutation_checks
        candidate = copy.deepcopy(receipt_inputs)
        kwargs: dict[str, Any] = {}
        mutate(candidate, kwargs)
        observed = sha256(receipt_preimage(**candidate, **kwargs))
        require(observed != baseline_receipt,
                f"receipt mutation survived identity oracle: {label}")
        mutation_checks += 1

    receipt_changed(
        "receipt separator",
        lambda _value, kwargs: kwargs.__setitem__(
            "separator", RECEIPT_SEPARATOR[:-2] + b"2\0"
        ),
    )
    receipt_changed(
        "receipt kind",
        lambda _value, kwargs: kwargs.__setitem__("kind", RECEIPT_KIND[:-1] + b"2"),
    )
    for label, key in (
        ("evaluation ID", "evaluation_id"),
        ("relation digest", "relation_set_digest"),
        ("policy digest", "policy_digest"),
    ):
        receipt_changed(
            label,
            lambda value, _kwargs, key=key: value.__setitem__(
                key, bytes([value[key][0] ^ 1]) + value[key][1:]
            ),
        )
    receipt_changed(
        "proof SHA",
        lambda _value, kwargs: kwargs.__setitem__(
            "proof_sha256",
            bytes([feasibility.PROOF_SHA256[0] ^ 1])
            + feasibility.PROOF_SHA256[1:],
        ),
    )
    receipt_changed(
        "selected ordinal",
        lambda value, _kwargs: value.__setitem__(
            "selected_policy_ordinal", value["selected_policy_ordinal"] + 1
        ),
    )
    receipt_changed(
        "selected candidate ID",
        lambda value, _kwargs: value.__setitem__("selected_candidate_id", b"changed"),
    )
    receipt_changed(
        "selected emitted RGB",
        lambda value, _kwargs: value.__setitem__("selected_emitted", b"\x74\x75\x75"),
    )
    for label, key in (
        ("profile key", "profile_key"),
        ("artifact key", "artifact_key"),
        ("bound key", "bound_key"),
        ("proof key", "proof_key"),
    ):
        receipt_changed(
            label,
            lambda _value, kwargs, key=key: kwargs.__setitem__(
                key, getattr(feasibility, key.upper()) + b"-changed"
            ),
        )
    # Count and records must mutate together to remain structurally valid.
    receipt_changed(
        "verified edge count and appended record",
        lambda value, _kwargs: (
            value["relations"][0]["edges"].append(
                {
                    **copy.deepcopy(value["relations"][0]["edges"][-1]),
                    "edgeOrdinal": value["verified_applicable_edges"],
                }
            ),
            value.__setitem__(
                "verified_applicable_edges", value["verified_applicable_edges"] + 1
            ),
        ),
    )
    receipt_changed(
        "omitted edge record and count",
        lambda value, _kwargs: (
            value["relations"][0]["edges"].pop(),
            value.__setitem__(
                "verified_applicable_edges", value["verified_applicable_edges"] - 1
            ),
        ),
    )
    receipt_changed(
        "duplicated edge payload",
        lambda value, _kwargs: value["relations"][0]["edges"].__setitem__(
            1,
            {
                **copy.deepcopy(value["relations"][0]["edges"][0]),
                "edgeOrdinal": 1,
            },
        ),
    )
    for label, key, replacement in (
        ("relation ordinal", "relationOrdinal", 7),
        ("relation ID", "relationId", b"other"),
        ("relation criterion", "criterion", b"other-criterion"),
    ):
        receipt_changed(
            label,
            lambda value, _kwargs, key=key, replacement=replacement: value[
                "relations"
            ][0].__setitem__(key, replacement),
        )
    for label, key, replacement in (
        ("edge ordinal", "edgeOrdinal", 7),
        ("edge foreground", "foreground", b"\x74\x75\x75"),
        ("edge background", "background", b"\x01\x00\x00"),
    ):
        receipt_changed(
            label,
            lambda value, _kwargs, key=key, replacement=replacement: value[
                "relations"
            ][0]["edges"][0].__setitem__(key, replacement),
        )
    invalid_decision = copy.deepcopy(receipt_inputs)
    invalid_decision["relations"][0]["edges"][0]["decisionTag"] = 2
    try:
        receipt_preimage(**invalid_decision)
    except ValueError:
        mutation_checks += 1
    else:
        raise ValueError("non-Pass edge decision survived the V1 receipt grammar")
    receipt_changed(
        "edge order",
        lambda value, _kwargs: value["relations"][0]["edges"].reverse(),
    )

    # Input permutations are canonicalization invariants inherited from A.
    permuted = copy.deepcopy(model)
    permuted["candidates"].reverse()
    require(
        evaluate_policy(permuted, policy_cases()[0])["selectionReceiptDigestSha256"]
        == baseline_receipt,
        "declared domain permutation changed selection receipt",
    )
    invariance_checks += 1
    permuted = copy.deepcopy(model)
    permuted["relations"].reverse()
    permuted["relations"][0]["adjacent"].reverse()
    require(
        evaluate_policy(permuted, policy_cases()[0])["selectionReceiptDigestSha256"]
        == baseline_receipt,
        "relation or adjacency declaration permutation changed selection receipt",
    )
    invariance_checks += 1
    require(
        evaluate_policy(model, policy_cases()[0])["policyDigestSha256"]
        == baseline_policy_digest,
        "identical policy was not deterministic",
    )
    invariance_checks += 1
    other_identity = evaluate_policy(model, policy_cases()[1])
    require(
        other_identity["selectedEmitted"] == baseline["selectedEmitted"]
        and other_identity["selectionReceiptDigestSha256"] != baseline_receipt,
        "same emitted bytes under different selected IDs collapsed receipt identity",
    )
    invariance_checks += 1
    no_selection = evaluate_policy(model, policy_cases()[2])
    require(
        no_selection["outcome"] == "noSelection"
        and no_selection["selectionReceiptDigestSha256"] is None,
        "NoSelection unexpectedly minted a final receipt",
    )
    invariance_checks += 1
    changed_physical = copy.deepcopy(model)
    changed_physical["candidates"][1]["emitted"] = [118, 118, 118]
    require(
        evaluate_policy(changed_physical, policy_cases()[0])["policyDigestSha256"]
        == baseline_policy_digest,
        "physical candidate bytes leaked into client policy identity",
    )
    invariance_checks += 1

    for label, order in (
        ("foreign tail", ["é", "foreign"]),
        ("duplicate tail", ["é", "e\u0301", "é"]),
    ):
        try:
            evaluate_policy(
                model,
                {"policyId": "invalid", "orderedCandidateIds": order},
            )
        except ValueError:
            rejection_checks += 1
        else:
            raise ValueError(f"invalid policy survived full-tail validation: {label}")

    return mutation_checks, invariance_checks, rejection_checks


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
    require(
        actual == expected,
        "explicit selection identity fixture drift; run --print and review exact bytes",
    )
    payload = json.loads(actual)
    mutations, invariants, rejections = (
        mutation_self_tests() if args.self_test else (0, 0, 0)
    )
    policies = payload["fixture"]["policies"]
    print(
        "WCAG22 explicit selection identity oracle: PASS; "
        f"policy={policies[0]['policyDigestSha256']}; "
        f"receipt={policies[0]['selectionReceiptDigestSha256']}; "
        f"fixture_sha256={sha256(actual).hex()}; "
        f"mutation_checks={mutations}; "
        f"invariance_checks={invariants}; "
        f"rejection_checks={rejections}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        raise SystemExit(
            f"WCAG22 explicit selection identity oracle: {error}"
        ) from error
