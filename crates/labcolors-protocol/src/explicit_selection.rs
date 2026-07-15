//! Строгий транспорт атомарной операции `wcag22-explicit-selection-v1` (#296-C2).
//!
//! Один запрос несёт ТОЛЬКО клиентский конечный домен, отношения, resource
//! profile и политику. Никакие счётчики, дайджесты, матрицы, evaluation ID,
//! proof или receipt не принимаются с провода: всё доказательное состояние
//! выводит и запечатывает Core за один вызов. Сериализованный результат —
//! wire/conformance evidence; он никогда не принимается обратно как selection
//! authority, поэтому ни один исходовый тип не реализует `Deserialize`.
//!
//! ```compile_fail
//! let _: labcolors_protocol::explicit_selection::OutcomeV1 =
//!     serde_json::from_str(r#"{"schemaVersion":1,"outcome":"success"}"#).unwrap();
//! ```
//!
//! ```compile_fail
//! let _: labcolors_protocol::explicit_selection::SelectedV1 =
//!     serde_json::from_str("{}").unwrap();
//! ```
//!
//! Неизвестные schema/domain/profile/policy kind падают в этом строгом
//! декодере типизированными transport-ошибками; классификация семантики
//! (пустые/дублирующие ID, ресурсные пределы, integrity) остаётся за Core.

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use labcolors_core::Srgb8;
use labcolors_core::wcag22::Wcag22ApplicableDecisionV1 as CoreDecisionV1;
use labcolors_core::wcag22_feasibility::explicit::atomic::{
    EvaluateAndSelectErrorV1 as CoreAtomicErrorV1,
    EvaluateAndSelectOutcomeV1 as CoreAtomicOutcomeV1,
    ValidatedPolicyBindingV1 as CoreValidatedPolicyBindingV1, evaluate_and_select,
};
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FinalRelationVerificationV1 as CoreFinalVerificationV1,
    FirstFeasibleInDeclaredOrderV1 as CorePolicyV1,
    InvalidSelectionRequestV1 as CoreInvalidSelectionRequestV1,
    NoSelectionReasonV1 as CoreNoSelectionReasonV1, NoSelectionV1 as CoreNoSelectionV1,
    PolicyId as CorePolicyId, SelectedV1 as CoreSelectedV1,
    SelectionErrorV1 as CoreSelectionErrorV1,
    SelectionIntegrityViolationV1 as CoreSelectionIntegrityViolationV1,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId as CoreCandidateId, CandidateV1 as CoreCandidateV1,
    DomainKindV1 as CoreDomainKindV1, DomainRequestV1 as CoreDomainRequestV1,
    EvaluatedV1 as CoreExplicitEvaluatedV1, NotEvaluatedV1 as CoreExplicitNotEvaluatedV1,
    RequestV1 as CoreExplicitRequestV1,
};
use labcolors_core::wcag22_feasibility::{
    RelationV1 as CoreRelationV1, ResourceProfileIdV1 as CoreResourceProfileIdV1,
};

use crate::{
    APPLICABLE_SKELETON_BYTES_V1, CoreErrorV1, CoreInvalidRequestV1, DomainDigestV1,
    EvaluationIdV1, EvaluatorInvariantV1, MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1,
    MAX_RGB_TRIPLE_BYTES_V1, MalformedEnvelopeClassV1, NOT_APPLICABLE_SKELETON_BYTES_V1,
    NumericalArtifactIdV2, NumericalErrorBoundIdV2, NumericalProofIdV2, OPAQUE_UTF8_LIMIT_V1,
    ProtocolEncodingErrorV1, ProtocolErrorV1, RAW_ADJACENT_LIMIT_V1, RAW_RELATION_LIMIT_V1,
    RawRelationV1, RelationSetDigestV1, RelationV1, ResourceDimensionV1, ResourceProfileIdV1,
    SCHEMA_VERSION_V1, TransportErrorV1, Wcag22CriterionV1, Wcag22ProfileIdV1, map_core_invalid,
    project_core_error, project_evaluator_invariant, project_relations,
    project_resource_profile_id, serialize_u64_decimal, usize_as_u64,
};

/// Стабильный Core-ключ единственного V1 explicit-домена.
pub const DOMAIN_KIND_KEY_V1: &str = CoreDomainKindV1::ExplicitSrgb8Set.key();
/// Стабильный Core-ключ единственного V1 policy kind.
pub const POLICY_KIND_KEY_V1: &str = CorePolicyV1::KIND_KEY_V1;

// ─────────────────────────────────────────────────────────────────────────────
// Выведенный envelope: точный максимум compact-запроса, допустимого каждым
// RAW-измерением профиля Compile. Дубликаты ID на этом ярусе ещё допустимы
// (канонизация Core их отклонит ПОСЛЕ приёма конверта), поэтому 1-байтовые
// escaped-ID достижимы, и потолок точен без дискреционного запаса.
// ─────────────────────────────────────────────────────────────────────────────

const FIXED_ENVELOPE_BYTES: u64 = b"{\"schemaVersion\":1,\"domainId\":\"".len() as u64
    + DOMAIN_KIND_KEY_V1.len() as u64
    + b"\",\"resourceProfileId\":\"".len() as u64
    + CoreResourceProfileIdV1::Compile.key().len() as u64
    + b"\",\"candidates\":[".len() as u64
    + b"],\"relations\":[".len() as u64
    + b"],\"policy\":{\"policyKind\":\"".len() as u64
    + POLICY_KIND_KEY_V1.len() as u64
    + b"\",\"policyId\":\"".len() as u64
    + b"\",\"orderedCandidateIds\":[".len() as u64
    + b"]}}".len() as u64;

const CANDIDATE_SKELETON_BYTES: u64 = b"{\"candidateId\":\"\",\"emitted\":".len() as u64
    + MAX_RGB_TRIPLE_BYTES_V1
    + b"}".len() as u64;

// Маржинальная длина JSON на один opaque-байт feasibility-бюджета.
// Кандидат: скелет + разделитель + 6 байт escape за 1 opaque-байт.
const CANDIDATE_MARGINAL_BYTES: u64 =
    CANDIDATE_SKELETON_BYTES + 1 + MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1;
// Applicable-отношение: скелет + один RGB-триплет + разделитель + два
// escaped 1-байтовых ID за 2 opaque-байта.
const APPLICABLE_MARGINAL_BYTES: u64 = APPLICABLE_SKELETON_BYTES_V1
    + MAX_RGB_TRIPLE_BYTES_V1
    + 1
    + 2 * MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1;
