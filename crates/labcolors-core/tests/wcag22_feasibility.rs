//! Public contract for complete WCAG 2.2 enumeration over the registered
//! encoded-sRGB8 neutral axis (#295).
//!
//! This target uses only the public API. It does not import Q55 tables,
//! the private atomic evaluator, canonicalization helpers or solver code.

use std::fs;
use std::process::Command;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod fixture_sha256;

use labcolors_core::Srgb8;
use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1,
    wcag22_profile_v1,
};
use labcolors_core::wcag22_feasibility::{
    DomainIdV1, ErrorV1, EvaluatedV1, FeasibilityV1, InvalidRequestV1, NotEvaluatedV1,
    OccurrenceId, RelationId, RelationV1, RequestV1, ResourceDimensionV1, ResourceProfileIdV1,
    evaluate,
};

const DOMAIN: DomainIdV1 = DomainIdV1::Srgb8NeutralAxis;
const PROFILE: ResourceProfileIdV1 = ResourceProfileIdV1::Compile;
const ORACLE_FIXTURE: &str = include_str!("../contracts/wcag22-neutral-axis-oracle-v1.json");

fn colour(bytes: [u8; 3]) -> Srgb8 {
    Srgb8::new(bytes)
}

fn grey(value: u8) -> Srgb8 {
    colour([value; 3])
}

fn relation_id(value: &str) -> RelationId {
    RelationId::try_new(value).expect("non-empty opaque relation ID is valid")
}

fn occurrence_id(value: &str) -> OccurrenceId {
    OccurrenceId::try_new(value).expect("non-empty opaque occurrence ID is valid")
}

fn applicable_relation(
    relation: &str,
    occurrence: &str,
    adjacent: Vec<Srgb8>,
    criterion: Wcag22CriterionV1,
) -> RelationV1 {
    RelationV1::applicable(
        relation_id(relation),
        occurrence_id(occurrence),
        criterion,
        adjacent,
    )
    .expect("an applicable relation with physical adjacency is valid")
}

fn not_applicable_relation(relation: &str, occurrence: &str, reason: &str) -> RelationV1 {
    let declaration = Wcag22ClientDeclaredNotApplicableV1::try_new(reason)
        .expect("a non-empty client-owned reason is valid");
    RelationV1::not_applicable(
        relation_id(relation),
        occurrence_id(occurrence),
        declaration,
    )
}

fn request(relations: Vec<RelationV1>) -> RequestV1 {
    RequestV1::try_new(DOMAIN, relations, PROFILE).expect("a non-empty raw request is valid")
}

fn evaluated_record(result: &FeasibilityV1) -> &EvaluatedV1 {
    result
        .evaluated()
        .expect("expected an evaluated terminal payload")
}

fn not_evaluated_record(result: &FeasibilityV1) -> &NotEvaluatedV1 {
    result
        .not_evaluated()
        .expect("expected a NotEvaluated terminal payload")
}

fn feasible_bytes(result: &FeasibilityV1) -> Vec<[u8; 3]> {
    evaluated_record(result)
        .feasible_candidates()
        .map(Srgb8::bytes)
        .collect()
}

fn infeasible_bytes(result: &FeasibilityV1) -> Vec<[u8; 3]> {
    evaluated_record(result)
        .infeasible_candidates()
        .map(Srgb8::bytes)
        .collect()
}

fn exact_decision(
    result: &FeasibilityV1,
    candidate: Srgb8,
    relation_id: &RelationId,
    adjacent: Srgb8,
) -> Wcag22ApplicableDecisionV1 {
    let mut matching = evaluated_record(result).assessments().filter(|assessment| {
        assessment.candidate() == candidate
            && assessment.relation_id() == relation_id
            && assessment.adjacent() == adjacent
    });
    let decision = matching
        .next()
        .expect("the complete matrix must contain this exact cell")
        .decision();
    assert!(
        matching.next().is_none(),
        "the complete matrix must not duplicate a cell"
    );
    decision
}

