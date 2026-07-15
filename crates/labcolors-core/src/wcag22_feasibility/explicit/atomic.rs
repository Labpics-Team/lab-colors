//! Атомарная explicit-операция (#296-C2): `feasibility → полная валидация
//! политики → selection → финальная перепроверка` за один вызов.
//!
//! Композиция не вводит второй WCAG-движок и не копирует sealed-результат A:
//! терминал A перемещается в исход без пересчёта и без повторного хеширования
//! `C×E`-матрицы. Политика валидируется единственным SSOT-валидатором
//! selection после ЛЮБОГО успешного A-терминала, поэтому некорректный хвост
//! политики не может спрятаться за `Infeasible` или `NotEvaluated`. Если сама
//! A-фаза не удалась, приоритет у feasibility-ошибки: канонического домена не
//! существует, и членство политики непроверяемо.
//!
//! Каждый терминал — запечатанная структура: снаружи Core нельзя ни собрать
//! терминал, ни перепарить feasibility-запись с чужим selection-результатом
//! через `&mut`-доступ к полям. Разобрать терминал на владеемые части можно,
//! собрать обратно — нет.
//!
//! ```compile_fail
//! use labcolors_core::wcag22_feasibility::explicit::atomic::{
//!     EvaluateAndSelectOutcomeV1, SelectedTerminalV1,
//! };
//!
//! fn forge(terminal: SelectedTerminalV1) -> EvaluateAndSelectOutcomeV1 {
//!     EvaluateAndSelectOutcomeV1::Selected(terminal)
//! }
//! ```
//!
//! ```compile_fail
//! use labcolors_core::wcag22_feasibility::explicit::atomic::SelectedTerminalV1;
//! use labcolors_core::wcag22_feasibility::explicit::EvaluatedV1;
//! use labcolors_core::wcag22_feasibility::explicit::selection::SelectedV1;
//!
//! fn reseal(feasibility: EvaluatedV1, selection: SelectedV1) -> SelectedTerminalV1 {
//!     SelectedTerminalV1 { feasibility, selection }
//! }
//! ```

use core::fmt;

use super::super::{AtomicPairEvaluator, ErrorV1, PairEvaluator, ResourceProfileIdV1};
use super::selection::{
    FirstFeasibleInDeclaredOrderV1, NoSelectionV1, PolicyDigestV1, PolicyId, SelectedV1,
    SelectionErrorV1, SelectionOutcomeV1, select_from_feasible_record_with,
    validate_complete_policy,
};
use super::{CandidateV1, EvaluatedV1, FeasibilityV1, NotEvaluatedV1, RequestV1, evaluate};

/// Core-запечатанное связывание одной полностью провалидированной политики
/// с невыборным терминалом. Конструктора вне Core нет: успешный невыборный
/// терминал доказуемо провалидировал точную клиентскую политику, но не выдаёт
/// selection-receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPolicyBindingV1 {
    policy_id: PolicyId,
    policy_digest: PolicyDigestV1,
    declared_entries: u64,
}

impl ValidatedPolicyBindingV1 {
    /// Opaque клиентская идентичность политики.
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    /// Каноническая идентичность версии, ID и полного объявленного порядка.
    pub const fn policy_digest(&self) -> PolicyDigestV1 {
        self.policy_digest
    }

    /// Точное число объявленных записей `P`.
    pub const fn declared_entries(&self) -> u64 {
        self.declared_entries
    }
}

/// Первый feasible член объявленного порядка с финальной перепроверкой.
#[derive(Debug)]
pub struct SelectedTerminalV1 {
    feasibility: EvaluatedV1,
    selection: SelectedV1,
}

impl SelectedTerminalV1 {
    /// Полный sealed-результат A, перемещённый без копирования.
    pub const fn feasibility(&self) -> &EvaluatedV1 {
        &self.feasibility
    }

    /// Sealed-выбор B с финальным receipt.
    pub const fn selection(&self) -> &SelectedV1 {
        &self.selection
    }

    /// Разобрать терминал на владеемые части. Обратной сборки вне Core нет.
    pub fn into_parts(self) -> (EvaluatedV1, SelectedV1) {
        (self.feasibility, self.selection)
    }
}

/// Валидная политика без единого feasible члена.
#[derive(Debug)]
pub struct NoSelectionTerminalV1 {
    feasibility: EvaluatedV1,
    selection: NoSelectionV1,
}

impl NoSelectionTerminalV1 {
    /// Полный sealed-результат A, перемещённый без копирования.
    pub const fn feasibility(&self) -> &EvaluatedV1 {
        &self.feasibility
    }

    /// Sealed-отказ B без скрытого fallback.
    pub const fn selection(&self) -> &NoSelectionV1 {
        &self.selection
    }

