//! Приватный V2a-срез совместного point-selection.
//!
//! Один code-owned program связывает две Paint-переменные через реальный
//! `lower occurrence -> visible surface -> upper occurrence`. Candidate domain,
//! полный hard-report, declared policy и fresh recheck являются разными типами.
//! Модуль не знает клиентских recipes, role taxonomy или evaluator families и
//! не минтит terminal output certificate.

use core::fmt::Debug;

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, ModeledSrgb8PointOccurrence, PaintId, PointOpacityOverSurfaceV1,
    ResolvedOccurrence, SurfaceInputPortId,
};
use crate::constraints::{
    Evaluator, ExactSrgb8IdentityV1, HardClassifier, HardDecision, PointInvocation,
    PointMeasurement, VisiblePointPassEvidence, VisiblePointViolationEvidence,
    assess_visible_point_hard,
};
use crate::observation::{RevisionBoundObservationV1, ScenarioId};
use crate::session::SessionObservationBindingPermitV1;

/// Sealed evaluator family, которую joint-program вызывает одинаково для
/// каждого target occurrence. Конкретные Exact/WCAG/readability payload-и
/// остаются в evaluator-модулях и не образуют центральный enum.
pub(crate) trait JointPointEvaluatorV1: Clone + Debug + PartialEq {
    type Invocation: Clone + Debug + PartialEq;
    type PassEvidence: Clone + Debug + PartialEq;
    type ViolationEvidence: Clone + Debug + PartialEq;
    type Error: Clone + Debug + PartialEq;

    fn assess(
        &self,
        occurrence: &ResolvedOccurrence,
        invocation: Self::Invocation,
    ) -> Result<HardDecision<Self::PassEvidence, Self::ViolationEvidence>, Self::Error>;
}

impl<Evaluation> JointPointEvaluatorV1 for Evaluation
where
    Evaluation: Clone
        + Debug
        + PartialEq
        + Evaluator<ModeledSrgb8PointOccurrence>
        + HardClassifier<PointInvocation<Evaluation>, PointMeasurement<Evaluation>>,
    PointInvocation<Evaluation>: Clone + Debug + PartialEq,
    VisiblePointPassEvidence<Evaluation>: Clone + Debug + PartialEq,
    VisiblePointViolationEvidence<Evaluation>: Clone + Debug + PartialEq,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Error: Clone + Debug + PartialEq,
{
    type Invocation = PointInvocation<Evaluation>;
    type PassEvidence = VisiblePointPassEvidence<Evaluation>;
    type ViolationEvidence = VisiblePointViolationEvidence<Evaluation>;
    type Error = <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Error;

    fn assess(
        &self,
        occurrence: &ResolvedOccurrence,
        invocation: Self::Invocation,
    ) -> Result<HardDecision<Self::PassEvidence, Self::ViolationEvidence>, Self::Error> {
        assess_visible_point_hard(occurrence, self, invocation)
    }
}

/// Canonical identity одного joint candidate. Число не является declaration
/// order, расстоянием или скрытым приоритетом.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CandidateOrdinalV1(u32);

impl CandidateOrdinalV1 {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Две solver-owned Paint-переменные одного code-owned joint program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JointCandidateTupleV1 {
    ordinal: CandidateOrdinalV1,
    lower: EncodedPointPaintV1,
    upper: EncodedPointPaintV1,
}

impl JointCandidateTupleV1 {
    pub(crate) const fn new(
        ordinal: CandidateOrdinalV1,
        lower: EncodedPointPaintV1,
        upper: EncodedPointPaintV1,
    ) -> Self {
        Self {
            ordinal,
            lower,
            upper,
        }
    }
}

/// Order-free candidate domain. Policy не участвует в его construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JointCandidateSetV1 {
    candidates: Box<[JointCandidateTupleV1]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateSetErrorV1 {
    Empty,
    DuplicateOrdinal(CandidateOrdinalV1),
    DuplicatePhysicalTuple {
        first: CandidateOrdinalV1,
        second: CandidateOrdinalV1,
    },
}

impl JointCandidateSetV1 {
    pub(crate) fn new(
        mut candidates: Vec<JointCandidateTupleV1>,
    ) -> Result<Self, CandidateSetErrorV1> {
        if candidates.is_empty() {
            return Err(CandidateSetErrorV1::Empty);
        }
        candidates.sort_unstable_by_key(|candidate| candidate.ordinal);
        for pair in candidates.windows(2) {
            if pair[0].ordinal == pair[1].ordinal {
                return Err(CandidateSetErrorV1::DuplicateOrdinal(pair[0].ordinal));
            }
        }
        for (index, first) in candidates.iter().enumerate() {
            if let Some(second) = candidates[index + 1..]
                .iter()
                .find(|second| first.lower == second.lower && first.upper == second.upper)
            {
                return Err(CandidateSetErrorV1::DuplicatePhysicalTuple {
                    first: first.ordinal,
                    second: second.ordinal,
                });
            }
        }
        Ok(Self {
            candidates: candidates.into_boxed_slice(),
        })
    }

