//! Private generic point Program compiler and lowering path.
//!
//! The authored graph has no client/UI role vocabulary. Paints are physical
//! source-plus-straight-alpha programs, occurrences are modeled applications of
//! Paint to Surface, constraints declare assessments of those exact
//! occurrences, and outputs bind opaque slots back to Paints. The compiled
//! result owns only admitted, canonical topology; runtime observation,
//! lifecycle and terminal emission belong to the sole revision-bound Session.
//! Its current values are encoded point transport-only; they are not LCS
//! observation or evidence.

use std::marker::PhantomData;
use std::rc::Rc;

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, AppearanceWorkspace,
    BindingError, ColorInputId, CompileError, CompiledAppearanceGraph, CompiledOccurrenceSlotV1,
    CompiledPaintSlotV1, EncodedPointPaintV1, OccurrenceId, OccurrenceSpec, OpacityInputId,
    PaintId, PaintSpec, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::composition::CompositionProfileV1;
use crate::constraints::{
    HardDecision, PointEvaluatorV1, PointInvocation, VisiblePointPassEvidence,
    VisiblePointViolationEvidence, assess_visible_point_hard,
};
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationError, ObservationGroupId,
    ObservationSchemaMismatchV1, ObservationStreamId, RevisionBoundObservationV1,
    canonicalize_observation_schema,
};
use crate::session::{
    Session, SessionDecision, SessionEvidenceV1, SessionObservationBindingPermitV1, SessionPlanV1,
    private as session_private,
};

/// One immutable encoded colour binding owned by a [`Program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorInput {
    id: ColorInputId,
    value: Srgb8,
}

impl ColorInput {
    pub const fn new(id: ColorInputId, value: Srgb8) -> Self {
        Self { id, value }
    }

    pub const fn id(self) -> ColorInputId {
        self.id
    }

    pub const fn value(self) -> Srgb8 {
        self.value
    }
}

/// One immutable straight-alpha binding owned by a [`Program`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpacityInput {
    id: OpacityInputId,
    value: f64,
}

impl OpacityInput {
    pub const fn new(id: OpacityInputId, value: f64) -> Self {
        Self { id, value }
    }

    pub const fn id(self) -> OpacityInputId {
        self.id
    }

    pub const fn value(self) -> f64 {
        self.value
    }
}

/// Generic point Paint constructor algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Solid {
        id: PaintId,
        color: ColorInputId,
    },
    Opacity {
        id: PaintId,
        source: PaintId,
        opacity: OpacityInputId,
    },
}

/// Generic point Surface constructor algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Input {
        id: SurfaceId,
        input: SurfaceInputPortId,
    },
    FromOccurrence {
        id: SurfaceId,
        occurrence: OccurrenceId,
    },
}

/// Closed mathematical composition profile set for this Program version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionProfile {
    EncodedSrgb8SourceOverV1,
}

/// The canonical application of one Paint to one Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    id: OccurrenceId,
    subject: PaintId,
    against: SurfaceId,
    composition: CompositionProfile,
}

impl Occurrence {
    pub const fn new(
        id: OccurrenceId,
        subject: PaintId,
        against: SurfaceId,
        composition: CompositionProfile,
    ) -> Self {
        Self {
            id,
            subject,
            against,
            composition,
        }
    }

    pub const fn id(self) -> OccurrenceId {
        self.id
    }

    pub const fn subject(self) -> PaintId {
        self.subject
    }

    pub const fn against(self) -> SurfaceId {
        self.against
    }

    pub const fn composition(self) -> CompositionProfile {
        self.composition
    }
}

/// Opaque authored constraint identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstraintId(u32);

impl ConstraintId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Opaque authored terminal output identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputSlotId(u32);

impl OutputSlotId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Type-level marker for a mandatory constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardModeV1 {}

/// Type-level marker for a diagnostic-only constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportModeV1 {}

/// One typed evaluator invocation over one exact visible occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstraintInvocation<Invocation, Mode> {
    id: ConstraintId,
    target: OccurrenceId,
    invocation: Invocation,
    mode: PhantomData<fn() -> Mode>,
}

