//! P1: frozen Pair frontend поверх общей point graph algebra.
//!
//! Модуль не выбирает «сторону пары», не двигает цвет по Oklab и не содержит
//! собственного compositor-а или evaluator switch. Он только лоуверит
//! замороженный authoring tag в одну физическую цепочку и передаёт конечный
//! candidate domain общему joint engine.

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, PaintId, PointOpacityOverSurfaceV1, ResolvedOccurrence, SurfaceInputPortId,
};
use crate::composition::AdmittedOpacityV1;
use crate::constraints::{ExactSrgb8IdentityV1, Wcag22Srgb8V1};
use crate::joint::{
    CandidateOrdinalV1, CandidateSetErrorV1, DeclaredTotalOrderV1, JointCandidateSetV1,
    JointCandidateTupleV1, JointConstraintIdV1, JointExecutionRecordV1, JointProgramErrorV1,
    JointVisibleTargetV1, PointwiseFullHardReportV1, PointwiseHardFeasibilityV1,
    PointwiseJointHardConstraintV1, PointwiseJointPointProgramV1, PointwiseJointReportErrorV1,
    PointwiseSelectedRecheckErrorV1, PointwiseVerifiedSelectionV1, SelectionPolicyErrorV1,
    StaticJointObservationV1,
};
use crate::wcag22::Wcag22CriterionV1;

const ROOT_SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(1);
const FILL_PAINT: PaintId = PaintId::new(1);
const LABEL_PAINT: PaintId = PaintId::new(2);
const LABEL_CONSTRAINT: JointConstraintIdV1 = JointConstraintIdV1::new(1);

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

/// Один frontend-proposed label Paint. Ordinal задаёт только identity; порядок
/// предпочтения приходит отдельным declared total order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PairLabelCandidateV1 {
    ordinal: CandidateOrdinalV1,
    source: Srgb8,
}

impl PairLabelCandidateV1 {
    pub(crate) const fn new(ordinal: CandidateOrdinalV1, source: Srgb8) -> Self {
        Self { ordinal, source }
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.source
    }
}

/// Hard requirement PairLabel. Отсутствие пола означает пустой hard-set, а не
/// тождественный `Exact(candidate, candidate)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairLabelRequirementV1 {
    None,
    Wcag22(Wcag22CriterionV1),
}

#[derive(Debug, Clone, PartialEq)]
enum PairSelectionEvidenceV1 {
    Unconstrained(PointwiseVerifiedSelectionV1<ExactSrgb8IdentityV1, StaticJointObservationV1>),
    Wcag22(PointwiseVerifiedSelectionV1<Wcag22Srgb8V1, StaticJointObservationV1>),
}

/// Полное fresh evidence одной выбранной Pair-кандидатуры.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VerifiedPairV1 {
    evidence: PairSelectionEvidenceV1,
}

impl VerifiedPairV1 {
    fn execution(&self) -> &JointExecutionRecordV1 {
        let executions = match &self.evidence {
            PairSelectionEvidenceV1::Unconstrained(evidence) => evidence.fresh_executions(),
            PairSelectionEvidenceV1::Wcag22(evidence) => evidence.fresh_executions(),
        };
        executions.first().unwrap_or_else(|| {
            unreachable!("one selected Pair tuple over one case has one execution")
        })
    }

