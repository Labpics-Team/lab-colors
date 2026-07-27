use crate::Srgb8;
use crate::appearance::{
    ExactFinalOwnedPointDomainV1, OccurrenceId, OpacityInputId, PaintId,
    PointOccurrenceAbsenceReleaseV1, SurfaceId, SurfaceInputPortId,
};
use crate::constraints::{CountingProgramWcag22Srgb8V1, ExactSrgb8IdentityV1};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    ObservationGroupId, ObservationHeadViewV1, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput,
    SurfaceInputBinding, UnknownReasonId,
};
use crate::program_session::{
    CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    DeclaredJointSelectionV1, JointCandidateStateV1, ObservationGroup, Occurrence, OpacityInput,
    OutputBinding, OutputSlotId, Paint, PointPresentationRootV1, PointPresentationTargetV1,
    PresentationRootId, Program, ProgramPointCausalConsideredStateV1,
    ProgramPointCausalSelectedStateV1, ProgramSessionEvaluationError, Source, SourceId, Surface,
    Target, TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1, TargetId,
    checked_program_point_causal_cardinality_for_test, fail_program_preflight_reservation_for_test,
};
use crate::session::{SessionState, SessionUpdateError};
use crate::wcag22::Wcag22CriterionV1;

const SOURCE: SourceId = SourceId::new(1);
const TARGET: TargetId = TargetId::new(2);
const PORT: SurfaceInputPortId = SurfaceInputPortId::new(3);
const PAINT: PaintId = PaintId::new(4);
const SURFACE: SurfaceId = SurfaceId::new(5);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(6);
const CONSTRAINT: ConstraintId = ConstraintId::new(7);
const OUTPUT: OutputSlotId = OutputSlotId::new(8);
const ROOT: PresentationRootId = PresentationRootId::new(9);
const GROUP: ObservationGroupId = ObservationGroupId::new(10);
const STREAM: ObservationStreamId = ObservationStreamId::new(11);

fn signal(value: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(value))
}

fn context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn preflight_program(
    evaluator: CountingProgramWcag22Srgb8V1,
) -> crate::program_session::CompiledProgram<CountingProgramWcag22Srgb8V1> {
    preflight_program_with_presentations(evaluator, true)
}

fn preflight_program_with_presentations(
    evaluator: CountingProgramWcag22Srgb8V1,
    declare_presentations: bool,
) -> crate::program_session::CompiledProgram<CountingProgramWcag22Srgb8V1> {
    preflight_program_with_mode(evaluator, declare_presentations, true)
}

fn preflight_program_with_mode(
    evaluator: CountingProgramWcag22Srgb8V1,
    declare_presentations: bool,
    hard: bool,
) -> crate::program_session::CompiledProgram<CountingProgramWcag22Srgb8V1> {
    let constraints = if hard {
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                CONSTRAINT,
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        )
    } else {
        ConstraintSet::new(
            vec![],
            vec![ConstraintInvocation::report_only(
                CONSTRAINT,
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
        )
    };
    let program = Program::new(
        vec![Source::new(SOURCE, signal([0xFF; 3]))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        constraints,
        vec![OutputBinding::new(OUTPUT, PAINT)],
        evaluator,
    );
    let program = if declare_presentations {
        program.with_point_presentations(
            vec![PointPresentationRootV1::new(ROOT, OCCURRENCE)],
            vec![PointPresentationTargetV1::new(ROOT, OCCURRENCE)],
        )
    } else {
        program
    };
    program.compile().unwrap()
}

fn observed(revision: u64) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(PORT, signal([0; 3]))],
            }],
        }),
    }
}

fn observed_backdrop(revision: u64, backdrop: [u8; 3]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(PORT, signal(backdrop))],
            }],
        }),
    }
}

fn observed_backdrops(revision: u64, backdrops: &[(u32, [u8; 3])]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: backdrops
                .iter()
                .map(|&(scenario, backdrop)| ScenarioInput {
                    id: ScenarioId::new(scenario),
                    bindings: vec![SurfaceInputBinding::new(PORT, signal(backdrop))],
                })
                .collect(),
        }),
    }
}

fn unknown(revision: u64) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(90)),
    }
}

