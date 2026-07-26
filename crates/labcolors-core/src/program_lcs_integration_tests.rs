use crate::Srgb8;
use crate::appearance::{
    OccurrenceId, OpacityInputId, PaintId, PointOpacityOverSurfaceV1, SurfaceId, SurfaceInputPortId,
};
use crate::constraints::{
    ExactSrgb8IdentityV1, ProgramPointAssessmentErrorV1, ProgramVisiblePointBindingV1,
    Wcag22Srgb8V1, assess_program_point_hard,
};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId, AppearanceState,
    BackgroundLuminanceRatio, ColorSignal, HueState, IEC_SRGB_D65_XYZ_FRAME_V1,
    MODELED_TRISTIMULUS_DERIVATION_CALLS, ModeledLcsOccurrenceV1, SurroundProfileId,
};
use crate::observation::{
    ObservationGroupId, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::program_session::{
    CompiledProgram, CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    ObservationGroup, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint, Program,
    ProgramConstraintResultV1, Source, SourceId, Surface, Target, TargetId,
};
use crate::session::SessionState;
use crate::spaces::cam16::FORWARD_CALLS;
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22CriterionV1};

const BLACK_SOURCE: SourceId = SourceId::new(1);
const WHITE_SOURCE: SourceId = SourceId::new(2);
const BLACK_TARGET: TargetId = TargetId::new(1);
const WHITE_TARGET: TargetId = TargetId::new(2);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(3);
const HALF: OpacityInputId = OpacityInputId::new(4);

const BLACK_SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT_BLACK: PaintId = PaintId::new(11);
const WHITE_SOLID: PaintId = PaintId::new(12);
const BACKDROP: SurfaceId = SurfaceId::new(20);

const AVERAGE_OCCURRENCE: OccurrenceId = OccurrenceId::new(30);
const DIM_OCCURRENCE: OccurrenceId = OccurrenceId::new(31);
const AVERAGE_CONSTRAINT: ConstraintId = ConstraintId::new(40);
const DIM_CONSTRAINT: ConstraintId = ConstraintId::new(41);

const BLACK_OUTPUT: OutputSlotId = OutputSlotId::new(50);
const GROUP: ObservationGroupId = ObservationGroupId::new(60);
const STREAM_A: ObservationStreamId = ObservationStreamId::new(70);
const STREAM_B: ObservationStreamId = ObservationStreamId::new(71);

fn signal(bytes: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(bytes))
}

fn context(surround: SurroundProfileId) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        surround,
    )
}

fn observed_white(stream: ObservationStreamId) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal([0xFF; 3]))],
            }],
        }),
    }
}

fn observed_two_backdrops(stream: ObservationStreamId) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![
                ScenarioInput {
                    id: ScenarioId::new(1),
                    bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal([0xFF; 3]))],
                },
                ScenarioInput {
                    id: ScenarioId::new(2),
                    bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal([0x80; 3]))],
                },
            ],
        }),
    }
}

