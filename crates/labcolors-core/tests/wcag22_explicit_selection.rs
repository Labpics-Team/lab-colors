//! Public RED contract for client-owned explicit selection (#296-B).
//!
//! Feasibility remains the sealed physical fact from #296-A. This target owns
//! only the client-declared order, complete request validation and the final
//! selected-row recheck through the existing #284 evaluator.

use std::fs;
use std::process::Command;

use labcolors_core::Srgb8;
use labcolors_core::wcag22::{Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1};
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FirstFeasibleInDeclaredOrderV1, InvalidSelectionRequestV1, NoSelectionReasonV1, PolicyId,
    SelectionErrorV1, select,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainRequestV1, EvaluatedV1, FeasibilityV1, RequestV1, evaluate,
};
use labcolors_core::wcag22_feasibility::{
    OccurrenceId, RelationId, RelationV1, ResourceDimensionV1, ResourceProfileIdV1,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

const PROFILE: ResourceProfileIdV1 = ResourceProfileIdV1::Compile;

fn candidate(id: &str, emitted: [u8; 3]) -> CandidateV1 {
    CandidateV1::new(
        CandidateId::try_new(id).expect("test candidate ID is non-empty"),
        Srgb8::new(emitted),
    )
}

fn relation_id(value: &str) -> RelationId {
    RelationId::try_new(value).expect("test relation ID is non-empty")
}

fn occurrence_id(value: &str) -> OccurrenceId {
    OccurrenceId::try_new(value).expect("test occurrence ID is non-empty")
}

fn applicable(id: &str, adjacent: Vec<Srgb8>) -> RelationV1 {
    RelationV1::applicable(
        relation_id(id),
        occurrence_id("occurrence"),
        Wcag22CriterionV1::Sc143TextDefault,
        adjacent,
    )
    .expect("test relation has adjacency")
}

fn request(candidates: Vec<CandidateV1>, relations: Vec<RelationV1>) -> RequestV1 {
    RequestV1::try_new(
        DomainRequestV1::try_new(candidates).expect("test domain is non-empty"),
        relations,
        PROFILE,
    )
    .expect("test request has relations")
}

fn evaluated(result: &FeasibilityV1) -> &EvaluatedV1 {
    result.evaluated().expect("expected an evaluated terminal")
}

fn policy(id: &str, order: &[&str]) -> FirstFeasibleInDeclaredOrderV1 {
    FirstFeasibleInDeclaredOrderV1::try_new(
        PolicyId::try_new(id).expect("test policy ID is non-empty"),
        order
            .iter()
            .map(|value| CandidateId::try_new(*value).expect("test order ID is non-empty"))
            .collect(),
    )
    .expect("test order is non-empty")
}

fn compile(candidates: Vec<CandidateV1>, relations: Vec<RelationV1>) -> FeasibilityV1 {
    evaluate(request(candidates, relations)).expect("test feasibility compiles")
}

fn check_property<S: Strategy>(strategy: S, body: impl Fn(S::Value) -> Result<(), TestCaseError>)
where
    S::Value: std::fmt::Debug,
{
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 64,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&strategy, body)
        .expect("selection property failed with a minimized counterexample");
}

fn assert_downstream_rejected(source: &str, expected_fragments: &[&str]) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    let package_dir = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        temp.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"forge-explicit-selection\"\nversion = \"0.0.0\"\n\
             edition = \"2024\"\n\n[dependencies]\n\
             labcolors-core = {{ path = \"{package_dir}\" }}\n"
        ),
    )
    .unwrap();
    fs::write(temp.path().join("src/main.rs"), source).unwrap();

    let output = Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .env("CARGO_TARGET_DIR", temp.path().join("target"))
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "forged selection unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for fragment in expected_fragments {
        assert!(
            stderr.contains(fragment),
            "expected downstream rejection mentioning {fragment:?}, stderr:\n{stderr}"
        );
    }
}