fn fanout_program() -> crate::program_session::CompiledProgram<ExactSrgb8IdentityV1> {
    let alpha_target = OpacityInputId::new(20);
    let alpha_opaque_root = OpacityInputId::new(21);
    let alpha_translucent_root = OpacityInputId::new(22);
    let target_paint = PaintId::new(30);
    let opaque_root_paint = PaintId::new(31);
    let translucent_root_paint = PaintId::new(32);
    let derived = SurfaceId::new(40);
    let opaque_root = OccurrenceId::new(50);
    let translucent_root = OccurrenceId::new(51);
    let opaque_root_id = PresentationRootId::new(60);
    let translucent_root_id = PresentationRootId::new(61);

    Program::new(
        vec![Source::new(SOURCE, signal([0; 3]))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![PORT]),
        vec![
            OpacityInput::new(alpha_target, 0.01),
            OpacityInput::new(alpha_opaque_root, 0.95),
            OpacityInput::new(alpha_translucent_root, 0.5),
        ],
        vec![
            Paint::Solid {
                id: PAINT,
                target: TARGET,
            },
            Paint::Opacity {
                id: target_paint,
                source: PAINT,
                opacity: alpha_target,
            },
            Paint::Opacity {
                id: opaque_root_paint,
                source: PAINT,
                opacity: alpha_opaque_root,
            },
            Paint::Opacity {
                id: translucent_root_paint,
                source: PAINT,
                opacity: alpha_translucent_root,
            },
        ],
        vec![
            Surface::Input {
                id: SURFACE,
                input: PORT,
            },
            Surface::FromOccurrence {
                id: derived,
                occurrence: OCCURRENCE,
            },
        ],
        vec![
            Occurrence::new(
                OCCURRENCE,
                target_paint,
                SURFACE,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            ),
            Occurrence::new(
                opaque_root,
                opaque_root_paint,
                derived,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            ),
            Occurrence::new(
                translucent_root,
                translucent_root_paint,
                derived,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                CONSTRAINT,
                OCCURRENCE,
                Srgb8::new([252; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, target_paint)],
        ExactSrgb8IdentityV1,
    )
    .with_point_presentations(
        vec![
            PointPresentationRootV1::new(opaque_root_id, opaque_root),
            PointPresentationRootV1::new(translucent_root_id, translucent_root),
        ],
        vec![
            PointPresentationTargetV1::new(opaque_root_id, OCCURRENCE),
            PointPresentationTargetV1::new(translucent_root_id, OCCURRENCE),
        ],
    )
    .compile()
    .unwrap()
}

#[test]
fn fixed_fanout_retains_empty_and_singleton_as_distinct_modeled_roots() {
    let compiled = fanout_program();
    assert_eq!(compiled.point_presentation_count(), 2);
    let expected_identity = compiled.content_identity();
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [255; 3])).unwrap()
    else {
        panic!("exact fixed fixture must verify");
    };

    let certificates = current.point_causal_certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 2);
    assert_eq!(
        certificates[0].steps().as_ptr_range().end,
        certificates[1].steps().as_ptr_range().start,
        "flat replay spans must be adjacent without overlap or gaps"
    );
    assert!(certificates.iter().all(|certificate| {
        certificate.content_identity() == expected_identity
            && certificate.observation().revision() == Revision::new(1)
            && certificate.state() == ProgramPointCausalSelectedStateV1::Fixed
            && certificate.case_index() == 0
            && certificate.release() == PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1
            && certificate.target() == OCCURRENCE
            && certificate.steps().len() == 2
            && certificate.steps()[0].occurrence() == OCCURRENCE
    }));

    let opaque = &certificates[0];
    assert_eq!(opaque.presentation_root(), PresentationRootId::new(60));
    assert_eq!(opaque.modeled_terminal_occurrence(), OccurrenceId::new(50));
    assert_eq!(
        opaque.modeled_terminal_codes(),
        opaque.modeled_terminal_without_target_codes()
    );
    assert_eq!(opaque.domain(), ExactFinalOwnedPointDomainV1::Empty);

    let translucent = &certificates[1];
    assert_eq!(translucent.presentation_root(), PresentationRootId::new(61));
    assert_eq!(
        translucent.modeled_terminal_occurrence(),
        OccurrenceId::new(51)
    );
    assert_ne!(
        translucent.modeled_terminal_codes(),
        translucent.modeled_terminal_without_target_codes()
    );
    assert_eq!(
        translucent.domain(),
        ExactFinalOwnedPointDomainV1::Singleton {
            visible: translucent.modeled_terminal_codes(),
        }
    );
}

