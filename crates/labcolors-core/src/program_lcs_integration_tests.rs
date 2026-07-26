use crate::Srgb8;
use crate::appearance::{
    OccurrenceId, OpacityInputId, PaintId, PointOpacityOverSurfaceV1, SurfaceId, SurfaceInputPortId,
};
use crate::constraints::{
    ExactSrgb8IdentityV1, HardDecision, LcsProbeProgramEvaluatorV1, ProgramLcsDependencyReleaseV1,
    ProgramLcsPointAdapterV1, ProgramLcsPointAssessmentErrorV1, ProgramPointOccurrenceV1,
    ProgramVisiblePointBindingV1, Wcag22Srgb8V1, assess_program_lcs_point_hard,
};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1,
    MODELED_TRISTIMULUS_DERIVATION_CALLS, MUTATION_SENTINEL_XYZ_FRAME_V1, SurroundProfileId,
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

fn assert_binding_matches_physical(
    binding: ProgramVisiblePointBindingV1,
    expected_context: AppearanceContextId,
    expected_visible: Srgb8,
) {
    assert_eq!(binding.context(), expected_context);
    assert_eq!(
        binding.physical().occurrence().output_rgb(),
        expected_visible.bytes()
    );
}

#[test]
fn ready_cell_binds_the_actual_visible_signal_and_declared_context_without_lcs() {
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

    assert_eq!(cell.appearance_context(), average);
    let ProgramConstraintResultV1::Pass(evidence) = cell.result() else {
        panic!("exact equality must retain typed pass evidence");
    };
    assert_binding_matches_physical(*evidence.binding(), average, Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.measurement().value(), Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.invocation(), Srgb8::new([0x80; 3]));
}

#[test]
fn identical_physical_bytes_keep_distinct_declared_contexts_without_deriving_views() {
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
    assert_eq!(average_cell.appearance_context(), average);
    assert_eq!(dim_cell.appearance_context(), dim);
    assert_ne!(
        average_cell.appearance_context(),
        dim_cell.appearance_context()
    );
    for cell in [average_cell, dim_cell] {
        let ProgramConstraintResultV1::Pass(evidence) = cell.result() else {
            panic!("both exact constraints must pass");
        };
        assert_eq!(
            evidence.binding().physical().occurrence().output_rgb(),
            [0x80; 3]
        );
        assert_eq!(*evidence.measurement().value(), Srgb8::new([0x80; 3]));
    }
}

#[test]
fn exact_black_visible_occurrence_does_not_construct_a_colorimetric_view() {
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
    assert_eq!(cell.appearance_context(), average);
    let ProgramConstraintResultV1::Pass(evidence) = cell.result() else {
        panic!("exact black identity must pass");
    };
    assert_eq!(*evidence.measurement().value(), Srgb8::new([0; 3]));
}

#[test]
fn hard_violation_retains_physical_binding_and_context_without_current_outputs() {
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
    assert_eq!(cell.appearance_context(), average);
    let ProgramConstraintResultV1::Violation(evidence) = cell.result() else {
        panic!("exact mismatch must retain typed violation evidence");
    };
    assert_binding_matches_physical(*evidence.binding(), average, Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.measurement().value(), Srgb8::new([0x80; 3]));
    assert_eq!(*evidence.invocation(), Srgb8::new([0x7F; 3]));
    // `ProgramConflictV1` intentionally exposes only `report()`: current
    // outputs exist exclusively on the `ProgramVerifiedV1` Ready branch.
}

#[test]
fn program_wcag_pass_binds_physical_occurrence_and_declared_context() {
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
    assert_binding_matches_physical(
        *evidence.binding(),
        context(SurroundProfileId::AverageV1),
        Srgb8::new([0; 3]),
    );
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
fn program_wcag_violation_retains_physical_evidence_without_current_outputs() {
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
    assert_binding_matches_physical(
        *evidence.binding(),
        context(SurroundProfileId::AverageV1),
        Srgb8::new([0x80; 3]),
    );
    let measurement = evidence.measurement().value();
    assert_eq!(measurement.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(measurement.measurement().foreground, [0x80; 3]);
    assert_eq!(measurement.measurement().background, [0xFF; 3]);
    assert_eq!(
        evidence.binding().physical().occurrence().backdrop_rgb(),
        measurement.measurement().background,
    );
}

#[test]
fn one_occurrence_scoped_adapter_serves_two_lcs_constraints_with_one_derivation() {
    let physical = PointOpacityOverSurfaceV1::evaluate([0; 3], 0.5, [0xFF; 3]).unwrap();
    let average = context(SurroundProfileId::AverageV1);
    let point = ProgramPointOccurrenceV1::from_resolved(&physical, average);
    let adapter = ProgramLcsPointAdapterV1::new(point);
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));

    let first = assess_program_lcs_point_hard(&adapter, &LcsProbeProgramEvaluatorV1, 1).unwrap();
    let second = assess_program_lcs_point_hard(&adapter, &LcsProbeProgramEvaluatorV1, 2).unwrap();
    let HardDecision::Pass(first) = first else {
        panic!("the LCS probe has no violation branch");
    };
    let HardDecision::Pass(second) = second else {
        panic!("the LCS probe has no violation branch");
    };

    assert_eq!(first.binding(), second.binding());
    assert_eq!(first.binding().physical(), point.binding());
    assert_eq!(*first.release(), ProgramLcsDependencyReleaseV1::current());
    assert_eq!((*first.invocation(), *second.invocation()), (1, 2));
    assert_eq!(first.measurement().value(), second.measurement().value());
    assert_eq!(first.binding().modeled_lcs(), *first.measurement().value());
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get()),
        1,
        "two LCS-aware constraints over one occurrence scope must share one derivation",
    );
}

