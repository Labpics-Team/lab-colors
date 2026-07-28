use crate::Srgb8;
use crate::appearance::{OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::constraints::{ExactSrgb8IdentityV1, ProgramPointEvaluatorV1, ProgramPointInvocation};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    OBSERVATION_ARENA_SLOT_COUNT_V1, ObservationGroupId, ObservationHeadViewV1,
    ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput, ObservedScenarioSetInput,
    Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::program_session::{
    CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet, ObservationGroup,
    Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint,
    PointOutputPresentationBindErrorV1, PointPresentationRootV1, PointPresentationTargetV1,
    PresentationRootId, Program, ProgramCompileError, ProgramConstraintBodyV1,
    ProgramConstraintSubjectV1, Source, SourceId, Surface, Target, TargetId, TargetIntentV1,
    canonical_surface_input_port_sequence_matches, check_render_node_count,
};
use crate::session::{SessionPlanV1, SessionState, SessionUpdateError};
use crate::session_tests::CommitSessionUpdateForTest as _;

const SOURCE: SourceId = SourceId::new(1);
const TARGET: TargetId = TargetId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE_SURFACE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);
const OUTPUT: OutputSlotId = OutputSlotId::new(40);
const EARLIER_OUTPUT: OutputSlotId = OutputSlotId::new(39);
const REQUIRED: ConstraintId = ConstraintId::new(50);
const PRESENTATION_ROOT: PresentationRootId = PresentationRootId::new(91);
const EARLIER_PRESENTATION_ROOT: PresentationRootId = PresentationRootId::new(90);
const GROUP: ObservationGroupId = ObservationGroupId::new(60);
const STREAM_A: ObservationStreamId = ObservationStreamId::new(70);
const STREAM_B: ObservationStreamId = ObservationStreamId::new(71);

fn appearance_context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn observation_group(surface_input_ports: Vec<SurfaceInputPortId>) -> ObservationGroup {
    ObservationGroup::new(GROUP, surface_input_ports)
}

fn base_program<Evaluation>(
    opacity: f64,
    against: SurfaceId,
    constraints: ConstraintSet<ProgramPointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: ProgramPointEvaluatorV1,
    ProgramPointInvocation<Evaluation>: Copy,
{
    base_program_in_group(GROUP, opacity, against, constraints, outputs, evaluator)
}

fn base_program_in_group<Evaluation>(
    group: ObservationGroupId,
    opacity: f64,
    against: SurfaceId,
    constraints: ConstraintSet<ProgramPointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: ProgramPointEvaluatorV1,
    ProgramPointInvocation<Evaluation>: Copy,
{
    Program::new(
        vec![Source::new(
            SOURCE,
            ColorSignal::from_srgb8(Srgb8::new([0; 3])),
        )],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(group, vec![SURFACE_PORT]),
        vec![OpacityInput::new(OPACITY, opacity)],
        vec![
            Paint::Solid {
                id: SOLID,
                target: TARGET,
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
            appearance_context(),
        )],
        constraints,
        outputs,
        evaluator,
    )
}

fn compile_error<Evaluation>(program: Program<Evaluation>) -> ProgramCompileError
where
    Evaluation: ProgramPointEvaluatorV1,
    ProgramPointInvocation<Evaluation>: Copy,
{
    match program.compile() {
        Ok(_) => panic!("invalid declaration compiled"),
        Err(error) => error,
    }
}

fn observed_update(
    stream: ObservationStreamId,
    revision: u64,
    scenarios: &[(u32, [u8; 3])],
) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: scenarios
                .iter()
                .map(|(scenario, backdrop)| ScenarioInput {
                    id: ScenarioId::new(*scenario),
                    bindings: vec![SurfaceInputBinding::new(
                        SURFACE_PORT,
                        ColorSignal::from_srgb8(Srgb8::new(*backdrop)),
                    )],
                })
                .collect(),
        }),
    }
}

fn exact_compiled(
    constraints: ConstraintSet<Srgb8>,
) -> crate::program_session::CompiledProgram<ExactSrgb8IdentityV1> {
    base_program(
        0.5,
        BACKDROP,
        constraints,
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        ExactSrgb8IdentityV1,
    )
    .compile()
    .unwrap()
}