    /// Разобрать терминал на владеемые части. Обратной сборки вне Core нет.
    pub fn into_parts(self) -> (EvaluatedV1, NoSelectionV1) {
        (self.feasibility, self.selection)
    }
}

/// Полное перечисление доказало пустую feasible-партицию.
#[derive(Debug)]
pub struct InfeasibleTerminalV1 {
    feasibility: EvaluatedV1,
    policy: ValidatedPolicyBindingV1,
}

impl InfeasibleTerminalV1 {
    /// Полный sealed-результат A, перемещённый без копирования.
    pub const fn feasibility(&self) -> &EvaluatedV1 {
        &self.feasibility
    }

    /// Доказательство полной валидации точной клиентской политики.
    pub const fn policy(&self) -> &ValidatedPolicyBindingV1 {
        &self.policy
    }

    /// Разобрать терминал на владеемые части. Обратной сборки вне Core нет.
    pub fn into_parts(self) -> (EvaluatedV1, ValidatedPolicyBindingV1) {
        (self.feasibility, self.policy)
    }
}

/// Ни одно отношение не было applicable; ни одна пара не оценивалась.
#[derive(Debug)]
pub struct NotEvaluatedTerminalV1 {
    feasibility: NotEvaluatedV1,
    policy: ValidatedPolicyBindingV1,
}

impl NotEvaluatedTerminalV1 {
    /// Канонический declaration-only терминал A.
    pub const fn feasibility(&self) -> &NotEvaluatedV1 {
        &self.feasibility
    }

    /// Доказательство полной валидации точной клиентской политики.
    pub const fn policy(&self) -> &ValidatedPolicyBindingV1 {
        &self.policy
    }

    /// Разобрать терминал на владеемые части. Обратной сборки вне Core нет.
    pub fn into_parts(self) -> (NotEvaluatedV1, ValidatedPolicyBindingV1) {
        (self.feasibility, self.policy)
    }
}

/// Исчерпывающая алгебра одной атомарной explicit-операции. Каждый успешный
/// терминал связывает точную клиентскую политику; selection-результат несут
/// только `Selected | NoSelection`. Конструкторы и инварианты payload
/// запечатаны в Core.
#[derive(Debug)]
#[non_exhaustive]
pub enum EvaluateAndSelectOutcomeV1 {
    /// Первый feasible член объявленного порядка с финальной перепроверкой.
    #[non_exhaustive]
    Selected(SelectedTerminalV1),
    /// Валидная политика не содержит ни одного feasible члена.
    #[non_exhaustive]
    NoSelection(NoSelectionTerminalV1),
    /// Полное перечисление доказало пустую feasible-партицию.
    #[non_exhaustive]
    Infeasible(InfeasibleTerminalV1),
    /// Ни одно отношение не было applicable; ни одна пара не оценивалась.
    #[non_exhaustive]
    NotEvaluated(NotEvaluatedTerminalV1),
}

impl EvaluateAndSelectOutcomeV1 {
    /// Заимствовать Selected-терминал.
    pub const fn selected(&self) -> Option<&SelectedTerminalV1> {
        match self {
            Self::Selected(terminal) => Some(terminal),
            _ => None,
        }
    }

    /// Заимствовать NoSelection-терминал.
    pub const fn no_selection(&self) -> Option<&NoSelectionTerminalV1> {
        match self {
            Self::NoSelection(terminal) => Some(terminal),
            _ => None,
        }
    }

    /// Заимствовать Infeasible-терминал.
    pub const fn infeasible(&self) -> Option<&InfeasibleTerminalV1> {
        match self {
            Self::Infeasible(terminal) => Some(terminal),
            _ => None,
        }
    }

    /// Заимствовать NotEvaluated-терминал.
    pub const fn not_evaluated(&self) -> Option<&NotEvaluatedTerminalV1> {
        match self {
            Self::NotEvaluated(terminal) => Some(terminal),
            _ => None,
        }
    }
}

/// Полная алгебра отказа атомарной операции. Порядок фаз фиксирован:
/// ошибка A исключает валидацию политики, ошибка политики/selection возможна
/// только после успешного A-терминала.
#[derive(Debug)]
#[non_exhaustive]
pub enum EvaluateAndSelectErrorV1 {
    /// A-фаза не выдала терминал; политика не валидировалась.
    Feasibility(ErrorV1),
    /// Успешный A-терминал, затем невалидная политика или сорванная
    /// перепроверка selection.
    Selection(SelectionErrorV1),
}

impl fmt::Display for EvaluateAndSelectErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Feasibility(error) => write!(formatter, "feasibility phase failed: {error}"),
            Self::Selection(error) => write!(formatter, "selection phase failed: {error}"),
        }
    }
}

