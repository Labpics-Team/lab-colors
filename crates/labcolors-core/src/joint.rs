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
    /// Joint execution repeats one invocation across the complete physical
    /// matrix. Requiring a value type here keeps that repetition allocation-free
    /// instead of hiding an arbitrary `Clone` behind the engine's preflight.
    type Invocation: Copy + Debug + PartialEq;
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
    PointInvocation<Evaluation>: Copy + Debug + PartialEq,
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
    candidates: Vec<JointCandidateTupleV1>,
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
        // Group equal physical tuples without auxiliary storage. The explicit
        // ordinal tie-break makes every group canonical even though the sort is
        // unstable. We inspect every duplicate group and retain the same error
        // precedence as the former ordinal-order scan: the smallest first
        // ordinal, followed by the smallest matching second ordinal.
        candidates.sort_unstable_by(|left, right| {
            candidate_physical_key(left)
                .cmp(&candidate_physical_key(right))
                .then_with(|| left.ordinal.cmp(&right.ordinal))
        });
        let duplicate = candidates
            .windows(2)
            .filter(|pair| pair[0].lower == pair[1].lower && pair[0].upper == pair[1].upper)
            .map(|pair| (pair[0].ordinal, pair[1].ordinal))
            .min();
        if let Some((first, second)) = duplicate {
            return Err(CandidateSetErrorV1::DuplicatePhysicalTuple { first, second });
        }
        candidates.sort_unstable_by_key(|candidate| candidate.ordinal);
        Ok(Self { candidates })
    }

    pub(crate) fn candidates(&self) -> &[JointCandidateTupleV1] {
        &self.candidates
    }
}

fn candidate_physical_key(
    candidate: &JointCandidateTupleV1,
) -> (PaintId, Srgb8, u64, PaintId, Srgb8, u64) {
    (
        candidate.lower.id(),
        candidate.lower.source(),
        candidate.lower.opacity().bits(),
        candidate.upper.id(),
        candidate.upper.source(),
        candidate.upper.opacity().bits(),
    )
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
    fn bind_surface(&self, surface: SurfaceInputPortId) -> Option<usize>;
    fn value_at_bound(&self, case_index: usize, bound_index: usize) -> Option<Srgb8>;
    fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]>;
}

impl observation_seal::Sealed for RevisionBoundObservationV1 {}

impl JointObservationV1 for RevisionBoundObservationV1 {
    fn case_count(&self) -> usize {
        self.physical_case_count()
    }

    fn bind_surface(&self, surface: SurfaceInputPortId) -> Option<usize> {
        self.schema().binary_search(&surface).ok()
    }

