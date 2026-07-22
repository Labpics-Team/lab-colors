//! Private generic point Program, constraint recheck and terminal Paint path.
//!
//! The authored graph has no client/UI role vocabulary. Paints are physical
//! source-plus-straight-alpha programs, occurrences are modeled applications of
//! Paint to Surface, constraints assess those exact occurrences, and outputs
//! bind opaque slots back to Paints. A visible occurrence is evidence, never a
//! terminal emitted value.
//!
//! This is the encoded-sRGB8 transport-only executable slice, not the LCS
//! observation/evidence layer.

use std::marker::PhantomData;
use std::mem;
use std::rc::{Rc, Weak};

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, AppearanceWorkspace,
    BindingError, ColorInputId, CompileError, CompiledAppearanceGraph, CompiledOccurrenceSlotV1,
    CompiledPaintSlotV1, OccurrenceId, OccurrenceSpec, OpacityInputId, PaintId, PaintSpec,
    SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
use crate::constraints::{
    HardDecision, PointEvaluationError, PointEvaluatorV1, PointInvocation,
    VisiblePointPassEvidence, VisiblePointViolationEvidence, assess_visible_point_hard,
};
use crate::observation::{ObservationGroupId, ObservationStreamId};

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

/// Attachment-time binding of a compiled group to one runtime stream epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationStreamBinding {
    group: ObservationGroupId,
    stream: ObservationStreamId,
}

impl ObservationStreamBinding {
    pub const fn new(group: ObservationGroupId, stream: ObservationStreamId) -> Self {
        Self { group, stream }
    }

    pub const fn group(self) -> ObservationGroupId {
        self.group
    }

    pub const fn stream(self) -> ObservationStreamId {
        self.stream
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
        prepare_program(self).map(|epoch| CompiledProgram { epoch })
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
    surface_input_ports: Box<[SurfaceInputPortId]>,
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
    epoch: ProgramEpochV1<Evaluation>,
}

impl<Evaluation> CompiledProgram<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub const fn observation_group_id(&self) -> ObservationGroupId {
        self.epoch.observation_group.id
    }

    pub fn surface_input_ports(&self) -> &[SurfaceInputPortId] {
        &self.epoch.observation_group.surface_input_ports
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

    pub fn into_owner(self) -> PointRenderOwner<Evaluation> {
        PointRenderOwner::new(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointRenderAttachError {
    Disposed,
    ObservationGroupMismatch {
        expected: ObservationGroupId,
        actual: ObservationGroupId,
    },
    ResourceExhausted,
    InternalInvariant,
}

/// Sole strong owner of one non-reusable compiled epoch.
pub struct PointRenderOwner<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    current: Option<Rc<ProgramEpochV1<Evaluation>>>,
}

impl<Evaluation> PointRenderOwner<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub fn new(compiled: CompiledProgram<Evaluation>) -> Self {
        Self {
            current: Some(Rc::new(compiled.epoch)),
        }
    }

    pub fn replace(&mut self, compiled: CompiledProgram<Evaluation>) {
        self.current = Some(Rc::new(compiled.epoch));
    }

    pub fn dispose(&mut self) {
        self.current = None;
    }

    pub fn observation_group_id(&self) -> Option<ObservationGroupId> {
        self.current
            .as_deref()
            .map(|epoch| epoch.observation_group.id)
    }

    pub fn surface_input_ports(&self) -> Option<&[SurfaceInputPortId]> {
        self.current
            .as_deref()
            .map(|epoch| epoch.observation_group.surface_input_ports.as_ref())
    }

    pub fn constraint_ids(&self) -> Option<impl ExactSizeIterator<Item = ConstraintId> + '_> {
        self.current
            .as_deref()
            .map(|epoch| epoch.constraints.iter().map(|constraint| constraint.id))
    }

    pub fn outputs(&self) -> Option<impl ExactSizeIterator<Item = (OutputSlotId, PaintId)> + '_> {
        self.current.as_deref().map(|epoch| {
            epoch
                .outputs
                .iter()
                .map(|output| (output.output, output.paint_id))
        })
    }

