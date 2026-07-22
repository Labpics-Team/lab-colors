use crate::Srgb8;
use crate::appearance::{
    ColorInputId, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::constraints::{ExactSrgb8IdentityV1, PointEvaluatorV1, PointInvocation};
use crate::observation::ObservationGroupId;
use crate::program_session::{
    ColorInput, CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    ObservationGroup, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint, Program,
    ProgramCompileError, Surface, canonical_surface_input_port_sequence_matches,
    check_render_node_count,
};

const COLOR: ColorInputId = ColorInputId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE_SURFACE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);
const OUTPUT: OutputSlotId = OutputSlotId::new(40);
const REQUIRED: ConstraintId = ConstraintId::new(50);
const GROUP: ObservationGroupId = ObservationGroupId::new(60);

fn observation_group(surface_input_ports: Vec<SurfaceInputPortId>) -> ObservationGroup {
    ObservationGroup::new(GROUP, surface_input_ports)
}

fn base_program<Evaluation>(
    opacity: f64,
    against: SurfaceId,
    constraints: ConstraintSet<PointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    base_program_in_group(GROUP, opacity, against, constraints, outputs, evaluator)
}

fn base_program_in_group<Evaluation>(
    group: ObservationGroupId,
    opacity: f64,
    against: SurfaceId,
    constraints: ConstraintSet<PointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        ObservationGroup::new(group, vec![SURFACE_PORT]),
        vec![OpacityInput::new(OPACITY, opacity)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: VISIBLE_SURFACE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            against,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
        constraints,
        outputs,
        evaluator,
    )
}

fn compile_error<Evaluation>(program: Program<Evaluation>) -> ProgramCompileError
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    match program.compile() {
        Ok(_) => panic!("invalid declaration compiled"),
        Err(error) => error,
    }
}

#[test]
fn authored_modes_are_marker_typed_and_values_preserve_exact_ids() {
    let hard = ConstraintInvocation::hard(REQUIRED, OCCURRENCE, Srgb8::new([0x80; 3]));
    let report =
        ConstraintInvocation::report_only(ConstraintId::new(51), OCCURRENCE, Srgb8::new([0x81; 3]));
    let set = ConstraintSet::new(vec![hard], vec![report]);
    assert_eq!(set.hard()[0].id(), REQUIRED);
    assert_eq!(set.hard()[0].target(), OCCURRENCE);
    assert_eq!(*set.hard()[0].invocation(), Srgb8::new([0x80; 3]));
    assert_eq!(set.report_only()[0].id(), ConstraintId::new(51));

    let output = OutputBinding::new(OUTPUT, TRANSLUCENT);
    assert_eq!(output.output(), OUTPUT);
    assert_eq!(output.paint(), TRANSLUCENT);
    assert_eq!(ConstraintId::new(7).value(), 7);
    assert_eq!(OutputSlotId::new(8).value(), 8);

    let color = ColorInput::new(COLOR, Srgb8::new([1, 2, 3]));
    assert_eq!(color.id(), COLOR);
    assert_eq!(color.value(), Srgb8::new([1, 2, 3]));
    let opacity = OpacityInput::new(OPACITY, 0.375);
    assert_eq!(opacity.id(), OPACITY);
    assert_eq!(opacity.value(), 0.375);

    let occurrence = Occurrence::new(
        OCCURRENCE,
        TRANSLUCENT,
        BACKDROP,
        CompositionProfile::EncodedSrgb8SourceOverV1,
    );
    assert_eq!(occurrence.id(), OCCURRENCE);
    assert_eq!(occurrence.subject(), TRANSLUCENT);
    assert_eq!(occurrence.against(), BACKDROP);
    assert_eq!(
        occurrence.composition(),
        CompositionProfile::EncodedSrgb8SourceOverV1
    );

    let group = observation_group(vec![SURFACE_PORT]);
    assert_eq!(group.id(), GROUP);
    assert_eq!(group.surface_input_ports(), &[SURFACE_PORT]);
}