impl<Invocation> ConstraintInvocation<Invocation, HardModeV1> {
    pub const fn hard(id: ConstraintId, target: OccurrenceId, invocation: Invocation) -> Self {
        Self {
            id,
            target,
            invocation,
            mode: PhantomData,
        }
    }
}

impl<Invocation> ConstraintInvocation<Invocation, ReportModeV1> {
    pub const fn report_only(
        id: ConstraintId,
        target: OccurrenceId,
        invocation: Invocation,
    ) -> Self {
        Self {
            id,
            target,
            invocation,
            mode: PhantomData,
        }
    }
}

impl<Invocation, Mode> ConstraintInvocation<Invocation, Mode> {
    pub const fn id(&self) -> ConstraintId {
        self.id
    }

    pub const fn target(&self) -> OccurrenceId {
        self.target
    }

    pub const fn invocation(&self) -> &Invocation {
        &self.invocation
    }
}

/// The two authored modality domains remain type-separated until compilation.
#[derive(Debug, PartialEq, Eq)]
pub struct ConstraintSet<Invocation> {
    hard: Vec<ConstraintInvocation<Invocation, HardModeV1>>,
    report_only: Vec<ConstraintInvocation<Invocation, ReportModeV1>>,
}

impl<Invocation> ConstraintSet<Invocation> {
    pub fn new(
        hard: Vec<ConstraintInvocation<Invocation, HardModeV1>>,
        report_only: Vec<ConstraintInvocation<Invocation, ReportModeV1>>,
    ) -> Self {
        Self { hard, report_only }
    }

    pub fn hard(&self) -> &[ConstraintInvocation<Invocation, HardModeV1>] {
        &self.hard
    }

    pub fn report_only(&self) -> &[ConstraintInvocation<Invocation, ReportModeV1>] {
        &self.report_only
    }

    fn is_empty(&self) -> bool {
        self.hard.is_empty() && self.report_only.is_empty()
    }

    fn checked_len(&self) -> Option<usize> {
        self.hard.len().checked_add(self.report_only.len())
    }
}

/// Compile-time binding from one terminal slot to one Paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputBinding {
    output: OutputSlotId,
    paint: PaintId,
}

impl OutputBinding {
    pub const fn new(output: OutputSlotId, paint: PaintId) -> Self {
        Self { output, paint }
    }

    pub const fn output(self) -> OutputSlotId {
        self.output
    }

    pub const fn paint(self) -> PaintId {
        self.paint
    }
}

/// One compile-time atomic correlation boundary for this Program epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationGroup {
    id: ObservationGroupId,
    surface_input_ports: Vec<SurfaceInputPortId>,
}

impl ObservationGroup {
    pub const fn new(id: ObservationGroupId, surface_input_ports: Vec<SurfaceInputPortId>) -> Self {
        Self {
            id,
            surface_input_ports,
        }
    }

    pub const fn id(&self) -> ObservationGroupId {
        self.id
    }

    pub fn surface_input_ports(&self) -> &[SurfaceInputPortId] {
        &self.surface_input_ports
    }
}

/// Immutable generic point Program.
pub struct Program<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    colors: Vec<ColorInput>,
    observation_group: ObservationGroup,
    opacities: Vec<OpacityInput>,
    paints: Vec<Paint>,
    surfaces: Vec<Surface>,
    occurrences: Vec<Occurrence>,
    constraints: ConstraintSet<PointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
}

impl<Evaluation> Program<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        colors: Vec<ColorInput>,
        observation_group: ObservationGroup,
        opacities: Vec<OpacityInput>,
        paints: Vec<Paint>,
        surfaces: Vec<Surface>,
        occurrences: Vec<Occurrence>,
        constraints: ConstraintSet<PointInvocation<Evaluation>>,
        outputs: Vec<OutputBinding>,
        evaluator: Evaluation,
    ) -> Self {
        Self {
            colors,
            observation_group,
            opacities,
            paints,
            surfaces,
            occurrences,
            constraints,
            outputs,
            evaluator,
        }
    }

    pub fn compile(self) -> Result<CompiledProgram<Evaluation>, ProgramCompileError> {
        prepare_program(self).map(|epoch| CompiledProgram {
            epoch: Rc::new(epoch),
        })
    }
}

