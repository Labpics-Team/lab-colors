//! Terminal generation-bound path prepared for the atomic public cut.
//!
//! [`Program`] is the authored, typed Paint/Surface/Occurrence declaration.
//! [`Program::compile`] validates and canonicalises the complete graph before a
//! [`CompiledProgram`] can exist. [`PointRenderOwner`] then becomes the sole
//! strong owner of one compiled epoch. Attached [`Session`] values retain only
//! a [`Weak`] reference, so replacement, disposal or owner drop makes the old
//! graph physically unreachable from every old session.
//! The crate root keeps this path private until the atomic public-surface cut;
//! it must not create a second simultaneously supported authoring schema.
//!
//! The first executable transport is intentionally narrow: one correlated set
//! of encoded Surface input signals per revision. It is transport-only state,
//! not an observed stimulus, physical evidence or certificate. F0
//! observer/output/render identities remain a terminal prerequisite before any
//! such claim can be minted. Expanding the private transport to a ScenarioSet
//! does not require exposing the legacy multi-background metric matrix.
//! In particular, the wire magic is not an `lcs` or physical identity.

use std::mem;
use std::rc::{Rc, Weak};

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, AppearanceWorkspace,
    BindingError, ColorInputId, CompileError, CompiledAppearanceGraph, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::composition::CompositionProfileV1;

/// One immutable encoded colour binding owned by a [`Program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorInput {
    id: ColorInputId,
    value: Srgb8,
}

impl ColorInput {
    /// Bind one opaque input identity to exact encoded-sRGB8 bytes.
    pub const fn new(id: ColorInputId, value: Srgb8) -> Self {
        Self { id, value }
    }

    /// Return the opaque input identity.
    pub const fn id(self) -> ColorInputId {
        self.id
    }

    /// Return the exact immutable value.
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
    /// Bind one opaque input identity to a finite value in `[0, 1]`.
    ///
    /// Numeric admission happens atomically in [`Program::compile`].
    pub const fn new(id: OpacityInputId, value: f64) -> Self {
        Self { id, value }
    }

    /// Return the opaque input identity.
    pub const fn id(self) -> OpacityInputId {
        self.id
    }

    /// Return the authored binary64 value.
    pub const fn value(self) -> f64 {
        self.value
    }
}

/// Generic Paint constructor algebra supported by the point renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Create an opaque Paint from exact encoded bytes.
    Solid { id: PaintId, color: ColorInputId },
    /// Multiply a Paint's straight alpha by one admitted scalar.
    Opacity {
        id: PaintId,
        source: PaintId,
        opacity: OpacityInputId,
    },
}

/// Generic Surface constructor algebra supported by the point renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Read one revision-bound runtime point input.
    Input {
        id: SurfaceId,
        input: SurfaceInputPortId,
    },
    /// Give a visible occurrence result a Surface identity for nesting.
    FromOccurrence {
        id: SurfaceId,
        occurrence: OccurrenceId,
    },
}

/// Closed mathematical composition profile set for this point-program version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionProfile {
    /// Exact encoded-sRGB8 source-over with its declared byte rounding order.
    EncodedSrgb8SourceOverV1,
}

/// The only canonical application of one Paint to one Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    id: OccurrenceId,
    subject: PaintId,
    against: SurfaceId,
    composition: CompositionProfile,
}

impl Occurrence {
    /// Declare one Paint-on-Surface application.
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

    /// Return the occurrence identity.
    pub const fn id(self) -> OccurrenceId {
        self.id
    }

    /// Return the subject Paint identity.
    pub const fn subject(self) -> PaintId {
        self.subject
    }

    /// Return the backdrop Surface identity.
    pub const fn against(self) -> SurfaceId {
        self.against
    }

    /// Return the exact mathematical composition profile.
    pub const fn composition(self) -> CompositionProfile {
        self.composition
    }
}

