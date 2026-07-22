//! Sole concrete package projection for a compiled Core Program Session.
//!
//! This hidden module is deliberately narrower than the authored Program IR:
//! it exposes no evaluator trait, generic Session plan, threshold, candidate
//! domain, client vocabulary, transport word, or lifecycle generation. The
//! package adapter supplies schema-ordered physical scenarios and receives a
//! borrowed, allocation-free projection of Core-owned state and evidence.

use core::iter::FusedIterator;
use core::slice;

use crate::Srgb8;
use crate::observation::{
    ObservationError, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    Revision, ScenarioId, SchemaOrderedScenarioSourceV1, UnknownReasonId,
};
use crate::program_session::{
    CompiledCoreProgramV1, CoreProgramEvaluatorErrorV1, CoreProgramEvaluatorsV1, ProgramConflictV1,
    ProgramOutputV1, ProgramSessionEvaluationError, ProgramSessionInstantiateError,
    ProgramSessionPlan, ProgramVerifiedV1,
};
use crate::session::{Session, SessionState, SessionUpdateError};

type CoreVerifiedV1 = ProgramVerifiedV1<CoreProgramEvaluatorsV1>;
type CoreConflictV1 = ProgramConflictV1<CoreProgramEvaluatorsV1>;
type CoreProgramPlanV1 = ProgramSessionPlan<CoreProgramEvaluatorsV1>;
type CoreProgramSessionV1 = Session<CoreProgramPlanV1>;
type CoreProgramStateV1 = SessionState<CoreVerifiedV1, CoreConflictV1>;
type CoreProgramPlanErrorV1 = ProgramSessionEvaluationError<CoreProgramEvaluatorErrorV1>;

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
    /// Internal handoff from the canonical Core lowerer. Keeping this
    /// constructor crate-private prevents an adapter-authored Program dialect.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the canonical package lowerer is linked in the following stacked slice"
        )
    )]
    pub(crate) const fn from_compiled(compiled: CompiledCoreProgramV1) -> Self {
        Self { compiled }
    }

    /// Number of schema-ordered surface values required in every scenario.
    pub fn surface_input_count(&self) -> usize {
        self.compiled.surface_input_ports().len()
    }

    /// Canonically ordered opaque output slots owned by this Program.
    pub fn output_slots(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.compiled.outputs().map(|(slot, _paint)| slot.value())
    }

    /// Instantiate one stream-affine Session without exposing a generation.
    pub fn instantiate(
        &self,
        stream_id: u32,
    ) -> Result<PackageProgramSessionV1, PackageProgramInstantiateErrorV1> {
        let surface_input_count = self.compiled.surface_input_ports().len();
        let output_slots = try_copy_output_slots(&self.compiled)?;
        let stream = ObservationStreamId::new(stream_id);
        let session = self
            .compiled
            .instantiate(stream)
            .map_err(PackageProgramInstantiateErrorV1::from_core)?;
        Ok(PackageProgramSessionV1 {
            stream,
            surface_input_count,
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
    surface_input_count: usize,
    output_slots: Box<[u32]>,
    scenario_order_scratch: Vec<usize>,
    session: CoreProgramSessionV1,
}

impl PackageProgramSessionV1 {
    /// Number of schema-ordered values required in every observed scenario.
    pub fn surface_input_count(&self) -> usize {
        self.surface_input_count
    }

    /// Canonically ordered opaque output slots for one-time host binding.
    pub fn output_slots(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
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
    output_slots: &'a [u32],
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
                        .all(|(output, slot)| output.output().value() == *slot)
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
        output_slot: u32,
        source: Srgb8,
        opacity: f64,
        certificate_index: usize,
    },
    Remove {
        output_slot: u32,
    },
    Hold {
        output_slot: u32,
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
        slots: slice::Iter<'a, u32>,
        certificate_index: usize,
    },
    Remove(slice::Iter<'a, u32>),
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
                    output_slot: output.output().value(),
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

fn try_copy_output_slots(
    compiled: &CompiledCoreProgramV1,
) -> Result<Box<[u32]>, PackageProgramInstantiateErrorV1> {
    let outputs = compiled.outputs();
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(outputs.len())
        .map_err(|_| PackageProgramInstantiateErrorKindV1::ResourceExhausted)?;
    copied.extend(outputs.map(|(slot, _paint)| slot.value()));
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
