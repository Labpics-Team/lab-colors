//! Private generic point Program compiler and lowering path.
//!
//! The authored graph has no client/UI role vocabulary. Paints are physical
//! source-plus-straight-alpha programs, occurrences are modeled applications of
//! Paint to Surface, constraints declare assessments of those exact
//! occurrences, and outputs bind opaque slots back to Paints. The compiled
//! result owns only admitted, canonical topology. Every finite candidate is
//! one atomic source-plus-straight-alpha Paint value, so selection cannot
//! synthesize an undeclared cross-product combination. Runtime observation and
//! lifecycle belong to the sole revision-bound Session. Attachment, renderer
//! and actual terminal sink are outside this module.
//! Finite candidate search executes only hard constraints. Every fresh hard
//! phase completes across its whole physical support (and across every state
//! of an exhaustive conflict) before diagnostics execute. Report cells are
//! then restored to canonical ID order, keeping evidence order separate from
//! selection authority. A selected finite state whose fresh hard recheck fails
//! exits before diagnostics, preserving the authoritative typed failure. A
//! diagnostic evaluator error may abort fixed or exhaustive report construction
//! only after their hard verdict is fixed; no partial certificate is emitted.
//! The verified report retains exact physical occurrence evidence plus the
//! declared appearance context; a routed Paint output contains only its opaque
//! slot and encoded Paint. A modeled LCS occurrence is derived only through its
//! separate typed capability; neither claim is renderer observation or
//! human-subject evidence.

use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::rc::{Rc, Weak};

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceEvaluationView, AppearanceGraphSpec,
    AppearanceWorkspace, BindingError, CompileError, CompiledAppearanceGraph,
    CompiledOccurrenceSlotV1, CompiledPaintInputSlotV1, CompiledPaintSlotV1,
    CompiledPointPresentationPathV1, EncodedPointPaintV1, EncodedPointPaintValueV1,
    ExactFinalOwnedPointDomainV1, OccurrenceId, OccurrenceSpec, OpacityInputId, PaintId,
    PaintInputId, PaintSpec, PointOccurrenceAbsenceReleaseV1, PointOccurrenceAbsenceStepV1,
    PointOccurrenceAbsenceSummaryV1, PointPresentationPathErrorV1, SurfaceId, SurfaceInputPortId,
    SurfaceSpec,
};
use crate::clean_set::{
    ClosedRejectedBlueIntervalV1, ExactNominalSrgb8CleanSetDecisionV1, ExactNominalSrgb8CleanSetV1,
};
use crate::composition::CompositionProfileV1;
use crate::constraints::{
    CoreIntrinsicUnaryInvocationV1, CoreIntrinsicUnaryMeasurementV1, CoreIntrinsicUnaryPassV1,
    CoreIntrinsicUnaryViolationV1, CoreRelationInvocationV1, CoreRelationMeasurementV1,
    CoreRelationPassV1, CoreRelationViolationV1, Evaluator, ExactSrgb8IdentityV1, HardDecision,
    ProgramConstraintContentV1, ProgramPointAssessmentErrorV1, ProgramPointEvaluatorContentV1,
    ProgramPointEvaluatorV1, ProgramPointInvocation, ProgramPointOccurrenceV1,
    ProgramPointTargetV1, ProgramVisiblePointBindingV1, ProgramVisiblePointPassEvidence,
    ProgramVisiblePointViolationEvidence, Wcag22Srgb8V1, assess_program_point_hard,
};
use crate::joint::{
    AdmittedFiniteJointOrderV1, FiniteDomainOrdinalV1, FiniteJointOrderAdmissionErrorV1,
    FiniteJointOrderErrorV1, NonEmptyFiniteDomainCardinalitiesV1, admit_finite_joint_order_v1,
};
use crate::lcs_occurrence::{AppearanceContextId, ColorSignal};
use crate::observation::{
    CanonicalObservationSchemaV1, NonEmptyScenarioSetV1, OBSERVATION_ARENA_SLOT_COUNT_V1,
    ObservationArenaSlotV1, ObservationError, ObservationGroupId, ObservationSchemaMismatchV1,
    ObservationStreamId, RevisionBoundObservationV1, canonicalize_observation_schema,
};
use crate::relation::DirectedRelationV1;
use crate::session::{
    Session, SessionDecision, SessionEvidenceV1, SessionObservationBindingPermitV1, SessionPlanV1,
    private as session_private,
};
use crate::wcag22::Wcag22CriterionV1;

#[path = "program_identity.rs"]
mod identity;
pub(crate) use identity::ProgramContentIdentityV6;
#[cfg(test)]
pub(crate) use identity::edge_role_count_for_test as program_identity_edge_role_count_for_test;
#[cfg(test)]
pub(crate) use identity::graph_schema_for_test as program_identity_graph_schema_for_test;

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

/// One atomic source-plus-straight-alpha value in a finite target domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCandidateV1 {
    id: TargetCandidateId,
    value: EncodedPointPaintValueV1,
}

impl TargetCandidateV1 {
    pub const fn new(id: TargetCandidateId, value: EncodedPointPaintValueV1) -> Self {
        Self { id, value }
    }

    pub const fn id(self) -> TargetCandidateId {
        self.id
    }

    pub const fn value(self) -> EncodedPointPaintValueV1 {
        self.value
    }
}

/// Admission failure for an authored finite Paint domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinitePaintDomainAdmissionErrorV1 {
    Empty,
}

/// A non-empty closed set of atomic Paint candidates.
///
/// Candidate order is authored data only until the declared joint order binds
/// it; an empty set is structurally impossible after admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinitePaintDomainV1(Vec<TargetCandidateV1>);

impl FinitePaintDomainV1 {
    pub fn try_new(
        candidates: Vec<TargetCandidateV1>,
    ) -> Result<Self, FinitePaintDomainAdmissionErrorV1> {
        if candidates.is_empty() {
            return Err(FinitePaintDomainAdmissionErrorV1::Empty);
        }
        Ok(Self(candidates))
    }

    pub fn candidates(&self) -> &[TargetCandidateV1] {
        &self.0
    }

    fn candidates_mut(&mut self) -> &mut [TargetCandidateV1] {
        &mut self.0
    }
}

/// Closed authored intent of one Target. Fixed targets reference immutable
/// source data; finite targets own all of their physical freedom directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetIntentV1 {
    FixedSource(SourceId),
    Finite(FinitePaintDomainV1),
}

/// A Paint-addressable target distinct from both source data and appearance
/// storage. Only finite targets participate in declared joint selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    id: TargetId,
    intent: TargetIntentV1,
}

impl Target {
    pub const fn new(id: TargetId, intent: TargetIntentV1) -> Self {
        Self { id, intent }
    }

    pub const fn fixed(id: TargetId, source: SourceId) -> Self {
        Self::new(id, TargetIntentV1::FixedSource(source))
    }

    pub const fn finite(id: TargetId, domain: FinitePaintDomainV1) -> Self {
        Self::new(id, TargetIntentV1::Finite(domain))
    }

    pub const fn id(&self) -> TargetId {
        self.id
    }

    pub const fn intent(&self) -> &TargetIntentV1 {
        &self.intent
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

/// Непрозрачный идентификатор одного корня моделируемого представления точки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationRootId(u32);

impl PresentationRootId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Объявление терминального корня моделируемого точечного графа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointPresentationRootV1 {
    id: PresentationRootId,
    terminal: OccurrenceId,
}

impl PointPresentationRootV1 {
    pub const fn new(id: PresentationRootId, terminal: OccurrenceId) -> Self {
        Self { id, terminal }
    }

    pub const fn id(self) -> PresentationRootId {
        self.id
    }

    pub const fn terminal(self) -> OccurrenceId {
        self.terminal
    }
}

/// Целевой `Occurrence`, для которого компилятор доказывает путь к объявленному
/// корню и фиксирует явную версию интервенции отсутствия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointPresentationTargetV1 {
    root: PresentationRootId,
    occurrence: OccurrenceId,
    absence_release: PointOccurrenceAbsenceReleaseV1,
}

impl PointPresentationTargetV1 {
    pub const fn new(root: PresentationRootId, occurrence: OccurrenceId) -> Self {
        Self {
            root,
            occurrence,
            absence_release: PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
        }
    }

    pub const fn root(self) -> PresentationRootId {
        self.root
    }

    pub const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    pub const fn absence_release(self) -> PointOccurrenceAbsenceReleaseV1 {
        self.absence_release
    }
}

/// Type-level marker for a mandatory constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardModeV1 {}

/// Type-level marker for a diagnostic-only constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportModeV1 {}

/// Атомарное тело одного ограничения над одним физически типизированным
/// объектом. Закрытая сумма не позволяет оценщику `Occurrence` и конвенции
/// `PointPresentation` притвориться взаимозаменяемыми либо хранить объект
/// ограничения отдельным полем.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProgramConstraintBodyV1<Invocation> {
    VisibleUnary {
        occurrence: OccurrenceId,
        invocation: Invocation,
    },
    IntrinsicUnary {
        target: TargetId,
        invocation: CoreIntrinsicUnaryInvocationV1,
    },
    IntrinsicRelation {
        relation: DirectedRelationV1<TargetId>,
        invocation: CoreRelationInvocationV1,
    },
    VisibleRelation {
        relation: DirectedRelationV1<OccurrenceId>,
        invocation: CoreRelationInvocationV1,
    },
    DeclaredSrgb8CleanSet {
        target: PointPresentationTargetV1,
    },
    #[cfg(test)]
    DeclaredSrgb8CleanSetFinalRecheckMutant {
        target: PointPresentationTargetV1,
    },
}

/// Одно атомарное типизированное ограничение над одним точным физическим объектом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintInvocation<Invocation, Mode> {
    id: ConstraintId,
    body: ProgramConstraintBodyV1<Invocation>,
    mode: PhantomData<fn() -> Mode>,
}

impl<Invocation> ConstraintInvocation<Invocation, HardModeV1> {
    pub const fn visible_unary_hard(
        id: ConstraintId,
        occurrence: OccurrenceId,
        invocation: Invocation,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::VisibleUnary {
                occurrence,
                invocation,
            },
            mode: PhantomData,
        }
    }

    pub(crate) const fn exact_intrinsic_unary_hard(
        id: ConstraintId,
        target: TargetId,
        expected: Srgb8,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::IntrinsicUnary {
                target,
                invocation: CoreIntrinsicUnaryInvocationV1::exact_srgb8(expected),
            },
            mode: PhantomData,
        }
    }

    pub(crate) const fn exact_intrinsic_relation_hard(
        id: ConstraintId,
        relation: DirectedRelationV1<TargetId>,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::IntrinsicRelation {
                relation,
                invocation: CoreRelationInvocationV1::exact_srgb8(),
            },
            mode: PhantomData,
        }
    }

    pub(crate) const fn exact_visible_relation_hard(
        id: ConstraintId,
        relation: DirectedRelationV1<OccurrenceId>,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::VisibleRelation {
                relation,
                invocation: CoreRelationInvocationV1::exact_srgb8(),
            },
            mode: PhantomData,
        }
    }

    pub(crate) const fn declared_srgb8_clean_set_hard(
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { target },
            mode: PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) fn declared_srgb8_clean_set_final_recheck_mutant(
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) -> Self {
        CLEAN_SET_FINAL_RECHECK_MUTANT_CALLS.with(|calls| calls.set(0));
        Self {
            id,
            body: ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { target },
            mode: PhantomData,
        }
    }
}

impl<Invocation> ConstraintInvocation<Invocation, ReportModeV1> {
    pub const fn visible_unary_report_only(
        id: ConstraintId,
        occurrence: OccurrenceId,
        invocation: Invocation,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::VisibleUnary {
                occurrence,
                invocation,
            },
            mode: PhantomData,
        }
    }

    pub(crate) const fn declared_srgb8_clean_set_report_only(
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) -> Self {
        Self {
            id,
            body: ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { target },
            mode: PhantomData,
        }
    }
}

impl<Invocation, Mode> ConstraintInvocation<Invocation, Mode> {
    pub const fn id(&self) -> ConstraintId {
        self.id
    }