#[test]
fn one_occurrence_scoped_adapter_memoizes_failure_across_two_lcs_constraints() {
    let physical = PointOpacityOverSurfaceV1::evaluate([0; 3], 0.5, [0xFF; 3]).unwrap();
    let incompatible = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        MUTATION_SENTINEL_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    );
    let point = ProgramPointOccurrenceV1::from_resolved(&physical, incompatible);
    let adapter = ProgramLcsPointAdapterV1::new(point);
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));

    let first = assess_program_lcs_point_hard(&adapter, &LcsProbeProgramEvaluatorV1, 1)
        .expect_err("the incompatible frame must fail");
    let second = assess_program_lcs_point_hard(&adapter, &LcsProbeProgramEvaluatorV1, 2)
        .expect_err("the memoized incompatible frame must fail identically");

    assert_eq!(first, second);
    assert!(matches!(
        first,
        ProgramLcsPointAssessmentErrorV1::Formation(_)
    ));
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get()),
        1,
        "one adapter must cache a failed derivation as well as a successful one",
    );
}

#[test]
fn equal_bytes_and_context_cannot_mix_provenance_between_physical_occurrences() {
    let context = context(SurroundProfileId::AverageV1);
    let translucent = PointOpacityOverSurfaceV1::evaluate([0; 3], 0.5, [0xFF; 3]).unwrap();
    let solid = PointOpacityOverSurfaceV1::evaluate([0x80; 3], 1.0, [0; 3]).unwrap();
    assert_eq!(translucent.visible(), solid.visible());
    assert_ne!(
        translucent.visible_point_binding(),
        solid.visible_point_binding()
    );
    let translucent_point = ProgramPointOccurrenceV1::from_resolved(&translucent, context);
    let solid_point = ProgramPointOccurrenceV1::from_resolved(&solid, context);
    let translucent_adapter = ProgramLcsPointAdapterV1::new(translucent_point);
    let solid_adapter = ProgramLcsPointAdapterV1::new(solid_point);
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));

    let translucent_evidence =
        assess_program_lcs_point_hard(&translucent_adapter, &LcsProbeProgramEvaluatorV1, 1)
            .unwrap();
    let solid_evidence =
        assess_program_lcs_point_hard(&solid_adapter, &LcsProbeProgramEvaluatorV1, 1).unwrap();
    let HardDecision::Pass(translucent_evidence) = translucent_evidence else {
        panic!("the LCS probe has no violation branch");
    };
    let HardDecision::Pass(solid_evidence) = solid_evidence else {
        panic!("the LCS probe has no violation branch");
    };

    assert_eq!(
        translucent_evidence.binding().physical(),
        translucent_point.binding()
    );
    assert_eq!(solid_evidence.binding().physical(), solid_point.binding());
    assert_ne!(
        translucent_evidence.binding().physical(),
        solid_evidence.binding().physical()
    );
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get()),
        2,
        "byte equality must not collapse distinct physical occurrence scopes",
    );
}

#[test]
fn exact_report_only_program_does_not_derive_lcs() {
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
        derivations, 0,
        "encoded-only constraints must not pay for or depend on a derived LCS occurrence",
    );
    assert_eq!(
        cam16_forwards, 0,
        "Program lowering must not eagerly derive the contextual CAM16 view",
    );
}

#[test]
fn wcag_program_does_not_derive_lcs() {
    let compiled = compiled_wcag_program(1.0);
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    let state = session.update(observed_white(STREAM_A)).unwrap();
    assert!(matches!(state, SessionState::Ready { .. }));
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get()),
        0,
        "WCAG must consume the final encoded pair without constructing LCS",
    );
}

#[test]
fn encoded_only_program_is_not_rejected_by_an_lcs_incompatible_declared_context() {
    let incompatible = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        MUTATION_SENTINEL_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    );
    let compiled = compiled_program(
        &[(AVERAGE_OCCURRENCE, AVERAGE_CONSTRAINT, incompatible)],
        Srgb8::new([0x80; 3]),
        0.5,
        false,
    );
    let mut session = compiled.instantiate(STREAM_A).unwrap();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));

    let state = session.update(observed_white(STREAM_A)).unwrap();
    let SessionState::Ready { current } = state else {
        panic!("an encoded evaluator must not inherit an unrelated LCS frame requirement");
    };
    let [cell] = current.report().cells() else {
        panic!("the exact constraint must still emit one evidence cell");
    };
    assert_eq!(cell.appearance_context(), incompatible);
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.get()),
        0,
        "an LCS-incompatible declared context must not be derived by an encoded-only constraint",
    );
}

#[test]
fn declaration_permutations_preserve_canonical_context_cells_and_output_signals() {
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
            cell.appearance_context(),
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