/// Atomic compile failure. No executable partial graph escapes.
#[derive(Debug, PartialEq, Eq)]
pub enum ProgramCompileError {
    DuplicateColorInput {
        input: ColorInputId,
    },
    DuplicateOpacityInput {
        input: OpacityInputId,
    },
    DuplicateSurfaceInputPort {
        input: SurfaceInputPortId,
    },
    DuplicatePaint {
        paint: PaintId,
    },
    DuplicateSurface {
        surface: SurfaceId,
    },
    DuplicateOccurrence {
        occurrence: OccurrenceId,
    },
    MissingPaintColorInput {
        paint: PaintId,
        input: ColorInputId,
    },
    MissingPaintSource {
        paint: PaintId,
        source: PaintId,
    },
    MissingPaintOpacityInput {
        paint: PaintId,
        input: OpacityInputId,
    },
    MissingSurfaceInputPort {
        surface: SurfaceId,
        input: SurfaceInputPortId,
    },
    MissingSurfaceOccurrence {
        surface: SurfaceId,
        occurrence: OccurrenceId,
    },
    MissingOccurrencePaint {
        occurrence: OccurrenceId,
        paint: PaintId,
    },
    MissingOccurrenceBackdrop {
        occurrence: OccurrenceId,
        surface: SurfaceId,
    },
    PaintCycle {
        paints: Vec<PaintId>,
    },
    RenderCycle {
        surfaces: Vec<SurfaceId>,
        occurrences: Vec<OccurrenceId>,
    },
    OpacityOutOfDomain {
        input: OpacityInputId,
    },
    EmptyObservationGroup {
        group: ObservationGroupId,
    },
    EmptyOccurrenceSet,
    EmptyConstraintSet,
    EmptyOutputSet,
    DuplicateConstraint {
        constraint: ConstraintId,
    },
    MissingConstraintOccurrence {
        constraint: ConstraintId,
        occurrence: OccurrenceId,
    },
    DuplicateOutputSlot {
        output: OutputSlotId,
    },
    MissingOutputPaint {
        output: OutputSlotId,
        paint: PaintId,
    },
    ResourceExhausted,
    InternalInvariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompiledConstraintModeV1 {
    Hard,
    ReportOnly,
}

struct CompiledPointConstraint<Invocation> {
    id: ConstraintId,
    target_id: OccurrenceId,
    target: CompiledOccurrenceSlotV1,
    mode: CompiledConstraintModeV1,
    invocation: Invocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledOutputBinding {
    output: OutputSlotId,
    paint_id: PaintId,
    paint: CompiledPaintSlotV1,
}

struct CompiledObservationGroupV1 {
    id: ObservationGroupId,
    schema: CanonicalObservationSchemaV1,
}

struct ProgramEpochV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    evaluator: Evaluation,
    graph: CompiledAppearanceGraph,
    binding_template: AdmittedAppearanceBindings,
    observation_group: CompiledObservationGroupV1,
    constraints: Box<[CompiledPointConstraint<PointInvocation<Evaluation>>]>,
    outputs: Box<[CompiledOutputBinding]>,
}

/// Fully validated immutable Program, not yet attached to runtime.
pub struct CompiledProgram<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    epoch: Rc<ProgramEpochV1<Evaluation>>,
}

