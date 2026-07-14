//! Public RED contract for client-declared finite sRGB8 feasibility (#296-A).
//!
//! This target intentionally uses only the public Core API. The explicit set
//! owns opaque candidate identities; WCAG mathematics remains the existing
//! proof-bound atomic evaluator.

use std::fs;
use std::process::Command;

use labcolors_core::Srgb8;
use labcolors_core::wcag22::{Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainKindV1, DomainRequestV1, EvaluatedV1, FeasibilityV1, RequestV1,
    evaluate,
};
use labcolors_core::wcag22_feasibility::{
    DomainIdV1, ErrorV1, InvalidRequestV1, OccurrenceId, RelationId, RelationV1,
    RequestV1 as NeutralRequestV1, ResourceProfileIdV1, evaluate as evaluate_neutral,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod fixture_sha256;

const PROFILE: ResourceProfileIdV1 = ResourceProfileIdV1::Compile;
const IDENTITY_FIXTURE: &str =
    include_str!("../contracts/wcag22-explicit-feasibility-identity-v1.json");

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

fn applicable(adjacent: Vec<Srgb8>) -> RelationV1 {
    RelationV1::applicable(
        relation_id("contrast"),
        occurrence_id("occurrence"),
        Wcag22CriterionV1::Sc143TextDefault,
        adjacent,
    )
    .expect("test relation has adjacency")
}

fn request(candidates: Vec<CandidateV1>, relations: Vec<RelationV1>) -> RequestV1 {
    let domain = DomainRequestV1::try_new(candidates).expect("test domain is non-empty");
    RequestV1::try_new(domain, relations, PROFILE).expect("test request has relations")
}

fn evaluated(result: &FeasibilityV1) -> &EvaluatedV1 {
    result.evaluated().expect("expected an evaluated terminal")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
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
        .expect("explicit feasibility property failed with a minimized counterexample");
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
            "[package]\nname = \"forge-explicit-feasibility\"\nversion = \"0.0.0\"\n\
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
        "forged explicit proof unexpectedly compiled"
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
fn empty_candidate_shapes_are_rejected_before_compilation() {
    assert!(matches!(
        CandidateId::try_new(""),
        Err(InvalidRequestV1::EmptyCandidateId)
    ));
    assert!(matches!(
        DomainRequestV1::try_new(Vec::new()),
        Err(InvalidRequestV1::EmptyCandidates)
    ));
}

#[test]
fn candidate_order_is_canonical_but_duplicate_ids_are_never_deduplicated() {
    let canonical = evaluate(request(
        vec![candidate("alpha", [0x75; 3]), candidate("zeta", [0x76; 3])],
        vec![applicable(vec![Srgb8::new([0; 3]), Srgb8::new([255; 3])])],
    ))
    .unwrap();
    let permuted = evaluate(request(
        vec![candidate("zeta", [0x76; 3]), candidate("alpha", [0x75; 3])],
        vec![applicable(vec![Srgb8::new([255; 3]), Srgb8::new([0; 3])])],
    ))
    .unwrap();

    let canonical = evaluated(&canonical);
    let permuted = evaluated(&permuted);
    assert_eq!(canonical.domain_digest(), permuted.domain_digest());
    assert_eq!(canonical.evaluation_id(), permuted.evaluation_id());
    assert_eq!(canonical.failure_matrix(), permuted.failure_matrix());
    assert_eq!(canonical.proof().partition(), permuted.proof().partition());
    assert_eq!(
        canonical
            .candidates()
            .iter()
            .map(|value| value.candidate_id().as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );

    let error = evaluate(request(
        vec![candidate("same", [1, 2, 3]), candidate("same", [4, 5, 6])],
        vec![applicable(vec![Srgb8::new([255; 3])])],
    ))
    .expect_err("duplicate candidate IDs are contradictory, not duplicate noise");
    assert!(matches!(
        error,
        ErrorV1::InvalidRequest(InvalidRequestV1::DuplicateCandidateId { candidate_id })
            if candidate_id.as_str() == "same"
    ));
}

#[test]
fn same_emitted_bytes_under_distinct_ids_remain_distinct_matrix_rows() {
    let result = evaluate(request(
        vec![
            candidate("first", [0x75; 3]),
            candidate("second", [0x75; 3]),
        ],
        vec![applicable(vec![Srgb8::new([0; 3])])],
    ))
    .unwrap();
    let record = evaluated(&result);

    assert_eq!(record.candidates().len(), 2);
    assert_eq!(record.assessments().len(), 2);
    assert_eq!(record.proof().domain().candidate_count(), 2);
    assert_eq!(
        record
            .assessments()
            .map(|value| (value.candidate_id().as_str(), value.emitted().bytes()))
            .collect::<Vec<_>>(),
        [("first", [0x75; 3]), ("second", [0x75; 3])]
    );
}

#[test]
fn variable_bit_layout_uses_one_contiguous_matrix_and_zero_tail_bits() {
    let result = evaluate(request(
        vec![
            candidate("a", [0; 3]),
            candidate("b", [0x76; 3]),
            candidate("c", [255; 3]),
        ],
        vec![applicable(vec![
            Srgb8::new([0; 3]),
            Srgb8::new([0x76; 3]),
            Srgb8::new([255; 3]),
        ])],
    ))
    .unwrap();
    let record = evaluated(&result);
    let proof = record.proof();

    assert_eq!(proof.domain().kind(), DomainKindV1::ExplicitSrgb8Set);
    assert_eq!(proof.domain().kind().key(), "explicit-srgb8-set-v1");
    assert_eq!(proof.domain().candidate_count(), 3);
    assert_eq!(proof.applicable_edges(), 3);
    assert_eq!(proof.logical_assessments(), 9);
    assert_eq!(record.failure_matrix().len(), 2, "ceil(3*3/8)");
    assert_eq!(proof.partition().len(), 1, "ceil(3/8)");
    assert_eq!(record.failure_matrix()[1] & 0b1111_1110, 0);
    assert_eq!(proof.partition()[0] & 0b1111_1000, 0);
    assert_eq!(record.assessments().len(), 9);
}

#[test]
fn variable_domain_crosses_the_256_candidate_boundary_without_truncation() {
    const CANDIDATES: usize = 513;
    let result = evaluate(request(
        (0..CANDIDATES)
            .map(|index| candidate(&format!("candidate/{index:03}"), [255; 3]))
            .collect(),
        vec![applicable(vec![Srgb8::new([0; 3])])],
    ))
    .unwrap();
    let record = evaluated(&result);

    assert_eq!(record.proof().domain().candidate_count(), 513);
    assert_eq!(record.proof().logical_assessments(), 513);
    assert_eq!(record.failure_matrix(), [0_u8; 65]);
    assert_eq!(record.proof().partition()[..64], [u8::MAX; 64]);
    assert_eq!(record.proof().partition()[64], 1);
    assert_eq!(record.assessments().len(), 513);
    assert_eq!(record.feasible_candidates().count(), 513);
    assert_eq!(
        record.candidates().last().unwrap().candidate_id().as_str(),
        "candidate/512",
    );
}

#[test]
fn full_explicit_neutral_set_matches_every_neutral_v1_physical_bit() {
    let adjacent = vec![Srgb8::new([0x76; 3])];
    let neutral = evaluate_neutral(
        NeutralRequestV1::try_new(
            DomainIdV1::Srgb8NeutralAxis,
            vec![applicable(adjacent.clone())],
            PROFILE,
        )
        .unwrap(),
    )
    .unwrap();
    let explicit = evaluate(request(
        (0_u16..256)
            .map(|value| {
                let value = value as u8;
                candidate(&format!("grey/{value:03}"), [value; 3])
            })
            .collect(),
        vec![applicable(adjacent)],
    ))
    .unwrap();
    let neutral = neutral.evaluated().unwrap();
    let explicit = evaluated(&explicit);

    assert_eq!(explicit.failure_matrix(), neutral.failure_matrix());
    assert_eq!(explicit.proof().partition(), neutral.proof().partition());
    assert_eq!(
        explicit
            .assessments()
            .map(|value| (value.emitted(), value.adjacent(), value.decision()))
            .collect::<Vec<_>>(),
        neutral
            .assessments()
            .map(|value| (value.candidate(), value.adjacent(), value.decision()))
            .collect::<Vec<_>>()
    );
    assert_ne!(explicit.domain_digest(), neutral.domain_digest());
    assert_ne!(explicit.evaluation_id(), neutral.evaluation_id());
}

#[test]
fn no_applicable_relation_is_the_only_not_evaluated_terminal() {
    let relation = RelationV1::not_applicable(
        relation_id("ornament"),
        occurrence_id("decorative"),
        Wcag22ClientDeclaredNotApplicableV1::try_new("client-declared").unwrap(),
    );
    let result = evaluate(request(
        vec![candidate("one", [1, 2, 3]), candidate("two", [4, 5, 6])],
        vec![relation],
    ))
    .unwrap();

    assert!(result.is_not_evaluated());
    let record = result.not_evaluated().unwrap();
    assert_eq!(record.domain().kind(), DomainKindV1::ExplicitSrgb8Set);
    assert_eq!(record.domain().candidate_count(), 2);
    assert_eq!(record.candidates().len(), 2);
    assert_eq!(record.relations().len(), 1);
}

#[test]
fn candidate_ids_share_the_existing_opaque_byte_envelope_without_a_new_limit() {
    let tiny_relation = || {
        RelationV1::applicable(
            relation_id("r"),
            occurrence_id("o"),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![Srgb8::new([255; 3])],
        )
        .unwrap()
    };
    let at_limit = evaluate(request(
        vec![candidate(&"x".repeat(65_534), [0; 3])],
        vec![tiny_relation()],
    ));
    assert!(
        at_limit.is_ok(),
        "candidate ID plus relation/occurrence is 65536 bytes"
    );

    let error = evaluate(request(
        vec![candidate(&"x".repeat(65_535), [0; 3])],
        vec![tiny_relation()],
    ))
    .expect_err("65537 aggregate opaque bytes must fail before evaluation");
    assert!(matches!(
        error,
        ErrorV1::ResourceLimitExceeded {
            dimension: labcolors_core::wcag22_feasibility::ResourceDimensionV1::OpaqueUtf8Bytes,
            requested: 65_537,
            limit: 65_536,
            ..
        }
    ));
}

#[test]
fn property_variable_c_times_e_is_complete_canonical_and_exactly_packed() {
    check_property((1_u8..18, 1_u8..10, any::<bool>()), |(c, e, reverse)| {
        let mut candidates = (0..c)
            .map(|index| {
                candidate(
                    &format!("candidate/{index:03}"),
                    [index, index.wrapping_mul(17), index.wrapping_mul(31)],
                )
            })
            .collect::<Vec<_>>();
        if reverse {
            candidates.reverse();
        }
        let adjacent = (0..e)
            .map(|index| Srgb8::new([index, index.wrapping_mul(11), 255 - index]))
            .collect();
        let result = evaluate(request(candidates, vec![applicable(adjacent)])).unwrap();
        let record = evaluated(&result);
        let work = u64::from(c) * u64::from(e);
        let matrix_bytes = work.div_ceil(8) as usize;
        let partition_bytes = u64::from(c).div_ceil(8) as usize;

        prop_assert_eq!(record.proof().logical_assessments(), work);
        prop_assert_eq!(record.assessments().len(), work as usize);
        prop_assert_eq!(record.failure_matrix().len(), matrix_bytes);
        prop_assert_eq!(record.proof().partition().len(), partition_bytes);
        prop_assert_eq!(
            record.feasible_candidates().count() + record.infeasible_candidates().count(),
            usize::from(c)
        );
        prop_assert_eq!(
            record
                .candidates()
                .windows(2)
                .all(|pair| pair[0].candidate_id().as_str().as_bytes()
                    < pair[1].candidate_id().as_str().as_bytes()),
            true
        );
        let matrix_tail = (work % 8) as u8;
        if matrix_tail != 0 {
            let used = ((1_u16 << matrix_tail) - 1) as u8;
            prop_assert_eq!(record.failure_matrix().last().unwrap() & !used, 0);
        }
        let partition_tail = c % 8;
        if partition_tail != 0 {
            let used = ((1_u16 << partition_tail) - 1) as u8;
            prop_assert_eq!(record.proof().partition().last().unwrap() & !used, 0);
        }
        Ok(())
    });
}

#[test]
fn exact_unicode_bytes_and_emitted_bytes_both_belong_to_domain_identity() {
    let compile = |id: &str, emitted: [u8; 3]| {
        evaluate(request(
            vec![candidate(id, emitted)],
            vec![applicable(vec![Srgb8::new([255; 3])])],
        ))
        .unwrap()
    };
    let composed = compile("caf\u{e9}", [1, 2, 3]);
    let decomposed = compile("cafe\u{301}", [1, 2, 3]);
    let changed_bytes = compile("caf\u{e9}", [1, 2, 4]);

    assert_ne!(
        evaluated(&composed).domain_digest(),
        evaluated(&decomposed).domain_digest(),
        "Core must not normalize opaque Unicode IDs"
    );
    assert_ne!(
        evaluated(&composed).domain_digest(),
        evaluated(&changed_bytes).domain_digest(),
        "the emitted physical bytes are part of the declared-domain identity"
    );
    assert_ne!(
        evaluated(&composed).evaluation_id(),
        evaluated(&changed_bytes).evaluation_id()
    );
}

#[test]
fn production_identity_matches_the_independent_unicode_oracle_fixture() {
    assert_eq!(
        fixture_sha256::digest(IDENTITY_FIXTURE.as_bytes()).to_hex(),
        "92a03a0ac961163b1e0e69f3166026544af3b7c3acf89fb4d13eaaa462952d7f"
    );
    let applicable = RelationV1::applicable(
        relation_id("alpha"),
        occurrence_id("hover/🎨"),
        Wcag22CriterionV1::Sc143TextDefault,
        vec![
            Srgb8::new([255; 3]),
            Srgb8::new([0; 3]),
            Srgb8::new([118; 3]),
            Srgb8::new([0; 3]),
        ],
    )
    .unwrap();
    let not_applicable = RelationV1::not_applicable(
        relation_id("zeta"),
        occurrence_id("ornament"),
        Wcag22ClientDeclaredNotApplicableV1::try_new("client/не-применимо").unwrap(),
    );
    let result = evaluate(request(
        vec![
            candidate("🎨", [255, 128, 1]),
            candidate("é", [18, 52, 86]),
            candidate("海", [0, 0, 0]),
            candidate("e\u{301}", [18, 52, 86]),
        ],
        vec![not_applicable, applicable],
    ))
    .unwrap();
    let record = evaluated(&result);

    assert_eq!(
        record
            .candidates()
            .iter()
            .map(|value| value.candidate_id().as_str())
            .collect::<Vec<_>>(),
        ["e\u{301}", "é", "海", "🎨"]
    );
    assert_eq!(record.failure_matrix(), [0x5b, 0x0c]);
    assert_eq!(record.proof().partition(), [0x00]);
    assert_eq!(
        hex(record.domain_digest().as_bytes()),
        "71960b339a5af0421a5562e02aea28217b3f985c88a53f3244ea73f6c19258f4"
    );
    assert_eq!(
        hex(record.relation_set_digest().as_bytes()),
        "990dbc58252dc518ccf63b2f4b63ef5ae227a2bed48dda9e5e5959f3e2477132"
    );
    assert_eq!(
        hex(record.evaluation_id().as_bytes()),
        "59e69b867d8feb8afae4d28708bd353d0f3a0e89c12b0f34952f2e9a5e8be700"
    );
    assert_eq!(
        hex(record.proof().matrix_digest()),
        "f414937d1b17276054be72790c34aef5a4eb5b6dc2132122599d47297bba5507"
    );
}

#[test]
fn explicit_terminals_and_proof_views_cannot_be_forged_or_rewrapped_downstream() {
    assert_downstream_rejected(
        r#"use labcolors_core::wcag22_feasibility::explicit::{
    DomainDescriptorV1, EvaluatedV1, EvaluationProofV1, FeasibilityV1,
    NotEvaluatedV1,
};

fn wrap_feasible(value: EvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::Feasible(value)
}

fn wrap_infeasible(value: EvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::Infeasible(value)
}

fn wrap_not_evaluated(value: NotEvaluatedV1) -> FeasibilityV1 {
    FeasibilityV1::NotEvaluated(value)
}

fn main() {
    let _domain = DomainDescriptorV1 {};
    let _proof: EvaluationProofV1<'static> = EvaluationProofV1 {};
    let _evaluated = EvaluatedV1 {};
    let _not_evaluated = NotEvaluatedV1 {};
}
"#,
        &[
            "Feasible",
            "Infeasible",
            "NotEvaluated",
            "DomainDescriptorV1",
            "EvaluationProofV1",
            "EvaluatedV1",
            "NotEvaluatedV1",
        ],
    );
}
