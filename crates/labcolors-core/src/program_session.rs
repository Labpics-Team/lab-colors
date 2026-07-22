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

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, BindingError,
    ColorInputId, CompileError, CompiledAppearanceGraph, CompiledOccurrenceSlotV1,
    CompiledPaintSlotV1, OccurrenceId, OccurrenceSpec, OpacityInputId, PaintId, PaintSpec,
    SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::composition::CompositionProfileV1;
use crate::constraints::{PointEvaluatorV1, PointInvocation};
use crate::observation::ObservationGroupId;

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

#[expect(
    dead_code,
    reason = "the compiled constraint payload is retained for the direct sole-Session bridge; erasing it would reduce lowering to a shape-only placeholder"
)]
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

#[expect(
    dead_code,
    reason = "the executable graph, admitted bindings and evaluator are retained for the direct sole-Session bridge in the next stack"
)]
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
