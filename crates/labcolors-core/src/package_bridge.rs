//! Sole concrete package authoring and runtime seam for a Core Program.
//!
//! The cold path appends typed physical declarations directly to the real Core
//! Program IR and compiles it with the code-owned evaluator union. The hot path
//! supplies schema-ordered physical scenarios and receives a borrowed,
//! allocation-free projection of Core-owned state and evidence. Neither path
//! exposes evaluator traits, generic Session plans, client vocabulary,
//! transport words, strings, or lifecycle generations.

use core::iter::FusedIterator;
use core::slice;

use crate::Srgb8;
use crate::appearance::{OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::joint::FiniteJointOrderErrorV1;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextDomainErrorV1, AppearanceContextFieldV1,
    AppearanceContextId, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, ColorSignal,
    IEC_SRGB_D65_XYZ_FRAME_V1, NumericDomainError, SurroundProfileId,
};
use crate::observation::{
    ObservationError, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    Revision, ScenarioId, SchemaOrderedScenarioSourceV1, UnknownReasonId,
};
use crate::program_session::{
    CompiledCoreProgramV1, CompositionProfile, ConstraintId, ConstraintInvocation,
    CoreProgramConstraintInvocationV1, CoreProgramDraftErrorV1, CoreProgramDraftV1,
    CoreProgramEvaluatorErrorV1, CoreProgramEvaluatorsV1, DeclaredJointSelectionV1,
    JointCandidateStateV1, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint,
    ProgramCompileError, ProgramConflictV1, ProgramOutputV1, ProgramSessionEvaluationError,
    ProgramSessionInstantiateError, ProgramSessionPlan, ProgramVerifiedV1, Source, SourceId,
    Surface, Target, TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1, TargetId,
};
use crate::session::{Session, SessionState, SessionUpdateError};
use crate::wcag22::Wcag22CriterionV1;

type CoreVerifiedV1 = ProgramVerifiedV1<CoreProgramEvaluatorsV1>;
type CoreConflictV1 = ProgramConflictV1<CoreProgramEvaluatorsV1>;
type CoreProgramPlanV1 = ProgramSessionPlan<CoreProgramEvaluatorsV1>;
type CoreProgramSessionV1 = Session<CoreProgramPlanV1>;
type CoreProgramStateV1 = SessionState<CoreVerifiedV1, CoreConflictV1>;
type CoreProgramPlanErrorV1 = ProgramSessionEvaluationError<CoreProgramEvaluatorErrorV1>;

macro_rules! package_program_id {
    ($name:ident, $core:ty) => {
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[must_use]
        pub struct $name($core);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(<$core>::new(value))
            }

            pub const fn value(self) -> u32 {
                self.0.value()
            }

            const fn from_core(value: $core) -> Self {
                Self(value)
            }

            const fn into_core(self) -> $core {
                self.0
            }
        }

        impl core::hash::Hash for $name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                core::hash::Hash::hash(&self.value(), state);
            }
        }
    };
}

package_program_id!(PackageProgramSourceIdV1, SourceId);
package_program_id!(PackageProgramTargetIdV1, TargetId);
package_program_id!(PackageProgramTargetCandidateIdV1, TargetCandidateId);
package_program_id!(PackageProgramOpacityInputIdV1, OpacityInputId);
package_program_id!(PackageProgramPaintIdV1, PaintId);
package_program_id!(PackageProgramSurfaceInputPortIdV1, SurfaceInputPortId);
package_program_id!(PackageProgramSurfaceIdV1, SurfaceId);
package_program_id!(PackageProgramOccurrenceIdV1, OccurrenceId);
package_program_id!(PackageProgramConstraintIdV1, ConstraintId);
package_program_id!(PackageProgramOutputSlotIdV1, OutputSlotId);

/// One finite candidate, stored as the actual Core target-candidate IR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramTargetCandidateV1(TargetCandidateV1);

impl PackageProgramTargetCandidateV1 {
    pub const fn new(id: PackageProgramTargetCandidateIdV1, source: Srgb8) -> Self {
        Self(TargetCandidateV1::from_srgb8(id.into_core(), source))
    }
}

/// One typed target/candidate choice stored as the actual Core joint IR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramJointChoiceV1(TargetCandidateChoiceV1);

impl PackageProgramJointChoiceV1 {
    pub const fn new(
        target: PackageProgramTargetIdV1,
        candidate: PackageProgramTargetCandidateIdV1,
    ) -> Self {
        Self(TargetCandidateChoiceV1::new(
            target.into_core(),
            candidate.into_core(),
        ))
    }
}

/// One complete explicit state in the finite joint order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageProgramJointStateV1(JointCandidateStateV1);

impl PackageProgramJointStateV1 {
    pub fn new(choices: Vec<PackageProgramJointChoiceV1>) -> Self {
        Self(JointCandidateStateV1::new(
            choices.into_iter().map(|choice| choice.0).collect(),
        ))
    }
}

/// Registered surround input for the current CIECAM16 context release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramSurroundV1 {
    Average,
    Dim,
    Dark,
}

impl PackageProgramSurroundV1 {
    const fn into_core(self) -> SurroundProfileId {
        match self {
            Self::Average => SurroundProfileId::AverageV1,
            Self::Dim => SurroundProfileId::DimV1,
            Self::Dark => SurroundProfileId::DarkV1,
        }
    }
}

/// Exact semantic input field rejected while forming an appearance context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramAppearanceContextFieldV1 {
    AdaptingLuminanceCdM2,
    BackgroundLuminanceRatioYbYw,
}

/// Exact numeric reason rejected while forming an appearance context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramNumericDomainErrorV1 {
    NonFinite,
    Negative,
    NotPositive,
    AboveOne,
}

/// Closed appearance-context admission failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramAppearanceContextErrorKindV1 {
    Domain,
    InternalInvariant,
}

/// Closed appearance-context admission failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramAppearanceContextErrorV1 {
    kind: PackageProgramAppearanceContextErrorKindV1,
    field: Option<PackageProgramAppearanceContextFieldV1>,
    reason: Option<PackageProgramNumericDomainErrorV1>,
}

impl PackageProgramAppearanceContextErrorV1 {
    pub const fn kind(self) -> PackageProgramAppearanceContextErrorKindV1 {
        self.kind
    }

    pub const fn field(self) -> Option<PackageProgramAppearanceContextFieldV1> {
        self.field
    }

    pub const fn reason(self) -> Option<PackageProgramNumericDomainErrorV1> {
        self.reason
    }

    fn from_core(error: AppearanceContextDomainErrorV1) -> Self {
        let field = match error.field() {
            AppearanceContextFieldV1::AdaptingLuminanceCdM2 => {
                PackageProgramAppearanceContextFieldV1::AdaptingLuminanceCdM2
            }
            AppearanceContextFieldV1::BackgroundLuminanceRatio => {
                PackageProgramAppearanceContextFieldV1::BackgroundLuminanceRatioYbYw
            }
        };
        let reason = match error.reason() {
            NumericDomainError::NonFinite => Some(PackageProgramNumericDomainErrorV1::NonFinite),
            NumericDomainError::Negative => Some(PackageProgramNumericDomainErrorV1::Negative),
            NumericDomainError::NotPositive => {
                Some(PackageProgramNumericDomainErrorV1::NotPositive)
            }
            NumericDomainError::AboveOne => Some(PackageProgramNumericDomainErrorV1::AboveOne),
            NumericDomainError::HueOutOfRange => None,
        };
        match reason {
            Some(reason) => Self {
                kind: PackageProgramAppearanceContextErrorKindV1::Domain,
                field: Some(field),
                reason: Some(reason),
            },
            None => Self {
                kind: PackageProgramAppearanceContextErrorKindV1::InternalInvariant,
                field: None,
                reason: None,
            },
        }
    }
}

/// Immutable admitted appearance context stored as the actual Core value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageProgramAppearanceContextV1(AppearanceContextId);