impl<Evaluation> CompiledProgram<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub fn observation_group_id(&self) -> ObservationGroupId {
        self.epoch.observation_group.id
    }

    pub fn surface_input_ports(&self) -> &[SurfaceInputPortId] {
        self.epoch.observation_group.schema.as_slice()
    }

    pub fn constraint_ids(&self) -> impl ExactSizeIterator<Item = ConstraintId> + '_ {
        self.epoch
            .constraints
            .iter()
            .map(|constraint| constraint.id)
    }

    pub fn outputs(&self) -> impl ExactSizeIterator<Item = (OutputSlotId, PaintId)> + '_ {
        self.epoch
            .outputs
            .iter()
            .map(|output| (output.output, output.paint_id))
    }

    /// Create one independent stream-affine Session from the immutable
    /// compiled epoch. The graph/evaluator/schema stay shared by strong
    /// ownership; mutable bindings and workspace belong only to this Session.
    pub(crate) fn instantiate(
        &self,
        stream: ObservationStreamId,
    ) -> Result<Session<ProgramSessionPlan<Evaluation>>, ProgramSessionInstantiateError> {
        let bindings = self
            .epoch
            .binding_template
            .try_clone_v1()
            .map_err(map_session_instantiate_error)?;
        let workspace = self
            .epoch
            .graph
            .new_workspace()
            .map_err(map_session_instantiate_error)?;
        Ok(Session::new(
            stream,
            ProgramSessionPlan {
                epoch: Rc::clone(&self.epoch),
                bindings,
                workspace,
            },
        ))
    }
}

/// Failure while preparing mutable storage for one independent Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramSessionInstantiateError {
    ResourceExhausted,
    InternalInvariant,
}

fn map_session_instantiate_error(error: BindingError) -> ProgramSessionInstantiateError {
    match error {
        BindingError::ResourceExhausted => ProgramSessionInstantiateError::ResourceExhausted,
        _ => ProgramSessionInstantiateError::InternalInvariant,
    }
}

/// One evaluator classification retained in the complete Program report.
pub enum ProgramConstraintResultV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    Pass(VisiblePointPassEvidence<Evaluation>),
    Violation(VisiblePointViolationEvidence<Evaluation>),
}

impl<Evaluation> ProgramConstraintResultV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn is_violation(&self) -> bool {
        matches!(self, Self::Violation(_))
    }
}

/// One canonical `physical case × constraint` report cell.
pub struct ProgramConstraintCellV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    case_index: usize,
    constraint: ConstraintId,
    target: OccurrenceId,
    mode: CompiledConstraintModeV1,
    result: ProgramConstraintResultV1<Evaluation>,
}

impl<Evaluation> ProgramConstraintCellV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    pub const fn constraint(&self) -> ConstraintId {
        self.constraint
    }

    pub const fn target(&self) -> OccurrenceId {
        self.target
    }

    pub const fn is_hard(&self) -> bool {
        matches!(self.mode, CompiledConstraintModeV1::Hard)
    }

    pub const fn result(&self) -> &ProgramConstraintResultV1<Evaluation> {
        &self.result
    }
}

/// Complete revision-bound assessment in case-major, constraint-ID order.
pub struct ProgramReportV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    observation: RevisionBoundObservationV1,
    cells: Vec<ProgramConstraintCellV1<Evaluation>>,
}

impl<Evaluation> ProgramReportV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub fn cells(&self) -> &[ProgramConstraintCellV1<Evaluation>] {
        &self.cells
    }
}

/// One emitted Program Paint routed to an opaque output slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramOutputV1 {
    output: OutputSlotId,
    paint: EncodedPointPaintV1,
}

impl ProgramOutputV1 {
    pub const fn output(self) -> OutputSlotId {
        self.output
    }

    pub const fn paint(self) -> EncodedPointPaintV1 {
        self.paint
    }
}

/// All hard cells passed over the complete admitted physical support.
pub struct ProgramVerifiedV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    report: ProgramReportV1<Evaluation>,
    outputs: Vec<ProgramOutputV1>,
}

impl<Evaluation> session_private::EvidenceSealed for ProgramVerifiedV1<Evaluation> where
    Evaluation: PointEvaluatorV1
{
}

impl<Evaluation> SessionEvidenceV1 for ProgramVerifiedV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        self.report().observation()
    }
}

impl<Evaluation> ProgramVerifiedV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn report(&self) -> &ProgramReportV1<Evaluation> {
        &self.report
    }

    pub fn outputs(&self) -> &[ProgramOutputV1] {
        &self.outputs
    }
}