    pub(crate) fn candidates(&self) -> &[JointCandidateTupleV1] {
        &self.candidates
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct JointConstraintIdV1(u32);

impl JointConstraintIdV1 {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Physical occurrence, к которому относится hard predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointVisibleTargetV1 {
    Lower,
    Upper,
}

/// Один constraint конкретного evaluator family. Invocation типизирована самим
/// evaluator-ом; family-specific enum в joint engine отсутствует.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseJointHardConstraintV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    id: JointConstraintIdV1,
    target: JointVisibleTargetV1,
    invocation: Evaluation::Invocation,
}

impl<Evaluation> PointwiseJointHardConstraintV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    pub(crate) fn new(
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        invocation: Evaluation::Invocation,
    ) -> Self {
        Self {
            id,
            target,
            invocation,
        }
    }
}

pub(crate) type JointHardConstraintV1 = PointwiseJointHardConstraintV1<ExactSrgb8IdentityV1>;

impl PointwiseJointHardConstraintV1<ExactSrgb8IdentityV1> {
    pub(crate) fn exact(
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        invocation: Srgb8,
    ) -> Self {
        Self::new(id, target, invocation)
    }
}

/// Identity первой private joint topology. Она не является public Program ID и
/// не кодирует evaluator family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointPointProgramIdentityV1 {
    TwoPaintDerivedSurfacePointV1,
}

/// Наблюдение, пригодное для одного и того же execution/recheck kernel-а.
/// Runtime revision и статическая compiler binding остаются разными типами.
mod observation_seal {
    pub(crate) trait Sealed {}
}

pub(crate) trait JointObservationV1: observation_seal::Sealed + Debug + PartialEq {
    fn case_count(&self) -> usize;
    fn surface_at(&self, case_index: usize, surface: SurfaceInputPortId) -> Option<Srgb8>;
    fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]>;
}

impl observation_seal::Sealed for RevisionBoundObservationV1 {}

impl JointObservationV1 for RevisionBoundObservationV1 {
    fn case_count(&self) -> usize {
        self.set().cases().len()
    }

    fn surface_at(&self, case_index: usize, surface: SurfaceInputPortId) -> Option<Srgb8> {
        let bindings = self.physical_bindings(case_index)?;
        let index = bindings
            .binary_search_by_key(&surface, |binding| binding.port())
            .ok()?;
        Some(bindings[index].value())
    }

    fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        RevisionBoundObservationV1::provenance(self, case_index)
    }
}

/// Один статический point case для build/synchronous resolver path. Тип не
/// содержит runtime stream/revision и не изобретает provenance, которой у
/// синхронного вызова нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticJointObservationV1 {
    root_surface: SurfaceInputPortId,
    root: Srgb8,
}

impl StaticJointObservationV1 {
    pub(crate) const fn one_case(root_surface: SurfaceInputPortId, root: Srgb8) -> Self {
        Self { root_surface, root }
    }
}

impl observation_seal::Sealed for StaticJointObservationV1 {}

impl JointObservationV1 for StaticJointObservationV1 {
    fn case_count(&self) -> usize {
        1
    }

    fn surface_at(&self, case_index: usize, surface: SurfaceInputPortId) -> Option<Srgb8> {
        (case_index == 0 && surface == self.root_surface).then_some(self.root)
    }