impl PackageProgramAppearanceContextV1 {
    /// Admit the explicit CIECAM16 viewing inputs for encoded sRGB8/D65.
    /// `background_luminance_ratio_yb_yw` is the dimensionless ratio `Y_b/Y_w`
    /// and must be finite in `(0, 1]`; it is not an absolute luminance.
    pub fn try_new(
        adapting_luminance_cd_m2: f64,
        background_luminance_ratio_yb_yw: f64,
        surround: PackageProgramSurroundV1,
    ) -> Result<Self, PackageProgramAppearanceContextErrorV1> {
        let adapting_luminance_cd_m2 = AdaptingLuminanceCdM2::try_new(adapting_luminance_cd_m2)
            .map_err(PackageProgramAppearanceContextErrorV1::from_core)?;
        let background_luminance_ratio =
            BackgroundLuminanceRatio::try_new(background_luminance_ratio_yb_yw)
                .map_err(PackageProgramAppearanceContextErrorV1::from_core)?;
        Ok(Self(AppearanceContextId::from_inputs(
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
            IEC_SRGB_D65_XYZ_FRAME_V1,
            adapting_luminance_cd_m2,
            background_luminance_ratio,
            surround.into_core(),
        )))
    }
}

/// Closed compile classification; the generic Core error never escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramCompileErrorKindV1 {
    DuplicateSource,
    DuplicateTarget,
    DuplicateTargetCandidate,
    DuplicateTargetCandidateSignal,
    DuplicateOpacityInput,
    DuplicateSurfaceInputPort,
    UnusedSurfaceInputPort,
    DuplicateSurfaceInputBinding,
    DuplicatePaint,
    DuplicateSurface,
    DuplicateOccurrence,
    DuplicateConstraint,
    DuplicateOutputSlot,
    MissingTargetSource,
    MissingPaintTarget,
    MissingPaintSource,
    MissingPaintOpacityInput,
    MissingSurfaceInputPort,
    MissingSurfaceOccurrence,
    MissingOccurrencePaint,
    MissingOccurrenceBackdrop,
    MissingConstraintOccurrence,
    MissingOutputPaint,
    PaintCycle,
    RenderCycle,
    OpacityOutOfDomain,
    EmptyTargetDomain,
    UnconstrainedTarget,
    DisconnectedFiniteTargets,
    UnassessedOutput,
    MissingJointSelection,
    JointSelectionWithoutTargets,
    JointStateDuplicateTarget,
    JointStateMissingTarget,
    JointStateUnknownTarget,
    JointStateUnknownCandidate,
    InvalidJointOrder,
    EmptySurfaceInputPortSet,
    EmptyOccurrenceSet,
    EmptyConstraintSet,
    EmptyOutputSet,
    ResourceExhausted,
    InternalInvariant,
}

/// Typed offending identity when one error has a single attributable handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramCompileErrorHandleV1 {
    Source(PackageProgramSourceIdV1),
    Target(PackageProgramTargetIdV1),
    TargetCandidate(PackageProgramTargetCandidateIdV1),
    OpacityInput(PackageProgramOpacityInputIdV1),
    Paint(PackageProgramPaintIdV1),
    SurfaceInputPort(PackageProgramSurfaceInputPortIdV1),
    Surface(PackageProgramSurfaceIdV1),
    Occurrence(PackageProgramOccurrenceIdV1),
    Constraint(PackageProgramConstraintIdV1),
    OutputSlot(PackageProgramOutputSlotIdV1),
}

impl PackageProgramCompileErrorHandleV1 {
    pub const fn value(self) -> u32 {
        match self {
            Self::Source(value) => value.value(),
            Self::Target(value) => value.value(),
            Self::TargetCandidate(value) => value.value(),
            Self::OpacityInput(value) => value.value(),
            Self::Paint(value) => value.value(),
            Self::SurfaceInputPort(value) => value.value(),
            Self::Surface(value) => value.value(),
            Self::Occurrence(value) => value.value(),
            Self::Constraint(value) => value.value(),
            Self::OutputSlot(value) => value.value(),
        }
    }
}

/// Exact owned members of one paint dependency cycle.
///
/// The wrapper takes ownership of Core's existing allocation. Projecting a
/// compile failure therefore cannot introduce a second infallible allocation.
#[derive(Debug, PartialEq, Eq)]
pub struct PackageProgramPaintCycleV1 {
    paints: Vec<PaintId>,
}

impl PackageProgramPaintCycleV1 {
    pub fn paints(&self) -> impl ExactSizeIterator<Item = PackageProgramPaintIdV1> + '_ {
        self.paints
            .iter()
            .copied()
            .map(PackageProgramPaintIdV1::from_core)
    }
}

/// Exact owned members of one render dependency cycle.
///
/// Surface and occurrence identities remain separate physical namespaces.
#[derive(Debug, PartialEq, Eq)]
pub struct PackageProgramRenderCycleV1 {
    surfaces: Vec<SurfaceId>,
    occurrences: Vec<OccurrenceId>,
}

impl PackageProgramRenderCycleV1 {
    pub fn surfaces(&self) -> impl ExactSizeIterator<Item = PackageProgramSurfaceIdV1> + '_ {
        self.surfaces
            .iter()
            .copied()
            .map(PackageProgramSurfaceIdV1::from_core)
    }

    pub fn occurrences(&self) -> impl ExactSizeIterator<Item = PackageProgramOccurrenceIdV1> + '_ {
        self.occurrences
            .iter()
            .copied()
            .map(PackageProgramOccurrenceIdV1::from_core)
    }
}

/// Exact closed reason why an explicit finite joint order was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramJointOrderErrorV1 {
    EmptyDomain {
        dimension: usize,
    },
    CardinalityOverflow,
    EmptyOrder,
    TupleArity {
        state: usize,
        expected: usize,
        actual: usize,
    },
    OrdinalOutOfDomain {
        state: usize,
        dimension: usize,
        ordinal: usize,
        domain_len: usize,
    },
    DuplicateTuple {
        first_state: usize,
        duplicate_state: usize,
    },
    IncompleteOrder {
        expected: usize,
        actual: usize,
    },
    ResourceExhausted,
}

/// Atomic, lossless authored-program compile failure.
///
/// This public enum is authoritative. `kind`, `primary_handle`, and
/// `related_handle` are convenience projections only; they never replace the
/// complete typed payload carried by the matching variant.
#[derive(Debug, PartialEq, Eq)]
pub enum PackageProgramCompileErrorV1 {
    DuplicateSource {
        source: PackageProgramSourceIdV1,
    },
    DuplicateTarget {
        target: PackageProgramTargetIdV1,
    },
    MissingTargetSource {
        target: PackageProgramTargetIdV1,
        source: PackageProgramSourceIdV1,
    },
    DuplicateOpacityInput {
        input: PackageProgramOpacityInputIdV1,
    },
    DuplicateSurfaceInputPort {
        input: PackageProgramSurfaceInputPortIdV1,
    },
    UnusedSurfaceInputPort {
        input: PackageProgramSurfaceInputPortIdV1,
    },
    DuplicateSurfaceInputBinding {
        input: PackageProgramSurfaceInputPortIdV1,
        first: PackageProgramSurfaceIdV1,
        duplicate: PackageProgramSurfaceIdV1,
    },
    DuplicatePaint {
        paint: PackageProgramPaintIdV1,
    },
    DuplicateSurface {
        surface: PackageProgramSurfaceIdV1,
    },
    DuplicateOccurrence {
        occurrence: PackageProgramOccurrenceIdV1,
    },
    MissingPaintTarget {
        paint: PackageProgramPaintIdV1,
        target: PackageProgramTargetIdV1,
    },
    MissingPaintSource {
        paint: PackageProgramPaintIdV1,
        source: PackageProgramPaintIdV1,
    },
    MissingPaintOpacityInput {
        paint: PackageProgramPaintIdV1,
        input: PackageProgramOpacityInputIdV1,
    },
    MissingSurfaceInputPort {
        surface: PackageProgramSurfaceIdV1,
        input: PackageProgramSurfaceInputPortIdV1,
    },
    MissingSurfaceOccurrence {
        surface: PackageProgramSurfaceIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
    },
    MissingOccurrencePaint {
        occurrence: PackageProgramOccurrenceIdV1,
        paint: PackageProgramPaintIdV1,
    },
    MissingOccurrenceBackdrop {
        occurrence: PackageProgramOccurrenceIdV1,
        surface: PackageProgramSurfaceIdV1,
    },
    PaintCycle(PackageProgramPaintCycleV1),
    RenderCycle(PackageProgramRenderCycleV1),
    OpacityOutOfDomain {
        input: PackageProgramOpacityInputIdV1,
    },
    EmptyTargetDomain {
        target: PackageProgramTargetIdV1,
    },
    DuplicateTargetCandidate {
        target: PackageProgramTargetIdV1,
        candidate: PackageProgramTargetCandidateIdV1,
    },
    DuplicateTargetCandidateSignal {
        target: PackageProgramTargetIdV1,
        first: PackageProgramTargetCandidateIdV1,
        duplicate: PackageProgramTargetCandidateIdV1,
        encoded_srgb8: Srgb8,
    },
    UnconstrainedTarget {
        target: PackageProgramTargetIdV1,
    },
    DisconnectedFiniteTargets,
    UnassessedOutput {
        output: PackageProgramOutputSlotIdV1,
        paint: PackageProgramPaintIdV1,
    },
    MissingJointSelection,
    JointSelectionWithoutTargets,
    JointStateDuplicateTarget {
        state: usize,
        target: PackageProgramTargetIdV1,
    },
    JointStateMissingTarget {
        state: usize,
        target: PackageProgramTargetIdV1,
    },
    JointStateUnknownTarget {
        state: usize,
        target: PackageProgramTargetIdV1,
    },
    JointStateUnknownCandidate {
        state: usize,
        target: PackageProgramTargetIdV1,
        candidate: PackageProgramTargetCandidateIdV1,
    },
    InvalidJointOrder(PackageProgramJointOrderErrorV1),
    /// The package contract has one code-owned atomic observation group;
    /// authored input ports are its complete, canonical membership.
    EmptySurfaceInputPortSet,
    EmptyOccurrenceSet,
    EmptyConstraintSet,
    EmptyOutputSet,
    DuplicateConstraint {
        constraint: PackageProgramConstraintIdV1,
    },
    MissingConstraintOccurrence {
        constraint: PackageProgramConstraintIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
    },
    DuplicateOutputSlot {
        output: PackageProgramOutputSlotIdV1,
    },
    MissingOutputPaint {
        output: PackageProgramOutputSlotIdV1,
        paint: PackageProgramPaintIdV1,
    },
    ResourceExhausted,
    InternalInvariant,
}