fn compiled_program(
    declarations: &[(OccurrenceId, ConstraintId, AppearanceContextId)],
    expected_visible: Srgb8,
    opacity: f64,
    permuted: bool,
) -> CompiledProgram<ExactSrgb8IdentityV1> {
    let mut sources = vec![
        Source::new(BLACK_SOURCE, signal([0; 3])),
        Source::new(WHITE_SOURCE, signal([0xFF; 3])),
    ];
    let mut targets = vec![
        Target::fixed(BLACK_TARGET, BLACK_SOURCE),
        Target::fixed(WHITE_TARGET, WHITE_SOURCE),
    ];
    let mut paints = vec![
        Paint::Solid {
            id: BLACK_SOLID,
            target: BLACK_TARGET,
        },
        Paint::Opacity {
            id: TRANSLUCENT_BLACK,
            source: BLACK_SOLID,
            opacity: HALF,
        },
        Paint::Solid {
            id: WHITE_SOLID,
            target: WHITE_TARGET,
        },
    ];
    let mut occurrences = declarations
        .iter()
        .map(|(occurrence, _, occurrence_context)| {
            Occurrence::new(
                *occurrence,
                TRANSLUCENT_BLACK,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                *occurrence_context,
            )
        })
        .collect::<Vec<_>>();
    let mut hard = declarations
        .iter()
        .map(|(occurrence, constraint, _)| {
            ConstraintInvocation::hard(*constraint, *occurrence, expected_visible)
        })
        .collect::<Vec<_>>();
    let mut outputs = vec![OutputBinding::new(BLACK_OUTPUT, TRANSLUCENT_BLACK)];

    if permuted {
        sources.reverse();
        targets.reverse();
        paints.reverse();
        occurrences.reverse();
        hard.reverse();
        outputs.reverse();
    }

    Program::new(
        sources,
        targets,
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![OpacityInput::new(HALF, opacity)],
        paints,
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        occurrences,
        ConstraintSet::new(hard, vec![]),
        outputs,
        ExactSrgb8IdentityV1,
    )
    .compile()
    .unwrap()
}

fn compiled_wcag_program(opacity: f64) -> CompiledProgram<Wcag22Srgb8V1> {
    Program::new(
        vec![
            Source::new(BLACK_SOURCE, signal([0; 3])),
            Source::new(WHITE_SOURCE, signal([0xFF; 3])),
        ],
        vec![
            Target::fixed(BLACK_TARGET, BLACK_SOURCE),
            Target::fixed(WHITE_TARGET, WHITE_SOURCE),
        ],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![OpacityInput::new(HALF, opacity)],
        vec![
            Paint::Solid {
                id: BLACK_SOLID,
                target: BLACK_TARGET,
            },
            Paint::Opacity {
                id: TRANSLUCENT_BLACK,
                source: BLACK_SOLID,
                opacity: HALF,
            },
            Paint::Solid {
                id: WHITE_SOLID,
                target: WHITE_TARGET,
            },
        ],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            AVERAGE_OCCURRENCE,
            TRANSLUCENT_BLACK,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                AVERAGE_CONSTRAINT,
                AVERAGE_OCCURRENCE,
                Wcag22CriterionV1::Sc143TextDefault,
            )],
            vec![],
        ),
        vec![OutputBinding::new(BLACK_OUTPUT, TRANSLUCENT_BLACK)],
        Wcag22Srgb8V1,
    )
    .compile()
    .unwrap()
}

