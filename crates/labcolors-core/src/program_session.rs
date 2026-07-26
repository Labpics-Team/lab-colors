//! Private generic point Program compiler and lowering path.
//!
//! The authored graph has no client/UI role vocabulary. Paints are physical
//! source-plus-straight-alpha programs, occurrences are modeled applications of
//! Paint to Surface, constraints declare assessments of those exact
//! occurrences, and outputs bind opaque slots back to Paints. The compiled
//! result owns only admitted, canonical topology; runtime observation,
//! lifecycle and terminal emission belong to the sole revision-bound Session.
//! Output transport and encoded-only assessments retain exact physical
//! occurrence evidence plus the declared appearance context. A modeled LCS
//! occurrence is derived only through its separate typed capability; neither
//! claim is renderer observation or human-subject evidence.

use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, AppearanceWorkspace,
    BindingError, ColorInputId, CompileError, CompiledAppearanceGraph, CompiledColorInputSlotV1,
    CompiledOccurrenceSlotV1, CompiledPaintSlotV1, EncodedPointPaintV1, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::composition::CompositionProfileV1;
use crate::constraints::{
    Evaluator, ExactSrgb8IdentityV1, HardDecision, ProgramConstraintContentV1,
    ProgramPointAssessmentErrorV1, ProgramPointEvaluatorContentV1, ProgramPointEvaluatorV1,
    ProgramPointInvocation, ProgramPointOccurrenceV1, ProgramPointTargetV1,
    ProgramVisiblePointBindingV1, ProgramVisiblePointPassEvidence,
    ProgramVisiblePointViolationEvidence, Wcag22Srgb8V1, assess_program_point_hard,
};
use crate::joint::{
    AdmittedFiniteJointOrderV1, FiniteDomainOrdinalV1, FiniteJointOrderErrorV1,
    admit_finite_joint_order_v1,
};
use crate::lcs_occurrence::{AppearanceContextId, ColorSignal};
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationError, ObservationGroupId,
    ObservationSchemaMismatchV1, ObservationStreamId, RevisionBoundObservationV1,
    canonicalize_observation_schema,
};
use crate::session::{
    Session, SessionDecision, SessionEvidenceV1, SessionObservationBindingPermitV1, SessionPlanV1,
    private as session_private,
};
use crate::wcag22::Wcag22CriterionV1;

#[path = "program_identity.rs"]
mod identity;
pub(crate) use identity::ProgramContentIdentityV2;

/// Opaque identity of one immutable authored colour source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// One immutable encoded source owned by a [`Program`]. Sources carry data,
/// never solver freedom and never appear directly in Paint topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    id: SourceId,
    signal: ColorSignal,
}

impl Source {
    pub const fn new(id: SourceId, signal: ColorSignal) -> Self {
        Self { id, signal }
    }

    pub const fn id(self) -> SourceId {
        self.id
    }

    pub const fn signal(self) -> ColorSignal {
        self.signal
    }
}

/// Opaque identity of one jointly selected finite target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId(u32);

impl TargetId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Opaque identity of one candidate inside a finite target domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetCandidateId(u32);

impl TargetCandidateId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// One candidate signal in a finite target domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCandidateV1 {
    id: TargetCandidateId,
    signal: ColorSignal,
}

impl TargetCandidateV1 {
    pub const fn new(id: TargetCandidateId, signal: ColorSignal) -> Self {
        Self { id, signal }
    }

    /// Bind one encoded sRGB8 candidate through the sole admitted signal
    /// profile. Package authoring never carries a free-form profile tag.
    pub const fn from_srgb8(id: TargetCandidateId, value: Srgb8) -> Self {
        Self::new(id, ColorSignal::from_srgb8(value))
    }

    pub const fn id(self) -> TargetCandidateId {
        self.id
    }

    pub const fn signal(self) -> ColorSignal {
        self.signal
    }
}

/// Closed authored freedom of one Target. Fixed targets use their Source
/// signal exactly; finite targets can select only from the explicit domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetDomainV1 {
    Fixed,
    Finite(Vec<TargetCandidateV1>),
}

/// A Paint-addressable target distinct from both source data and appearance
/// storage. Only finite targets participate in declared joint selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    id: TargetId,
    source: SourceId,
    domain: TargetDomainV1,
}

impl Target {
    pub const fn new(id: TargetId, source: SourceId, domain: TargetDomainV1) -> Self {
        Self { id, source, domain }
    }

    pub const fn fixed(id: TargetId, source: SourceId) -> Self {
        Self::new(id, source, TargetDomainV1::Fixed)
    }

    pub const fn finite(
        id: TargetId,
        source: SourceId,
        candidates: Vec<TargetCandidateV1>,
    ) -> Self {
        Self::new(id, source, TargetDomainV1::Finite(candidates))
    }

    pub const fn id(&self) -> TargetId {
        self.id
    }

    pub const fn source(&self) -> SourceId {
        self.source
    }

    pub const fn domain(&self) -> &TargetDomainV1 {
        &self.domain
    }
}

/// One typed target/candidate assignment inside a declared joint state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetCandidateChoiceV1 {
    target: TargetId,
    candidate: TargetCandidateId,
}

impl TargetCandidateChoiceV1 {
    pub const fn new(target: TargetId, candidate: TargetCandidateId) -> Self {
        Self { target, candidate }
    }

    pub const fn target(self) -> TargetId {
        self.target
    }

    pub const fn candidate(self) -> TargetCandidateId {
        self.candidate
    }
}

/// One complete joint candidate state. Choices are keyed, so authored choice
/// order has no physical meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointCandidateStateV1 {
    choices: Vec<TargetCandidateChoiceV1>,
}

impl JointCandidateStateV1 {
    pub const fn new(choices: Vec<TargetCandidateChoiceV1>) -> Self {
        Self { choices }
    }

    pub fn choices(&self) -> &[TargetCandidateChoiceV1] {
        &self.choices
    }
}

/// Explicit total order over every state in the finite product domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredJointSelectionV1 {
    states: Vec<JointCandidateStateV1>,
}

impl DeclaredJointSelectionV1 {
    pub const fn new(states: Vec<JointCandidateStateV1>) -> Self {
        Self { states }
    }