    fn value_at_bound(&self, case_index: usize, bound_index: usize) -> Option<Srgb8> {
        self.physical_values(case_index)?
            .get(bound_index)
            .copied()
            .map(crate::lcs_occurrence::ColorSignal::srgb8)
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

    fn bind_surface(&self, surface: SurfaceInputPortId) -> Option<usize> {
        (surface == self.root_surface).then_some(0)
    }

    fn value_at_bound(&self, case_index: usize, bound_index: usize) -> Option<Srgb8> {
        (case_index == 0 && bound_index == 0).then_some(self.root)
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
    constraints: Vec<PointwiseJointHardConstraintV1<Evaluation>>,
}

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
        constraints: Vec<PointwiseJointHardConstraintV1<ExactSrgb8IdentityV1>>,
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
            constraints,
        })
    }

    const fn identity(&self) -> JointPointProgramIdentityV1 {
        JointPointProgramIdentityV1::TwoPaintDerivedSurfacePointV1
    }

    pub(crate) fn evaluate_static(
        self,
        candidates: JointCandidateSetV1,
        observation: StaticJointObservationV1,
    ) -> Result<
        PointwiseFullHardReportV1<Evaluation, StaticJointObservationV1>,
        PointwiseJointReportErrorV1<Evaluation>,
    > {
        self.evaluate_owned(candidates, observation)
    }

    pub(crate) fn evaluate_revision_bound(
        self,
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
        self,
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
        let root_binding = self.bind_observation_surface(&observation)?;
        let (execution_count, cell_count) = checked_joint_cardinality_raw(
            candidates.candidates.len(),
            observation.case_count(),
            self.constraints.len(),
        )
        .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)?;
        let mut feasible_ordinals = Vec::new();
        feasible_ordinals
            .try_reserve_exact(candidates.candidates.len())
            .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)?;
        feasible_ordinals.extend(
            candidates
                .candidates
                .iter()
                .map(|candidate| candidate.ordinal),
        );
        let matrices = self.execute(
            candidates.candidates(),
            &observation,
            root_binding,
            execution_count,
            cell_count,
        )?;
        retain_feasible_ordinals(&mut feasible_ordinals, &matrices.cells);
        Ok(PointwiseFullHardReportV1 {
            program_identity: self.identity(),
            program: self,
            candidates,
            observation,
            executions: matrices.executions,
            cells: matrices.cells,
            feasible_ordinals,
        })
    }

    fn bind_observation_surface<Observation>(
        &self,
        observation: &Observation,
    ) -> Result<usize, PointwiseJointReportErrorV1<Evaluation>>
    where
        Observation: JointObservationV1,
    {
        let Some(root_binding) = observation.bind_surface(self.root_surface) else {
            return Err(PointwiseJointReportErrorV1::MissingRootSurface(
                self.root_surface,
            ));
        };
        if (0..observation.case_count()).any(|case_index| {
            observation
                .value_at_bound(case_index, root_binding)
                .is_none()
        }) {
            return Err(PointwiseJointReportErrorV1::MissingRootSurface(
                self.root_surface,
            ));
        }
        Ok(root_binding)
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
        root_binding: usize,
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
                let root = observation.value_at_bound(case_index, root_binding).ok_or(
                    PointwiseJointReportErrorV1::MissingRootSurface(self.root_surface),
                )?;
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

                for constraint in &self.constraints {
                    let occurrence = match constraint.target {
                        JointVisibleTargetV1::Lower => &lower,
                        JointVisibleTargetV1::Upper => &upper,
                    };
                    // Evaluator `Err` означает отсутствие валидного hard verdict,
                    // поэтому частичная матрица не называется FullHardReport.
                    let decision = match self
                        .evaluator
                        .assess(occurrence, constraint.invocation)
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
        Ok(PointwiseJointEvaluationMatricesV1 { executions, cells })
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
) -> Result<(usize, usize), PointwiseJointReportErrorV1<ExactSrgb8IdentityV1>> {
    checked_joint_cardinality_raw(candidates, cases, constraints)
        .map_err(|_| PointwiseJointReportErrorV1::ResourceExhausted)
}

#[derive(Debug, PartialEq)]
struct PointwiseJointEvaluationMatricesV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    executions: Vec<JointExecutionRecordV1>,
    cells: Vec<PointwiseJointConstraintCellV1<Evaluation>>,
}