    fn provenance(&self, _case_index: usize) -> Option<&[ScenarioId]> {
        None
    }
}

/// Две связанные occurrences над одним observed root backdrop.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseJointPointProgramV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    evaluator: Evaluation,
    root_surface: SurfaceInputPortId,
    lower_paint: PaintId,
    upper_paint: PaintId,
    constraints: Box<[PointwiseJointHardConstraintV1<Evaluation>]>,
}

pub(crate) type JointPointProgramV1 = PointwiseJointPointProgramV1<ExactSrgb8IdentityV1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointProgramErrorV1 {
    SamePaintIdentity(PaintId),
    DuplicateConstraint(JointConstraintIdV1),
}

impl PointwiseJointPointProgramV1<ExactSrgb8IdentityV1> {
    pub(crate) fn new(
        root_surface: SurfaceInputPortId,
        lower_paint: PaintId,
        upper_paint: PaintId,
        constraints: Vec<JointHardConstraintV1>,
    ) -> Result<Self, JointProgramErrorV1> {
        Self::with_evaluator(
            ExactSrgb8IdentityV1,
            root_surface,
            lower_paint,
            upper_paint,
            constraints,
        )
    }
}

impl<Evaluation> PointwiseJointPointProgramV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    pub(crate) fn with_evaluator(
        evaluator: Evaluation,
        root_surface: SurfaceInputPortId,
        lower_paint: PaintId,
        upper_paint: PaintId,
        mut constraints: Vec<PointwiseJointHardConstraintV1<Evaluation>>,
    ) -> Result<Self, JointProgramErrorV1> {
        if lower_paint == upper_paint {
            return Err(JointProgramErrorV1::SamePaintIdentity(lower_paint));
        }
        constraints.sort_unstable_by_key(|constraint| constraint.id);
        for pair in constraints.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(JointProgramErrorV1::DuplicateConstraint(pair[0].id));
            }
        }
        Ok(Self {
            evaluator,
            root_surface,
            lower_paint,
            upper_paint,
            constraints: constraints.into_boxed_slice(),
        })
    }

    const fn identity(&self) -> JointPointProgramIdentityV1 {
        JointPointProgramIdentityV1::TwoPaintDerivedSurfacePointV1
    }

    pub(crate) fn evaluate_static(
        &self,
        candidates: JointCandidateSetV1,
        observation: StaticJointObservationV1,
    ) -> Result<
        PointwiseFullHardReportV1<Evaluation, StaticJointObservationV1>,
        PointwiseJointReportErrorV1<Evaluation>,
    > {
        self.evaluate_owned(candidates, observation)
    }

    pub(crate) fn evaluate_revision_bound(
        &self,
        candidates: JointCandidateSetV1,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<
        PointwiseFullHardReportV1<Evaluation, RevisionBoundObservationV1>,
        PointwiseJointReportErrorV1<Evaluation>,
    > {
        self.evaluate_owned(candidates, observation)
    }

    fn evaluate_owned<Observation>(
        &self,
        candidates: JointCandidateSetV1,
        observation: Observation,
    ) -> Result<
        PointwiseFullHardReportV1<Evaluation, Observation>,
        PointwiseJointReportErrorV1<Evaluation>,
    >
    where
        Observation: JointObservationV1,
    {
        self.validate_candidates(&candidates)?;
        self.validate_observation(&observation)?;
        let (execution_count, cell_count) = checked_joint_cardinality_raw(
            candidates.candidates.len(),
            observation.case_count(),
            self.constraints.len(),
        )
        .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)?;
        let matrices = self.execute(
            candidates.candidates(),
            &observation,
            execution_count,
            cell_count,
        )?;
        Ok(PointwiseFullHardReportV1 {
            program_identity: self.identity(),
            program: self.clone(),
            candidates,
            observation,
            executions: matrices.executions,
            cells: matrices.cells,
        })
    }

    fn validate_observation<Observation>(
        &self,
        observation: &Observation,
    ) -> Result<(), PointwiseJointReportErrorV1<Evaluation>>
    where
        Observation: JointObservationV1,
    {
        if (0..observation.case_count()).any(|case_index| {
            observation
                .surface_at(case_index, self.root_surface)
                .is_none()
        }) {
            return Err(PointwiseJointReportErrorV1::MissingRootSurface(
                self.root_surface,
            ));
        }
        Ok(())
    }

    fn validate_candidates(
        &self,
        candidates: &JointCandidateSetV1,
    ) -> Result<(), PointwiseJointReportErrorV1<Evaluation>> {
        for candidate in candidates.candidates() {
            if candidate.lower.id() != self.lower_paint {
                return Err(PointwiseJointReportErrorV1::CandidatePaintMismatch {
                    ordinal: candidate.ordinal,
                    stage: JointVisibleTargetV1::Lower,
                    expected: self.lower_paint,
                    actual: candidate.lower.id(),
                });
            }
            if candidate.upper.id() != self.upper_paint {
                return Err(PointwiseJointReportErrorV1::CandidatePaintMismatch {
                    ordinal: candidate.ordinal,
                    stage: JointVisibleTargetV1::Upper,
                    expected: self.upper_paint,
                    actual: candidate.upper.id(),
                });
            }
        }
        Ok(())
    }

    fn execute<Observation>(
        &self,
        candidates: &[JointCandidateTupleV1],
        observation: &Observation,
        execution_count: usize,
        cell_count: usize,
    ) -> Result<
        PointwiseJointEvaluationMatricesV1<Evaluation>,
        PointwiseJointReportErrorV1<Evaluation>,
    >
    where
        Observation: JointObservationV1,
    {
        let mut executions = Vec::new();
        executions
            .try_reserve_exact(execution_count)
            .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cell_count)
            .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)?;

        for candidate in candidates {
            for case_index in 0..observation.case_count() {
                let root = observation
                    .surface_at(case_index, self.root_surface)
                    .unwrap_or_else(|| unreachable!("joint observation passed keyed preflight"));
                let lower = PointOpacityOverSurfaceV1::evaluate_admitted(
                    candidate.lower.source().bytes(),
                    candidate.lower.opacity(),
                    root.bytes(),
                );
                let upper = PointOpacityOverSurfaceV1::evaluate_admitted(
                    candidate.upper.source().bytes(),
                    candidate.upper.opacity(),
                    lower.visible(),
                );
                debug_assert_eq!(upper.certificate().backdrop_rgb(), lower.visible());

                executions.push(JointExecutionRecordV1 {
                    ordinal: candidate.ordinal,
                    case_index,
                    lower_paint: candidate.lower,
                    upper_paint: candidate.upper,
                    lower,
                    upper,
                });

                for constraint in self.constraints.iter().cloned() {
                    let occurrence = match constraint.target {
                        JointVisibleTargetV1::Lower => &lower,
                        JointVisibleTargetV1::Upper => &upper,
                    };
                    // Evaluator `Err` означает отсутствие валидного hard verdict,
                    // поэтому частичная матрица не называется FullHardReport.
                    let decision = match self
                        .evaluator
                        .assess(occurrence, constraint.invocation.clone())
                        .map_err(PointwiseJointReportErrorV1::Evaluator)?
                    {
                        HardDecision::Pass(evidence) => {
                            PointwiseJointConstraintDecisionV1::Pass(evidence)
                        }
                        HardDecision::Violation(evidence) => {
                            PointwiseJointConstraintDecisionV1::Violation(evidence)
                        }
                    };
                    cells.push(PointwiseJointConstraintCellV1 {
                        ordinal: candidate.ordinal,
                        constraint: constraint.id,
                        target: constraint.target,
                        case_index,
                        decision,
                    });
                }
            }
        }

        debug_assert_eq!(executions.len(), execution_count);
        debug_assert_eq!(cells.len(), cell_count);
        Ok(PointwiseJointEvaluationMatricesV1 {
            executions: executions.into_boxed_slice(),
            cells: cells.into_boxed_slice(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointwiseJointReportErrorV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    MissingRootSurface(SurfaceInputPortId),
    CandidatePaintMismatch {
        ordinal: CandidateOrdinalV1,
        stage: JointVisibleTargetV1,
        expected: PaintId,
        actual: PaintId,
    },
    Evaluator(Evaluation::Error),
    ResourceExhausted,
}

pub(crate) type JointReportErrorV1 = PointwiseJointReportErrorV1<ExactSrgb8IdentityV1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JointCapacityErrorV1 {
    ResourceExhausted,
}

fn checked_joint_cardinality_raw(
    candidates: usize,
    cases: usize,
    constraints: usize,
) -> Result<(usize, usize), JointCapacityErrorV1> {
    let executions = candidates
        .checked_mul(cases)
        .ok_or(JointCapacityErrorV1::ResourceExhausted)?;
    let cells = executions
        .checked_mul(constraints)
        .ok_or(JointCapacityErrorV1::ResourceExhausted)?;
    Ok((executions, cells))
}

pub(crate) fn checked_joint_cardinality(
    candidates: usize,
    cases: usize,
    constraints: usize,
) -> Result<(usize, usize), JointReportErrorV1> {
    checked_joint_cardinality_raw(candidates, cases, constraints)
        .map_err(|_| JointReportErrorV1::ResourceExhausted)
}

#[derive(Debug, Clone, PartialEq)]
struct PointwiseJointEvaluationMatricesV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[PointwiseJointConstraintCellV1<Evaluation>]>,
}

/// Один execution record существует независимо от наличия constraint на lower.
/// Поэтому связь derived surface доказана даже при пустом constraint set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JointExecutionRecordV1 {
    ordinal: CandidateOrdinalV1,
    case_index: usize,
    lower_paint: EncodedPointPaintV1,
    upper_paint: EncodedPointPaintV1,
    lower: ResolvedOccurrence,
    upper: ResolvedOccurrence,
}