    pub fn states(&self) -> &[JointCandidateStateV1] {
        &self.states
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
        target: TargetId,
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
    context: AppearanceContextId,
}

impl Occurrence {
    pub const fn new(
        id: OccurrenceId,
        subject: PaintId,
        against: SurfaceId,
        composition: CompositionProfile,
        context: AppearanceContextId,
    ) -> Self {
        Self {
            id,
            subject,
            against,
            composition,
            context,
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

    pub const fn context(self) -> AppearanceContextId {
        self.context
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

/// Static dispatch contract used by one Program epoch. The invocation,
/// evaluator error, and both evidence branches are one closed type family;
/// no trait object or client-provided callback reaches the evaluation loop.
type ProgramConstraintAssessmentResultV1<Evaluation> = Result<
    HardDecision<
        <Evaluation as ProgramConstraintEvaluatorSetV1>::PassEvidence,
        <Evaluation as ProgramConstraintEvaluatorSetV1>::ViolationEvidence,
    >,
    ProgramPointAssessmentErrorV1<<Evaluation as ProgramConstraintEvaluatorSetV1>::Error>,
>;

pub(crate) trait ProgramConstraintEvaluatorSetV1: Sized {
    type Invocation: Copy;
    type PassEvidence;
    type ViolationEvidence;
    type Error;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> ProgramConstraintAssessmentResultV1<Self>;

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1;

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1;

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1;
}

impl<Evaluation> ProgramConstraintEvaluatorSetV1 for Evaluation
where
    Evaluation: ProgramPointEvaluatorV1,
    ProgramPointInvocation<Evaluation>: Copy,
{
    type Invocation = ProgramPointInvocation<Evaluation>;
    type PassEvidence = ProgramVisiblePointPassEvidence<Evaluation>;
    type ViolationEvidence = ProgramVisiblePointViolationEvidence<Evaluation>;
    type Error = <Evaluation as Evaluator<ProgramPointTargetV1>>::Error;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> ProgramConstraintAssessmentResultV1<Self> {
        assess_program_point_hard(point, self, invocation)
    }

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
        self.program_constraint_content_v1(invocation)
    }
}

#[cfg(test)]
thread_local! {
    /// Counts the concrete production evaluator dispatch itself, so certificate
    /// projection cannot accidentally recompute a verdict while still reusing
    /// the stored physical witness and declared context.
    pub(crate) static CORE_PROGRAM_ASSESSMENT_CALLS: core::cell::Cell<u64> =
        const { core::cell::Cell::new(0) };
}

/// Generates the code-owned heterogeneous evaluator set as parallel closed
/// unions. Each evidence variant retains the concrete evaluator's physical +
/// declared-context binding, identity, release, capability, invocation,
/// measurement, and classifier payload. Adding a family therefore requires a
/// Core code change in this single declaration, not a client-extensible
/// semantic registry.
macro_rules! define_core_program_evaluators_v1 {
    ($(
        $variant:ident {
            evaluator: $evaluator:ty = $evaluator_value:expr,
            invocation: $invocation:ty
        }
    ),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum CoreProgramConstraintInvocationV1 {
            $($variant($invocation)),+
        }

        pub(crate) enum CoreProgramPassEvidenceV1 {
            $($variant(ProgramVisiblePointPassEvidence<$evaluator>)),+
        }

        pub(crate) enum CoreProgramViolationEvidenceV1 {
            $($variant(ProgramVisiblePointViolationEvidence<$evaluator>)),+
        }

        #[derive(Debug, PartialEq)]
        pub(crate) enum CoreProgramEvaluatorErrorV1 {
            $($variant(<$evaluator as Evaluator<ProgramPointTargetV1>>::Error)),+
        }

        /// The sole production evaluator set for this Program schema version.
        /// Dispatch compiles to a direct match over the generated invocation
        /// tag; it performs neither virtual dispatch nor lookup allocation.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub(crate) struct CoreProgramEvaluatorsV1;

        impl ProgramConstraintEvaluatorSetV1 for CoreProgramEvaluatorsV1 {
            type Invocation = CoreProgramConstraintInvocationV1;
            type PassEvidence = CoreProgramPassEvidenceV1;
            type ViolationEvidence = CoreProgramViolationEvidenceV1;
            type Error = CoreProgramEvaluatorErrorV1;

            fn assess(
                &self,
                point: ProgramPointOccurrenceV1,
                invocation: Self::Invocation,
            ) -> ProgramConstraintAssessmentResultV1<Self> {
                #[cfg(test)]
                CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(calls.get() + 1));
                match invocation {
                    $(CoreProgramConstraintInvocationV1::$variant(invocation) => {
                        let evaluator: $evaluator = $evaluator_value;
                        match assess_program_point_hard(
                            point,
                            &evaluator,
                            invocation,
                        ) {
                            Ok(HardDecision::Pass(evidence)) => Ok(HardDecision::Pass(
                                CoreProgramPassEvidenceV1::$variant(evidence),
                            )),
                            Ok(HardDecision::Violation(evidence)) => Ok(HardDecision::Violation(
                                CoreProgramViolationEvidenceV1::$variant(evidence),
                            )),
                            Err(ProgramPointAssessmentErrorV1::Evaluator(source)) => Err(
                                ProgramPointAssessmentErrorV1::Evaluator(
                                    CoreProgramEvaluatorErrorV1::$variant(source),
                                ),
                            ),
                        }
                    }),+
                }
            }

            fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
                match evidence {
                    $(CoreProgramPassEvidenceV1::$variant(evidence) => *evidence.binding()),+
                }
            }

            fn violation_binding(
                evidence: &Self::ViolationEvidence,
            ) -> ProgramVisiblePointBindingV1 {
                match evidence {
                    $(CoreProgramViolationEvidenceV1::$variant(evidence) => *evidence.binding()),+
                }
            }

            fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
                match invocation {
                    $(CoreProgramConstraintInvocationV1::$variant(invocation) => {
                        let evaluator: $evaluator = $evaluator_value;
                        <$evaluator as ProgramPointEvaluatorContentV1>::program_constraint_content_v1(
                            &evaluator,
                            invocation,
                        )
                    }),+
                }
            }
        }
    };
}

define_core_program_evaluators_v1! {
    ExactSrgb8 {
        evaluator: ExactSrgb8IdentityV1 = ExactSrgb8IdentityV1,
        invocation: Srgb8
    },
    Wcag22Srgb8 {
        evaluator: Wcag22Srgb8V1 = Wcag22Srgb8V1,
        invocation: Wcag22CriterionV1
    },
}

type ProgramConstraintInvocationOf<Evaluation> =
    <Evaluation as ProgramConstraintEvaluatorSetV1>::Invocation;

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
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    sources: Vec<Source>,
    targets: Vec<Target>,
    joint_selection: Option<DeclaredJointSelectionV1>,
    observation_group: ObservationGroup,
    opacities: Vec<OpacityInput>,
    paints: Vec<Paint>,
    surfaces: Vec<Surface>,
    occurrences: Vec<Occurrence>,
    constraints: ConstraintSet<ProgramConstraintInvocationOf<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
}