#[test]
fn only_feasible_terminal_mints_a_selection_source() {
    let feasible = compile(
        vec![candidate("pass", [255; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );
    let infeasible = compile(
        vec![candidate("fail", [0; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );
    let not_evaluated = compile(
        vec![candidate("ornament", [1, 2, 3])],
        vec![RelationV1::not_applicable(
            relation_id("ornament"),
            occurrence_id("decorative"),
            Wcag22ClientDeclaredNotApplicableV1::try_new("client-declared").unwrap(),
        )],
    );

    assert!(feasible.selection_source().is_some());
    assert!(infeasible.selection_source().is_none());
    assert!(not_evaluated.selection_source().is_none());
}

#[test]
fn opposite_declared_orders_choose_opposite_opaque_ids_without_rewriting_feasibility() {
    let feasibility = compile(
        vec![
            candidate("first", [0x75; 3]),
            candidate("second", [0x75; 3]),
        ],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );
    let record = evaluated(&feasibility);
    let matrix_before = record.failure_matrix().to_vec();
    let partition_before = record.proof().partition().to_vec();
    let evaluation_before = record.evaluation_id();

    let first = select(
        feasibility.selection_source().unwrap(),
        policy("brand/order-a", &["first", "second"]),
    )
    .unwrap();
    let second = select(
        feasibility.selection_source().unwrap(),
        policy("brand/order-b", &["second", "first"]),
    )
    .unwrap();
    let first = first.selected().expect("first order selects");
    let second = second.selected().expect("second order selects");

    assert_eq!(first.candidate().candidate_id().as_str(), "first");
    assert_eq!(second.candidate().candidate_id().as_str(), "second");
    assert_eq!(first.candidate().emitted(), second.candidate().emitted());
    assert_ne!(first.policy_digest(), second.policy_digest());
    assert_eq!(first.evaluation_id(), evaluation_before);
    assert_eq!(second.evaluation_id(), evaluation_before);
    assert_eq!(record.failure_matrix(), matrix_before);
    assert_eq!(record.proof().partition(), partition_before);
    assert_eq!(record.evaluation_id(), evaluation_before);
}

#[test]
fn singleton_infeasible_policy_is_real_no_selection_without_domain_fallback() {
    let feasibility = compile(
        vec![candidate("fail", [0; 3]), candidate("pass", [255; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );
    assert!(
        feasibility.is_feasible(),
        "the whole domain has a feasible member"
    );

    let outcome = select(
        feasibility.selection_source().unwrap(),
        policy("exact/fail-only", &["fail"]),
    )
    .unwrap();
    let no_selection = outcome
        .no_selection()
        .expect("a valid singleton may select nothing");

    assert_eq!(
        no_selection.reason(),
        NoSelectionReasonV1::NoDeclaredCandidateFeasible
    );
    assert_eq!(
        no_selection.evaluation_id(),
        evaluated(&feasibility).evaluation_id()
    );
    assert_eq!(no_selection.policy_id().as_str(), "exact/fail-only");
}

#[test]
fn feasible_prefix_never_hides_a_foreign_or_duplicate_tail() {
    let feasibility = compile(
        vec![candidate("first", [255; 3]), candidate("second", [254; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );

    let foreign = select(
        feasibility.selection_source().unwrap(),
        policy("invalid/foreign-tail", &["first", "foreign"]),
    )
    .expect_err("foreign tail invalidates the whole policy");
    assert!(matches!(
        foreign,
        SelectionErrorV1::InvalidRequest(InvalidSelectionRequestV1::ForeignCandidateId {
            candidate_id,
        }) if candidate_id.as_str() == "foreign"
    ));

    let duplicate = select(
        feasibility.selection_source().unwrap(),
        policy("invalid/duplicate-tail", &["first", "second", "first"]),
    )
    .expect_err("duplicate tail invalidates the whole policy");
    assert!(matches!(
        duplicate,
        SelectionErrorV1::InvalidRequest(InvalidSelectionRequestV1::DuplicateCandidateId {
            candidate_id,
        }) if candidate_id.as_str() == "first"
    ));
}

#[test]
fn policy_resource_preflight_is_exact_and_precedes_semantic_lookup() {
    let feasibility = compile(
        vec![candidate("x", [255; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );
    let at_limit = select(
        feasibility.selection_source().unwrap(),
        policy(&"p".repeat(65_535), &["x"]),
    );
    assert!(
        at_limit.is_ok(),
        "policy ID plus order ID is exactly 65536 bytes"
    );

    let over_limit = select(
        feasibility.selection_source().unwrap(),
        policy(&"p".repeat(65_536), &["x"]),
    )
    .expect_err("65537 policy bytes must fail before cardinality or duplicate lookup");
    assert!(matches!(
        over_limit,
        SelectionErrorV1::ResourceLimitExceeded {
            profile_id: ResourceProfileIdV1::Compile,
            dimension: ResourceDimensionV1::OpaqueUtf8Bytes,
            requested: 65_537,
            limit: 65_536,
        }
    ));
}

#[test]
fn mixed_graph_final_receipt_counts_edges_not_relation_labels() {
    let feasibility = compile(
        vec![candidate("balanced", [0x75; 3])],
        vec![
            RelationV1::not_applicable(
                relation_id("ornament"),
                occurrence_id("decorative"),
                Wcag22ClientDeclaredNotApplicableV1::try_new("client-declared").unwrap(),
            ),
            applicable(
                "contrast",
                vec![
                    Srgb8::new([255; 3]),
                    Srgb8::new([0; 3]),
                    Srgb8::new([255; 3]),
                ],
            ),
        ],
    );
    let record = evaluated(&feasibility);
    assert_eq!(record.proof().canonical_relations(), 2);
    assert_eq!(record.proof().applicable_edges(), 2);

    let outcome = select(
        feasibility.selection_source().unwrap(),
        policy("mixed", &["balanced"]),
    )
    .unwrap();
    let selected = outcome.selected().unwrap();
    assert_eq!(selected.proof().selected_policy_ordinal(), 0);
    assert_eq!(selected.final_verification().verified_applicable_edges(), 2);
    assert_eq!(selected.evaluation_id(), record.evaluation_id());
    assert_eq!(
        selected.final_verification().profile_id(),
        record.proof().profile_id()
    );
}

#[test]
fn property_selection_equals_an_independent_declared_order_lsb0_oracle() {
    check_property(
        (
            prop::collection::vec(any::<u32>(), 8),
            prop::collection::vec(any::<bool>(), 8),
        ),
        |(keys, included)| {
            let ids = (0..8)
                .map(|index| format!("candidate/{index}"))
                .collect::<Vec<_>>();
            let feasibility = compile(
                [0_u8, 32, 64, 96, 117, 160, 200, 255]
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| candidate(&ids[index], [value; 3]))
                    .collect(),
                vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
            );
            let record = evaluated(&feasibility);
            let mut ordinals = (0..8).filter(|index| included[*index]).collect::<Vec<_>>();
            if ordinals.is_empty() {
                ordinals.push(0);
            }
            ordinals.sort_unstable_by_key(|index| (keys[*index], *index));
            let order = ordinals
                .iter()
                .map(|index| ids[*index].as_str())
                .collect::<Vec<_>>();

            let expected = order.iter().find_map(|id| {
                let canonical_index = record
                    .candidates()
                    .iter()
                    .position(|candidate| candidate.candidate_id().as_str() == *id)
                    .expect("oracle order is a domain subset");
                let bit =
                    (record.proof().partition()[canonical_index / 8] >> (canonical_index % 8)) & 1;
                (bit == 1).then_some(*id)
            });
            let outcome = select(
                feasibility.selection_source().unwrap(),
                policy("property", &order),
            )
            .unwrap();
            let actual = outcome
                .selected()
                .map(|selected| selected.candidate().candidate_id().as_str());

            prop_assert_eq!(actual, expected);
            prop_assert_eq!(outcome.is_no_selection(), expected.is_none());
            Ok(())
        },
    );
}

#[test]
fn selection_source_and_receipts_cannot_be_forged_or_rewrapped_downstream() {
    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::EvaluatedV1;
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FeasibleSelectionSourceV1, FinalRelationVerificationV1, SelectionProofV1,
};

fn forge_source(record: &EvaluatedV1) -> FeasibleSelectionSourceV1<'_> {
    FeasibleSelectionSourceV1 { record }
}

fn main() {
    let _proof = SelectionProofV1 {};
    let _verification = FinalRelationVerificationV1 {};
}
"#,
        &[
            "FeasibleSelectionSourceV1",
            "private",
            "SelectionProofV1",
            "FinalRelationVerificationV1",
        ],
    );
}
