//! Pack-контракт: pack 6 добавляет ровно одну atomic explicit-selection family
//! (#296-C2), сохраняя байт-в-байт все семь допущенных pack-5 семейств.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use labcolors_conformance::{FAMILY_FILES, MANIFEST_FILE, PACK_VERSION};

#[allow(dead_code)]
#[path = "../../labcolors-core/src/sha256.rs"]
mod sha256;

const LEGACY_FAMILY_SHA256: [(&str, &str); 7] = [
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
        "solve.json",
        "64acfc4a8c613a4b11e4e83c52a33ecf308320abc6ab18fde20853a7f2399f06",
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
fn pack_v6_adds_only_the_explicit_selection_family_and_preserves_prior_bytes() {
    assert_eq!(PACK_VERSION, "6.0.0");
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
            "wcag22-explicit-selection.json",
        ]
        .as_slice()
    );

    let dir = vectors_dir();
    for (name, expected) in LEGACY_FAMILY_SHA256 {
        assert_eq!(
            sha256::digest(&read(dir.join(name))).to_hex(),
            expected,
            "pack-5 family bytes drifted: {name}"
        );
    }

    let manifest: serde_json::Value =
        serde_json::from_slice(&read(dir.join(MANIFEST_FILE))).expect("valid manifest JSON");
    assert_eq!(manifest["packVersion"], "6.0.0");
    assert_eq!(manifest["counts"]["wcag22Feasibility"], 13);
    assert_eq!(manifest["counts"]["wcag22ExplicitSelection"], 12);
    assert_eq!(manifest["counts"]["total"], 113);
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

const EXPLICIT_SELECTION_CASES: [&str; 12] = [
    "selected-declared-order-overrides-canonical",
    "selected-mixed-not-applicable",
    "no-selection-singleton-infeasible",
    "infeasible-policy-bound",
    "not-evaluated-policy-bound",
    "opposite-order-forward",
    "opposite-order-reverse",
    "error-foreign-tail-after-feasible-prefix",
    "error-duplicate-order-tail",
    "error-policy-cardinality-exceeds-domain",
    "error-unsupported-policy-kind",
    "opaque-unicode-identities",
];

fn explicit_selection_vectors() -> Vec<serde_json::Value> {
    serde_json::from_slice(&read(vectors_dir().join("wcag22-explicit-selection.json")))
        .expect("valid explicit-selection family JSON")
}

#[test]
fn explicit_selection_family_is_canonical_compact_protocol_json() {
    let vectors = explicit_selection_vectors();
    assert_eq!(vectors.len(), EXPLICIT_SELECTION_CASES.len());

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
                "domainFirst",
                "domainLast",
            ] {
                assert!(
                    !outcome.contains(forbidden),
                    "outcome contains forbidden view {forbidden}"
                );
            }
            object["caseId"].as_str().expect("stable case id")
        })
        .collect();
    assert_eq!(actual, BTreeSet::from(EXPLICIT_SELECTION_CASES));
}