/// Complete report containing at least one hard violation. Outputs are absent
/// by construction and therefore cannot be mistaken for committed Paints.
pub struct ProgramViolationV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    report: ProgramReportV1<Evaluation>,
}

impl<Evaluation> session_private::EvidenceSealed for ProgramViolationV1<Evaluation> where
    Evaluation: PointEvaluatorV1
{
}

impl<Evaluation> SessionEvidenceV1 for ProgramViolationV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        self.report().observation()
    }
}

impl<Evaluation> ProgramViolationV1<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn report(&self) -> &ProgramReportV1<Evaluation> {
        &self.report
    }
}

/// Program execution failure before Session commit.
#[derive(Debug, PartialEq, Eq)]
pub enum ProgramSessionEvaluationError<EvaluationError> {
    ObservationSchemaMismatch(ObservationSchemaMismatchV1),
    ResourceExhausted,
    Evaluator {
        case_index: usize,
        constraint: ConstraintId,
        source: EvaluationError,
    },
    OutputVariesAcrossCases {
        output: OutputSlotId,
        first_case: usize,
        actual_case: usize,
    },
    InternalInvariant,
}

type ProgramEvaluatorError<Evaluation> = <Evaluation as crate::constraints::Evaluator<
    crate::appearance::ModeledSrgb8PointOccurrence,
>>::Error;

type ProgramSessionEvaluationResult<Evaluation> = Result<
    SessionDecision<ProgramVerifiedV1<Evaluation>, ProgramViolationV1<Evaluation>>,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>;

/// Per-Session mutable execution state backed by one strong immutable epoch.
pub struct ProgramSessionPlan<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    epoch: Rc<ProgramEpochV1<Evaluation>>,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
}

impl<Evaluation> session_private::PlanSealed for ProgramSessionPlan<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
}

impl<Evaluation> SessionPlanV1 for ProgramSessionPlan<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    type Verified = ProgramVerifiedV1<Evaluation>;
    type Violation = ProgramViolationV1<Evaluation>;
    type Error = ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>;

    fn observation_schema(&self) -> &CanonicalObservationSchemaV1 {
        &self.epoch.observation_group.schema
    }

    fn evaluate(
        &mut self,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        evaluate_program_session(self, observation)
    }
}

fn evaluate_program_session<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    observation: RevisionBoundObservationV1,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    let epoch = &plan.epoch;
    let schema = &epoch.observation_group.schema;
    if !observation.shares_schema_backing_with(schema) {
        observation
            .validate_surface_schema(schema.as_slice())
            .map_err(ProgramSessionEvaluationError::ObservationSchemaMismatch)?;
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }

    let case_count = observation.physical_case_count();
    let cell_count = case_count
        .checked_mul(epoch.constraints.len())
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(cell_count)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(epoch.outputs.len())
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;

    let mut has_hard_violation = false;
    let mut output_mismatch = None;
    for case_index in 0..case_count {
        let values = observation
            .physical_values(case_index)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if values.len() != schema.as_slice().len() {
            let binding_index = values.len().min(schema.as_slice().len());
            return Err(ProgramSessionEvaluationError::ObservationSchemaMismatch(
                ObservationSchemaMismatchV1::new(
                    case_index,
                    binding_index,
                    schema.as_slice().get(binding_index).copied(),
                    None,
                ),
            ));
        }
        plan.bindings
            .overwrite_surface_inputs_canonical(schema.as_slice().iter().copied(), |index| {
                values[index]
            })
            .map_err(map_program_execution_binding_error)?;
        let evaluation = epoch
            .graph
            .evaluate_admitted_into(&plan.bindings, &mut plan.workspace)
            .map_err(map_program_execution_binding_error)?;

        for constraint in epoch.constraints.iter() {
            let source = evaluation
                .occurrence_at(constraint.target)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            let decision =
                assess_visible_point_hard(source, &epoch.evaluator, constraint.invocation)
                    .map_err(|source| ProgramSessionEvaluationError::Evaluator {
                        case_index,
                        constraint: constraint.id,
                        source,
                    })?;
            let result = match decision {
                HardDecision::Pass(evidence) => ProgramConstraintResultV1::Pass(evidence),
                HardDecision::Violation(evidence) => {
                    if matches!(constraint.mode, CompiledConstraintModeV1::Hard) {
                        has_hard_violation = true;
                    }
                    ProgramConstraintResultV1::Violation(evidence)
                }
            };
            cells.push(ProgramConstraintCellV1 {
                case_index,
                constraint: constraint.id,
                target: constraint.target_id,
                mode: constraint.mode,
                result,
            });
        }

        for (output_index, output) in epoch.outputs.iter().enumerate() {
            let paint = evaluation
                .paint_at(output.paint)
                .copied()
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            if paint.id() != output.paint_id {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            let routed = ProgramOutputV1 {
                output: output.output,
                paint,
            };
            if case_index == 0 {
                outputs.push(routed);
            } else if outputs.get(output_index).copied() != Some(routed)
                && output_mismatch.is_none()
            {
                output_mismatch = Some(ProgramSessionEvaluationError::OutputVariesAcrossCases {
                    output: output.output,
                    first_case: 0,
                    actual_case: case_index,
                });
            }
        }
    }

    let report = ProgramReportV1 { observation, cells };
    if let Some(error) = output_mismatch {
        Err(error)
    } else if has_hard_violation {
        Ok(SessionDecision::Violation(ProgramViolationV1 { report }))
    } else {
        Ok(SessionDecision::Verified(ProgramVerifiedV1 {
            report,
            outputs,
        }))
    }
}