impl PackageProgramCompileErrorV1 {
    pub const fn kind(&self) -> PackageProgramCompileErrorKindV1 {
        use PackageProgramCompileErrorKindV1 as Kind;

        match self {
            Self::DuplicateSource { .. } => Kind::DuplicateSource,
            Self::DuplicateTarget { .. } => Kind::DuplicateTarget,
            Self::MissingTargetSource { .. } => Kind::MissingTargetSource,
            Self::DuplicateOpacityInput { .. } => Kind::DuplicateOpacityInput,
            Self::DuplicateSurfaceInputPort { .. } => Kind::DuplicateSurfaceInputPort,
            Self::UnusedSurfaceInputPort { .. } => Kind::UnusedSurfaceInputPort,
            Self::DuplicateSurfaceInputBinding { .. } => Kind::DuplicateSurfaceInputBinding,
            Self::DuplicatePaint { .. } => Kind::DuplicatePaint,
            Self::DuplicateSurface { .. } => Kind::DuplicateSurface,
            Self::DuplicateOccurrence { .. } => Kind::DuplicateOccurrence,
            Self::MissingPaintTarget { .. } => Kind::MissingPaintTarget,
            Self::MissingPaintSource { .. } => Kind::MissingPaintSource,
            Self::MissingPaintOpacityInput { .. } => Kind::MissingPaintOpacityInput,
            Self::MissingSurfaceInputPort { .. } => Kind::MissingSurfaceInputPort,
            Self::MissingSurfaceOccurrence { .. } => Kind::MissingSurfaceOccurrence,
            Self::MissingOccurrencePaint { .. } => Kind::MissingOccurrencePaint,
            Self::MissingOccurrenceBackdrop { .. } => Kind::MissingOccurrenceBackdrop,
            Self::PaintCycle(_) => Kind::PaintCycle,
            Self::RenderCycle(_) => Kind::RenderCycle,
            Self::OpacityOutOfDomain { .. } => Kind::OpacityOutOfDomain,
            Self::EmptyTargetDomain { .. } => Kind::EmptyTargetDomain,
            Self::DuplicateTargetCandidate { .. } => Kind::DuplicateTargetCandidate,
            Self::DuplicateTargetCandidateSignal { .. } => Kind::DuplicateTargetCandidateSignal,
            Self::UnconstrainedTarget { .. } => Kind::UnconstrainedTarget,
            Self::DisconnectedFiniteTargets => Kind::DisconnectedFiniteTargets,
            Self::UnassessedOutput { .. } => Kind::UnassessedOutput,
            Self::MissingJointSelection => Kind::MissingJointSelection,
            Self::JointSelectionWithoutTargets => Kind::JointSelectionWithoutTargets,
            Self::JointStateDuplicateTarget { .. } => Kind::JointStateDuplicateTarget,
            Self::JointStateMissingTarget { .. } => Kind::JointStateMissingTarget,
            Self::JointStateUnknownTarget { .. } => Kind::JointStateUnknownTarget,
            Self::JointStateUnknownCandidate { .. } => Kind::JointStateUnknownCandidate,
            Self::InvalidJointOrder(_) => Kind::InvalidJointOrder,
            Self::EmptySurfaceInputPortSet => Kind::EmptySurfaceInputPortSet,
            Self::EmptyOccurrenceSet => Kind::EmptyOccurrenceSet,
            Self::EmptyConstraintSet => Kind::EmptyConstraintSet,
            Self::EmptyOutputSet => Kind::EmptyOutputSet,
            Self::DuplicateConstraint { .. } => Kind::DuplicateConstraint,
            Self::MissingConstraintOccurrence { .. } => Kind::MissingConstraintOccurrence,
            Self::DuplicateOutputSlot { .. } => Kind::DuplicateOutputSlot,
            Self::MissingOutputPaint { .. } => Kind::MissingOutputPaint,
            Self::ResourceExhausted => Kind::ResourceExhausted,
            Self::InternalInvariant => Kind::InternalInvariant,
        }
    }

    pub const fn primary_handle(&self) -> Option<PackageProgramCompileErrorHandleV1> {
        use PackageProgramCompileErrorHandleV1 as Handle;

        match self {
            Self::DuplicateSource { source } => Some(Handle::Source(*source)),
            Self::DuplicateTarget { target }
            | Self::EmptyTargetDomain { target }
            | Self::UnconstrainedTarget { target }
            | Self::JointStateDuplicateTarget { target, .. }
            | Self::JointStateMissingTarget { target, .. }
            | Self::JointStateUnknownTarget { target, .. }
            | Self::JointStateUnknownCandidate { target, .. } => Some(Handle::Target(*target)),
            Self::MissingTargetSource { target, .. }
            | Self::DuplicateTargetCandidate { target, .. }
            | Self::DuplicateTargetCandidateSignal { target, .. } => Some(Handle::Target(*target)),
            Self::DuplicateOpacityInput { input } | Self::OpacityOutOfDomain { input } => {
                Some(Handle::OpacityInput(*input))
            }
            Self::DuplicateSurfaceInputPort { input }
            | Self::UnusedSurfaceInputPort { input }
            | Self::DuplicateSurfaceInputBinding { input, .. } => {
                Some(Handle::SurfaceInputPort(*input))
            }
            Self::DuplicatePaint { paint }
            | Self::MissingPaintTarget { paint, .. }
            | Self::MissingPaintSource { paint, .. }
            | Self::MissingPaintOpacityInput { paint, .. } => Some(Handle::Paint(*paint)),
            Self::DuplicateSurface { surface }
            | Self::MissingSurfaceInputPort { surface, .. }
            | Self::MissingSurfaceOccurrence { surface, .. } => Some(Handle::Surface(*surface)),
            Self::DuplicateOccurrence { occurrence }
            | Self::MissingOccurrencePaint { occurrence, .. }
            | Self::MissingOccurrenceBackdrop { occurrence, .. } => {
                Some(Handle::Occurrence(*occurrence))
            }
            Self::UnassessedOutput { output, .. }
            | Self::DuplicateOutputSlot { output }
            | Self::MissingOutputPaint { output, .. } => Some(Handle::OutputSlot(*output)),
            Self::DuplicateConstraint { constraint }
            | Self::MissingConstraintOccurrence { constraint, .. } => {
                Some(Handle::Constraint(*constraint))
            }
            Self::PaintCycle(_)
            | Self::RenderCycle(_)
            | Self::DisconnectedFiniteTargets
            | Self::MissingJointSelection
            | Self::JointSelectionWithoutTargets
            | Self::InvalidJointOrder(_)
            | Self::EmptySurfaceInputPortSet
            | Self::EmptyOccurrenceSet
            | Self::EmptyConstraintSet
            | Self::EmptyOutputSet
            | Self::ResourceExhausted
            | Self::InternalInvariant => None,
        }
    }