/// Immutable generic point-render declaration.
///
/// List order carries no semantics. Compilation canonicalises every typed ID
/// domain and rejects dangling edges, duplicates, cycles and invalid numeric
/// inputs before any runtime owner can be constructed.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    colors: Vec<ColorInput>,
    surface_input_ports: Vec<SurfaceInputPortId>,
    opacities: Vec<OpacityInput>,
    paints: Vec<Paint>,
    surfaces: Vec<Surface>,
    occurrences: Vec<Occurrence>,
}

impl Program {
    /// Assemble one declaration. This constructor performs no partial compile.
    pub fn new(
        colors: Vec<ColorInput>,
        surface_input_ports: Vec<SurfaceInputPortId>,
        opacities: Vec<OpacityInput>,
        paints: Vec<Paint>,
        surfaces: Vec<Surface>,
        occurrences: Vec<Occurrence>,
    ) -> Self {
        Self {
            colors,
            surface_input_ports,
            opacities,
            paints,
            surfaces,
            occurrences,
        }
    }

    /// Atomically validate, bind and canonicalise this complete declaration.
    pub fn compile(self) -> Result<CompiledProgram, ProgramCompileError> {
        prepare_program(self).map(|epoch| CompiledProgram { epoch })
    }
}

/// Public compile failure; every variant leaves no executable partial graph.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    EmptySurfaceSchema,
    EmptyOccurrenceSet,
    ResourceExhausted,
    InternalInvariant,
}

/// ASCII `LCR1`: code-owned Lab Colors Render transport version 1.
///
/// This is a wire discriminator, not an LCS, context or physical identity.
pub(crate) const PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1: u32 = 0x4c43_5231;
pub(crate) const PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1: u32 = 0;
pub(crate) const PACKED_ENCODED_SURFACE_PRESENT_TAG_V1: u32 = 1;
const PACKED_ENCODED_SURFACE_HEADER_WORDS_V1: usize = 4;
const PACKED_SURFACE_UNAVAILABLE_WORDS_V1: usize = 5;

#[derive(Debug)]
struct ProgramEpochV1 {
    graph: CompiledAppearanceGraph,
    binding_template: AdmittedAppearanceBindings,
    surface_input_ports: Box<[SurfaceInputPortId]>,
    occurrence_ids: Box<[OccurrenceId]>,
}

/// Fully validated immutable point-render program, not yet attached to runtime.
///
/// This value is deliberately not `Clone`: moving it into an owner establishes
/// one unambiguous strong-ownership root for its compiled epoch.
#[derive(Debug)]
pub struct CompiledProgram {
    epoch: ProgramEpochV1,
}

impl CompiledProgram {
    /// Canonical Surface-input order required by [`SurfaceUpdate::Present`].
    pub fn surface_input_ports(&self) -> &[SurfaceInputPortId] {
        &self.epoch.surface_input_ports
    }

    /// Canonical occurrence order emitted by every [`Snapshot`].
    pub fn occurrences(&self) -> &[OccurrenceId] {
        &self.epoch.occurrence_ids
    }

    /// Transfer this compiled epoch to its sole runtime owner.
    pub fn into_owner(self) -> PointRenderOwner {
        PointRenderOwner::new(self)
    }
}

/// Failure while preparing an independent allocation-owning [`Session`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointRenderAttachError {
    Disposed,
    ResourceExhausted,
    InternalInvariant,
}

/// The only strong owner of the current non-reusable program epoch.
///
/// Replacement accepts only an already complete [`CompiledProgram`]. Compile
/// failure therefore happens before the swap and cannot revoke the live epoch.
/// No numeric generation participates in this ownership proof.
#[derive(Debug)]
pub struct PointRenderOwner {
    current: Option<Rc<ProgramEpochV1>>,
}

impl PointRenderOwner {
    /// Establish the sole strong owner of one compiled epoch.
    pub fn new(compiled: CompiledProgram) -> Self {
        Self {
            current: Some(Rc::new(compiled.epoch)),
        }
    }

    /// Atomically replace the current epoch and revoke all attached old sessions.
    pub fn replace(&mut self, compiled: CompiledProgram) {
        self.current = Some(Rc::new(compiled.epoch));
    }

    /// Revoke the current epoch. Existing sessions fail on their next call.
    pub fn dispose(&mut self) {
        self.current = None;
    }

