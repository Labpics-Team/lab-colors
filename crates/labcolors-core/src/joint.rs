//! Приватный V2a-срез совместного point-selection.
//!
//! Один code-owned program связывает две Paint-переменные через реальный
//! `lower occurrence -> visible surface -> upper occurrence`. Candidate domain,
//! полный hard-report, declared policy и fresh recheck являются разными типами.
//! Модуль не знает клиентских recipes, role taxonomy или legacy solver state и
//! не минтит terminal output certificate.

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, PaintId, PointOpacityOverSurfaceV1, ResolvedOccurrence, SurfaceInputPortId,
};
use crate::constraints::{
    ApplicableWcag22EvaluationErrorV1, ExactPassEvidenceV1, ExactSrgb8IdentityV1,
    ExactViolationEvidenceV1, HardDecision, Wcag22PassEvidenceV1, Wcag22Srgb8V1,
    Wcag22ViolationEvidenceV1, assess_visible_point_hard,
};
use crate::observation::{RevisionBoundObservationV1, ScenarioId};
use crate::wcag22::Wcag22CriterionV1;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PointPaintKeyV1 {
    id: PaintId,
    source: Srgb8,
    opacity_bits: u64,
}

impl From<EncodedPointPaintV1> for PointPaintKeyV1 {
    fn from(paint: EncodedPointPaintV1) -> Self {
        Self {
            id: paint.id(),
            source: paint.source(),
            opacity_bits: paint.opacity().bits(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct JointPhysicalTupleKeyV1 {
    lower: PointPaintKeyV1,
    upper: PointPaintKeyV1,
}

impl From<JointCandidateTupleV1> for JointPhysicalTupleKeyV1 {
    fn from(candidate: JointCandidateTupleV1) -> Self {
        Self {
            lower: candidate.lower.into(),
            upper: candidate.upper.into(),
        }
    }
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
        candidates.sort_unstable_by_key(|candidate| {
            (JointPhysicalTupleKeyV1::from(*candidate), candidate.ordinal)
        });
        for pair in candidates.windows(2) {
            if JointPhysicalTupleKeyV1::from(pair[0]) == JointPhysicalTupleKeyV1::from(pair[1]) {
                return Err(CandidateSetErrorV1::DuplicatePhysicalTuple {
                    first: pair[0].ordinal,
                    second: pair[1].ordinal,
                });
            }
        }
        candidates.sort_unstable_by_key(|candidate| candidate.ordinal);
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

/// Physical occurrence, к которому относится exact hard predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointVisibleTargetV1 {
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointHardConstraintV1 {
    Exact {
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        invocation: Srgb8,
    },
    Wcag22 {
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        criterion: Wcag22CriterionV1,
    },
}

impl JointHardConstraintV1 {
    pub(crate) const fn exact(
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        invocation: Srgb8,
    ) -> Self {
        Self::Exact {
            id,
            target,
            invocation,
        }
    }

    pub(crate) const fn wcag22(
        id: JointConstraintIdV1,
        target: JointVisibleTargetV1,
        criterion: Wcag22CriterionV1,
    ) -> Self {
        Self::Wcag22 {
            id,
            target,
            criterion,
        }
    }

    const fn id(self) -> JointConstraintIdV1 {
        match self {
            Self::Exact { id, .. } | Self::Wcag22 { id, .. } => id,
        }
    }

    const fn target(self) -> JointVisibleTargetV1 {
        match self {
            Self::Exact { target, .. } | Self::Wcag22 { target, .. } => target,
        }
    }
}

/// Identity первой private joint topology. Она не является public Program ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointPointProgramIdentityV1 {
    TwoPaintDerivedSurfacePointV1,
}

/// Две связанные occurrences над одним observed root backdrop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JointPointProgramV1 {
    root_surface: SurfaceInputPortId,
    lower_paint: PaintId,
    upper_paint: PaintId,
    constraints: Box<[JointHardConstraintV1]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointProgramErrorV1 {
    SamePaintIdentity(PaintId),
    EmptyHardConstraintSet,
    DuplicateConstraint(JointConstraintIdV1),
}

impl JointPointProgramV1 {
    pub(crate) fn new(
        root_surface: SurfaceInputPortId,
        lower_paint: PaintId,
        upper_paint: PaintId,
        mut constraints: Vec<JointHardConstraintV1>,
    ) -> Result<Self, JointProgramErrorV1> {
        if lower_paint == upper_paint {
            return Err(JointProgramErrorV1::SamePaintIdentity(lower_paint));
        }
        if constraints.is_empty() {
            return Err(JointProgramErrorV1::EmptyHardConstraintSet);
        }
        constraints.sort_unstable_by_key(|constraint| constraint.id());
        for pair in constraints.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(JointProgramErrorV1::DuplicateConstraint(pair[0].id()));
            }
        }
        Ok(Self {
            root_surface,
            lower_paint,
            upper_paint,
            constraints: constraints.into_boxed_slice(),
        })
    }

    const fn identity(&self) -> JointPointProgramIdentityV1 {
        JointPointProgramIdentityV1::TwoPaintDerivedSurfacePointV1
    }

    pub(crate) fn evaluate(
        &self,
        candidates: JointCandidateSetV1,
        observation: RevisionBoundObservationV1,
    ) -> Result<FullHardReportV1, JointReportErrorV1> {
        self.validate_candidates(&candidates)?;
        let root_index = observation
            .schema()
            .binary_search(&self.root_surface)
            .map_err(|_| JointReportErrorV1::MissingRootSurface(self.root_surface))?;
        let (execution_count, cell_count) = checked_joint_cardinality(
            candidates.candidates.len(),
            observation.set().cases().len(),
            self.constraints.len(),
        )?;
        let matrices = self.execute(
            candidates.candidates(),
            &observation,
            root_index,
            execution_count,
            cell_count,
        )?;
        Ok(FullHardReportV1 {
            program_identity: self.identity(),
            program: self.clone(),
            candidates,
            observation,
            executions: matrices.executions,
            cells: matrices.cells,
        })
    }

    fn validate_candidates(
        &self,
        candidates: &JointCandidateSetV1,
    ) -> Result<(), JointReportErrorV1> {
        for candidate in candidates.candidates() {
            if candidate.lower.id() != self.lower_paint {
                return Err(JointReportErrorV1::CandidatePaintMismatch {
                    ordinal: candidate.ordinal,
                    stage: JointVisibleTargetV1::Lower,
                    expected: self.lower_paint,
                    actual: candidate.lower.id(),
                });
            }
            if candidate.upper.id() != self.upper_paint {
                return Err(JointReportErrorV1::CandidatePaintMismatch {
                    ordinal: candidate.ordinal,
                    stage: JointVisibleTargetV1::Upper,
                    expected: self.upper_paint,
                    actual: candidate.upper.id(),
                });
            }
        }
        Ok(())
    }

    fn execute(
        &self,
        candidates: &[JointCandidateTupleV1],
        observation: &RevisionBoundObservationV1,
        root_index: usize,
        execution_count: usize,
        cell_count: usize,
    ) -> Result<JointEvaluationMatricesV1, JointReportErrorV1> {
        let mut executions = Vec::new();
        executions
            .try_reserve_exact(execution_count)
            .map_err(|_| JointReportErrorV1::ResourceExhausted)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cell_count)
            .map_err(|_| JointReportErrorV1::ResourceExhausted)?;

        for candidate in candidates {
            for (case_index, case) in observation.set().cases().iter().enumerate() {
                let root = case.bindings()[root_index];
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

                for constraint in self.constraints.iter().copied() {
                    let target = constraint.target();
                    let occurrence = match target {
                        JointVisibleTargetV1::Lower => &lower,
                        JointVisibleTargetV1::Upper => &upper,
                    };
                    let decision = match constraint {
                        JointHardConstraintV1::Exact { invocation, .. } => {
                            match assess_visible_point_hard(
                                occurrence,
                                &ExactSrgb8IdentityV1,
                                invocation,
                            ) {
                                Ok(HardDecision::Pass(evidence)) => {
                                    JointConstraintDecisionV1::Pass(evidence)
                                }
                                Ok(HardDecision::Violation(evidence)) => {
                                    JointConstraintDecisionV1::Violation(evidence)
                                }
                                Err(error) => match error {},
                            }
                        }
                        JointHardConstraintV1::Wcag22 { criterion, .. } => {
                            match assess_visible_point_hard(occurrence, &Wcag22Srgb8V1, criterion)
                                .map_err(JointReportErrorV1::Evaluator)?
                            {
                                HardDecision::Pass(evidence) => {
                                    JointConstraintDecisionV1::Wcag22Pass(evidence)
                                }
                                HardDecision::Violation(evidence) => {
                                    JointConstraintDecisionV1::Wcag22Violation(evidence)
                                }
                            }
                        }
                    };
                    cells.push(JointConstraintCellV1 {
                        ordinal: candidate.ordinal,
                        constraint: constraint.id(),
                        target,
                        case_index,
                        decision,
                    });
                }
            }
        }

        debug_assert_eq!(executions.len(), execution_count);
        debug_assert_eq!(cells.len(), cell_count);
        Ok(JointEvaluationMatricesV1 {
            executions: executions.into_boxed_slice(),
            cells: cells.into_boxed_slice(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JointReportErrorV1 {
    MissingRootSurface(SurfaceInputPortId),
    CandidatePaintMismatch {
        ordinal: CandidateOrdinalV1,
        stage: JointVisibleTargetV1,
        expected: PaintId,
        actual: PaintId,
    },
    Evaluator(ApplicableWcag22EvaluationErrorV1),
    ResourceExhausted,
}

pub(crate) fn checked_joint_cardinality(
    candidates: usize,
    cases: usize,
    constraints: usize,
) -> Result<(usize, usize), JointReportErrorV1> {
    let executions = candidates
        .checked_mul(cases)
        .ok_or(JointReportErrorV1::ResourceExhausted)?;
    let cells = executions
        .checked_mul(constraints)
        .ok_or(JointReportErrorV1::ResourceExhausted)?;
    Ok((executions, cells))
}

#[derive(Debug, Clone, PartialEq)]
struct JointEvaluationMatricesV1 {
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[JointConstraintCellV1]>,
}

/// Один execution record существует независимо от наличия constraint на lower.
/// Поэтому связь derived surface доказана даже при единственном upper predicate.
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

    pub(crate) fn lower_visible(&self) -> Srgb8 {
        Srgb8::new(self.lower.visible())
    }

    pub(crate) fn upper_visible(&self) -> Srgb8 {
        Srgb8::new(self.upper.visible())
    }

    pub(crate) fn derived_surface_is_exact(&self) -> bool {
        self.upper.certificate().backdrop_rgb() == self.lower.visible()
    }

    pub(crate) const fn lower_occurrence(&self) -> &ResolvedOccurrence {
        &self.lower
    }

    pub(crate) const fn upper_occurrence(&self) -> &ResolvedOccurrence {
        &self.upper
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum JointConstraintDecisionV1 {
    Pass(ExactPassEvidenceV1),
    Violation(ExactViolationEvidenceV1),
    Wcag22Pass(Wcag22PassEvidenceV1),
    Wcag22Violation(Wcag22ViolationEvidenceV1),
}

impl JointConstraintDecisionV1 {
    pub(crate) const fn is_pass(&self) -> bool {
        matches!(self, Self::Pass(_) | Self::Wcag22Pass(_))
    }

    pub(crate) fn actual(&self) -> Srgb8 {
        match self {
            Self::Pass(evidence) => evidence.actual(),
            Self::Violation(evidence) => evidence.actual(),
            Self::Wcag22Pass(evidence) => {
                Srgb8::new(evidence.measurement().value().measurement().foreground)
            }
            Self::Wcag22Violation(evidence) => {
                Srgb8::new(evidence.measurement().value().measurement().foreground)
            }
        }
    }

    pub(crate) fn target(&self) -> Srgb8 {
        match self {
            Self::Pass(evidence) => evidence.target(),
            Self::Violation(evidence) => evidence.target(),
            Self::Wcag22Pass(evidence) => {
                Srgb8::new(evidence.measurement().value().measurement().background)
            }
            Self::Wcag22Violation(evidence) => {
                Srgb8::new(evidence.measurement().value().measurement().background)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct JointConstraintCellV1 {
    ordinal: CandidateOrdinalV1,
    constraint: JointConstraintIdV1,
    target: JointVisibleTargetV1,
    case_index: usize,
    decision: JointConstraintDecisionV1,
}

impl JointConstraintCellV1 {
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

    pub(crate) const fn decision(&self) -> &JointConstraintDecisionV1 {
        &self.decision
    }
}

/// Полная матрица candidate x constraint x unique physical case плюс отдельная
/// joint execution matrix candidate x case. Report не знает selection policy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FullHardReportV1 {
    program_identity: JointPointProgramIdentityV1,
    program: JointPointProgramV1,
    candidates: JointCandidateSetV1,
    observation: RevisionBoundObservationV1,
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[JointConstraintCellV1]>,
}

impl FullHardReportV1 {
    pub(crate) const fn program_identity(&self) -> JointPointProgramIdentityV1 {
        self.program_identity
    }

    pub(crate) fn candidate_set(&self) -> &JointCandidateSetV1 {
        &self.candidates
    }

    pub(crate) fn executions(&self) -> &[JointExecutionRecordV1] {
        &self.executions
    }

    pub(crate) fn cells(&self) -> &[JointConstraintCellV1] {
        &self.cells
    }

    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub(crate) fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        self.observation
            .set()
            .cases()
            .get(case_index)
            .map(|case| case.provenance())
    }

    pub(crate) fn classify(self) -> HardFeasibilityV1 {
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
            HardFeasibilityV1::Infeasible(self)
        } else {
            HardFeasibilityV1::NonEmpty(NonEmptyFeasibleJointTuplesV1 {
                report: self,
                feasible: feasible.into_boxed_slice(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HardFeasibilityV1 {
    Infeasible(FullHardReportV1),
    NonEmpty(NonEmptyFeasibleJointTuplesV1),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NonEmptyFeasibleJointTuplesV1 {
    report: FullHardReportV1,
    feasible: Box<[CandidateOrdinalV1]>,
}

impl NonEmptyFeasibleJointTuplesV1 {
    pub(crate) fn feasible(&self) -> &[CandidateOrdinalV1] {
        &self.feasible
    }

    pub(crate) fn candidate_set(&self) -> &JointCandidateSetV1 {
        self.report.candidate_set()
    }

    pub(crate) fn select(self, policy: DeclaredTotalOrderV1) -> SelectedJointTupleV1 {
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
        SelectedJointTupleV1 {
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
pub(crate) struct SelectedJointTupleV1 {
    report: FullHardReportV1,
    policy: DeclaredTotalOrderV1,
    candidate: JointCandidateTupleV1,
}

impl SelectedJointTupleV1 {
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.candidate.ordinal
    }

    pub(crate) fn recheck(
        self,
    ) -> Result<RevisionBoundVerifiedSelectionV1, SelectedRecheckErrorV1> {
        let root_index = self
            .report
            .observation
            .schema()
            .binary_search(&self.report.program.root_surface)
            .map_err(|_| SelectedRecheckErrorV1::InvariantDrift)?;
        let cases = self.report.observation.set().cases().len();
        let (execution_count, cell_count) =
            checked_joint_cardinality(1, cases, self.report.program.constraints.len())
                .map_err(|_| SelectedRecheckErrorV1::ResourceExhausted)?;
        let matrices = self
            .report
            .program
            .execute(
                core::slice::from_ref(&self.candidate),
                &self.report.observation,
                root_index,
                execution_count,
                cell_count,
            )
            .map_err(|error| match error {
                JointReportErrorV1::ResourceExhausted => SelectedRecheckErrorV1::ResourceExhausted,
                JointReportErrorV1::MissingRootSurface(_)
                | JointReportErrorV1::CandidatePaintMismatch { .. } => {
                    SelectedRecheckErrorV1::InvariantDrift
                }
                JointReportErrorV1::Evaluator(error) => SelectedRecheckErrorV1::Evaluator(error),
            })?;
        if let Some(violation) = matrices
            .cells
            .iter()
            .copied()
            .find(|cell| !cell.decision.is_pass())
        {
            return Err(SelectedRecheckErrorV1::Violation(Box::new(violation)));
        }
        Ok(RevisionBoundVerifiedSelectionV1 {
            selected: self,
            recheck: FreshJointRecheckV1 {
                executions: matrices.executions,
                cells: matrices.cells,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SelectedRecheckErrorV1 {
    ResourceExhausted,
    InvariantDrift,
    Evaluator(ApplicableWcag22EvaluationErrorV1),
    Violation(Box<JointConstraintCellV1>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FreshJointRecheckV1 {
    executions: Box<[JointExecutionRecordV1]>,
    cells: Box<[JointConstraintCellV1]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RevisionBoundVerifiedSelectionV1 {
    selected: SelectedJointTupleV1,
    recheck: FreshJointRecheckV1,
}

impl RevisionBoundVerifiedSelectionV1 {
    pub(crate) const fn ordinal(&self) -> CandidateOrdinalV1 {
        self.selected.candidate.ordinal
    }

    pub(crate) const fn report(&self) -> &FullHardReportV1 {
        &self.selected.report
    }

    pub(crate) fn policy(&self) -> &[CandidateOrdinalV1] {
        &self.selected.policy.order
    }

    pub(crate) fn fresh_executions(&self) -> &[JointExecutionRecordV1] {
        &self.recheck.executions
    }

    pub(crate) fn fresh_cells(&self) -> &[JointConstraintCellV1] {
        &self.recheck.cells
    }
}