    pub(crate) const fn body(&self) -> &ProgramConstraintBodyV1<Invocation> {
        &self.body
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
    presentation_roots: Vec<PointPresentationRootV1>,
    presentation_targets: Vec<PointPresentationTargetV1>,
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
            presentation_roots: Vec::new(),
            presentation_targets: Vec::new(),
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

    pub fn with_point_presentations(
        mut self,
        roots: Vec<PointPresentationRootV1>,
        targets: Vec<PointPresentationTargetV1>,
    ) -> Self {
        self.presentation_roots = roots;
        self.presentation_targets = targets;
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

    pub(crate) fn push_point_presentation_root(&mut self, root: PointPresentationRootV1) {
        self.program.presentation_roots.push(root);
    }

    pub(crate) fn push_point_presentation_target(&mut self, target: PointPresentationTargetV1) {
        self.program.presentation_targets.push(target);
    }

    pub(crate) fn push_hard_constraint(
        &mut self,
        constraint: ConstraintInvocation<CoreProgramConstraintInvocationV1, HardModeV1>,
    ) {
        self.program.constraints.hard.push(constraint);
    }

    pub(crate) fn push_exact_intrinsic_relation_hard(
        &mut self,
        id: ConstraintId,
        relation: DirectedRelationV1<TargetId>,
    ) {
        self.program
            .constraints
            .hard
            .push(ConstraintInvocation::exact_intrinsic_relation_hard(
                id, relation,
            ));
    }

    pub(crate) fn push_exact_intrinsic_unary_hard(
        &mut self,
        id: ConstraintId,
        target: TargetId,
        expected: Srgb8,
    ) {
        self.program
            .constraints
            .hard
            .push(ConstraintInvocation::exact_intrinsic_unary_hard(
                id, target, expected,
            ));
    }

    pub(crate) fn push_exact_visible_relation_hard(
        &mut self,
        id: ConstraintId,
        relation: DirectedRelationV1<OccurrenceId>,
    ) {
        self.program
            .constraints
            .hard
            .push(ConstraintInvocation::exact_visible_relation_hard(
                id, relation,
            ));
    }

    pub(crate) fn push_report_constraint(
        &mut self,
        constraint: ConstraintInvocation<CoreProgramConstraintInvocationV1, ReportModeV1>,
    ) {
        self.program.constraints.report_only.push(constraint);
    }

    pub(crate) fn push_declared_srgb8_clean_set_hard(
        &mut self,
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) {
        self.program
            .constraints
            .hard
            .push(ConstraintInvocation::declared_srgb8_clean_set_hard(
                id, target,
            ));
    }

    pub(crate) fn push_declared_srgb8_clean_set_report_only(
        &mut self,
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) {
        self.program.constraints.report_only.push(
            ConstraintInvocation::declared_srgb8_clean_set_report_only(id, target),
        );
    }

    #[cfg(test)]
    pub(crate) fn push_declared_srgb8_clean_set_final_recheck_mutant(
        &mut self,
        id: ConstraintId,
        target: PointPresentationTargetV1,
    ) {
        self.program
            .constraints
            .hard
            .push(ConstraintInvocation::declared_srgb8_clean_set_final_recheck_mutant(id, target));
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
    MissingFixedSource {
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
    DuplicatePresentationRoot {
        root: PresentationRootId,
    },
    MissingPresentationRootOccurrence {
        root: PresentationRootId,
        occurrence: OccurrenceId,
    },
    PresentationRootConsumedDownstream {
        root: PresentationRootId,
        occurrence: OccurrenceId,
    },
    UnusedPresentationRoot {
        root: PresentationRootId,
    },
    DuplicatePointPresentationTarget {
        root: PresentationRootId,
        occurrence: OccurrenceId,
    },
    MissingPointPresentationRoot {
        root: PresentationRootId,
    },
    MissingPointPresentationOccurrence {
        root: PresentationRootId,
        occurrence: OccurrenceId,
    },
    PointPresentationOccurrenceOutsideRootAncestry {
        root: PresentationRootId,
        terminal: OccurrenceId,
        occurrence: OccurrenceId,
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
    DuplicateTargetCandidate {
        target: TargetId,
        candidate: TargetCandidateId,
    },
    DuplicateTargetCandidateValue {
        target: TargetId,
        first: TargetCandidateId,
        duplicate: TargetCandidateId,
        value: EncodedPointPaintValueV1,
    },
    UnconstrainedFiniteTarget {
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
    MissingIntrinsicUnaryTarget {
        constraint: ConstraintId,
        target: TargetId,
    },
    MissingIntrinsicRelationReference {
        constraint: ConstraintId,
        reference: TargetId,
    },
    MissingIntrinsicRelationCandidate {
        constraint: ConstraintId,
        candidate: TargetId,
    },
    MissingVisibleRelationReference {
        constraint: ConstraintId,
        reference: OccurrenceId,
    },
    MissingVisibleRelationCandidate {
        constraint: ConstraintId,
        candidate: OccurrenceId,
    },
    SolverDependentIntrinsicRelationReference {
        constraint: ConstraintId,
        reference: TargetId,
    },
    SolverDependentVisibleRelationReference {
        constraint: ConstraintId,
        reference: OccurrenceId,
        target: TargetId,
    },
    MissingConstraintPresentationTarget {
        constraint: ConstraintId,
        root: PresentationRootId,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgramEvaluationPhaseV1 {
    Hard,
    ReportOnly,
}

impl ProgramEvaluationPhaseV1 {
    /// Phase separation prevents diagnostics from mutating evaluator state
    /// before any hard decision in the same authority scope is frozen.
    const fn includes(self, mode: CompiledConstraintModeV1) -> bool {
        match self {
            Self::Hard => mode.rejects_candidate(),
            Self::ReportOnly => !mode.rejects_candidate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledConstraintPhasesV1 {
    hard: bool,
    report_only: bool,
}

impl CompiledConstraintPhasesV1 {
    fn from_authored<Invocation>(constraints: &ConstraintSet<Invocation>) -> Self {
        Self {
            hard: !constraints.hard.is_empty(),
            report_only: !constraints.report_only.is_empty(),
        }
    }

    const fn contains(self, phase: ProgramEvaluationPhaseV1) -> bool {
        match phase {
            ProgramEvaluationPhaseV1::Hard => self.hard,
            ProgramEvaluationPhaseV1::ReportOnly => self.report_only,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeclaredSrgb8CleanSetV1 {
    classifier: ExactNominalSrgb8CleanSetV1,
    #[cfg(test)]
    final_recheck_mutant: bool,
}

impl DeclaredSrgb8CleanSetV1 {
    const fn package_pinned() -> Self {
        Self {
            classifier: ExactNominalSrgb8CleanSetV1,
            #[cfg(test)]
            final_recheck_mutant: false,
        }
    }

    #[cfg(test)]
    const fn final_recheck_mutant() -> Self {
        Self {
            classifier: ExactNominalSrgb8CleanSetV1,
            final_recheck_mutant: true,
        }
    }

    fn forces_absent_mutation(self) -> bool {
        #[cfg(test)]
        if self.final_recheck_mutant {
            return CLEAN_SET_FINAL_RECHECK_MUTANT_CALLS.with(|calls| {
                let previous = calls.get();
                calls.set(previous + 1);
                previous != 0
            });
        }
        false
    }
}

#[cfg(test)]
std::thread_local! {
    static CLEAN_SET_FINAL_RECHECK_MUTANT_CALLS: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledIntrinsicRelationEndpointV1 {
    target_id: TargetId,
    target: CompiledPaintInputSlotV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledVisibleRelationEndpointV1 {
    occurrence: OccurrenceId,
    slot: CompiledOccurrenceSlotV1,
    occurrence_context_index: usize,
}

enum CompiledProgramConstraintBodyV1<Invocation> {
    VisibleUnary {
        occurrence: OccurrenceId,
        slot: CompiledOccurrenceSlotV1,
        occurrence_context_index: usize,
        invocation: Invocation,
    },
    IntrinsicUnary {
        target_id: TargetId,
        target: CompiledPaintInputSlotV1,
        invocation: CoreIntrinsicUnaryInvocationV1,
    },
    IntrinsicRelation {
        reference: CompiledIntrinsicRelationEndpointV1,
        candidates: Box<[CompiledIntrinsicRelationEndpointV1]>,
        invocation: CoreRelationInvocationV1,
    },
    VisibleRelation {
        reference: CompiledVisibleRelationEndpointV1,
        candidates: Box<[CompiledVisibleRelationEndpointV1]>,
        invocation: CoreRelationInvocationV1,
    },
    PointPresentation {
        presentation_ordinal: usize,
        terminal: OccurrenceId,
        convention: DeclaredSrgb8CleanSetV1,
    },
}

struct CompiledPointConstraint<Invocation> {
    id: ConstraintId,
    mode: CompiledConstraintModeV1,
    body: CompiledProgramConstraintBodyV1<Invocation>,
}

fn compiled_relation_member_count<Invocation>(
    constraint: &CompiledPointConstraint<Invocation>,
) -> usize {
    match &constraint.body {
        CompiledProgramConstraintBodyV1::IntrinsicRelation { candidates, .. } => candidates.len(),
        CompiledProgramConstraintBodyV1::VisibleRelation { candidates, .. } => candidates.len(),
        CompiledProgramConstraintBodyV1::VisibleUnary { .. }
        | CompiledProgramConstraintBodyV1::IntrinsicUnary { .. }
        | CompiledProgramConstraintBodyV1::PointPresentation { .. } => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledOutputBinding {
    output: OutputSlotId,
    paint_id: PaintId,
    paint: CompiledPaintSlotV1,
}

/// Сминченная компилятором точная корреляция одного выхода Program и одной
/// моделируемой point-presentation цели.
///
/// Закрытые поля не дают подделать ordinal. Номинальные ID сохранены рядом,
/// чтобы hot path повторно проверял их без поиска. Само значение не доказывает
/// owner generation: enclosing owner обязан атомарно сминтить его и удержать
/// [`ProgramOwnerLeaseV1`] из той же [`CompiledProgram`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledPointOutputPresentationV1 {
    output_ordinal: usize,
    output: OutputSlotId,
    paint: PaintId,
    presentation_ordinal: usize,
    root: PresentationRootId,
    occurrence: OccurrenceId,
}

impl CompiledPointOutputPresentationV1 {
    pub(crate) const fn output_ordinal(self) -> usize {
        self.output_ordinal
    }

    pub(crate) const fn output(self) -> OutputSlotId {
        self.output
    }

    pub(crate) const fn paint(self) -> PaintId {
        self.paint
    }

    pub(crate) const fn presentation_ordinal(self) -> usize {
        self.presentation_ordinal
    }

    pub(crate) const fn root(self) -> PresentationRootId {
        self.root
    }

    pub(crate) const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }
}

/// Cold-ошибка binding до допуска point output в hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointOutputPresentationBindErrorV1 {
    /// Авторский output отсутствует в compiled output table.
    MissingOutput {
        /// Неизвестный output ID.
        output: OutputSlotId,
    },
    /// Авторская point-presentation цель отсутствует в compiled graph.
    MissingPresentationTarget {
        /// Root, в котором ожидалась цель.
        root: PresentationRootId,
        /// Occurrence, который должен быть доступен из root.
        occurrence: OccurrenceId,
    },
    /// Output и point-presentation цель ссылаются на разные Paint.
    SubjectPaintMismatch {
        /// Авторский output ID.
        output: OutputSlotId,
        /// Paint, связанный с output.
        output_paint: PaintId,
        /// Авторский presentation root.
        root: PresentationRootId,
        /// Авторский presentation occurrence.
        occurrence: OccurrenceId,
        /// Paint, физически представленный occurrence.
        subject_paint: PaintId,
    },
    /// Нарушен закрытый compiled-инвариант после успешной валидации Draft.
    InternalInvariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompiledOccurrenceContextV1 {
    occurrence: OccurrenceId,
    slot: CompiledOccurrenceSlotV1,
    context: AppearanceContextId,
}

fn coordinate_pair_matches<Left: PartialEq, Right: PartialEq>(
    expected_left: &Left,
    expected_right: &Right,
    actual_left: &Left,
    actual_right: &Right,
) -> bool {
    expected_left == actual_left && expected_right == actual_right
}

#[cfg(test)]
pub(crate) fn compiled_occurrence_coordinate_pair_matches_for_test<
    Left: PartialEq,
    Right: PartialEq,
>(
    expected_left: Left,
    expected_right: Right,
    actual_left: Left,
    actual_right: Right,
) -> bool {
    coordinate_pair_matches(&expected_left, &expected_right, &actual_left, &actual_right)
}

struct CompiledObservationGroupV1 {
    id: ObservationGroupId,
    schema: CanonicalObservationSchemaV1,
}

struct CompiledPointPresentationV1 {
    root: PresentationRootId,
    terminal: OccurrenceId,
    target: OccurrenceId,
    absence_release: PointOccurrenceAbsenceReleaseV1,
    path: CompiledPointPresentationPathV1,
}

struct CompiledPointPresentationsV1 {
    entries: Box<[CompiledPointPresentationV1]>,
    steps_per_case: usize,
}

impl CompiledPointPresentationsV1 {
    fn iter(&self) -> impl ExactSizeIterator<Item = &CompiledPointPresentationV1> {
        self.entries.iter()
    }

    const fn len(&self) -> usize {
        self.entries.len()
    }

    const fn steps_per_case(&self) -> usize {
        self.steps_per_case
    }
}

struct CompiledFiniteTargetV1 {
    binding: CompiledPaintInputSlotV1,
    candidates: Box<[EncodedPointPaintValueV1]>,
}

struct CompiledFiniteTargetsV1 {
    first: CompiledFiniteTargetV1,
    rest: Box<[CompiledFiniteTargetV1]>,
}

impl CompiledFiniteTargetsV1 {
    fn iter(&self) -> impl Iterator<Item = &CompiledFiniteTargetV1> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    fn len(&self) -> usize {
        self.rest.len() + 1
    }
}

mod admitted_compiled_joint_space {
    use super::*;

    pub(super) enum AdmissionErrorV1 {
        Authored(FiniteJointOrderErrorV1),
        ResourceExhausted,
        InternalInvariant,
    }

    pub(super) struct AdmittedCompiledJointSpaceV1 {
        targets: CompiledFiniteTargetsV1,
        order: AdmittedFiniteJointOrderV1,
    }

    pub(super) struct AdmittedCompiledJointStateV1<'space> {
        index: usize,
        targets: &'space CompiledFiniteTargetsV1,
        tuple: &'space [FiniteDomainOrdinalV1],
    }

    impl AdmittedCompiledJointSpaceV1 {
        pub(super) fn admit(
            targets: CompiledFiniteTargetsV1,
            authored: Vec<Vec<FiniteDomainOrdinalV1>>,
        ) -> Result<Self, AdmissionErrorV1> {
            let remaining_target_count = targets.len() - 1;
            let mut target_dimensions = targets.iter();
            let first = target_dimensions
                .next()
                .and_then(|target| NonZeroUsize::new(target.candidates.len()))
                .ok_or(AdmissionErrorV1::InternalInvariant)?;
            let mut rest = Vec::new();
            rest.try_reserve_exact(remaining_target_count)
                .map_err(|_| AdmissionErrorV1::ResourceExhausted)?;
            for target in target_dimensions {
                rest.push(
                    NonZeroUsize::new(target.candidates.len())
                        .ok_or(AdmissionErrorV1::InternalInvariant)?,
                );
            }
            let cardinalities =
                NonEmptyFiniteDomainCardinalitiesV1::new(first, rest.into_boxed_slice());
            let order = match admit_finite_joint_order_v1(&cardinalities, authored) {
                Ok(order) => order,
                Err(FiniteJointOrderAdmissionErrorV1::Authored(error)) => {
                    return Err(AdmissionErrorV1::Authored(error));
                }
                Err(FiniteJointOrderAdmissionErrorV1::ResourceExhausted) => {
                    return Err(AdmissionErrorV1::ResourceExhausted);
                }
                Err(FiniteJointOrderAdmissionErrorV1::InternalInvariant) => {
                    return Err(AdmissionErrorV1::InternalInvariant);
                }
            };
            Ok(Self { targets, order })
        }

        pub(super) fn state_count(&self) -> usize {
            self.order.state_count()
        }

        pub(super) fn states(&self) -> impl Iterator<Item = AdmittedCompiledJointStateV1<'_>> {
            self.order
                .tuples()
                .enumerate()
                .map(|(index, tuple)| AdmittedCompiledJointStateV1 {
                    index,
                    targets: &self.targets,
                    tuple,
                })
        }
    }

    impl AdmittedCompiledJointStateV1<'_> {
        pub(super) fn index(&self) -> usize {
            self.index
        }

        pub(super) fn assignments(
            &self,
        ) -> impl Iterator<Item = (&CompiledFiniteTargetV1, EncodedPointPaintValueV1)> {
            self.targets
                .iter()
                .zip(self.tuple)
                .map(|(target, ordinal)| {
                    let candidate = target.candidates[ordinal.index()];
                    (target, candidate)
                })
        }
    }
}

use admitted_compiled_joint_space::{
    AdmissionErrorV1 as CompiledJointSpaceAdmissionErrorV1, AdmittedCompiledJointSpaceV1,
    AdmittedCompiledJointStateV1,
};

enum CompiledTargetSelectionV1 {
    FixedOnly,
    Finite(AdmittedCompiledJointSpaceV1),
}

impl CompiledTargetSelectionV1 {
    fn state_count(&self) -> usize {
        match self {
            Self::FixedOnly => 1,
            Self::Finite(space) => space.state_count(),
        }
    }
}

struct ProgramEpochV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    content_identity: ProgramContentIdentityV6,
    evaluator: Evaluation,
    graph: CompiledAppearanceGraph,
    binding_template: AdmittedAppearanceBindings,
    observation_group: CompiledObservationGroupV1,
    occurrence_contexts: Box<[CompiledOccurrenceContextV1]>,
    constraints: Box<[CompiledPointConstraint<ProgramConstraintInvocationOf<Evaluation>>]>,
    constraint_phases: CompiledConstraintPhasesV1,
    point_presentations: CompiledPointPresentationsV1,
    outputs: Box<[CompiledOutputBinding]>,
    target_selection: CompiledTargetSelectionV1,
}

/// Strong pin одной точной compiled generation Program. Транзакция получает
/// его через upgrade слабой связи Session plan; enclosing cold owner также
/// может клонировать его прямо из [`CompiledProgram`]. Вложенная эпоха никогда
/// не становится независимо распространяемым API.
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

    /// Контентный адрес Program в границах текущей схемы identity.
    ///
    /// Opaque ID и порядок неупорядоченных объявлений исключены; явный joint
    /// order входит в адрес. Адрес не подтверждает поколение владельца и не
    /// заменяет revision-bound evidence.
    pub fn content_identity(&self) -> ProgramContentIdentityV6 {
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

    /// Минтит закрытую ordinal-backed корреляцию, только если Paint выхода
    /// точно совпадает с объявленным subject выбранной presentation target.
    pub(crate) fn bind_point_output_presentation(
        &self,
        output: OutputSlotId,
        root: PresentationRootId,
        occurrence: OccurrenceId,
    ) -> Result<CompiledPointOutputPresentationV1, PointOutputPresentationBindErrorV1> {
        let output_ordinal = self
            .owner_generation
            .outputs
            .binary_search_by_key(&output, |candidate| candidate.output)
            .map_err(|_| PointOutputPresentationBindErrorV1::MissingOutput { output })?;
        let compiled_output = &self.owner_generation.outputs[output_ordinal];

        let target = (root, occurrence);
        let presentation_ordinal = self
            .owner_generation
            .point_presentations
            .entries
            .binary_search_by_key(&target, |candidate| (candidate.root, candidate.target))
            .map_err(
                |_| PointOutputPresentationBindErrorV1::MissingPresentationTarget {
                    root,
                    occurrence,
                },
            )?;
        let subject_paint = self
            .owner_generation
            .graph
            .occurrence_subject(occurrence)
            .ok_or(PointOutputPresentationBindErrorV1::InternalInvariant)?;
        if compiled_output.paint_id != subject_paint {
            return Err(PointOutputPresentationBindErrorV1::SubjectPaintMismatch {
                output,
                output_paint: compiled_output.paint_id,
                root,
                occurrence,
                subject_paint,
            });
        }

        Ok(CompiledPointOutputPresentationV1 {
            output_ordinal,
            output,
            paint: compiled_output.paint_id,
            presentation_ordinal,
            root,
            occurrence,
        })
    }

    /// Удерживает точную owner generation независимо от `CompiledProgram`.
    pub(crate) fn pin_owner(&self) -> ProgramOwnerLeaseV1<Evaluation> {
        ProgramOwnerLeaseV1(Rc::clone(&self.owner_generation))
    }

    pub(crate) fn point_presentation_count(&self) -> usize {
        debug_assert!(
            self.owner_generation
                .point_presentations
                .entries
                .windows(2)
                .all(|pair| (pair[0].root, pair[0].target) < (pair[1].root, pair[1].target))
        );
        debug_assert!(
            self.owner_generation
                .point_presentations
                .entries
                .iter()
                .all(|presentation| {
                    presentation.path.belongs_to(&self.owner_generation.graph)
                        && presentation.path.root() == presentation.terminal
                        && presentation.path.target() == presentation.target
                        && presentation.path.len() != 0
                        && matches!(
                            presentation.absence_release,
                            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1
                        )
                })
        );
        self.owner_generation.point_presentations.len()
    }

    pub(crate) fn output_count(&self) -> usize {
        self.owner_generation.outputs.len()
    }

    pub(crate) fn evidence_cell_bounds(&self, scenario_count: usize) -> Option<(usize, usize)> {
        checked_program_epoch_evaluation_cell_counts(&self.owner_generation, scenario_count)
            .map(|counts| (counts.selected, counts.exhaustive_conflict))
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

    #[cfg(test)]
    pub(crate) fn point_resolution_count_for_test(
        &self,
        session: &Session<ProgramSessionPlan<Evaluation>>,
    ) -> Option<(usize, usize)> {
        self.owns_session(session)
            .then(|| session.plan().presentation_cache.replay_counts())
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
        let presentation_cache =
            ProgramPresentationCacheV1::try_new(&self.owner_generation.point_presentations)
                .map_err(|()| ProgramSessionInstantiateError::ResourceExhausted)?;
        Ok(Session::new(
            stream,
            ProgramSessionPlan {
                owner_generation: Rc::downgrade(&self.owner_generation),
                bindings,
                workspace,
                presentation_cache,
                evaluation_arenas: ProgramEvaluationArenaPoolV1::new(),
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

/// Физический объект одной ячейки ограничения. Варианты не дают цели
/// представления доступ к API контекста, предназначенному только для
/// `Occurrence`, и тем самым не подменяют финальный корень внутренним цветом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramConstraintSubjectV1 {
    VisibleUnary {
        occurrence: OccurrenceId,
        context: AppearanceContextId,
    },
    IntrinsicUnary {
        target: TargetId,
    },
    IntrinsicRelation {
        reference: TargetId,
    },
    VisibleRelation {
        reference: OccurrenceId,
        context: AppearanceContextId,
    },
    PointPresentation {
        target: PointPresentationTargetV1,
        terminal: OccurrenceId,
    },
}

/// Полная intrinsic-привязка Paint. Закон может читать только `source()`, но
/// для аудита сохраняется всё атомарное значение source+alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramIntrinsicPaintBindingV1 {
    target: TargetId,
    value: EncodedPointPaintValueV1,
}

impl ProgramIntrinsicPaintBindingV1 {
    pub(crate) const fn target(self) -> TargetId {
        self.target
    }

    pub(crate) const fn value(self) -> EncodedPointPaintValueV1 {
        self.value
    }
}

/// Полная final-visible привязка одного моделируемого физического сценария.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramVisibleRelationBindingV1 {
    occurrence: OccurrenceId,
    physical: ProgramVisiblePointBindingV1,
}

impl ProgramVisibleRelationBindingV1 {
    pub(crate) const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    pub(crate) const fn physical_ref(&self) -> &ProgramVisiblePointBindingV1 {
        &self.physical
    }
}

/// Вердикт участника хранится отдельно от сырого exact-измерения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramRelationMemberDecisionV1 {
    Pass(CoreRelationPassV1),
    Violation(CoreRelationViolationV1),
}

/// Одно однородное свидетельство reference→candidate в плоском report arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramRelationMemberEvidenceV1 {
    Intrinsic {
        reference: ProgramIntrinsicPaintBindingV1,
        candidate: ProgramIntrinsicPaintBindingV1,
        measurement: CoreRelationMeasurementV1,
        decision: ProgramRelationMemberDecisionV1,
    },
    Visible {
        reference: ProgramVisibleRelationBindingV1,
        candidate: ProgramVisibleRelationBindingV1,
        measurement: CoreRelationMeasurementV1,
        decision: ProgramRelationMemberDecisionV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramIntrinsicUnaryPassEvidenceV1 {
    binding: ProgramIntrinsicPaintBindingV1,
    measurement: CoreIntrinsicUnaryMeasurementV1,
    proof: CoreIntrinsicUnaryPassV1,
}

impl ProgramIntrinsicUnaryPassEvidenceV1 {
    pub(crate) const fn binding(self) -> ProgramIntrinsicPaintBindingV1 {
        self.binding
    }

    pub(crate) const fn measurement(self) -> CoreIntrinsicUnaryMeasurementV1 {
        self.measurement
    }

    pub(crate) const fn proof(self) -> CoreIntrinsicUnaryPassV1 {
        self.proof
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramIntrinsicUnaryViolationEvidenceV1 {
    binding: ProgramIntrinsicPaintBindingV1,
    measurement: CoreIntrinsicUnaryMeasurementV1,
    proof: CoreIntrinsicUnaryViolationV1,
}

impl ProgramIntrinsicUnaryViolationEvidenceV1 {
    pub(crate) const fn binding(self) -> ProgramIntrinsicPaintBindingV1 {
        self.binding
    }

    pub(crate) const fn measurement(self) -> CoreIntrinsicUnaryMeasurementV1 {
        self.measurement
    }

    pub(crate) const fn proof(self) -> CoreIntrinsicUnaryViolationV1 {
        self.proof
    }
}

impl ProgramRelationMemberEvidenceV1 {
    pub(crate) const fn measurement(self) -> CoreRelationMeasurementV1 {
        match self {
            Self::Intrinsic { measurement, .. } | Self::Visible { measurement, .. } => measurement,
        }
    }

    pub(crate) const fn decision(self) -> ProgramRelationMemberDecisionV1 {
        match self {
            Self::Intrinsic { decision, .. } | Self::Visible { decision, .. } => decision,
        }
    }

    pub(crate) const fn intrinsic_bindings(
        &self,
    ) -> Option<(
        &ProgramIntrinsicPaintBindingV1,
        &ProgramIntrinsicPaintBindingV1,
    )> {
        match self {
            Self::Intrinsic {
                reference,
                candidate,
                ..
            } => Some((reference, candidate)),
            Self::Visible { .. } => None,
        }
    }

    pub(crate) const fn visible_bindings(
        &self,
    ) -> Option<(
        &ProgramVisibleRelationBindingV1,
        &ProgramVisibleRelationBindingV1,
    )> {
        match self {
            Self::Visible {
                reference,
                candidate,
                ..
            } => Some((reference, candidate)),
            Self::Intrinsic { .. } => None,
        }
    }
}

/// Непустой диапазон в плоском relation-member arena отчёта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NonEmptyRelationMemberSpanV1 {
    start: usize,
    len: NonZeroUsize,
}

impl NonEmptyRelationMemberSpanV1 {
    fn from_start_and_len(start: usize, len: usize) -> Option<Self> {
        start.checked_add(len)?;
        Some(Self {
            start,
            len: NonZeroUsize::new(len)?,
        })
    }

    fn get(
        self,
        storage: &[ProgramRelationMemberEvidenceV1],
    ) -> Option<&[ProgramRelationMemberEvidenceV1]> {
        let end = self.start.checked_add(self.len.get())?;
        storage.get(self.start..end)
    }
}

/// Точное положительное свидетельство закреплённого пакетом clean-set над
/// непустым финальным доменом точки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeclaredSrgb8CleanSetPassV1 {
    visible: Srgb8,
}

impl DeclaredSrgb8CleanSetPassV1 {
    pub(crate) const fn visible(self) -> Srgb8 {
        self.visible
    }
}

/// Два взаимоисключающих способа нарушить предикат clean-set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclaredSrgb8CleanSetViolationV1 {
    FinalOwnedDomainAbsent,
    Rejected {
        visible: Srgb8,
        rejected_blue_interval: ClosedRejectedBlueIntervalV1,
    },
}

impl DeclaredSrgb8CleanSetViolationV1 {
    pub(crate) const fn visible(self) -> Option<Srgb8> {
        match self {
            Self::FinalOwnedDomainAbsent => None,
            Self::Rejected { visible, .. } => Some(visible),
        }
    }

    pub(crate) const fn rejected_blue_interval(self) -> Option<ClosedRejectedBlueIntervalV1> {
        match self {
            Self::FinalOwnedDomainAbsent => None,
            Self::Rejected {
                rejected_blue_interval,
                ..
            } => Some(rejected_blue_interval),
        }
    }
}

pub(crate) enum ProgramConstraintPassEvidenceV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    VisibleUnary(Evaluation::PassEvidence),
    IntrinsicUnary(ProgramIntrinsicUnaryPassEvidenceV1),
    Relation(NonEmptyRelationMemberSpanV1),
    DeclaredSrgb8CleanSet(DeclaredSrgb8CleanSetPassV1),
}

pub(crate) enum ProgramConstraintViolationEvidenceV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    VisibleUnary(Evaluation::ViolationEvidence),
    IntrinsicUnary(ProgramIntrinsicUnaryViolationEvidenceV1),
    Relation(NonEmptyRelationMemberSpanV1),
    DeclaredSrgb8CleanSet(DeclaredSrgb8CleanSetViolationV1),
}

/// One evaluator classification retained in the complete Program report.
pub enum ProgramConstraintResultV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    Pass(ProgramConstraintPassEvidenceV1<Evaluation>),
    Violation(ProgramConstraintViolationEvidenceV1<Evaluation>),
}

impl<Evaluation> ProgramConstraintResultV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    pub const fn is_violation(&self) -> bool {
        matches!(self, Self::Violation(_))
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
    subject: ProgramConstraintSubjectV1,
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

    pub(crate) const fn subject(&self) -> ProgramConstraintSubjectV1 {
        self.subject
    }

    pub const fn is_hard(&self) -> bool {
        self.mode.rejects_candidate()
    }

    pub const fn result(&self) -> &ProgramConstraintResultV1<Evaluation> {
        &self.result
    }
}

/// Непустой участок плоской истории пересчёта. Это диапазон индексов памяти,
/// не дискретизация непрерывной цветовой растяжки и не набор свотчей.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NonEmptyReplaySpanV1 {
    start: usize,
    len: NonZeroUsize,
}

impl NonEmptyReplaySpanV1 {
    fn from_bounds(start: usize, end: usize) -> Option<Self> {
        let len = NonZeroUsize::new(end.checked_sub(start)?)?;
        Some(Self { start, len })
    }

    fn get<T>(self, storage: &[T]) -> Option<&[T]> {
        let end = self.start.checked_add(self.len.get())?;
        storage.get(self.start..end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedPointPresentationV1 {
    domain: ExactFinalOwnedPointDomainV1,
    replay: Option<NonEmptyReplaySpanV1>,
}

/// Принадлежащий сессии временный буфер одной фазы. Каждая фаза начинает с
/// пустого кеша, поэтому поиск, отчёт и финальная перепроверка не наследуют
/// полномочия друг друга.
struct ProgramPresentationCacheV1 {
    domains: Vec<Option<ExactFinalOwnedPointDomainV1>>,
    scratch_steps: Vec<PointOccurrenceAbsenceStepV1>,
    #[cfg(test)]
    phase: ProgramEvaluationPhaseV1,
    #[cfg(test)]
    replay_counts: [usize; 2],
}

impl ProgramPresentationCacheV1 {
    fn try_new(presentations: &CompiledPointPresentationsV1) -> Result<Self, ()> {
        let mut domains = Vec::new();
        domains
            .try_reserve_exact(presentations.len())
            .map_err(|_| ())?;
        domains.resize(presentations.len(), None);
        let mut scratch_steps = Vec::new();
        scratch_steps
            .try_reserve_exact(presentations.steps_per_case())
            .map_err(|_| ())?;
        Ok(Self {
            domains,
            scratch_steps,
            #[cfg(test)]
            phase: ProgramEvaluationPhaseV1::Hard,
            #[cfg(test)]
            replay_counts: [0; 2],
        })
    }

    fn begin_case(&mut self, _phase: ProgramEvaluationPhaseV1) {
        self.domains.fill(None);
        self.scratch_steps.clear();
        #[cfg(test)]
        {
            self.phase = _phase;
        }
    }

    fn resolve(
        &mut self,
        evaluation: &AppearanceEvaluationView<'_, '_>,
        presentation_ordinal: usize,
        presentation: &CompiledPointPresentationV1,
        destination: Option<&mut Vec<PointOccurrenceAbsenceStepV1>>,
    ) -> Result<ResolvedPointPresentationV1, ()> {
        let cached = *self.domains.get(presentation_ordinal).ok_or(())?;
        if let Some(domain) = cached {
            return Ok(ResolvedPointPresentationV1 {
                domain,
                replay: None,
            });
        }

        let steps = destination.unwrap_or(&mut self.scratch_steps);
        if steps.capacity().saturating_sub(steps.len()) < presentation.path.len() {
            return Err(());
        }
        let start = steps.len();
        let replay = evaluation
            .replay_point_occurrence_absence_into(
                &presentation.path,
                presentation.absence_release,
                steps,
            )
            .map_err(|_| ())?;
        if replay.release() != presentation.absence_release
            || replay.target() != presentation.target
            || replay.root() != presentation.terminal
            || replay.steps().len() != presentation.path.len()
        {
            return Err(());
        }
        let domain = replay.domain();
        let end = start.checked_add(replay.steps().len()).ok_or(())?;
        let span = NonEmptyReplaySpanV1::from_bounds(start, end).ok_or(())?;
        self.domains[presentation_ordinal] = Some(domain);
        #[cfg(test)]
        {
            let phase_index = match self.phase {
                ProgramEvaluationPhaseV1::Hard => 0,
                ProgramEvaluationPhaseV1::ReportOnly => 1,
            };
            self.replay_counts[phase_index] += 1;
        }
        Ok(ResolvedPointPresentationV1 {
            domain,
            replay: Some(span),
        })
    }

    #[cfg(test)]
    const fn replay_counts(&self) -> (usize, usize) {
        (self.replay_counts[0], self.replay_counts[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProgramPointCausalRecordV1 {
    /// Только exhaustive conflict хранит рассмотренное состояние в строке.
    /// Selected/fixed authority берётся у владеющего типизированного отчёта.
    considered_state_index: Option<usize>,
    case_index: usize,
    presentation_root: PresentationRootId,
    release: PointOccurrenceAbsenceReleaseV1,
    replay: NonEmptyReplaySpanV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramPointCausalSelectedStateV1 {
    Fixed,
    Selected(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramPointCausalConsideredStateV1 {
    Fixed,
    Considered(usize),
}

/// Заимствованное revision-bound свидетельство о вкладе одной точки в байты
/// одного моделируемого terminal root. Оно ничего не утверждает о пикселе
/// браузера, восприятии или качестве цвета.
pub(crate) struct ProgramPointCausalEvidenceV1<'report, State> {
    content_identity: ProgramContentIdentityV6,
    observation: &'report RevisionBoundObservationV1,
    record: &'report ProgramPointCausalRecordV1,
    steps: &'report [PointOccurrenceAbsenceStepV1],
    state: State,
}

pub(crate) type ProgramPointCausalCertificateV1<'report> =
    ProgramPointCausalEvidenceV1<'report, ProgramPointCausalSelectedStateV1>;
pub(crate) type ProgramConsideredPointCausalEvidenceV1<'report> =
    ProgramPointCausalEvidenceV1<'report, ProgramPointCausalConsideredStateV1>;

impl<State> ProgramPointCausalEvidenceV1<'_, State>
where
    State: Copy,
{
    fn summary(&self) -> PointOccurrenceAbsenceSummaryV1 {
        PointOccurrenceAbsenceSummaryV1::from_nonempty_steps(self.steps)
            .unwrap_or_else(|| unreachable!("тип replay span запрещает пустой пересчёт"))
    }

    pub(crate) const fn content_identity(&self) -> ProgramContentIdentityV6 {
        self.content_identity
    }

    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        self.observation
    }

    pub(crate) const fn state(&self) -> State {
        self.state
    }

    pub(crate) const fn case_index(&self) -> usize {
        self.record.case_index
    }

    pub(crate) const fn presentation_root(&self) -> PresentationRootId {
        self.record.presentation_root
    }

    pub(crate) const fn release(&self) -> PointOccurrenceAbsenceReleaseV1 {
        self.record.release
    }

    pub(crate) fn target(&self) -> OccurrenceId {
        self.summary().target()
    }

    pub(crate) fn modeled_terminal_occurrence(&self) -> OccurrenceId {
        self.summary().root()
    }

    pub(crate) fn modeled_terminal_codes(&self) -> [u8; 3] {
        self.summary().normal_root()
    }

    pub(crate) fn modeled_terminal_without_target_codes(&self) -> [u8; 3] {
        self.summary().counterfactual_root()
    }

    pub(crate) fn domain(&self) -> ExactFinalOwnedPointDomainV1 {
        self.summary().domain()
    }

    pub(crate) fn steps(&self) -> &[PointOccurrenceAbsenceStepV1] {
        self.steps
    }
}

/// Полная оценка, привязанная к revision. Для выбранного или фиксированного
/// результата ячейки идут сначала по physical case, затем по constraint ID.
/// Исчерпывающий конфликт дополнительно упорядочен сначала по joint state.
/// Проекция причинного replay сохраняет порядок построения
/// `state × physical case × (root, target)` без сортировки.
pub struct ProgramReportV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    content_identity: ProgramContentIdentityV6,
    observation: RevisionBoundObservationV1,
    arena: ProgramEvaluationArenaLeaseV1<Evaluation>,
}

impl<Evaluation> ProgramReportV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    /// Адрес содержимого Program, по которому построен report; это не
    /// идентификатор поколения и не runtime-authority.
    pub const fn content_identity(&self) -> ProgramContentIdentityV6 {
        self.content_identity
    }

    pub const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub fn cells(&self) -> &[ProgramConstraintCellV1<Evaluation>] {
        &self.arena.storage.cells
    }

    pub(crate) fn relation_members_for(
        &self,
        span: NonEmptyRelationMemberSpanV1,
    ) -> Option<&[ProgramRelationMemberEvidenceV1]> {
        span.get(&self.arena.storage.relation_members)
    }

    #[cfg(test)]
    pub(crate) fn storage_capacities_for_test(&self) -> [usize; 3] {
        [
            self.arena.storage.cells.capacity(),
            self.arena.storage.point_causal_records.capacity(),
            self.arena.storage.point_causal_steps.capacity(),
        ]
    }

    fn into_arena(self) -> ProgramEvaluationArenaReturnV1<Evaluation> {
        let Self {
            content_identity: _,
            observation,
            arena,
        } = self;
        let slot = observation.arena_slot();
        drop(observation);
        ProgramEvaluationArenaReturnV1 {
            slot,
            storage: arena.storage,
        }
    }
}

/// Один encoded Paint из Program, направленный в непрозрачный клиентский slot.
///
/// Это выбранный source с opacity до клиентского sink, attachment, renderer
/// и final-visible композиции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramPaintOutputV1 {
    output: OutputSlotId,
    paint: EncodedPointPaintV1,
}

impl ProgramPaintOutputV1 {
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

    pub fn outputs(&self) -> &[ProgramPaintOutputV1] {
        &self.report.arena.storage.outputs
    }

    /// Index inside the authored total order. `None` means this Program has no
    /// finite targets and therefore performed validation only.
    pub const fn selected_state_index(&self) -> Option<usize> {
        self.selected_state_index
    }

    pub(crate) fn point_causal_certificates(
        &self,
    ) -> impl ExactSizeIterator<Item = ProgramPointCausalCertificateV1<'_>> + '_ {
        let state = self.selected_state_index.map_or(
            ProgramPointCausalSelectedStateV1::Fixed,
            ProgramPointCausalSelectedStateV1::Selected,
        );
        self.report
            .arena
            .storage
            .point_causal_records
            .iter()
            .map(move |record| {
                debug_assert!(record.considered_state_index.is_none());
                let steps = record
                    .replay
                    .get(&self.report.arena.storage.point_causal_steps)
                    .unwrap_or_else(|| unreachable!("report владеет каноническим replay span"));
                ProgramPointCausalEvidenceV1 {
                    content_identity: self.report.content_identity,
                    observation: &self.report.observation,
                    record,
                    steps,
                    state,
                }
            })
    }

    fn into_arena(self) -> ProgramEvaluationArenaReturnV1<Evaluation> {
        self.report.into_arena()
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

    #[cfg(test)]
    pub(crate) fn retained_output_value_count_for_test(&self) -> usize {
        self.report.arena.storage.outputs.len()
    }

    pub(crate) fn considered_point_causal_evidence(
        &self,
    ) -> impl ExactSizeIterator<Item = ProgramConsideredPointCausalEvidenceV1<'_>> + '_ {
        self.report
            .arena
            .storage
            .point_causal_records
            .iter()
            .map(move |record| {
                let state = record.considered_state_index.map_or(
                    ProgramPointCausalConsideredStateV1::Fixed,
                    ProgramPointCausalConsideredStateV1::Considered,
                );
                let steps = record
                    .replay
                    .get(&self.report.arena.storage.point_causal_steps)
                    .unwrap_or_else(|| unreachable!("report владеет каноническим replay span"));
                ProgramPointCausalEvidenceV1 {
                    content_identity: self.report.content_identity,
                    observation: &self.report.observation,
                    record,
                    steps,
                    state,
                }
            })
    }

    fn into_arena(self) -> ProgramEvaluationArenaReturnV1<Evaluation> {
        self.report.into_arena()
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
        subject: ProgramConstraintSubjectV1,
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
    let state_count = epoch.target_selection.state_count();
    let can_conflict = epoch
        .constraint_phases
        .contains(ProgramEvaluationPhaseV1::Hard);
    checked_program_evaluation_cell_counts(
        physical_case_count,
        epoch.constraints.len(),
        state_count,
        can_conflict,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProgramEvaluationCardinalityV1 {
    selected: usize,
    exhaustive_conflict: usize,
    selected_relation_members: usize,
    exhaustive_relation_members: usize,
    selected_point_records: usize,
    exhaustive_point_records: usize,
    selected_replay_steps: usize,
    exhaustive_replay_steps: usize,
}

fn checked_program_evaluation_cardinality(
    physical_case_count: usize,
    constraint_count: usize,
    point_presentation_count: usize,
    replay_steps_per_case: usize,
    relation_members_per_case: usize,
    state_count: usize,
    can_conflict: bool,
) -> Option<ProgramEvaluationCardinalityV1> {
    let cells = checked_program_evaluation_cell_counts(
        physical_case_count,
        constraint_count,
        state_count,
        can_conflict,
    )?;
    let selected_point_records = physical_case_count.checked_mul(point_presentation_count)?;
    let selected_relation_members = physical_case_count.checked_mul(relation_members_per_case)?;
    let selected_replay_steps = physical_case_count.checked_mul(replay_steps_per_case)?;
    let exhaustive_point_records = if can_conflict {
        selected_point_records.checked_mul(state_count)?
    } else {
        0
    };
    let exhaustive_relation_members = if can_conflict {
        selected_relation_members.checked_mul(state_count)?
    } else {
        0
    };
    let exhaustive_replay_steps = if can_conflict {
        selected_replay_steps.checked_mul(state_count)?
    } else {
        0
    };
    Some(ProgramEvaluationCardinalityV1 {
        selected: cells.selected,
        exhaustive_conflict: cells.exhaustive_conflict,
        selected_relation_members,
        exhaustive_relation_members,
        selected_point_records,
        exhaustive_point_records,
        selected_replay_steps,
        exhaustive_replay_steps,
    })
}

fn checked_program_epoch_evaluation_cardinality<Evaluation>(
    epoch: &ProgramEpochV1<Evaluation>,
    physical_case_count: usize,
) -> Option<ProgramEvaluationCardinalityV1>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let state_count = epoch.target_selection.state_count();
    let can_conflict = epoch
        .constraint_phases
        .contains(ProgramEvaluationPhaseV1::Hard);
    checked_program_evaluation_cardinality(
        physical_case_count,
        epoch.constraints.len(),
        epoch.point_presentations.len(),
        epoch.point_presentations.steps_per_case(),
        epoch
            .constraints
            .iter()
            .map(compiled_relation_member_count)
            .try_fold(0_usize, usize::checked_add)?,
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
pub(crate) fn checked_program_point_causal_cardinality_for_test(
    physical_case_count: usize,
    constraint_count: usize,
    point_presentation_count: usize,
    replay_steps_per_case: usize,
    state_count: usize,
    can_conflict: bool,
) -> Option<(usize, usize, usize, usize)> {
    checked_program_evaluation_cardinality(
        physical_case_count,
        constraint_count,
        point_presentation_count,
        replay_steps_per_case,
        0,
        state_count,
        can_conflict,
    )
    .map(|cardinality| {
        (
            cardinality.selected_point_records,
            cardinality.exhaustive_point_records,
            cardinality.selected_replay_steps,
            cardinality.exhaustive_replay_steps,
        )
    })
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
pub(crate) fn program_preflight_failure_remaining_for_test() -> Option<usize> {
    PROGRAM_PREFLIGHT_FAILURE_AT.with(std::cell::Cell::get)
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
    // Нулевая координата не является резервированием и не сдвигает индекс
    // fault injection; любая непустая координата учитывается и после прогрева.
    if capacity == 0 {
        return Ok(());
    }
    #[cfg(test)]
    let fail_this_coordinate = injected_program_preflight_failure();
    if buffer.capacity() >= capacity {
        return Ok(());
    }
    #[cfg(test)]
    if fail_this_coordinate {
        return Err(());
    }
    buffer.try_reserve_exact(capacity).map_err(|_| ())
}

#[cfg(test)]
mod program_preflight_reservation_tests {
    use super::*;

    #[test]
    fn mixed_warm_and_cold_coordinates_keep_stable_failure_indices() {
        let mut warm = Vec::<u8>::with_capacity(1);
        let mut cold = Vec::<u8>::new();
        let _failure = fail_program_preflight_reservation_for_test(1);

        assert_eq!(try_reserve_program_evaluation_buffer(&mut warm, 1), Ok(()));
        assert_eq!(try_reserve_program_evaluation_buffer(&mut cold, 1), Err(()));
    }

    #[test]
    fn a_warm_coordinate_consumes_but_cannot_realize_an_allocation_failure() {
        let mut warm = Vec::<u8>::with_capacity(1);
        let mut cold = Vec::<u8>::new();
        let _failure = fail_program_preflight_reservation_for_test(0);

        assert_eq!(try_reserve_program_evaluation_buffer(&mut warm, 1), Ok(()));
        assert_eq!(try_reserve_program_evaluation_buffer(&mut cold, 1), Ok(()));
        assert!(cold.capacity() >= 1);
    }

    #[test]
    fn an_empty_coordinate_does_not_consume_a_failure_index() {
        let mut empty = Vec::<u8>::new();
        let mut cold = Vec::<u8>::new();
        let _failure = fail_program_preflight_reservation_for_test(0);

        assert_eq!(try_reserve_program_evaluation_buffer(&mut empty, 0), Ok(()));
        assert_eq!(try_reserve_program_evaluation_buffer(&mut cold, 1), Err(()));
    }
}

struct ProgramEvaluationArenaV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    cells: Vec<ProgramConstraintCellV1<Evaluation>>,
    relation_members: Vec<ProgramRelationMemberEvidenceV1>,
    point_causal_records: Vec<ProgramPointCausalRecordV1>,
    point_causal_steps: Vec<PointOccurrenceAbsenceStepV1>,
    outputs: Vec<ProgramPaintOutputV1>,
}

impl<Evaluation> ProgramEvaluationArenaV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    const fn empty() -> Self {
        Self {
            cells: Vec::new(),
            relation_members: Vec::new(),
            point_causal_records: Vec::new(),
            point_causal_steps: Vec::new(),
            outputs: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.relation_members.clear();
        self.point_causal_records.clear();
        self.point_causal_steps.clear();
        self.outputs.clear();
    }
}

/// Move-only storage половина логического arena-слота Session. Return-route
/// остаётся только в observation, поэтому report нельзя привязать к чужому slot.
struct ProgramEvaluationArenaLeaseV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    storage: ProgramEvaluationArenaV1<Evaluation>,
}

/// Единственный return-route появляется при retirement из observation,
/// которая остаётся SSOT общей arena identity на всём lifetime report.
struct ProgramEvaluationArenaReturnV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    slot: ObservationArenaSlotV1,
    storage: ProgramEvaluationArenaV1<Evaluation>,
}

struct ProgramEvaluationArenaPoolV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    slots: [Option<ProgramEvaluationArenaV1<Evaluation>>; OBSERVATION_ARENA_SLOT_COUNT_V1],
}

/// Возвращает arena в точный pool-слот при любом выходе, включая unwind из
/// зарегистрированного evaluator или деструктора evidence.
struct ProgramEvaluationArenaGuardV1<'pool, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    destination: &'pool mut Option<ProgramEvaluationArenaV1<Evaluation>>,
    lease: Option<ProgramEvaluationArenaLeaseV1<Evaluation>>,
}

impl<Evaluation> ProgramEvaluationArenaGuardV1<'_, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn storage_mut(&mut self) -> &mut ProgramEvaluationArenaV1<Evaluation> {
        &mut self
            .lease
            .as_mut()
            .unwrap_or_else(|| unreachable!("an active guard owns one arena lease"))
            .storage
    }

    fn into_lease(mut self) -> ProgramEvaluationArenaLeaseV1<Evaluation> {
        self.lease
            .take()
            .unwrap_or_else(|| unreachable!("an active guard owns one arena lease"))
    }
}

impl<Evaluation> Drop for ProgramEvaluationArenaGuardV1<'_, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        // Эксклюзивное заимствование указывает ровно на слот, из которого
        // guard забрал storage. Drop не проверяет этот структурный инвариант
        // паникой: evaluator unwind иначе мог бы превратиться в abort.
        *self.destination = Some(lease.storage);
    }
}

impl<Evaluation> ProgramEvaluationArenaPoolV1<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| Some(ProgramEvaluationArenaV1::empty())),
        }
    }

    fn guard(
        &mut self,
        slot: ObservationArenaSlotV1,
    ) -> Option<ProgramEvaluationArenaGuardV1<'_, Evaluation>> {
        let destination = self.slots.get_mut(slot.index())?;
        let storage = destination.take()?;
        Some(ProgramEvaluationArenaGuardV1 {
            destination,
            lease: Some(ProgramEvaluationArenaLeaseV1 { storage }),
        })
    }

    fn restore(&mut self, returned: ProgramEvaluationArenaReturnV1<Evaluation>) {
        // Маршрут возврата минтится тем же observation, который выбрал слот
        // ограниченной арены; внешнего конструктора для такого слота нет.
        let destination = self
            .slots
            .get_mut(returned.slot.index())
            .unwrap_or_else(|| unreachable!("observation minted a bounded arena slot"));
        // Move-only lease удерживает storage до единственного retirement, поэтому
        // занятый destination означал бы повторный возврат одного владения.
        if destination.is_some() {
            unreachable!("a move-only Program arena cannot be returned twice");
        }
        *destination = Some(returned.storage);
    }
}

// Порядок координат совпадает с физическим владением отчёта: constraint cells,
// relation members, causal records и плоские replay steps.
fn program_report_cardinality_is_exact(actual: [usize; 4], expected: [usize; 4]) -> bool {
    actual == expected
}

#[cfg(test)]
pub(crate) fn program_report_cardinality_is_exact_for_test(
    actual: [usize; 4],
    expected: [usize; 4],
) -> bool {
    program_report_cardinality_is_exact(actual, expected)
}

fn storage_has_spare_capacity<const N: usize>(
    lengths: [usize; N],
    capacities: [usize; N],
    additional: [usize; N],
) -> bool {
    lengths
        .into_iter()
        .zip(capacities)
        .zip(additional)
        .all(|((length, capacity), additional)| {
            capacity
                .checked_sub(length)
                .is_some_and(|spare| spare >= additional)
        })
}

#[cfg(test)]
pub(crate) fn storage_has_spare_capacity_for_test<const N: usize>(
    lengths: [usize; N],
    capacities: [usize; N],
    additional: [usize; N],
) -> bool {
    storage_has_spare_capacity(lengths, capacities, additional)
}

// Пятая координата — output arena. Избыточная ёмкость допустима, но все арены
// обязаны быть пусты и независимо покрывать заранее рассчитанный объём.
fn selected_program_storage_is_prepared(
    lengths: [usize; 5],
    capacities: [usize; 5],
    required: [usize; 5],
) -> bool {
    lengths == [0; 5] && storage_has_spare_capacity(lengths, capacities, required)
}

#[cfg(test)]
pub(crate) fn selected_program_storage_is_prepared_for_test(
    lengths: [usize; 5],
    capacities: [usize; 5],
    required: [usize; 5],
) -> bool {
    selected_program_storage_is_prepared(lengths, capacities, required)
}

struct ProgramPointCausalBuffersV1<'buffers> {
    considered_state_index: Option<usize>,
    records: &'buffers mut Vec<ProgramPointCausalRecordV1>,
    steps: &'buffers mut Vec<PointOccurrenceAbsenceStepV1>,
}

enum ProgramConstraintEvidenceCaptureV1<'buffers, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    None,
    Report {
        cells: &'buffers mut Vec<ProgramConstraintCellV1<Evaluation>>,
        relation_members: &'buffers mut Vec<ProgramRelationMemberEvidenceV1>,
    },
}

/// Единственный accumulator плоской relation-arena. Физическая семья
/// endpoint-ов влияет только на member payload; capacity, span и verdict едины.
struct ProgramRelationEvidenceAccumulatorV1<'capture> {
    relation_members: Option<&'capture mut Vec<ProgramRelationMemberEvidenceV1>>,
    start: usize,
    expected: NonZeroUsize,
    written: usize,
    has_violation: bool,
}

impl<'capture> ProgramRelationEvidenceAccumulatorV1<'capture> {
    fn try_begin<'buffers, Evaluation>(
        capture: &'capture mut ProgramConstraintEvidenceCaptureV1<'buffers, Evaluation>,
        candidate_count: usize,
    ) -> Result<Self, ()>
    where
        Evaluation: ProgramConstraintEvaluatorSetV1,
    {
        let expected = NonZeroUsize::new(candidate_count).ok_or(())?;
        let relation_members = match capture {
            ProgramConstraintEvidenceCaptureV1::None => None,
            ProgramConstraintEvidenceCaptureV1::Report {
                relation_members, ..
            } => {
                if !storage_has_spare_capacity(
                    [relation_members.len()],
                    [relation_members.capacity()],
                    [candidate_count],
                ) {
                    return Err(());
                }
                Some(&mut **relation_members)
            }
        };
        let start = relation_members.as_ref().map_or(0, |members| members.len());
        Ok(Self {
            relation_members,
            start,
            expected,
            written: 0,
            has_violation: false,
        })
    }

    fn push(&mut self, member: ProgramRelationMemberEvidenceV1) -> Result<(), ()> {
        if self.written >= self.expected.get() {
            return Err(());
        }
        self.has_violation |= matches!(
            member.decision(),
            ProgramRelationMemberDecisionV1::Violation(_)
        );
        if let Some(relation_members) = self.relation_members.as_mut() {
            relation_members.push(member);
        }
        self.written = self.written.checked_add(1).ok_or(())?;
        Ok(())
    }

    fn finish(self) -> Option<(Option<NonEmptyRelationMemberSpanV1>, bool)> {
        if self.written != self.expected.get() {
            return None;
        }
        let span = match self.relation_members {
            None => None,
            Some(relation_members) => {
                let expected_end = self.start.checked_add(self.expected.get())?;
                if relation_members.len() != expected_end {
                    return None;
                }
                Some(NonEmptyRelationMemberSpanV1::from_start_and_len(
                    self.start,
                    self.expected.get(),
                )?)
            }
        };
        Some((span, self.has_violation))
    }
}

fn project_relation_result<Evaluation>(
    span: Option<NonEmptyRelationMemberSpanV1>,
    has_violation: bool,
) -> Option<ProgramConstraintResultV1<Evaluation>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    span.map(|span| {
        if has_violation {
            ProgramConstraintResultV1::Violation(ProgramConstraintViolationEvidenceV1::Relation(
                span,
            ))
        } else {
            ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::Relation(span))
        }
    })
}

struct ProgramCandidateCollectionV1<'buffers, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    evidence: ProgramConstraintEvidenceCaptureV1<'buffers, Evaluation>,
    outputs: Option<&'buffers mut Vec<ProgramPaintOutputV1>>,
    point_causal: Option<ProgramPointCausalBuffersV1<'buffers>>,
}