    /// Return the current canonical Surface-input order, or `None` if disposed.
    pub fn surface_input_ports(&self) -> Option<&[SurfaceInputPortId]> {
        self.current
            .as_deref()
            .map(|epoch| epoch.surface_input_ports.as_ref())
    }

    /// Return the current canonical occurrence order, or `None` if disposed.
    pub fn occurrences(&self) -> Option<&[OccurrenceId]> {
        self.current
            .as_deref()
            .map(|epoch| epoch.occurrence_ids.as_ref())
    }

    /// Allocate all independent mutable storage before a Session escapes.
    pub fn attach(&self) -> Result<Session, PointRenderAttachError> {
        let epoch = self
            .current
            .as_ref()
            .ok_or(PointRenderAttachError::Disposed)?;
        let workspace = epoch
            .graph
            .new_workspace()
            .map_err(map_attach_binding_error)?;
        let bindings = epoch
            .binding_template
            .try_clone_v1()
            .map_err(map_attach_binding_error)?;
        let initial_signal_buffers =
            CompositedSignalBuffersV1::try_new(&epoch.surface_input_ports, &epoch.occurrence_ids)?;
        Ok(Session {
            epoch: Rc::downgrade(epoch),
            bindings,
            workspace,
            initial_signal_buffers: Some(initial_signal_buffers),
            state: SessionState::Waiting {
                current_unavailable: None,
            },
        })
    }
}

fn map_attach_binding_error(error: BindingError) -> PointRenderAttachError {
    match error {
        BindingError::ResourceExhausted => PointRenderAttachError::ResourceExhausted,
        _ => PointRenderAttachError::InternalInvariant,
    }
}

fn try_zeroed_signal_words(len: usize) -> Result<Vec<u32>, PointRenderAttachError> {
    let mut words = Vec::new();
    words
        .try_reserve_exact(len)
        .map_err(|_| PointRenderAttachError::ResourceExhausted)?;
    words.resize(len, 0);
    Ok(words)
}