fn single_relation_result(adjacent: Vec<Srgb8>, criterion: Wcag22CriterionV1) -> FeasibilityV1 {
    evaluate(request(vec![applicable_relation(
        "relation",
        "occurrence",
        adjacent,
        criterion,
    )]))
    .expect("the bounded complete evaluator must be total")
}

fn exact_grey_range(start: u8, end: u8) -> Vec<[u8; 3]> {
    (start..=end).map(|value| [value; 3]).collect()
}

fn check_property<S: Strategy>(
    cases: u32,
    strategy: S,
    body: impl Fn(S::Value) -> Result<(), TestCaseError>,
) where
    S::Value: std::fmt::Debug,
{
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    runner
        .run(&strategy, body)
        .expect("feasibility property failed with a minimized counterexample");
}

#[test]
fn production_vectors_are_bound_to_the_exact_independent_oracle_fixture() {
    assert_eq!(
        fixture_sha256::digest(ORACLE_FIXTURE.as_bytes()).to_hex(),
        "af56e71febf2994a186a7d4b1e51d5297263220f4adbe482d8c7a7f3b155f8b2"
    );
}

#[test]
fn sealed_proof_exposes_every_bound_fact_without_exposing_a_constructor() {
    let result = single_relation_result(vec![grey(0x76)], Wcag22CriterionV1::Sc143TextDefault);
    let record = evaluated_record(&result);
    let proof = record.proof();
    let profile = wcag22_profile_v1();

    assert_eq!(proof.evaluation_id(), record.evaluation_id());
    assert_eq!(proof.resource_profile_id(), PROFILE);
    assert_eq!(proof.domain_id(), DOMAIN);
    assert_eq!(proof.domain_digest(), record.domain_digest());
    assert_eq!(proof.domain_count(), 256);
    assert_eq!(proof.domain_first(), grey(0));
    assert_eq!(proof.domain_last(), grey(255));
    assert_eq!(proof.relation_set_digest(), record.relation_set_digest());
    assert_eq!(proof.canonical_relations(), 1);
    assert_eq!(proof.applicable_relations(), 1);
    assert_eq!(proof.not_applicable_relations(), 0);
    assert_eq!(proof.applicable_edges(), 1);
    assert_eq!(proof.logical_assessments(), 256);
    assert_eq!(proof.profile_id(), profile.profile_id);
    assert_eq!(proof.artifact_id(), profile.artifact_id);
    assert_eq!(proof.bound_id(), profile.bound_id);
    assert_eq!(proof.proof_id(), profile.proof_id);

    let mut matrix = [0_u8; 32];
    for (index, assessment) in record.assessments().enumerate() {
        if assessment.decision() == Wcag22ApplicableDecisionV1::Fail {
            matrix[index / 8] |= 1_u8 << (index % 8);
        }
    }
    assert_eq!(record.failure_matrix(), &matrix);
    assert_eq!(
        proof.matrix_digest(),
        fixture_sha256::digest(&matrix).as_bytes()
    );

    let mut partition = [0_u8; 32];
    for candidate in record.feasible_candidates() {
        let index = usize::from(candidate.bytes()[0]);
        partition[index / 8] |= 1_u8 << (index % 8);
    }
    assert_eq!(proof.partition(), &partition);

    let proof_sha256 = proof
        .proof_sha256()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(proof_sha256, profile.proof_sha256);
}