    /// Fallibly allocate every hot-path frame before the Session escapes.
    pub fn attach(
        &self,
        binding: ObservationStreamBinding,
    ) -> Result<Session<Evaluation>, PointRenderAttachError> {
        let epoch = self
            .current
            .as_ref()
            .ok_or(PointRenderAttachError::Disposed)?;
        if binding.group != epoch.observation_group.id {
            return Err(PointRenderAttachError::ObservationGroupMismatch {
                expected: epoch.observation_group.id,
                actual: binding.group,
            });
        }
        let workspace = epoch
            .graph
            .new_workspace()
            .map_err(map_attach_binding_error)?;
        let bindings = epoch
            .binding_template
            .try_clone_v1()
            .map_err(map_attach_binding_error)?;
        let free_frames = [
            Some(ExecutionFrame::try_new(epoch)?),
            Some(ExecutionFrame::try_new(epoch)?),
            Some(ExecutionFrame::try_new(epoch)?),
        ];
        Ok(Session {
            epoch: Rc::downgrade(epoch),
            stream: binding.stream,
            bindings,
            workspace,
            free_frames,
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

    let constraints = compile_constraints::<Evaluation>(&graph, program.constraints)?;
    let outputs = compile_outputs(&graph, program.outputs)?;
    Ok(ProgramEpochV1 {
        evaluator: program.evaluator,
        graph,
        binding_template,
        observation_group: CompiledObservationGroupV1 {
            id: program.observation_group.id,
            surface_input_ports: surface_input_ports.into_boxed_slice(),
        },
        constraints,
        outputs,
    })
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

/// Revision-bound unavailable input descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceUnavailable {
    revision: u64,
    reason: u32,
}

impl SurfaceUnavailable {
    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub const fn reason(self) -> u32 {
        self.reason
    }
}

/// One typed runtime Surface input signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSignal {
    input: SurfaceInputPortId,
    value: Srgb8,
}

impl SurfaceSignal {
    pub const fn new(input: SurfaceInputPortId, value: Srgb8) -> Self {
        Self { input, value }
    }

    pub const fn input(self) -> SurfaceInputPortId {
        self.input
    }

    pub const fn value(self) -> Srgb8 {
        self.value
    }
}

/// Transport-only payload carried by one stream-affine point update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceUpdatePayload<'input> {
    Unavailable { reason: u32 },
    Present { surfaces: &'input [SurfaceSignal] },
}

/// Borrowed, correlated runtime update for one attached Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceUpdate<'input> {
    stream: ObservationStreamId,
    revision: u64,
    payload: SurfaceUpdatePayload<'input>,
}

impl<'input> SurfaceUpdate<'input> {
    pub const fn unavailable(stream: ObservationStreamId, revision: u64, reason: u32) -> Self {
        Self {
            stream,
            revision,
            payload: SurfaceUpdatePayload::Unavailable { reason },
        }
    }

    pub const fn present(
        stream: ObservationStreamId,
        revision: u64,
        surfaces: &'input [SurfaceSignal],
    ) -> Self {
        Self {
            stream,
            revision,
            payload: SurfaceUpdatePayload::Present { surfaces },
        }
    }

    pub const fn stream(self) -> ObservationStreamId {
        self.stream
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub const fn payload(self) -> SurfaceUpdatePayload<'input> {
        self.payload
    }
}

/// Evaluator classification with the exact bound occurrence evidence.
pub enum ConstraintOutcome<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    Pass(VisiblePointPassEvidence<Evaluation>),
    Violation(VisiblePointViolationEvidence<Evaluation>),
}

/// One mode-refined report cell.
pub struct ConstraintAssessment<Evaluation, Mode>
where
    Evaluation: PointEvaluatorV1,
{
    constraint: ConstraintId,
    target: OccurrenceId,
    outcome: ConstraintOutcome<Evaluation>,
    mode: PhantomData<fn() -> Mode>,
}

impl<Evaluation, Mode> ConstraintAssessment<Evaluation, Mode>
where
    Evaluation: PointEvaluatorV1,
{
    fn new(
        constraint: ConstraintId,
        target: OccurrenceId,
        outcome: ConstraintOutcome<Evaluation>,
    ) -> Self {
        Self {
            constraint,
            target,
            outcome,
            mode: PhantomData,
        }
    }

    pub const fn constraint(&self) -> ConstraintId {
        self.constraint
    }

    pub const fn target(&self) -> OccurrenceId {
        self.target
    }

    pub const fn outcome(&self) -> &ConstraintOutcome<Evaluation> {
        &self.outcome
    }
}

/// Canonical full-report entry; authored mode remains visible in the type.
pub enum ConstraintReportEntry<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    Hard(ConstraintAssessment<Evaluation, HardModeV1>),
    ReportOnly(ConstraintAssessment<Evaluation, ReportModeV1>),
}