fn exact_compiled_with_point_presentations(
    outputs: Vec<OutputBinding>,
) -> crate::program_session::CompiledProgram<ExactSrgb8IdentityV1> {
    base_program(
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
        outputs,
        ExactSrgb8IdentityV1,
    )
    .with_point_presentations(
        vec![
            PointPresentationRootV1::new(PRESENTATION_ROOT, OCCURRENCE),
            PointPresentationRootV1::new(EARLIER_PRESENTATION_ROOT, OCCURRENCE),
        ],
        vec![
            PointPresentationTargetV1::new(PRESENTATION_ROOT, OCCURRENCE),
            PointPresentationTargetV1::new(EARLIER_PRESENTATION_ROOT, OCCURRENCE),
        ],
    )
    .compile()
    .unwrap()
}

#[test]
fn authored_modes_are_marker_typed_and_values_preserve_exact_ids() {
    let hard = ConstraintInvocation::hard(REQUIRED, OCCURRENCE, Srgb8::new([0x80; 3]));
    let report =
        ConstraintInvocation::report_only(ConstraintId::new(51), OCCURRENCE, Srgb8::new([0x81; 3]));
    let set = ConstraintSet::new(vec![hard], vec![report]);
    assert_eq!(set.hard()[0].id(), REQUIRED);
    assert_eq!(
        *set.hard()[0].body(),
        ProgramConstraintBodyV1::ModeledOccurrence {
            occurrence: OCCURRENCE,
            invocation: Srgb8::new([0x80; 3]),
        },
    );
    assert_eq!(set.report_only()[0].id(), ConstraintId::new(51));

    let output = OutputBinding::new(OUTPUT, TRANSLUCENT);
    assert_eq!(output.output(), OUTPUT);
    assert_eq!(output.paint(), TRANSLUCENT);
    assert_eq!(ConstraintId::new(7).value(), 7);
    assert_eq!(OutputSlotId::new(8).value(), 8);

    let source = Source::new(SOURCE, ColorSignal::from_srgb8(Srgb8::new([1, 2, 3])));
    assert_eq!(SOURCE.value(), 1);
    assert_eq!(source.id(), SOURCE);
    assert_eq!(
        source.signal(),
        ColorSignal::from_srgb8(Srgb8::new([1, 2, 3]))
    );
    let target = Target::fixed(TARGET, SOURCE);
    assert_eq!(target.id(), TARGET);
    assert_eq!(target.intent(), &TargetIntentV1::FixedSource(SOURCE));
    let opacity = OpacityInput::new(OPACITY, 0.375);
    assert_eq!(opacity.id(), OPACITY);
    assert_eq!(opacity.value(), 0.375);

    let occurrence = Occurrence::new(
        OCCURRENCE,
        TRANSLUCENT,
        BACKDROP,
        CompositionProfile::EncodedSrgb8SourceOverV1,
        appearance_context(),
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
        vec![Source::new(
            SOURCE,
            ColorSignal::from_srgb8(Srgb8::new([0; 3])),
        )],
        vec![Target::fixed(TARGET, SOURCE)],
        observation_group(vec![]),
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            target: TARGET,
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
            appearance_context(),
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
        vec![Source::new(
            SOURCE,
            ColorSignal::from_srgb8(Srgb8::new([0; 3])),
        )],
        vec![Target::fixed(TARGET, SOURCE)],
        observation_group(vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            target: TARGET,
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
fn point_output_presentation_binding_retains_exact_ids_and_canonical_ordinals() {
    let compiled = exact_compiled_with_point_presentations(vec![
        OutputBinding::new(OUTPUT, TRANSLUCENT),
        OutputBinding::new(EARLIER_OUTPUT, TRANSLUCENT),
    ]);

    let binding = compiled
        .bind_point_output_presentation(OUTPUT, PRESENTATION_ROOT, OCCURRENCE)
        .unwrap();

    assert_eq!(binding.output(), OUTPUT);
    assert_eq!(binding.paint(), TRANSLUCENT);
    assert_eq!(binding.root(), PRESENTATION_ROOT);
    assert_eq!(binding.occurrence(), OCCURRENCE);
    // Canonical ordinals следуют возрастающим numeric IDs: оба EARLIER_ ID
    // меньше выбранных OUTPUT/PRESENTATION_ROOT и потому занимают ordinal 0.
    assert_eq!(binding.output_ordinal(), 1);
    assert_eq!(binding.presentation_ordinal(), 1);
}

#[test]
fn point_output_presentation_uses_the_selected_target_subject_not_the_terminal_subject() {
    let lower_source = SourceId::new(101);
    let terminal_source = SourceId::new(102);
    let lower_target = TargetId::new(103);
    let terminal_target = TargetId::new(104);
    let lower_paint = PaintId::new(105);
    let terminal_paint = PaintId::new(106);
    let root_surface = SurfaceId::new(107);
    let derived_surface = SurfaceId::new(108);
    let selected_occurrence = OccurrenceId::new(109);
    let terminal_occurrence = OccurrenceId::new(110);
    let selected_output = OutputSlotId::new(111);
    let presentation_root = PresentationRootId::new(112);

    let compiled = Program::new(
        vec![
            Source::new(
                lower_source,
                ColorSignal::from_srgb8(Srgb8::new([10, 20, 30])),
            ),
            Source::new(
                terminal_source,
                ColorSignal::from_srgb8(Srgb8::new([40, 50, 60])),
            ),
        ],
        vec![
            Target::fixed(lower_target, lower_source),
            Target::fixed(terminal_target, terminal_source),
        ],
        observation_group(vec![SURFACE_PORT]),
        vec![],
        vec![
            Paint::Solid {
                id: lower_paint,
                target: lower_target,
            },
            Paint::Solid {
                id: terminal_paint,
                target: terminal_target,
            },
        ],
        vec![
            Surface::Input {
                id: root_surface,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: derived_surface,
                occurrence: selected_occurrence,
            },
        ],
        vec![
            Occurrence::new(
                selected_occurrence,
                lower_paint,
                root_surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
            Occurrence::new(
                terminal_occurrence,
                terminal_paint,
                derived_surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                terminal_occurrence,
                Srgb8::new([40, 50, 60]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(selected_output, lower_paint)],
        ExactSrgb8IdentityV1,
    )
    .with_point_presentations(
        vec![PointPresentationRootV1::new(
            presentation_root,
            terminal_occurrence,
        )],
        vec![PointPresentationTargetV1::new(
            presentation_root,
            selected_occurrence,
        )],
    )
    .compile()
    .unwrap();

    let binding = compiled
        .bind_point_output_presentation(selected_output, presentation_root, selected_occurrence)
        .unwrap();
    assert_eq!(binding.paint(), lower_paint);
    assert_eq!(binding.occurrence(), selected_occurrence);
    assert_ne!(binding.paint(), terminal_paint);
}

#[test]
fn point_output_presentation_binding_reports_each_exact_failure() {
    let compiled =
        exact_compiled_with_point_presentations(vec![OutputBinding::new(OUTPUT, TRANSLUCENT)]);
    let missing_output = OutputSlotId::new(u32::MAX);
    let missing_root = PresentationRootId::new(u32::MAX);
    let missing_occurrence = OccurrenceId::new(u32::MAX);

    assert_eq!(
        compiled.bind_point_output_presentation(missing_output, missing_root, missing_occurrence,),
        Err(PointOutputPresentationBindErrorV1::MissingOutput {
            output: missing_output,
        })
    );
    assert_eq!(
        compiled.bind_point_output_presentation(OUTPUT, missing_root, OCCURRENCE),
        Err(
            PointOutputPresentationBindErrorV1::MissingPresentationTarget {
                root: missing_root,
                occurrence: OCCURRENCE,
            }
        )
    );
    assert_eq!(
        compiled.bind_point_output_presentation(OUTPUT, PRESENTATION_ROOT, missing_occurrence),
        Err(
            PointOutputPresentationBindErrorV1::MissingPresentationTarget {
                root: PRESENTATION_ROOT,
                occurrence: missing_occurrence,
            }
        )
    );

    let mismatch = exact_compiled_with_point_presentations(vec![OutputBinding::new(OUTPUT, SOLID)]);
    assert_eq!(
        mismatch.bind_point_output_presentation(OUTPUT, PRESENTATION_ROOT, OCCURRENCE),
        Err(PointOutputPresentationBindErrorV1::SubjectPaintMismatch {
            output: OUTPUT,
            output_paint: SOLID,
            root: PRESENTATION_ROOT,
            occurrence: OCCURRENCE,
            subject_paint: TRANSLUCENT,
        })
    );
}

#[test]
fn compiled_program_owner_pin_keeps_the_exact_generation_alive_until_drop() {
    let compiled = exact_compiled(ConstraintSet::new(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
        vec![],
    ));
    let owner_pin = compiled.pin_owner();
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    drop(compiled);

    assert!(matches!(
        session
            .commit(observed_update(STREAM_A, 1, &[(1, [0xFF; 3])]))
            .unwrap(),
        SessionState::Ready { .. }
    ));

    drop(owner_pin);
    assert!(matches!(
        session.commit(observed_update(STREAM_A, 2, &[(1, [0xFF; 3])])),
        Err(SessionUpdateError::OwnerExpired)
    ));
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

#[test]
fn independently_instantiated_streams_expire_with_their_compiled_owner_generation() {
    let compiled = exact_compiled(ConstraintSet::new(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
        vec![],
    ));
    let mut first = compiled.instantiate(STREAM_A).unwrap();
    let mut second = compiled.instantiate(STREAM_B).unwrap();
    drop(compiled);

    assert!(matches!(
        first.commit(observed_update(STREAM_A, 1, &[(1, [0xFF; 3])])),
        Err(SessionUpdateError::OwnerExpired),
    ));
    assert!(matches!(
        second.commit(observed_update(STREAM_B, 9, &[(2, [0xFF; 3])])),
        Err(SessionUpdateError::OwnerExpired),
    ));
    assert!(matches!(first.state(), SessionState::Waiting));
    assert!(matches!(second.state(), SessionState::Waiting));
    assert_eq!(first.raw_head(), ObservationHeadViewV1::Empty);
    assert_eq!(second.raw_head(), ObservationHeadViewV1::Empty);
}

#[test]
fn program_sessions_each_prewarm_three_arenas_over_the_owner_canonical_schema() {
    let compiled = exact_compiled(ConstraintSet::new(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
        vec![],
    ));

    assert_eq!(compiled.observation_schema_strong_count_for_test(), 1);

    let mut first = compiled.instantiate(STREAM_A).unwrap();
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        1 + OBSERVATION_ARENA_SLOT_COUNT_V1,
    );
    let second = compiled.instantiate(STREAM_B).unwrap();
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        1 + 2 * OBSERVATION_ARENA_SLOT_COUNT_V1,
    );

    drop(second);
    let one_session_schema_handle_count = 1 + OBSERVATION_ARENA_SLOT_COUNT_V1;
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        one_session_schema_handle_count,
    );

    let schema_ptr = {
        let owner = first.plan().try_acquire_owner().unwrap();
        first
            .plan()
            .observation_schema(&owner)
            .backing_ptr_for_test()
    };
    let (report_schema_ptr, report_backing_ptr) = match first
        .commit(observed_update(STREAM_A, 1, &[(1, [0xFF; 3])]))
        .unwrap()
    {
        SessionState::Ready { current } => (
            current.report().observation().schema_ptr_for_test(),
            current.report().observation().backing_ptr_for_test(),
        ),
        _ => panic!("the exact Program must verify"),
    };
    assert_eq!(report_schema_ptr, schema_ptr);
    let ObservationHeadViewV1::Observed(raw) = first.raw_head() else {
        panic!("the raw head must retain the admitted observation");
    };
    assert_eq!(raw.schema_ptr_for_test(), schema_ptr);
    assert_eq!(raw.backing_ptr_for_test(), report_backing_ptr);
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        one_session_schema_handle_count,
    );

    let idempotent_report_backing_ptr = match first
        .commit(observed_update(STREAM_A, 1, &[(1, [0xFF; 3])]))
        .unwrap()
    {
        SessionState::Ready { current } => current.report().observation().backing_ptr_for_test(),
        _ => panic!("an exact replay must retain the verified Program report"),
    };
    assert_eq!(idempotent_report_backing_ptr, report_backing_ptr);
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        one_session_schema_handle_count,
    );

    let observation_clone = match first.state() {
        SessionState::Ready { current } => current.report().observation().clone(),
        _ => panic!("the verified Program report must remain current"),
    };
    assert_eq!(observation_clone.schema_ptr_for_test(), schema_ptr);
    assert_eq!(
        compiled.observation_schema_strong_count_for_test(),
        one_session_schema_handle_count,
        "cloning an observation must not add a canonical schema Rc",
    );
    drop(observation_clone);

    drop(first);
    assert_eq!(compiled.observation_schema_strong_count_for_test(), 1);
}

#[test]
fn multi_case_hard_failure_retains_the_full_matrix_without_outputs() {
    let low = ConstraintId::new(1);
    let high = ConstraintId::new(2);
    let compiled = exact_compiled(ConstraintSet::new(
        vec![
            ConstraintInvocation::hard(high, OCCURRENCE, Srgb8::new([0x00; 3])),
            ConstraintInvocation::hard(low, OCCURRENCE, Srgb8::new([0x80; 3])),
        ],
        vec![],
    ));
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session
        .commit(observed_update(
            STREAM_A,
            1,
            &[(11, [0x00; 3]), (12, [0xFF; 3])],
        ))
        .unwrap();
    let SessionState::Failed { cause, previous } = state else {
        panic!("each candidate target fails on one admitted physical case");
    };
    assert!(previous.is_none());
    assert_eq!(
        cause.retained_output_value_count_for_test(),
        0,
        "conflict evidence must not retain output values without output authority",
    );
    let cells = cause.report().cells();
    assert_eq!(
        cells.len(),
        4,
        "two cases × two constraints must be complete"
    );
    assert_eq!(
        cells
            .iter()
            .map(|cell| (cell.case_index(), cell.constraint()))
            .collect::<Vec<_>>(),
        vec![(0, low), (0, high), (1, low), (1, high)],
    );
    assert!(cells.iter().all(|cell| cell.is_hard()));
    assert!(cells.iter().all(|cell| {
        cell.subject()
            == ProgramConstraintSubjectV1::ModeledOccurrence {
                occurrence: OCCURRENCE,
                context: appearance_context(),
            }
    }));
    assert_eq!(
        cells
            .iter()
            .filter(|cell| cell.result().is_violation())
            .count(),
        2,
    );
    // `ProgramConflictV1` owns only the complete report; no output accessor or
    // output storage exists on the failure type.
}

#[test]
fn mixed_modes_retain_the_full_canonical_matrix_without_outputs_on_hard_failure() {
    let diagnostic = ConstraintId::new(1);
    let required = ConstraintId::new(2);
    let compiled = exact_compiled(ConstraintSet::new(
        vec![ConstraintInvocation::hard(
            required,
            OCCURRENCE,
            Srgb8::new([0x00; 3]),
        )],
        vec![ConstraintInvocation::report_only(
            diagnostic,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
    ));
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session
        .commit(observed_update(
            STREAM_A,
            1,
            &[(11, [0x00; 3]), (12, [0xFF; 3])],
        ))
        .unwrap();
    let SessionState::Failed { cause, previous } = state else {
        panic!("the hard constraint must gate the otherwise complete mixed report");
    };
    assert!(previous.is_none());

    let cells = cause.report().cells();
    assert_eq!(cells.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| (cell.case_index(), cell.constraint(), cell.is_hard()))
            .collect::<Vec<_>>(),
        vec![
            (0, diagnostic, false),
            (0, required, true),
            (1, diagnostic, false),
            (1, required, true),
        ],
    );
    assert_eq!(
        cells
            .iter()
            .map(|cell| cell.result().is_violation())
            .collect::<Vec<_>>(),
        vec![true, false, false, true],
    );
    assert!(cells.iter().all(|cell| {
        cell.subject()
            == ProgramConstraintSubjectV1::ModeledOccurrence {
                occurrence: OCCURRENCE,
                context: appearance_context(),
            }
    }));
    // `cause` is `ProgramConflictV1`: the failure surface exposes only this
    // complete report, while Paint outputs exist only on `ProgramVerifiedV1`.
}

#[test]
fn report_only_violations_do_not_block_program_scope_paint_outputs() {
    let diagnostic = ConstraintId::new(7);
    let compiled = exact_compiled(ConstraintSet::new(
        vec![],
        vec![ConstraintInvocation::report_only(
            diagnostic,
            OCCURRENCE,
            Srgb8::new([0x7F; 3]),
        )],
    ));
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session
        .commit(observed_update(
            STREAM_A,
            1,
            &[(1, [0x00; 3]), (2, [0xFF; 3])],
        ))
        .unwrap();
    let SessionState::Ready { current } = state else {
        panic!("report-only violations must not gate outputs");
    };
    assert_eq!(current.report().cells().len(), 2);
    assert!(
        current
            .report()
            .cells()
            .iter()
            .all(|cell| !cell.is_hard() && cell.result().is_violation()),
    );
    let [output] = current.outputs() else {
        panic!("one canonical output must be emitted");
    };
    assert_eq!(output.output(), OUTPUT);
    assert_eq!(output.paint().id(), TRANSLUCENT);
    assert_eq!(output.paint().source(), Srgb8::new([0; 3]));
    assert_eq!(output.paint().opacity_bits(), 0.5_f64.to_bits());
}

#[test]
fn nested_surface_uses_the_lower_occurrence_before_assessing_the_upper() {
    const LOWER_SOURCE: SourceId = SourceId::new(101);
    const UPPER_SOURCE: SourceId = SourceId::new(102);
    const LOWER_TARGET: TargetId = TargetId::new(101);
    const UPPER_TARGET: TargetId = TargetId::new(102);
    const HALF: OpacityInputId = OpacityInputId::new(103);
    const LOWER_PAINT: PaintId = PaintId::new(110);
    const UPPER_SOLID: PaintId = PaintId::new(111);
    const UPPER_PAINT: PaintId = PaintId::new(112);
    const ROOT: SurfaceId = SurfaceId::new(120);
    const DERIVED: SurfaceId = SurfaceId::new(121);
    const LOWER: OccurrenceId = OccurrenceId::new(130);
    const UPPER: OccurrenceId = OccurrenceId::new(131);
    const NESTED_OUTPUT: OutputSlotId = OutputSlotId::new(140);

    let program = Program::new(
        vec![
            Source::new(LOWER_SOURCE, ColorSignal::from_srgb8(Srgb8::new([0x80; 3]))),
            Source::new(UPPER_SOURCE, ColorSignal::from_srgb8(Srgb8::new([0xFF; 3]))),
        ],
        vec![
            Target::fixed(LOWER_TARGET, LOWER_SOURCE),
            Target::fixed(UPPER_TARGET, UPPER_SOURCE),
        ],
        observation_group(vec![SURFACE_PORT]),
        vec![OpacityInput::new(HALF, 0.5)],
        vec![
            Paint::Solid {
                id: LOWER_PAINT,
                target: LOWER_TARGET,
            },
            Paint::Solid {
                id: UPPER_SOLID,
                target: UPPER_TARGET,
            },
            Paint::Opacity {
                id: UPPER_PAINT,
                source: UPPER_SOLID,
                opacity: HALF,
            },
        ],
        vec![
            Surface::Input {
                id: ROOT,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: DERIVED,
                occurrence: LOWER,
            },
        ],
        vec![
            Occurrence::new(
                LOWER,
                LOWER_PAINT,
                ROOT,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
            Occurrence::new(
                UPPER,
                UPPER_PAINT,
                DERIVED,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                UPPER,
                Srgb8::new([0xC0; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(NESTED_OUTPUT, UPPER_PAINT)],
        ExactSrgb8IdentityV1,
    );
    let compiled = program.compile().unwrap();
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session
        .commit(observed_update(STREAM_A, 1, &[(1, [0x00; 3])]))
        .unwrap();
    let SessionState::Ready { current } = state else {
        panic!("upper occurrence must compose over the lower visible result");
    };
    assert!(!current.report().cells()[0].result().is_violation());
    let [output] = current.outputs() else {
        panic!("nested program must emit its Paint, not visible composite");
    };
    assert_eq!(output.output(), NESTED_OUTPUT);
    assert_eq!(output.paint().id(), UPPER_PAINT);
    assert_eq!(output.paint().source(), Srgb8::new([0xFF; 3]));
}

#[test]
fn raw_head_and_program_report_share_one_observation_backing() {
    let compiled = exact_compiled(ConstraintSet::new(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
        vec![],
    ));
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    session
        .commit(observed_update(STREAM_A, 1, &[(9, [0xFF; 3])]))
        .unwrap();
    let ObservationHeadViewV1::Observed(raw) = session.raw_head() else {
        panic!("successful observed update must own a raw observed head");
    };
    let SessionState::Ready { current } = session.state() else {
        panic!("fixture must verify");
    };
    let report = current.report().observation();
    assert_eq!(raw, report);
    assert_eq!(raw.backing_ptr_for_test(), report.backing_ptr_for_test());
    assert_eq!(raw.schema_ptr_for_test(), report.schema_ptr_for_test());
    assert_eq!(
        raw.physical_values(0).unwrap().as_ptr(),
        report.physical_values(0).unwrap().as_ptr(),
    );
    assert_eq!(
        raw.provenance(0).unwrap().as_ptr(),
        report.provenance(0).unwrap().as_ptr(),
    );
}