fn retain_feasible_ordinals<Evaluation>(
    feasible: &mut Vec<CandidateOrdinalV1>,
    cells: &[PointwiseJointConstraintCellV1<Evaluation>],
) where
    Evaluation: JointPointEvaluatorV1,
{
    // Both vectors are candidate-major and ordinal-canonical. One cursor keeps
    // classification O(candidates + cells), including the empty-constraint case.
    let mut cell_index = 0;
    feasible.retain(|ordinal| {
        let mut passes = true;
        while let Some(cell) = cells
            .get(cell_index)
            .filter(|cell| cell.ordinal == *ordinal)
        {
            passes &= cell.decision.is_pass();
            cell_index += 1;
        }
        passes
    });
    debug_assert_eq!(cell_index, cells.len());
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
#[derive(Debug, PartialEq)]
pub(crate) struct PointwiseFullHardReportV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    program_identity: JointPointProgramIdentityV1,
    program: PointwiseJointPointProgramV1<Evaluation>,
    candidates: JointCandidateSetV1,
    observation: Observation,
    executions: Vec<JointExecutionRecordV1>,
    cells: Vec<PointwiseJointConstraintCellV1<Evaluation>>,
    feasible_ordinals: Vec<CandidateOrdinalV1>,
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
        if self.feasible_ordinals.is_empty() {
            PointwiseHardFeasibilityV1::Infeasible(self)
        } else {
            PointwiseHardFeasibilityV1::NonEmpty(PointwiseNonEmptyFeasibleJointTuplesV1 {
                report: self,
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum PointwiseHardFeasibilityV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    Infeasible(PointwiseFullHardReportV1<Evaluation, Observation>),
    NonEmpty(PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>),
}

#[derive(Debug, PartialEq)]
pub(crate) struct PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    report: PointwiseFullHardReportV1<Evaluation, Observation>,
}

impl<Evaluation, Observation> PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) fn feasible(&self) -> &[CandidateOrdinalV1] {
        &self.report.feasible_ordinals
    }

    pub(crate) fn candidate_set(&self) -> &JointCandidateSetV1 {
        self.report.candidate_set()
    }

    #[expect(
        clippy::result_large_err,
        reason = "ownership-preserving rejection keeps the expensive report retryable without heap allocation or recomputation"
    )]
    pub(crate) fn select(
        self,
        policy: DeclaredTotalOrderV1,
    ) -> Result<
        PointwiseSelectedJointTupleV1<Evaluation, Observation>,
        PointwiseSelectionFailureV1<Evaluation, Observation>,
    > {
        if !policy.is_bound_to(self.report.candidate_set()) {
            return Err(PointwiseSelectionFailureV1 {
                feasible: self,
                policy,
                reason: SelectionPolicyErrorV1::CandidateDomainMismatch,
            });
        }
        // Canonical policy entries and feasible ordinals are both sorted by
        // ordinal. One merge scan finds the feasible entry with minimum
        // client-declared rank without C×log(F) lookup or auxiliary storage.
        let mut feasible_index = 0;
        let mut selected: Option<(usize, usize)> = None;
        for (candidate_index, entry) in policy.domain.iter().enumerate() {
            if self.report.feasible_ordinals.get(feasible_index) == Some(&entry.ordinal) {
                if selected.is_none_or(|(rank, _)| entry.rank < rank) {
                    selected = Some((entry.rank, candidate_index));
                }
                feasible_index += 1;
            }
        }
        let Some((_, candidate_index)) =
            selected.filter(|_| feasible_index == self.report.feasible_ordinals.len())
        else {
            return Err(PointwiseSelectionFailureV1 {
                feasible: self,
                policy,
                reason: SelectionPolicyErrorV1::InternalInvariant,
            });
        };
        let Some(candidate) = self
            .report
            .candidates
            .candidates()
            .get(candidate_index)
            .copied()
        else {
            return Err(PointwiseSelectionFailureV1 {
                feasible: self,
                policy,
                reason: SelectionPolicyErrorV1::InternalInvariant,
            });
        };
        Ok(PointwiseSelectedJointTupleV1 {
            report: self.report,
            policy,
            candidate,
        })
    }
}

/// Recoverable selection rejection. A foreign/malformed policy cannot destroy
/// the expensive full report: the caller can replace only the policy and retry
/// without recomposition or evaluator execution.
#[derive(Debug, PartialEq)]
pub(crate) struct PointwiseSelectionFailureV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    feasible: PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>,
    policy: DeclaredTotalOrderV1,
    reason: SelectionPolicyErrorV1,
}

impl<Evaluation, Observation> PointwiseSelectionFailureV1<Evaluation, Observation>
where
    Evaluation: JointPointEvaluatorV1,
    Observation: JointObservationV1,
{
    pub(crate) const fn feasible(
        &self,
    ) -> &PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation> {
        &self.feasible
    }

    pub(crate) const fn policy(&self) -> &DeclaredTotalOrderV1 {
        &self.policy
    }

    pub(crate) const fn reason(&self) -> SelectionPolicyErrorV1 {
        self.reason
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        PointwiseNonEmptyFeasibleJointTuplesV1<Evaluation, Observation>,
        DeclaredTotalOrderV1,
        SelectionPolicyErrorV1,
    ) {
        (self.feasible, self.policy, self.reason)
    }
}