impl<Evaluation> ProgramCandidateCollectionV1<'_, Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    const fn none() -> Self {
        Self {
            evidence: ProgramConstraintEvidenceCaptureV1::None,
            outputs: None,
            point_causal: None,
        }
    }
}

fn prepare_program_evaluation_arena<Evaluation>(
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<Evaluation>,
) -> Result<
    ProgramEvaluationCardinalityV1,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let counts = checked_program_epoch_evaluation_cardinality(epoch, scenario_set.len().get())
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;

    // Search и exhaustive conflict взаимоисключают друг друга в одном update.
    // Покомпонентный максимум резервируется до evaluator work: старые два
    // владельца буферов исчезают без ослабления fail-before-work.
    arena.clear();
    try_reserve_program_evaluation_buffer(
        &mut arena.cells,
        counts.selected.max(counts.exhaustive_conflict),
    )
    .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    try_reserve_program_evaluation_buffer(
        &mut arena.relation_members,
        counts
            .selected_relation_members
            .max(counts.exhaustive_relation_members),
    )
    .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    try_reserve_program_evaluation_buffer(
        &mut arena.point_causal_records,
        counts
            .selected_point_records
            .max(counts.exhaustive_point_records),
    )
    .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    try_reserve_program_evaluation_buffer(
        &mut arena.point_causal_steps,
        counts
            .selected_replay_steps
            .max(counts.exhaustive_replay_steps),
    )
    .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    // До verdict любой из трёх logical slots может стать новым Ready и потому
    // заранее покрывает outputs. Conflict очистит значения; перенос capacity
    // потребовал бы второго pool/lease authority вместо одной связанной arena.
    try_reserve_program_evaluation_buffer(&mut arena.outputs, epoch.outputs.len())
        .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?;
    Ok(counts)
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
    presentation_cache: ProgramPresentationCacheV1,
    evaluation_arenas: ProgramEvaluationArenaPoolV1<Evaluation>,
}