// NotApplicable-отношение: скелет + разделитель + три escaped ID за 3 байта.
const NOT_APPLICABLE_MARGINAL_BYTES: u64 =
    NOT_APPLICABLE_SKELETON_BYTES_V1 + 1 + 3 * MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1;

// Линейный корнер-анализ (машинно-проверяемый): каждый applicable-слот с одним
// adjacent даёт больше JSON-байтов, чем два кандидата на те же 2 opaque-байта,
// а NotApplicable-слот даёт меньше, чем applicable-слот плюс кандидат на
// освободившийся байт. Следовательно максимум — на угле «максимум applicable
// отношений, по одному adjacent, остальной opaque-бюджет — кандидатам».
const _: () = assert!(APPLICABLE_MARGINAL_BYTES > 2 * CANDIDATE_MARGINAL_BYTES);
const _: () =
    assert!(APPLICABLE_MARGINAL_BYTES + CANDIDATE_MARGINAL_BYTES > NOT_APPLICABLE_MARGINAL_BYTES);
// Каждая запись объявленного порядка (8 байт JSON + разделитель за 1 opaque-байт)
// длиннее, чем тот же байт в policyId (6 байт escape), поэтому selection-бюджет
// уходит в orderedCandidateIds при минимальном непустом policyId.
const ORDERED_ID_MARGINAL_BYTES: u64 =
    b"\"\"".len() as u64 + MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1 + 1;
const _: () = assert!(ORDERED_ID_MARGINAL_BYTES > MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1);

const MAX_CANDIDATES_AT_ENVELOPE: u64 = OPAQUE_UTF8_LIMIT_V1 - 2 * RAW_RELATION_LIMIT_V1;
const MAX_ORDERED_IDS_AT_ENVELOPE: u64 = OPAQUE_UTF8_LIMIT_V1 - 1;

/// Точный максимум байтов compact-запроса `wcag22-explicit-selection-v1`,
/// допускаемого каждым RAW-измерением профиля `compile-v1`. Без запаса.
pub const MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1: u64 = FIXED_ENVELOPE_BYTES
    // Кандидаты: 1-байтовые escaped ID, максимальные RGB-триплеты.
    + MAX_CANDIDATES_AT_ENVELOPE * CANDIDATE_SKELETON_BYTES
    + MAX_CANDIDATES_AT_ENVELOPE * MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1
    + (MAX_CANDIDATES_AT_ENVELOPE - 1)
    // Отношения: максимум applicable-слотов, ровно один adjacent на каждый.
    + RAW_RELATION_LIMIT_V1 * APPLICABLE_SKELETON_BYTES_V1
    + RAW_ADJACENT_LIMIT_V1 * MAX_RGB_TRIPLE_BYTES_V1
    + RAW_RELATION_LIMIT_V1 * 2 * MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1
    + (RAW_RELATION_LIMIT_V1 - 1)
    + (RAW_ADJACENT_LIMIT_V1 - RAW_RELATION_LIMIT_V1)
    // Политика: 1-байтовый policyId, остальной selection-бюджет — записям порядка.
    + MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1
    + MAX_ORDERED_IDS_AT_ENVELOPE * (b"\"\"".len() as u64 + MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1)
    + (MAX_ORDERED_IDS_AT_ENVELOPE - 1);

// ─────────────────────────────────────────────────────────────────────────────
// Запрос
// ─────────────────────────────────────────────────────────────────────────────

/// Один explicit-кандидат в каноническом wire-виде.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateV1 {
    candidate_id: String,
    emitted: [u8; 3],
}

impl CandidateV1 {
    /// Построить локально валидного кандидата. Пустой ID классифицирует Core.
    pub fn new(
        candidate_id: impl Into<String>,
        emitted: [u8; 3],
    ) -> Result<Self, OperationErrorV1> {
        let candidate_id = candidate_id.into();
        if candidate_id.is_empty() {
            return Err(feasibility_invalid(CoreInvalidRequestV1::EmptyCandidateId));
        }
        Ok(Self {
            candidate_id,
            emitted,
        })
    }

    /// Opaque клиентская идентичность.
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    /// Точные финальные sRGB8-байты.
    pub const fn emitted(&self) -> [u8; 3] {
        self.emitted
    }
}

impl Serialize for CandidateV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("CandidateV1", 2)?;
        state.serialize_field("candidateId", &self.candidate_id)?;
        state.serialize_field("emitted", &self.emitted)?;
        state.end()
    }
}

/// Единственная V1-политика в каноническом wire-виде.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyV1 {
    policy_id: String,
    ordered_candidate_ids: Vec<String>,
}

impl PolicyV1 {
    /// Построить локально валидную политику. Агрегатные законы остаются Core.
    pub fn first_feasible_in_declared_order(
        policy_id: impl Into<String>,
        ordered_candidate_ids: Vec<String>,
    ) -> Result<Self, OperationErrorV1> {
        let policy_id = policy_id.into();
        if policy_id.is_empty() {
            return Err(selection_invalid(InvalidSelectionRequestV1::EmptyPolicyId));
        }
        if ordered_candidate_ids.is_empty() {
            return Err(selection_invalid(
                InvalidSelectionRequestV1::EmptyCandidateOrder,
            ));
        }
        Ok(Self {
            policy_id,
            ordered_candidate_ids,
        })
    }

    /// Opaque клиентская идентичность политики.
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    /// Точный объявленный порядок.
    pub fn ordered_candidate_ids(&self) -> &[String] {
        &self.ordered_candidate_ids
    }
}

impl Serialize for PolicyV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PolicyV1", 3)?;
        state.serialize_field("policyKind", POLICY_KIND_KEY_V1)?;
        state.serialize_field("policyId", &self.policy_id)?;
        state.serialize_field("orderedCandidateIds", &self.ordered_candidate_ids)?;
        state.end()
    }
}

/// Валидированный V1-запрос атомарной операции для канонического кодирования.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestV1 {
    schema_version: u32,
    domain_id: DomainKindWireV1,
    resource_profile_id: ResourceProfileIdV1,
    candidates: Vec<CandidateV1>,
    relations: Vec<RelationV1>,
    policy: PolicyV1,
}