impl<Evaluation> Program<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sources: Vec<Source>,
        targets: Vec<Target>,
        observation_group: ObservationGroup,
        opacities: Vec<OpacityInput>,
        paints: Vec<Paint>,
        surfaces: Vec<Surface>,
        occurrences: Vec<Occurrence>,
        constraints: ConstraintSet<ProgramConstraintInvocationOf<Evaluation>>,
        outputs: Vec<OutputBinding>,
        evaluator: Evaluation,
    ) -> Self {
        Self {
            sources,
            targets,
            joint_selection: None,
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

    /// Attach the complete explicit order for all finite Target domains.
    /// No order is synthesized from target IDs, candidate bytes, or
    /// declaration position.
    pub fn with_joint_selection(mut self, selection: DeclaredJointSelectionV1) -> Self {
        self.joint_selection = Some(selection);
        self
    }

    pub fn compile(self) -> Result<CompiledProgram<Evaluation>, ProgramCompileError> {
        prepare_program(self).map(|epoch| CompiledProgram {
            owner_generation: Rc::new(epoch),
        })
    }
}

/// Concrete monomorphized Program boundary for package/WASM lowering. The
/// generic form remains an internal test seam; package code binds only this
/// code-owned evaluator union.
pub(crate) type CoreProgramV1 = Program<CoreProgramEvaluatorsV1>;

/// Mutable cold-edge builder for the one concrete Core Program IR.
///
/// Every pushed value is already an actual Program declaration type. This
/// builder owns no transport tags, names, client taxonomy, or second graph;
/// compilation moves the concrete Program directly into the existing atomic
/// compiler.
pub(crate) struct CoreProgramDraftV1 {
    program: CoreProgramV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreProgramDraftErrorV1 {
    JointSelectionAlreadyDeclared,
}

impl CoreProgramDraftV1 {
    pub(crate) fn new() -> Self {
        Self {
            program: Program::new(
                Vec::new(),
                Vec::new(),
                ObservationGroup::new(ObservationGroupId::new(0), Vec::new()),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                ConstraintSet::new(Vec::new(), Vec::new()),
                Vec::new(),
                CoreProgramEvaluatorsV1,
            ),
        }
    }

    pub(crate) fn push_source(&mut self, source: Source) {
        self.program.sources.push(source);
    }

    pub(crate) fn push_target(&mut self, target: Target) {
        self.program.targets.push(target);
    }

    pub(crate) fn set_joint_selection(
        &mut self,
        selection: DeclaredJointSelectionV1,
    ) -> Result<(), CoreProgramDraftErrorV1> {
        if self.program.joint_selection.is_some() {
            return Err(CoreProgramDraftErrorV1::JointSelectionAlreadyDeclared);
        }
        self.program.joint_selection = Some(selection);
        Ok(())
    }

    pub(crate) fn push_surface_input_port(&mut self, input: SurfaceInputPortId) {
        self.program
            .observation_group
            .surface_input_ports
            .push(input);
    }

    pub(crate) fn push_opacity_input(&mut self, opacity: OpacityInput) {
        self.program.opacities.push(opacity);
    }

    pub(crate) fn push_paint(&mut self, paint: Paint) {
        self.program.paints.push(paint);
    }

    pub(crate) fn push_surface(&mut self, surface: Surface) {
        self.program.surfaces.push(surface);
    }

    pub(crate) fn push_occurrence(&mut self, occurrence: Occurrence) {
        self.program.occurrences.push(occurrence);
    }

    pub(crate) fn push_hard_constraint(
        &mut self,
        constraint: ConstraintInvocation<CoreProgramConstraintInvocationV1, HardModeV1>,
    ) {
        self.program.constraints.hard.push(constraint);
    }

    pub(crate) fn push_report_constraint(
        &mut self,
        constraint: ConstraintInvocation<CoreProgramConstraintInvocationV1, ReportModeV1>,
    ) {
        self.program.constraints.report_only.push(constraint);
    }

    pub(crate) fn push_output(&mut self, output: OutputBinding) {
        self.program.outputs.push(output);
    }

    pub(crate) fn compile(self) -> Result<CompiledCoreProgramV1, ProgramCompileError> {
        self.program.compile()
    }
}

/// Atomic compile failure. No executable partial graph escapes.
#[derive(Debug, PartialEq, Eq)]
pub enum ProgramCompileError {
    DuplicateSource {
        source: SourceId,
    },
    DuplicateTarget {
        target: TargetId,
    },
    MissingTargetSource {
        target: TargetId,
        source: SourceId,
    },
    DuplicateOpacityInput {
        input: OpacityInputId,
    },
    DuplicateSurfaceInputPort {
        input: SurfaceInputPortId,
    },
    UnusedSurfaceInputPort {
        input: SurfaceInputPortId,
    },
    DuplicateSurfaceInputBinding {
        input: SurfaceInputPortId,
        first: SurfaceId,
        duplicate: SurfaceId,
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
    MissingPaintTarget {
        paint: PaintId,
        target: TargetId,
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
    EmptyTargetDomain {
        target: TargetId,
    },
    DuplicateTargetCandidate {
        target: TargetId,
        candidate: TargetCandidateId,
    },
    DuplicateTargetCandidateSignal {
        target: TargetId,
        first: TargetCandidateId,
        duplicate: TargetCandidateId,
        signal: ColorSignal,
    },
    UnconstrainedTarget {
        target: TargetId,
    },
    DisconnectedFiniteTargets,
    UnassessedOutput {
        output: OutputSlotId,
        paint: PaintId,
    },
    MissingJointSelection,
    JointSelectionWithoutTargets,
    JointStateDuplicateTarget {
        state: usize,
        target: TargetId,
    },
    JointStateMissingTarget {
        state: usize,
        target: TargetId,
    },
    JointStateUnknownTarget {
        state: usize,
        target: TargetId,
    },
    JointStateUnknownCandidate {
        state: usize,
        target: TargetId,
        candidate: TargetCandidateId,
    },
    InvalidJointOrder(FiniteJointOrderErrorV1),
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

impl CompiledConstraintModeV1 {
    const fn rejects_candidate(self) -> bool {
        matches!(self, Self::Hard)
    }
}

struct CompiledPointConstraint<Invocation> {
    id: ConstraintId,
    target_id: OccurrenceId,
    target: CompiledOccurrenceSlotV1,
    occurrence_context_index: usize,
    mode: CompiledConstraintModeV1,
    invocation: Invocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledOutputBinding {
    output: OutputSlotId,
    paint_id: PaintId,
    paint: CompiledPaintSlotV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledOccurrenceContextV1 {
    occurrence: OccurrenceId,
    target: CompiledOccurrenceSlotV1,
    context: AppearanceContextId,
}

struct CompiledObservationGroupV1 {
    id: ObservationGroupId,
    schema: CanonicalObservationSchemaV1,
}

struct CompiledFiniteTargetV1 {
    binding: CompiledColorInputSlotV1,
    candidates: Box<[ColorSignal]>,
}

struct CompiledJointSelectionV1 {
    order: AdmittedFiniteJointOrderV1,
}

struct ProgramEpochV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    content_identity: ProgramContentIdentityV2,
    evaluator: Evaluation,
    graph: CompiledAppearanceGraph,
    binding_template: AdmittedAppearanceBindings,
    observation_group: CompiledObservationGroupV1,
    occurrence_contexts: Box<[CompiledOccurrenceContextV1]>,
    constraints: Box<[CompiledPointConstraint<ProgramConstraintInvocationOf<Evaluation>>]>,
    outputs: Box<[CompiledOutputBinding]>,
    finite_targets: Box<[CompiledFiniteTargetV1]>,
    joint_selection: Option<CompiledJointSelectionV1>,
}

/// Transaction-local strong pin for one exact compiled Program generation.
/// Construction is possible only by upgrading a Session plan's weak binding;
/// the contained epoch never becomes an independently shareable API.
pub(crate) struct ProgramOwnerLeaseV1<Evaluation>(Rc<ProgramEpochV1<Evaluation>>)
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy;

/// Fully validated immutable Program, not yet attached to runtime.
pub struct CompiledProgram<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    owner_generation: Rc<ProgramEpochV1<Evaluation>>,
}

pub(crate) type CompiledCoreProgramV1 = CompiledProgram<CoreProgramEvaluatorsV1>;

impl<Evaluation> CompiledProgram<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    pub fn observation_group_id(&self) -> ObservationGroupId {
        self.owner_generation.observation_group.id
    }

    /// Контентный адрес Program в границах схемы V2.
    ///
    /// Opaque ID и порядок неупорядоченных объявлений исключены; явный joint
    /// order входит в адрес. Адрес не подтверждает поколение владельца и не
    /// заменяет revision-bound evidence.
    pub fn content_identity(&self) -> ProgramContentIdentityV2 {
        self.owner_generation.content_identity
    }

    pub fn surface_input_ports(&self) -> &[SurfaceInputPortId] {
        self.owner_generation.observation_group.schema.as_slice()
    }

    pub fn constraint_ids(&self) -> impl ExactSizeIterator<Item = ConstraintId> + '_ {
        self.owner_generation
            .constraints
            .iter()
            .map(|constraint| constraint.id)
    }

    pub fn outputs(&self) -> impl ExactSizeIterator<Item = (OutputSlotId, PaintId)> + '_ {
        self.owner_generation
            .outputs
            .iter()
            .map(|output| (output.output, output.paint_id))
    }

    pub(crate) fn output_count(&self) -> usize {
        self.owner_generation.outputs.len()
    }

    pub(crate) fn evidence_cell_bounds(&self, scenario_count: usize) -> Option<(usize, usize)> {
        checked_program_epoch_evaluation_cell_counts(&self.owner_generation, scenario_count)
            .map(|counts| (counts.selected, counts.exhaustive_conflict))
    }

    pub(crate) fn output_slot_at(&self, index: usize) -> Option<OutputSlotId> {
        self.owner_generation
            .outputs
            .get(index)
            .map(|output| output.output)
    }

    #[cfg(test)]
    pub(crate) fn observation_schema_strong_count_for_test(&self) -> usize {
        self.owner_generation
            .observation_group
            .schema
            .strong_count_for_test()
    }

    /// Membership is the exact live owner allocation, never equivalent
    /// compiled content. The Session's `Weak` keeps the old control block
    /// address reserved until the Session itself is destroyed.
    pub(crate) fn owns_session(&self, session: &Session<ProgramSessionPlan<Evaluation>>) -> bool {
        core::ptr::eq(
            session.plan().owner_generation.as_ptr(),
            Rc::as_ptr(&self.owner_generation),
        )
    }

    /// Create one independent stream-affine Session for this exact compiled
    /// owner generation. Mutable bindings and workspace belong to the Session,
    /// while executable graph/evaluator state is reached only through a weak
    /// generation binding and expires when this owner is dropped or replaced.
    pub(crate) fn instantiate(
        &self,
        stream: ObservationStreamId,
    ) -> Result<Session<ProgramSessionPlan<Evaluation>>, ProgramSessionInstantiateError> {
        let bindings = self
            .owner_generation
            .binding_template
            .try_clone_v1()
            .map_err(map_session_instantiate_error)?;
        let workspace = self
            .owner_generation
            .graph
            .new_workspace()
            .map_err(map_session_instantiate_error)?;
        Ok(Session::new(
            stream,
            ProgramSessionPlan {
                owner_generation: Rc::downgrade(&self.owner_generation),
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
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    Pass(Evaluation::PassEvidence),
    Violation(Evaluation::ViolationEvidence),
}

impl<Evaluation> ProgramConstraintResultV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    pub const fn is_violation(&self) -> bool {
        matches!(self, Self::Violation(_))
    }

    fn binding(&self) -> ProgramVisiblePointBindingV1 {
        match self {
            Self::Pass(evidence) => Evaluation::pass_binding(evidence),
            Self::Violation(evidence) => Evaluation::violation_binding(evidence),
        }
    }
}

/// One canonical `physical case × constraint` report cell.
pub struct ProgramConstraintCellV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    candidate_state_index: usize,
    case_index: usize,
    constraint: ConstraintId,
    target: OccurrenceId,
    mode: CompiledConstraintModeV1,
    result: ProgramConstraintResultV1<Evaluation>,
}

impl<Evaluation> ProgramConstraintCellV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    pub const fn candidate_state_index(&self) -> usize {
        self.candidate_state_index
    }

    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    pub const fn constraint(&self) -> ConstraintId {
        self.constraint
    }

    pub const fn target(&self) -> OccurrenceId {
        self.target
    }

    pub fn appearance_context(&self) -> AppearanceContextId {
        self.result.binding().context()
    }

    pub const fn is_hard(&self) -> bool {
        self.mode.rejects_candidate()
    }

    pub const fn result(&self) -> &ProgramConstraintResultV1<Evaluation> {
        &self.result
    }
}

/// Полная оценка, привязанная к revision. Для selected/fixed результата ячейки
/// идут сначала по physical case, затем по constraint ID. Exhaustive conflict
/// дополнительно упорядочен сначала по joint state.
pub struct ProgramReportV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    content_identity: ProgramContentIdentityV2,
    observation: RevisionBoundObservationV1,
    cells: Vec<ProgramConstraintCellV1<Evaluation>>,
}

impl<Evaluation> ProgramReportV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    /// Адрес содержимого Program, по которому построен report; это не
    /// идентификатор поколения и не runtime-authority.
    pub const fn content_identity(&self) -> ProgramContentIdentityV2 {
        self.content_identity
    }

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

    pub const fn source_signal(self) -> ColorSignal {
        ColorSignal::from_srgb8(self.paint.source())
    }
}

/// All hard cells passed over the complete admitted physical support.
pub struct ProgramVerifiedV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    report: ProgramReportV1<Evaluation>,
    outputs: Vec<ProgramOutputV1>,
    selected_state_index: Option<usize>,
}