impl JointExecutionRecordV1 {
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.ordinal
    }

    pub(crate) const fn case_index(&self) -> usize {
        self.case_index
    }

    pub(crate) const fn lower_paint(&self) -> EncodedPointPaintV1 {
        self.lower_paint
    }

    pub(crate) const fn upper_paint(&self) -> EncodedPointPaintV1 {
        self.upper_paint
    }

    pub(crate) const fn lower_occurrence(&self) -> &ResolvedOccurrence {
        &self.lower
    }

    pub(crate) const fn upper_occurrence(&self) -> &ResolvedOccurrence {
        &self.upper
    }

    pub(crate) fn lower_visible(&self) -> Srgb8 {
        Srgb8::new(self.lower.visible())
    }

    pub(crate) fn upper_visible(&self) -> Srgb8 {
        Srgb8::new(self.upper.visible())
    }

    pub(crate) fn derived_surface_is_exact(&self) -> bool {
        self.upper.certificate().backdrop_rgb() == self.lower.visible()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointwiseJointConstraintDecisionV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    Pass(Evaluation::PassEvidence),
    Violation(Evaluation::ViolationEvidence),
}

pub(crate) type JointConstraintDecisionV1 =
    PointwiseJointConstraintDecisionV1<ExactSrgb8IdentityV1>;

