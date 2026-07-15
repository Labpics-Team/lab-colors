//! Транспортные законы атомарной операции `wcag22-explicit-selection-v1`.
//!
//! Проверяются: точность и достижимость выведенного envelope, строгость
//! декодера (неизвестные kind — типизированный transport), фазовая атрибуция
//! ошибок конструирования, отсутствие частичного feasibility на отказе и
//! байт-идентичность feasibility-поддерева при противоположных порядках.

use labcolors_protocol::explicit_selection::{
    CandidateV1, MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1, PolicyV1, RequestV1,
    encode_explicit_selection_request_v1, evaluate_wcag22_explicit_selection_v1,
};
use labcolors_protocol::{RelationV1, Wcag22CriterionV1};
use serde_json::Value;

fn candidate(id: &str, emitted: [u8; 3]) -> CandidateV1 {
    CandidateV1::new(id, emitted).expect("test candidate is locally valid")
}

fn applicable(relation_id: &str, adjacent: Vec<[u8; 3]>) -> RelationV1 {
    RelationV1::applicable(
        relation_id,
        "occurrence",
        Wcag22CriterionV1::Sc143TextDefault,
        adjacent,
    )
    .expect("test relation is locally valid")
}

fn policy(id: &str, order: &[&str]) -> PolicyV1 {
    PolicyV1::first_feasible_in_declared_order(
        id,
        order.iter().map(|id| (*id).to_string()).collect(),
    )
    .expect("test policy is locally valid")
}

fn evaluate_json(request: &RequestV1) -> Value {
    let bytes = encode_explicit_selection_request_v1(request).expect("request encodes");
    let outcome = evaluate_wcag22_explicit_selection_v1(&bytes);
    let encoded =
        labcolors_protocol::explicit_selection::encode_explicit_selection_outcome_v1(&outcome)
            .expect("outcome encodes");
    serde_json::from_slice(&encoded).expect("outcome JSON parses")
}

fn evaluate_raw_json(raw: &str) -> Value {
    let outcome = evaluate_wcag22_explicit_selection_v1(raw.as_bytes());
    let encoded =
        labcolors_protocol::explicit_selection::encode_explicit_selection_outcome_v1(&outcome)
            .expect("outcome encodes");
    serde_json::from_slice(&encoded).expect("outcome JSON parses")
}

fn base_request_json() -> Value {
    serde_json::json!({
        "schemaVersion": 1,
        "domainId": "explicit-srgb8-set-v1",
        "resourceProfileId": "compile-v1",
        "candidates": [
            {"candidateId": "member-a", "emitted": [255, 255, 255]},
            {"candidateId": "member-b", "emitted": [0, 0, 0]},
        ],
        "relations": [
            {
                "relationId": "relation",
                "occurrenceId": "occurrence",
                "kind": "applicable",
                "criterion": "sc-1.4.3-text-default",
                "adjacent": [[0, 0, 0]],
            },
        ],
        "policy": {
            "policyKind": "first-feasible-in-declared-order-v1",
            "policyId": "client",
            "orderedCandidateIds": ["member-a"],
        },
    })
}