#[test]
fn borrowed_causal_projection_does_not_recompute_or_allocate() {
    let compiled = fanout_program();
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [255; 3])).unwrap()
    else {
        panic!("exact fixed fixture must verify");
    };
    crate::composition::reset_source_over_evaluation_count();

    let (checksum, allocations) = crate::test_support::measured_allocations(|| {
        current
            .point_causal_certificates()
            .map(|certificate| {
                certificate.steps().len()
                    + certificate.case_index()
                    + usize::from(certificate.modeled_terminal_codes()[0])
                    + usize::from(certificate.modeled_terminal_without_target_codes()[0])
            })
            .sum::<usize>()
    });

    assert_ne!(checksum, 0);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn no_declared_presentation_means_no_causal_evidence() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let compiled = preflight_program_with_presentations(evaluator, false);
    assert_eq!(compiled.point_presentation_count(), 0);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed(1)).unwrap() else {
        panic!("the fixed control Program must verify");
    };
    assert_eq!(current.point_causal_certificates().len(), 0);
}

#[test]
fn report_only_fixed_program_collects_terminal_causality_and_outputs_in_one_pass() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = preflight_program_with_mode(evaluator, true, false);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [0; 3])).unwrap()
    else {
        panic!("report-only evidence cannot reject the fixed Program");
    };

    assert_eq!(calls.calls().len(), 1);
    assert_eq!(current.outputs().len(), 1);
    let certificate = current.point_causal_certificates().next().unwrap();
    assert_eq!(
        certificate.state(),
        ProgramPointCausalSelectedStateV1::Fixed
    );
    assert_eq!(certificate.case_index(), 0);
    assert_eq!(certificate.modeled_terminal_codes(), [255; 3]);
    assert_eq!(certificate.modeled_terminal_without_target_codes(), [0; 3]);
    assert_eq!(
        certificate.domain(),
        ExactFinalOwnedPointDomainV1::Singleton { visible: [255; 3] }
    );
}

fn finite_program(
    expected: Srgb8,
) -> crate::program_session::CompiledProgram<ExactSrgb8IdentityV1> {
    let dark = TargetCandidateId::new(70);
    let light = TargetCandidateId::new(71);
    Program::new(
        vec![Source::new(SOURCE, signal([0; 3]))],
        vec![Target::finite(
            TARGET,
            SOURCE,
            vec![
                TargetCandidateV1::new(dark, signal([0; 3])),
                TargetCandidateV1::new(light, signal([255; 3])),
            ],
        )],
        ObservationGroup::new(GROUP, vec![PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(CONSTRAINT, OCCURRENCE, expected)],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        ExactSrgb8IdentityV1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, dark)]),
        JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, light)]),
    ]))
    .with_point_presentations(
        vec![PointPresentationRootV1::new(ROOT, OCCURRENCE)],
        vec![PointPresentationTargetV1::new(ROOT, OCCURRENCE)],
    )
    .compile()
    .unwrap()
}