impl<Evaluation> ConstraintReportEntry<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    pub const fn constraint(&self) -> ConstraintId {
        match self {
            Self::Hard(assessment) => assessment.constraint,
            Self::ReportOnly(assessment) => assessment.constraint,
        }
    }

    pub const fn target(&self) -> OccurrenceId {
        match self {
            Self::Hard(assessment) => assessment.target,
            Self::ReportOnly(assessment) => assessment.target,
        }
    }
}

/// Pure terminal Paint value. Routing identities are intentionally outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPaintV1 {
    source: Srgb8,
    straight_alpha: AdmittedOpacityV1,
}

impl OutputPaintV1 {
    pub const fn source(self) -> Srgb8 {
        self.source
    }

    pub const fn straight_alpha(self) -> f64 {
        self.straight_alpha.value()
    }

    pub const fn straight_alpha_bits(self) -> u64 {
        self.straight_alpha.bits()
    }
}

/// One routed terminal cell: opaque client slot, authored Paint identity and
/// the independent physical Paint value produced for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputValueV1 {
    output: OutputSlotId,
    paint: PaintId,
    value: OutputPaintV1,
}

impl OutputValueV1 {
    pub const fn output(self) -> OutputSlotId {
        self.output
    }

    pub const fn paint(self) -> PaintId {
        self.paint
    }

    pub const fn value(self) -> OutputPaintV1 {
        self.value
    }
}

struct ExecutionFrame<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    surfaces: Box<[SurfaceSignal]>,
    reports: Vec<Option<ConstraintReportEntry<Evaluation>>>,
    outputs: Vec<Option<OutputValueV1>>,
}

impl<Evaluation> ExecutionFrame<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    fn try_new(epoch: &ProgramEpochV1<Evaluation>) -> Result<Self, PointRenderAttachError> {
        let mut surfaces = Vec::new();
        surfaces
            .try_reserve_exact(epoch.observation_group.surface_input_ports.len())
            .map_err(|_| PointRenderAttachError::ResourceExhausted)?;
        surfaces.extend(
            epoch
                .observation_group
                .surface_input_ports
                .iter()
                .copied()
                .map(|input| SurfaceSignal::new(input, Srgb8::new([0; 3]))),
        );

        let mut reports = Vec::new();
        reports
            .try_reserve_exact(epoch.constraints.len())
            .map_err(|_| PointRenderAttachError::ResourceExhausted)?;
        reports.resize_with(epoch.constraints.len(), || None);

        let mut outputs = Vec::new();
        outputs
            .try_reserve_exact(epoch.outputs.len())
            .map_err(|_| PointRenderAttachError::ResourceExhausted)?;
        outputs.resize_with(epoch.outputs.len(), || None);

        Ok(Self {
            surfaces: surfaces.into_boxed_slice(),
            reports,
            outputs,
        })
    }

    fn clear_dynamic(&mut self) {
        for report in &mut self.reports {
            report.take();
        }
        for output in &mut self.outputs {
            output.take();
        }
    }

    fn report(&self) -> impl ExactSizeIterator<Item = &ConstraintReportEntry<Evaluation>> + '_ {
        self.reports.iter().map(|report| {
            report
                .as_ref()
                .unwrap_or_else(|| unreachable!("committed report is complete"))
        })
    }

    fn outputs(&self) -> impl ExactSizeIterator<Item = OutputValueV1> + '_ {
        self.outputs.iter().map(|output| {
            output
                .as_ref()
                .copied()
                .unwrap_or_else(|| unreachable!("verified output set is complete"))
        })
    }

    fn present_payload_matches(&self, mut value_at: impl FnMut(usize) -> Srgb8) -> bool {
        let mut exact = true;
        for (index, signal) in self.surfaces.iter().enumerate() {
            if value_at(index) != signal.value {
                exact = false;
            }
        }
        exact
    }
}

/// One hard-admitted snapshot with full evidence and terminal Paints.
pub struct Snapshot<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    revision: u64,
    frame: ExecutionFrame<Evaluation>,
}