#[test]
fn all_four_lawful_terminals_serialize_with_bound_policy() {
    // Selected.
    let selected = evaluate_json(
        &RequestV1::try_new(
            vec![
                candidate("member-a", [255; 3]),
                candidate("member-b", [0; 3]),
            ],
            vec![applicable("relation", vec![[0; 3]])],
            policy("client", &["member-b", "member-a"]),
        )
        .unwrap(),
    );
    assert_eq!(selected["outcome"], "success");
    assert_eq!(selected["result"]["status"], "selected");
    assert_eq!(selected["result"]["selection"]["candidateId"], "member-a");
    assert_eq!(
        selected["result"]["selection"]["selectedPolicyOrdinal"], "1",
        "u64 fields serialize as exact decimal text"
    );
    assert_eq!(
        selected["result"]["selection"]["finalVerification"]["verifiedApplicableEdges"],
        "1"
    );
    assert_eq!(
        selected["result"]["feasibility"]["proof"]["domainKind"],
        "explicit-srgb8-set-v1"
    );
    assert_eq!(
        selected["result"]["feasibility"]["proof"]["candidateCount"],
        "2"
    );
    assert!(
        selected["result"]["feasibility"]["proof"]
            .get("domainFirst")
            .is_none(),
        "explicit evidence must not reuse neutral-only descriptor fields"
    );
    assert!(
        selected["result"]["feasibility"]["proof"]
            .get("domainLast")
            .is_none()
    );

    // NoSelection: валидная политика без feasible члена.
    let no_selection = evaluate_json(
        &RequestV1::try_new(
            vec![
                candidate("member-a", [255; 3]),
                candidate("member-b", [0; 3]),
            ],
            vec![applicable("relation", vec![[0; 3]])],
            policy("client", &["member-b"]),
        )
        .unwrap(),
    );
    assert_eq!(no_selection["result"]["status"], "noSelection");
    assert_eq!(
        no_selection["result"]["selection"]["reason"],
        "noDeclaredCandidateFeasible"
    );
    assert_eq!(no_selection["result"]["selection"]["policyId"], "client");
    assert!(no_selection["result"]["feasibility"]["proof"].is_object());

    // Infeasible: полная пустая партиция, политика связана без receipt.
    let infeasible = evaluate_json(
        &RequestV1::try_new(
            vec![candidate("member-a", [0; 3]), candidate("member-b", [1; 3])],
            vec![applicable("relation", vec![[0; 3]])],
            policy("client", &["member-a"]),
        )
        .unwrap(),
    );
    assert_eq!(infeasible["result"]["status"], "infeasible");
    assert_eq!(infeasible["result"]["policy"]["policyId"], "client");
    assert_eq!(infeasible["result"]["policy"]["declaredEntries"], "1");
    assert!(
        infeasible["result"].get("selection").is_none(),
        "non-selection terminals must not expose a selection receipt"
    );

    // NotEvaluated: declaration-only терминал, политика связана.
    let not_evaluated = evaluate_raw_json(
        &serde_json::json!({
            "schemaVersion": 1,
            "domainId": "explicit-srgb8-set-v1",
            "resourceProfileId": "compile-v1",
            "candidates": [{"candidateId": "member-a", "emitted": [255, 255, 255]}],
            "relations": [{
                "relationId": "relation",
                "occurrenceId": "occurrence",
                "kind": "notApplicable",
                "reasonId": "out-of-scope",
            }],
            "policy": {
                "policyKind": "first-feasible-in-declared-order-v1",
                "policyId": "client",
                "orderedCandidateIds": ["member-a"],
            },
        })
        .to_string(),
    );
    assert_eq!(not_evaluated["result"]["status"], "notEvaluated");
    assert_eq!(not_evaluated["result"]["policy"]["policyId"], "client");
    assert_eq!(
        not_evaluated["result"]["feasibility"]["candidateCount"],
        "1"
    );
    assert!(not_evaluated["result"].get("selection").is_none());
}

#[test]
fn opposite_orders_share_a_byte_identical_feasibility_subtree() {
    let fixture = |policy_id: &str, order: &[&str]| {
        evaluate_json(
            &RequestV1::try_new(
                vec![
                    candidate("member-a", [255; 3]),
                    candidate("member-b", [254; 3]),
                ],
                vec![applicable("relation", vec![[0; 3]])],
                policy(policy_id, order),
            )
            .unwrap(),
        )
    };
    let forward = fixture("forward", &["member-a", "member-b"]);
    let reverse = fixture("reverse", &["member-b", "member-a"]);

    assert_eq!(
        serde_json::to_string(&forward["result"]["feasibility"]).unwrap(),
        serde_json::to_string(&reverse["result"]["feasibility"]).unwrap(),
        "opposite declared orders must not rewrite the physical feasibility subtree"
    );
    assert_eq!(forward["result"]["selection"]["candidateId"], "member-a");
    assert_eq!(reverse["result"]["selection"]["candidateId"], "member-b");
    assert_ne!(
        forward["result"]["selection"]["policyDigest"],
        reverse["result"]["selection"]["policyDigest"]
    );
}