#[test]
fn explicit_selection_corpus_pins_the_atomic_terminal_and_error_algebra() {
    let vectors = explicit_selection_vectors();

    // Selected: объявленный порядок перекрывает канонический; финальная
    // перепроверка отчитывается точным числом рёбер.
    let selected = outcome(vector(
        &vectors,
        "selected-declared-order-overrides-canonical",
    ));
    assert_eq!(selected["outcome"], "success");
    assert_eq!(selected["result"]["status"], "selected");
    assert_eq!(selected["result"]["selection"]["candidateId"], "z-bright");
    assert_eq!(
        selected["result"]["selection"]["selectedPolicyOrdinal"],
        "1"
    );
    assert_eq!(
        selected["result"]["selection"]["finalVerification"]["verifiedApplicableEdges"],
        "1"
    );
    let proof = &selected["result"]["feasibility"]["proof"];
    assert_eq!(proof["domainKind"], "explicit-srgb8-set-v1");
    assert_eq!(proof["candidateCount"], "3");
    // Переменная партиция: ceil(3/8) = 1 байт, без 256-байтного зазора.
    assert_eq!(proof["partition"].as_array().expect("partition").len(), 1);

    // Смешанный граф: NA-декларация сохраняется, рёбра считаются точно.
    let mixed = outcome(vector(&vectors, "selected-mixed-not-applicable"));
    assert_eq!(
        mixed["result"]["selection"]["finalVerification"]["verifiedApplicableEdges"],
        "2"
    );
    assert_eq!(
        mixed["result"]["feasibility"]["proof"]["notApplicableRelations"],
        "1"
    );

    // NoSelection: настоящий отказ с привязкой политики и evaluation.
    let no_selection = outcome(vector(&vectors, "no-selection-singleton-infeasible"));
    assert_eq!(no_selection["result"]["status"], "noSelection");
    assert_eq!(
        no_selection["result"]["selection"]["reason"],
        "noDeclaredCandidateFeasible"
    );
    assert_eq!(
        no_selection["result"]["selection"]["evaluationId"],
        no_selection["result"]["feasibility"]["proof"]["evaluationId"],
        "NoSelection must bind the exact source evaluation"
    );

    // Невыборные терминалы связывают точную политику без selection-receipt.
    let infeasible = outcome(vector(&vectors, "infeasible-policy-bound"));
    assert_eq!(infeasible["result"]["status"], "infeasible");
    assert_eq!(infeasible["result"]["policy"]["policyId"], "any-member");
    assert_eq!(infeasible["result"]["policy"]["declaredEntries"], "2");
    assert!(infeasible["result"].get("selection").is_none());

    let not_evaluated = outcome(vector(&vectors, "not-evaluated-policy-bound"));
    assert_eq!(not_evaluated["result"]["status"], "notEvaluated");
    assert_eq!(not_evaluated["result"]["policy"]["policyId"], "still-bound");
    assert!(not_evaluated["result"].get("selection").is_none());
    assert!(
        not_evaluated["result"]["feasibility"]
            .get("proof")
            .is_none(),
        "a declaration-only terminal must not fabricate numerical proof"
    );

    // Противоположные порядки: физика байт-идентична, выбор и binding меняются.
    let forward = outcome(vector(&vectors, "opposite-order-forward"));
    let reverse = outcome(vector(&vectors, "opposite-order-reverse"));
    assert_eq!(
        forward["result"]["feasibility"], reverse["result"]["feasibility"],
        "opposite declared orders must not rewrite the physical feasibility subtree"
    );
    assert_eq!(
        forward["result"]["selection"]["candidateId"],
        "first-bright"
    );
    assert_eq!(
        reverse["result"]["selection"]["candidateId"],
        "second-bright"
    );
    assert_ne!(
        forward["result"]["selection"]["policyDigest"],
        reverse["result"]["selection"]["policyDigest"]
    );

    // Ошибочные политики: malformed, не NoSelection; без частичного payload.
    for (case_id, code) in [
        (
            "error-foreign-tail-after-feasible-prefix",
            "foreignCandidateId",
        ),
        ("error-duplicate-order-tail", "duplicateCandidateId"),
        (
            "error-policy-cardinality-exceeds-domain",
            "policyCardinalityExceedsDomain",
        ),
    ] {
        let failure = outcome(vector(&vectors, case_id));
        assert_eq!(failure["outcome"], "failure", "{case_id}");
        assert!(failure.get("result").is_none(), "{case_id} leaked a result");
        assert_eq!(failure["error"]["source"], "selection", "{case_id}");
        assert_eq!(failure["error"]["error"]["code"], "invalidRequest");
        assert_eq!(failure["error"]["error"]["details"]["code"], code);
    }

    // Неизвестный policy kind — строгий декодер, typed transport.
    let unsupported = outcome(vector(&vectors, "error-unsupported-policy-kind"));
    assert_eq!(unsupported["error"]["source"], "transport");
    assert_eq!(
        unsupported["error"]["error"]["code"],
        "unsupportedPolicyKind"
    );
    assert_eq!(
        unsupported["error"]["error"]["received"],
        "best-feasible-v1"
    );

    // Unicode opaque ID проходят без нормализации; выбран первый feasible
    // из объявленного порядка.
    let unicode = outcome(vector(&vectors, "opaque-unicode-identities"));
    assert_eq!(unicode["result"]["status"], "selected");
    assert_eq!(
        unicode["result"]["selection"]["candidateId"], "z:\u{03bb}9",
        "the declared order starts with the infeasible dark member"
    );
}