fn prepare_program(program: Program) -> Result<ProgramEpochV1, ProgramCompileError> {
    if program.surface_input_ports.is_empty() {
        return Err(ProgramCompileError::EmptySurfaceSchema);
    }
    if program.occurrences.is_empty() {
        return Err(ProgramCompileError::EmptyOccurrenceSet);
    }
    check_render_node_count(program.surfaces.len(), program.occurrences.len())?;

    let graph = lower_graph(&program).compile().map_err(map_compile_error)?;

    let mut surface_input_ports = Vec::new();
    surface_input_ports
        .try_reserve_exact(program.surface_input_ports.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    surface_input_ports.extend_from_slice(&program.surface_input_ports);
    surface_input_ports.sort_unstable();

    let mut occurrence_ids = Vec::new();
    occurrence_ids
        .try_reserve_exact(program.occurrences.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    occurrence_ids.extend(program.occurrences.iter().map(|occurrence| occurrence.id));
    occurrence_ids.sort_unstable();

    if !canonical_surface_input_port_sequence_matches(
        graph.surface_input_ports(),
        &surface_input_ports,
    ) || !canonical_occurrence_sequence_matches(graph.occurrence_ids(), &occurrence_ids)
    {
        return Err(ProgramCompileError::InternalInvariant);
    }

    let bindings = lower_bindings(&program);
    let binding_template = graph
        .admit_bindings(&bindings)
        .map_err(map_binding_compile_error)?;
    Ok(ProgramEpochV1 {
        graph,
        binding_template,
        surface_input_ports: surface_input_ports.into_boxed_slice(),
        occurrence_ids: occurrence_ids.into_boxed_slice(),
    })
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

pub(crate) fn canonical_occurrence_sequence_matches(
    actual: impl IntoIterator<Item = OccurrenceId>,
    expected: &[OccurrenceId],
) -> bool {
    actual.into_iter().eq(expected.iter().copied())
}

fn lower_graph(program: &Program) -> AppearanceGraphSpec {
    AppearanceGraphSpec::new(
        program.colors.iter().map(|input| input.id).collect(),
        program.surface_input_ports.clone(),
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

fn lower_bindings(program: &Program) -> AppearanceBindings {
    AppearanceBindings::new(
        program
            .colors
            .iter()
            .map(|input| (input.id, input.value))
            .collect(),
        program
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

/// One revision-bound absence of the correlated Surface-input set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceUnavailable {
    revision: u64,
    reason: u32,
}

impl SurfaceUnavailable {
    /// Return the stream revision carrying this absence.
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Return the client-owned opaque absence reason.
    pub const fn reason(self) -> u32 {
        self.reason
    }
}

/// One exact runtime Surface-input value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSignal {
    input: SurfaceInputPortId,
    value: Srgb8,
}

impl SurfaceSignal {
    /// Bind one runtime input identity to exact encoded bytes.
    pub const fn new(input: SurfaceInputPortId, value: Srgb8) -> Self {
        Self { input, value }
    }

    /// Return the runtime input identity.
    pub const fn input(self) -> SurfaceInputPortId {
        self.input
    }

    /// Return the exact encoded value.
    pub const fn value(self) -> Srgb8 {
        self.value
    }
}

/// One exact visible value emitted for a compiled occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OccurrenceSignal {
    occurrence: OccurrenceId,
    value: Srgb8,
}

impl OccurrenceSignal {
    /// Return the compiled occurrence identity.
    pub const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    /// Return the exact encoded visible value.
    pub const fn value(self) -> Srgb8 {
        self.value
    }
}

/// Borrowed, correlated runtime update for one attached Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceUpdate<'input> {
    /// The entire Surface-input set is unavailable at this revision.
    Unavailable { revision: u64, reason: u32 },
    /// The complete input set in [`CompiledProgram::surface_input_ports`] order.
    Present {
        revision: u64,
        surfaces: &'input [SurfaceSignal],
    },
}

/// Compact committed value: one word per compiled occurrence, in the graph's
/// canonical occurrence order. No metric, threshold or JS-derived verdict is
/// present on this boundary.
#[derive(Debug, PartialEq, Eq)]
struct CompositedSignalBuffersV1 {
    surface_input_ports: Box<[SurfaceInputPortId]>,
    input_surface_signals_rgb24: Vec<u32>,
    occurrence_ids: Box<[OccurrenceId]>,
    composited_occurrence_signals_rgb24: Vec<u32>,
}

impl CompositedSignalBuffersV1 {
    fn try_new(
        surface_input_ports: &[SurfaceInputPortId],
        occurrence_ids: &[OccurrenceId],
    ) -> Result<Self, PointRenderAttachError> {
        Ok(Self {
            surface_input_ports: try_copy_ids(surface_input_ports)?,
            input_surface_signals_rgb24: try_zeroed_signal_words(surface_input_ports.len())?,
            occurrence_ids: try_copy_ids(occurrence_ids)?,
            composited_occurrence_signals_rgb24: try_zeroed_signal_words(occurrence_ids.len())?,
        })
    }
}

fn try_copy_ids<T: Copy>(values: &[T]) -> Result<Box<[T]>, PointRenderAttachError> {
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(values.len())
        .map_err(|_| PointRenderAttachError::ResourceExhausted)?;
    copied.extend_from_slice(values);
    Ok(copied.into_boxed_slice())
}

/// One committed, revision-bound point-render result.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    revision: u64,
    buffers: CompositedSignalBuffersV1,
}

impl Snapshot {
    /// Return the exact revision used for every input and output in this value.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Iterate admitted inputs in canonical compiled order without allocation.
    pub fn surfaces(&self) -> impl ExactSizeIterator<Item = SurfaceSignal> + '_ {
        self.buffers
            .surface_input_ports
            .iter()
            .copied()
            .zip(self.buffers.input_surface_signals_rgb24.iter().copied())
            .map(|(input, value)| SurfaceSignal {
                input,
                value: Srgb8::new(unpack_rgb24(value)),
            })
    }

    /// Iterate visible occurrence values in canonical compiled order.
    pub fn occurrences(&self) -> impl ExactSizeIterator<Item = OccurrenceSignal> + '_ {
        self.buffers
            .occurrence_ids
            .iter()
            .copied()
            .zip(
                self.buffers
                    .composited_occurrence_signals_rgb24
                    .iter()
                    .copied(),
            )
            .map(|(occurrence, value)| OccurrenceSignal {
                occurrence,
                value: Srgb8::new(unpack_rgb24(value)),
            })
    }

    /// Look up one canonical occurrence output without allocation.
    pub fn occurrence(&self, occurrence: OccurrenceId) -> Option<Srgb8> {
        self.buffers
            .occurrence_ids
            .binary_search(&occurrence)
            .ok()
            .map(|index| {
                Srgb8::new(unpack_rgb24(
                    self.buffers.composited_occurrence_signals_rgb24[index],
                ))
            })
    }

    pub(crate) fn input_surface_signals_rgb24(&self) -> &[u32] {
        &self.buffers.input_surface_signals_rgb24
    }

    pub(crate) fn composited_occurrence_signals_rgb24(&self) -> &[u32] {
        &self.buffers.composited_occurrence_signals_rgb24
    }
}