    pub const fn related_handle(&self) -> Option<PackageProgramCompileErrorHandleV1> {
        use PackageProgramCompileErrorHandleV1 as Handle;

        match self {
            Self::MissingTargetSource { source, .. } => Some(Handle::Source(*source)),
            Self::DuplicateSurfaceInputBinding { duplicate, .. } => {
                Some(Handle::Surface(*duplicate))
            }
            Self::MissingPaintTarget { target, .. } => Some(Handle::Target(*target)),
            Self::MissingPaintSource { source, .. } => Some(Handle::Paint(*source)),
            Self::MissingPaintOpacityInput { input, .. } => Some(Handle::OpacityInput(*input)),
            Self::MissingSurfaceInputPort { input, .. } => Some(Handle::SurfaceInputPort(*input)),
            Self::MissingSurfaceOccurrence { occurrence, .. }
            | Self::MissingConstraintOccurrence { occurrence, .. } => {
                Some(Handle::Occurrence(*occurrence))
            }
            Self::MissingOccurrencePaint { paint, .. }
            | Self::UnassessedOutput { paint, .. }
            | Self::MissingOutputPaint { paint, .. } => Some(Handle::Paint(*paint)),
            Self::MissingOccurrenceBackdrop { surface, .. } => Some(Handle::Surface(*surface)),
            Self::DuplicateTargetCandidate { candidate, .. }
            | Self::DuplicateTargetCandidateSignal {
                duplicate: candidate,
                ..
            }
            | Self::JointStateUnknownCandidate { candidate, .. } => {
                Some(Handle::TargetCandidate(*candidate))
            }
            Self::DuplicateSource { .. }
            | Self::DuplicateTarget { .. }
            | Self::DuplicateOpacityInput { .. }
            | Self::DuplicateSurfaceInputPort { .. }
            | Self::UnusedSurfaceInputPort { .. }
            | Self::DuplicatePaint { .. }
            | Self::DuplicateSurface { .. }
            | Self::DuplicateOccurrence { .. }
            | Self::PaintCycle(_)
            | Self::RenderCycle(_)
            | Self::OpacityOutOfDomain { .. }
            | Self::EmptyTargetDomain { .. }
            | Self::UnconstrainedTarget { .. }
            | Self::DisconnectedFiniteTargets
            | Self::MissingJointSelection
            | Self::JointSelectionWithoutTargets
            | Self::JointStateDuplicateTarget { .. }
            | Self::JointStateMissingTarget { .. }
            | Self::JointStateUnknownTarget { .. }
            | Self::InvalidJointOrder(_)
            | Self::EmptySurfaceInputPortSet
            | Self::EmptyOccurrenceSet
            | Self::EmptyConstraintSet
            | Self::EmptyOutputSet
            | Self::DuplicateConstraint { .. }
            | Self::DuplicateOutputSlot { .. }
            | Self::ResourceExhausted
            | Self::InternalInvariant => None,
        }
    }
}

/// Concrete cold-path builder over the actual Core Program IR.
#[must_use]
pub struct PackageProgramDraftV1 {
    inner: CoreProgramDraftV1,
}

/// Draft mutation rejected before Core compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramDraftErrorV1 {
    JointSelectionAlreadyDeclared,
}

impl PackageProgramDraftV1 {
    pub fn new() -> Self {
        Self {
            inner: CoreProgramDraftV1::new(),
        }
    }

    pub fn push_source(&mut self, id: PackageProgramSourceIdV1, source: Srgb8) -> &mut Self {
        self.inner
            .push_source(Source::new(id.into_core(), ColorSignal::from_srgb8(source)));
        self
    }

    pub fn push_fixed_target(
        &mut self,
        id: PackageProgramTargetIdV1,
        source: PackageProgramSourceIdV1,
    ) -> &mut Self {
        self.inner
            .push_target(Target::fixed(id.into_core(), source.into_core()));
        self
    }

    pub fn push_finite_target(
        &mut self,
        id: PackageProgramTargetIdV1,
        source: PackageProgramSourceIdV1,
        candidates: Vec<PackageProgramTargetCandidateV1>,
    ) -> &mut Self {
        self.inner.push_target(Target::finite(
            id.into_core(),
            source.into_core(),
            candidates
                .into_iter()
                .map(|candidate| candidate.0)
                .collect(),
        ));
        self
    }

    pub fn set_joint_selection(
        &mut self,
        states: Vec<PackageProgramJointStateV1>,
    ) -> Result<&mut Self, PackageProgramDraftErrorV1> {
        self.inner
            .set_joint_selection(DeclaredJointSelectionV1::new(
                states.into_iter().map(|state| state.0).collect(),
            ))
            .map_err(|error| match error {
                CoreProgramDraftErrorV1::JointSelectionAlreadyDeclared => {
                    PackageProgramDraftErrorV1::JointSelectionAlreadyDeclared
                }
            })?;
        Ok(self)
    }

    pub fn push_surface_input_port(
        &mut self,
        input: PackageProgramSurfaceInputPortIdV1,
    ) -> &mut Self {
        self.inner.push_surface_input_port(input.into_core());
        self
    }

    pub fn push_opacity_input(
        &mut self,
        id: PackageProgramOpacityInputIdV1,
        value: f64,
    ) -> &mut Self {
        self.inner
            .push_opacity_input(OpacityInput::new(id.into_core(), value));
        self
    }

    pub fn push_solid_paint(
        &mut self,
        id: PackageProgramPaintIdV1,
        target: PackageProgramTargetIdV1,
    ) -> &mut Self {
        self.inner.push_paint(Paint::Solid {
            id: id.into_core(),
            target: target.into_core(),
        });
        self
    }

    pub fn push_opacity_paint(
        &mut self,
        id: PackageProgramPaintIdV1,
        source: PackageProgramPaintIdV1,
        opacity: PackageProgramOpacityInputIdV1,
    ) -> &mut Self {
        self.inner.push_paint(Paint::Opacity {
            id: id.into_core(),
            source: source.into_core(),
            opacity: opacity.into_core(),
        });
        self
    }

    pub fn push_input_surface(
        &mut self,
        id: PackageProgramSurfaceIdV1,
        input: PackageProgramSurfaceInputPortIdV1,
    ) -> &mut Self {
        self.inner.push_surface(Surface::Input {
            id: id.into_core(),
            input: input.into_core(),
        });
        self
    }

    pub fn push_occurrence_surface(
        &mut self,
        id: PackageProgramSurfaceIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
    ) -> &mut Self {
        self.inner.push_surface(Surface::FromOccurrence {
            id: id.into_core(),
            occurrence: occurrence.into_core(),
        });
        self
    }

    pub fn push_source_over_occurrence(
        &mut self,
        id: PackageProgramOccurrenceIdV1,
        subject: PackageProgramPaintIdV1,
        against: PackageProgramSurfaceIdV1,
        context: PackageProgramAppearanceContextV1,
    ) -> &mut Self {
        self.inner.push_occurrence(Occurrence::new(
            id.into_core(),
            subject.into_core(),
            against.into_core(),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context.0,
        ));
        self
    }

    pub fn push_exact_hard(
        &mut self,
        id: PackageProgramConstraintIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner.push_hard_constraint(ConstraintInvocation::hard(
            id.into_core(),
            occurrence.into_core(),
            CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
        ));
        self
    }

