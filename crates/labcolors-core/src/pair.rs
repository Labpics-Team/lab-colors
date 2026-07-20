//! P1: frozen one-way Pair frontend поверх общей point graph algebra.
//!
//! Модуль больше не выбирает «сторону пары», не двигает цвет по Oklab и не
//! содержит собственного solver-а. Он владеет только code-owned lowering одной
//! физической цепочки:
//!
//! ```text
//! fill Paint + page Surface
//!   → fill-on-page Occurrence
//!   → surfaceFrom(fill-on-page)
//! label Paint + emitted fill Surface
//!   → label-on-fill Occurrence
//! ```
//!
//! `PairFill` и `PairLabel` остаются временными authoring-тегами до C7c, но их
//! исполняемая физика уже принадлежит [`crate::joint`], единственному point
//! compositor-у и общим constraint evaluators. Публичного `Program` здесь нет.

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, PaintId, PointOpacityOverSurfaceV1, ResolvedOccurrence, SurfaceInputPortId,
};
use crate::composition::AdmittedOpacityV1;
use crate::joint::{
    CandidateOrdinalV1, CandidateSetErrorV1, DeclaredTotalOrderV1, FullHardReportV1,
    HardFeasibilityV1, JointCandidateSetV1, JointCandidateTupleV1, JointConstraintIdV1,
    JointHardConstraintV1, JointPointProgramV1, JointProgramErrorV1, JointReportErrorV1,
    JointVisibleTargetV1, RevisionBoundVerifiedSelectionV1, SelectedRecheckErrorV1,
    SelectionPolicyErrorV1,
};
use crate::observation::{
    ObservationError, ObservationPayloadInput, ObservationState, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, PreparedObservationViewV1, Revision,
    RevisionBoundObservationV1, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::wcag22::Wcag22CriterionV1;

const ROOT_SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(1);
const FILL_PAINT: PaintId = PaintId::new(1);
const LABEL_PAINT: PaintId = PaintId::new(2);
const CANDIDATE: CandidateOrdinalV1 = CandidateOrdinalV1::new(1);
const LABEL_CONSTRAINT: JointConstraintIdV1 = JointConstraintIdV1::new(1);
const OBSERVATION_STREAM: ObservationStreamId = ObservationStreamId::new(1);
const OBSERVATION_REVISION: Revision = Revision::new(1);
const OBSERVATION_SCENARIO: ScenarioId = ScenarioId::new(1);

/// Один канонический fill Paint, действительно применённый к page Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoweredPairFillV1 {
    paint: EncodedPointPaintV1,
    occurrence: ResolvedOccurrence,
}

impl LoweredPairFillV1 {
    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.paint
    }

    pub(crate) const fn occurrence(&self) -> &ResolvedOccurrence {
        &self.occurrence
    }

    pub(crate) fn visible(self) -> Srgb8 {
        Srgb8::new(self.occurrence.visible())
    }
}

/// Ограничение именно видимого label occurrence. Семантический frontend
/// выбирает criterion; Pair lowering не знает role/family taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairLabelRequirementV1 {
    Exact(Srgb8),
    Wcag22(Wcag22CriterionV1),
}

impl PairLabelRequirementV1 {
    fn lower(self) -> JointHardConstraintV1 {
        match self {
            Self::Exact(target) => {
                JointHardConstraintV1::exact(LABEL_CONSTRAINT, JointVisibleTargetV1::Upper, target)
            }
            Self::Wcag22(criterion) => JointHardConstraintV1::wcag22(
                LABEL_CONSTRAINT,
                JointVisibleTargetV1::Upper,
                criterion,
            ),
        }
    }
}

/// Полное fresh evidence одной выбранной Pair-кандидатуры.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VerifiedPairV1 {
    verified: RevisionBoundVerifiedSelectionV1,
}

impl VerifiedPairV1 {
    fn execution(&self) -> &crate::joint::JointExecutionRecordV1 {
        self.verified
            .fresh_executions()
            .first()
            .unwrap_or_else(|| unreachable!("one Pair candidate over one case has one execution"))
    }

    pub(crate) fn fill_paint(&self) -> EncodedPointPaintV1 {
        self.execution().lower_paint()
    }

    pub(crate) fn label_paint(&self) -> EncodedPointPaintV1 {
        self.execution().upper_paint()
    }