#[test]
fn exhaustive_conflict_retains_each_considered_state_without_minting_selection() {
    let compiled = finite_program(Srgb8::new([128; 3]));
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Failed {
        cause,
        previous: None,
    } = session.update(observed_backdrop(1, [0; 3])).unwrap()
    else {
        panic!("both authored states must remain hard-infeasible");
    };

    assert_eq!(cause.considered_state_count(), 2);
    let evidence_rows = cause.considered_point_causal_evidence().collect::<Vec<_>>();
    assert_eq!(evidence_rows.len(), 2);
    for (state_index, evidence) in evidence_rows.iter().enumerate() {
        assert_eq!(
            evidence.state(),
            ProgramPointCausalConsideredStateV1::Considered(state_index)
        );
        assert_eq!(evidence.case_index(), 0);
        assert_eq!(evidence.presentation_root(), ROOT);
        assert_eq!(evidence.target(), OCCURRENCE);
        assert_eq!(evidence.modeled_terminal_occurrence(), OCCURRENCE);
        assert_eq!(evidence.steps().len(), 1);
    }
    assert_eq!(
        evidence_rows[0].domain(),
        ExactFinalOwnedPointDomainV1::Empty
    );
    assert_eq!(evidence_rows[0].modeled_terminal_codes(), [0; 3]);
    assert_eq!(
        evidence_rows[1].domain(),
        ExactFinalOwnedPointDomainV1::Singleton { visible: [255; 3] }
    );
    assert_eq!(evidence_rows[1].modeled_terminal_codes(), [255; 3]);
}

#[test]
fn fixed_conflict_retains_fixed_causal_evidence_without_minting_selection() {
    let compiled = preflight_program(CountingProgramWcag22Srgb8V1::default());
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Failed {
        cause,
        previous: None,
    } = session.update(observed_backdrop(1, [255; 3])).unwrap()
    else {
        panic!("white on white must fail the fixed hard criterion");
    };

    assert_eq!(cause.considered_state_count(), 1);
    let evidence = cause.considered_point_causal_evidence().next().unwrap();
    assert_eq!(evidence.state(), ProgramPointCausalConsideredStateV1::Fixed);
    assert_eq!(evidence.case_index(), 0);
    assert_eq!(evidence.modeled_terminal_codes(), [255; 3]);
    assert_eq!(evidence.modeled_terminal_without_target_codes(), [255; 3]);
    assert_eq!(evidence.domain(), ExactFinalOwnedPointDomainV1::Empty);
}

#[test]
fn selected_certificate_comes_only_from_the_fresh_selected_state() {
    let compiled = finite_program(Srgb8::new([255; 3]));
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [0; 3])).unwrap()
    else {
        panic!("the second finite state must be the selected state");
    };

    assert_eq!(current.selected_state_index(), Some(1));
    let certificates = current.point_causal_certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert_eq!(
        certificates[0].state(),
        ProgramPointCausalSelectedStateV1::Selected(1)
    );
    assert_eq!(certificates[0].modeled_terminal_codes(), [255; 3]);
    assert_eq!(
        certificates[0].domain(),
        ExactFinalOwnedPointDomainV1::Singleton { visible: [255; 3] }
    );
}

#[test]
fn causal_rows_follow_canonical_physical_cases_not_duplicate_scenarios() {
    let compiled = finite_program(Srgb8::new([255; 3]));
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session
        .update(observed_backdrops(
            1,
            &[(3, [255; 3]), (1, [0; 3]), (2, [0; 3])],
        ))
        .unwrap()
    else {
        panic!("the light state must pass every canonical physical case");
    };

    assert_eq!(current.report().observation().physical_case_count(), 2);
    let certificates = current.point_causal_certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 2);
    assert_eq!(
        certificates
            .iter()
            .map(|certificate| certificate.case_index())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        certificates[0]
            .observation()
            .provenance(certificates[0].case_index())
            .unwrap(),
        &[ScenarioId::new(1), ScenarioId::new(2)]
    );
    assert_eq!(
        certificates[1]
            .observation()
            .provenance(certificates[1].case_index())
            .unwrap(),
        &[ScenarioId::new(3)]
    );
    assert!(certificates.iter().all(|certificate| {
        certificate.state() == ProgramPointCausalSelectedStateV1::Selected(1)
            && certificate.modeled_terminal_codes() == [255; 3]
    }));
    assert_eq!(
        certificates[0].modeled_terminal_without_target_codes(),
        [0; 3]
    );
    assert_eq!(
        certificates[0].domain(),
        ExactFinalOwnedPointDomainV1::Singleton { visible: [255; 3] }
    );
    assert_eq!(
        certificates[1].modeled_terminal_without_target_codes(),
        [255; 3]
    );
    assert_eq!(
        certificates[1].domain(),
        ExactFinalOwnedPointDomainV1::Empty
    );
}

