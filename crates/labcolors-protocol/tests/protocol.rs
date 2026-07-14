use labcolors_protocol::{
    CoreErrorV1, CoreInvalidRequestV1, DomainIdV1, FeasibilityV1, MAX_ENVELOPE_BYTES_V1,
    ProtocolErrorV1, ProtocolOutcomeV1, RelationV1, RequestV1, ResourceDimensionV1,
    ResourceProfileIdV1, TransportErrorV1, Wcag22CriterionV1, encode_outcome_v1, encode_request_v1,
    envelope_too_large_outcome_v1, evaluate_wcag22_feasibility_v1,
};

fn applicable(
    id: &str,
    occurrence: &str,
    criterion: Wcag22CriterionV1,
    adjacent: Vec<[u8; 3]>,
) -> RelationV1 {
    RelationV1::applicable(id, occurrence, criterion, adjacent).expect("valid relation")
}

fn not_applicable(id: &str, occurrence: &str, reason: &str) -> RelationV1 {
    RelationV1::not_applicable(id, occurrence, reason).expect("valid declaration")
}

fn request(relations: Vec<RelationV1>) -> RequestV1 {
    RequestV1::try_new(
        DomainIdV1::Srgb8NeutralAxis,
        relations,
        ResourceProfileIdV1::Compile,
    )
    .expect("valid request")
}

fn evaluate(request: &RequestV1) -> ProtocolOutcomeV1 {
    let encoded = encode_request_v1(request).expect("canonical request encoding");
    evaluate_wcag22_feasibility_v1(&encoded)
}

fn evaluated(outcome: &ProtocolOutcomeV1) -> &labcolors_protocol::EvaluatedV1 {
    outcome
        .feasibility()
        .and_then(FeasibilityV1::evaluated)
        .expect("successful evaluated feasibility terminal")
}

fn feasible_count(outcome: &ProtocolOutcomeV1) -> u32 {
    evaluated(outcome)
        .proof()
        .partition()
        .iter()
        .map(|byte| byte.count_ones())
        .sum()
}

#[test]
fn exact_oracle_counts_cross_the_protocol_without_adapter_math() {
    let default = Wcag22CriterionV1::Sc143TextDefault;
    let ui = Wcag22CriterionV1::Sc1411UiComponentOrState;

    let seven = evaluate(&request(vec![applicable(
        "seven",
        "occurrence",
        default,
        vec![[0x76; 3]],
    )]));
    assert!(matches!(
        seven.feasibility(),
        Some(FeasibilityV1::Feasible(_))
    ));
    assert_eq!(feasible_count(&seven), 7);

    let two = evaluate(&request(vec![applicable(
        "two",
        "occurrence",
        default,
        vec![[0; 3], [255; 3]],
    )]));
    assert!(matches!(
        two.feasibility(),
        Some(FeasibilityV1::Feasible(_))
    ));
    assert_eq!(feasible_count(&two), 2);

    let zero = evaluate(&request(vec![applicable(
        "zero",
        "occurrence",
        default,
        vec![[0; 3], [255; 3], [0x76; 3]],
    )]));
    assert!(matches!(
        zero.feasibility(),
        Some(FeasibilityV1::Infeasible(_))
    ));
    assert_eq!(feasible_count(&zero), 0);

    let ninety_two = evaluate(&request(vec![applicable(
        "ninety-two",
        "occurrence",
        ui,
        vec![[0x76; 3]],
    )]));
    assert_eq!(feasible_count(&ninety_two), 92);

    let fifty_nine = evaluate(&request(vec![applicable(
        "fifty-nine",
        "occurrence",
        ui,
        vec![[0; 3], [255; 3]],
    )]));
    assert_eq!(feasible_count(&fifty_nine), 59);
}