impl<Evaluation> session_private::EvidenceSealed for ProgramVerifiedV1<Evaluation> where
    Evaluation: ProgramConstraintEvaluatorSetV1
{
}

impl<Evaluation> SessionEvidenceV1 for ProgramVerifiedV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        self.report().observation()
    }
}

impl<Evaluation> ProgramVerifiedV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    pub const fn report(&self) -> &ProgramReportV1<Evaluation> {
        &self.report
    }

    pub fn outputs(&self) -> &[ProgramOutputV1] {
        &self.outputs
    }

    /// Index inside the authored total order. `None` means this Program has no
    /// finite targets and therefore performed validation only.
    pub const fn selected_state_index(&self) -> Option<usize> {
        self.selected_state_index
    }
}

/// Exhaustive hard-infeasibility report. Outputs are absent by construction
/// and therefore cannot be mistaken for committed Paints.
pub struct ProgramConflictV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    report: ProgramReportV1<Evaluation>,
    considered_state_count: usize,
}

impl<Evaluation> session_private::EvidenceSealed for ProgramConflictV1<Evaluation> where
    Evaluation: ProgramConstraintEvaluatorSetV1
{
}

impl<Evaluation> SessionEvidenceV1 for ProgramConflictV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        self.report().observation()
    }
}

impl<Evaluation> ProgramConflictV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    pub const fn report(&self) -> &ProgramReportV1<Evaluation> {
        &self.report
    }

    pub const fn considered_state_count(&self) -> usize {
        self.considered_state_count
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
        occurrence: OccurrenceId,
        context: AppearanceContextId,
        source: EvaluationError,
    },
    OutputVariesAcrossCases {
        output: OutputSlotId,
        first_case: usize,
        actual_case: usize,
    },
    FinalRecheckViolation {
        state_index: usize,
        case_index: usize,
        constraint: ConstraintId,
        target: OccurrenceId,
        hard_violation_count: usize,
    },
    InternalInvariant,
}

type ProgramEvaluatorError<Evaluation> = <Evaluation as ProgramConstraintEvaluatorSetV1>::Error;

type ProgramSessionEvaluationResult<Evaluation> = Result<
    SessionDecision<ProgramVerifiedV1<Evaluation>, ProgramConflictV1<Evaluation>>,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProgramEvaluationCellCountsV1 {
    selected: usize,
    exhaustive_conflict: usize,
}

fn checked_program_evaluation_cell_counts(
    physical_case_count: usize,
    constraint_count: usize,
    state_count: usize,
    can_conflict: bool,
) -> Option<ProgramEvaluationCellCountsV1> {
    let selected = physical_case_count.checked_mul(constraint_count)?;
    let exhaustive_conflict = if can_conflict {
        selected.checked_mul(state_count)?
    } else {
        0
    };
    Some(ProgramEvaluationCellCountsV1 {
        selected,
        exhaustive_conflict,
    })
}

fn checked_program_epoch_evaluation_cell_counts<Evaluation>(
    epoch: &ProgramEpochV1<Evaluation>,
    physical_case_count: usize,
) -> Option<ProgramEvaluationCellCountsV1>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let state_count = epoch
        .joint_selection
        .as_ref()
        .map(|selection| selection.order.state_count())
        // Without joint selection the epoch has one fixed configuration, so
        // the exhaustive-cell multiplier remains the multiplicative identity.
        .unwrap_or(1);
    let can_conflict = epoch
        .constraints
        .iter()
        .any(|constraint| constraint.mode.rejects_candidate());
    checked_program_evaluation_cell_counts(
        physical_case_count,
        epoch.constraints.len(),
        state_count,
        can_conflict,
    )
}

#[cfg(test)]
pub(crate) fn checked_program_evaluation_cell_counts_for_test(
    physical_case_count: usize,
    constraint_count: usize,
    state_count: usize,
) -> Option<(usize, usize)> {
    checked_program_evaluation_cell_counts(physical_case_count, constraint_count, state_count, true)
        .map(|counts| (counts.selected, counts.exhaustive_conflict))
}

#[cfg(test)]
std::thread_local! {
    static PROGRAM_PREFLIGHT_FAILURE_AT: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
    static PROGRAM_PREFLIGHT_FAILURE_ACTIVE: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
}

#[cfg(test)]
pub(crate) struct ProgramPreflightFailureGuardV1 {
    _not_send: PhantomData<Rc<()>>,
}

#[cfg(test)]
impl Drop for ProgramPreflightFailureGuardV1 {
    fn drop(&mut self) {
        PROGRAM_PREFLIGHT_FAILURE_AT.with(|failure| failure.set(None));
        PROGRAM_PREFLIGHT_FAILURE_ACTIVE.with(|active| active.set(false));
    }
}

#[cfg(test)]
pub(crate) fn fail_program_preflight_reservation_for_test(
    reservation_index: usize,
) -> ProgramPreflightFailureGuardV1 {
    PROGRAM_PREFLIGHT_FAILURE_ACTIVE.with(|active| {
        assert!(
            !active.replace(true),
            "a preflight failure is already armed"
        );
    });
    PROGRAM_PREFLIGHT_FAILURE_AT.with(|failure| failure.set(Some(reservation_index)));
    ProgramPreflightFailureGuardV1 {
        _not_send: PhantomData,
    }
}

#[cfg(test)]
fn injected_program_preflight_failure() -> bool {
    PROGRAM_PREFLIGHT_FAILURE_AT.with(|failure| match failure.get() {
        Some(0) => {
            failure.set(None);
            true
        }
        Some(remaining) => {
            failure.set(Some(remaining - 1));
            false
        }
        None => false,
    })
}

fn try_reserve_program_evaluation_buffer<T>(
    buffer: &mut Vec<T>,
    capacity: usize,
) -> Result<(), ()> {
    #[cfg(test)]
    if injected_program_preflight_failure() {
        return Err(());
    }
    buffer.try_reserve_exact(capacity).map_err(|_| ())
}

struct PreparedProgramEvaluationBuffersV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    selected_cells: Vec<ProgramConstraintCellV1<Evaluation>>,
    conflict_cells: Vec<ProgramConstraintCellV1<Evaluation>>,
    outputs: Vec<ProgramOutputV1>,
    counts: ProgramEvaluationCellCountsV1,
}

struct SelectedProgramEvaluationBuffersV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    cells: Vec<ProgramConstraintCellV1<Evaluation>>,
    outputs: Vec<ProgramOutputV1>,
    expected_cell_count: usize,
}

impl<Evaluation> PreparedProgramEvaluationBuffersV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn take_selected(&mut self) -> SelectedProgramEvaluationBuffersV1<Evaluation> {
        SelectedProgramEvaluationBuffersV1 {
            cells: std::mem::take(&mut self.selected_cells),
            outputs: std::mem::take(&mut self.outputs),
            expected_cell_count: self.counts.selected,
        }
    }
}

