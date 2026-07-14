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
    FirstFeasibleInDeclaredOrderV1, InvalidSelectionRequestV1, NoSelectionReasonV1, NoSelectionV1,
    PolicyId, SelectedV1, SelectionErrorV1, SelectionOutcomeV1, select,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainRequestV1, EvaluatedV1, FeasibilityV1, RequestV1, evaluate,
};
use labcolors_core::wcag22_feasibility::{
    OccurrenceId, RelationId, RelationV1, ResourceDimensionV1, ResourceProfileIdV1,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod fixture_sha256;

const PROFILE: ResourceProfileIdV1 = ResourceProfileIdV1::Compile;
const IDENTITY_FIXTURE: &str =
    include_str!("../contracts/wcag22-explicit-selection-identity-v1.json");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

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

fn as_selected(outcome: &SelectionOutcomeV1) -> Option<&SelectedV1> {
    match outcome {
        SelectionOutcomeV1::Selected { selected, .. } => Some(selected),
        SelectionOutcomeV1::NoSelection { .. } => None,
    }
}

fn as_no_selection(outcome: &SelectionOutcomeV1) -> Option<&NoSelectionV1> {
    match outcome {
        SelectionOutcomeV1::Selected { .. } => None,
        SelectionOutcomeV1::NoSelection { no_selection, .. } => Some(no_selection),
    }
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
fn empty_policy_shapes_are_rejected_before_selection() {
    assert!(matches!(
        PolicyId::try_new(""),
        Err(InvalidSelectionRequestV1::EmptyPolicyId)
    ));
    assert!(matches!(
        FirstFeasibleInDeclaredOrderV1::try_new(PolicyId::try_new("policy").unwrap(), Vec::new(),),
        Err(InvalidSelectionRequestV1::EmptyCandidateOrder)
    ));
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
        policy("brand/order", &["first", "second"]),
    )
    .unwrap();
    let second = select(
        feasibility.selection_source().unwrap(),
        policy("brand/order", &["second", "first"]),
    )
    .unwrap();
    let first = as_selected(&first).expect("first order selects");
    let second = as_selected(&second).expect("second order selects");

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
fn first_feasible_after_an_infeasible_prefix_keeps_its_declared_ordinal() {
    let feasibility = compile(
        vec![candidate("fail", [0; 3]), candidate("pass", [255; 3])],
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );

    let outcome = select(
        feasibility.selection_source().unwrap(),
        policy("ordered", &["fail", "pass"]),
    )
    .unwrap();
    let selected = as_selected(&outcome).expect("the second declared ID passes");

    assert_eq!(selected.candidate().candidate_id().as_str(), "pass");
    assert_eq!(selected.proof().selected_policy_ordinal(), 1);
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
    let no_selection = as_no_selection(&outcome).expect("a valid singleton may select nothing");

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
        vec![
            candidate("first", [255; 3]),
            candidate("second", [254; 3]),
            candidate("third", [253; 3]),
        ],
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
        vec![candidate("x", [255; 3]), candidate("y", [254; 3])],
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
        policy(&"p".repeat(65_530), &["x", "foreign"]),
    )
    .expect_err("over-limit policy bytes must fail before foreign-ID lookup");
    assert!(matches!(
        over_limit,
        SelectionErrorV1::ResourceLimitExceeded {
            profile_id: ResourceProfileIdV1::Compile,
            dimension: ResourceDimensionV1::OpaqueUtf8Bytes,
            requested: 65_538,
            limit: 65_536,
        }
    ));
}

#[test]
fn policy_cardinality_accepts_the_domain_size_and_rejects_size_plus_one_first() {
    let ids = ["candidate/0", "candidate/1"];
    let feasibility = compile(
        ids.iter().map(|id| candidate(id, [255; 3])).collect(),
        vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
    );

    assert!(
        select(
            feasibility.selection_source().unwrap(),
            policy("count-bound", &ids),
        )
        .is_ok(),
        "P equal to the finite domain cardinality must remain valid",
    );

    let error = select(
        feasibility.selection_source().unwrap(),
        policy("count-bound", &[ids[0], ids[1], ids[0]]),
    )
    .expect_err("P = C + 1 must fail before duplicate lookup");
    assert!(matches!(
        error,
        SelectionErrorV1::InvalidRequest(
            InvalidSelectionRequestV1::PolicyCardinalityExceedsDomain {
                requested: 3,
                domain: 2,
            }
        )
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
    let selected = as_selected(&outcome).unwrap();
    assert_eq!(selected.proof().selected_policy_ordinal(), 0);
    assert_eq!(selected.final_verification().verified_applicable_edges(), 2);
    assert_eq!(selected.evaluation_id(), record.evaluation_id());
    assert_eq!(
        selected.final_verification().profile_id(),
        record.proof().profile_id()
    );
    assert_eq!(
        selected.final_verification().artifact_id(),
        record.proof().artifact_id()
    );
    assert_eq!(
        selected.final_verification().bound_id(),
        record.proof().bound_id()
    );
    assert_eq!(
        selected.final_verification().proof_id(),
        record.proof().proof_id()
    );
    assert_eq!(
        selected.final_verification().proof_sha256(),
        record.proof().proof_sha256()
    );
    assert_eq!(
        selected.final_verification().relation_set_digest(),
        record.relation_set_digest()
    );
}

#[test]
fn property_selection_equals_an_independent_declared_order_lsb0_oracle() {
    const CANDIDATES: usize = 17;
    check_property(
        (
            prop::collection::vec(any::<u32>(), CANDIDATES),
            prop::collection::vec(any::<bool>(), CANDIDATES),
        ),
        |(keys, included)| {
            let ids = (0..CANDIDATES)
                .map(|index| format!("candidate/{index:02}"))
                .collect::<Vec<_>>();
            let feasibility = compile(
                (0..CANDIDATES)
                    .map(|index| {
                        let value = match index {
                            7 => 0x75,
                            8 => 0x76,
                            16 => 0xff,
                            _ => 0x00,
                        };
                        candidate(&ids[index], [value; 3])
                    })
                    .collect(),
                vec![applicable("contrast", vec![Srgb8::new([0; 3])])],
            );
            let record = evaluated(&feasibility);
            let proof = record.proof();
            prop_assert_eq!(proof.partition(), &[0x80, 0x01, 0x01]);
            let mut ordinals = (0..CANDIDATES)
                .filter(|index| included[*index])
                .collect::<Vec<_>>();
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
            let actual =
                as_selected(&outcome).map(|selected| selected.candidate().candidate_id().as_str());

            prop_assert_eq!(actual, expected);
            prop_assert_eq!(as_no_selection(&outcome).is_some(), expected.is_none());
            Ok(())
        },
    );
}

#[test]
fn production_identity_matches_the_independent_unicode_selection_fixture() {
    assert_eq!(
        fixture_sha256::digest(IDENTITY_FIXTURE.as_bytes()).to_hex(),
        "ca6c9c83a87a400655ea6cbdc4efcfb36176095c082ec9342bda08ba7dfed955"
    );
    let applicable = RelationV1::applicable(
        relation_id("alpha"),
        occurrence_id("hover/🎨"),
        Wcag22CriterionV1::Sc143TextDefault,
        vec![
            Srgb8::new([255; 3]),
            Srgb8::new([0; 3]),
            Srgb8::new([255; 3]),
        ],
    )
    .unwrap();
    let not_applicable = RelationV1::not_applicable(
        relation_id("zeta"),
        occurrence_id("ornament"),
        Wcag22ClientDeclaredNotApplicableV1::try_new("client/не-применимо").unwrap(),
    );
    let feasibility = compile(
        vec![
            candidate("海", [0; 3]),
            candidate("é", [117; 3]),
            candidate("e\u{301}", [117; 3]),
        ],
        vec![not_applicable, applicable],
    );
    let record = evaluated(&feasibility);

    assert_eq!(
        record
            .candidates()
            .iter()
            .map(|value| value.candidate_id().as_str())
            .collect::<Vec<_>>(),
        ["e\u{301}", "é", "海"]
    );
    assert_eq!(record.failure_matrix(), [0x10]);
    assert_eq!(record.proof().partition(), [0x03]);
    assert_eq!(
        hex(record.domain_digest().as_bytes()),
        "9c99082645e713daf56f65e15012d21e80b3b491763d28ec7ddf8fa966164f17"
    );
    assert_eq!(
        hex(record.relation_set_digest().as_bytes()),
        "f163238ded41b3a5e7e181153a2fe48530d1a9426bf32737d52f571842ce7a3e"
    );
    assert_eq!(
        hex(record.evaluation_id().as_bytes()),
        "4d93a22b27f2e9a6241f6f4a93e83c497c1a6162ddebd958febdc4277bb9adee"
    );

    let composed = select(
        feasibility.selection_source().unwrap(),
        policy("brand/выбор/🎨", &["海", "é", "e\u{301}"]),
    )
    .unwrap();
    let composed = as_selected(&composed).expect("fixture policy selects");
    assert_eq!(composed.candidate().candidate_id().as_str(), "é");
    assert_eq!(composed.candidate().emitted(), Srgb8::new([117; 3]));
    assert_eq!(composed.proof().selected_policy_ordinal(), 1);
    assert_eq!(
        hex(composed.policy_digest().as_bytes()),
        "60f67cfa7931a33f7968740343936e52209abbf322525933c83c919eac61f4d7"
    );
    assert_eq!(
        hex(composed.receipt_digest().as_bytes()),
        "3adbc3d926e75cc719eb9f3c31442e9cc2c272dfc175d0f942ef60423ee6d538"
    );
    assert_eq!(
        composed.proof().receipt_digest(),
        composed.final_verification().receipt_digest()
    );
    assert_eq!(composed.final_verification().verified_applicable_edges(), 2);

    let decomposed = select(
        feasibility.selection_source().unwrap(),
        policy("brand/выбор/🎨", &["海", "e\u{301}", "é"]),
    )
    .unwrap();
    let decomposed = as_selected(&decomposed).expect("opposite order selects");
    assert_eq!(decomposed.candidate().candidate_id().as_str(), "e\u{301}");
    assert_eq!(decomposed.candidate().emitted(), Srgb8::new([117; 3]));
    assert_eq!(decomposed.proof().selected_policy_ordinal(), 1);
    assert_eq!(
        hex(decomposed.policy_digest().as_bytes()),
        "422bc9682e4829d6155ea319cf79ca2198c23373760763baee84057f38ccca25"
    );
    assert_eq!(
        hex(decomposed.receipt_digest().as_bytes()),
        "4859d800ff978319bdb0b84a6efe31090bf6ca0bc0d0090745fd53228e0ca991"
    );

    let no_selection = select(
        feasibility.selection_source().unwrap(),
        policy("brand/выбор/🎨", &["海"]),
    )
    .unwrap();
    let no_selection =
        as_no_selection(&no_selection).expect("infeasible singleton does not receive a fallback");
    assert_eq!(
        hex(no_selection.policy_digest().as_bytes()),
        "25906476eb6f6baf6378f0d421ea953291de5fa33d8d7733313b353660756c62"
    );
}

#[test]
fn selection_source_and_receipts_cannot_be_forged_or_rewrapped_downstream() {
    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::EvaluatedV1;
use labcolors_core::wcag22_feasibility::explicit::selection::FeasibleSelectionSourceV1;

fn forge_source(record: &EvaluatedV1) -> FeasibleSelectionSourceV1<'_> {
    FeasibleSelectionSourceV1 { record }
}

fn main() {}
"#,
        &["FeasibleSelectionSourceV1", "private"],
    );

    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::selection::{
    FinalRelationVerificationV1, SelectionProofV1,
};

fn main() {
    let _proof = SelectionProofV1 {};
    let _verification = FinalRelationVerificationV1 {};
}
"#,
        &["SelectionProofV1", "FinalRelationVerificationV1", "private"],
    );

    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::EvaluatedV1;
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FirstFeasibleInDeclaredOrderV1, select,
};

fn bypass_source(record: &EvaluatedV1, policy: FirstFeasibleInDeclaredOrderV1) {
    let _ = select(record, policy);
}

fn main() {}
"#,
        &["FeasibleSelectionSourceV1", "&EvaluatedV1"],
    );

    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::selection::{
    NoSelectionV1, SelectedV1, SelectionOutcomeV1,
};

fn wrap_selected(value: SelectedV1) -> SelectionOutcomeV1 {
    SelectionOutcomeV1::Selected { selected: value }
}

fn wrap_no_selection(value: NoSelectionV1) -> SelectionOutcomeV1 {
    SelectionOutcomeV1::NoSelection { no_selection: value }
}

fn main() {}
"#,
        &["Selected", "NoSelection"],
    );
}