/// Mutable execution state не владеет arena: guard удерживает непересекающееся
/// заимствование pool на всём fallible evaluator work.
struct ProgramEvaluationRuntimeV1<'plan> {
    bindings: &'plan mut AdmittedAppearanceBindings,
    workspace: &'plan mut AppearanceWorkspace,
    presentation_cache: &'plan mut ProgramPresentationCacheV1,
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

    fn retire_verified(&mut self, evidence: Self::Verified) {
        self.evaluation_arenas.restore(evidence.into_arena());
    }

    fn retire_violation(&mut self, evidence: Self::Violation) {
        self.evaluation_arenas.restore(evidence.into_arena());
    }
}

enum ProgramEvaluationOutcomeV1 {
    Verified { selected_state_index: Option<usize> },
    Conflict { considered_state_count: usize },
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
    let slot = observation.arena_slot();
    let ProgramSessionPlan {
        owner_generation: _,
        bindings,
        workspace,
        presentation_cache,
        evaluation_arenas,
    } = plan;
    let mut arena = evaluation_arenas
        .guard(slot)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let mut runtime = ProgramEvaluationRuntimeV1 {
        bindings,
        workspace,
        presentation_cache,
    };
    let scenario_set = NonEmptyScenarioSetV1::from_admitted(&observation)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let counts = prepare_program_evaluation_arena(epoch, scenario_set, arena.storage_mut())?;
    let outcome = evaluate_program_session_into(
        &mut runtime,
        epoch,
        scenario_set,
        arena.storage_mut(),
        counts,
    )?;
    if matches!(&outcome, ProgramEvaluationOutcomeV1::Conflict { .. }) {
        // Conflict не имеет output-authority: сохраняется только capacity для
        // следующего prospective update, но ни одно значение не переживает verdict.
        arena.storage_mut().outputs.clear();
    }
    let report = ProgramReportV1 {
        content_identity: epoch.content_identity,
        observation,
        arena: arena.into_lease(),
    };
    Ok(match outcome {
        ProgramEvaluationOutcomeV1::Verified {
            selected_state_index,
        } => SessionDecision::Verified(ProgramVerifiedV1 {
            report,
            selected_state_index,
        }),
        ProgramEvaluationOutcomeV1::Conflict {
            considered_state_count,
        } => SessionDecision::Violation(ProgramConflictV1 {
            report,
            considered_state_count,
        }),
    })
}