impl<Evaluation> PointwiseJointConstraintDecisionV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    pub(crate) const fn is_pass(&self) -> bool {
        matches!(self, Self::Pass(_))
    }
}

impl PointwiseJointConstraintDecisionV1<ExactSrgb8IdentityV1> {
    pub(crate) fn actual(&self) -> Srgb8 {
        match self {
            Self::Pass(evidence) => evidence.actual(),
            Self::Violation(evidence) => evidence.actual(),
        }
    }

    pub(crate) fn target(&self) -> Srgb8 {
        match self {
            Self::Pass(evidence) => evidence.target(),
            Self::Violation(evidence) => evidence.target(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseJointConstraintCellV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    ordinal: CandidateOrdinalV1,
    constraint: JointConstraintIdV1,
    target: JointVisibleTargetV1,
    case_index: usize,
    decision: PointwiseJointConstraintDecisionV1<Evaluation>,
}

impl<Evaluation> PointwiseJointConstraintCellV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.ordinal
    }

    pub(crate) const fn constraint(&self) -> JointConstraintIdV1 {
        self.constraint
    }

    pub(crate) const fn target_kind(&self) -> JointVisibleTargetV1 {
        self.target
    }

    pub(crate) const fn case_index(&self) -> usize {
        self.case_index
    }

    pub(crate) const fn decision(&self) -> &PointwiseJointConstraintDecisionV1<Evaluation> {
        &self.decision
    }
}

/// Полная матрица candidate x constraint x unique physical case плюс отдельная
/// joint execution matrix candidate x case. Report не знает selection policy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseFullHardReportV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    program_identity: JointPointProgramIdentityV1,
    program: PointwiseJointPointProgramV1<Evaluation>,
    candidates: JointCandidateSetV1,
    observation: Observation,
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[PointwiseJointConstraintCellV1<Evaluation>]>,
}