impl std::error::Error for EvaluateAndSelectErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Feasibility(error) => Some(error),
            Self::Selection(error) => Some(error),
        }
    }
}

fn bind_validated_policy(
    candidates: &[CandidateV1],
    candidate_count: u64,
    resource_profile_id: ResourceProfileIdV1,
    mut policy: FirstFeasibleInDeclaredOrderV1,
) -> Result<ValidatedPolicyBindingV1, SelectionErrorV1> {
    // Невыборные терминалы валидируются без чтения feasibility-битов:
    // партиция не передаётся, поэтому selection-право здесь непредставимо.
    let validation = validate_complete_policy(
        candidates,
        candidate_count,
        resource_profile_id,
        None,
        &mut policy,
    )?;
    Ok(ValidatedPolicyBindingV1 {
        policy_id: policy.into_policy_id(),
        policy_digest: validation.digest(),
        declared_entries: validation.declared_entries(),
    })
}

fn evaluate_and_select_with<E: PairEvaluator>(
    request: RequestV1,
    policy: FirstFeasibleInDeclaredOrderV1,
    selection_evaluator: &mut E,
) -> Result<EvaluateAndSelectOutcomeV1, EvaluateAndSelectErrorV1> {
    let feasibility = evaluate(request).map_err(EvaluateAndSelectErrorV1::Feasibility)?;
    match feasibility {
        FeasibilityV1::Feasible(record) => {
            let (record, outcome) =
                select_from_feasible_record_with(record, policy, selection_evaluator)
                    .map_err(EvaluateAndSelectErrorV1::Selection)?;
            Ok(match outcome {
                SelectionOutcomeV1::Selected { selected, .. } => {
                    EvaluateAndSelectOutcomeV1::Selected(SelectedTerminalV1 {
                        feasibility: record,
                        selection: selected,
                    })
                }
                SelectionOutcomeV1::NoSelection { no_selection, .. } => {
                    EvaluateAndSelectOutcomeV1::NoSelection(NoSelectionTerminalV1 {
                        feasibility: record,
                        selection: no_selection,
                    })
                }
            })
        }
        FeasibilityV1::Infeasible(record) => {
            let binding = bind_validated_policy(
                record.candidates(),
                record.domain().candidate_count(),
                record.proof().resource_profile_id(),
                policy,
            )
            .map_err(EvaluateAndSelectErrorV1::Selection)?;
            Ok(EvaluateAndSelectOutcomeV1::Infeasible(
                InfeasibleTerminalV1 {
                    feasibility: record,
                    policy: binding,
                },
            ))
        }
        FeasibilityV1::NotEvaluated(record) => {
            let binding = bind_validated_policy(
                record.candidates(),
                record.domain().candidate_count(),
                record.resource_profile_id(),
                policy,
            )
            .map_err(EvaluateAndSelectErrorV1::Selection)?;
            Ok(EvaluateAndSelectOutcomeV1::NotEvaluated(
                NotEvaluatedTerminalV1 {
                    feasibility: record,
                    policy: binding,
                },
            ))
        }
    }
}