#[test]
fn causal_cardinality_is_the_exact_state_case_presentation_product() {
    assert_eq!(
        checked_program_point_causal_cardinality_for_test(3, 2, 2, 5, 4, true),
        Some((6, 24, 15, 60))
    );
    assert_eq!(
        checked_program_point_causal_cardinality_for_test(3, 2, 2, 5, usize::MAX, false),
        Some((6, 0, 15, 0)),
        "report-only selection must not multiply by unreachable conflict states"
    );

    assert_eq!(
        checked_program_point_causal_cardinality_for_test(usize::MAX, 0, 2, 0, 1, false,),
        None
    );
    assert_eq!(
        checked_program_point_causal_cardinality_for_test(usize::MAX, 0, 0, 2, 1, false,),
        None
    );
    assert_eq!(
        checked_program_point_causal_cardinality_for_test(2, 0, 1, 0, usize::MAX, true),
        None
    );
    assert_eq!(
        checked_program_point_causal_cardinality_for_test(2, 0, 0, 1, usize::MAX, true),
        None
    );
}

#[test]
fn causal_evidence_remains_bound_to_its_own_revision_through_replay_and_stale() {
    let compiled = fanout_program();
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [255; 3])).unwrap()
    else {
        panic!("revision one must verify");
    };
    let first_steps = current
        .point_causal_certificates()
        .next()
        .unwrap()
        .steps()
        .as_ptr();

    crate::composition::reset_source_over_evaluation_count();
    let SessionState::Ready { current } = session.update(observed_backdrop(1, [255; 3])).unwrap()
    else {
        panic!("exact replay must preserve Ready");
    };
    let replayed = current.point_causal_certificates().next().unwrap();
    assert_eq!(replayed.observation().revision(), Revision::new(1));
    assert_eq!(replayed.steps().as_ptr(), first_steps);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let SessionState::Stale { previous } = session.update(unknown(2)).unwrap() else {
        panic!("unknown successor must retain the previous verified witness");
    };
    let stale = previous.point_causal_certificates().next().unwrap();
    assert_eq!(stale.observation().revision(), Revision::new(1));
    assert_eq!(stale.steps().as_ptr(), first_steps);

    crate::composition::reset_source_over_evaluation_count();
    let SessionState::Ready { current } = session.update(observed_backdrop(3, [255; 3])).unwrap()
    else {
        panic!("new observed revision must mint fresh evidence");
    };
    assert!(
        current
            .point_causal_certificates()
            .all(|certificate| certificate.observation().revision() == Revision::new(3))
    );
    assert!(crate::composition::source_over_evaluation_count() > 0);
}

#[test]
fn every_causal_preflight_reservation_precedes_graph_and_evaluator_work() {
    for reservation_index in 0..4 {
        let evaluator = CountingProgramWcag22Srgb8V1::default();
        let calls = evaluator.clone();
        let compiled = preflight_program(evaluator);
        let mut session = compiled.instantiate(STREAM).unwrap();
        crate::composition::reset_source_over_evaluation_count();

        let result = {
            let _failure = fail_program_preflight_reservation_for_test(reservation_index);
            session.update(observed(1))
        };
        let error = match result {
            Ok(_) => panic!("reservation {reservation_index} was not preflighted"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            SessionUpdateError::Plan(ProgramSessionEvaluationError::ResourceExhausted)
        );
        assert!(calls.calls().is_empty());
        assert_eq!(crate::composition::source_over_evaluation_count(), 0);
        assert!(matches!(session.state(), SessionState::Waiting));
        assert_eq!(session.raw_head(), ObservationHeadViewV1::Empty);
        assert!(matches!(
            session.update(observed(1)).unwrap(),
            SessionState::Ready { .. }
        ));
    }

    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = preflight_program(evaluator);
    let mut session = compiled.instantiate(STREAM).unwrap();
    crate::composition::reset_source_over_evaluation_count();
    let state = {
        let _failure = fail_program_preflight_reservation_for_test(4);
        session.update(observed(1)).unwrap()
    };
    assert!(matches!(state, SessionState::Ready { .. }));
    assert!(!calls.calls().is_empty());
    assert!(crate::composition::source_over_evaluation_count() > 0);
}