    pub fn push_exact_report_only(
        &mut self,
        id: PackageProgramConstraintIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
        expected: Srgb8,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::ExactSrgb8(expected),
            ));
        self
    }

    pub fn push_wcag22_hard(
        &mut self,
        id: PackageProgramConstraintIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner.push_hard_constraint(ConstraintInvocation::hard(
            id.into_core(),
            occurrence.into_core(),
            CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
        ));
        self
    }

    pub fn push_wcag22_report_only(
        &mut self,
        id: PackageProgramConstraintIdV1,
        occurrence: PackageProgramOccurrenceIdV1,
        criterion: Wcag22CriterionV1,
    ) -> &mut Self {
        self.inner
            .push_report_constraint(ConstraintInvocation::report_only(
                id.into_core(),
                occurrence.into_core(),
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(criterion),
            ));
        self
    }

    pub fn push_output(
        &mut self,
        output: PackageProgramOutputSlotIdV1,
        paint: PackageProgramPaintIdV1,
    ) -> &mut Self {
        self.inner
            .push_output(OutputBinding::new(output.into_core(), paint.into_core()));
        self
    }

    pub fn compile(self) -> Result<PackageProgramOwnerV1, PackageProgramCompileErrorV1> {
        let compiled = self.inner.compile().map_err(map_program_compile_error)?;
        Ok(PackageProgramOwnerV1::from_compiled(compiled))
    }
}

impl Default for PackageProgramDraftV1 {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque strong owner of one exact compiled Core Program.
///
/// Sessions instantiated from this owner are independently mutable. In the
/// terminal stacked build they retain only the canonical weak owner binding;
/// dropping this value therefore expires every such Session before its next
/// admission.
pub struct PackageProgramOwnerV1 {
    compiled: CompiledCoreProgramV1,
}

impl PackageProgramOwnerV1 {
    /// Internal handoff from the canonical concrete Core draft compiler.
    pub(crate) const fn from_compiled(compiled: CompiledCoreProgramV1) -> Self {
        Self { compiled }
    }

    /// Number of schema-ordered surface values required in every scenario.
    pub fn surface_input_port_count(&self) -> usize {
        self.compiled.surface_input_ports().len()
    }

    /// Canonically ordered authored input handles for one-time host binding.
    pub fn surface_input_ports(
        &self,
    ) -> impl ExactSizeIterator<Item = PackageProgramSurfaceInputPortIdV1> + '_ {
        self.compiled
            .surface_input_ports()
            .iter()
            .copied()
            .map(PackageProgramSurfaceInputPortIdV1::from_core)
    }

    /// Canonically ordered opaque output slots owned by this Program.
    pub fn output_slots(&self) -> impl ExactSizeIterator<Item = PackageProgramOutputSlotIdV1> + '_ {
        self.compiled
            .outputs()
            .map(|(slot, _paint)| PackageProgramOutputSlotIdV1::from_core(slot))
    }

    /// Instantiate one stream-affine Session without exposing a generation.
    pub fn instantiate(
        &self,
        stream_id: u32,
    ) -> Result<PackageProgramSessionV1, PackageProgramInstantiateErrorV1> {
        let surface_input_ports = try_copy_surface_input_ports(&self.compiled)?;
        let output_slots = try_copy_output_slots(&self.compiled)?;
        let stream = ObservationStreamId::new(stream_id);
        let session = self
            .compiled
            .instantiate(stream)
            .map_err(PackageProgramInstantiateErrorV1::from_core)?;
        Ok(PackageProgramSessionV1 {
            stream,
            surface_input_ports,
            output_slots,
            scenario_order_scratch: Vec::new(),
            session,
        })
    }
}

/// One borrowed physical scenario in the compiled schema order.
///
/// A scenario ID is opaque provenance. `values` contains exactly one encoded
/// sRGB8 value per compiled surface input; ports are intentionally absent from
/// the hot package boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramScenarioV1<'a> {
    scenario_id: u32,
    values: &'a [Srgb8],
}

impl<'a> PackageProgramScenarioV1<'a> {
    /// Construct one simultaneous physical tuple in compiled schema order.
    pub const fn new(scenario_id: u32, values: &'a [Srgb8]) -> Self {
        Self {
            scenario_id,
            values,
        }
    }
}

/// One revision-bound package update. Stream ownership stays in the Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramUpdateV1<'a> {
    /// Correlated, schema-ordered physical scenarios.
    Observed {
        revision: u64,
        scenarios: &'a [PackageProgramScenarioV1<'a>],
    },
    /// Explicitly unavailable observation; no background is invented.
    Unknown { revision: u64, reason_id: u32 },
}

/// Concrete opaque owner of one mutable Core Program Session.
pub struct PackageProgramSessionV1 {
    stream: ObservationStreamId,
    surface_input_ports: Box<[PackageProgramSurfaceInputPortIdV1]>,
    output_slots: Box<[PackageProgramOutputSlotIdV1]>,
    scenario_order_scratch: Vec<usize>,
    session: CoreProgramSessionV1,
}

impl PackageProgramSessionV1 {
    /// Number of schema-ordered values required in every observed scenario.
    pub fn surface_input_port_count(&self) -> usize {
        self.surface_input_ports.len()
    }

    /// Canonically ordered authored input handles for one-time host binding.
    pub fn surface_input_ports(
        &self,
    ) -> impl ExactSizeIterator<Item = PackageProgramSurfaceInputPortIdV1> + '_ {
        self.surface_input_ports.iter().copied()
    }

    /// Canonically ordered opaque output slots for one-time host binding.
    pub fn output_slots(&self) -> impl ExactSizeIterator<Item = PackageProgramOutputSlotIdV1> + '_ {
        self.output_slots.iter().copied()
    }

    /// Allocation-free view of the current Core-owned lifecycle state.
    pub fn state(&self) -> PackageProgramStateViewV1<'_> {
        let revision = self.session.raw_head().revision().map(Revision::value);
        PackageProgramStateViewV1 {
            state: self.session.state(),
            revision,
            output_slots: &self.output_slots,
        }
    }

    /// Admit, evaluate and atomically commit one revision before projecting it.
    pub fn update(
        &mut self,
        update: PackageProgramUpdateV1<'_>,
    ) -> Result<PackageProgramStateViewV1<'_>, PackageProgramUpdateErrorV1> {
        match update {
            PackageProgramUpdateV1::Observed {
                revision,
                scenarios,
            } => {
                let source = PackageProgramScenarioSourceV1(scenarios);
                self.session
                    .update_schema_ordered(
                        Revision::new(revision),
                        &source,
                        &mut self.scenario_order_scratch,
                    )
                    .map_err(map_session_update_error)?;
            }
            PackageProgramUpdateV1::Unknown {
                revision,
                reason_id,
            } => {
                self.session
                    .update(ObservationUpdateInput {
                        stream: self.stream,
                        revision: Revision::new(revision),
                        payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(reason_id)),
                    })
                    .map_err(map_session_update_error)?;
            }
        }
        Ok(self.state())
    }
}

struct PackageProgramScenarioSourceV1<'a>(&'a [PackageProgramScenarioV1<'a>]);

impl SchemaOrderedScenarioSourceV1 for PackageProgramScenarioSourceV1<'_> {
    fn scenario_count(&self) -> usize {
        self.0.len()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        ScenarioId::new(self.0[scenario_index].scenario_id)
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        self.0[scenario_index].values.len()
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.0[scenario_index].values[binding_index]
    }
}

/// Closed package-visible lifecycle classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramStateKindV1 {
    Waiting,
    Ready,
    Stale,
    Failed,
}

/// Borrowed projection of one complete Core-owned lifecycle state.
#[derive(Clone, Copy)]
pub struct PackageProgramStateViewV1<'a> {
    state: &'a CoreProgramStateV1,
    revision: Option<u64>,
    output_slots: &'a [PackageProgramOutputSlotIdV1],
}

impl<'a> PackageProgramStateViewV1<'a> {
    pub const fn kind(self) -> PackageProgramStateKindV1 {
        match self.state {
            SessionState::Waiting => PackageProgramStateKindV1::Waiting,
            SessionState::Ready { .. } => PackageProgramStateKindV1::Ready,
            SessionState::Stale { .. } => PackageProgramStateKindV1::Stale,
            SessionState::Failed { .. } => PackageProgramStateKindV1::Failed,
        }
    }