    pub(crate) fn fill_occurrence(&self) -> &ResolvedOccurrence {
        self.execution().lower_occurrence()
    }

    pub(crate) fn label_occurrence(&self) -> &ResolvedOccurrence {
        self.execution().upper_occurrence()
    }

    #[cfg(test)]
    pub(crate) const fn evidence(&self) -> &RevisionBoundVerifiedSelectionV1 {
        &self.verified
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PairLoweringErrorV1 {
    Observation(ObservationError),
    Candidate(CandidateSetErrorV1),
    Program(JointProgramErrorV1),
    Report(JointReportErrorV1),
    Policy(SelectionPolicyErrorV1),
    Infeasible(Box<FullHardReportV1>),
    Recheck(SelectedRecheckErrorV1),
}

/// Материализовать fill occurrence без role-specific эвристики.
pub(crate) fn lower_fill(
    source: Srgb8,
    opacity: AdmittedOpacityV1,
    backdrop: Srgb8,
) -> LoweredPairFillV1 {
    let paint = EncodedPointPaintV1::from_admitted(FILL_PAINT, source, opacity);
    let occurrence =
        PointOpacityOverSurfaceV1::evaluate_admitted(source.bytes(), opacity, backdrop.bytes());
    LoweredPairFillV1 { paint, occurrence }
}

/// Проверить одну уже предложенную label-краску через общий joint report,
/// declared total order и обязательный fresh recheck.
pub(crate) fn verify_label(
    fill_source: Srgb8,
    fill_opacity: AdmittedOpacityV1,
    label_source: Srgb8,
    backdrop: Srgb8,
    requirement: PairLabelRequirementV1,
) -> Result<VerifiedPairV1, PairLoweringErrorV1> {
    let observation = one_case_observation(backdrop).map_err(PairLoweringErrorV1::Observation)?;
    let candidates = JointCandidateSetV1::new(vec![JointCandidateTupleV1::new(
        CANDIDATE,
        EncodedPointPaintV1::from_admitted(FILL_PAINT, fill_source, fill_opacity),
        EncodedPointPaintV1::from_admitted(LABEL_PAINT, label_source, AdmittedOpacityV1::OPAQUE),
    )])
    .map_err(PairLoweringErrorV1::Candidate)?;
    let program = JointPointProgramV1::new(
        ROOT_SURFACE,
        FILL_PAINT,
        LABEL_PAINT,
        vec![requirement.lower()],
    )
    .map_err(PairLoweringErrorV1::Program)?;
    let report = program
        .evaluate(candidates, observation)
        .map_err(PairLoweringErrorV1::Report)?;
    let feasible = match report.classify() {
        HardFeasibilityV1::NonEmpty(feasible) => feasible,
        HardFeasibilityV1::Infeasible(report) => {
            return Err(PairLoweringErrorV1::Infeasible(Box::new(report)));
        }
    };
    let policy = DeclaredTotalOrderV1::new(feasible.candidate_set(), vec![CANDIDATE])
        .map_err(PairLoweringErrorV1::Policy)?;
    let verified = feasible
        .select(policy)
        .recheck()
        .map_err(PairLoweringErrorV1::Recheck)?;
    Ok(VerifiedPairV1 { verified })
}

fn one_case_observation(backdrop: Srgb8) -> Result<RevisionBoundObservationV1, ObservationError> {
    let mut state = ObservationState::new(OBSERVATION_STREAM, vec![ROOT_SURFACE])?;
    let transaction = state.prepare(ObservationUpdateInput {
        stream: OBSERVATION_STREAM,
        revision: OBSERVATION_REVISION,
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: OBSERVATION_SCENARIO,
                bindings: vec![SurfaceInputBinding {
                    port: ROOT_SURFACE,
                    value: backdrop,
                }],
            }],
        }),
    })?;
    let observation = match transaction.view() {
        PreparedObservationViewV1::AppliedObserved(observation) => observation,
        PreparedObservationViewV1::Idempotent | PreparedObservationViewV1::AppliedUnknown(_) => {
            unreachable!("fresh Pair observation is one applied physical case")
        }
    };
    let disposition = transaction.commit();
    debug_assert_eq!(disposition, crate::observation::UpdateDisposition::Applied);
    Ok(observation)
}