impl RequestV1 {
    /// Построить локально валидный запрос. Агрегатные resource/conflict-законы
    /// остаются Core-owned и проверяются только после transport-preflight.
    pub fn try_new(
        candidates: Vec<CandidateV1>,
        relations: Vec<RelationV1>,
        policy: PolicyV1,
    ) -> Result<Self, OperationErrorV1> {
        if candidates.is_empty() {
            return Err(feasibility_invalid(CoreInvalidRequestV1::EmptyCandidates));
        }
        if relations.is_empty() {
            return Err(feasibility_invalid(CoreInvalidRequestV1::EmptyRelations));
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION_V1,
            domain_id: DomainKindWireV1::ExplicitSrgb8Set,
            resource_profile_id: ResourceProfileIdV1::Compile,
            candidates,
            relations,
            policy,
        })
    }

    /// Объявленные кандидаты в клиентском порядке.
    pub fn candidates(&self) -> &[CandidateV1] {
        &self.candidates
    }

    /// Объявленные отношения в клиентском порядке.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    /// Объявленная политика.
    pub const fn policy(&self) -> &PolicyV1 {
        &self.policy
    }
}

/// Wire-ключ Core-owned kind explicit-домена.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainKindWireV1 {
    /// Канонический набор opaque ID с точными финальными sRGB8-байтами.
    ExplicitSrgb8Set,
}

impl DomainKindWireV1 {
    /// Стабильный Core-ключ.
    pub const fn key(self) -> &'static str {
        DOMAIN_KIND_KEY_V1
    }
}

impl Serialize for DomainKindWireV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Строгие raw-DTO декодера
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRequestV1 {
    schema_version: u32,
    domain_id: String,
    resource_profile_id: String,
    candidates: Vec<RawCandidateV1>,
    relations: Vec<RawRelationV1>,
    policy: RawPolicyV1,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawCandidateV1 {
    candidate_id: String,
    emitted: [u8; 3],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPolicyV1 {
    policy_kind: String,
    policy_id: String,
    ordered_candidate_ids: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire-проекции исхода (Serialize-only)
// ─────────────────────────────────────────────────────────────────────────────

/// Sealed полностью спроецированное доказательство explicit-перечисления.
/// Доменно-нейтральный дескриптор: kind, дайджест и точная конечная
/// кардинальность; neutral-only полей `domainFirst/domainLast` здесь нет.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationProofV1 {
    evaluation_id: EvaluationIdV1,
    resource_profile_id: ResourceProfileIdV1,
    domain_kind: DomainKindWireV1,
    domain_digest: DomainDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    candidate_count: u64,
    relation_set_digest: RelationSetDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    canonical_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    applicable_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    not_applicable_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    applicable_edges: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    logical_assessments: u64,
    matrix_digest: [u8; 32],
    /// Переменная feasible-партиция, LSB0 по каноническому индексу кандидата.
    partition: Vec<u8>,
    wcag22_profile_id: Wcag22ProfileIdV1,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    proof_sha256: [u8; 32],
}

/// Полный evaluated-payload обоих полных терминалов.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedV1 {
    /// Канонические кандидаты, транспортируются один раз.
    candidates: Vec<CandidateV1>,
    relations: Vec<RelationV1>,
    failure_matrix: Vec<u8>,
    proof: EvaluationProofV1,
}

impl EvaluatedV1 {
    /// Канонические кандидаты в точном порядке байтов ID.
    pub fn candidates(&self) -> &[CandidateV1] {
        &self.candidates
    }

    /// Канонические объявления, транспортируются один раз.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    /// Candidate-major LSB0 packed матрица отказов.
    pub fn failure_matrix(&self) -> &[u8] {
        &self.failure_matrix
    }

    /// Sealed доказательство полного перечисления.
    pub const fn proof(&self) -> &EvaluationProofV1 {
        &self.proof
    }
}

/// Declaration-only терминал без сфабрикованного числового доказательства.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotEvaluatedV1 {
    domain_kind: DomainKindWireV1,
    domain_digest: DomainDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    candidate_count: u64,
    relation_set_digest: RelationSetDigestV1,
    resource_profile_id: ResourceProfileIdV1,
    candidates: Vec<CandidateV1>,
    relations: Vec<RelationV1>,
}

/// Wire-проекция атомарного WCAG-вердикта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionV1 {
    /// Порог выполнен.
    Pass,
    /// Порог не выполнен.
    Fail,
}

impl Serialize for DecisionV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        })
    }
}

/// Sealed финальная перепроверка выбранной строки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalVerificationV1 {
    relation_set_digest: RelationSetDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    verified_applicable_edges: u64,
    wcag22_profile_id: Wcag22ProfileIdV1,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    proof_sha256: [u8; 32],
    receipt_digest: [u8; 32],
}

impl FinalVerificationV1 {
    /// Точное число перепроверенных канонических applicable-рёбер.
    pub const fn verified_applicable_edges(&self) -> u64 {
        self.verified_applicable_edges
    }
}

/// Sealed выбранный кандидат с финальным receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedV1 {
    candidate_id: String,
    emitted: [u8; 3],
    evaluation_id: EvaluationIdV1,
    policy_id: String,
    policy_digest: [u8; 32],
    #[serde(serialize_with = "serialize_u64_decimal")]
    selected_policy_ordinal: u64,
    receipt_digest: [u8; 32],
    final_verification: FinalVerificationV1,
}

impl SelectedV1 {
    /// Выбранный opaque ID.
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    /// Точные финальные sRGB8-байты выбора.
    pub const fn emitted(&self) -> [u8; 3] {
        self.emitted
    }

    /// Финальная перепроверка каждого применимого ребра.
    pub const fn final_verification(&self) -> &FinalVerificationV1 {
        &self.final_verification
    }
}

/// Sealed отказ валидной политики без скрытого fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoSelectionV1 {
    reason: NoSelectionReasonV1,
    policy_id: String,
    policy_digest: [u8; 32],
    evaluation_id: EvaluationIdV1,
}

/// Исчерпывающая причина отсутствия выбора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoSelectionReasonV1 {
    /// Ни один объявленный ID не принадлежит feasible-партиции.
    NoDeclaredCandidateFeasible,
}

impl Serialize for NoSelectionReasonV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::NoDeclaredCandidateFeasible => "noDeclaredCandidateFeasible",
        })
    }
}

/// Sealed связывание полностью провалидированной политики.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyBindingV1 {
    policy_id: String,
    policy_digest: [u8; 32],
    #[serde(serialize_with = "serialize_u64_decimal")]
    declared_entries: u64,
}