    /// Current raw-head revision; only the initial Waiting state has none.
    pub const fn revision(self) -> Option<u64> {
        self.revision
    }

    /// Failed-state cause ordinal inside [`Self::certificates`].
    pub const fn cause_certificate_index(self) -> Option<usize> {
        match self.state {
            SessionState::Failed { .. } => Some(0),
            SessionState::Waiting | SessionState::Ready { .. } | SessionState::Stale { .. } => None,
        }
    }

    /// Core-owned certificates in canonical same-call ordinal order.
    pub fn certificates(
        self,
    ) -> impl ExactSizeIterator<Item = PackageProgramCertificateV1<'a>> + 'a {
        let (first, second) = match self.state {
            SessionState::Waiting => (None, None),
            SessionState::Ready { current } | SessionState::Stale { previous: current } => {
                (Some(PackageProgramCertificateV1::verified(current)), None)
            }
            SessionState::Failed { cause, previous } => (
                Some(PackageProgramCertificateV1::conflict(cause)),
                previous.as_ref().map(PackageProgramCertificateV1::verified),
            ),
        };
        PackageProgramCertificatesV1::new(first, second)
    }

    /// Total canonical output projection for this lifecycle state.
    pub fn operations(self) -> impl ExactSizeIterator<Item = PackageProgramOperationV1> + 'a {
        let inner = match self.state {
            SessionState::Waiting => PackageProgramOperationSourceV1::Empty,
            SessionState::Ready { current } => {
                debug_assert_eq!(current.outputs().len(), self.output_slots.len());
                debug_assert!(
                    current
                        .outputs()
                        .iter()
                        .zip(self.output_slots)
                        .all(|(output, slot)| output.output().value() == slot.value())
                );
                PackageProgramOperationSourceV1::Set(current.outputs().iter())
            }
            SessionState::Stale { .. } => PackageProgramOperationSourceV1::Hold {
                slots: self.output_slots.iter(),
                certificate_index: 0,
            },
            SessionState::Failed {
                previous: Some(_), ..
            } => PackageProgramOperationSourceV1::Hold {
                slots: self.output_slots.iter(),
                certificate_index: 1,
            },
            SessionState::Failed { previous: None, .. } => {
                PackageProgramOperationSourceV1::Remove(self.output_slots.iter())
            }
        };
        PackageProgramOperationsV1 { inner }
    }
}

/// Opaque certificate family; evaluator-specific evidence never escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramCertificateKindV1 {
    Verified,
    Conflict,
}

#[derive(Clone, Copy)]
enum PackageProgramCertificateRefV1<'a> {
    Verified(&'a CoreVerifiedV1),
    Conflict(&'a CoreConflictV1),
}

/// Borrowed opaque handle to one Core-owned certificate.
#[derive(Clone, Copy)]
pub struct PackageProgramCertificateV1<'a> {
    inner: PackageProgramCertificateRefV1<'a>,
}

impl<'a> PackageProgramCertificateV1<'a> {
    const fn verified(value: &'a CoreVerifiedV1) -> Self {
        Self {
            inner: PackageProgramCertificateRefV1::Verified(value),
        }
    }

    const fn conflict(value: &'a CoreConflictV1) -> Self {
        Self {
            inner: PackageProgramCertificateRefV1::Conflict(value),
        }
    }

    pub const fn kind(self) -> PackageProgramCertificateKindV1 {
        match self.inner {
            PackageProgramCertificateRefV1::Verified(_) => {
                PackageProgramCertificateKindV1::Verified
            }
            PackageProgramCertificateRefV1::Conflict(_) => {
                PackageProgramCertificateKindV1::Conflict
            }
        }
    }

    /// Revision bound into this exact evidence object.
    pub const fn revision(self) -> u64 {
        let revision = match self.inner {
            PackageProgramCertificateRefV1::Verified(value) => {
                value.report().observation().revision()
            }
            PackageProgramCertificateRefV1::Conflict(value) => {
                value.report().observation().revision()
            }
        };
        revision.value()
    }

    #[cfg(test)]
    pub(crate) fn observation_backing_ptr_for_test(self) -> *const () {
        match self.inner {
            PackageProgramCertificateRefV1::Verified(value) => {
                value.report().observation().backing_ptr_for_test()
            }
            PackageProgramCertificateRefV1::Conflict(value) => {
                value.report().observation().backing_ptr_for_test()
            }
        }
    }
}

/// Closed total operation union over opaque output slots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackageProgramOperationV1 {
    Set {
        output_slot: PackageProgramOutputSlotIdV1,
        source: Srgb8,
        opacity: f64,
        certificate_index: usize,
    },
    Remove {
        output_slot: PackageProgramOutputSlotIdV1,
    },
    Hold {
        output_slot: PackageProgramOutputSlotIdV1,
        certificate_index: usize,
    },
}

struct PackageProgramCertificatesV1<'a> {
    values: [Option<PackageProgramCertificateV1<'a>>; 2],
    index: usize,
    len: usize,
}

impl<'a> PackageProgramCertificatesV1<'a> {
    fn new(
        first: Option<PackageProgramCertificateV1<'a>>,
        second: Option<PackageProgramCertificateV1<'a>>,
    ) -> Self {
        let len = usize::from(first.is_some()) + usize::from(second.is_some());
        debug_assert!(first.is_some() || second.is_none());
        Self {
            values: [first, second],
            index: 0,
            len,
        }
    }
}

impl<'a> Iterator for PackageProgramCertificatesV1<'a> {
    type Item = PackageProgramCertificateV1<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.len {
            return None;
        }
        let value = self.values[self.index];
        self.index += 1;
        value
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PackageProgramCertificatesV1<'_> {}
impl FusedIterator for PackageProgramCertificatesV1<'_> {}

enum PackageProgramOperationSourceV1<'a> {
    Empty,
    Set(slice::Iter<'a, ProgramOutputV1>),
    Hold {
        slots: slice::Iter<'a, PackageProgramOutputSlotIdV1>,
        certificate_index: usize,
    },
    Remove(slice::Iter<'a, PackageProgramOutputSlotIdV1>),
}

struct PackageProgramOperationsV1<'a> {
    inner: PackageProgramOperationSourceV1<'a>,
}

impl Iterator for PackageProgramOperationsV1<'_> {
    type Item = PackageProgramOperationV1;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            PackageProgramOperationSourceV1::Empty => None,
            PackageProgramOperationSourceV1::Set(outputs) => {
                let output = *outputs.next()?;
                let paint = output.paint();
                Some(PackageProgramOperationV1::Set {
                    output_slot: PackageProgramOutputSlotIdV1::from_core(output.output()),
                    source: paint.source(),
                    opacity: paint.opacity().value(),
                    certificate_index: 0,
                })
            }
            PackageProgramOperationSourceV1::Hold {
                slots,
                certificate_index,
            } => Some(PackageProgramOperationV1::Hold {
                output_slot: *slots.next()?,
                certificate_index: *certificate_index,
            }),
            PackageProgramOperationSourceV1::Remove(slots) => {
                Some(PackageProgramOperationV1::Remove {
                    output_slot: *slots.next()?,
                })
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match &self.inner {
            PackageProgramOperationSourceV1::Empty => 0,
            PackageProgramOperationSourceV1::Set(outputs) => outputs.len(),
            PackageProgramOperationSourceV1::Hold { slots, .. }
            | PackageProgramOperationSourceV1::Remove(slots) => slots.len(),
        };
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PackageProgramOperationsV1<'_> {}
impl FusedIterator for PackageProgramOperationsV1<'_> {}

/// Closed package error classifications for Session construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramInstantiateErrorKindV1 {
    ResourceExhausted,
    InternalInvariant,
}

/// Opaque Session construction failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramInstantiateErrorV1 {
    kind: PackageProgramInstantiateErrorKindV1,
}

impl PackageProgramInstantiateErrorV1 {
    const fn new(kind: PackageProgramInstantiateErrorKindV1) -> Self {
        Self { kind }
    }

    fn from_core(error: ProgramSessionInstantiateError) -> Self {
        let kind = match error {
            ProgramSessionInstantiateError::ResourceExhausted => {
                PackageProgramInstantiateErrorKindV1::ResourceExhausted
            }
            ProgramSessionInstantiateError::InternalInvariant => {
                PackageProgramInstantiateErrorKindV1::InternalInvariant
            }
        };
        Self::new(kind)
    }