impl<Evaluation> Snapshot<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn surfaces(&self) -> impl ExactSizeIterator<Item = SurfaceSignal> + '_ {
        self.frame.surfaces.iter().copied()
    }

    pub fn report(&self) -> impl ExactSizeIterator<Item = &ConstraintReportEntry<Evaluation>> + '_ {
        self.frame.report()
    }

    pub fn outputs(&self) -> impl ExactSizeIterator<Item = OutputValueV1> + '_ {
        self.frame.outputs()
    }

    pub fn output(&self, output: OutputSlotId) -> Option<OutputValueV1> {
        let index = self
            .frame
            .outputs
            .binary_search_by_key(&output, |slot| {
                slot.as_ref()
                    .unwrap_or_else(|| unreachable!("verified output set is complete"))
                    .output
            })
            .ok()?;
        self.frame.outputs[index]
    }

    #[cfg(test)]
    pub(crate) fn storage_pointers_for_test(&self) -> (*const SurfaceSignal, *const ()) {
        (
            self.frame.surfaces.as_ptr(),
            self.frame.reports.as_ptr().cast(),
        )
    }
}

/// Complete current report containing at least one hard violation.
pub struct ConstraintConflict<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    revision: u64,
    frame: ExecutionFrame<Evaluation>,
}

impl<Evaluation> ConstraintConflict<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn surfaces(&self) -> impl ExactSizeIterator<Item = SurfaceSignal> + '_ {
        self.frame.surfaces.iter().copied()
    }

    pub fn report(&self) -> impl ExactSizeIterator<Item = &ConstraintReportEntry<Evaluation>> + '_ {
        self.frame.report()
    }

    #[cfg(test)]
    pub(crate) fn storage_pointers_for_test(&self) -> (*const SurfaceSignal, *const ()) {
        (
            self.frame.surfaces.as_ptr(),
            self.frame.reports.as_ptr().cast(),
        )
    }
}

/// Current lifecycle state of one generation-bound Session.
pub enum SessionState<Evaluation>
where
    Evaluation: PointEvaluatorV1,
{
    Waiting {
        current_unavailable: Option<SurfaceUnavailable>,
    },
    Ready {
        current: Snapshot<Evaluation>,
    },
    Stale {
        previous: Snapshot<Evaluation>,
        current_unavailable: SurfaceUnavailable,
    },
    Conflict {
        current: ConstraintConflict<Evaluation>,
        previous: Option<Snapshot<Evaluation>>,
    },
}

impl<Evaluation> SessionState<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
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
            Self::Conflict { current, .. } => Some(current.revision),
        }
    }
}

/// Failure to admit or evaluate one Session update. A hard violation is not an
/// error; it commits [`SessionState::Conflict`] with its full report.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionUpdateError<EvaluationError> {
    ProgramExpired,
    ObservationStreamMismatch {
        expected: ObservationStreamId,
        actual: ObservationStreamId,
    },
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
    Evaluator {
        constraint: ConstraintId,
        source: EvaluationError,
    },
    InternalInvariant,
}

/// Mutable runtime with three attach-allocated transactional frames.
pub struct Session<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    epoch: Weak<ProgramEpochV1<Evaluation>>,
    stream: ObservationStreamId,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
    free_frames: [Option<ExecutionFrame<Evaluation>>; 3],
    state: SessionState<Evaluation>,
}