impl<Evaluation, Observation> PointwiseFullHardReportV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) const fn program_identity(&self) -> JointPointProgramIdentityV1 {
        self.program_identity
    }

    pub(crate) fn candidate_set(&self) -> &JointCandidateSetV1 {
        &self.candidates
    }

    pub(crate) fn executions(&self) -> &[JointExecutionRecordV1] {
        &self.executions
    }

    pub(crate) fn cells(&self) -> &[PointwiseJointConstraintCellV1<Evaluation>] {
        &self.cells
    }

    pub(crate) const fn observation(&self) -> &Observation {
        &self.observation
    }

    pub(crate) fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        self.observation.provenance(case_index)
    }

    pub(crate) fn classify(self) -> PointwiseHardFeasibilityV1<Evaluation, Observation> {
        let mut feasible = Vec::new();
        for candidate in self.candidates.candidates() {
            if self
                .cells
                .iter()
                .filter(|cell| cell.ordinal == candidate.ordinal)
                .all(|cell| cell.decision.is_pass())
            {
                feasible.push(candidate.ordinal);
            }
        }
        if feasible.is_empty() {
            PointwiseHardFeasibilityV1::Infeasible(self)
        } else {
            PointwiseHardFeasibilityV1::NonEmpty(PointwiseNonEmptyFeasibleJointTuplesV1 {
                report: self,
                feasible: feasible.into_boxed_slice(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointwiseHardFeasibilityV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    Infeasible(PointwiseFullHardReportV1<Evaluation, Observation>),
    NonEmpty(PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>),
}

pub(crate) type HardFeasibilityV1 =
    PointwiseHardFeasibilityV1<ExactSrgb8IdentityV1, RevisionBoundObservationV1>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    report: PointwiseFullHardReportV1<Evaluation, Observation>,
    feasible: Box<[CandidateOrdinalV1]>,
}

impl<Evaluation, Observation> PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) fn feasible(&self) -> &[CandidateOrdinalV1] {
        &self.feasible
    }

    pub(crate) fn candidate_set(&self) -> &JointCandidateSetV1 {
        self.report.candidate_set()
    }

    pub(crate) fn select(
        self,
        policy: DeclaredTotalOrderV1,
    ) -> PointwiseSelectedJointTupleV1<Evaluation, Observation> {
        let ordinal = policy
            .order
            .iter()
            .copied()
            .find(|ordinal| self.feasible.binary_search(ordinal).is_ok())
            .unwrap_or_else(|| unreachable!("validated total order covers nonempty feasible set"));
        let candidate = *self
            .report
            .candidates
            .candidates()
            .iter()
            .find(|candidate| candidate.ordinal == ordinal)
            .unwrap_or_else(|| unreachable!("validated ordinal belongs to candidate set"));
        PointwiseSelectedJointTupleV1 {
            report: self.report,
            policy,
            candidate,
        }
    }
}

/// Полный client-declared tie-break. Он не участвует в measurement/report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredTotalOrderV1 {
    order: Box<[CandidateOrdinalV1]>,
}