/// Успешный результат: ровно один из четырёх законных терминалов.
/// Payload-типы variant-sealed: клонировать evidence можно, пересобрать чужой
/// терминал из кусков — нет.
///
/// ```compile_fail
/// use labcolors_protocol::explicit_selection::{EvaluatedV1, PolicyBindingV1, ResultV1};
///
/// fn rewrap(feasibility: EvaluatedV1, policy: PolicyBindingV1) -> ResultV1 {
///     ResultV1::Infeasible { feasibility, policy }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[non_exhaustive]
pub enum ResultV1 {
    /// Первый feasible член объявленного порядка с финальной перепроверкой.
    #[non_exhaustive]
    Selected {
        /// Полный sealed-результат A.
        feasibility: EvaluatedV1,
        /// Sealed-выбор B.
        selection: SelectedV1,
    },
    /// Валидная политика без единого feasible члена.
    #[non_exhaustive]
    NoSelection {
        /// Полный sealed-результат A.
        feasibility: EvaluatedV1,
        /// Sealed-отказ B.
        selection: NoSelectionV1,
    },
    /// Полное перечисление доказало пустую feasible-партицию.
    #[non_exhaustive]
    Infeasible {
        /// Полный sealed-результат A.
        feasibility: EvaluatedV1,
        /// Полностью провалидированная политика без selection-receipt.
        policy: PolicyBindingV1,
    },
    /// Ни одно отношение не было applicable.
    #[non_exhaustive]
    NotEvaluated {
        /// Declaration-only терминал A.
        feasibility: NotEvaluatedV1,
        /// Полностью провалидированная политика без selection-receipt.
        policy: PolicyBindingV1,
    },
}

impl ResultV1 {
    /// Заимствовать evaluated-payload обоих полных терминалов.
    pub const fn evaluated(&self) -> Option<&EvaluatedV1> {
        match self {
            Self::Selected { feasibility, .. }
            | Self::NoSelection { feasibility, .. }
            | Self::Infeasible { feasibility, .. } => Some(feasibility),
            Self::NotEvaluated { .. } => None,
        }
    }

    /// Заимствовать sealed-выбор, если терминал `Selected`.
    pub const fn selected(&self) -> Option<&SelectedV1> {
        match self {
            Self::Selected { selection, .. } => Some(selection),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire-проекции ошибок
// ─────────────────────────────────────────────────────────────────────────────

/// Невалидный или противоречивый selection-вход, спроецированный из Core.
/// `EmptyCandidateId` — декодерная фазовая атрибуция Core-закона непустых ID
/// к записи объявленного порядка; остальные коды взаимно однозначны с Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InvalidSelectionRequestV1 {
    /// Пустая идентичность политики.
    EmptyPolicyId,
    /// Пустой объявленный порядок.
    EmptyCandidateOrder,
    /// Пустой ID внутри объявленного порядка.
    EmptyCandidateId,
    /// Проверяемая арифметика размеров переполнилась.
    ArithmeticOverflow,
    /// Объявленный порядок не может быть подмножеством конечного домена.
    PolicyCardinalityExceedsDomain {
        /// Объявленные записи.
        #[serde(serialize_with = "serialize_u64_decimal")]
        requested: u64,
        /// Точная кардинальность домена.
        #[serde(serialize_with = "serialize_u64_decimal")]
        domain: u64,
    },
    /// Объявленный ID вне запечатанного домена.
    ForeignCandidateId {
        /// Точный opaque ID.
        candidate_id: String,
    },
    /// Один и тот же ID объявлен в порядке более одного раза.
    DuplicateCandidateId {
        /// Точный opaque ID.
        candidate_id: String,
    },
}

/// Перепроверка выбранной строки разошлась с sealed-доказательством.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SelectionIntegrityViolationV1 {
    /// Общий proof-bound атомарный evaluator нарушил adapter-контракт.
    EvaluatorContract {
        /// Выбранный opaque ID.
        candidate_id: String,
        /// Opaque идентичность отношения.
        relation_id: String,
        /// Точные adjacent-байты.
        adjacent: [u8; 3],
        /// Точная причина инварианта.
        violation: EvaluatorInvariantV1,
    },
    /// Финальный атомарный вердикт разошёлся с sealed-ячейкой.
    SealedDecisionMismatch {
        /// Выбранный opaque ID.
        candidate_id: String,
        /// Opaque идентичность отношения.
        relation_id: String,
        /// Точные adjacent-байты.
        adjacent: [u8; 3],
        /// Sealed-вердикт.
        sealed: DecisionV1,
        /// Перепроверенный вердикт.
        rechecked: DecisionV1,
    },
    /// Якобы feasible выбранная строка содержала непроходную sealed-ячейку.
    SelectedRowNotPassing {
        /// Выбранный opaque ID.
        candidate_id: String,
        /// Opaque идентичность отношения.
        relation_id: String,
        /// Точные adjacent-байты.
        adjacent: [u8; 3],
    },
    /// Канонический граф разошёлся со своим sealed-числом рёбер.
    ApplicableEdgeCountMismatch {
        /// Ожидаемое точное число.
        #[serde(serialize_with = "serialize_u64_decimal")]
        expected: u64,
        /// Наблюдаемое точное число.
        #[serde(serialize_with = "serialize_u64_decimal")]
        observed: u64,
    },
    /// Sealed-обход превысил уже допущенный целочисленный конверт.
    SealedTraversalArithmeticOverflow,
}

/// Полная алгебра отказа selection-фазы, спроецированная как данные.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    content = "details",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SelectionErrorV1 {
    /// Невалидный или противоречивый объявленный вход.
    InvalidRequest(InvalidSelectionRequestV1),
    /// Одно точное preflight-измерение превысило профиль источника.
    ResourceLimitExceeded {
        /// Допускающий профиль.
        profile_id: ResourceProfileIdV1,
        /// Отклонённое измерение.
        dimension: ResourceDimensionV1,
        /// Запрошенное точное число.
        #[serde(serialize_with = "serialize_u64_decimal")]
        requested: u64,
        /// Допущенное точное число.
        #[serde(serialize_with = "serialize_u64_decimal")]
        limit: u64,
    },
    /// Финальная перепроверка разошлась с sealed-доказательством.
    IntegrityViolation(SelectionIntegrityViolationV1),
}