impl<Evaluation> Session<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    pub const fn state(&self) -> &SessionState<Evaluation> {
        &self.state
    }

    pub const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub fn update(
        &mut self,
        update: SurfaceUpdate<'_>,
    ) -> Result<&SessionState<Evaluation>, SessionUpdateError<PointEvaluationError<Evaluation>>>
    {
        let epoch = self
            .epoch
            .upgrade()
            .ok_or(SessionUpdateError::ProgramExpired)?;
        if update.stream != self.stream {
            return Err(SessionUpdateError::ObservationStreamMismatch {
                expected: self.stream,
                actual: update.stream,
            });
        }
        match update.payload {
            SurfaceUpdatePayload::Unavailable { reason } => {
                self.apply_unavailable(SurfaceUnavailable {
                    revision: update.revision,
                    reason,
                })
            }
            SurfaceUpdatePayload::Present { surfaces } => {
                if surfaces.len() != epoch.observation_group.surface_input_ports.len() {
                    return Err(SessionUpdateError::SurfaceInputPortLengthMismatch {
                        expected: epoch.observation_group.surface_input_ports.len(),
                        actual: surfaces.len(),
                    });
                }
                for (index, (&expected, actual)) in epoch
                    .observation_group
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
                self.apply_canonical_present(&epoch, update.revision, surfaces.len(), |index| {
                    surfaces[index].value
                })
            }
        }
    }

    pub(crate) fn update_canonical_present(
        &mut self,
        stream: ObservationStreamId,
        revision: u64,
        surface_input_port_count: usize,
        value_at: impl FnMut(usize) -> Srgb8,
    ) -> Result<&SessionState<Evaluation>, SessionUpdateError<PointEvaluationError<Evaluation>>>
    {
        let epoch = self
            .epoch
            .upgrade()
            .ok_or(SessionUpdateError::ProgramExpired)?;
        if stream != self.stream {
            return Err(SessionUpdateError::ObservationStreamMismatch {
                expected: self.stream,
                actual: stream,
            });
        }
        self.apply_canonical_present(&epoch, revision, surface_input_port_count, value_at)
    }

    fn apply_unavailable(
        &mut self,
        unavailable: SurfaceUnavailable,
    ) -> Result<&SessionState<Evaluation>, SessionUpdateError<PointEvaluationError<Evaluation>>>
    {
        let incoming_revision = unavailable.revision;
        if let Some(current) = self.state.head_revision() {
            if incoming_revision < current {
                return Err(SessionUpdateError::RevisionOutOfOrder {
                    current,
                    incoming: incoming_revision,
                });
            }
            if incoming_revision == current {
                let exact = match &self.state {
                    SessionState::Waiting {
                        current_unavailable: Some(current),
                    }
                    | SessionState::Stale {
                        current_unavailable: current,
                        ..
                    } => unavailable == *current,
                    _ => false,
                };
                return if exact {
                    Ok(&self.state)
                } else {
                    Err(SessionUpdateError::RevisionConflict { revision: current })
                };
            }
        }

        let previous = self.take_last_verified_and_recycle_current();
        self.state = match previous {
            Some(previous) => SessionState::Stale {
                previous,
                current_unavailable: unavailable,
            },
            None => SessionState::Waiting {
                current_unavailable: Some(unavailable),
            },
        };
        Ok(&self.state)
    }

    fn apply_canonical_present(
        &mut self,
        epoch: &ProgramEpochV1<Evaluation>,
        revision: u64,
        surface_input_port_count: usize,
        mut value_at: impl FnMut(usize) -> Srgb8,
    ) -> Result<&SessionState<Evaluation>, SessionUpdateError<PointEvaluationError<Evaluation>>>
    {
        if surface_input_port_count != epoch.observation_group.surface_input_ports.len() {
            return Err(SessionUpdateError::SurfaceInputPortLengthMismatch {
                expected: epoch.observation_group.surface_input_ports.len(),
                actual: surface_input_port_count,
            });
        }
        if let Some(current) = self.state.head_revision() {
            if revision < current {
                return Err(SessionUpdateError::RevisionOutOfOrder {
                    current,
                    incoming: revision,
                });
            }
            if revision == current {
                return self.admit_same_revision_present(revision, value_at);
            }
        }

        let mut frame =
            take_free_frame(&mut self.free_frames).ok_or(SessionUpdateError::InternalInvariant)?;
        frame.clear_dynamic();
        if frame.surfaces.len() != epoch.observation_group.surface_input_ports.len()
            || frame.reports.len() != epoch.constraints.len()
            || frame.outputs.len() != epoch.outputs.len()
        {
            put_free_frame(&mut self.free_frames, frame);
            return Err(SessionUpdateError::InternalInvariant);
        }

        let surface_slots = &mut frame.surfaces;
        if self
            .bindings
            .overwrite_surface_inputs_canonical(
                epoch.observation_group.surface_input_ports.iter().copied(),
                &mut |index| {
                    let value = value_at(index);
                    surface_slots[index] = SurfaceSignal::new(
                        epoch.observation_group.surface_input_ports[index],
                        value,
                    );
                    value
                },
            )
            .is_err()
        {
            put_free_frame(&mut self.free_frames, frame);
            return Err(SessionUpdateError::InternalInvariant);
        }

        let evaluation = match epoch
            .graph
            .evaluate_admitted_into(&self.bindings, &mut self.workspace)
        {
            Ok(evaluation) => evaluation,
            Err(_) => {
                put_free_frame(&mut self.free_frames, frame);
                return Err(SessionUpdateError::InternalInvariant);
            }
        };

        let mut has_hard_violation = false;
        for (index, constraint) in epoch.constraints.iter().enumerate() {
            let Some(source) = evaluation.occurrence_at(constraint.target) else {
                put_free_frame(&mut self.free_frames, frame);
                return Err(SessionUpdateError::InternalInvariant);
            };
            let decision =
                match assess_visible_point_hard(source, &epoch.evaluator, constraint.invocation) {
                    Ok(decision) => decision,
                    Err(source) => {
                        put_free_frame(&mut self.free_frames, frame);
                        return Err(SessionUpdateError::Evaluator {
                            constraint: constraint.id,
                            source,
                        });
                    }
                };
            let (outcome, violation) = match decision {
                HardDecision::Pass(evidence) => (ConstraintOutcome::Pass(evidence), false),
                HardDecision::Violation(evidence) => (ConstraintOutcome::Violation(evidence), true),
            };
            frame.reports[index] = Some(match constraint.mode {
                CompiledConstraintModeV1::Hard => {
                    has_hard_violation |= violation;
                    ConstraintReportEntry::Hard(ConstraintAssessment::new(
                        constraint.id,
                        constraint.target_id,
                        outcome,
                    ))
                }
                CompiledConstraintModeV1::ReportOnly => ConstraintReportEntry::ReportOnly(
                    ConstraintAssessment::new(constraint.id, constraint.target_id, outcome),
                ),
            });
        }

        if !has_hard_violation {
            for (index, output) in epoch.outputs.iter().enumerate() {
                let Some(paint) = evaluation.paint_at(output.paint) else {
                    put_free_frame(&mut self.free_frames, frame);
                    return Err(SessionUpdateError::InternalInvariant);
                };
                frame.outputs[index] = Some(OutputValueV1 {
                    output: output.output,
                    paint: output.paint_id,
                    value: OutputPaintV1 {
                        source: paint.source(),
                        straight_alpha: paint.opacity(),
                    },
                });
            }
        }
        if has_hard_violation {
            let previous = self.take_last_verified_and_recycle_current();
            self.state = SessionState::Conflict {
                current: ConstraintConflict { revision, frame },
                previous,
            };
        } else {
            self.recycle_entire_state();
            self.state = SessionState::Ready {
                current: Snapshot { revision, frame },
            };
        }
        Ok(&self.state)
    }

    fn admit_same_revision_present(
        &self,
        revision: u64,
        value_at: impl FnMut(usize) -> Srgb8,
    ) -> Result<&SessionState<Evaluation>, SessionUpdateError<PointEvaluationError<Evaluation>>>
    {
        let exact = match &self.state {
            SessionState::Ready { current } => current.frame.present_payload_matches(value_at),
            SessionState::Conflict { current, .. } => {
                current.frame.present_payload_matches(value_at)
            }
            _ => return Err(SessionUpdateError::RevisionConflict { revision }),
        };
        if exact {
            Ok(&self.state)
        } else {
            Err(SessionUpdateError::RevisionConflict { revision })
        }
    }

    fn take_last_verified_and_recycle_current(&mut self) -> Option<Snapshot<Evaluation>> {
        match mem::replace(
            &mut self.state,
            SessionState::Waiting {
                current_unavailable: None,
            },
        ) {
            SessionState::Waiting { .. } => None,
            SessionState::Ready { current } => Some(current),
            SessionState::Stale { previous, .. } => Some(previous),
            SessionState::Conflict { current, previous } => {
                put_free_frame(&mut self.free_frames, current.frame);
                previous
            }
        }
    }

    fn recycle_entire_state(&mut self) {
        match mem::replace(
            &mut self.state,
            SessionState::Waiting {
                current_unavailable: None,
            },
        ) {
            SessionState::Waiting { .. } => {}
            SessionState::Ready { current } => {
                put_free_frame(&mut self.free_frames, current.frame);
            }
            SessionState::Stale { previous, .. } => {
                put_free_frame(&mut self.free_frames, previous.frame);
            }
            SessionState::Conflict { current, previous } => {
                put_free_frame(&mut self.free_frames, current.frame);
                if let Some(previous) = previous {
                    put_free_frame(&mut self.free_frames, previous.frame);
                }
            }
        }
    }
}

fn take_free_frame<Evaluation>(
    pool: &mut [Option<ExecutionFrame<Evaluation>>; 3],
) -> Option<ExecutionFrame<Evaluation>>
where
    Evaluation: PointEvaluatorV1,
{
    pool.iter_mut().find_map(Option::take)
}

fn put_free_frame<Evaluation>(
    pool: &mut [Option<ExecutionFrame<Evaluation>>; 3],
    frame: ExecutionFrame<Evaluation>,
) where
    Evaluation: PointEvaluatorV1,
{
    let Some(slot) = pool.iter_mut().find(|slot| slot.is_none()) else {
        unreachable!("three-frame ownership invariant exceeded")
    };
    *slot = Some(frame);
}