    pub(crate) fn ordinal(&self) -> CandidateOrdinalV1 {
        match &self.evidence {
            PairSelectionEvidenceV1::Unconstrained(evidence) => evidence.ordinal(),
            PairSelectionEvidenceV1::Wcag22(evidence) => evidence.ordinal(),
        }
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
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PairLoweringErrorV1 {
    Candidate(CandidateSetErrorV1),
    Program(JointProgramErrorV1),
    ExactReport(PointwiseJointReportErrorV1<ExactSrgb8IdentityV1>),
    WcagReport(PointwiseJointReportErrorV1<Wcag22Srgb8V1>),
    Policy(SelectionPolicyErrorV1),
    ExactInfeasible(Box<PointwiseFullHardReportV1<ExactSrgb8IdentityV1, StaticJointObservationV1>>),
    WcagInfeasible(Box<PointwiseFullHardReportV1<Wcag22Srgb8V1, StaticJointObservationV1>>),
    ExactRecheck(PointwiseSelectedRecheckErrorV1<ExactSrgb8IdentityV1>),
    WcagRecheck(PointwiseSelectedRecheckErrorV1<Wcag22Srgb8V1>),
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

/// Выбрать один label Paint из полного frontend candidate domain. Fill Paint
/// фиксирован authoring representation-ом, но каждый tuple исполняет обе
/// linked occurrences; hard report и fresh recheck используют тот же kernel.
pub(crate) fn select_label_candidates(
    fill_source: Srgb8,
    fill_opacity: AdmittedOpacityV1,
    candidates: Vec<PairLabelCandidateV1>,
    declared_order: Vec<CandidateOrdinalV1>,
    backdrop: Srgb8,
    requirement: PairLabelRequirementV1,
) -> Result<VerifiedPairV1, PairLoweringErrorV1> {
    let tuples = candidates
        .into_iter()
        .map(|candidate| {
            JointCandidateTupleV1::new(
                candidate.ordinal,
                EncodedPointPaintV1::from_admitted(FILL_PAINT, fill_source, fill_opacity),
                EncodedPointPaintV1::from_admitted(
                    LABEL_PAINT,
                    candidate.source,
                    AdmittedOpacityV1::OPAQUE,
                ),
            )
        })
        .collect();
    let candidates = JointCandidateSetV1::new(tuples).map_err(PairLoweringErrorV1::Candidate)?;
    let policy = DeclaredTotalOrderV1::new(&candidates, declared_order)
        .map_err(PairLoweringErrorV1::Policy)?;
    let observation = StaticJointObservationV1::one_case(ROOT_SURFACE, backdrop);

    match requirement {
        PairLabelRequirementV1::None => {
            let program = PointwiseJointPointProgramV1::with_evaluator(
                ExactSrgb8IdentityV1,
                ROOT_SURFACE,
                FILL_PAINT,
                LABEL_PAINT,
                Vec::new(),
            )
            .map_err(PairLoweringErrorV1::Program)?;
            let report = program
                .evaluate(candidates, observation)
                .map_err(PairLoweringErrorV1::ExactReport)?;
            let feasible = match report.classify() {
                PointwiseHardFeasibilityV1::NonEmpty(feasible) => feasible,
                PointwiseHardFeasibilityV1::Infeasible(report) => {
                    return Err(PairLoweringErrorV1::ExactInfeasible(Box::new(report)));
                }
            };
            let verified = feasible
                .select(policy)
                .recheck()
                .map_err(PairLoweringErrorV1::ExactRecheck)?;
            Ok(VerifiedPairV1 {
                evidence: PairSelectionEvidenceV1::Unconstrained(verified),
            })
        }
        PairLabelRequirementV1::Wcag22(criterion) => {
            let constraints = vec![PointwiseJointHardConstraintV1::new(
                LABEL_CONSTRAINT,
                JointVisibleTargetV1::Upper,
                criterion,
            )];
            let program = PointwiseJointPointProgramV1::with_evaluator(
                Wcag22Srgb8V1,
                ROOT_SURFACE,
                FILL_PAINT,
                LABEL_PAINT,
                constraints,
            )
            .map_err(PairLoweringErrorV1::Program)?;
            let report = program
                .evaluate(candidates, observation)
                .map_err(PairLoweringErrorV1::WcagReport)?;
            let feasible = match report.classify() {
                PointwiseHardFeasibilityV1::NonEmpty(feasible) => feasible,
                PointwiseHardFeasibilityV1::Infeasible(report) => {
                    return Err(PairLoweringErrorV1::WcagInfeasible(Box::new(report)));
                }
            };
            let verified = feasible
                .select(policy)
                .recheck()
                .map_err(PairLoweringErrorV1::WcagRecheck)?;
            Ok(VerifiedPairV1 {
                evidence: PairSelectionEvidenceV1::Wcag22(verified),
            })
        }
    }
}