fn prepare_program_evaluation_buffers<Evaluation>(
    epoch: &ProgramEpochV1<Evaluation>,
    observation: &RevisionBoundObservationV1,
) -> Result<
    PreparedProgramEvaluationBuffersV1<Evaluation>,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let counts =
        checked_program_epoch_evaluation_cell_counts(epoch, observation.physical_case_count())
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;

    let mut selected_cells = Vec::new();
    try_reserve_program_evaluation_buffer(&mut selected_cells, counts.selected)
        .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut conflict_cells = Vec::new();
    if epoch.joint_selection.is_some() && counts.exhaustive_conflict != 0 {
        try_reserve_program_evaluation_buffer(&mut conflict_cells, counts.exhaustive_conflict)
            .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    }
    let mut outputs = Vec::new();
    try_reserve_program_evaluation_buffer(&mut outputs, epoch.outputs.len())
        .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;

    Ok(PreparedProgramEvaluationBuffersV1 {
        selected_cells,
        conflict_cells,
        outputs,
        counts,
    })
}

/// Per-Session mutable execution state bound weakly to one immutable compiled
/// owner generation. A transaction pins the generation before raw admission;
/// the Session itself cannot prolong the owner lifetime.
pub(crate) struct ProgramSessionPlan<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    owner_generation: Weak<ProgramEpochV1<Evaluation>>,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
}

impl<Evaluation> session_private::PlanSealed for ProgramSessionPlan<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
}

impl<Evaluation> SessionPlanV1 for ProgramSessionPlan<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    type OwnerLease = ProgramOwnerLeaseV1<Evaluation>;
    type Verified = ProgramVerifiedV1<Evaluation>;
    type Violation = ProgramConflictV1<Evaluation>;
    type Error = ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        self.owner_generation.upgrade().map(ProgramOwnerLeaseV1)
    }

    fn observation_schema<'a>(
        &'a self,
        owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        &owner.0.observation_group.schema
    }

    fn evaluate(
        &mut self,
        owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        evaluate_program_session(self, &owner.0, observation)
    }
}

fn evaluate_program_session<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: RevisionBoundObservationV1,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let Some(selection) = &epoch.joint_selection else {
        let mut buffers = prepare_program_evaluation_buffers(epoch, &observation)?;
        return collect_program_candidate_into(
            plan,
            epoch,
            observation,
            None,
            1,
            buffers.take_selected(),
        );
    };

    let state_count = selection.order.state_count();
    let mut buffers = prepare_program_evaluation_buffers(epoch, &observation)?;
    for (state_index, tuple) in selection.order.tuples().enumerate() {
        apply_joint_candidate(plan, &epoch.finite_targets, tuple)?;
        if !scan_program_candidate(plan, epoch, &observation, state_index, None, None)? {
            // A selected tuple is never certified from its allocation-free
            // search pass. Re-apply and collect fresh terminal evidence.
            apply_joint_candidate(plan, &epoch.finite_targets, tuple)?;
            match collect_program_candidate_into(
                plan,
                epoch,
                observation.clone(),
                Some(state_index),
                state_index + 1,
                buffers.take_selected(),
            )? {
                SessionDecision::Verified(verified) => {
                    return Ok(SessionDecision::Verified(verified));
                }
                SessionDecision::Violation(conflict) => {
                    let first = conflict
                        .report
                        .cells
                        .iter()
                        .find(|cell| cell.is_hard() && cell.result().is_violation())
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    let hard_violation_count = conflict
                        .report
                        .cells
                        .iter()
                        .filter(|cell| cell.is_hard() && cell.result().is_violation())
                        .count();
                    return Err(ProgramSessionEvaluationError::FinalRecheckViolation {
                        state_index,
                        case_index: first.case_index,
                        constraint: first.constraint,
                        target: first.target,
                        hard_violation_count,
                    });
                }
            }
        }
    }

    for (state_index, tuple) in selection.order.tuples().enumerate() {
        apply_joint_candidate(plan, &epoch.finite_targets, tuple)?;
        if !scan_program_candidate(
            plan,
            epoch,
            &observation,
            state_index,
            Some(&mut buffers.conflict_cells),
            None,
        )? {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
    }
    if buffers.conflict_cells.len() != buffers.counts.exhaustive_conflict {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }

    Ok(SessionDecision::Violation(ProgramConflictV1 {
        report: ProgramReportV1 {
            content_identity: epoch.content_identity,
            observation,
            cells: buffers.conflict_cells,
        },
        considered_state_count: state_count,
    }))
}

fn apply_joint_candidate<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    targets: &[CompiledFiniteTargetV1],
    tuple: &[FiniteDomainOrdinalV1],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    if targets.len() != tuple.len() {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    for (target, ordinal) in targets.iter().zip(tuple) {
        let candidate = target
            .candidates
            .get(ordinal.index())
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        plan.bindings
            .overwrite_color_at(target.binding, candidate.srgb8())
            .map_err(map_program_execution_binding_error)?;
    }
    Ok(())
}

fn collect_program_candidate_into<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: RevisionBoundObservationV1,
    selected_state_index: Option<usize>,
    considered_state_count: usize,
    buffers: SelectedProgramEvaluationBuffersV1<Evaluation>,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let SelectedProgramEvaluationBuffersV1 {
        mut cells,
        mut outputs,
        expected_cell_count,
    } = buffers;
    if !cells.is_empty()
        || cells.capacity() < expected_cell_count
        || !outputs.is_empty()
        || outputs.capacity() < epoch.outputs.len()
    {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    let candidate_state_index = selected_state_index.unwrap_or(0);
    let has_hard_violation = scan_program_candidate(
        plan,
        epoch,
        &observation,
        candidate_state_index,
        Some(&mut cells),
        Some(&mut outputs),
    )?;
    if cells.len() != expected_cell_count {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    let report = ProgramReportV1 {
        content_identity: epoch.content_identity,
        observation,
        cells,
    };
    if has_hard_violation {
        Ok(SessionDecision::Violation(ProgramConflictV1 {
            report,
            considered_state_count,
        }))
    } else {
        Ok(SessionDecision::Verified(ProgramVerifiedV1 {
            report,
            outputs,
            selected_state_index,
        }))
    }
}

fn scan_program_candidate<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: &RevisionBoundObservationV1,
    candidate_state_index: usize,
    mut cells: Option<&mut Vec<ProgramConstraintCellV1<Evaluation>>>,
    mut outputs: Option<&mut Vec<ProgramOutputV1>>,
) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let schema = &epoch.observation_group.schema;
    if !observation.shares_schema_backing_with(schema) {
        observation
            .validate_surface_schema(schema.as_slice())
            .map_err(ProgramSessionEvaluationError::ObservationSchemaMismatch)?;
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }

    let case_count = observation.physical_case_count();
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
                values[index].srgb8()
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
            if source.visible() != source.certificate().output_rgb() {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            let binding = epoch
                .occurrence_contexts
                .get(constraint.occurrence_context_index)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            if binding.occurrence != constraint.target_id || binding.target != constraint.target {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            let point = ProgramPointOccurrenceV1::from_resolved(source, binding.context);
            let decision = Evaluation::assess(&epoch.evaluator, point, constraint.invocation)
                .map_err(|error| match error {
                    ProgramPointAssessmentErrorV1::Evaluator(source) => {
                        ProgramSessionEvaluationError::Evaluator {
                            case_index,
                            constraint: constraint.id,
                            occurrence: constraint.target_id,
                            context: binding.context,
                            source,
                        }
                    }
                })?;
            let result = match decision {
                HardDecision::Pass(evidence) => ProgramConstraintResultV1::Pass(evidence),
                HardDecision::Violation(evidence) => {
                    if constraint.mode.rejects_candidate() {
                        has_hard_violation = true;
                    }
                    ProgramConstraintResultV1::Violation(evidence)
                }
            };
            debug_assert_eq!(result.binding().physical(), source.visible_point_binding());
            debug_assert_eq!(result.binding().context(), binding.context);
            if let Some(cells) = cells.as_deref_mut() {
                cells.push(ProgramConstraintCellV1 {
                    candidate_state_index,
                    case_index,
                    constraint: constraint.id,
                    target: constraint.target_id,
                    mode: constraint.mode,
                    result,
                });
            }
        }

        if let Some(outputs) = outputs.as_deref_mut() {
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
                    output_mismatch =
                        Some(ProgramSessionEvaluationError::OutputVariesAcrossCases {
                            output: output.output,
                            first_case: 0,
                            actual_case: case_index,
                        });
                }
            }
        }
    }

    if let Some(error) = output_mismatch {
        Err(error)
    } else {
        Ok(has_hard_violation)
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

fn validate_surface_input_bijection<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let mut bindings = Vec::new();
    bindings
        .try_reserve_exact(program.surfaces.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for surface in &program.surfaces {
        if let Surface::Input { id, input } = *surface {
            bindings.push((input, id));
        }
    }
    bindings.sort_unstable();

    for pair in bindings.windows(2) {
        if let [
            (first_input, first_surface),
            (duplicate_input, duplicate_surface),
        ] = pair
        {
            if first_input == duplicate_input {
                return Err(ProgramCompileError::DuplicateSurfaceInputBinding {
                    input: *first_input,
                    first: *first_surface,
                    duplicate: *duplicate_surface,
                });
            }
        }
    }

    for input in &program.observation_group.surface_input_ports {
        if bindings
            .binary_search_by_key(input, |(bound, _surface)| *bound)
            .is_err()
        {
            return Err(ProgramCompileError::UnusedSurfaceInputPort { input: *input });
        }
    }
    Ok(())
}

fn prepare_program<Evaluation>(
    mut program: Program<Evaluation>,
) -> Result<ProgramEpochV1<Evaluation>, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
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
    canonicalize_sources_and_targets(&mut program)?;

    let graph = lower_graph(&program)?
        .compile()
        .map_err(map_compile_error)?;
    validate_surface_input_bijection(&program)?;
    let binding_template = graph
        .admit_bindings(&lower_bindings(&program)?)
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

    validate_terminal_dependency_cone(&program)?;
    let (finite_targets, joint_selection) = compile_targets(
        &graph,
        &mut program.targets,
        program.joint_selection.as_mut(),
    )?;
    let all_occurrence_contexts = compile_occurrence_contexts(&graph, &program.occurrences)?;
    let mut constraints =
        compile_constraints::<Evaluation>(&graph, &all_occurrence_contexts, &program.constraints)?;
    let occurrence_contexts =
        compact_constraint_contexts(&all_occurrence_contexts, &mut constraints)?;
    let outputs = compile_outputs(&graph, &mut program.outputs)?;
    let content_identity = identity::compile_program_content_identity_v2(&program)?;
    Ok(ProgramEpochV1 {
        content_identity,
        evaluator: program.evaluator,
        graph,
        binding_template,
        observation_group: CompiledObservationGroupV1 {
            id: program.observation_group.id,
            schema: observation_schema,
        },
        occurrence_contexts,
        constraints,
        outputs,
        finite_targets,
        joint_selection,
    })
}

