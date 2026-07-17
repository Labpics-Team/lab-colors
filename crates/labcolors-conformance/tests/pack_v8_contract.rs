//! Pack-контракт: pack 8 удаляет ровно `wcag22-explicit-selection.json`
//! (roadmap C4a — параллельная explicit/atomic операция вырезана), сохраняя
//! байт-в-байт все семь оставшихся семейств pack 7.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use labcolors_conformance::{FAMILY_FILES, MANIFEST_FILE, PACK_VERSION};

#[allow(dead_code)]
#[path = "../../labcolors-core/src/sha256.rs"]
mod sha256;

const UNCHANGED_FAMILY_SHA256: [(&str, &str); 7] = [
    (
        "solve.json",
        "db04e50698cc3b10223f4005f74dd35cc5ae0a29988825e44db5c985aa9207af",
    ),
    (
        "contrasts.json",
        "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145",
    ),
    (
        "ladders.json",
        "496f562e55ad8110aeb8a07042b1964ec9ff4d0f1e8c09e362d1b2d14c513036",
    ),
    (
        "alpha.json",
        "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1",
    ),
    (
        "muddiness.json",
        "3c5497b251f04c089d33452b9bf0bfba7f4ef9a72dc496180ff42aad08377aa3",
    ),
    (
        "wcag22.json",
        "6e234fa3a0d4e2b21f515b8f4e6be76f223768821e0308e774c31a5ce7a1d826",
    ),
    (
        "wcag22-feasibility.json",
        "ae2caec47a7b650e73b8d4029a69b4e401dfb7cc199db579c0f95106eebe8dc3",
    ),
];

const REQUIRED_CASES: [&str; 13] = [
    "text-default-seven",
    "text-default-two",
    "text-default-zero",
    "text-large-scale-ninety-two",
    "ui-component-ninety-two",
    "graphical-object-ninety-two",
    "ui-component-fifty-nine",
    "mixed-not-applicable",
    "all-not-applicable",
    "conflicting-relation-id",
    "raw-adjacent-resource-rejection",
    "opaque-identity-a",
    "opaque-identity-b",
];

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("conformance")
        .join("vectors")
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    std::fs::read(path.as_ref())
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.as_ref().display()))
}

fn feasibility_vectors() -> Vec<serde_json::Value> {
    serde_json::from_slice(&read(vectors_dir().join("wcag22-feasibility.json")))
        .expect("valid feasibility family JSON")
}

fn vector<'a>(vectors: &'a [serde_json::Value], case_id: &str) -> &'a serde_json::Value {
    vectors
        .iter()
        .find(|vector| vector["caseId"] == case_id)
        .unwrap_or_else(|| panic!("missing case {case_id}"))
}

fn outcome(vector: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(
        vector["outcomeJson"]
            .as_str()
            .expect("canonical outcome JSON string"),
    )
    .expect("outcome protocol JSON")
}

fn evaluated_result(outcome: &serde_json::Value) -> &serde_json::Value {
    assert_eq!(outcome["outcome"], "success");
    assert!(matches!(
        outcome["feasibility"]["status"].as_str(),
        Some("feasible" | "infeasible")
    ));
    &outcome["feasibility"]["result"]
}

fn partition_count(outcome: &serde_json::Value) -> u32 {
    evaluated_result(outcome)["proof"]["partition"]
        .as_array()
        .expect("packed partition")
        .iter()
        .map(|byte| {
            u8::try_from(byte.as_u64().expect("partition byte"))
                .expect("partition byte range")
                .count_ones()
        })
        .sum()
}

#[test]
fn pack_v8_removes_only_the_explicit_selection_family() {
    assert_eq!(PACK_VERSION, "8.0.0");
    assert_eq!(
        FAMILY_FILES.as_slice(),
        [
            "contrasts.json",
            "ladders.json",
            "alpha.json",
            "solve.json",
            "muddiness.json",
            "wcag22.json",
            "wcag22-feasibility.json",
        ]
        .as_slice()
    );

    let dir = vectors_dir();
    for (name, expected) in UNCHANGED_FAMILY_SHA256 {
        assert_eq!(
            sha256::digest(&read(dir.join(name))).to_hex(),
            expected,
            "pack-7 family bytes drifted during the C4a removal: {name}"
        );
    }
    assert!(
        !dir.join("wcag22-explicit-selection.json").exists(),
        "C4a: the explicit-selection family must be gone, not regenerated"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&read(dir.join(MANIFEST_FILE))).expect("valid manifest JSON");
    assert_eq!(manifest["packVersion"], "8.0.0");
    assert_eq!(manifest["counts"]["wcag22Feasibility"], 13);
    assert!(
        manifest["counts"].get("wcag22ExplicitSelection").is_none(),
        "manifest must not carry the removed family count"
    );
    assert_eq!(manifest["counts"]["total"], 103);
}