/// Полный client-declared tie-break. Он не участвует в measurement/report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredTotalOrderV1 {
    order: Vec<CandidateOrdinalV1>,
    domain: Vec<DeclaredPolicyDomainEntryV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeclaredPolicyDomainEntryV1 {
    ordinal: CandidateOrdinalV1,
    rank: usize,
}

impl DeclaredTotalOrderV1 {
    pub(crate) fn new(
        candidates: &JointCandidateSetV1,
        order: Vec<CandidateOrdinalV1>,
    ) -> Result<Self, SelectionPolicyErrorV1> {
        if order.len() != candidates.candidates.len() {
            return Err(SelectionPolicyErrorV1::NotATotalOrder);
        }
        let mut domain = Vec::new();
        domain
            .try_reserve_exact(candidates.candidates.len())
            .map_err(|_| SelectionPolicyErrorV1::ResourceExhausted)?;
        domain.extend(
            order
                .iter()
                .copied()
                .enumerate()
                .map(|(rank, ordinal)| DeclaredPolicyDomainEntryV1 { ordinal, rank }),
        );
        domain.sort_unstable_by_key(|entry| entry.ordinal);
        if let Some(pair) = domain
            .windows(2)
            .find(|pair| pair[0].ordinal == pair[1].ordinal)
        {
            return Err(SelectionPolicyErrorV1::DuplicateOrdinal(pair[0].ordinal));
        }
        if domain.iter().map(|entry| entry.ordinal).ne(candidates
            .candidates
            .iter()
            .map(|candidate| candidate.ordinal))
        {
            return Err(SelectionPolicyErrorV1::NotATotalOrder);
        }
        Ok(Self { order, domain })
    }

    pub(crate) fn order(&self) -> &[CandidateOrdinalV1] {
        &self.order
    }

    pub(crate) fn into_order(self) -> Vec<CandidateOrdinalV1> {
        self.order
    }

    fn is_bound_to(&self, candidates: &JointCandidateSetV1) -> bool {
        self.domain.iter().map(|entry| entry.ordinal).eq(candidates
            .candidates
            .iter()
            .map(|candidate| candidate.ordinal))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionPolicyErrorV1 {
    ResourceExhausted,
    DuplicateOrdinal(CandidateOrdinalV1),
    NotATotalOrder,
    CandidateDomainMismatch,
    InternalInvariant,
}

#[derive(Debug, PartialEq)]
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
        let root_binding = self
            .report
            .program
            .bind_observation_surface(&self.report.observation)
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
                root_binding,
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
        if let Some(violation_index) = matrices
            .cells
            .iter()
            .position(|cell| !cell.decision.is_pass())
        {
            return Err(PointwiseSelectedRecheckErrorV1::Violation {
                evidence: PointwiseFreshJointRecheckV1 {
                    executions: matrices.executions,
                    cells: matrices.cells,
                },
                violation_index,
            });
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

#[derive(Debug, PartialEq)]
pub(crate) enum PointwiseSelectedRecheckErrorV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    ResourceExhausted,
    InvariantDrift,
    Evaluator(Evaluation::Error),
    Violation {
        evidence: PointwiseFreshJointRecheckV1<Evaluation>,
        violation_index: usize,
    },
}

#[derive(Debug, PartialEq)]
pub(crate) struct PointwiseFreshJointRecheckV1<Evaluation>
where
    Evaluation: JointPointEvaluatorV1,
{
    executions: Vec<JointExecutionRecordV1>,
    cells: Vec<PointwiseJointConstraintCellV1<Evaluation>>,
}

#[derive(Debug, PartialEq)]
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