    pub const fn kind(self) -> PackageProgramInstantiateErrorKindV1 {
        self.kind
    }
}

impl From<PackageProgramInstantiateErrorKindV1> for PackageProgramInstantiateErrorV1 {
    fn from(kind: PackageProgramInstantiateErrorKindV1) -> Self {
        Self::new(kind)
    }
}

/// Closed package error classifications for one atomic update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProgramUpdateErrorKindV1 {
    OwnerExpired,
    InvalidObservation,
    RevisionOutOfOrder,
    RevisionConflict,
    ResourceExhausted,
    EvaluationFailed,
    InternalInvariant,
}

/// Opaque update failure. Core state is unchanged for every returned error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageProgramUpdateErrorV1 {
    kind: PackageProgramUpdateErrorKindV1,
}

impl PackageProgramUpdateErrorV1 {
    const fn new(kind: PackageProgramUpdateErrorKindV1) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> PackageProgramUpdateErrorKindV1 {
        self.kind
    }
}

fn map_joint_order_error(error: FiniteJointOrderErrorV1) -> PackageProgramJointOrderErrorV1 {
    match error {
        FiniteJointOrderErrorV1::EmptyDomain { dimension } => {
            PackageProgramJointOrderErrorV1::EmptyDomain { dimension }
        }
        FiniteJointOrderErrorV1::CardinalityOverflow => {
            PackageProgramJointOrderErrorV1::CardinalityOverflow
        }
        FiniteJointOrderErrorV1::EmptyOrder => PackageProgramJointOrderErrorV1::EmptyOrder,
        FiniteJointOrderErrorV1::TupleArity {
            tuple,
            expected,
            actual,
        } => PackageProgramJointOrderErrorV1::TupleArity {
            state: tuple,
            expected,
            actual,
        },
        FiniteJointOrderErrorV1::OrdinalOutOfDomain {
            tuple,
            dimension,
            ordinal,
            domain_len,
        } => PackageProgramJointOrderErrorV1::OrdinalOutOfDomain {
            state: tuple,
            dimension,
            ordinal,
            domain_len,
        },
        FiniteJointOrderErrorV1::DuplicateTuple { first, duplicate } => {
            PackageProgramJointOrderErrorV1::DuplicateTuple {
                first_state: first,
                duplicate_state: duplicate,
            }
        }
        FiniteJointOrderErrorV1::IncompleteOrder { expected, actual } => {
            PackageProgramJointOrderErrorV1::IncompleteOrder { expected, actual }
        }
        FiniteJointOrderErrorV1::ResourceExhausted => {
            PackageProgramJointOrderErrorV1::ResourceExhausted
        }
    }
}