/// Current state of one generation-bound Session.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionState {
    Waiting {
        current_unavailable: Option<SurfaceUnavailable>,
    },
    Ready {
        current: Snapshot,
    },
    Stale {
        previous: Snapshot,
        current_unavailable: SurfaceUnavailable,
    },
}

impl SessionState {
    const fn head_revision(&self) -> Option<u64> {
        match self {
            Self::Waiting {
                current_unavailable: None,
            } => None,
            Self::Waiting {
                current_unavailable: Some(unavailable),
            }
            | Self::Stale {
                current_unavailable: unavailable,
                ..
            } => Some(unavailable.revision),
            Self::Ready { current } => Some(current.revision),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackedEncodedSurfaceUpdateErrorV1 {
    HeaderTooShort,
    MagicMismatch { actual: u32 },
    UnsupportedTag { actual: u32 },
    LengthMismatch { expected: usize, actual: usize },
    ReservedSignalByteNonZero { surface_index: usize, value: u32 },
    RevisionOutOfOrder { current: u64, incoming: u64 },
    RevisionConflict { revision: u64 },
    ResourceExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointRenderSessionUpdateErrorV1 {
    ProgramExpired,
    EncodedSurfaceUpdate(PackedEncodedSurfaceUpdateErrorV1),
    Evaluation(BindingError),
}

/// Failure to admit or execute one typed Session update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionUpdateError {
    ProgramExpired,
    SurfaceInputPortLengthMismatch {
        expected: usize,
        actual: usize,
    },
    SurfaceInputPortMismatch {
        index: usize,
        expected: SurfaceInputPortId,
        actual: SurfaceInputPortId,
    },
    RevisionOutOfOrder {
        current: u64,
        incoming: u64,
    },
    RevisionConflict {
        revision: u64,
    },
    InternalInvariant,
}

enum PreparedSurfaceValuesV1<'input> {
    Typed(&'input [SurfaceSignal]),
    PackedRgb24(&'input [u32]),
}

impl PreparedSurfaceValuesV1<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Typed(values) => values.len(),
            Self::PackedRgb24(values) => values.len(),
        }
    }

    fn value(&self, index: usize) -> Srgb8 {
        match self {
            Self::Typed(values) => values[index].value,
            Self::PackedRgb24(values) => Srgb8::new(unpack_rgb24(values[index])),
        }
    }

    fn matches_rgb24(&self, expected: &[u32]) -> bool {
        self.len() == expected.len()
            && expected
                .iter()
                .enumerate()
                .all(|(index, &word)| pack_rgb24(self.value(index).bytes()) == word)
    }
}

enum PreparedEncodedSurfaceUpdateV1<'input> {
    Unavailable(SurfaceUnavailable),
    Present {
        revision: u64,
        surfaces: PreparedSurfaceValuesV1<'input>,
    },
}