/// Выполнить одну атомарную explicit-операцию `wcag22-explicit-selection-v1`.
///
/// Запрос не несёт клиентских счётчиков, дайджестов, матриц, evaluation ID или
/// proof: всё доказательное состояние выводится и запечатывается Core внутри
/// этого вызова. Selection-право существует только между A-терминалом
/// `Feasible` и финальной перепроверкой и не переживает сериализацию.
pub fn evaluate_and_select(
    request: RequestV1,
    policy: FirstFeasibleInDeclaredOrderV1,
) -> Result<EvaluateAndSelectOutcomeV1, EvaluateAndSelectErrorV1> {
    let mut evaluator = AtomicPairEvaluator::new();
    evaluate_and_select_with(request, policy, &mut evaluator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Srgb8;
    use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22CriterionV1, Wcag22EvaluationErrorV1};
    use crate::wcag22_feasibility::explicit::selection::SelectionIntegrityViolationV1;
    use crate::wcag22_feasibility::explicit::{CandidateId, DomainRequestV1};
    use crate::wcag22_feasibility::{
        AtomicEvidenceBindingV1, OccurrenceId, PairEvaluationV1, RelationId, RelationV1,
        expected_atomic_evidence_binding_v1,
    };

    fn candidate(id: &str, value: u8) -> CandidateV1 {
        CandidateV1::new(
            CandidateId::try_new(id).expect("test candidate ID is non-empty"),
            Srgb8::new([value; 3]),
        )
    }

    fn request(candidates: Vec<CandidateV1>, adjacent: Vec<Srgb8>) -> RequestV1 {
        let relation = RelationV1::applicable(
            RelationId::try_new("relation").unwrap(),
            OccurrenceId::try_new("occurrence").unwrap(),
            Wcag22CriterionV1::Sc143TextDefault,
            adjacent,
        )
        .unwrap();
        RequestV1::try_new(
            DomainRequestV1::try_new(candidates).unwrap(),
            vec![relation],
            ResourceProfileIdV1::Compile,
        )
        .unwrap()
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

    #[derive(Debug, Clone, Copy)]
    enum ProbeMode {
        Pass,
        FailAt(u64),
        InputMismatchAt(u64),
    }

    struct ProbeEvaluator {
        calls: u64,
        mode: ProbeMode,
        evidence: AtomicEvidenceBindingV1,
    }

    impl ProbeEvaluator {
        fn new(mode: ProbeMode) -> Self {
            Self {
                calls: 0,
                mode,
                evidence: expected_atomic_evidence_binding_v1().unwrap(),
            }
        }
    }

    impl PairEvaluator for ProbeEvaluator {
        fn evaluate_pair(
            &mut self,
            candidate: Srgb8,
            adjacent: Srgb8,
            criterion: Wcag22CriterionV1,
        ) -> Result<PairEvaluationV1, Wcag22EvaluationErrorV1> {
            self.calls += 1;
            let decision = if matches!(self.mode, ProbeMode::FailAt(call) if call == self.calls) {
                Wcag22ApplicableDecisionV1::Fail
            } else {
                Wcag22ApplicableDecisionV1::Pass
            };
            let mut foreground = candidate.bytes();
            if matches!(self.mode, ProbeMode::InputMismatchAt(call) if call == self.calls) {
                foreground[0] ^= 1;
            }
            Ok(PairEvaluationV1::Evaluated {
                foreground,
                background: adjacent.bytes(),
                criterion,
                decision,
                evidence: self.evidence.clone(),
            })
        }
    }

    #[test]
    fn combined_final_recheck_makes_exactly_one_call_per_applicable_edge() {
        let mut evaluator = ProbeEvaluator::new(ProbeMode::Pass);
        let outcome = evaluate_and_select_with(
            request(
                vec![candidate("selected", 255)],
                vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
            ),
            policy("exact-e", &["selected"]),
            &mut evaluator,
        )
        .expect("feasible singleton selects");

        assert_eq!(evaluator.calls, 3);
        let terminal = outcome.selected().expect("fixture must select");
        assert_eq!(
            terminal
                .selection()
                .final_verification()
                .verified_applicable_edges(),
            3
        );
    }

    #[test]
    fn combined_injected_mismatch_fails_closed_at_that_exact_edge() {
        for fault_at in 1..=3 {
            let mut verdict_fault = ProbeEvaluator::new(ProbeMode::FailAt(fault_at));
            let error = evaluate_and_select_with(
                request(
                    vec![candidate("selected", 255)],
                    vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
                ),
                policy("verdict-fault", &["selected"]),
                &mut verdict_fault,
            )
            .expect_err("a verdict mismatch on any edge cannot mint a combined terminal");
            assert!(matches!(
                error,
                EvaluateAndSelectErrorV1::Selection(SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedDecisionMismatch { .. }
                ))
            ));
            assert_eq!(verdict_fault.calls, fault_at);

            let mut input_fault = ProbeEvaluator::new(ProbeMode::InputMismatchAt(fault_at));
            let error = evaluate_and_select_with(
                request(
                    vec![candidate("selected", 255)],
                    vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
                ),
                policy("input-fault", &["selected"]),
                &mut input_fault,
            )
            .expect_err("an adapter mismatch on any edge cannot mint a combined terminal");
            assert!(matches!(
                error,
                EvaluateAndSelectErrorV1::Selection(SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::EvaluatorContract { .. }
                ))
            ));
            assert_eq!(input_fault.calls, fault_at);
        }
    }

    #[test]
    fn invalid_policy_and_no_selection_make_zero_final_evaluator_calls() {
        let mut evaluator = ProbeEvaluator::new(ProbeMode::Pass);

        evaluate_and_select_with(
            request(
                vec![candidate("member-a", 255), candidate("member-b", 0)],
                vec![Srgb8::new([0; 3])],
            ),
            policy("invalid", &["member-a", "foreign"]),
            &mut evaluator,
        )
        .expect_err("a feasible prefix cannot hide a foreign tail");
        assert_eq!(evaluator.calls, 0);

        let outcome = evaluate_and_select_with(
            request(
                vec![candidate("member-a", 255), candidate("member-b", 0)],
                vec![Srgb8::new([0; 3])],
            ),
            policy("no-selection", &["member-b"]),
            &mut evaluator,
        )
        .expect("valid singleton policy");
        assert!(outcome.no_selection().is_some());
        assert_eq!(evaluator.calls, 0);
    }
}