#[test]
fn all_three_ratio_three_criteria_share_the_exact_ninety_two_partition() {
    let criteria = [
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ];
    let outcomes: Vec<_> = criteria
        .into_iter()
        .map(|criterion| {
            evaluate(&request(vec![applicable(
                "ratio-three",
                "occurrence",
                criterion,
                vec![[0x76; 3]],
            )]))
        })
        .collect();
    for outcome in &outcomes {
        assert_eq!(feasible_count(outcome), 92);
    }
    assert_eq!(
        evaluated(&outcomes[0]).proof().partition(),
        evaluated(&outcomes[1]).proof().partition()
    );
    assert_eq!(
        evaluated(&outcomes[1]).proof().partition(),
        evaluated(&outcomes[2]).proof().partition()
    );
}

#[test]
fn mixed_and_all_not_applicable_keep_declarations_without_fabricated_evidence() {
    let mixed = evaluate(&request(vec![
        applicable(
            "applicable",
            "shared-occurrence",
            Wcag22CriterionV1::Sc143TextDefault,
            vec![[0x76; 3]],
        ),
        not_applicable("declared-na", "shared-occurrence", "client-reason"),
    ]));
    let mixed_value = evaluated(&mixed);
    assert_eq!(mixed_value.relations().len(), 2);
    assert_eq!(mixed_value.proof().applicable_relations(), 1);
    assert_eq!(mixed_value.proof().not_applicable_relations(), 1);

    let all_na = evaluate(&request(vec![not_applicable(
        "declared-na",
        "occurrence",
        "client-reason",
    )]));
    let declaration = match all_na.feasibility() {
        Some(FeasibilityV1::NotEvaluated(value)) => value,
        other => panic!("expected successful NotEvaluated, got {other:?}"),
    };
    assert_eq!(declaration.relations().len(), 1);
}

#[test]
fn conflicting_relation_and_resource_rejection_remain_typed() {
    let conflicting = evaluate(&request(vec![
        applicable(
            "same-id",
            "first",
            Wcag22CriterionV1::Sc143TextDefault,
            vec![[0; 3]],
        ),
        applicable(
            "same-id",
            "second",
            Wcag22CriterionV1::Sc143TextDefault,
            vec![[0; 3]],
        ),
    ]));
    assert!(matches!(
        conflicting.error(),
        Some(ProtocolErrorV1::Core(CoreErrorV1::InvalidRequest(
            CoreInvalidRequestV1::ConflictingRelationId { relation_id }
        ))) if relation_id == "same-id"
    ));

    let repeated = applicable(
        "duplicate",
        "occurrence",
        Wcag22CriterionV1::Sc143TextDefault,
        vec![[0; 3]],
    );
    let too_many = evaluate(&request(vec![repeated; 2_048]));
    assert!(matches!(
        too_many.error(),
        Some(ProtocolErrorV1::Core(CoreErrorV1::ResourceLimitExceeded {
            profile_id: ResourceProfileIdV1::Compile,
            dimension: ResourceDimensionV1::RawRelations,
            requested: 2_048,
            limit: 2_047,
        }))
    ));
}

#[test]
fn opaque_identity_changes_identity_but_not_physical_decisions() {
    let first = evaluate(&request(vec![applicable(
        "first-id",
        "occurrence",
        Wcag22CriterionV1::Sc143TextDefault,
        vec![[0x76; 3]],
    )]));
    let second = evaluate(&request(vec![applicable(
        "second-id",
        "occurrence",
        Wcag22CriterionV1::Sc143TextDefault,
        vec![[0x76; 3]],
    )]));
    let first = evaluated(&first);
    let second = evaluated(&second);
    assert_eq!(first.failure_matrix(), second.failure_matrix());
    assert_eq!(first.proof().partition(), second.proof().partition());
    assert_eq!(
        first.proof().matrix_digest(),
        second.proof().matrix_digest()
    );
    assert_ne!(
        first.proof().relation_set_digest(),
        second.proof().relation_set_digest()
    );
    assert_ne!(
        first.proof().evaluation_id(),
        second.proof().evaluation_id()
    );
}