fn map_program_execution_binding_error<EvaluationError>(
    error: BindingError,
) -> ProgramSessionEvaluationError<EvaluationError> {
    match error {
        BindingError::ResourceExhausted => ProgramSessionEvaluationError::ResourceExhausted,
        _ => ProgramSessionEvaluationError::InternalInvariant,
    }
}

fn prepare_program<Evaluation>(
    program: Program<Evaluation>,
) -> Result<ProgramEpochV1<Evaluation>, ProgramCompileError>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    if program.observation_group.surface_input_ports.is_empty() {
        return Err(ProgramCompileError::EmptyObservationGroup {
            group: program.observation_group.id,
        });
    }
    if program.occurrences.is_empty() {
        return Err(ProgramCompileError::EmptyOccurrenceSet);
    }
    if program.constraints.is_empty() {
        return Err(ProgramCompileError::EmptyConstraintSet);
    }
    if program.outputs.is_empty() {
        return Err(ProgramCompileError::EmptyOutputSet);
    }
    check_render_node_count(program.surfaces.len(), program.occurrences.len())?;
    program
        .constraints
        .checked_len()
        .ok_or(ProgramCompileError::ResourceExhausted)?;

    let graph = lower_graph(&program).compile().map_err(map_compile_error)?;
    let binding_template = graph
        .admit_bindings(&lower_bindings(&program))
        .map_err(map_binding_compile_error)?;

    let mut surface_input_ports = Vec::new();
    surface_input_ports
        .try_reserve_exact(program.observation_group.surface_input_ports.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    surface_input_ports.extend_from_slice(&program.observation_group.surface_input_ports);
    surface_input_ports.sort_unstable();
    if !canonical_surface_input_port_sequence_matches(
        graph.surface_input_ports(),
        &surface_input_ports,
    ) {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let observation_schema = canonicalize_observation_schema(surface_input_ports)
        .map_err(map_observation_schema_compile_error)?;

    let constraints = compile_constraints::<Evaluation>(&graph, program.constraints)?;
    let outputs = compile_outputs(&graph, program.outputs)?;
    Ok(ProgramEpochV1 {
        evaluator: program.evaluator,
        graph,
        binding_template,
        observation_group: CompiledObservationGroupV1 {
            id: program.observation_group.id,
            schema: observation_schema,
        },
        constraints,
        outputs,
    })
}

fn map_observation_schema_compile_error(error: ObservationError) -> ProgramCompileError {
    match error {
        ObservationError::ResourceExhausted => ProgramCompileError::ResourceExhausted,
        _ => ProgramCompileError::InternalInvariant,
    }
}

struct LoweredConstraint<Invocation> {
    id: ConstraintId,
    target: OccurrenceId,
    mode: CompiledConstraintModeV1,
    invocation: Invocation,
}

fn compile_constraints<Evaluation>(
    graph: &CompiledAppearanceGraph,
    authored: ConstraintSet<PointInvocation<Evaluation>>,
) -> Result<Box<[CompiledPointConstraint<PointInvocation<Evaluation>>]>, ProgramCompileError>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    let total = authored
        .hard
        .len()
        .checked_add(authored.report_only.len())
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let mut lowered = Vec::new();
    lowered
        .try_reserve_exact(total)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    lowered.extend(
        authored
            .hard
            .into_iter()
            .map(|constraint| LoweredConstraint {
                id: constraint.id,
                target: constraint.target,
                mode: CompiledConstraintModeV1::Hard,
                invocation: constraint.invocation,
            }),
    );
    lowered.extend(
        authored
            .report_only
            .into_iter()
            .map(|constraint| LoweredConstraint {
                id: constraint.id,
                target: constraint.target,
                mode: CompiledConstraintModeV1::ReportOnly,
                invocation: constraint.invocation,
            }),
    );
    lowered.sort_unstable_by_key(|constraint| constraint.id);
    if let Some(duplicate) = lowered
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
        .map(|pair| pair[0].id)
    {
        return Err(ProgramCompileError::DuplicateConstraint {
            constraint: duplicate,
        });
    }
    for constraint in &lowered {
        if graph.bind_occurrence(constraint.target).is_none() {
            return Err(ProgramCompileError::MissingConstraintOccurrence {
                constraint: constraint.id,
                occurrence: constraint.target,
            });
        }
    }

    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(total)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for constraint in lowered {
        let target = graph
            .bind_occurrence(constraint.target)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        compiled.push(CompiledPointConstraint {
            id: constraint.id,
            target_id: constraint.target,
            target,
            mode: constraint.mode,
            invocation: constraint.invocation,
        });
    }
    Ok(compiled.into_boxed_slice())
}