/// Generation-bound mutable runtime. It owns reusable values/scratch, never a
/// strong reference or a copy of the compiled graph. All fixed-cardinality
/// signal buffers are allocated fallibly by `attach`; updates only move and
/// overwrite their ownership after evaluation succeeds.
#[derive(Debug)]
pub struct Session {
    epoch: Weak<ProgramEpochV1>,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
    initial_signal_buffers: Option<CompositedSignalBuffersV1>,
    state: SessionState,
}

impl Session {
    /// Borrow the current committed state.
    pub const fn state(&self) -> &SessionState {
        &self.state
    }

    /// Admit, evaluate and atomically commit one typed Surface-input update.
    pub fn update(
        &mut self,
        update: SurfaceUpdate<'_>,
    ) -> Result<&SessionState, SessionUpdateError> {
        let epoch = self
            .epoch
            .upgrade()
            .ok_or(SessionUpdateError::ProgramExpired)?;
        let prepared = match update {
            SurfaceUpdate::Unavailable { revision, reason } => {
                PreparedEncodedSurfaceUpdateV1::Unavailable(SurfaceUnavailable { revision, reason })
            }
            SurfaceUpdate::Present { revision, surfaces } => {
                if surfaces.len() != epoch.surface_input_ports.len() {
                    return Err(SessionUpdateError::SurfaceInputPortLengthMismatch {
                        expected: epoch.surface_input_ports.len(),
                        actual: surfaces.len(),
                    });
                }
                for (index, (&expected, actual)) in epoch
                    .surface_input_ports
                    .iter()
                    .zip(surfaces.iter())
                    .enumerate()
                {
                    if actual.input != expected {
                        return Err(SessionUpdateError::SurfaceInputPortMismatch {
                            index,
                            expected,
                            actual: actual.input,
                        });
                    }
                }
                PreparedEncodedSurfaceUpdateV1::Present {
                    revision,
                    surfaces: PreparedSurfaceValuesV1::Typed(surfaces),
                }
            }
        };
        self.apply_prepared(&epoch, prepared)
            .map_err(map_session_update_error)
    }