#[test]
fn property_permutation_and_duplicate_noise_preserve_canonical_result() {
    check_property(
        64,
        (
            proptest::collection::vec(any::<u8>(), 1..9),
            proptest::collection::vec(any::<u8>(), 1..9),
            any::<bool>(),
        ),
        |(a_values, b_values, reverse_relations)| {
            let canonical_a = applicable_relation(
                "a",
                "shared-occurrence",
                a_values.iter().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc143TextDefault,
            );
            let canonical_b = applicable_relation(
                "b",
                "shared-occurrence",
                b_values.iter().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc1411GraphicalObject,
            );
            let canonical = evaluate(request(vec![canonical_a, canonical_b])).unwrap();

            let mut noisy_a = a_values.clone();
            noisy_a.extend(a_values.iter().rev().copied());
            let mut noisy_b = b_values.clone();
            noisy_b.extend(b_values.iter().rev().copied());
            noisy_b.reverse();
            let a1 = applicable_relation(
                "a",
                "shared-occurrence",
                noisy_a.iter().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc143TextDefault,
            );
            let a2 = applicable_relation(
                "a",
                "shared-occurrence",
                noisy_a.iter().rev().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc143TextDefault,
            );
            let b1 = applicable_relation(
                "b",
                "shared-occurrence",
                noisy_b.iter().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc1411GraphicalObject,
            );
            let b2 = applicable_relation(
                "b",
                "shared-occurrence",
                noisy_b.iter().rev().copied().map(grey).collect(),
                Wcag22CriterionV1::Sc1411GraphicalObject,
            );
            let relations = if reverse_relations {
                vec![b2, a2, b1, a1]
            } else {
                vec![a1, b1, a2, b2]
            };
            let noisy = evaluate(request(relations)).unwrap();

            prop_assert_eq!(
                evaluated_record(&canonical).evaluation_id(),
                evaluated_record(&noisy).evaluation_id()
            );
            prop_assert_eq!(feasible_bytes(&canonical), feasible_bytes(&noisy));
            prop_assert_eq!(infeasible_bytes(&canonical), infeasible_bytes(&noisy));
            prop_assert_eq!(
                evaluated_record(&canonical).assessments().count(),
                evaluated_record(&noisy).assessments().count()
            );
            prop_assert_eq!(
                feasible_bytes(&noisy).len() + infeasible_bytes(&noisy).len(),
                256
            );
            Ok(())
        },
    );
}

#[test]
fn root_srgb8_and_registered_domain_preserve_exact_256_bytes() {
    assert_eq!(Srgb8::new([0x1A, 0x2B, 0x3C]).bytes(), [0x1A, 0x2B, 0x3C]);
    assert_eq!(<[u8; 3]>::from(Srgb8::from([7, 8, 9])), [7, 8, 9]);

    let candidates: Vec<_> = DOMAIN.candidates().map(Srgb8::bytes).collect();
    assert_eq!(candidates.len(), 256);
    assert_eq!(candidates.first(), Some(&[0, 0, 0]));
    assert_eq!(candidates.last(), Some(&[255, 255, 255]));
    assert_eq!(
        candidates,
        (0_u8..=u8::MAX).map(|value| [value; 3]).collect::<Vec<_>>()
    );
}

#[test]
fn local_constructors_reject_empty_relation_shapes_before_compilation() {
    assert!(
        RelationV1::applicable(
            relation_id("empty-adjacency"),
            occurrence_id("occurrence"),
            Wcag22CriterionV1::Sc143TextDefault,
            Vec::new(),
        )
        .is_err()
    );
    assert!(RequestV1::try_new(DOMAIN, Vec::new(), PROFILE).is_err());
}