fn compiled_duplicate_constraint_program() -> CompiledProgram<ExactSrgb8IdentityV1> {
    Program::new(
        vec![Source::new(BLACK_SOURCE, signal([0; 3]))],
        vec![Target::fixed(BLACK_TARGET, BLACK_SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![OpacityInput::new(HALF, 0.5)],
        vec![
            Paint::Solid {
                id: BLACK_SOLID,
                target: BLACK_TARGET,
            },
            Paint::Opacity {
                id: TRANSLUCENT_BLACK,
                source: BLACK_SOLID,
                opacity: HALF,
            },
        ],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            AVERAGE_OCCURRENCE,
            TRANSLUCENT_BLACK,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![],
            vec![
                ConstraintInvocation::report_only(
                    AVERAGE_CONSTRAINT,
                    AVERAGE_OCCURRENCE,
                    Srgb8::new([0x80; 3]),
                ),
                ConstraintInvocation::report_only(
                    DIM_CONSTRAINT,
                    AVERAGE_OCCURRENCE,
                    Srgb8::new([0x80; 3]),
                ),
            ],
        ),
        vec![OutputBinding::new(BLACK_OUTPUT, TRANSLUCENT_BLACK)],
        ExactSrgb8IdentityV1,
    )
    .compile()
    .unwrap()
}

fn assert_binding_matches_modeled(
    binding: ProgramVisiblePointBindingV1,
    modeled: ModeledLcsOccurrenceV1,
) {
    assert_eq!(binding.modeled_lcs(), modeled);
    assert_eq!(
        binding.physical().occurrence().output_rgb(),
        modeled.signal().srgb8().bytes(),
    );
}

#[test]
fn ready_cell_binds_the_actual_visible_signal_to_the_exact_authored_context() {
    let average = context(SurroundProfileId::AverageV1);
    let compiled = compiled_program(
        &[(AVERAGE_OCCURRENCE, AVERAGE_CONSTRAINT, average)],
        Srgb8::new([0x80; 3]),
        0.5,
        false,
    );
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Ready { current } = state else {
        panic!("the exact visible composite must pass");
    };
    let [cell] = current.report().cells() else {
        panic!("one physical case times one constraint must produce one cell");
    };

    let modeled = cell.modeled_lcs_occurrence();
    assert_eq!(
        modeled.derivation().provenance().source_signal(),
        signal([0x80; 3]),
        "provenance must name the visible source-over result, not the Paint source or backdrop",
    );
    assert_eq!(modeled.occurrence().context(), average);
    let ProgramConstraintResultV1::Pass(evidence) = cell.result() else {
        panic!("exact equality must retain typed pass evidence");
    };
    assert_binding_matches_modeled(*evidence.binding(), modeled);
    assert_eq!(*evidence.measurement().value(), Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.invocation(), Srgb8::new([0x80; 3]));
}

#[test]
fn identical_physical_bytes_share_tristimulus_but_contextual_cam16_views_diverge() {
    let average = context(SurroundProfileId::AverageV1);
    let dim = context(SurroundProfileId::DimV1);
    let compiled = compiled_program(
        &[
            (AVERAGE_OCCURRENCE, AVERAGE_CONSTRAINT, average),
            (DIM_OCCURRENCE, DIM_CONSTRAINT, dim),
        ],
        Srgb8::new([0x80; 3]),
        0.5,
        false,
    );
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Ready { current } = state else {
        panic!("both context-bound declarations assess the same passing physical point");
    };
    let [average_cell, dim_cell] = current.report().cells() else {
        panic!("one case times two constraints must produce two canonical cells");
    };
    let average_modeled = average_cell.modeled_lcs_occurrence();
    let dim_modeled = dim_cell.modeled_lcs_occurrence();

    assert_eq!(
        average_modeled.derivation().provenance().source_signal(),
        dim_modeled.derivation().provenance().source_signal(),
    );
    assert_eq!(
        average_modeled.derivation().sample(),
        dim_modeled.derivation().sample(),
    );
    assert_eq!(average_modeled.occurrence().context(), average);
    assert_eq!(dim_modeled.occurrence().context(), dim);
    assert_ne!(average_modeled.occurrence(), dim_modeled.occurrence());

    let average_state = AppearanceState::derive_v1(average_modeled.occurrence()).unwrap();
    let dim_state = AppearanceState::derive_v1(dim_modeled.occurrence()).unwrap();
    assert_eq!(average_state.oklab(), dim_state.oklab());
    assert_ne!(
        average_state.cam16().j().to_bits(),
        dim_state.cam16().j().to_bits(),
        "CAM16 must be evaluated under each occurrence's own context",
    );
}

#[test]
fn exact_black_visible_occurrence_keeps_undefined_hue_in_lcs() {
    let average = context(SurroundProfileId::AverageV1);
    let compiled = compiled_program(
        &[(AVERAGE_OCCURRENCE, AVERAGE_CONSTRAINT, average)],
        Srgb8::new([0x00; 3]),
        1.0,
        false,
    );
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Ready { current } = state else {
        panic!("fixture must verify");
    };
    let [cell] = current.report().cells() else {
        panic!("the complete report must retain its sole visible occurrence");
    };
    let modeled = cell.modeled_lcs_occurrence();
    assert_eq!(modeled.signal(), signal([0x00; 3]));
    let state = AppearanceState::derive_v1(modeled.occurrence()).unwrap();
    assert_eq!(state.cam16().hue(), HueState::UndefinedExact);
}

#[test]
fn hard_violation_retains_modeled_lcs_and_commits_no_current_outputs() {
    let average = context(SurroundProfileId::AverageV1);
    let compiled = compiled_program(
        &[(AVERAGE_OCCURRENCE, AVERAGE_CONSTRAINT, average)],
        Srgb8::new([0x7F; 3]),
        0.5,
        false,
    );
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Failed { cause, previous } = state else {
        panic!("the mandatory exact constraint must fail");
    };
    assert!(
        previous.is_none(),
        "a fresh hard failure must not expose any previous Ready outputs",
    );
    let [cell] = cause.report().cells() else {
        panic!("the complete failed report must retain its sole cell");
    };
    assert!(cell.result().is_violation());
    let modeled = cell.modeled_lcs_occurrence();
    assert_eq!(
        modeled.derivation().provenance().source_signal(),
        signal([0x80; 3]),
    );
    assert_eq!(modeled.occurrence().context(), average);
    let ProgramConstraintResultV1::Violation(evidence) = cell.result() else {
        panic!("exact mismatch must retain typed violation evidence");
    };
    assert_binding_matches_modeled(*evidence.binding(), modeled);
    assert_eq!(*evidence.measurement().value(), Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.invocation(), Srgb8::new([0x7F; 3]));
    // `ProgramConflictV1` intentionally exposes only `report()`: current
    // outputs exist exclusively on the `ProgramVerifiedV1` Ready branch.
}

#[test]
fn program_wcag_pass_binds_physical_and_modeled_occurrence_to_one_evidence() {
    let compiled = compiled_wcag_program(1.0);
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Ready { current } = state else {
        panic!("opaque black on white must pass WCAG default text");
    };
    let [cell] = current.report().cells() else {
        panic!("one WCAG declaration must produce one report cell");
    };
    let ProgramConstraintResultV1::Pass(evidence) = cell.result() else {
        panic!("WCAG pass must retain typed pass evidence");
    };
    let modeled = cell.modeled_lcs_occurrence();
    assert_binding_matches_modeled(*evidence.binding(), modeled);
    let measurement = evidence.measurement().value();
    assert_eq!(measurement.decision(), Wcag22ApplicableDecisionV1::Pass);
    assert_eq!(measurement.measurement().foreground, [0; 3]);
    assert_eq!(measurement.measurement().background, [0xFF; 3]);
    assert_eq!(
        evidence.binding().physical().occurrence().backdrop_rgb(),
        measurement.measurement().background,
    );
    assert_eq!(*evidence.invocation(), Wcag22CriterionV1::Sc143TextDefault,);
}

#[test]
fn program_wcag_violation_retains_coherent_evidence_without_current_outputs() {
    let compiled = compiled_wcag_program(0.5);
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Failed { cause, previous } = state else {
        panic!("half-black composite on white must violate WCAG default text");
    };
    assert!(previous.is_none());
    let [cell] = cause.report().cells() else {
        panic!("one WCAG declaration must produce one failed report cell");
    };
    let ProgramConstraintResultV1::Violation(evidence) = cell.result() else {
        panic!("WCAG failure must retain typed violation evidence");
    };
    let modeled = cell.modeled_lcs_occurrence();
    assert_binding_matches_modeled(*evidence.binding(), modeled);
    let measurement = evidence.measurement().value();
    assert_eq!(measurement.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(measurement.measurement().foreground, [0x80; 3]);
    assert_eq!(measurement.measurement().background, [0xFF; 3]);
    assert_eq!(
        evidence.binding().physical().occurrence().backdrop_rgb(),
        measurement.measurement().background,
    );
    assert_eq!(
        modeled.signal(),
        signal(measurement.measurement().foreground)
    );
}

#[test]
fn mismatched_modeled_signal_is_rejected_before_program_evaluation() {
    let physical = PointOpacityOverSurfaceV1::evaluate([0; 3], 1.0, [0xFF; 3]).unwrap();
    let mismatched = ModeledLcsOccurrenceV1::from_signal_in_context(
        signal([0xFF; 3]),
        context(SurroundProfileId::AverageV1),
    )
    .unwrap();

    let error = assess_program_point_hard(
        &physical,
        mismatched,
        &ExactSrgb8IdentityV1,
        Srgb8::new([0; 3]),
    )
    .expect_err("binding mismatch must be rejected before the evaluator can run");
    let mismatch = match error {
        ProgramPointAssessmentErrorV1::Binding(mismatch) => mismatch,
        ProgramPointAssessmentErrorV1::Evaluator(unreachable) => match unreachable {},
    };
    assert_eq!(mismatch.physical(), Srgb8::new([0; 3]));
    assert_eq!(mismatch.modeled(), Srgb8::new([0xFF; 3]));
}

#[test]
fn program_derives_lcs_once_per_target_and_case_without_eager_cam16() {
    let compiled = compiled_duplicate_constraint_program();
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    FORWARD_CALLS.with(|calls| calls.set(0));

    let state = session.update(observed_two_backdrops(STREAM_A)).unwrap();

    let derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get());
    let cam16_forwards = FORWARD_CALLS.with(|calls| calls.get());
    let SessionState::Ready { current } = state else {
        panic!("report-only constraints must retain the complete two-case result");
    };
    assert_eq!(current.report().cells().len(), 4);
    assert_eq!(
        derivations, 2,
        "two constraints sharing one target must reuse one LCS derivation in each physical case",
    );
    assert_eq!(
        cam16_forwards, 0,
        "Program lowering must not eagerly derive the contextual CAM16 view",
    );
}

#[test]
fn declaration_permutations_preserve_canonical_lcs_cells_and_output_signals() {
    let declarations = [
        (
            AVERAGE_OCCURRENCE,
            AVERAGE_CONSTRAINT,
            context(SurroundProfileId::AverageV1),
        ),
        (
            DIM_OCCURRENCE,
            DIM_CONSTRAINT,
            context(SurroundProfileId::DimV1),
        ),
    ];
    let canonical = compiled_program(&declarations, Srgb8::new([0x80; 3]), 0.5, false);
    let permuted = compiled_program(&declarations, Srgb8::new([0x80; 3]), 0.5, true);
    let mut canonical_session = canonical.instantiate(STREAM_A).unwrap();
    let mut permuted_session = permuted.instantiate(STREAM_B).unwrap();

    let canonical_state = canonical_session.update(observed_white(STREAM_A)).unwrap();
    let permuted_state = permuted_session.update(observed_white(STREAM_B)).unwrap();
    let SessionState::Ready { current: canonical } = canonical_state else {
        panic!("canonical declarations must verify");
    };
    let SessionState::Ready { current: permuted } = permuted_state else {
        panic!("permuted declarations must verify");
    };

    let cell_signature = |cell: &crate::program_session::ProgramConstraintCellV1<_>| {
        (
            cell.case_index(),
            cell.constraint(),
            cell.target(),
            cell.modeled_lcs_occurrence(),
            cell.result().is_violation(),
        )
    };
    assert_eq!(
        canonical
            .report()
            .cells()
            .iter()
            .map(cell_signature)
            .collect::<Vec<_>>(),
        permuted
            .report()
            .cells()
            .iter()
            .map(cell_signature)
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        canonical
            .outputs()
            .iter()
            .map(|output| (output.output(), output.source_signal()))
            .collect::<Vec<_>>(),
        permuted
            .outputs()
            .iter()
            .map(|output| (output.output(), output.source_signal()))
            .collect::<Vec<_>>(),
    );
}
