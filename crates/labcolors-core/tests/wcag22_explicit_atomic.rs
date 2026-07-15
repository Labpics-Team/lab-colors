//! Законы атомарной explicit-операции (#296-C2).
//!
//! Композиция `feasibility → полная валидация политики → selection → финальная
//! перепроверка` обязана: отдавать приоритет ошибке A-фазы, валидировать
//! точную политику после КАЖДОГО успешного A-терминала одинаково, связывать
//! политику с невыборными терминалами без selection-receipt и переносить
//! sealed-результат A в исход без изменения feasibility-байтов.

use std::fs;
use std::process::Command;

use labcolors_core::Srgb8;
use labcolors_core::wcag22::{Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1};
use labcolors_core::wcag22_feasibility::explicit::atomic::{
    EvaluateAndSelectErrorV1, EvaluateAndSelectOutcomeV1, evaluate_and_select,
};
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FirstFeasibleInDeclaredOrderV1, InvalidSelectionRequestV1, PolicyId, SelectionErrorV1,
    SelectionOutcomeV1, select,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainRequestV1, RequestV1, evaluate,
};
use labcolors_core::wcag22_feasibility::{
    ErrorV1, InvalidRequestV1, OccurrenceId, RelationId, RelationV1, ResourceDimensionV1,
    ResourceProfileIdV1,
};

fn candidate(id: &str, emitted: [u8; 3]) -> CandidateV1 {
    CandidateV1::new(
        CandidateId::try_new(id).expect("test candidate ID is non-empty"),
        Srgb8::new(emitted),
    )
}

fn applicable(adjacent: Vec<Srgb8>) -> RelationV1 {
    RelationV1::applicable(
        RelationId::try_new("relation").unwrap(),
        OccurrenceId::try_new("occurrence").unwrap(),
        Wcag22CriterionV1::Sc143TextDefault,
        adjacent,
    )
    .unwrap()
}

fn not_applicable() -> RelationV1 {
    RelationV1::not_applicable(
        RelationId::try_new("relation").unwrap(),
        OccurrenceId::try_new("occurrence").unwrap(),
        Wcag22ClientDeclaredNotApplicableV1::try_new("out-of-scope").unwrap(),
    )
}

fn request(candidates: Vec<CandidateV1>, relations: Vec<RelationV1>) -> RequestV1 {
    RequestV1::try_new(
        DomainRequestV1::try_new(candidates).unwrap(),
        relations,
        ResourceProfileIdV1::Compile,
    )
    .unwrap()
}

/// `member-a` feasible, `member-b` infeasible относительно чёрного фона.
fn feasible_request() -> RequestV1 {
    request(
        vec![
            candidate("member-a", [255; 3]),
            candidate("member-b", [0; 3]),
        ],
        vec![applicable(vec![Srgb8::new([0; 3])])],
    )
}

/// Оба члена проваливают единственное отношение: полная пустая партиция.
fn infeasible_request() -> RequestV1 {
    request(
        vec![candidate("member-a", [0; 3]), candidate("member-b", [1; 3])],
        vec![applicable(vec![Srgb8::new([0; 3])])],
    )
}

/// Ни одного applicable отношения: declaration-only терминал.
fn not_evaluated_request() -> RequestV1 {
    request(
        vec![
            candidate("member-a", [255; 3]),
            candidate("member-b", [0; 3]),
        ],
        vec![not_applicable()],
    )
}

fn policy(id: &str, order: &[&str]) -> FirstFeasibleInDeclaredOrderV1 {
    FirstFeasibleInDeclaredOrderV1::try_new(
        PolicyId::try_new(id).unwrap(),
        order
            .iter()
            .map(|id| CandidateId::try_new(*id).unwrap())
            .collect(),
    )
    .unwrap()
}

fn selection_error(
    result: Result<EvaluateAndSelectOutcomeV1, EvaluateAndSelectErrorV1>,
) -> SelectionErrorV1 {
    match result {
        Err(EvaluateAndSelectErrorV1::Selection(error)) => error,
        other => panic!("expected a selection-phase failure, got {other:?}"),
    }
}

#[test]
fn selected_composition_is_byte_identical_to_standalone_a_then_b() {
    let standalone = evaluate(feasible_request()).expect("A compiles");
    let standalone_selected = match select(
        standalone.selection_source().expect("fixture is feasible"),
        policy("brand", &["member-b", "member-a"]),
    )
    .expect("standalone selection succeeds")
    {
        SelectionOutcomeV1::Selected { selected, .. } => selected,
        other => panic!("standalone fixture must select, got {other:?}"),
    };
    let standalone_record = standalone.evaluated().expect("feasible carries a record");

    let combined = evaluate_and_select(
        feasible_request(),
        policy("brand", &["member-b", "member-a"]),
    )
    .expect("combined operation succeeds");
    let terminal = combined.selected().expect("combined fixture must select");

    assert_eq!(
        terminal.feasibility().evaluation_id(),
        standalone_record.evaluation_id()
    );
    assert_eq!(
        terminal.feasibility().failure_matrix(),
        standalone_record.failure_matrix()
    );
    assert_eq!(
        terminal.feasibility().proof().partition(),
        standalone_record.proof().partition()
    );
    assert_eq!(*terminal.selection(), standalone_selected);
    assert_eq!(
        terminal.selection().candidate().candidate_id().as_str(),
        "member-a",
        "the declared order lists the infeasible member first, so the first \
         feasible member must be member-a"
    );
    assert_eq!(terminal.selection().proof().selected_policy_ordinal(), 1);
}