    /// Private allocation-free packed bridge for the WASM boundary.
    pub(crate) fn update_packed(
        &mut self,
        words: &[u32],
    ) -> Result<&SessionState, PointRenderSessionUpdateErrorV1> {
        let epoch = self
            .epoch
            .upgrade()
            .ok_or(PointRenderSessionUpdateErrorV1::ProgramExpired)?;
        let prepared = decode_encoded_surface_update(words, epoch.surface_input_ports.len())
            .map_err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate)?;
        self.apply_prepared(&epoch, prepared)
    }

    fn apply_prepared(
        &mut self,
        epoch: &ProgramEpochV1,
        prepared: PreparedEncodedSurfaceUpdateV1<'_>,
    ) -> Result<&SessionState, PointRenderSessionUpdateErrorV1> {
        let incoming_revision = match &prepared {
            PreparedEncodedSurfaceUpdateV1::Unavailable(unavailable) => unavailable.revision,
            PreparedEncodedSurfaceUpdateV1::Present { revision, .. } => *revision,
        };

        if let Some(current) = self.state.head_revision() {
            if incoming_revision < current {
                return Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
                    PackedEncodedSurfaceUpdateErrorV1::RevisionOutOfOrder {
                        current,
                        incoming: incoming_revision,
                    },
                ));
            }
            if incoming_revision == current {
                return self.admit_same_revision(prepared);
            }
        }

        match prepared {
            PreparedEncodedSurfaceUpdateV1::Unavailable(unavailable) => {
                let previous = take_last_ready(&mut self.state);
                self.state = match previous {
                    Some(previous) => SessionState::Stale {
                        previous,
                        current_unavailable: unavailable,
                    },
                    None => SessionState::Waiting {
                        current_unavailable: Some(unavailable),
                    },
                };
            }
            PreparedEncodedSurfaceUpdateV1::Present { revision, surfaces } => {
                let retained_shape_matches = match &self.state {
                    SessionState::Waiting { .. } => {
                        self.initial_signal_buffers.as_ref().is_some_and(|buffers| {
                            buffers.input_surface_signals_rgb24.len()
                                == epoch.surface_input_ports.len()
                                && buffers.composited_occurrence_signals_rgb24.len()
                                    == epoch.occurrence_ids.len()
                        })
                    }
                    SessionState::Ready { current } => {
                        current.buffers.input_surface_signals_rgb24.len()
                            == epoch.surface_input_ports.len()
                            && current.buffers.composited_occurrence_signals_rgb24.len()
                                == epoch.occurrence_ids.len()
                    }
                    SessionState::Stale { previous, .. } => {
                        previous.buffers.input_surface_signals_rgb24.len()
                            == epoch.surface_input_ports.len()
                            && previous.buffers.composited_occurrence_signals_rgb24.len()
                                == epoch.occurrence_ids.len()
                    }
                };
                if !retained_shape_matches {
                    return Err(PointRenderSessionUpdateErrorV1::Evaluation(
                        BindingError::IncompatibleWorkspace,
                    ));
                }

                // Decode admitted the exact epoch-owned cardinality and every
                // word before this loop. Each typed port therefore exists in
                // the cloned admitted schema; setters cannot partially reject
                // a later element. These mutable values are scratch only and
                // are not published until the final state replacement below.
                for (index, &port) in epoch.surface_input_ports.iter().enumerate() {
                    self.bindings
                        .set_surface_input(port, surfaces.value(index))
                        .map_err(PointRenderSessionUpdateErrorV1::Evaluation)?;
                }
                let evaluation = epoch
                    .graph
                    .evaluate_admitted_into(&self.bindings, &mut self.workspace)
                    .map_err(PointRenderSessionUpdateErrorV1::Evaluation)?;
                if evaluation.occurrences().len() != epoch.occurrence_ids.len() {
                    return Err(PointRenderSessionUpdateErrorV1::Evaluation(
                        BindingError::IncompatibleWorkspace,
                    ));
                }

                // No fallible work follows. Preserve the committed snapshot
                // through decode, binding mutation and evaluation; only now
                // reclaim the one fixed buffer pair and overwrite it.
                let mut buffers = match take_last_ready(&mut self.state) {
                    Some(previous) => previous.buffers,
                    None => self.initial_signal_buffers.take().unwrap_or_else(|| {
                        unreachable!("a Session without prior Ready must retain initial buffers")
                    }),
                };
                debug_assert_eq!(buffers.input_surface_signals_rgb24.len(), surfaces.len());
                debug_assert_eq!(
                    buffers.composited_occurrence_signals_rgb24.len(),
                    epoch.occurrence_ids.len()
                );
                for (index, output) in buffers.input_surface_signals_rgb24.iter_mut().enumerate() {
                    *output = pack_rgb24(surfaces.value(index).bytes());
                }
                for (resolved, output) in evaluation
                    .occurrences()
                    .zip(buffers.composited_occurrence_signals_rgb24.iter_mut())
                {
                    // Packing an already resolved encoded point is infallible.
                    // Any future fallible verifier must finish before buffer
                    // reclamation above (or introduce its own staging value).
                    *output = pack_rgb24(resolved.visible());
                }
                self.state = SessionState::Ready {
                    current: Snapshot { revision, buffers },
                };
            }
        }
        Ok(&self.state)
    }

    fn admit_same_revision(
        &self,
        prepared: PreparedEncodedSurfaceUpdateV1<'_>,
    ) -> Result<&SessionState, PointRenderSessionUpdateErrorV1> {
        let exact = match (prepared, &self.state) {
            (
                PreparedEncodedSurfaceUpdateV1::Unavailable(incoming),
                SessionState::Waiting {
                    current_unavailable: Some(current),
                }
                | SessionState::Stale {
                    current_unavailable: current,
                    ..
                },
            ) => incoming == *current,
            (
                PreparedEncodedSurfaceUpdateV1::Present { revision, surfaces },
                SessionState::Ready { current },
            ) => {
                revision == current.revision
                    && surfaces.matches_rgb24(&current.buffers.input_surface_signals_rgb24)
            }
            _ => false,
        };
        if exact {
            Ok(&self.state)
        } else {
            Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
                PackedEncodedSurfaceUpdateErrorV1::RevisionConflict {
                    revision: self
                        .state
                        .head_revision()
                        .unwrap_or_else(|| unreachable!("same-revision branch has a head")),
                },
            ))
        }
    }
}