impl DeclaredTotalOrderV1 {
    pub(crate) fn new(
        candidates: &JointCandidateSetV1,
        order: Vec<CandidateOrdinalV1>,
    ) -> Result<Self, SelectionPolicyErrorV1> {
        if order.len() != candidates.candidates.len() {
            return Err(SelectionPolicyErrorV1::NotATotalOrder);
        }
        let mut canonical = order.clone();
        canonical.sort_unstable();
        for pair in canonical.windows(2) {
            if pair[0] == pair[1] {
                return Err(SelectionPolicyErrorV1::DuplicateOrdinal(pair[0]));
            }
        }
        if canonical.iter().copied().ne(candidates
            .candidates
            .iter()
            .map(|candidate| candidate.ordinal))
        {
            return Err(SelectionPolicyErrorV1::NotATotalOrder);
        }
        Ok(Self {
            order: order.into_boxed_slice(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionPolicyErrorV1 {
    DuplicateOrdinal(CandidateOrdinalV1),
    NotATotalOrder,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseSelectedJointTupleV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    report: PointwiseFullHardReportV1<Evaluation, Observation>,
    policy: DeclaredTotalOrderV1,
    candidate: JointCandidateTupleV1,
}

impl<Evaluation, Observation> PointwiseSelectedJointTupleV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.candidate.ordinal
    }

    pub(crate) fn recheck(
        self,
    ) -> Result<
        PointwiseVerifiedSelectionV1<Evaluation, Observation>,
        PointwiseSelectedRecheckErrorV1<Evaluation>,
    > {
        // A revision-bound report can enter this consuming chain only through
        // `evaluate_revision_bound` with a Session-minted linear permit. Static
        // reports have a different concrete observation type.
        self.report
            .program
            .validate_observation(&self.report.observation)
            .map_err(|_| PointwiseSelectedRecheckErrorV1::InvariantDrift)?;
        let cases = self.report.observation.case_count();
        let (execution_count, cell_count) =
            checked_joint_cardinality_raw(1, cases, self.report.program.constraints.len())
                .map_err(|_| PointwiseSelectedRecheckErrorV1::ResourceExhausted)?;
        let matrices = self
            .report
            .program
            .execute(
                core::slice::from_ref(&self.candidate),
                &self.report.observation,
                execution_count,
                cell_count,
            )
            .map_err(|error| match error {
                PointwiseJointReportErrorV1::ResourceExhausted => {
                    PointwiseSelectedRecheckErrorV1::ResourceExhausted
                }
                PointwiseJointReportErrorV1::Evaluator(error) => {
                    PointwiseSelectedRecheckErrorV1::Evaluator(error)
                }
                PointwiseJointReportErrorV1::MissingRootSurface(_)
                | PointwiseJointReportErrorV1::CandidatePaintMismatch { .. } => {
                    PointwiseSelectedRecheckErrorV1::InvariantDrift
                }
            })?;
        if let Some(violation) = matrices
            .cells
            .iter()
            .find(|cell| !cell.decision.is_pass())
            .cloned()
        {
            return Err(PointwiseSelectedRecheckErrorV1::Violation(Box::new(
                violation,
            )));
        }
        Ok(PointwiseVerifiedSelectionV1 {
            selected: self,
            recheck: PointwiseFreshJointRecheckV1 {
                executions: matrices.executions,
                cells: matrices.cells,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointwiseSelectedRecheckErrorV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    ResourceExhausted,
    InvariantDrift,
    Evaluator(Evaluation::Error),
    Violation(Box<PointwiseJointConstraintCellV1<Evaluation>>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseFreshJointRecheckV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[PointwiseJointConstraintCellV1<Evaluation>]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointwiseVerifiedSelectionV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    selected: PointwiseSelectedJointTupleV1<Evaluation, Observation>,
    recheck: PointwiseFreshJointRecheckV1<Evaluation>,
}

impl<Evaluation, Observation> PointwiseVerifiedSelectionV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.selected.candidate.ordinal
    }

    pub(crate) const fn report(&self) -> &PointwiseFullHardReportV1<Evaluation, Observation> {
        &self.selected.report
    }

    pub(crate) fn policy(&self) -> &[CandidateOrdinalV1] {
        &self.selected.policy.order
    }

    pub(crate) fn fresh_executions(&self) -> &[JointExecutionRecordV1] {
        &self.recheck.executions
    }

    pub(crate) fn fresh_cells(&self) -> &[PointwiseJointConstraintCellV1<Evaluation>] {
        &self.recheck.cells
    }
}