#[test]
fn solve_failure_wire_is_exact_and_closed() {
    let vectors: Vec<serde_json::Value> =
        serde_json::from_slice(&read(vectors_dir().join("solve.json")))
            .expect("valid solve family JSON");
    let failures: Vec<_> = vectors
        .iter()
        .filter_map(|vector| {
            let outcome = &vector["outcome"];
            (outcome["kind"] == "failure").then_some(outcome)
        })
        .collect();
    assert!(
        !failures.is_empty(),
        "anti-vacuum: solve family has no failure"
    );
    let mut actual = BTreeSet::new();
    for failure in failures {
        let fields: BTreeSet<_> = failure
            .as_object()
            .expect("failure object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(fields, BTreeSet::from(["category", "code", "kind"]));
        actual.insert((
            failure["category"].as_str().expect("failure category"),
            failure["code"].as_str().expect("failure code"),
        ));
    }
    assert_eq!(
        actual,
        BTreeSet::from([
            ("unreachable", "below_contrast_floor"),
            ("unreachable", "exceeds_range"),
            ("unreachable", "floor_unreachable"),
        ])
    );
    assert!(vectors.iter().all(|vector| matches!(
        vector["outcome"]["kind"].as_str(),
        Some("solved" | "failure")
    )));
}

#[test]
fn feasibility_family_is_canonical_protocol_json_without_cell_graphs() {
    let vectors = feasibility_vectors();
    assert_eq!(vectors.len(), REQUIRED_CASES.len());

    let actual: BTreeSet<_> = vectors
        .iter()
        .map(|vector| {
            let object = vector.as_object().expect("vector object");
            assert_eq!(object.len(), 3, "vector schema must stay compact");
            let request = object["requestJson"]
                .as_str()
                .expect("canonical request JSON string");
            let outcome = object["outcomeJson"]
                .as_str()
                .expect("canonical outcome JSON string");
            serde_json::from_str::<serde_json::Value>(request).expect("request protocol JSON");
            serde_json::from_str::<serde_json::Value>(outcome).expect("outcome protocol JSON");
            for forbidden in [
                "feasibleCandidates",
                "infeasibleCandidates",
                "cells",
                "assessments",
            ] {
                assert!(
                    !outcome.contains(forbidden),
                    "outcome contains forbidden proportional view {forbidden}"
                );
            }
            object["caseId"].as_str().expect("stable case id")
        })
        .collect();
    assert_eq!(actual, BTreeSet::from(REQUIRED_CASES));
}

#[test]
fn feasibility_corpus_pins_terminals_counts_packing_and_opaque_identity_law() {
    let vectors = feasibility_vectors();
    for (case_id, expected) in [
        ("text-default-seven", 7),
        ("text-default-two", 2),
        ("text-default-zero", 0),
        ("text-large-scale-ninety-two", 92),
        ("ui-component-ninety-two", 92),
        ("graphical-object-ninety-two", 92),
        ("ui-component-fifty-nine", 59),
    ] {
        assert_eq!(
            partition_count(&outcome(vector(&vectors, case_id))),
            expected,
            "wrong feasible count in {case_id}"
        );
    }
    assert_eq!(
        outcome(vector(&vectors, "text-default-zero"))["feasibility"]["status"],
        "infeasible"
    );

    for value in &vectors {
        let outcome = outcome(value);
        if outcome["outcome"] != "success" || outcome["feasibility"]["status"] == "notEvaluated" {
            continue;
        }
        let result = evaluated_result(&outcome);
        assert_eq!(result["domain"].as_array().expect("domain").len(), 256);
        let proof = &result["proof"];
        let edges: usize = proof["applicableEdges"]
            .as_str()
            .expect("decimal edge count")
            .parse()
            .expect("numeric edge count");
        assert_eq!(
            result["failureMatrix"]
                .as_array()
                .expect("packed matrix")
                .len(),
            32 * edges
        );
        assert_eq!(
            proof["partition"]
                .as_array()
                .expect("packed partition")
                .len(),
            32
        );
        assert_eq!(
            proof["logicalAssessments"]
                .as_str()
                .expect("decimal assessment count")
                .parse::<usize>()
                .expect("numeric assessment count"),
            256 * edges
        );
    }

    let mixed = outcome(vector(&vectors, "mixed-not-applicable"));
    assert_eq!(partition_count(&mixed), 7);
    assert_eq!(
        mixed["feasibility"]["result"]["proof"]["applicableRelations"],
        "1"
    );
    assert_eq!(
        mixed["feasibility"]["result"]["proof"]["notApplicableRelations"],
        "1"
    );
    let all_na = outcome(vector(&vectors, "all-not-applicable"));
    assert_eq!(all_na["outcome"], "success");
    assert_eq!(all_na["feasibility"]["status"], "notEvaluated");
    assert!(all_na["feasibility"]["result"].get("proof").is_none());
    assert!(
        all_na["feasibility"]["result"]
            .get("failureMatrix")
            .is_none()
    );

    let conflict = outcome(vector(&vectors, "conflicting-relation-id"));
    assert_eq!(conflict["outcome"], "failure");
    assert!(conflict.get("feasibility").is_none());
    assert_eq!(conflict["error"]["source"], "core");
    assert_eq!(conflict["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        conflict["error"]["error"]["details"]["code"],
        "conflictingRelationId"
    );
    let resource = outcome(vector(&vectors, "raw-adjacent-resource-rejection"));
    assert_eq!(resource["outcome"], "failure");
    assert!(resource.get("feasibility").is_none());
    assert_eq!(resource["error"]["error"]["code"], "resourceLimitExceeded");
    assert_eq!(
        resource["error"]["error"]["details"]["dimension"],
        "rawAdjacentEntries"
    );
    assert_eq!(resource["error"]["error"]["details"]["requested"], "2048");
    assert_eq!(resource["error"]["error"]["details"]["limit"], "2047");

    let opaque_a = outcome(vector(&vectors, "opaque-identity-a"));
    let opaque_b = outcome(vector(&vectors, "opaque-identity-b"));
    let a = evaluated_result(&opaque_a);
    let b = evaluated_result(&opaque_b);
    assert_eq!(a["failureMatrix"], b["failureMatrix"]);
    assert_eq!(a["proof"]["partition"], b["proof"]["partition"]);
    assert_eq!(a["proof"]["matrixDigest"], b["proof"]["matrixDigest"]);
    assert_ne!(
        a["proof"]["relationSetDigest"],
        b["proof"]["relationSetDigest"]
    );
    assert_ne!(a["proof"]["evaluationId"], b["proof"]["evaluationId"]);
}

// ── Законы новой atomic explicit-selection family ────────────────────────────