fn evaluate_program_session_into<Evaluation>(
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<Evaluation>,
    counts: ProgramEvaluationCardinalityV1,
) -> Result<
    ProgramEvaluationOutcomeV1,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let space = match &epoch.target_selection {
        CompiledTargetSelectionV1::FixedOnly => {
            return collect_program_candidate_into(
                runtime,
                epoch,
                scenario_set,
                None,
                1,
                arena,
                counts,
            );
        }
        CompiledTargetSelectionV1::Finite(space) => space,
    };

    let state_count = space.state_count();
    for state in space.states() {
        let state_index = state.index();
        apply_joint_candidate::<Evaluation>(runtime, &state)?;
        if !scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            state_index,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
        )? {
            // A selected tuple is never certified from its allocation-free
            // search pass. Re-apply and collect fresh terminal evidence.
            apply_joint_candidate::<Evaluation>(runtime, &state)?;
            match collect_program_candidate_into(
                runtime,
                epoch,
                scenario_set,
                Some(state_index),
                state_index + 1,
                arena,
                counts,
            )? {
                ProgramEvaluationOutcomeV1::Verified { .. } => {
                    return Ok(ProgramEvaluationOutcomeV1::Verified {
                        selected_state_index: Some(state_index),
                    });
                }
                ProgramEvaluationOutcomeV1::Conflict { .. } => {
                    // A selected finite state converts every fresh hard failure
                    // into FinalRecheckViolation inside collect_*. Reaching a
                    // plain Violation here means that contract was broken.
                    return Err(ProgramSessionEvaluationError::InternalInvariant);
                }
            }
        }
    }

    if !selected_program_storage_is_prepared(
        [
            arena.cells.len(),
            arena.relation_members.len(),
            arena.point_causal_records.len(),
            arena.point_causal_steps.len(),
            arena.outputs.len(),
        ],
        [
            arena.cells.capacity(),
            arena.relation_members.capacity(),
            arena.point_causal_records.capacity(),
            arena.point_causal_steps.capacity(),
            arena.outputs.capacity(),
        ],
        [
            counts.exhaustive_conflict,
            counts.exhaustive_relation_members,
            counts.exhaustive_point_records,
            counts.exhaustive_replay_steps,
            0,
        ],
    ) {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }

    for state in space.states() {
        let state_index = state.index();
        apply_joint_candidate::<Evaluation>(runtime, &state)?;
        if !scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            state_index,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1 {
                evidence: ProgramConstraintEvidenceCaptureV1::Report {
                    cells: &mut arena.cells,
                    relation_members: &mut arena.relation_members,
                },
                outputs: None,
                point_causal: Some(ProgramPointCausalBuffersV1 {
                    considered_state_index: Some(state_index),
                    records: &mut arena.point_causal_records,
                    steps: &mut arena.point_causal_steps,
                }),
            },
        )? {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
    }
    if epoch
        .constraint_phases
        .contains(ProgramEvaluationPhaseV1::ReportOnly)
    {
        for state in space.states() {
            let state_index = state.index();
            apply_joint_candidate::<Evaluation>(runtime, &state)?;
            if scan_program_candidate(
                runtime,
                epoch,
                scenario_set,
                state_index,
                ProgramEvaluationPhaseV1::ReportOnly,
                ProgramCandidateCollectionV1 {
                    evidence: ProgramConstraintEvidenceCaptureV1::Report {
                        cells: &mut arena.cells,
                        relation_members: &mut arena.relation_members,
                    },
                    outputs: None,
                    point_causal: None,
                },
            )? {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
        }
    }
    if !program_report_cardinality_is_exact(
        [
            arena.cells.len(),
            arena.relation_members.len(),
            arena.point_causal_records.len(),
            arena.point_causal_steps.len(),
        ],
        [
            counts.exhaustive_conflict,
            counts.exhaustive_relation_members,
            counts.exhaustive_point_records,
            counts.exhaustive_replay_steps,
        ],
    ) {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    canonicalize_program_report_cells(&mut arena.cells);

    Ok(ProgramEvaluationOutcomeV1::Conflict {
        considered_state_count: state_count,
    })
}

fn apply_joint_candidate<Evaluation>(
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    state: &AdmittedCompiledJointStateV1<'_>,
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    for (target, candidate) in state.assignments() {
        runtime
            .bindings
            .overwrite_paint_input_at(target.binding, candidate)
            .map_err(map_program_execution_binding_error)?;
    }
    Ok(())
}

fn collect_program_candidate_into<Evaluation>(
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
    selected_state_index: Option<usize>,
    considered_state_count: usize,
    arena: &mut ProgramEvaluationArenaV1<Evaluation>,
    counts: ProgramEvaluationCardinalityV1,
) -> Result<
    ProgramEvaluationOutcomeV1,
    ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>,
>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let expected_cell_count = counts.selected;
    let expected_relation_member_count = counts.selected_relation_members;
    let expected_point_record_count = counts.selected_point_records;
    let expected_replay_step_count = counts.selected_replay_steps;
    if !selected_program_storage_is_prepared(
        [
            arena.cells.len(),
            arena.relation_members.len(),
            arena.point_causal_records.len(),
            arena.point_causal_steps.len(),
            arena.outputs.len(),
        ],
        [
            arena.cells.capacity(),
            arena.relation_members.capacity(),
            arena.point_causal_records.capacity(),
            arena.point_causal_steps.capacity(),
            arena.outputs.capacity(),
        ],
        [
            expected_cell_count,
            expected_relation_member_count,
            expected_point_record_count,
            expected_replay_step_count,
            epoch.outputs.len(),
        ],
    ) {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    let candidate_state_index = selected_state_index.unwrap_or(0);
    let has_hard_constraints = epoch
        .constraint_phases
        .contains(ProgramEvaluationPhaseV1::Hard);
    let has_report_constraints = epoch
        .constraint_phases
        .contains(ProgramEvaluationPhaseV1::ReportOnly);
    let has_hard_violation = if has_hard_constraints {
        scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            candidate_state_index,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1 {
                evidence: ProgramConstraintEvidenceCaptureV1::Report {
                    cells: &mut arena.cells,
                    relation_members: &mut arena.relation_members,
                },
                outputs: Some(&mut arena.outputs),
                point_causal: Some(ProgramPointCausalBuffersV1 {
                    considered_state_index: None,
                    records: &mut arena.point_causal_records,
                    steps: &mut arena.point_causal_steps,
                }),
            },
        )?
    } else {
        false
    };
    if let Some(state_index) = selected_state_index.filter(|_| has_hard_violation) {
        // Search only nominates a finite state. Its fresh hard recheck owns the
        // terminal verdict, so diagnostics cannot mask or mutate that failure.
        if arena.cells.iter().any(|cell| !cell.is_hard()) {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
        let mut violations = arena
            .cells
            .iter()
            .filter(|cell| cell.result().is_violation());
        let first = violations
            .next()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let hard_violation_count = 1 + violations.count();
        return Err(ProgramSessionEvaluationError::FinalRecheckViolation {
            state_index,
            case_index: first.case_index,
            constraint: first.constraint,
            subject: first.subject,
            hard_violation_count,
        });
    }
    if has_report_constraints {
        let point_causal = (!has_hard_constraints).then_some(ProgramPointCausalBuffersV1 {
            considered_state_index: None,
            records: &mut arena.point_causal_records,
            steps: &mut arena.point_causal_steps,
        });
        if scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            candidate_state_index,
            ProgramEvaluationPhaseV1::ReportOnly,
            ProgramCandidateCollectionV1 {
                evidence: ProgramConstraintEvidenceCaptureV1::Report {
                    cells: &mut arena.cells,
                    relation_members: &mut arena.relation_members,
                },
                outputs: (!has_hard_constraints).then_some(&mut arena.outputs),
                point_causal,
            },
        )? {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
    }
    if !program_report_cardinality_is_exact(
        [
            arena.cells.len(),
            arena.relation_members.len(),
            arena.point_causal_records.len(),
            arena.point_causal_steps.len(),
        ],
        [
            expected_cell_count,
            expected_relation_member_count,
            expected_point_record_count,
            expected_replay_step_count,
        ],
    ) {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    canonicalize_program_report_cells(&mut arena.cells);
    if has_hard_violation {
        Ok(ProgramEvaluationOutcomeV1::Conflict {
            considered_state_count,
        })
    } else {
        Ok(ProgramEvaluationOutcomeV1::Verified {
            selected_state_index,
        })
    }
}

/// Evaluation is authority-first, while the emitted contract remains
/// `state × physical case × ConstraintId`. Sorting is in-place and therefore
/// adds no allocation to the preflight-bounded terminal path.
fn canonicalize_program_report_cells<Evaluation>(cells: &mut [ProgramConstraintCellV1<Evaluation>])
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    cells.sort_unstable_by_key(|cell| {
        (cell.candidate_state_index, cell.case_index, cell.constraint)
    });
}

fn resolve_visible_relation_endpoint(
    evaluation: &AppearanceEvaluationView<'_, '_>,
    contexts: &[CompiledOccurrenceContextV1],
    endpoint: CompiledVisibleRelationEndpointV1,
) -> Option<(Srgb8, ProgramVisibleRelationBindingV1)> {
    let source = evaluation.occurrence_at(endpoint.slot)?;
    if source.visible() != source.certificate().output_rgb() {
        return None;
    }
    let context = contexts.get(endpoint.occurrence_context_index)?;
    if !coordinate_pair_matches(
        &context.occurrence,
        &context.slot,
        &endpoint.occurrence,
        &endpoint.slot,
    ) {
        return None;
    }
    let point = ProgramPointOccurrenceV1::from_resolved(source, context.context);
    Some((
        Srgb8::new(source.visible()),
        ProgramVisibleRelationBindingV1 {
            occurrence: endpoint.occurrence,
            physical: point.binding(),
        },
    ))
}

fn scan_program_candidate<Evaluation>(
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
    candidate_state_index: usize,
    phase: ProgramEvaluationPhaseV1,
    collection: ProgramCandidateCollectionV1<'_, Evaluation>,
) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let ProgramCandidateCollectionV1 {
        mut evidence,
        mut outputs,
        mut point_causal,
    } = collection;
    let observation = scenario_set.observation();
    let schema = &epoch.observation_group.schema;
    if !observation.shares_schema_backing_with(schema) {
        observation
            .validate_surface_schema(schema.as_slice())
            .map_err(ProgramSessionEvaluationError::ObservationSchemaMismatch)?;
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }

    let case_count = scenario_set.len().get();
    let mut has_hard_violation = false;
    let mut output_mismatch = None;
    for case_index in 0..case_count {
        let values = scenario_set
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
        runtime
            .bindings
            .overwrite_surface_inputs_canonical(schema.as_slice().iter().copied(), |index| {
                values[index].srgb8()
            })
            .map_err(map_program_execution_binding_error)?;
        let evaluation = epoch
            .graph
            .evaluate_admitted_into(runtime.bindings, runtime.workspace)
            .map_err(map_program_execution_binding_error)?;
        runtime.presentation_cache.begin_case(phase);

        if let Some(point_causal) = point_causal.as_mut() {
            // Предварительный расчёт зарезервировал арены целиком. Локальная
            // проверка не даёт причинному replay незаметно начать аллоцировать.
            if !storage_has_spare_capacity(
                [point_causal.records.len(), point_causal.steps.len()],
                [
                    point_causal.records.capacity(),
                    point_causal.steps.capacity(),
                ],
                [
                    epoch.point_presentations.len(),
                    epoch.point_presentations.steps_per_case(),
                ],
            ) {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            for (presentation_ordinal, presentation) in epoch.point_presentations.iter().enumerate()
            {
                let resolved = runtime
                    .presentation_cache
                    .resolve(
                        &evaluation,
                        presentation_ordinal,
                        presentation,
                        Some(point_causal.steps),
                    )
                    .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                let replay = resolved
                    .replay
                    .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                point_causal.records.push(ProgramPointCausalRecordV1 {
                    considered_state_index: point_causal.considered_state_index,
                    case_index,
                    presentation_root: presentation.root,
                    release: presentation.absence_release,
                    replay,
                });
            }
        }

        for constraint in epoch
            .constraints
            .iter()
            .filter(|constraint| phase.includes(constraint.mode))
        {
            let (subject, result, is_violation) = match &constraint.body {
                CompiledProgramConstraintBodyV1::VisibleUnary {
                    occurrence,
                    slot,
                    occurrence_context_index,
                    invocation,
                } => {
                    let source = evaluation
                        .occurrence_at(*slot)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if source.visible() != source.certificate().output_rgb() {
                        return Err(ProgramSessionEvaluationError::InternalInvariant);
                    }
                    let binding = epoch
                        .occurrence_contexts
                        .get(*occurrence_context_index)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if !coordinate_pair_matches(
                        &binding.occurrence,
                        &binding.slot,
                        occurrence,
                        slot,
                    ) {
                        return Err(ProgramSessionEvaluationError::InternalInvariant);
                    }
                    let point = ProgramPointOccurrenceV1::from_resolved(source, binding.context);
                    let decision = Evaluation::assess(&epoch.evaluator, point, *invocation)
                        .map_err(|error| match error {
                            ProgramPointAssessmentErrorV1::Evaluator(source) => {
                                ProgramSessionEvaluationError::Evaluator {
                                    case_index,
                                    constraint: constraint.id,
                                    occurrence: *occurrence,
                                    context: binding.context,
                                    source,
                                }
                            }
                        })?;
                    let (result, is_violation) = match decision {
                        HardDecision::Pass(evidence) => {
                            debug_assert_eq!(
                                Evaluation::pass_binding(&evidence).physical(),
                                source.visible_point_binding(),
                            );
                            debug_assert_eq!(
                                Evaluation::pass_binding(&evidence).context(),
                                binding.context,
                            );
                            (
                                ProgramConstraintResultV1::Pass(
                                    ProgramConstraintPassEvidenceV1::VisibleUnary(evidence),
                                ),
                                false,
                            )
                        }
                        HardDecision::Violation(evidence) => {
                            debug_assert_eq!(
                                Evaluation::violation_binding(&evidence).physical(),
                                source.visible_point_binding(),
                            );
                            debug_assert_eq!(
                                Evaluation::violation_binding(&evidence).context(),
                                binding.context,
                            );
                            (
                                ProgramConstraintResultV1::Violation(
                                    ProgramConstraintViolationEvidenceV1::VisibleUnary(evidence),
                                ),
                                true,
                            )
                        }
                    };
                    (
                        ProgramConstraintSubjectV1::VisibleUnary {
                            occurrence: *occurrence,
                            context: binding.context,
                        },
                        Some(result),
                        is_violation,
                    )
                }
                CompiledProgramConstraintBodyV1::IntrinsicUnary {
                    target_id,
                    target,
                    invocation,
                } => {
                    let value = runtime
                        .bindings
                        .paint_input_at(*target)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    let binding = ProgramIntrinsicPaintBindingV1 {
                        target: *target_id,
                        value,
                    };
                    let (measurement, decision) = invocation.assess(value.source());
                    let (result, is_violation) = match decision {
                        HardDecision::Pass(proof) => (
                            ProgramConstraintResultV1::Pass(
                                ProgramConstraintPassEvidenceV1::IntrinsicUnary(
                                    ProgramIntrinsicUnaryPassEvidenceV1 {
                                        binding,
                                        measurement,
                                        proof,
                                    },
                                ),
                            ),
                            false,
                        ),
                        HardDecision::Violation(proof) => (
                            ProgramConstraintResultV1::Violation(
                                ProgramConstraintViolationEvidenceV1::IntrinsicUnary(
                                    ProgramIntrinsicUnaryViolationEvidenceV1 {
                                        binding,
                                        measurement,
                                        proof,
                                    },
                                ),
                            ),
                            true,
                        ),
                    };
                    (
                        ProgramConstraintSubjectV1::IntrinsicUnary { target: *target_id },
                        Some(result),
                        is_violation,
                    )
                }
                CompiledProgramConstraintBodyV1::IntrinsicRelation {
                    reference,
                    candidates,
                    invocation,
                } => {
                    let mut relation_evidence = ProgramRelationEvidenceAccumulatorV1::try_begin(
                        &mut evidence,
                        candidates.len(),
                    )
                    .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                    let reference_value = runtime
                        .bindings
                        .paint_input_at(reference.target)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    let reference_binding = ProgramIntrinsicPaintBindingV1 {
                        target: reference.target_id,
                        value: reference_value,
                    };
                    for candidate in candidates.iter() {
                        let candidate_value = runtime
                            .bindings
                            .paint_input_at(candidate.target)
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                        let candidate_binding = ProgramIntrinsicPaintBindingV1 {
                            target: candidate.target_id,
                            value: candidate_value,
                        };
                        let (measurement, decision) =
                            invocation.assess(reference_value.source(), candidate_value.source());
                        let decision = match decision {
                            HardDecision::Pass(pass) => ProgramRelationMemberDecisionV1::Pass(pass),
                            HardDecision::Violation(evidence) => {
                                ProgramRelationMemberDecisionV1::Violation(evidence)
                            }
                        };
                        relation_evidence
                            .push(ProgramRelationMemberEvidenceV1::Intrinsic {
                                reference: reference_binding,
                                candidate: candidate_binding,
                                measurement,
                                decision,
                            })
                            .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                    }
                    let (span, has_violation) = relation_evidence
                        .finish()
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    (
                        ProgramConstraintSubjectV1::IntrinsicRelation {
                            reference: reference.target_id,
                        },
                        project_relation_result(span, has_violation),
                        has_violation,
                    )
                }
                CompiledProgramConstraintBodyV1::VisibleRelation {
                    reference,
                    candidates,
                    invocation,
                } => {
                    let mut relation_evidence = ProgramRelationEvidenceAccumulatorV1::try_begin(
                        &mut evidence,
                        candidates.len(),
                    )
                    .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                    let (reference_visible, reference_binding) = resolve_visible_relation_endpoint(
                        &evaluation,
                        &epoch.occurrence_contexts,
                        *reference,
                    )
                    .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    for candidate in candidates.iter().copied() {
                        let (candidate_visible, candidate_binding) =
                            resolve_visible_relation_endpoint(
                                &evaluation,
                                &epoch.occurrence_contexts,
                                candidate,
                            )
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                        let (measurement, decision) =
                            invocation.assess(reference_visible, candidate_visible);
                        let decision = match decision {
                            HardDecision::Pass(pass) => ProgramRelationMemberDecisionV1::Pass(pass),
                            HardDecision::Violation(evidence) => {
                                ProgramRelationMemberDecisionV1::Violation(evidence)
                            }
                        };
                        relation_evidence
                            .push(ProgramRelationMemberEvidenceV1::Visible {
                                reference: reference_binding,
                                candidate: candidate_binding,
                                measurement,
                                decision,
                            })
                            .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                    }
                    let (span, has_violation) = relation_evidence
                        .finish()
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    (
                        ProgramConstraintSubjectV1::VisibleRelation {
                            reference: reference.occurrence,
                            context: reference_binding.physical.context(),
                        },
                        project_relation_result(span, has_violation),
                        has_violation,
                    )
                }
                CompiledProgramConstraintBodyV1::PointPresentation {
                    presentation_ordinal,
                    terminal,
                    convention,
                } => {
                    let presentation = epoch
                        .point_presentations
                        .entries
                        .get(*presentation_ordinal)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if presentation.terminal != *terminal {
                        return Err(ProgramSessionEvaluationError::InternalInvariant);
                    }
                    let resolved = runtime
                        .presentation_cache
                        .resolve(&evaluation, *presentation_ordinal, presentation, None)
                        .map_err(|()| ProgramSessionEvaluationError::InternalInvariant)?;
                    let result = if convention.forces_absent_mutation() {
                        ProgramConstraintResultV1::Violation(
                            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                                DeclaredSrgb8CleanSetViolationV1::FinalOwnedDomainAbsent,
                            ),
                        )
                    } else {
                        match resolved.domain {
                            ExactFinalOwnedPointDomainV1::Empty => {
                                ProgramConstraintResultV1::Violation(
                                    ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                                        DeclaredSrgb8CleanSetViolationV1::FinalOwnedDomainAbsent,
                                    ),
                                )
                            }
                            ExactFinalOwnedPointDomainV1::Singleton { visible } => {
                                let visible = Srgb8::new(visible);
                                match convention.classifier.classify(visible) {
                                ExactNominalSrgb8CleanSetDecisionV1::Accepted => {
                                    ProgramConstraintResultV1::Pass(
                                        ProgramConstraintPassEvidenceV1::DeclaredSrgb8CleanSet(
                                            DeclaredSrgb8CleanSetPassV1 { visible },
                                        ),
                                    )
                                }
                                ExactNominalSrgb8CleanSetDecisionV1::Rejected(interval) => {
                                    ProgramConstraintResultV1::Violation(
                                        ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                                            DeclaredSrgb8CleanSetViolationV1::Rejected {
                                                visible,
                                                rejected_blue_interval: interval,
                                            },
                                        ),
                                    )
                                }
                            }
                            }
                        }
                    };
                    let is_violation = result.is_violation();
                    (
                        ProgramConstraintSubjectV1::PointPresentation {
                            target: PointPresentationTargetV1 {
                                root: presentation.root,
                                occurrence: presentation.target,
                                absence_release: presentation.absence_release,
                            },
                            terminal: *terminal,
                        },
                        Some(result),
                        is_violation,
                    )
                }
            };
            if constraint.mode.rejects_candidate() && is_violation {
                has_hard_violation = true;
            }
            if let ProgramConstraintEvidenceCaptureV1::Report { cells, .. } = &mut evidence {
                let result = result.ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                cells.push(ProgramConstraintCellV1 {
                    candidate_state_index,
                    case_index,
                    constraint: constraint.id,
                    subject,
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
                let routed = ProgramPaintOutputV1 {
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

    let target_selection = compile_targets(
        &graph,
        &mut program.targets,
        program.joint_selection.as_mut(),
    )?;
    let all_occurrence_contexts = compile_occurrence_contexts(&graph, &program.occurrences)?;
    let point_presentations = compile_point_presentations(
        &graph,
        &mut program.presentation_roots,
        &mut program.presentation_targets,
    )?;
    let dependency_index = index_program_dependencies(&program)?;
    let mut dependency_scratch = ProgramDependencyScratchV1::new(&program)?;
    let mut constraints = compile_constraints::<Evaluation>(
        &program,
        &graph,
        &dependency_index,
        &mut dependency_scratch,
        &all_occurrence_contexts,
        &point_presentations,
        &program.constraints,
    )?;
    validate_terminal_dependency_cone(
        &program,
        &constraints,
        &dependency_index,
        &mut dependency_scratch,
    )?;
    let constraint_phases = CompiledConstraintPhasesV1::from_authored(&program.constraints);
    let occurrence_contexts =
        compact_constraint_contexts(&all_occurrence_contexts, &mut constraints)?;
    let outputs = compile_outputs(&graph, &mut program.outputs)?;
    let content_identity = identity::compile_program_content_identity_v6(&program)?;
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
        constraint_phases,
        point_presentations,
        outputs,
        target_selection,
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
        let TargetIntentV1::FixedSource(source) = target.intent else {
            continue;
        };
        if program
            .sources
            .binary_search_by_key(&source, |candidate| candidate.id)
            .is_err()
        {
            return Err(ProgramCompileError::MissingFixedSource {
                target: target.id,
                source,
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
    constraints: &[CompiledPointConstraint<ProgramConstraintInvocationOf<Evaluation>>],
    index: &IndexedProgramDependenciesV1,
    scratch: &mut ProgramDependencyScratchV1,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    // Объекты ограничений здесь уже скомпилированы, поэтому более точная
    // диагностика отсутствующей ссылки ещё может принадлежать только выходу.
    if program.outputs.iter().any(|output| {
        !program.paints.iter().any(|paint| match *paint {
            Paint::Solid { id, .. } | Paint::Opacity { id, .. } => id == output.paint,
        })
    }) {
        return Ok(());
    }

    let mut constrained_targets = false_slots(program.targets.len())?;
    let mut assessed_paints = false_slots(program.paints.len())?;
    for constraint in constraints {
        mark_constraint_coverage(
            program,
            index,
            scratch,
            constraint,
            &mut constrained_targets,
            Some(&mut assessed_paints),
        )?;
    }
    for (target_index, target) in program.targets.iter().enumerate() {
        if matches!(&target.intent, TargetIntentV1::Finite(_)) && !constrained_targets[target_index]
        {
            return Err(ProgramCompileError::UnconstrainedFiniteTarget { target: target.id });
        }
    }
    for output in &program.outputs {
        let paint_index = index
            .paint(output.paint)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if !assessed_paints[paint_index] {
            return Err(ProgramCompileError::UnassessedOutput {
                output: output.output,
                paint: output.paint,
            });
        }
    }

    let finite_count = program
        .targets
        .iter()
        .filter(|target| matches!(&target.intent, TargetIntentV1::Finite(_)))
        .count();
    if finite_count > 1 {
        let mut has_common_assessment = false;
        let mut per_constraint_targets = false_slots(program.targets.len())?;
        for constraint in constraints {
            per_constraint_targets.fill(false);
            mark_constraint_coverage(
                program,
                index,
                scratch,
                constraint,
                &mut per_constraint_targets,
                None,
            )?;
            if program.targets.iter().enumerate().all(|(index, target)| {
                !matches!(&target.intent, TargetIntentV1::Finite(_))
                    || per_constraint_targets[index]
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

fn mark_constraint_coverage<Invocation, Evaluation>(
    program: &Program<Evaluation>,
    index: &IndexedProgramDependenciesV1,
    scratch: &mut ProgramDependencyScratchV1,
    constraint: &CompiledPointConstraint<Invocation>,
    constrained_targets: &mut [bool],
    assessed_paints: Option<&mut [bool]>,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    match &constraint.body {
        CompiledProgramConstraintBodyV1::IntrinsicUnary { target_id, .. } => {
            let ordinal = program
                .targets
                .binary_search_by_key(target_id, |candidate| candidate.id)
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            *constrained_targets
                .get_mut(ordinal)
                .ok_or(ProgramCompileError::InternalInvariant)? = true;
            Ok(())
        }
        CompiledProgramConstraintBodyV1::IntrinsicRelation {
            reference,
            candidates,
            ..
        } => {
            for target in std::iter::once(reference.target_id)
                .chain(candidates.iter().map(|candidate| candidate.target_id))
            {
                let ordinal = program
                    .targets
                    .binary_search_by_key(&target, |candidate| candidate.id)
                    .map_err(|_| ProgramCompileError::InternalInvariant)?;
                *constrained_targets
                    .get_mut(ordinal)
                    .ok_or(ProgramCompileError::InternalInvariant)? = true;
            }
            Ok(())
        }
        CompiledProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
            merge_visible_constraint_coverage(
                index,
                scratch,
                [*occurrence],
                constrained_targets,
                assessed_paints,
            )
        }
        CompiledProgramConstraintBodyV1::VisibleRelation {
            reference,
            candidates,
            ..
        } => merge_visible_constraint_coverage(
            index,
            scratch,
            std::iter::once(reference.occurrence)
                .chain(candidates.iter().map(|candidate| candidate.occurrence)),
            constrained_targets,
            assessed_paints,
        ),
        CompiledProgramConstraintBodyV1::PointPresentation { terminal, .. } => {
            merge_visible_constraint_coverage(
                index,
                scratch,
                [*terminal],
                constrained_targets,
                assessed_paints,
            )
        }
    }
}

fn merge_visible_constraint_coverage(
    index: &IndexedProgramDependenciesV1,
    scratch: &mut ProgramDependencyScratchV1,
    roots: impl IntoIterator<Item = OccurrenceId>,
    constrained_targets: &mut [bool],
    assessed_paints: Option<&mut [bool]>,
) -> Result<(), ProgramCompileError> {
    scratch.scan(index, roots)?;
    if scratch.targets.len() != constrained_targets.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    for (destination, reached) in constrained_targets.iter_mut().zip(&scratch.targets) {
        *destination |= *reached;
    }
    if let Some(assessed_paints) = assessed_paints {
        if scratch.paints.len() != assessed_paints.len() {
            return Err(ProgramCompileError::InternalInvariant);
        }
        for (destination, reached) in assessed_paints.iter_mut().zip(&scratch.paints) {
            *destination |= *reached;
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

struct LoweredConstraint<'a, Invocation> {
    id: ConstraintId,
    mode: CompiledConstraintModeV1,
    body: &'a ProgramConstraintBodyV1<Invocation>,
}

fn compile_targets(
    graph: &CompiledAppearanceGraph,
    authored_targets: &mut [Target],
    authored_selection: Option<&mut DeclaredJointSelectionV1>,
) -> Result<CompiledTargetSelectionV1, ProgramCompileError> {
    struct CanonicalFiniteTargetV1<'a> {
        id: TargetId,
        binding: CompiledPaintInputSlotV1,
        domain: &'a FinitePaintDomainV1,
    }

    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(authored_targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in authored_targets {
        let TargetIntentV1::Finite(domain) = &mut target.intent else {
            continue;
        };
        let candidates = domain.candidates_mut();
        let binding = graph
            .bind_paint_input(target_paint_input_id(target.id))
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
        physical.extend(candidates.iter().map(|candidate| {
            (
                candidate.value.source(),
                candidate.value.opacity_bits(),
                candidate.id,
                candidate.value,
            )
        }));
        physical
            .sort_unstable_by_key(|(source, opacity_bits, id, _)| (*source, *opacity_bits, *id));
        if let Some(pair) = physical
            .windows(2)
            .find(|pair| pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1)
        {
            return Err(ProgramCompileError::DuplicateTargetCandidateValue {
                target: target.id,
                first: pair[0].2,
                duplicate: pair[1].2,
                value: pair[0].3,
            });
        }
        compiled.push(CanonicalFiniteTargetV1 {
            id: target.id,
            binding,
            domain,
        });
    }

    if compiled.is_empty() {
        return match authored_selection {
            None => Ok(CompiledTargetSelectionV1::FixedOnly),
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
                .domain
                .candidates()
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

    let lower_target = |target: CanonicalFiniteTargetV1<'_>| {
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(target.domain.candidates().len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        candidates.extend(
            target
                .domain
                .candidates()
                .iter()
                .map(|candidate| candidate.value()),
        );
        Ok(CompiledFiniteTargetV1 {
            binding: target.binding,
            candidates: candidates.into_boxed_slice(),
        })
    };
    let mut compiled = compiled.into_iter();
    let first = lower_target(
        compiled
            .next()
            .ok_or(ProgramCompileError::InternalInvariant)?,
    )?;
    let mut rest = Vec::new();
    rest.try_reserve_exact(compiled.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in compiled {
        rest.push(lower_target(target)?);
    }
    let targets = CompiledFiniteTargetsV1 {
        first,
        rest: rest.into_boxed_slice(),
    };
    let space =
        AdmittedCompiledJointSpaceV1::admit(targets, authored_tuples).map_err(
            |error| match error {
                CompiledJointSpaceAdmissionErrorV1::Authored(error) => {
                    ProgramCompileError::InvalidJointOrder(error)
                }
                CompiledJointSpaceAdmissionErrorV1::ResourceExhausted => {
                    ProgramCompileError::ResourceExhausted
                }
                CompiledJointSpaceAdmissionErrorV1::InternalInvariant => {
                    ProgramCompileError::InternalInvariant
                }
            },
        )?;
    Ok(CompiledTargetSelectionV1::Finite(space))
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
        let slot = graph
            .bind_occurrence(occurrence)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        compiled.push(CompiledOccurrenceContextV1 {
            occurrence,
            slot,
            context: contexts[index].1,
        });
    }
    if compiled.len() != authored.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(compiled.into_boxed_slice())
}

fn compile_declared_clean_set_body<Invocation>(
    presentations: &CompiledPointPresentationsV1,
    constraint: ConstraintId,
    target: PointPresentationTargetV1,
    convention: DeclaredSrgb8CleanSetV1,
) -> Result<CompiledProgramConstraintBodyV1<Invocation>, ProgramCompileError> {
    let missing = || ProgramCompileError::MissingConstraintPresentationTarget {
        constraint,
        root: target.root(),
        occurrence: target.occurrence(),
    };
    let key = (target.root(), target.occurrence());
    let presentation_ordinal = presentations
        .entries
        .binary_search_by_key(&key, |presentation| {
            (presentation.root, presentation.target)
        })
        .map_err(|_| missing())?;
    let presentation = &presentations.entries[presentation_ordinal];
    if presentation.absence_release != target.absence_release() {
        return Err(missing());
    }
    Ok(CompiledProgramConstraintBodyV1::PointPresentation {
        presentation_ordinal,
        terminal: presentation.terminal,
        convention,
    })
}

fn compile_constraints<Evaluation>(
    program: &Program<Evaluation>,
    graph: &CompiledAppearanceGraph,
    dependency_index: &IndexedProgramDependenciesV1,
    dependency_scratch: &mut ProgramDependencyScratchV1,
    occurrence_contexts: &[CompiledOccurrenceContextV1],
    presentations: &CompiledPointPresentationsV1,
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
        mode: CompiledConstraintModeV1::Hard,
        body: constraint.body(),
    }));
    lowered.extend(
        authored
            .report_only
            .iter()
            .map(|constraint| LoweredConstraint {
                id: constraint.id,
                mode: CompiledConstraintModeV1::ReportOnly,
                body: constraint.body(),
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
    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(total)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for constraint in lowered {
        let body = match constraint.body {
            ProgramConstraintBodyV1::VisibleUnary {
                occurrence,
                invocation,
            } => {
                let slot = graph.bind_occurrence(*occurrence).ok_or(
                    ProgramCompileError::MissingConstraintOccurrence {
                        constraint: constraint.id,
                        occurrence: *occurrence,
                    },
                )?;
                let occurrence_context_index = occurrence_contexts
                    .binary_search_by_key(occurrence, |binding| binding.occurrence)
                    .map_err(|_| ProgramCompileError::InternalInvariant)?;
                if occurrence_contexts[occurrence_context_index].slot != slot {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                CompiledProgramConstraintBodyV1::VisibleUnary {
                    occurrence: *occurrence,
                    slot,
                    occurrence_context_index,
                    invocation: *invocation,
                }
            }
            ProgramConstraintBodyV1::IntrinsicUnary { target, invocation } => {
                if program
                    .targets
                    .binary_search_by_key(target, |candidate| candidate.id)
                    .is_err()
                {
                    return Err(ProgramCompileError::MissingIntrinsicUnaryTarget {
                        constraint: constraint.id,
                        target: *target,
                    });
                }
                CompiledProgramConstraintBodyV1::IntrinsicUnary {
                    target_id: *target,
                    target: graph
                        .bind_paint_input(target_paint_input_id(*target))
                        .ok_or(ProgramCompileError::InternalInvariant)?,
                    invocation: *invocation,
                }
            }
            ProgramConstraintBodyV1::IntrinsicRelation {
                relation,
                invocation,
            } => {
                let reference_id = relation.reference();
                let reference_index = program
                    .targets
                    .binary_search_by_key(&reference_id, |target| target.id)
                    .map_err(|_| ProgramCompileError::MissingIntrinsicRelationReference {
                        constraint: constraint.id,
                        reference: reference_id,
                    })?;
                if matches!(
                    program.targets[reference_index].intent,
                    TargetIntentV1::Finite(_)
                ) {
                    return Err(
                        ProgramCompileError::SolverDependentIntrinsicRelationReference {
                            constraint: constraint.id,
                            reference: reference_id,
                        },
                    );
                }
                let reference = CompiledIntrinsicRelationEndpointV1 {
                    target_id: reference_id,
                    target: graph
                        .bind_paint_input(target_paint_input_id(reference_id))
                        .ok_or(ProgramCompileError::InternalInvariant)?,
                };
                let mut candidates = Vec::new();
                candidates
                    .try_reserve_exact(relation.candidates().len())
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                for candidate_id in relation.candidates().iter().copied() {
                    if program
                        .targets
                        .binary_search_by_key(&candidate_id, |target| target.id)
                        .is_err()
                    {
                        return Err(ProgramCompileError::MissingIntrinsicRelationCandidate {
                            constraint: constraint.id,
                            candidate: candidate_id,
                        });
                    }
                    candidates.push(CompiledIntrinsicRelationEndpointV1 {
                        target_id: candidate_id,
                        target: graph
                            .bind_paint_input(target_paint_input_id(candidate_id))
                            .ok_or(ProgramCompileError::InternalInvariant)?,
                    });
                }
                CompiledProgramConstraintBodyV1::IntrinsicRelation {
                    reference,
                    candidates: candidates.into_boxed_slice(),
                    invocation: *invocation,
                }
            }
            ProgramConstraintBodyV1::VisibleRelation {
                relation,
                invocation,
            } => {
                let compile_endpoint = |occurrence: OccurrenceId,
                                        missing: ProgramCompileError|
                 -> Result<
                    CompiledVisibleRelationEndpointV1,
                    ProgramCompileError,
                > {
                    let slot = graph.bind_occurrence(occurrence).ok_or(missing)?;
                    let occurrence_context_index = occurrence_contexts
                        .binary_search_by_key(&occurrence, |binding| binding.occurrence)
                        .map_err(|_| ProgramCompileError::InternalInvariant)?;
                    if occurrence_contexts[occurrence_context_index].slot != slot {
                        return Err(ProgramCompileError::InternalInvariant);
                    }
                    Ok(CompiledVisibleRelationEndpointV1 {
                        occurrence,
                        slot,
                        occurrence_context_index,
                    })
                };
                let reference_id = relation.reference();
                let reference = compile_endpoint(
                    reference_id,
                    ProgramCompileError::MissingVisibleRelationReference {
                        constraint: constraint.id,
                        reference: reference_id,
                    },
                )?;
                let mut candidates = Vec::new();
                candidates
                    .try_reserve_exact(relation.candidates().len())
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                for candidate_id in relation.candidates().iter().copied() {
                    candidates.push(compile_endpoint(
                        candidate_id,
                        ProgramCompileError::MissingVisibleRelationCandidate {
                            constraint: constraint.id,
                            candidate: candidate_id,
                        },
                    )?);
                }
                dependency_scratch.scan(dependency_index, [reference_id])?;
                if let Some(target) =
                    program
                        .targets
                        .iter()
                        .enumerate()
                        .find_map(|(index, target)| {
                            (dependency_scratch.targets[index]
                                && matches!(target.intent, TargetIntentV1::Finite(_)))
                            .then_some(target.id)
                        })
                {
                    return Err(
                        ProgramCompileError::SolverDependentVisibleRelationReference {
                            constraint: constraint.id,
                            reference: reference_id,
                            target,
                        },
                    );
                }
                CompiledProgramConstraintBodyV1::VisibleRelation {
                    reference,
                    candidates: candidates.into_boxed_slice(),
                    invocation: *invocation,
                }
            }
            ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { target } => {
                compile_declared_clean_set_body(
                    presentations,
                    constraint.id,
                    *target,
                    DeclaredSrgb8CleanSetV1::package_pinned(),
                )?
            }
            #[cfg(test)]
            ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { target } => {
                compile_declared_clean_set_body(
                    presentations,
                    constraint.id,
                    *target,
                    DeclaredSrgb8CleanSetV1::final_recheck_mutant(),
                )?
            }
        };
        compiled.push(CompiledPointConstraint {
            id: constraint.id,
            mode: constraint.mode,
            body,
        });
    }
    Ok(compiled.into_boxed_slice())
}

fn compact_constraint_contexts<Invocation>(
    all: &[CompiledOccurrenceContextV1],
    constraints: &mut [CompiledPointConstraint<Invocation>],
) -> Result<Box<[CompiledOccurrenceContextV1]>, ProgramCompileError> {
    let target_count = constraints
        .iter()
        .try_fold(0_usize, |count, constraint| {
            count.checked_add(match &constraint.body {
                CompiledProgramConstraintBodyV1::VisibleUnary { .. } => 1,
                CompiledProgramConstraintBodyV1::VisibleRelation { candidates, .. } => {
                    candidates.len().checked_add(1)?
                }
                CompiledProgramConstraintBodyV1::IntrinsicUnary { .. }
                | CompiledProgramConstraintBodyV1::IntrinsicRelation { .. }
                | CompiledProgramConstraintBodyV1::PointPresentation { .. } => 0,
            })
        })
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(target_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for constraint in constraints.iter() {
        match &constraint.body {
            CompiledProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
                targets.push(*occurrence);
            }
            CompiledProgramConstraintBodyV1::VisibleRelation {
                reference,
                candidates,
                ..
            } => {
                targets.push(reference.occurrence);
                targets.extend(candidates.iter().map(|candidate| candidate.occurrence));
            }
            CompiledProgramConstraintBodyV1::IntrinsicUnary { .. }
            | CompiledProgramConstraintBodyV1::IntrinsicRelation { .. }
            | CompiledProgramConstraintBodyV1::PointPresentation { .. } => {}
        }
    }
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
        if let CompiledProgramConstraintBodyV1::VisibleUnary {
            occurrence,
            slot,
            occurrence_context_index,
            ..
        } = &mut constraint.body
        {
            let index = compact
                .binary_search_by_key(occurrence, |binding| binding.occurrence)
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            if compact[index].slot != *slot {
                return Err(ProgramCompileError::InternalInvariant);
            }
            *occurrence_context_index = index;
        }
        if let CompiledProgramConstraintBodyV1::VisibleRelation {
            reference,
            candidates,
            ..
        } = &mut constraint.body
        {
            for endpoint in std::iter::once(reference).chain(candidates.iter_mut()) {
                let index = compact
                    .binary_search_by_key(&endpoint.occurrence, |binding| binding.occurrence)
                    .map_err(|_| ProgramCompileError::InternalInvariant)?;
                if compact[index].slot != endpoint.slot {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                endpoint.occurrence_context_index = index;
            }
        }
    }
    Ok(compact.into_boxed_slice())
}

fn compile_point_presentations(
    graph: &CompiledAppearanceGraph,
    roots: &mut [PointPresentationRootV1],
    targets: &mut [PointPresentationTargetV1],
) -> Result<CompiledPointPresentationsV1, ProgramCompileError> {
    roots.sort_unstable_by_key(|root| root.id);
    if let Some(root) = roots
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
        .map(|pair| pair[0].id)
    {
        return Err(ProgramCompileError::DuplicatePresentationRoot { root });
    }

    let mut compiled_roots = Vec::new();
    compiled_roots
        .try_reserve_exact(roots.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for root in roots.iter().copied() {
        let compiled = graph
            .compile_point_presentation_root(root.terminal)
            .map_err(|error| match error {
                PointPresentationPathErrorV1::MissingRoot => {
                    ProgramCompileError::MissingPresentationRootOccurrence {
                        root: root.id,
                        occurrence: root.terminal,
                    }
                }
                PointPresentationPathErrorV1::RootConsumedDownstream => {
                    ProgramCompileError::PresentationRootConsumedDownstream {
                        root: root.id,
                        occurrence: root.terminal,
                    }
                }
                PointPresentationPathErrorV1::ResourceExhausted => {
                    ProgramCompileError::ResourceExhausted
                }
                PointPresentationPathErrorV1::MissingTarget
                | PointPresentationPathErrorV1::TargetOutsideRootAncestry
                | PointPresentationPathErrorV1::IncompatibleRoot
                | PointPresentationPathErrorV1::InternalInvariant => {
                    ProgramCompileError::InternalInvariant
                }
            })?;
        compiled_roots.push((root.id, compiled));
    }

    targets.sort_unstable_by_key(|target| (target.root, target.occurrence));
    if let Some(duplicate) = targets
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
    {
        return Err(ProgramCompileError::DuplicatePointPresentationTarget {
            root: duplicate.root,
            occurrence: duplicate.occurrence,
        });
    }

    let mut compiled = Vec::new();
    let mut steps_per_case = 0_usize;
    compiled
        .try_reserve_exact(targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in targets.iter().copied() {
        let root_index = compiled_roots
            .binary_search_by_key(&target.root, |(root, _)| *root)
            .map_err(|_| ProgramCompileError::MissingPointPresentationRoot { root: target.root })?;
        let compiled_root = &compiled_roots[root_index].1;
        let terminal = compiled_root.terminal();
        let path = graph
            .compile_point_presentation_path(target.occurrence, compiled_root)
            .map_err(|error| match error {
                PointPresentationPathErrorV1::MissingTarget => {
                    ProgramCompileError::MissingPointPresentationOccurrence {
                        root: target.root,
                        occurrence: target.occurrence,
                    }
                }
                PointPresentationPathErrorV1::TargetOutsideRootAncestry => {
                    ProgramCompileError::PointPresentationOccurrenceOutsideRootAncestry {
                        root: target.root,
                        terminal,
                        occurrence: target.occurrence,
                    }
                }
                PointPresentationPathErrorV1::ResourceExhausted => {
                    ProgramCompileError::ResourceExhausted
                }
                PointPresentationPathErrorV1::MissingRoot
                | PointPresentationPathErrorV1::RootConsumedDownstream
                | PointPresentationPathErrorV1::IncompatibleRoot
                | PointPresentationPathErrorV1::InternalInvariant => {
                    ProgramCompileError::InternalInvariant
                }
            })?;
        steps_per_case = steps_per_case
            .checked_add(path.len())
            .ok_or(ProgramCompileError::ResourceExhausted)?;
        compiled.push(CompiledPointPresentationV1 {
            root: target.root,
            terminal,
            target: target.occurrence,
            absence_release: target.absence_release,
            path,
        });
    }

    for root in roots.iter() {
        if targets
            .binary_search_by_key(&root.id, |target| target.root)
            .is_err()
        {
            return Err(ProgramCompileError::UnusedPresentationRoot { root: root.id });
        }
    }
    Ok(CompiledPointPresentationsV1 {
        entries: compiled.into_boxed_slice(),
        steps_per_case,
    })
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

const fn target_paint_input_id(target: TargetId) -> PaintInputId {
    PaintInputId::new(target.value())
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
    let paint_inputs = try_collect_program(
        program.targets.len(),
        program
            .targets
            .iter()
            .map(|target| target_paint_input_id(target.id)),
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
            Paint::Solid { id, target } => PaintSpec::Input {
                id,
                input: target_paint_input_id(target),
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
        paint_inputs,
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
    let mut paint_inputs = Vec::new();
    paint_inputs
        .try_reserve_exact(program.targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in &program.targets {
        let value = match &target.intent {
            TargetIntentV1::FixedSource(source) => {
                let source_index = program
                    .sources
                    .binary_search_by_key(source, |candidate| candidate.id)
                    .map_err(|_| ProgramCompileError::InternalInvariant)?;
                EncodedPointPaintValueV1::opaque(program.sources[source_index].signal.srgb8())
            }
            TargetIntentV1::Finite(domain) => domain
                .candidates()
                .iter()
                // A canonical admitted value exists only to satisfy the cold
                // graph binding. Every search pass overwrites it atomically;
                // physical ordering avoids client-ID and declaration-order
                // influence even on this non-authoritative seed.
                .min_by_key(|candidate| {
                    (
                        candidate.value.source().bytes(),
                        candidate.value.opacity_bits(),
                    )
                })
                .map(|candidate| candidate.value)
                .ok_or(ProgramCompileError::InternalInvariant)?,
        };
        paint_inputs.push((target_paint_input_id(target.id), value));
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
    Ok(AppearanceBindings::new(paint_inputs, surfaces, opacities))
}

fn map_compile_error(error: CompileError) -> ProgramCompileError {
    match error {
        CompileError::DuplicatePaintInput { .. } => ProgramCompileError::InternalInvariant,
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
        CompileError::MissingPaintInput { .. } => ProgramCompileError::InternalInvariant,
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