#[test]
fn invalid_policy_tails_fail_identically_after_every_successful_a_terminal() {
    let foreign = |_: ()| policy("client", &["member-a", "foreign"]);
    let duplicate = |_: ()| policy("client", &["member-a", "member-a"]);
    let oversized = |_: ()| policy(&"p".repeat(65_535), &["member-a", "member-b"]);

    for make_policy in [foreign, duplicate, oversized] {
        let requests = [
            feasible_request(),
            infeasible_request(),
            not_evaluated_request(),
        ];
        let mut errors = Vec::new();
        for request in requests {
            errors.push(selection_error(evaluate_and_select(
                request,
                make_policy(()),
            )));
        }
        assert_eq!(
            errors[0], errors[1],
            "Feasible and Infeasible must classify the malformed policy identically"
        );
        assert_eq!(
            errors[1], errors[2],
            "Infeasible and NotEvaluated must classify the malformed policy identically"
        );
    }

    let foreign_error = selection_error(evaluate_and_select(
        feasible_request(),
        policy("client", &["member-a", "foreign"]),
    ));
    assert!(
        matches!(
            foreign_error,
            SelectionErrorV1::InvalidRequest(InvalidSelectionRequestV1::ForeignCandidateId { .. })
        ),
        "a feasible prefix must not hide a foreign tail: {foreign_error:?}"
    );
    let oversized_error = selection_error(evaluate_and_select(
        not_evaluated_request(),
        policy(&"p".repeat(65_535), &["member-a", "member-b"]),
    ));
    assert!(
        matches!(
            oversized_error,
            SelectionErrorV1::ResourceLimitExceeded {
                dimension: ResourceDimensionV1::OpaqueUtf8Bytes,
                ..
            }
        ),
        "byte-envelope breaches stay ResourceLimitExceeded: {oversized_error:?}"
    );
}

#[test]
fn a_phase_failure_has_priority_over_any_policy_defect() {
    let duplicate_domain = RequestV1::try_new(
        DomainRequestV1::try_new(vec![
            candidate("member-a", [255; 3]),
            candidate("member-a", [0; 3]),
        ])
        .unwrap(),
        vec![applicable(vec![Srgb8::new([0; 3])])],
        ResourceProfileIdV1::Compile,
    )
    .unwrap();

    // Политика тоже некорректна (foreign ID), но канонического домена нет —
    // приоритет обязан быть у feasibility-ошибки.
    let error = evaluate_and_select(duplicate_domain, policy("client", &["foreign"]))
        .expect_err("a duplicate domain cannot produce a terminal");
    assert!(
        matches!(
            error,
            EvaluateAndSelectErrorV1::Feasibility(ErrorV1::InvalidRequest(
                InvalidRequestV1::DuplicateCandidateId { .. }
            ))
        ),
        "feasibility errors must win over policy validation: {error:?}"
    );
}

#[test]
fn valid_non_feasible_terminals_bind_the_exact_policy_without_a_selection_receipt() {
    // Дифференциал закона дайджеста: автономный B на другом (feasible) домене
    // с той же политикой обязан дать байт-в-байт тот же policy digest.
    let standalone = evaluate(feasible_request()).expect("A compiles");
    let standalone_no_selection = match select(
        standalone.selection_source().expect("fixture is feasible"),
        policy("shared-policy", &["member-b"]),
    )
    .expect("valid singleton policy")
    {
        SelectionOutcomeV1::NoSelection { no_selection, .. } => no_selection,
        other => panic!("member-b is infeasible in the fixture, got {other:?}"),
    };

    let infeasible =
        evaluate_and_select(infeasible_request(), policy("shared-policy", &["member-b"]))
            .expect("a valid policy over an infeasible terminal binds");
    let terminal = infeasible.infeasible().expect("fixture must be infeasible");
    assert_eq!(terminal.policy().policy_id().as_str(), "shared-policy");
    assert_eq!(terminal.policy().declared_entries(), 1);
    assert_eq!(
        terminal.policy().policy_digest(),
        standalone_no_selection.policy_digest(),
        "the combined binding must reuse the sole policy-digest law"
    );
    assert!(
        terminal
            .feasibility()
            .feasible_candidates()
            .next()
            .is_none(),
        "the moved Infeasible record must keep its empty feasible partition"
    );

    let not_evaluated = evaluate_and_select(
        not_evaluated_request(),
        policy("shared-policy", &["member-b"]),
    )
    .expect("a valid policy over a declaration-only terminal binds");
    let terminal = not_evaluated
        .not_evaluated()
        .expect("fixture must be NotEvaluated");
    assert_eq!(terminal.policy().policy_id().as_str(), "shared-policy");
    assert_eq!(
        terminal.policy().policy_digest(),
        standalone_no_selection.policy_digest()
    );
    assert_eq!(terminal.feasibility().domain().candidate_count(), 2);
}