fn compile_outputs(
    graph: &CompiledAppearanceGraph,
    authored: Vec<OutputBinding>,
) -> Result<Box<[CompiledOutputBinding]>, ProgramCompileError> {
    let len = authored.len();
    let mut authored = authored;
    authored.sort_unstable_by_key(|output| output.output);
    if let Some(duplicate) = authored
        .windows(2)
        .find(|pair| pair[0].output == pair[1].output)
        .map(|pair| pair[0].output)
    {
        return Err(ProgramCompileError::DuplicateOutputSlot { output: duplicate });
    }
    for output in &authored {
        if graph.bind_paint(output.paint).is_none() {
            return Err(ProgramCompileError::MissingOutputPaint {
                output: output.output,
                paint: output.paint,
            });
        }
    }

    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(len)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for output in authored {
        let paint = graph
            .bind_paint(output.paint)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        compiled.push(CompiledOutputBinding {
            output: output.output,
            paint_id: output.paint,
            paint,
        });
    }
    Ok(compiled.into_boxed_slice())
}

pub(crate) fn check_render_node_count(
    surface_count: usize,
    occurrence_count: usize,
) -> Result<(), ProgramCompileError> {
    surface_count
        .checked_add(occurrence_count)
        .ok_or(ProgramCompileError::ResourceExhausted)
        .map(|_| ())
}

pub(crate) fn canonical_surface_input_port_sequence_matches(
    actual: impl IntoIterator<Item = SurfaceInputPortId>,
    expected: &[SurfaceInputPortId],
) -> bool {
    actual.into_iter().eq(expected.iter().copied())
}