#[test]
fn evaluated_wire_contains_only_canonical_relations_and_packed_evidence() {
    let outcome = evaluate(&request(vec![applicable(
        "packed",
        "occurrence",
        Wcag22CriterionV1::Sc143TextDefault,
        vec![[0; 3], [255; 3]],
    )]));
    let value = evaluated(&outcome);
    let edges = value.proof().applicable_edges() as usize;
    assert_eq!(value.failure_matrix().len(), 32 * edges);
    assert_eq!(value.proof().partition().len(), 32);
    assert_eq!(value.proof().logical_assessments(), 256 * edges as u64);
    assert_eq!(value.domain().len(), 256);
    assert_eq!(value.domain().first(), Some(&[0, 0, 0]));
    assert_eq!(value.domain().last(), Some(&[255, 255, 255]));

    let canonical = encode_outcome_v1(&outcome).expect("canonical outcome encoding");
    let json: serde_json::Value =
        serde_json::from_slice(&canonical).expect("canonical outcome JSON");
    let object = json.as_object().expect("tagged outcome object");
    assert_eq!(
        object.get("schemaVersion").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        object.get("outcome").and_then(|value| value.as_str()),
        Some("success")
    );
    assert!(object.get("error").is_none());
    assert_eq!(
        object
            .get("feasibility")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("feasible")
    );
    let encoded = json.to_string();
    assert_eq!(encoded.matches("\"domain\":").count(), 1);
    for forbidden in [
        "feasibleCandidates",
        "infeasibleCandidates",
        "cells",
        "assessments",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "forbidden proportional view: {forbidden}"
        );
    }

    let proof = object
        .get("feasibility")
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("proof"))
        .expect("evaluated proof");
    for field in [
        "domainCount",
        "canonicalRelations",
        "applicableRelations",
        "notApplicableRelations",
        "applicableEdges",
        "logicalAssessments",
    ] {
        assert!(
            proof.get(field).is_some_and(serde_json::Value::is_string),
            "{field} must be an exact decimal string"
        );
    }

    assert_eq!(
        canonical,
        encode_outcome_v1(&outcome).expect("canonical encoding is byte-stable")
    );
}

#[test]
fn errors_are_failure_outcomes_not_a_fourth_feasibility_terminal() {
    let outcome = evaluate_wcag22_feasibility_v1(br#"{}"#);
    assert!(outcome.feasibility().is_none());
    assert!(outcome.error().is_some());

    let canonical = encode_outcome_v1(&outcome).expect("canonical failure encoding");
    let json: serde_json::Value =
        serde_json::from_slice(&canonical).expect("canonical failure JSON");
    assert_eq!(
        json.get("schemaVersion").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        json.get("outcome").and_then(|value| value.as_str()),
        Some("failure")
    );
    assert!(json.get("error").is_some());
    assert!(json.get("feasibility").is_none());
}

#[test]
fn schema_and_input_failures_are_strict_and_typed() {
    let cases = [
        (
            br#"{"schemaVersion":2,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[]}"#.as_slice(),
            "version",
        ),
        (
            br#"{"schemaVersion":1,"domainId":"other","resourceProfileId":"compile-v1","relations":[]}"#.as_slice(),
            "domain",
        ),
        (
            br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"other","relations":[]}"#.as_slice(),
            "profile",
        ),
        (
            br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[{"relationId":"r","occurrenceId":"o","kind":"applicable","criterion":"other","adjacent":[[0,0,0]]}]}"#.as_slice(),
            "criterion",
        ),
    ];
    for (raw, expected) in cases {
        let outcome = evaluate_wcag22_feasibility_v1(raw);
        let error = outcome.error().expect("typed error");
        match (expected, error) {
            (
                "version",
                ProtocolErrorV1::Transport(TransportErrorV1::UnsupportedSchemaVersion { .. }),
            )
            | (
                "domain",
                ProtocolErrorV1::Transport(TransportErrorV1::UnsupportedDomainId { .. }),
            )
            | (
                "profile",
                ProtocolErrorV1::Transport(TransportErrorV1::UnsupportedResourceProfileId {
                    ..
                }),
            )
            | (
                "criterion",
                ProtocolErrorV1::Transport(TransportErrorV1::UnsupportedCriterion { .. }),
            ) => {}
            _ => panic!("unexpected {expected} error: {error:?}"),
        }
    }

    for raw in [
        br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[],"extra":true}"#.as_slice(),
        br#"{"schemaVersion":1,"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[]}"#.as_slice(),
        br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[{"relationId":"r","occurrenceId":"o","kind":"applicable","criterion":"sc-1.4.3-text-default","adjacent":[[0,0,0]],"extra":true}]}"#.as_slice(),
        br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[{"relationId":"r","relationId":"r","occurrenceId":"o","kind":"applicable","criterion":"sc-1.4.3-text-default","adjacent":[[0,0,0]]}]}"#.as_slice(),
        br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[{"relationId":"r","occurrenceId":"o","kind":"applicable","criterion":"sc-1.4.3-text-default","adjacent":[[0,0]]}]}"#.as_slice(),
        br#"{"schemaVersion":1,"domainId":"srgb8-neutral-axis-v1","resourceProfileId":"compile-v1","relations":[{"relationId":"r","occurrenceId":"o","kind":"applicable","criterion":"sc-1.4.3-text-default","adjacent":[[256,0,0]]}]}"#.as_slice(),
    ] {
        assert!(matches!(
            evaluate_wcag22_feasibility_v1(raw).error(),
            Some(ProtocolErrorV1::Transport(TransportErrorV1::MalformedEnvelope { .. }))
        ));
    }

    assert!(matches!(
        evaluate_wcag22_feasibility_v1(&[0xff]).error(),
        Some(ProtocolErrorV1::Transport(TransportErrorV1::InvalidUtf8))
    ));
}