/// Источник отказа на публичной границе атомарной операции.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", content = "error", rename_all = "camelCase")]
pub enum OperationErrorV1 {
    /// Отказ raw-конверта или строгой схемы.
    Transport(TransportErrorV1),
    /// Точный отказ A-фазы; политика не валидировалась.
    Feasibility(CoreErrorV1),
    /// Отказ политики или перепроверки после успешного A-терминала.
    Selection(SelectionErrorV1),
    /// Будущий non-exhaustive Core-вариант не представим этой схемой.
    IncompatibleCoreContract,
}

/// Тотальный публичный результат атомарной операции: успех с одним законным
/// терминалом или отказ с типизированными данными и БЕЗ частичного
/// feasibility-payload.
///
/// Тип намеренно не реализует `Deserialize`: proof-несущие терминалы
/// получаются только исполнением Core, а не парсингом чужого JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
// Успешный терминал держит bounded owned-buffer заголовки inline — как и
// нейтральный `ProtocolOutcomeV1`: без лишней heap-аллокации, OOM-ребра и
// indirection на каждый вызов компилятора. Это не wire-ABI.
#[allow(clippy::large_enum_variant)]
pub enum OutcomeV1 {
    /// Успешная атомарная операция.
    Success {
        /// Ровно один законный терминал.
        result: ResultV1,
    },
    /// Отказ без частичного feasibility-терминала.
    Failure {
        /// Типизированные данные отказа.
        error: OperationErrorV1,
    },
}

impl OutcomeV1 {
    /// Заимствовать успешный терминал.
    pub const fn result(&self) -> Option<&ResultV1> {
        match self {
            Self::Success { result } => Some(result),
            Self::Failure { .. } => None,
        }
    }

    /// Заимствовать типизированные данные отказа.
    pub const fn error(&self) -> Option<&OperationErrorV1> {
        match self {
            Self::Success { .. } => None,
            Self::Failure { error } => Some(error),
        }
    }

    fn failure(error: OperationErrorV1) -> Self {
        Self::Failure { error }
    }
}