fn canonicalize_sources_and_targets<Evaluation>(
    program: &mut Program<Evaluation>,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    program.sources.sort_unstable_by_key(|source| source.id);
    if let Some(source) = program
        .sources
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
        .map(|pair| pair[0].id)
    {
        return Err(ProgramCompileError::DuplicateSource { source });
    }

    program.targets.sort_unstable_by_key(|target| target.id);
    if let Some(target) = program
        .targets
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
        .map(|pair| pair[0].id)
    {
        return Err(ProgramCompileError::DuplicateTarget { target });
    }
    for target in &program.targets {
        if program
            .sources
            .binary_search_by_key(&target.source, |source| source.id)
            .is_err()
        {
            return Err(ProgramCompileError::MissingTargetSource {
                target: target.id,
                source: target.source,
            });
        }
    }
    for paint in &program.paints {
        if let Paint::Solid { id, target } = *paint {
            if program
                .targets
                .binary_search_by_key(&target, |candidate| candidate.id)
                .is_err()
            {
                return Err(ProgramCompileError::MissingPaintTarget { paint: id, target });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum IndexedPaintDependencyV1 {
    Target(usize),
    Paint(usize),
}

#[derive(Debug, Clone, Copy)]
enum IndexedDependencyNodeV1 {
    Paint(usize),
    Surface(usize),
    Occurrence(usize),
}

struct IndexedProgramDependenciesV1 {
    paint_ids: Vec<(PaintId, usize)>,
    occurrence_ids: Vec<(OccurrenceId, usize)>,
    paint_dependencies: Vec<IndexedPaintDependencyV1>,
    surface_occurrences: Vec<Option<usize>>,
    occurrence_paints: Vec<usize>,
    occurrence_surfaces: Vec<usize>,
}

impl IndexedProgramDependenciesV1 {
    fn paint(&self, id: PaintId) -> Option<usize> {
        self.paint_ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| self.paint_ids[index].1)
    }

    fn occurrence(&self, id: OccurrenceId) -> Option<usize> {
        self.occurrence_ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| self.occurrence_ids[index].1)
    }
}

fn index_program_dependencies<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<IndexedProgramDependenciesV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let mut paint_ids = Vec::new();
    paint_ids
        .try_reserve_exact(program.paints.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    paint_ids.extend(program.paints.iter().enumerate().map(|(index, paint)| {
        let id = match *paint {
            Paint::Solid { id, .. } | Paint::Opacity { id, .. } => id,
        };
        (id, index)
    }));
    paint_ids.sort_unstable_by_key(|(id, _)| *id);

    let mut surface_ids = Vec::new();
    surface_ids
        .try_reserve_exact(program.surfaces.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    surface_ids.extend(program.surfaces.iter().enumerate().map(|(index, surface)| {
        let id = match *surface {
            Surface::Input { id, .. } | Surface::FromOccurrence { id, .. } => id,
        };
        (id, index)
    }));
    surface_ids.sort_unstable_by_key(|(id, _)| *id);

    let mut occurrence_ids = Vec::new();
    occurrence_ids
        .try_reserve_exact(program.occurrences.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    occurrence_ids.extend(
        program
            .occurrences
            .iter()
            .enumerate()
            .map(|(index, occurrence)| (occurrence.id, index)),
    );
    occurrence_ids.sort_unstable_by_key(|(id, _)| *id);

    let paint_ordinal = |id: PaintId| {
        paint_ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| paint_ids[index].1)
            .ok_or(ProgramCompileError::InternalInvariant)
    };
    let surface_ordinal = |id: SurfaceId| {
        surface_ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| surface_ids[index].1)
            .ok_or(ProgramCompileError::InternalInvariant)
    };
    let occurrence_ordinal = |id: OccurrenceId| {
        occurrence_ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| occurrence_ids[index].1)
            .ok_or(ProgramCompileError::InternalInvariant)
    };

    let mut paint_dependencies = Vec::new();
    paint_dependencies
        .try_reserve_exact(program.paints.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for paint in &program.paints {
        paint_dependencies.push(match *paint {
            Paint::Solid { target, .. } => IndexedPaintDependencyV1::Target(
                program
                    .targets
                    .binary_search_by_key(&target, |candidate| candidate.id)
                    .map_err(|_| ProgramCompileError::InternalInvariant)?,
            ),
            Paint::Opacity { source, .. } => {
                IndexedPaintDependencyV1::Paint(paint_ordinal(source)?)
            }
        });
    }
    let mut surface_occurrences = Vec::new();
    surface_occurrences
        .try_reserve_exact(program.surfaces.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for surface in &program.surfaces {
        surface_occurrences.push(match *surface {
            Surface::Input { .. } => None,
            Surface::FromOccurrence { occurrence, .. } => Some(occurrence_ordinal(occurrence)?),
        });
    }
    let mut occurrence_paints = Vec::new();
    let mut occurrence_surfaces = Vec::new();
    occurrence_paints
        .try_reserve_exact(program.occurrences.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    occurrence_surfaces
        .try_reserve_exact(program.occurrences.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for occurrence in &program.occurrences {
        occurrence_paints.push(paint_ordinal(occurrence.subject)?);
        occurrence_surfaces.push(surface_ordinal(occurrence.against)?);
    }
    Ok(IndexedProgramDependenciesV1 {
        paint_ids,
        occurrence_ids,
        paint_dependencies,
        surface_occurrences,
        occurrence_paints,
        occurrence_surfaces,
    })
}

struct ProgramDependencyScratchV1 {
    targets: Vec<bool>,
    paints: Vec<bool>,
    surfaces: Vec<bool>,
    occurrences: Vec<bool>,
    queue: Vec<IndexedDependencyNodeV1>,
}

impl ProgramDependencyScratchV1 {
    fn new<Evaluation>(program: &Program<Evaluation>) -> Result<Self, ProgramCompileError>
    where
        Evaluation: ProgramConstraintEvaluatorSetV1,
        ProgramConstraintInvocationOf<Evaluation>: Copy,
    {
        let node_count = program
            .paints
            .len()
            .checked_add(program.surfaces.len())
            .and_then(|count| count.checked_add(program.occurrences.len()))
            .ok_or(ProgramCompileError::ResourceExhausted)?;
        let mut queue = Vec::new();
        queue
            .try_reserve_exact(node_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        Ok(Self {
            targets: false_slots(program.targets.len())?,
            paints: false_slots(program.paints.len())?,
            surfaces: false_slots(program.surfaces.len())?,
            occurrences: false_slots(program.occurrences.len())?,
            queue,
        })
    }

    fn scan(
        &mut self,
        index: &IndexedProgramDependenciesV1,
        roots: impl IntoIterator<Item = OccurrenceId>,
    ) -> Result<(), ProgramCompileError> {
        self.targets.fill(false);
        self.paints.fill(false);
        self.surfaces.fill(false);
        self.occurrences.fill(false);
        self.queue.clear();
        for root in roots {
            let occurrence = index
                .occurrence(root)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if !self.occurrences[occurrence] {
                self.occurrences[occurrence] = true;
                self.queue
                    .push(IndexedDependencyNodeV1::Occurrence(occurrence));
            }
        }

        let mut cursor = 0usize;
        while let Some(node) = self.queue.get(cursor).copied() {
            cursor += 1;
            match node {
                IndexedDependencyNodeV1::Occurrence(occurrence) => {
                    let paint = index.occurrence_paints[occurrence];
                    if !self.paints[paint] {
                        self.paints[paint] = true;
                        self.queue.push(IndexedDependencyNodeV1::Paint(paint));
                    }
                    let surface = index.occurrence_surfaces[occurrence];
                    if !self.surfaces[surface] {
                        self.surfaces[surface] = true;
                        self.queue.push(IndexedDependencyNodeV1::Surface(surface));
                    }
                }
                IndexedDependencyNodeV1::Surface(surface) => {
                    if let Some(occurrence) = index.surface_occurrences[surface] {
                        if !self.occurrences[occurrence] {
                            self.occurrences[occurrence] = true;
                            self.queue
                                .push(IndexedDependencyNodeV1::Occurrence(occurrence));
                        }
                    }
                }
                IndexedDependencyNodeV1::Paint(paint) => match index.paint_dependencies[paint] {
                    IndexedPaintDependencyV1::Target(target) => self.targets[target] = true,
                    IndexedPaintDependencyV1::Paint(source) => {
                        if !self.paints[source] {
                            self.paints[source] = true;
                            self.queue.push(IndexedDependencyNodeV1::Paint(source));
                        }
                    }
                },
            }
        }
        Ok(())
    }
}

fn false_slots(len: usize) -> Result<Vec<bool>, ProgramCompileError> {
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(len)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    slots.resize(len, false);
    Ok(slots)
}

fn validate_terminal_dependency_cone<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    // Preserve the canonical missing-reference diagnostics owned by constraint
    // and output compilation before applying the stronger terminal-safety law.
    if program
        .constraints
        .hard
        .iter()
        .map(|constraint| constraint.target)
        .chain(
            program
                .constraints
                .report_only
                .iter()
                .map(|constraint| constraint.target),
        )
        .any(|target| {
            !program
                .occurrences
                .iter()
                .any(|occurrence| occurrence.id == target)
        })
        || program.outputs.iter().any(|output| {
            !program.paints.iter().any(|paint| match *paint {
                Paint::Solid { id, .. } | Paint::Opacity { id, .. } => id == output.paint,
            })
        })
    {
        return Ok(());
    }

    let index = index_program_dependencies(program)?;
    let mut scratch = ProgramDependencyScratchV1::new(program)?;
    scratch.scan(
        &index,
        program
            .constraints
            .hard
            .iter()
            .map(|constraint| constraint.target)
            .chain(
                program
                    .constraints
                    .report_only
                    .iter()
                    .map(|constraint| constraint.target),
            ),
    )?;
    for (target_index, target) in program.targets.iter().enumerate() {
        if matches!(&target.domain, TargetDomainV1::Finite(_)) && !scratch.targets[target_index] {
            return Err(ProgramCompileError::UnconstrainedTarget { target: target.id });
        }
    }
    for output in &program.outputs {
        let paint_index = index
            .paint(output.paint)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if !scratch.paints[paint_index] {
            return Err(ProgramCompileError::UnassessedOutput {
                output: output.output,
                paint: output.paint,
            });
        }
    }

    let finite_count = program
        .targets
        .iter()
        .filter(|target| matches!(&target.domain, TargetDomainV1::Finite(_)))
        .count();
    if finite_count > 1 {
        let mut has_common_assessment = false;
        for target in program
            .constraints
            .hard
            .iter()
            .map(|constraint| constraint.target)
            .chain(
                program
                    .constraints
                    .report_only
                    .iter()
                    .map(|constraint| constraint.target),
            )
        {
            scratch.scan(&index, [target])?;
            if program.targets.iter().enumerate().all(|(index, target)| {
                !matches!(&target.domain, TargetDomainV1::Finite(_)) || scratch.targets[index]
            }) {
                has_common_assessment = true;
                break;
            }
        }
        if !has_common_assessment {
            return Err(ProgramCompileError::DisconnectedFiniteTargets);
        }
    }
    Ok(())
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

fn compile_targets(
    graph: &CompiledAppearanceGraph,
    authored_targets: &mut [Target],
    authored_selection: Option<&mut DeclaredJointSelectionV1>,
) -> Result<
    (
        Box<[CompiledFiniteTargetV1]>,
        Option<CompiledJointSelectionV1>,
    ),
    ProgramCompileError,
> {
    struct CanonicalFiniteTargetV1<'a> {
        id: TargetId,
        binding: CompiledColorInputSlotV1,
        candidates: &'a [TargetCandidateV1],
    }

    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(authored_targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in authored_targets {
        let TargetDomainV1::Finite(candidates) = &mut target.domain else {
            continue;
        };
        if candidates.is_empty() {
            return Err(ProgramCompileError::EmptyTargetDomain { target: target.id });
        }
        let binding = graph
            .bind_color_input(target_color_input_id(target.id))
            .ok_or(ProgramCompileError::InternalInvariant)?;
        candidates.sort_unstable_by_key(|candidate| candidate.id);
        if let Some(candidate) = candidates
            .windows(2)
            .find(|pair| pair[0].id == pair[1].id)
            .map(|pair| pair[0].id)
        {
            return Err(ProgramCompileError::DuplicateTargetCandidate {
                target: target.id,
                candidate,
            });
        }
        let mut physical = Vec::new();
        physical
            .try_reserve_exact(candidates.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        physical.extend(
            candidates
                .iter()
                .map(|candidate| (candidate.signal, candidate.id)),
        );
        physical.sort_unstable();
        if let Some(pair) = physical.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(ProgramCompileError::DuplicateTargetCandidateSignal {
                target: target.id,
                first: pair[0].1,
                duplicate: pair[1].1,
                signal: pair[0].0,
            });
        }
        compiled.push(CanonicalFiniteTargetV1 {
            id: target.id,
            binding,
            candidates,
        });
    }

    if compiled.is_empty() {
        return match authored_selection {
            None => Ok((Box::new([]), None)),
            Some(_) => Err(ProgramCompileError::JointSelectionWithoutTargets),
        };
    }
    let Some(authored_selection) = authored_selection else {
        return Err(ProgramCompileError::MissingJointSelection);
    };

    let mut authored_tuples = Vec::new();
    authored_tuples
        .try_reserve_exact(authored_selection.states.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (state_index, authored_state) in authored_selection.states.iter_mut().enumerate() {
        authored_state
            .choices
            .sort_unstable_by_key(|choice| choice.target);
        if let Some(target) = authored_state
            .choices
            .windows(2)
            .find(|pair| pair[0].target == pair[1].target)
            .map(|pair| pair[0].target)
        {
            return Err(ProgramCompileError::JointStateDuplicateTarget {
                state: state_index,
                target,
            });
        }
        if let Some(choice) = authored_state.choices.iter().find(|choice| {
            compiled
                .binary_search_by_key(&choice.target, |target| target.id)
                .is_err()
        }) {
            return Err(ProgramCompileError::JointStateUnknownTarget {
                state: state_index,
                target: choice.target,
            });
        }

        let mut tuple = Vec::new();
        tuple
            .try_reserve_exact(compiled.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for target in &compiled {
            let choice_index = authored_state
                .choices
                .binary_search_by_key(&target.id, |choice| choice.target)
                .map_err(|_| ProgramCompileError::JointStateMissingTarget {
                    state: state_index,
                    target: target.id,
                })?;
            let choice = authored_state.choices[choice_index];
            let candidate_index = target
                .candidates
                .binary_search_by_key(&choice.candidate, |candidate| candidate.id)
                .map_err(|_| ProgramCompileError::JointStateUnknownCandidate {
                    state: state_index,
                    target: target.id,
                    candidate: choice.candidate,
                })?;
            tuple.push(FiniteDomainOrdinalV1::new(candidate_index));
        }
        authored_tuples.push(tuple);
    }

    let mut domain_lengths = Vec::new();
    domain_lengths
        .try_reserve_exact(compiled.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    domain_lengths.extend(compiled.iter().map(|target| target.candidates.len()));
    let order = admit_finite_joint_order_v1(&domain_lengths, authored_tuples)
        .map_err(ProgramCompileError::InvalidJointOrder)?;
    let mut runtime_targets = Vec::new();
    runtime_targets
        .try_reserve_exact(compiled.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in compiled {
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(target.candidates.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        candidates.extend(target.candidates.iter().map(|candidate| candidate.signal()));
        runtime_targets.push(CompiledFiniteTargetV1 {
            binding: target.binding,
            candidates: candidates.into_boxed_slice(),
        });
    }
    Ok((
        runtime_targets.into_boxed_slice(),
        Some(CompiledJointSelectionV1 { order }),
    ))
}

fn compile_occurrence_contexts(
    graph: &CompiledAppearanceGraph,
    authored: &[Occurrence],
) -> Result<Box<[CompiledOccurrenceContextV1]>, ProgramCompileError> {
    let mut contexts = Vec::new();
    contexts
        .try_reserve_exact(authored.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    contexts.extend(
        authored
            .iter()
            .map(|occurrence| (occurrence.id(), occurrence.context())),
    );
    contexts.sort_unstable_by_key(|(occurrence, _)| *occurrence);

    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(authored.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for occurrence in graph.occurrence_ids() {
        let index = contexts
            .binary_search_by_key(&occurrence, |(declared, _)| *declared)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        let target = graph
            .bind_occurrence(occurrence)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        compiled.push(CompiledOccurrenceContextV1 {
            occurrence,
            target,
            context: contexts[index].1,
        });
    }
    if compiled.len() != authored.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(compiled.into_boxed_slice())
}

fn compile_constraints<Evaluation>(
    graph: &CompiledAppearanceGraph,
    occurrence_contexts: &[CompiledOccurrenceContextV1],
    authored: &ConstraintSet<ProgramConstraintInvocationOf<Evaluation>>,
) -> Result<
    Box<[CompiledPointConstraint<ProgramConstraintInvocationOf<Evaluation>>]>,
    ProgramCompileError,
>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
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
    lowered.extend(authored.hard.iter().map(|constraint| LoweredConstraint {
        id: constraint.id,
        target: constraint.target,
        mode: CompiledConstraintModeV1::Hard,
        invocation: constraint.invocation,
    }));
    lowered.extend(
        authored
            .report_only
            .iter()
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
        let occurrence_context_index = occurrence_contexts
            .binary_search_by_key(&constraint.target, |binding| binding.occurrence)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        if occurrence_contexts[occurrence_context_index].target != target {
            return Err(ProgramCompileError::InternalInvariant);
        }
        compiled.push(CompiledPointConstraint {
            id: constraint.id,
            target_id: constraint.target,
            target,
            occurrence_context_index,
            mode: constraint.mode,
            invocation: constraint.invocation,
        });
    }
    Ok(compiled.into_boxed_slice())
}

fn compact_constraint_contexts<Invocation>(
    all: &[CompiledOccurrenceContextV1],
    constraints: &mut [CompiledPointConstraint<Invocation>],
) -> Result<Box<[CompiledOccurrenceContextV1]>, ProgramCompileError> {
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(constraints.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    targets.extend(constraints.iter().map(|constraint| constraint.target_id));
    targets.sort_unstable();
    targets.dedup();

    let mut compact = Vec::new();
    compact
        .try_reserve_exact(targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for occurrence in targets {
        let index = all
            .binary_search_by_key(&occurrence, |binding| binding.occurrence)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        compact.push(all[index]);
    }

    for constraint in constraints {
        let index = compact
            .binary_search_by_key(&constraint.target_id, |binding| binding.occurrence)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        if compact[index].target != constraint.target {
            return Err(ProgramCompileError::InternalInvariant);
        }
        constraint.occurrence_context_index = index;
    }
    Ok(compact.into_boxed_slice())
}

fn compile_outputs(
    graph: &CompiledAppearanceGraph,
    authored: &mut [OutputBinding],
) -> Result<Box<[CompiledOutputBinding]>, ProgramCompileError> {
    let len = authored.len();
    authored.sort_unstable_by_key(|output| output.output);
    if let Some(duplicate) = authored
        .windows(2)
        .find(|pair| pair[0].output == pair[1].output)
        .map(|pair| pair[0].output)
    {
        return Err(ProgramCompileError::DuplicateOutputSlot { output: duplicate });
    }
    for output in authored.iter() {
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
    for output in authored.iter().copied() {
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

const fn target_color_input_id(target: TargetId) -> ColorInputId {
    ColorInputId::new(target.value())
}

fn try_collect_program<T>(
    exact_len: usize,
    values: impl IntoIterator<Item = T>,
) -> Result<Vec<T>, ProgramCompileError> {
    let mut collected = Vec::new();
    collected
        .try_reserve_exact(exact_len)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    collected.extend(values);
    Ok(collected)
}

fn lower_graph<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<AppearanceGraphSpec, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let colors = try_collect_program(
        program.targets.len(),
        program
            .targets
            .iter()
            .map(|target| target_color_input_id(target.id)),
    )?;
    let surface_inputs = try_collect_program(
        program.observation_group.surface_input_ports.len(),
        program
            .observation_group
            .surface_input_ports
            .iter()
            .copied(),
    )?;
    let opacities = try_collect_program(
        program.opacities.len(),
        program.opacities.iter().map(|input| input.id),
    )?;
    let paints = try_collect_program(
        program.paints.len(),
        program.paints.iter().map(|paint| match *paint {
            Paint::Solid { id, target } => PaintSpec::Solid {
                id,
                color: target_color_input_id(target),
            },
            Paint::Opacity {
                id,
                source,
                opacity,
            } => PaintSpec::Opacity {
                id,
                source,
                opacity,
            },
        }),
    )?;
    let surfaces = try_collect_program(
        program.surfaces.len(),
        program.surfaces.iter().map(|surface| match *surface {
            Surface::Input { id, input } => SurfaceSpec::Input { id, port: input },
            Surface::FromOccurrence { id, occurrence } => {
                SurfaceSpec::FromOccurrence { id, occurrence }
            }
        }),
    )?;
    let occurrences = try_collect_program(
        program.occurrences.len(),
        program.occurrences.iter().map(|occurrence| OccurrenceSpec {
            id: occurrence.id,
            subject: occurrence.subject,
            against: occurrence.against,
            profile: match occurrence.composition {
                CompositionProfile::EncodedSrgb8SourceOverV1 => {
                    CompositionProfileV1::EncodedSrgb8SourceOverV1
                }
            },
        }),
    )?;
    Ok(AppearanceGraphSpec::new(
        colors,
        surface_inputs,
        opacities,
        paints,
        surfaces,
        occurrences,
    ))
}

fn lower_bindings<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<AppearanceBindings, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let mut colors = Vec::new();
    colors
        .try_reserve_exact(program.targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in &program.targets {
        let source_index = program
            .sources
            .binary_search_by_key(&target.source, |source| source.id)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        colors.push((
            target_color_input_id(target.id),
            program.sources[source_index].signal.srgb8(),
        ));
    }
    let surfaces = try_collect_program(
        program.observation_group.surface_input_ports.len(),
        program
            .observation_group
            .surface_input_ports
            .iter()
            .map(|input| (*input, Srgb8::new([0; 3]))),
    )?;
    let opacities = try_collect_program(
        program.opacities.len(),
        program
            .opacities
            .iter()
            .map(|input| (input.id, input.value)),
    )?;
    Ok(AppearanceBindings::new(colors, surfaces, opacities))
}

fn map_compile_error(error: CompileError) -> ProgramCompileError {
    match error {
        CompileError::DuplicateColorInput { .. } => ProgramCompileError::InternalInvariant,
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
        CompileError::MissingPaintColorInput { .. } => ProgramCompileError::InternalInvariant,
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