#[test]
fn failure_carries_no_partial_feasibility_payload() {
    let foreign_tail = evaluate_json(
        &RequestV1::try_new(
            vec![
                candidate("member-a", [255; 3]),
                candidate("member-b", [0; 3]),
            ],
            vec![applicable("relation", vec![[0; 3]])],
            policy("client", &["member-a", "foreign"]),
        )
        .unwrap(),
    );
    assert_eq!(foreign_tail["outcome"], "failure");
    assert_eq!(foreign_tail["error"]["source"], "selection");
    assert_eq!(foreign_tail["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        foreign_tail["error"]["error"]["details"]["code"],
        "foreignCandidateId"
    );
    assert!(
        foreign_tail.get("result").is_none(),
        "failure must not leak a partial feasibility terminal"
    );

    let duplicate_domain = evaluate_json(
        &RequestV1::try_new(
            vec![
                candidate("member-a", [255; 3]),
                candidate("member-a", [0; 3]),
            ],
            vec![applicable("relation", vec![[0; 3]])],
            policy("client", &["foreign"]),
        )
        .unwrap(),
    );
    assert_eq!(
        duplicate_domain["error"]["source"], "feasibility",
        "an A-phase failure has priority over any policy defect"
    );
    assert_eq!(
        duplicate_domain["error"]["error"]["details"]["code"],
        "duplicateCandidateId"
    );
}

#[test]
fn strict_decoder_rejects_unknown_kinds_and_shapes_with_typed_errors() {
    let mutate = |mutator: &dyn Fn(&mut Value)| {
        let mut request = base_request_json();
        mutator(&mut request);
        evaluate_raw_json(&request.to_string())
    };

    let unknown_schema = mutate(&|request| request["schemaVersion"] = 2.into());
    assert_eq!(unknown_schema["error"]["source"], "transport");
    assert_eq!(
        unknown_schema["error"]["error"]["code"],
        "unsupportedSchemaVersion"
    );

    let unknown_domain = mutate(&|request| request["domainId"] = "srgb8-neutral-axis-v1".into());
    assert_eq!(
        unknown_domain["error"]["error"]["code"], "unsupportedDomainId",
        "the neutral domain does not leak into the explicit operation"
    );

    let unknown_profile = mutate(&|request| request["resourceProfileId"] = "runtime-v1".into());
    assert_eq!(
        unknown_profile["error"]["error"]["code"],
        "unsupportedResourceProfileId"
    );

    let unknown_policy_kind =
        mutate(&|request| request["policy"]["policyKind"] = "best-feasible-v1".into());
    assert_eq!(
        unknown_policy_kind["error"]["error"]["code"],
        "unsupportedPolicyKind"
    );
    assert_eq!(
        unknown_policy_kind["error"]["error"]["received"],
        "best-feasible-v1"
    );

    let unknown_criterion =
        mutate(&|request| request["relations"][0]["criterion"] = "sc-9.9.9".into());
    assert_eq!(
        unknown_criterion["error"]["error"]["code"],
        "unsupportedCriterion"
    );

    let unknown_field = mutate(&|request| request["proof"] = "forged".into());
    assert_eq!(
        unknown_field["error"]["error"]["code"], "malformedEnvelope",
        "caller-supplied proof/count/digest fields are rejected by the strict shape"
    );
    assert_eq!(unknown_field["error"]["error"]["class"], "shape");

    let unknown_policy_field = mutate(&|request| {
        request["policy"]["proof"] = "forged".into();
    });
    assert_eq!(
        unknown_policy_field["error"]["error"]["code"],
        "malformedEnvelope"
    );

    let not_utf8 = evaluate_wcag22_explicit_selection_v1(&[0xFF, 0xFE]);
    let encoded =
        labcolors_protocol::explicit_selection::encode_explicit_selection_outcome_v1(&not_utf8)
            .unwrap();
    let not_utf8: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(not_utf8["error"]["error"]["code"], "invalidUtf8");
}

#[test]
fn construction_defects_attribute_to_their_owning_phase() {
    let mutate = |mutator: &dyn Fn(&mut Value)| {
        let mut request = base_request_json();
        mutator(&mut request);
        evaluate_raw_json(&request.to_string())
    };

    // A-фаза: пустой candidateId — Core-классификация в feasibility-источнике.
    let empty_candidate = mutate(&|request| {
        request["candidates"][0]["candidateId"] = "".into();
    });
    assert_eq!(empty_candidate["error"]["source"], "feasibility");
    assert_eq!(
        empty_candidate["error"]["error"]["details"]["code"],
        "emptyCandidateId"
    );

    // B-фаза: пустой policyId — selection-источник.
    let empty_policy_id = mutate(&|request| {
        request["policy"]["policyId"] = "".into();
    });
    assert_eq!(empty_policy_id["error"]["source"], "selection");
    assert_eq!(
        empty_policy_id["error"]["error"]["details"]["code"],
        "emptyPolicyId"
    );

    // B-фаза: пустая запись порядка — декодерная атрибуция Core-закона ID.
    let empty_ordered_id = mutate(&|request| {
        request["policy"]["orderedCandidateIds"][0] = "".into();
    });
    assert_eq!(empty_ordered_id["error"]["source"], "selection");
    assert_eq!(
        empty_ordered_id["error"]["error"]["details"]["code"],
        "emptyCandidateId"
    );

    // Пустой порядок целиком.
    let empty_order = mutate(&|request| {
        request["policy"]["orderedCandidateIds"] = serde_json::json!([]);
    });
    assert_eq!(
        empty_order["error"]["error"]["details"]["code"],
        "emptyCandidateOrder"
    );

    // A-дефект формы (пустой candidateId) побеждает B-дефект (пустой policyId).
    let both = mutate(&|request| {
        request["candidates"][0]["candidateId"] = "".into();
        request["policy"]["policyId"] = "".into();
    });
    assert_eq!(both["error"]["source"], "feasibility");
}

/// Достижимость точного потолка: настоящий compact-запрос, допустимый каждым
/// RAW-измерением, кодируется ровно в `MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1`
/// байтов, принимается конвертом и отклоняется уже Core-канонизацией
/// (дубликаты 1-байтовых ID), а не транспортом.
#[test]
fn derived_envelope_is_achievable_by_a_raw_admissible_compact_request() {
    let candidate_count = usize::try_from(65_536_u64 - 2 * 2_047).unwrap();
    let candidates = vec![candidate("\u{0}", [255; 3]); candidate_count];
    // Отношения используют по 2 opaque-байта (relationId и occurrenceId по
    // одному NUL-байту), самый длинный criterion-ключ и максимальные триплеты.
    let relations: Vec<RelationV1> = (0..2_047)
        .map(|_| {
            RelationV1::applicable(
                "\u{0}",
                "\u{0}",
                Wcag22CriterionV1::Sc1411UiComponentOrState,
                vec![[255; 3]],
            )
            .unwrap()
        })
        .collect();
    let ordered = vec!["\u{0}".to_string(); 65_535];
    let request = RequestV1::try_new(
        candidates,
        relations,
        PolicyV1::first_feasible_in_declared_order("\u{0}", ordered).unwrap(),
    )
    .unwrap();

    let bytes = encode_explicit_selection_request_v1(&request).unwrap();
    assert_eq!(
        u64::try_from(bytes.len()).unwrap(),
        MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1,
        "the derived ceiling must be achieved exactly, with zero discretionary headroom"
    );

    let outcome = evaluate_wcag22_explicit_selection_v1(&bytes);
    let encoded =
        labcolors_protocol::explicit_selection::encode_explicit_selection_outcome_v1(&outcome)
            .unwrap();
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        value["error"]["source"], "feasibility",
        "the at-limit envelope must be accepted by transport and classified by Core"
    );
    assert_eq!(
        value["error"]["error"]["details"]["code"],
        "duplicateCandidateId"
    );
}