fn lower_graph<Evaluation>(program: &Program<Evaluation>) -> AppearanceGraphSpec
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    AppearanceGraphSpec::new(
        program.colors.iter().map(|input| input.id).collect(),
        program.observation_group.surface_input_ports.clone(),
        program.opacities.iter().map(|input| input.id).collect(),
        program
            .paints
            .iter()
            .map(|paint| match *paint {
                Paint::Solid { id, color } => PaintSpec::Solid { id, color },
                Paint::Opacity {
                    id,
                    source,
                    opacity,
                } => PaintSpec::Opacity {
                    id,
                    source,
                    opacity,
                },
            })
            .collect(),
        program
            .surfaces
            .iter()
            .map(|surface| match *surface {
                Surface::Input { id, input } => SurfaceSpec::Input { id, port: input },
                Surface::FromOccurrence { id, occurrence } => {
                    SurfaceSpec::FromOccurrence { id, occurrence }
                }
            })
            .collect(),
        program
            .occurrences
            .iter()
            .map(|occurrence| OccurrenceSpec {
                id: occurrence.id,
                subject: occurrence.subject,
                against: occurrence.against,
                profile: match occurrence.composition {
                    CompositionProfile::EncodedSrgb8SourceOverV1 => {
                        CompositionProfileV1::EncodedSrgb8SourceOverV1
                    }
                },
            })
            .collect(),
    )
}

fn lower_bindings<Evaluation>(program: &Program<Evaluation>) -> AppearanceBindings
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    AppearanceBindings::new(
        program
            .colors
            .iter()
            .map(|input| (input.id, input.value))
            .collect(),
        program
            .observation_group
            .surface_input_ports
            .iter()
            .map(|input| (*input, Srgb8::new([0; 3])))
            .collect(),
        program
            .opacities
            .iter()
            .map(|input| (input.id, input.value))
            .collect(),
    )
}

fn map_compile_error(error: CompileError) -> ProgramCompileError {
    match error {
        CompileError::DuplicateColorInput { input } => {
            ProgramCompileError::DuplicateColorInput { input }
        }
        CompileError::DuplicateOpacityInput { input } => {
            ProgramCompileError::DuplicateOpacityInput { input }
        }
        CompileError::DuplicateSurfaceInputPort { input } => {
            ProgramCompileError::DuplicateSurfaceInputPort { input }
        }
        CompileError::DuplicatePaint { paint } => ProgramCompileError::DuplicatePaint { paint },
        CompileError::DuplicateSurface { surface } => {
            ProgramCompileError::DuplicateSurface { surface }
        }
        CompileError::DuplicateOccurrence { occurrence } => {
            ProgramCompileError::DuplicateOccurrence { occurrence }
        }
        CompileError::MissingPaintColorInput { paint, input } => {
            ProgramCompileError::MissingPaintColorInput { paint, input }
        }
        CompileError::MissingPaintSource { paint, source } => {
            ProgramCompileError::MissingPaintSource { paint, source }
        }
        CompileError::MissingPaintOpacityInput { paint, input } => {
            ProgramCompileError::MissingPaintOpacityInput { paint, input }
        }
        CompileError::MissingSurfaceInputPort { surface, input } => {
            ProgramCompileError::MissingSurfaceInputPort { surface, input }
        }
        CompileError::MissingSurfaceOccurrence {
            surface,
            occurrence,
        } => ProgramCompileError::MissingSurfaceOccurrence {
            surface,
            occurrence,
        },
        CompileError::MissingOccurrencePaint { occurrence, paint } => {
            ProgramCompileError::MissingOccurrencePaint { occurrence, paint }
        }
        CompileError::MissingOccurrenceBackdrop {
            occurrence,
            surface,
        } => ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence,
            surface,
        },
        CompileError::PaintCycle { paints } => ProgramCompileError::PaintCycle { paints },
        CompileError::RenderCycle {
            surfaces,
            occurrences,
        } => ProgramCompileError::RenderCycle {
            surfaces,
            occurrences,
        },
    }
}

fn map_binding_compile_error(error: BindingError) -> ProgramCompileError {
    match error {
        BindingError::OpacityOutOfDomain { input, .. } => {
            ProgramCompileError::OpacityOutOfDomain { input }
        }
        BindingError::ResourceExhausted => ProgramCompileError::ResourceExhausted,
        _ => ProgramCompileError::InternalInvariant,
    }
}