#[test]
fn exact_4_5_fixtures_are_7_2_and_proven_zero() {
    let seven = single_relation_result(vec![grey(0x76)], Wcag22CriterionV1::Sc143TextDefault);
    assert!(seven.is_feasible());
    assert_eq!(
        feasible_bytes(&seven),
        vec![
            [0x00; 3], [0x01; 3], [0x02; 3], [0x03; 3], [0x04; 3], [0xFE; 3], [0xFF; 3],
        ]
    );
    assert_eq!(infeasible_bytes(&seven).len(), 249);

    let two = single_relation_result(
        vec![grey(0x00), grey(0xFF)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    assert!(two.is_feasible());
    assert_eq!(feasible_bytes(&two), vec![[0x75; 3], [0x76; 3]]);
    assert_eq!(infeasible_bytes(&two).len(), 254);

    let zero = single_relation_result(
        vec![grey(0x00), grey(0xFF), grey(0x76)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    assert!(zero.is_infeasible());
    assert!(feasible_bytes(&zero).is_empty());
    assert_eq!(infeasible_bytes(&zero), exact_grey_range(0, u8::MAX));
}

#[test]
fn exact_3_to_1_fixtures_are_92_and_59_for_every_declared_criterion() {
    for criterion in [
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ] {
        let ninety_two = single_relation_result(vec![grey(0x76)], criterion);
        let mut expected_92 = exact_grey_range(0x00, 0x2D);
        expected_92.extend(exact_grey_range(0xD2, 0xFF));
        assert_eq!(feasible_bytes(&ninety_two), expected_92, "{criterion:?}");
        assert_eq!(infeasible_bytes(&ninety_two).len(), 164, "{criterion:?}");

        let fifty_nine = single_relation_result(vec![grey(0x00), grey(0xFF)], criterion);
        assert_eq!(
            feasible_bytes(&fifty_nine),
            exact_grey_range(0x5A, 0x94),
            "{criterion:?}"
        );
        assert_eq!(infeasible_bytes(&fifty_nine).len(), 197, "{criterion:?}");
    }
}

#[test]
fn assessments_are_the_full_candidate_major_matrix_and_all_edges_must_pass() {
    let text_id = relation_id("text-contract");
    let graphic_id = relation_id("graphic-contract");
    let black = grey(0x00);
    let white = grey(0xFF);
    let tinted_neighbour = grey(0x76);
    let result = evaluate(request(vec![
        RelationV1::applicable(
            text_id.clone(),
            occurrence_id("body-copy"),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![black, white],
        )
        .unwrap(),
        RelationV1::applicable(
            graphic_id.clone(),
            occurrence_id("focus-ring"),
            Wcag22CriterionV1::Sc1411GraphicalObject,
            vec![tinted_neighbour],
        )
        .unwrap(),
    ]))
    .unwrap();

    let mut assessments = evaluated_record(&result).assessments();
    for candidate in DOMAIN.candidates() {
        // Canonical relation order is exact opaque-ID byte order; adjacency is
        // exact sRGB8 byte order inside each relation.
        for (relation, adjacent) in [
            (&graphic_id, tinted_neighbour),
            (&text_id, black),
            (&text_id, white),
        ] {
            let assessment = assessments
                .next()
                .expect("candidate-major matrix ended before the exact 256E cardinality");
            assert_eq!(assessment.candidate(), candidate);
            assert_eq!(assessment.relation_id(), relation);
            assert_eq!(assessment.adjacent(), adjacent);
        }
    }
    assert!(
        assessments.next().is_none(),
        "candidate-major matrix contains more than the exact 256E cells"
    );

    // #757575 passes both text edges but fails the third edge. Any-pass,
    // first-only and skipped-adjacent implementations therefore cannot admit it.
    let witness = grey(0x75);
    assert_eq!(
        exact_decision(&result, witness, &text_id, black),
        Wcag22ApplicableDecisionV1::Pass
    );
    assert_eq!(
        exact_decision(&result, witness, &text_id, white),
        Wcag22ApplicableDecisionV1::Pass
    );
    assert_eq!(
        exact_decision(&result, witness, &graphic_id, tinted_neighbour),
        Wcag22ApplicableDecisionV1::Fail
    );
    assert!(!feasible_bytes(&result).contains(&witness.bytes()));
}

#[test]
fn assessment_iterator_len_decreases_exactly_to_zero() {
    let result = single_relation_result(vec![grey(0x76)], Wcag22CriterionV1::Sc143TextDefault);
    let mut assessments = evaluated_record(&result).assessments();
    assert_eq!(assessments.len(), 256);
    assert!(assessments.next().is_some());
    assert_eq!(assessments.len(), 255);
    for expected in (0..255).rev() {
        assert!(assessments.next().is_some());
        assert_eq!(assessments.len(), expected);
    }
    assert!(assessments.next().is_none());
    assert_eq!(assessments.len(), 0);
}

#[test]
fn relation_and_adjacent_permutation_plus_exact_duplicates_preserve_identity() {
    let a = applicable_relation(
        "a",
        "shared-occurrence",
        vec![grey(0x00), grey(0xFF)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let b = applicable_relation(
        "b",
        "shared-occurrence",
        vec![grey(0x76), grey(0xD2)],
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    );
    let canonical = evaluate(request(vec![a, b])).unwrap();

    let duplicated_a = applicable_relation(
        "a",
        "shared-occurrence",
        vec![grey(0xFF), grey(0x00), grey(0xFF), grey(0x00)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let duplicated_b = applicable_relation(
        "b",
        "shared-occurrence",
        vec![grey(0xD2), grey(0x76), grey(0x76)],
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    );
    let permuted = evaluate(request(vec![
        duplicated_b.clone(),
        duplicated_a.clone(),
        duplicated_b,
        duplicated_a,
    ]))
    .unwrap();

    assert_eq!(
        evaluated_record(&canonical).evaluation_id(),
        evaluated_record(&permuted).evaluation_id()
    );
    assert_eq!(feasible_bytes(&canonical), feasible_bytes(&permuted));
    assert_eq!(infeasible_bytes(&canonical), infeasible_bytes(&permuted));
}

fn assert_relation_conflict(first: RelationV1, conflicting: RelationV1) {
    let error = evaluate(request(vec![first, conflicting]))
        .expect_err("conflicts are detected only at the compiler boundary");
    assert!(matches!(
        error,
        ErrorV1::InvalidRequest(InvalidRequestV1::ConflictingRelationId { .. })
    ));
}

#[test]
fn conflicting_relation_id_is_typed_for_every_canonical_relation_field() {
    let base = || {
        applicable_relation(
            "same",
            "occurrence-a",
            vec![grey(0x00)],
            Wcag22CriterionV1::Sc143TextDefault,
        )
    };

    assert_relation_conflict(
        base(),
        applicable_relation(
            "same",
            "occurrence-b",
            vec![grey(0x00)],
            Wcag22CriterionV1::Sc143TextDefault,
        ),
    );
    assert_relation_conflict(
        base(),
        applicable_relation(
            "same",
            "occurrence-a",
            vec![grey(0x00)],
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ),
    );
    assert_relation_conflict(
        base(),
        applicable_relation(
            "same",
            "occurrence-a",
            vec![grey(0xFF)],
            Wcag22CriterionV1::Sc143TextDefault,
        ),
    );
    assert_relation_conflict(
        base(),
        not_applicable_relation("same", "occurrence-a", "reason-a"),
    );
    assert_relation_conflict(
        not_applicable_relation("same", "occurrence-a", "reason-a"),
        not_applicable_relation("same", "occurrence-a", "reason-b"),
    );
}

#[test]
fn opaque_renames_change_identity_but_not_the_physical_result() {
    let original_id = relation_id("primary-label");
    let renamed_id = relation_id("totally-unrelated-client-id");
    let adjacent = grey(0x76);
    let original = evaluate(request(vec![
        RelationV1::applicable(
            original_id.clone(),
            occurrence_id("hover"),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![adjacent],
        )
        .unwrap(),
    ]))
    .unwrap();
    let renamed = evaluate(request(vec![
        RelationV1::applicable(
            renamed_id.clone(),
            occurrence_id("surface-state-from-another-design-language"),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![adjacent],
        )
        .unwrap(),
    ]))
    .unwrap();

    assert_ne!(
        evaluated_record(&original).evaluation_id(),
        evaluated_record(&renamed).evaluation_id()
    );
    assert_eq!(feasible_bytes(&original), feasible_bytes(&renamed));
    assert_eq!(infeasible_bytes(&original), infeasible_bytes(&renamed));
    for candidate in DOMAIN.candidates() {
        assert_eq!(
            exact_decision(&original, candidate, &original_id, adjacent),
            exact_decision(&renamed, candidate, &renamed_id, adjacent)
        );
    }
}

#[test]
fn all_not_applicable_is_not_evaluated_and_canonical_without_fake_decisions() {
    let a = not_applicable_relation("a", "decorative-edge", "reason-a");
    let b = not_applicable_relation("b", "client-state", "reason-b");
    let canonical = evaluate(request(vec![a, b])).unwrap();

    let duplicated_a = not_applicable_relation("a", "decorative-edge", "reason-a");
    let duplicated_b = not_applicable_relation("b", "client-state", "reason-b");
    let permuted = evaluate(request(vec![
        duplicated_b.clone(),
        duplicated_a.clone(),
        duplicated_b,
        duplicated_a,
    ]))
    .unwrap();

    assert!(canonical.is_not_evaluated());
    assert!(permuted.is_not_evaluated());
    let canonical = not_evaluated_record(&canonical);
    let permuted = not_evaluated_record(&permuted);
    assert_eq!(canonical.domain_id(), DOMAIN);
    assert_eq!(canonical.domain_digest(), permuted.domain_digest());
    assert_eq!(
        canonical.relation_set_digest(),
        permuted.relation_set_digest()
    );
    assert_eq!(canonical.relations().len(), 2);
    assert_eq!(permuted.relations().len(), 2);
    assert_eq!(canonical.relations()[0].relation_id().as_str(), "a");
    assert_eq!(canonical.relations()[1].relation_id().as_str(), "b");
    assert!(canonical.relations()[0].as_applicable().is_none());
    assert_eq!(
        canonical.relations()[0]
            .as_not_applicable()
            .expect("all-NotApplicable terminal retains its declarations")
            .reason_id(),
        "reason-a"
    );
}

#[test]
fn mixed_applicable_and_not_applicable_is_evaluated_without_fabricated_cells() {
    let applicable = applicable_relation(
        "contrast",
        "body",
        vec![grey(0x76)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let applicable_only = evaluate(request(vec![applicable.clone()])).unwrap();
    let mixed = evaluate(request(vec![
        not_applicable_relation("decoration", "ornament", "client-not-applicable"),
        applicable,
    ]))
    .unwrap();

    assert!(applicable_only.is_feasible());
    assert!(mixed.is_feasible());
    assert_eq!(feasible_bytes(&applicable_only), feasible_bytes(&mixed));
    assert_eq!(infeasible_bytes(&applicable_only), infeasible_bytes(&mixed));
    assert_eq!(
        evaluated_record(&applicable_only).assessments().count(),
        256
    );
    assert_eq!(evaluated_record(&mixed).assessments().count(), 256);
    assert_ne!(
        evaluated_record(&applicable_only).evaluation_id(),
        evaluated_record(&mixed).evaluation_id(),
        "NotApplicable declarations belong to canonical identity"
    );

    let relations = evaluated_record(&mixed).relations();
    assert_eq!(relations.len(), 2);
    let (criterion, adjacent) = relations[0]
        .as_applicable()
        .expect("applicable declaration remains inspectable");
    assert_eq!(relations[0].occurrence_id().as_str(), "body");
    assert_eq!(criterion, Wcag22CriterionV1::Sc143TextDefault);
    assert_eq!(adjacent, &[grey(0x76)]);
    assert!(relations[0].as_not_applicable().is_none());
    assert_eq!(relations[1].occurrence_id().as_str(), "ornament");
    assert_eq!(
        relations[1]
            .as_not_applicable()
            .expect("mixed terminal retains NotApplicable declarations")
            .reason_id(),
        "client-not-applicable"
    );
    assert!(relations[1].as_applicable().is_none());
}

fn assert_resource_error(
    error: ErrorV1,
    dimension: ResourceDimensionV1,
    requested: u64,
    limit: u64,
) {
    assert!(matches!(
        error,
        ErrorV1::ResourceLimitExceeded {
            profile_id: PROFILE,
            dimension: actual_dimension,
            requested: actual_requested,
            limit: actual_limit,
        } if actual_dimension == dimension
            && actual_requested == requested
            && actual_limit == limit
    ));
}

#[test]
fn raw_relation_limit_plus_one_rejects_before_duplicate_normalization() {
    let dimension = ResourceDimensionV1::RawRelations;
    let limit = PROFILE.limit(dimension);
    let count = usize::try_from(limit.checked_add(1).unwrap()).unwrap();
    let duplicate = applicable_relation(
        "same-raw-relation",
        "same-occurrence",
        vec![grey(0x00)],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let relations = std::iter::repeat_n(duplicate, count).collect();
    let error = evaluate(request(relations))
        .expect_err("limit+1 duplicates must reject before normalization");
    assert_resource_error(error, dimension, limit + 1, limit);
}

#[test]
fn raw_applicable_adjacent_limit_plus_one_rejects_before_deduplication() {
    let dimension = ResourceDimensionV1::RawAdjacentEntries;
    let limit = PROFILE.limit(dimension);
    let count = usize::try_from(limit.checked_add(1).unwrap()).unwrap();
    let adjacent = std::iter::repeat_n(grey(0x00), count).collect();
    let relation = applicable_relation(
        "applicable",
        "occurrence",
        adjacent,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let error = evaluate(request(vec![relation]))
        .expect_err("duplicate adjacency cannot bypass raw-input bounds");
    assert_resource_error(error, dimension, limit + 1, limit);
}

#[test]
fn aggregate_opaque_id_bytes_are_bounded_without_interpreting_names() {
    let dimension = ResourceDimensionV1::OpaqueUtf8Bytes;
    let limit = PROFILE.limit(dimension);
    assert!(limit > 0, "the profile must admit a non-empty ID");
    let relation_len = usize::try_from(limit).unwrap();
    let relation = RelationV1::applicable(
        relation_id(&"r".repeat(relation_len)),
        occurrence_id("o"),
        Wcag22CriterionV1::Sc143TextDefault,
        vec![grey(0x00)],
    )
    .unwrap();
    let error = evaluate(request(vec![relation])).expect_err("aggregate bytes = limit+1");
    assert_resource_error(error, dimension, limit + 1, limit);
}

#[test]
fn not_applicable_reason_bytes_share_the_same_raw_utf8_bound() {
    let dimension = ResourceDimensionV1::OpaqueUtf8Bytes;
    let limit = PROFILE.limit(dimension);
    assert!(
        limit >= 2,
        "the profile must admit relation and occurrence IDs"
    );
    let reason_len = usize::try_from(limit - 1).unwrap();
    let declaration = Wcag22ClientDeclaredNotApplicableV1::try_new("n".repeat(reason_len)).unwrap();
    let relation = RelationV1::not_applicable(relation_id("r"), occurrence_id("o"), declaration);
    let error = evaluate(request(vec![relation])).expect_err("aggregate bytes = limit+1");
    assert_resource_error(error, dimension, limit + 1, limit);
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
            "[package]\nname = \"forge-feasibility\"\nversion = \"0.0.0\"\n\
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
        "forged feasibility value unexpectedly compiled"
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
fn terminal_variants_cannot_be_rewrapped_by_a_downstream_crate() {
    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::{
    EvaluatedV1, FeasibilityV1, NotEvaluatedV1,
};

fn wrap_feasible(record: EvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::Feasible(record)
}

fn wrap_infeasible(record: EvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::Infeasible(record)
}

fn wrap_not_evaluated(record: NotEvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::NotEvaluated(record)
}

fn main() {}
"#,
        &["Feasible", "Infeasible", "NotEvaluated"],
    );
}

#[test]
fn terminal_records_proof_and_identity_values_cannot_be_forged_downstream() {
    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::{
    DomainDigestV1, EvaluatedV1, EvaluationIdV1, EvaluationProofV1,
    NotEvaluatedV1, RelationSetDigestV1,
};

fn main() {
    let _domain = DomainDigestV1([0_u8; 32]);
    let _relations = RelationSetDigestV1([0_u8; 32]);
    let _evaluation = EvaluationIdV1([0_u8; 32]);
    let _proof = EvaluationProofV1 {};
    let _evaluated = EvaluatedV1 {};
    let _not_evaluated = NotEvaluatedV1 {};
}
"#,
        &[
            "DomainDigestV1",
            "RelationSetDigestV1",
            "EvaluationIdV1",
            "EvaluationProofV1",
            "EvaluatedV1",
            "NotEvaluatedV1",
        ],
    );
}

#[test]
fn root_srgb8_does_not_reintroduce_transport_or_domain_specific_convenience() {
    assert_downstream_rejected(
        r##"use labcolors_core::Srgb8;

fn main() {
    let value = Srgb8::new([7, 7, 7]);
    let _parsed = Srgb8::try_from_hex("#070707");
    let _hex = value.to_hex();
    let _neutral = value.neutral_code();
}
"##,
        &["try_from_hex", "to_hex", "neutral_code"],
    );
}