impl Serialize for OutcomeV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Success { result } => {
                let mut state = serializer.serialize_struct("OutcomeV1", 3)?;
                state.serialize_field("schemaVersion", &SCHEMA_VERSION_V1)?;
                state.serialize_field("outcome", "success")?;
                state.serialize_field("result", result)?;
                state.end()
            }
            Self::Failure { error } => {
                let mut state = serializer.serialize_struct("OutcomeV1", 3)?;
                state.serialize_field("schemaVersion", &SCHEMA_VERSION_V1)?;
                state.serialize_field("outcome", "failure")?;
                state.serialize_field("error", error)?;
                state.end()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Кодирование
// ─────────────────────────────────────────────────────────────────────────────

/// Закодировать валидированный запрос детерминированным compact UTF-8 JSON.
pub fn encode_explicit_selection_request_v1(
    request: &RequestV1,
) -> Result<Vec<u8>, ProtocolEncodingErrorV1> {
    serde_json::to_vec(request).map_err(|_| ProtocolEncodingErrorV1::SerializationFailed)
}

/// Закодировать sealed-исход детерминированным compact UTF-8 JSON.
pub fn encode_explicit_selection_outcome_v1(
    outcome: &OutcomeV1,
) -> Result<Vec<u8>, ProtocolEncodingErrorV1> {
    serde_json::to_vec(outcome).map_err(|_| ProtocolEncodingErrorV1::SerializationFailed)
}

/// Канонический типизированный отказ для host-side байтового preflight.
pub fn explicit_selection_envelope_too_large_outcome_v1(requested_bytes: u64) -> OutcomeV1 {
    OutcomeV1::failure(OperationErrorV1::Transport(
        TransportErrorV1::EnvelopeTooLarge {
            requested_bytes,
            limit_bytes: MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1,
        },
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Исполнение
// ─────────────────────────────────────────────────────────────────────────────

/// Выполнить один точный raw UTF-8 JSON-конверт через единственную атомарную
/// Core-операцию.
///
/// Длина отклоняется до UTF-8-валидации, JSON-парсинга, вложенных аллокаций и
/// работы Core. Каждая public-input и Core-ошибка становится типизированным
/// [`OutcomeV1::Failure`]; ни `Result`, ни panic, ни fallback наружу не выходят.
pub fn evaluate_wcag22_explicit_selection_v1(raw: &[u8]) -> OutcomeV1 {
    evaluate_with_decoder(raw, decode_request)
}

fn evaluate_with_decoder<F>(raw: &[u8], decoder: F) -> OutcomeV1
where
    F: FnOnce(&str) -> Result<(CoreExplicitRequestV1, CorePolicyV1), OperationErrorV1>,
{
    let requested_bytes = usize_as_u64(raw.len());
    if requested_bytes > MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1 {
        return explicit_selection_envelope_too_large_outcome_v1(requested_bytes);
    }
    let text = match core::str::from_utf8(raw) {
        Ok(text) => text,
        Err(_) => {
            return OutcomeV1::failure(OperationErrorV1::Transport(TransportErrorV1::InvalidUtf8));
        }
    };
    let (request, policy) = match decoder(text) {
        Ok(decoded) => decoded,
        Err(error) => return OutcomeV1::failure(error),
    };
    match evaluate_and_select(request, policy) {
        Ok(outcome) => match project_outcome(&outcome) {
            Ok(result) => OutcomeV1::Success { result },
            Err(error) => OutcomeV1::failure(error),
        },
        Err(CoreAtomicErrorV1::Feasibility(error)) => {
            OutcomeV1::failure(project_feasibility_error(error))
        }
        Err(CoreAtomicErrorV1::Selection(error)) => {
            OutcomeV1::failure(project_selection_error(error))
        }
        Err(_) => OutcomeV1::failure(OperationErrorV1::IncompatibleCoreContract),
    }
}

fn transport(error: TransportErrorV1) -> OperationErrorV1 {
    OperationErrorV1::Transport(error)
}

fn feasibility_invalid(error: CoreInvalidRequestV1) -> OperationErrorV1 {
    OperationErrorV1::Feasibility(CoreErrorV1::InvalidRequest(error))
}

fn selection_invalid(error: InvalidSelectionRequestV1) -> OperationErrorV1 {
    OperationErrorV1::Selection(SelectionErrorV1::InvalidRequest(error))
}

/// Перенос уже спроецированной неявной feasibility-ошибки (`ProtocolErrorV1`)
/// в источники атомарной операции без второй классификации.
fn lift_protocol_error(error: ProtocolErrorV1) -> OperationErrorV1 {
    match error {
        ProtocolErrorV1::Transport(error) => OperationErrorV1::Transport(error),
        ProtocolErrorV1::Core(error) => OperationErrorV1::Feasibility(error),
        ProtocolErrorV1::IncompatibleCoreContract => OperationErrorV1::IncompatibleCoreContract,
    }
}

fn project_feasibility_error(
    error: labcolors_core::wcag22_feasibility::ErrorV1,
) -> OperationErrorV1 {
    lift_protocol_error(project_core_error(error))
}

fn decode_request(text: &str) -> Result<(CoreExplicitRequestV1, CorePolicyV1), OperationErrorV1> {
    let raw: RawRequestV1 = serde_json::from_str(text).map_err(|error| {
        let class = match error.classify() {
            serde_json::error::Category::Io => MalformedEnvelopeClassV1::Io,
            serde_json::error::Category::Syntax => MalformedEnvelopeClassV1::Syntax,
            serde_json::error::Category::Data => MalformedEnvelopeClassV1::Shape,
            serde_json::error::Category::Eof => MalformedEnvelopeClassV1::EndOfInput,
        };
        transport(TransportErrorV1::MalformedEnvelope { class })
    })?;
    if raw.schema_version != SCHEMA_VERSION_V1 {
        return Err(transport(TransportErrorV1::UnsupportedSchemaVersion {
            received: raw.schema_version,
        }));
    }
    if raw.domain_id != DOMAIN_KIND_KEY_V1 {
        return Err(transport(TransportErrorV1::UnsupportedDomainId {
            received: raw.domain_id,
        }));
    }
    if raw.resource_profile_id != CoreResourceProfileIdV1::Compile.key() {
        return Err(transport(TransportErrorV1::UnsupportedResourceProfileId {
            received: raw.resource_profile_id,
        }));
    }

    // A-фаза раньше B-фазы: дефект формы кандидатов/отношений имеет приоритет
    // над дефектом формы политики, как и в Core.
    let mut candidates = Vec::with_capacity(raw.candidates.len());
    for candidate in raw.candidates {
        let candidate_id = CoreCandidateId::try_new(candidate.candidate_id)
            .map_err(|error| lift_protocol_error(map_core_invalid(error)))?;
        candidates.push(CoreCandidateV1::new(
            candidate_id,
            Srgb8::new(candidate.emitted),
        ));
    }
    let domain = CoreDomainRequestV1::try_new(candidates)
        .map_err(|error| lift_protocol_error(map_core_invalid(error)))?;

    let relations = raw
        .relations
        .into_iter()
        .map(|relation| {
            decode_relation(relation)
                .and_then(|relation| relation.into_core().map_err(lift_protocol_error))
        })
        .collect::<Result<Vec<CoreRelationV1>, OperationErrorV1>>()?;
    let request =
        CoreExplicitRequestV1::try_new(domain, relations, CoreResourceProfileIdV1::Compile)
            .map_err(|error| lift_protocol_error(map_core_invalid(error)))?;

    if raw.policy.policy_kind != POLICY_KIND_KEY_V1 {
        return Err(transport(TransportErrorV1::UnsupportedPolicyKind {
            received: raw.policy.policy_kind,
        }));
    }
    let policy_id = CorePolicyId::try_new(raw.policy.policy_id)
        .map_err(project_invalid_selection_request_as_error)?;
    let mut ordered = Vec::with_capacity(raw.policy.ordered_candidate_ids.len());
    for candidate_id in raw.policy.ordered_candidate_ids {
        // Единственный закон непустых opaque ID принадлежит Core; декодер лишь
        // атрибутирует его selection-фазе, которой принадлежит эта запись.
        let candidate_id = CoreCandidateId::try_new(candidate_id)
            .map_err(|_| selection_invalid(InvalidSelectionRequestV1::EmptyCandidateId))?;
        ordered.push(candidate_id);
    }
    let policy = CorePolicyV1::try_new(policy_id, ordered)
        .map_err(project_invalid_selection_request_as_error)?;
    Ok((request, policy))
}

fn decode_relation(relation: RawRelationV1) -> Result<RelationV1, OperationErrorV1> {
    match relation {
        RawRelationV1::Applicable {
            relation_id,
            occurrence_id,
            criterion,
            adjacent,
        } => {
            let criterion = Wcag22CriterionV1::parse(&criterion).ok_or_else(|| {
                transport(TransportErrorV1::UnsupportedCriterion {
                    received: criterion,
                })
            })?;
            RelationV1::applicable(relation_id, occurrence_id, criterion, adjacent)
                .map_err(lift_protocol_error)
        }
        RawRelationV1::NotApplicable {
            relation_id,
            occurrence_id,
            reason_id,
        } => RelationV1::not_applicable(relation_id, occurrence_id, reason_id)
            .map_err(lift_protocol_error),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core → wire проекции
// ─────────────────────────────────────────────────────────────────────────────

fn project_outcome(outcome: &CoreAtomicOutcomeV1) -> Result<ResultV1, OperationErrorV1> {
    // Sealed-терминалы Core заимствуются только через аксессоры: их payload
    // намеренно не разбирается позиционно за пределами Core.
    if let Some(terminal) = outcome.selected() {
        return Ok(ResultV1::Selected {
            feasibility: project_evaluated(terminal.feasibility())?,
            selection: project_selected(terminal.selection())?,
        });
    }
    if let Some(terminal) = outcome.no_selection() {
        return Ok(ResultV1::NoSelection {
            feasibility: project_evaluated(terminal.feasibility())?,
            selection: project_no_selection(terminal.selection())?,
        });
    }
    if let Some(terminal) = outcome.infeasible() {
        return Ok(ResultV1::Infeasible {
            feasibility: project_evaluated(terminal.feasibility())?,
            policy: project_policy_binding(terminal.policy()),
        });
    }
    if let Some(terminal) = outcome.not_evaluated() {
        return Ok(ResultV1::NotEvaluated {
            feasibility: project_not_evaluated(terminal.feasibility())?,
            policy: project_policy_binding(terminal.policy()),
        });
    }
    Err(OperationErrorV1::IncompatibleCoreContract)
}

fn project_candidates(values: &[CoreCandidateV1]) -> Vec<CandidateV1> {
    values
        .iter()
        .map(|candidate| CandidateV1 {
            candidate_id: candidate.candidate_id().as_str().to_string(),
            emitted: candidate.emitted().bytes(),
        })
        .collect()
}

fn project_domain_kind(value: CoreDomainKindV1) -> Result<DomainKindWireV1, OperationErrorV1> {
    match value {
        CoreDomainKindV1::ExplicitSrgb8Set => Ok(DomainKindWireV1::ExplicitSrgb8Set),
        _ => Err(OperationErrorV1::IncompatibleCoreContract),
    }
}

fn project_evaluated(value: &CoreExplicitEvaluatedV1) -> Result<EvaluatedV1, OperationErrorV1> {
    let proof = value.proof();
    let domain = value.domain();
    Ok(EvaluatedV1 {
        candidates: project_candidates(value.candidates()),
        relations: project_relations(value.relations()).map_err(lift_protocol_error)?,
        failure_matrix: value.failure_matrix().to_vec(),
        proof: EvaluationProofV1 {
            evaluation_id: EvaluationIdV1(*value.evaluation_id().as_bytes()),
            resource_profile_id: project_resource_profile_id(proof.resource_profile_id())
                .map_err(lift_protocol_error)?,
            domain_kind: project_domain_kind(domain.kind())?,
            domain_digest: DomainDigestV1(*domain.digest().as_bytes()),
            candidate_count: domain.candidate_count(),
            relation_set_digest: RelationSetDigestV1(*value.relation_set_digest().as_bytes()),
            canonical_relations: proof.canonical_relations(),
            applicable_relations: proof.applicable_relations(),
            not_applicable_relations: proof.not_applicable_relations(),
            applicable_edges: proof.applicable_edges(),
            logical_assessments: proof.logical_assessments(),
            matrix_digest: *proof.matrix_digest(),
            partition: proof.partition().to_vec(),
            wcag22_profile_id: crate::project_wcag22_profile_id(proof.profile_id())
                .map_err(lift_protocol_error)?,
            artifact_id: crate::project_artifact_id(proof.artifact_id())
                .map_err(lift_protocol_error)?,
            bound_id: crate::project_bound_id(proof.bound_id()).map_err(lift_protocol_error)?,
            proof_id: crate::project_proof_id(proof.proof_id()).map_err(lift_protocol_error)?,
            proof_sha256: *proof.proof_sha256(),
        },
    })
}

fn project_not_evaluated(
    value: &CoreExplicitNotEvaluatedV1,
) -> Result<NotEvaluatedV1, OperationErrorV1> {
    let domain = value.domain();
    Ok(NotEvaluatedV1 {
        domain_kind: project_domain_kind(domain.kind())?,
        domain_digest: DomainDigestV1(*domain.digest().as_bytes()),
        candidate_count: domain.candidate_count(),
        relation_set_digest: RelationSetDigestV1(*value.relation_set_digest().as_bytes()),
        resource_profile_id: project_resource_profile_id(value.resource_profile_id())
            .map_err(lift_protocol_error)?,
        candidates: project_candidates(value.candidates()),
        relations: project_relations(value.relations()).map_err(lift_protocol_error)?,
    })
}

fn project_final_verification(
    value: &CoreFinalVerificationV1,
) -> Result<FinalVerificationV1, OperationErrorV1> {
    Ok(FinalVerificationV1 {
        relation_set_digest: RelationSetDigestV1(*value.relation_set_digest().as_bytes()),
        verified_applicable_edges: value.verified_applicable_edges(),
        wcag22_profile_id: crate::project_wcag22_profile_id(value.profile_id())
            .map_err(lift_protocol_error)?,
        artifact_id: crate::project_artifact_id(value.artifact_id())
            .map_err(lift_protocol_error)?,
        bound_id: crate::project_bound_id(value.bound_id()).map_err(lift_protocol_error)?,
        proof_id: crate::project_proof_id(value.proof_id()).map_err(lift_protocol_error)?,
        proof_sha256: *value.proof_sha256(),
        receipt_digest: *value.receipt_digest().as_bytes(),
    })
}

fn project_selected(value: &CoreSelectedV1) -> Result<SelectedV1, OperationErrorV1> {
    Ok(SelectedV1 {
        candidate_id: value.candidate().candidate_id().as_str().to_string(),
        emitted: value.candidate().emitted().bytes(),
        evaluation_id: EvaluationIdV1(*value.evaluation_id().as_bytes()),
        policy_id: value.policy_id().as_str().to_string(),
        policy_digest: *value.policy_digest().as_bytes(),
        selected_policy_ordinal: value.proof().selected_policy_ordinal(),
        receipt_digest: *value.receipt_digest().as_bytes(),
        final_verification: project_final_verification(value.final_verification())?,
    })
}

fn project_no_selection(value: &CoreNoSelectionV1) -> Result<NoSelectionV1, OperationErrorV1> {
    let reason = match value.reason() {
        CoreNoSelectionReasonV1::NoDeclaredCandidateFeasible => {
            NoSelectionReasonV1::NoDeclaredCandidateFeasible
        }
        _ => return Err(OperationErrorV1::IncompatibleCoreContract),
    };
    Ok(NoSelectionV1 {
        reason,
        policy_id: value.policy_id().as_str().to_string(),
        policy_digest: *value.policy_digest().as_bytes(),
        evaluation_id: EvaluationIdV1(*value.evaluation_id().as_bytes()),
    })
}

fn project_policy_binding(value: &CoreValidatedPolicyBindingV1) -> PolicyBindingV1 {
    PolicyBindingV1 {
        policy_id: value.policy_id().as_str().to_string(),
        policy_digest: *value.policy_digest().as_bytes(),
        declared_entries: value.declared_entries(),
    }
}

fn project_invalid_selection_request(
    value: CoreInvalidSelectionRequestV1,
) -> Result<InvalidSelectionRequestV1, ()> {
    match value {
        CoreInvalidSelectionRequestV1::EmptyPolicyId => {
            Ok(InvalidSelectionRequestV1::EmptyPolicyId)
        }
        CoreInvalidSelectionRequestV1::EmptyCandidateOrder => {
            Ok(InvalidSelectionRequestV1::EmptyCandidateOrder)
        }
        CoreInvalidSelectionRequestV1::ArithmeticOverflow => {
            Ok(InvalidSelectionRequestV1::ArithmeticOverflow)
        }
        CoreInvalidSelectionRequestV1::PolicyCardinalityExceedsDomain { requested, domain } => {
            Ok(InvalidSelectionRequestV1::PolicyCardinalityExceedsDomain { requested, domain })
        }
        CoreInvalidSelectionRequestV1::ForeignCandidateId { candidate_id } => {
            Ok(InvalidSelectionRequestV1::ForeignCandidateId {
                candidate_id: candidate_id.as_str().to_string(),
            })
        }
        CoreInvalidSelectionRequestV1::DuplicateCandidateId { candidate_id } => {
            Ok(InvalidSelectionRequestV1::DuplicateCandidateId {
                candidate_id: candidate_id.as_str().to_string(),
            })
        }
        _ => Err(()),
    }
}

fn project_invalid_selection_request_as_error(
    value: CoreInvalidSelectionRequestV1,
) -> OperationErrorV1 {
    match project_invalid_selection_request(value) {
        Ok(error) => selection_invalid(error),
        Err(()) => OperationErrorV1::IncompatibleCoreContract,
    }
}

fn project_decision(value: CoreDecisionV1) -> Result<DecisionV1, ()> {
    match value {
        CoreDecisionV1::Pass => Ok(DecisionV1::Pass),
        CoreDecisionV1::Fail => Ok(DecisionV1::Fail),
        _ => Err(()),
    }
}

fn project_selection_integrity(
    value: CoreSelectionIntegrityViolationV1,
) -> Result<SelectionIntegrityViolationV1, ()> {
    match value {
        CoreSelectionIntegrityViolationV1::EvaluatorContract {
            candidate_id,
            relation_id,
            adjacent,
            violation,
        } => Ok(SelectionIntegrityViolationV1::EvaluatorContract {
            candidate_id: candidate_id.as_str().to_string(),
            relation_id: relation_id.as_str().to_string(),
            adjacent: adjacent.bytes(),
            violation: project_evaluator_invariant(violation)?,
        }),
        CoreSelectionIntegrityViolationV1::SealedDecisionMismatch {
            candidate_id,
            relation_id,
            adjacent,
            sealed,
            rechecked,
        } => Ok(SelectionIntegrityViolationV1::SealedDecisionMismatch {
            candidate_id: candidate_id.as_str().to_string(),
            relation_id: relation_id.as_str().to_string(),
            adjacent: adjacent.bytes(),
            sealed: project_decision(sealed)?,
            rechecked: project_decision(rechecked)?,
        }),
        CoreSelectionIntegrityViolationV1::SelectedRowNotPassing {
            candidate_id,
            relation_id,
            adjacent,
        } => Ok(SelectionIntegrityViolationV1::SelectedRowNotPassing {
            candidate_id: candidate_id.as_str().to_string(),
            relation_id: relation_id.as_str().to_string(),
            adjacent: adjacent.bytes(),
        }),
        CoreSelectionIntegrityViolationV1::ApplicableEdgeCountMismatch { expected, observed } => {
            Ok(SelectionIntegrityViolationV1::ApplicableEdgeCountMismatch { expected, observed })
        }
        CoreSelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow => {
            Ok(SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow)
        }
        _ => Err(()),
    }
}

fn project_selection_error(error: CoreSelectionErrorV1) -> OperationErrorV1 {
    let projected = match error {
        CoreSelectionErrorV1::InvalidRequest(error) => {
            project_invalid_selection_request(error).map(SelectionErrorV1::InvalidRequest)
        }
        CoreSelectionErrorV1::ResourceLimitExceeded {
            profile_id,
            dimension,
            requested,
            limit,
        } => {
            let profile_id = match project_resource_profile_id(profile_id) {
                Ok(value) => value,
                Err(_) => return OperationErrorV1::IncompatibleCoreContract,
            };
            let dimension = match crate::project_resource_dimension(dimension) {
                Ok(value) => value,
                Err(()) => return OperationErrorV1::IncompatibleCoreContract,
            };
            Ok(SelectionErrorV1::ResourceLimitExceeded {
                profile_id,
                dimension,
                requested,
                limit,
            })
        }
        CoreSelectionErrorV1::IntegrityViolation(violation) => {
            project_selection_integrity(violation).map(SelectionErrorV1::IntegrityViolation)
        }
        _ => Err(()),
    };
    match projected {
        Ok(error) => OperationErrorV1::Selection(error),
        Err(()) => OperationErrorV1::IncompatibleCoreContract,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_core_request() -> (CoreExplicitRequestV1, CorePolicyV1) {
        let candidate = CoreCandidateV1::new(
            CoreCandidateId::try_new("member").unwrap(),
            Srgb8::new([255; 3]),
        );
        let relation = CoreRelationV1::applicable(
            labcolors_core::wcag22_feasibility::RelationId::try_new("relation").unwrap(),
            labcolors_core::wcag22_feasibility::OccurrenceId::try_new("occurrence").unwrap(),
            labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextDefault,
            vec![Srgb8::new([0; 3])],
        )
        .unwrap();
        let request = CoreExplicitRequestV1::try_new(
            CoreDomainRequestV1::try_new(vec![candidate]).unwrap(),
            vec![relation],
            CoreResourceProfileIdV1::Compile,
        )
        .unwrap();
        let policy = CorePolicyV1::try_new(
            CorePolicyId::try_new("policy").unwrap(),
            vec![CoreCandidateId::try_new("member").unwrap()],
        )
        .unwrap();
        (request, policy)
    }

    #[test]
    fn raw_byte_preflight_calls_no_decoder_above_limit_and_one_at_limit() {
        use std::cell::Cell;

        let calls = Cell::new(0_u32);
        let over =
            vec![b' '; usize::try_from(MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1 + 1).unwrap()];
        let outcome = evaluate_with_decoder(&over, |_| {
            calls.set(calls.get() + 1);
            Ok(one_core_request())
        });
        assert_eq!(calls.get(), 0);
        assert_eq!(
            outcome,
            explicit_selection_envelope_too_large_outcome_v1(
                MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1 + 1
            )
        );

        let at_limit =
            vec![b' '; usize::try_from(MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1).unwrap()];
        let outcome = evaluate_with_decoder(&at_limit, |_| {
            calls.set(calls.get() + 1);
            Ok(one_core_request())
        });
        assert_eq!(calls.get(), 1);
        assert!(matches!(
            outcome,
            OutcomeV1::Success {
                result: ResultV1::Selected { .. }
            }
        ));
    }
}