fn map_session_update_error(error: PointRenderSessionUpdateErrorV1) -> SessionUpdateError {
    match error {
        PointRenderSessionUpdateErrorV1::ProgramExpired => SessionUpdateError::ProgramExpired,
        PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::RevisionOutOfOrder { current, incoming },
        ) => SessionUpdateError::RevisionOutOfOrder { current, incoming },
        PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::RevisionConflict { revision },
        ) => SessionUpdateError::RevisionConflict { revision },
        PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(_)
        | PointRenderSessionUpdateErrorV1::Evaluation(_) => SessionUpdateError::InternalInvariant,
    }
}

fn decode_encoded_surface_update(
    words: &[u32],
    surface_count: usize,
) -> Result<PreparedEncodedSurfaceUpdateV1<'_>, PackedEncodedSurfaceUpdateErrorV1> {
    if words.len() < PACKED_ENCODED_SURFACE_HEADER_WORDS_V1 {
        return Err(PackedEncodedSurfaceUpdateErrorV1::HeaderTooShort);
    }
    if words[0] != PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1 {
        return Err(PackedEncodedSurfaceUpdateErrorV1::MagicMismatch { actual: words[0] });
    }
    let revision = u64::from(words[2]) | (u64::from(words[3]) << 32);
    match words[1] {
        PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1 => {
            if words.len() != PACKED_SURFACE_UNAVAILABLE_WORDS_V1 {
                return Err(PackedEncodedSurfaceUpdateErrorV1::LengthMismatch {
                    expected: PACKED_SURFACE_UNAVAILABLE_WORDS_V1,
                    actual: words.len(),
                });
            }
            Ok(PreparedEncodedSurfaceUpdateV1::Unavailable(
                SurfaceUnavailable {
                    revision,
                    reason: words[4],
                },
            ))
        }
        PACKED_ENCODED_SURFACE_PRESENT_TAG_V1 => {
            let expected = PACKED_ENCODED_SURFACE_HEADER_WORDS_V1
                .checked_add(surface_count)
                .ok_or(PackedEncodedSurfaceUpdateErrorV1::ResourceExhausted)?;
            if words.len() != expected {
                return Err(PackedEncodedSurfaceUpdateErrorV1::LengthMismatch {
                    expected,
                    actual: words.len(),
                });
            }
            let surfaces = &words[PACKED_ENCODED_SURFACE_HEADER_WORDS_V1..];
            for (surface_index, &value) in surfaces.iter().enumerate() {
                if value & 0xff00_0000 != 0 {
                    return Err(
                        PackedEncodedSurfaceUpdateErrorV1::ReservedSignalByteNonZero {
                            surface_index,
                            value,
                        },
                    );
                }
            }
            Ok(PreparedEncodedSurfaceUpdateV1::Present {
                revision,
                surfaces: PreparedSurfaceValuesV1::PackedRgb24(surfaces),
            })
        }
        actual => Err(PackedEncodedSurfaceUpdateErrorV1::UnsupportedTag { actual }),
    }
}

fn take_last_ready(state: &mut SessionState) -> Option<Snapshot> {
    match mem::replace(
        state,
        SessionState::Waiting {
            current_unavailable: None,
        },
    ) {
        SessionState::Waiting { .. } => None,
        SessionState::Ready { current } => Some(current),
        SessionState::Stale { previous, .. } => Some(previous),
    }
}

const fn unpack_rgb24(word: u32) -> [u8; 3] {
    [
        ((word >> 16) & 0xff) as u8,
        ((word >> 8) & 0xff) as u8,
        (word & 0xff) as u8,
    ]
}

const fn pack_rgb24(bytes: [u8; 3]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32
}