#[test]
fn exact_compact_ceiling_is_derived_and_attainable() {
    const RELATIONS: usize = 2_047;
    const OPAQUE_BYTES: usize = 65_536;
    const ESCAPE_SIX_ALPHABET: [u8; 27] = [
        0, 1, 2, 3, 4, 5, 6, 7, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
        30, 31,
    ];

    let relation_id_bytes = RELATIONS * 3;
    let first_occurrence_bytes = OPAQUE_BYTES - relation_id_bytes - (RELATIONS - 1);
    let mut relations = Vec::with_capacity(RELATIONS);
    for index in 0..RELATIONS {
        let id = String::from_utf8(vec![
            ESCAPE_SIX_ALPHABET[(index / (27 * 27)) % 27],
            ESCAPE_SIX_ALPHABET[(index / 27) % 27],
            ESCAPE_SIX_ALPHABET[index % 27],
        ])
        .expect("ASCII controls are valid UTF-8");
        let occurrence = if index == 0 {
            "\0".repeat(first_occurrence_bytes)
        } else {
            "\0".to_string()
        };
        relations.push(applicable(
            &id,
            &occurrence,
            Wcag22CriterionV1::Sc1411UiComponentOrState,
            vec![[255; 3]],
        ));
    }
    let encoded = encode_request_v1(&request(relations)).expect("exact-limit encoding");
    assert_eq!(encoded.len() as u64, MAX_ENVELOPE_BYTES_V1);

    let mut over = encoded;
    over.push(b' ');
    let over_outcome = evaluate_wcag22_feasibility_v1(&over);
    assert!(matches!(
        over_outcome.error(),
        Some(ProtocolErrorV1::Transport(TransportErrorV1::EnvelopeTooLarge {
            requested_bytes,
            limit_bytes: MAX_ENVELOPE_BYTES_V1,
        })) if *requested_bytes == MAX_ENVELOPE_BYTES_V1 + 1
    ));
    let encoded_error = encode_outcome_v1(&over_outcome).expect("canonical limit failure");
    assert_eq!(
        encoded_error,
        encode_outcome_v1(&envelope_too_large_outcome_v1(MAX_ENVELOPE_BYTES_V1 + 1,))
            .expect("host-preflight limit failure")
    );
    assert_eq!(
        String::from_utf8(encoded_error).expect("UTF-8 JSON"),
        concat!(
            "{\"schemaVersion\":1,\"outcome\":\"failure\",\"error\":",
            "{\"source\":\"transport\",\"error\":{\"code\":\"envelopeTooLarge\",",
            "\"requestedBytes\":\"657381\",\"limitBytes\":\"657380\"}}}"
        )
    );
}