fn map_program_compile_error(error: ProgramCompileError) -> PackageProgramCompileErrorV1 {
    match error {
        ProgramCompileError::DuplicateSource { source } => {
            PackageProgramCompileErrorV1::DuplicateSource {
                source: PackageProgramSourceIdV1::from_core(source),
            }
        }
        ProgramCompileError::DuplicateTarget { target } => {
            PackageProgramCompileErrorV1::DuplicateTarget {
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::MissingTargetSource { target, source } => {
            PackageProgramCompileErrorV1::MissingTargetSource {
                target: PackageProgramTargetIdV1::from_core(target),
                source: PackageProgramSourceIdV1::from_core(source),
            }
        }
        ProgramCompileError::DuplicateOpacityInput { input } => {
            PackageProgramCompileErrorV1::DuplicateOpacityInput {
                input: PackageProgramOpacityInputIdV1::from_core(input),
            }
        }
        ProgramCompileError::DuplicateSurfaceInputPort { input } => {
            PackageProgramCompileErrorV1::DuplicateSurfaceInputPort {
                input: PackageProgramSurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::UnusedSurfaceInputPort { input } => {
            PackageProgramCompileErrorV1::UnusedSurfaceInputPort {
                input: PackageProgramSurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::DuplicateSurfaceInputBinding {
            input,
            first,
            duplicate,
        } => PackageProgramCompileErrorV1::DuplicateSurfaceInputBinding {
            input: PackageProgramSurfaceInputPortIdV1::from_core(input),
            first: PackageProgramSurfaceIdV1::from_core(first),
            duplicate: PackageProgramSurfaceIdV1::from_core(duplicate),
        },
        ProgramCompileError::DuplicatePaint { paint } => {
            PackageProgramCompileErrorV1::DuplicatePaint {
                paint: PackageProgramPaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::DuplicateSurface { surface } => {
            PackageProgramCompileErrorV1::DuplicateSurface {
                surface: PackageProgramSurfaceIdV1::from_core(surface),
            }
        }
        ProgramCompileError::DuplicateOccurrence { occurrence } => {
            PackageProgramCompileErrorV1::DuplicateOccurrence {
                occurrence: PackageProgramOccurrenceIdV1::from_core(occurrence),
            }
        }
        ProgramCompileError::MissingPaintTarget { paint, target } => {
            PackageProgramCompileErrorV1::MissingPaintTarget {
                paint: PackageProgramPaintIdV1::from_core(paint),
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::MissingPaintSource { paint, source } => {
            PackageProgramCompileErrorV1::MissingPaintSource {
                paint: PackageProgramPaintIdV1::from_core(paint),
                source: PackageProgramPaintIdV1::from_core(source),
            }
        }
        ProgramCompileError::MissingPaintOpacityInput { paint, input } => {
            PackageProgramCompileErrorV1::MissingPaintOpacityInput {
                paint: PackageProgramPaintIdV1::from_core(paint),
                input: PackageProgramOpacityInputIdV1::from_core(input),
            }
        }
        ProgramCompileError::MissingSurfaceInputPort { surface, input } => {
            PackageProgramCompileErrorV1::MissingSurfaceInputPort {
                surface: PackageProgramSurfaceIdV1::from_core(surface),
                input: PackageProgramSurfaceInputPortIdV1::from_core(input),
            }
        }
        ProgramCompileError::MissingSurfaceOccurrence {
            surface,
            occurrence,
        } => PackageProgramCompileErrorV1::MissingSurfaceOccurrence {
            surface: PackageProgramSurfaceIdV1::from_core(surface),
            occurrence: PackageProgramOccurrenceIdV1::from_core(occurrence),
        },
        ProgramCompileError::MissingOccurrencePaint { occurrence, paint } => {
            PackageProgramCompileErrorV1::MissingOccurrencePaint {
                occurrence: PackageProgramOccurrenceIdV1::from_core(occurrence),
                paint: PackageProgramPaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence,
            surface,
        } => PackageProgramCompileErrorV1::MissingOccurrenceBackdrop {
            occurrence: PackageProgramOccurrenceIdV1::from_core(occurrence),
            surface: PackageProgramSurfaceIdV1::from_core(surface),
        },
        ProgramCompileError::PaintCycle { paints } => {
            PackageProgramCompileErrorV1::PaintCycle(PackageProgramPaintCycleV1 { paints })
        }
        ProgramCompileError::RenderCycle {
            surfaces,
            occurrences,
        } => PackageProgramCompileErrorV1::RenderCycle(PackageProgramRenderCycleV1 {
            surfaces,
            occurrences,
        }),
        ProgramCompileError::OpacityOutOfDomain { input } => {
            PackageProgramCompileErrorV1::OpacityOutOfDomain {
                input: PackageProgramOpacityInputIdV1::from_core(input),
            }
        }
        ProgramCompileError::EmptyTargetDomain { target } => {
            PackageProgramCompileErrorV1::EmptyTargetDomain {
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::DuplicateTargetCandidate { target, candidate } => {
            PackageProgramCompileErrorV1::DuplicateTargetCandidate {
                target: PackageProgramTargetIdV1::from_core(target),
                candidate: PackageProgramTargetCandidateIdV1::from_core(candidate),
            }
        }
        ProgramCompileError::DuplicateTargetCandidateSignal {
            target,
            first,
            duplicate,
            signal,
        } => PackageProgramCompileErrorV1::DuplicateTargetCandidateSignal {
            target: PackageProgramTargetIdV1::from_core(target),
            first: PackageProgramTargetCandidateIdV1::from_core(first),
            duplicate: PackageProgramTargetCandidateIdV1::from_core(duplicate),
            encoded_srgb8: signal.srgb8(),
        },
        ProgramCompileError::UnconstrainedTarget { target } => {
            PackageProgramCompileErrorV1::UnconstrainedTarget {
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::DisconnectedFiniteTargets => {
            PackageProgramCompileErrorV1::DisconnectedFiniteTargets
        }
        ProgramCompileError::UnassessedOutput { output, paint } => {
            PackageProgramCompileErrorV1::UnassessedOutput {
                output: PackageProgramOutputSlotIdV1::from_core(output),
                paint: PackageProgramPaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::MissingJointSelection => {
            PackageProgramCompileErrorV1::MissingJointSelection
        }
        ProgramCompileError::JointSelectionWithoutTargets => {
            PackageProgramCompileErrorV1::JointSelectionWithoutTargets
        }
        ProgramCompileError::JointStateDuplicateTarget { state, target } => {
            PackageProgramCompileErrorV1::JointStateDuplicateTarget {
                state,
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateMissingTarget { state, target } => {
            PackageProgramCompileErrorV1::JointStateMissingTarget {
                state,
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateUnknownTarget { state, target } => {
            PackageProgramCompileErrorV1::JointStateUnknownTarget {
                state,
                target: PackageProgramTargetIdV1::from_core(target),
            }
        }
        ProgramCompileError::JointStateUnknownCandidate {
            state,
            target,
            candidate,
        } => PackageProgramCompileErrorV1::JointStateUnknownCandidate {
            state,
            target: PackageProgramTargetIdV1::from_core(target),
            candidate: PackageProgramTargetCandidateIdV1::from_core(candidate),
        },
        ProgramCompileError::InvalidJointOrder(error) => {
            PackageProgramCompileErrorV1::InvalidJointOrder(map_joint_order_error(error))
        }
        ProgramCompileError::EmptyObservationGroup { .. } => {
            PackageProgramCompileErrorV1::EmptySurfaceInputPortSet
        }
        ProgramCompileError::EmptyOccurrenceSet => PackageProgramCompileErrorV1::EmptyOccurrenceSet,
        ProgramCompileError::EmptyConstraintSet => PackageProgramCompileErrorV1::EmptyConstraintSet,
        ProgramCompileError::EmptyOutputSet => PackageProgramCompileErrorV1::EmptyOutputSet,
        ProgramCompileError::DuplicateConstraint { constraint } => {
            PackageProgramCompileErrorV1::DuplicateConstraint {
                constraint: PackageProgramConstraintIdV1::from_core(constraint),
            }
        }
        ProgramCompileError::MissingConstraintOccurrence {
            constraint,
            occurrence,
        } => PackageProgramCompileErrorV1::MissingConstraintOccurrence {
            constraint: PackageProgramConstraintIdV1::from_core(constraint),
            occurrence: PackageProgramOccurrenceIdV1::from_core(occurrence),
        },
        ProgramCompileError::DuplicateOutputSlot { output } => {
            PackageProgramCompileErrorV1::DuplicateOutputSlot {
                output: PackageProgramOutputSlotIdV1::from_core(output),
            }
        }
        ProgramCompileError::MissingOutputPaint { output, paint } => {
            PackageProgramCompileErrorV1::MissingOutputPaint {
                output: PackageProgramOutputSlotIdV1::from_core(output),
                paint: PackageProgramPaintIdV1::from_core(paint),
            }
        }
        ProgramCompileError::ResourceExhausted => PackageProgramCompileErrorV1::ResourceExhausted,
        ProgramCompileError::InternalInvariant => PackageProgramCompileErrorV1::InternalInvariant,
    }
}
fn try_copy_surface_input_ports(
    compiled: &CompiledCoreProgramV1,
) -> Result<Box<[PackageProgramSurfaceInputPortIdV1]>, PackageProgramInstantiateErrorV1> {
    let inputs = compiled.surface_input_ports();
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(inputs.len())
        .map_err(|_| PackageProgramInstantiateErrorKindV1::ResourceExhausted)?;
    copied.extend(
        inputs
            .iter()
            .copied()
            .map(PackageProgramSurfaceInputPortIdV1::from_core),
    );
    Ok(copied.into_boxed_slice())
}

fn try_copy_output_slots(
    compiled: &CompiledCoreProgramV1,
) -> Result<Box<[PackageProgramOutputSlotIdV1]>, PackageProgramInstantiateErrorV1> {
    let outputs = compiled.outputs();
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(outputs.len())
        .map_err(|_| PackageProgramInstantiateErrorKindV1::ResourceExhausted)?;
    copied.extend(outputs.map(|(slot, _paint)| PackageProgramOutputSlotIdV1::from_core(slot)));
    Ok(copied.into_boxed_slice())
}

fn map_session_update_error(
    error: SessionUpdateError<CoreProgramPlanErrorV1>,
) -> PackageProgramUpdateErrorV1 {
    let kind = match error {
        SessionUpdateError::OwnerExpired => PackageProgramUpdateErrorKindV1::OwnerExpired,
        SessionUpdateError::Observation(error) => map_observation_error(error),
        SessionUpdateError::Plan(error) => map_plan_error(error),
        SessionUpdateError::EvidenceBindingInvariant => {
            PackageProgramUpdateErrorKindV1::InternalInvariant
        }
    };
    PackageProgramUpdateErrorV1::new(kind)
}

fn map_observation_error(error: ObservationError) -> PackageProgramUpdateErrorKindV1 {
    match error {
        ObservationError::EmptyScenarioSet
        | ObservationError::DuplicateScenarioId { .. }
        | ObservationError::SchemaOrderedValueCountMismatch { .. } => {
            PackageProgramUpdateErrorKindV1::InvalidObservation
        }
        ObservationError::RevisionOutOfOrder { .. } => {
            PackageProgramUpdateErrorKindV1::RevisionOutOfOrder
        }
        ObservationError::RevisionConflict { .. } => {
            PackageProgramUpdateErrorKindV1::RevisionConflict
        }
        ObservationError::ResourceExhausted => PackageProgramUpdateErrorKindV1::ResourceExhausted,
        ObservationError::EmptyCompiledSurfaceInputSchema
        | ObservationError::DuplicateCompiledSurfaceInputPort { .. }
        | ObservationError::StreamMismatch { .. }
        | ObservationError::DuplicateSurfaceInputBinding { .. }
        | ObservationError::MissingSurfaceInputBinding { .. }
        | ObservationError::UnexpectedSurfaceInputBinding { .. } => {
            PackageProgramUpdateErrorKindV1::InternalInvariant
        }
    }
}

fn map_plan_error(error: CoreProgramPlanErrorV1) -> PackageProgramUpdateErrorKindV1 {
    match error {
        ProgramSessionEvaluationError::ResourceExhausted => {
            PackageProgramUpdateErrorKindV1::ResourceExhausted
        }
        ProgramSessionEvaluationError::Evaluator { .. } => {
            PackageProgramUpdateErrorKindV1::EvaluationFailed
        }
        ProgramSessionEvaluationError::ObservationSchemaMismatch(_)
        | ProgramSessionEvaluationError::ProgramTargetBinding { .. }
        | ProgramSessionEvaluationError::ModeledOccurrence { .. }
        | ProgramSessionEvaluationError::OutputVariesAcrossCases { .. }
        | ProgramSessionEvaluationError::FinalRecheckViolation { .. }
        | ProgramSessionEvaluationError::InternalInvariant => {
            PackageProgramUpdateErrorKindV1::InternalInvariant
        }
    }
}

#[cfg(test)]
mod compile_error_projection_tests {
    use super::*;

    #[test]
    fn nested_joint_resource_exhaustion_keeps_its_exact_reason_and_site_kind() {
        let error = map_program_compile_error(ProgramCompileError::InvalidJointOrder(
            FiniteJointOrderErrorV1::ResourceExhausted,
        ));

        assert_eq!(
            error.kind(),
            PackageProgramCompileErrorKindV1::InvalidJointOrder
        );
        assert_eq!(
            error,
            PackageProgramCompileErrorV1::InvalidJointOrder(
                PackageProgramJointOrderErrorV1::ResourceExhausted
            )
        );
    }
}