#[test]
fn empty_domains_have_stable_precedence() {
    let empty_surface = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        observation_group(vec![]),
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            SOLID,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, SOLID)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_surface),
        ProgramCompileError::EmptyObservationGroup { group: GROUP }
    );

    let empty_occurrence = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        observation_group(vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, SOLID)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_occurrence),
        ProgramCompileError::EmptyOccurrenceSet
    );

    let empty_constraints = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::<Srgb8>::new(vec![], vec![]),
        vec![],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_constraints),
        ProgramCompileError::EmptyConstraintSet
    );

    let empty_outputs = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_outputs),
        ProgramCompileError::EmptyOutputSet
    );
}

#[test]
fn physical_errors_precede_constraint_and_output_errors() {
    let missing_surface = SurfaceId::new(999);
    let duplicate = ConstraintId::new(77);
    let program = base_program(
        0.5,
        missing_surface,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                duplicate,
                OccurrenceId::new(998),
                Srgb8::new([0; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                duplicate,
                OccurrenceId::new(997),
                Srgb8::new([0; 3]),
            )],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(996)),
            OutputBinding::new(OUTPUT, PaintId::new(995)),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(program),
        ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence: OCCURRENCE,
            surface: missing_surface,
        }
    );
}

#[test]
fn constraint_and_output_error_precedence_is_canonical() {
    let duplicate = ConstraintId::new(77);
    let missing_occurrence = OccurrenceId::new(999);
    let duplicate_constraints = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                duplicate,
                missing_occurrence,
                Srgb8::new([0; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                duplicate,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(998)),
            OutputBinding::new(OUTPUT, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(duplicate_constraints),
        ProgramCompileError::DuplicateConstraint {
            constraint: duplicate,
        }
    );

    let missing_constraint = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                missing_occurrence,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, TRANSLUCENT),
            OutputBinding::new(OUTPUT, PaintId::new(998)),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(missing_constraint),
        ProgramCompileError::MissingConstraintOccurrence {
            constraint: REQUIRED,
            occurrence: missing_occurrence,
        }
    );

    let duplicate_output = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(998)),
            OutputBinding::new(OUTPUT, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(duplicate_output),
        ProgramCompileError::DuplicateOutputSlot { output: OUTPUT }
    );

    let missing_paint = PaintId::new(998);
    let missing_output = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, missing_paint)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(missing_output),
        ProgramCompileError::MissingOutputPaint {
            output: OUTPUT,
            paint: missing_paint,
        }
    );
}

#[test]
fn compile_canonicalizes_constraints_and_outputs_independent_of_mode_lists() {
    let low = ConstraintId::new(1);
    let high = ConstraintId::new(9);
    let output_low = OutputSlotId::new(2);
    let output_high = OutputSlotId::new(8);
    let compiled = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                high,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                low,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
        ),
        vec![
            OutputBinding::new(output_high, TRANSLUCENT),
            OutputBinding::new(output_low, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    )
    .compile()
    .unwrap();
    assert_eq!(compiled.observation_group_id(), GROUP);
    assert_eq!(compiled.surface_input_ports(), &[SURFACE_PORT]);
    assert_eq!(
        compiled.constraint_ids().collect::<Vec<_>>(),
        vec![low, high]
    );
    assert_eq!(
        compiled.outputs().collect::<Vec<_>>(),
        vec![(output_low, TRANSLUCENT), (output_high, TRANSLUCENT)]
    );
}

#[test]
fn canonical_helpers_and_checked_cardinality_fail_closed() {
    assert_eq!(check_render_node_count(usize::MAX - 1, 1), Ok(()));
    assert_eq!(
        check_render_node_count(usize::MAX, 1),
        Err(ProgramCompileError::ResourceExhausted)
    );
    let canonical = [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2)];
    assert!(canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2)],
        &canonical,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(2), SurfaceInputPortId::new(1)],
        &canonical,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1)],
        &canonical,
    ));
}