#[test]
fn no_selection_is_atomic_and_binds_the_source_evaluation() {
    let outcome = evaluate_and_select(feasible_request(), policy("client", &["member-b"]))
        .expect("valid singleton policy");
    let terminal = outcome
        .no_selection()
        .expect("member-b is infeasible, expected NoSelection");
    assert_eq!(
        terminal.selection().evaluation_id(),
        terminal.feasibility().evaluation_id()
    );
    assert_eq!(terminal.selection().policy_id().as_str(), "client");
}

#[test]
fn opposite_orders_keep_feasibility_bytes_and_change_selection_binding() {
    let fixture = || {
        request(
            vec![
                candidate("member-a", [255; 3]),
                candidate("member-b", [254; 3]),
            ],
            vec![applicable(vec![Srgb8::new([0; 3])])],
        )
    };
    let forward = evaluate_and_select(fixture(), policy("forward", &["member-a", "member-b"]))
        .expect("forward order selects");
    let reverse = evaluate_and_select(fixture(), policy("reverse", &["member-b", "member-a"]))
        .expect("reverse order selects");

    let forward = forward.selected().expect("forward fixture must select");
    let reverse = reverse.selected().expect("reverse fixture must select");

    assert_eq!(
        forward.feasibility().evaluation_id(),
        reverse.feasibility().evaluation_id()
    );
    assert_eq!(
        forward.feasibility().failure_matrix(),
        reverse.feasibility().failure_matrix()
    );
    assert_eq!(
        forward.feasibility().proof().partition(),
        reverse.feasibility().proof().partition()
    );
    assert_eq!(
        forward.selection().candidate().candidate_id().as_str(),
        "member-a"
    );
    assert_eq!(
        reverse.selection().candidate().candidate_id().as_str(),
        "member-b"
    );
    assert_ne!(
        forward.selection().policy_digest(),
        reverse.selection().policy_digest(),
        "opposite declared orders must produce different policy bindings"
    );
}

#[test]
fn downstream_code_cannot_forge_or_mispair_atomic_terminals() {
    // Конструирование терминала снаружи Core не компилируется: варианты
    // #[non_exhaustive], payload-структуры без публичных конструкторов.
    assert_downstream_rejected(
        r#"
use labcolors_core::wcag22_feasibility::explicit::atomic::{
    EvaluateAndSelectOutcomeV1, SelectedTerminalV1,
};

fn forge(terminal: SelectedTerminalV1) -> EvaluateAndSelectOutcomeV1 {
    EvaluateAndSelectOutcomeV1::Selected(terminal)
}

fn main() {}
"#,
        &["is private"],
    );

    // Пересборка payload из владеемых частей отклоняется: поля приватны.
    assert_downstream_rejected(
        r#"
use labcolors_core::wcag22_feasibility::explicit::atomic::SelectedTerminalV1;
use labcolors_core::wcag22_feasibility::explicit::EvaluatedV1;
use labcolors_core::wcag22_feasibility::explicit::selection::SelectedV1;

fn reseal(feasibility: EvaluatedV1, selection: SelectedV1) -> SelectedTerminalV1 {
    SelectedTerminalV1 { feasibility, selection }
}

fn main() {}
"#,
        &["private field"],
    );

    // Перепаривание feasibility-записи с чужим selection через &mut-доступ
    // к полям терминала невозможно: поля приватны, есть только getters.
    assert_downstream_rejected(
        r#"
use labcolors_core::wcag22_feasibility::explicit::atomic::SelectedTerminalV1;

fn mispair(left: &mut SelectedTerminalV1, right: &mut SelectedTerminalV1) {
    core::mem::swap(&mut left.selection, &mut right.selection);
}

fn main() {}
"#,
        &["is private"],
    );

    // Связывание политики не собирается из клиентских кусков.
    assert_downstream_rejected(
        r#"
use labcolors_core::wcag22_feasibility::explicit::atomic::ValidatedPolicyBindingV1;
use labcolors_core::wcag22_feasibility::explicit::selection::PolicyId;

fn forge(policy_id: PolicyId) -> ValidatedPolicyBindingV1 {
    ValidatedPolicyBindingV1 { policy_id }
}

fn main() {}
"#,
        &["private field"],
    );
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
            "[package]\nname = \"forge-explicit-atomic\"\nversion = \"0.0.0\"\n\
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
        "forged atomic terminal unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for fragment in expected_fragments {
        assert!(
            stderr.contains(fragment),
            "expected downstream rejection mentioning {fragment:?}, stderr:\n{stderr}"
        );
    }
}
