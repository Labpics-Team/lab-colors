use crate::Srgb8;
use crate::appearance::{OccurrenceId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    ObservationGroupId, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::package_bridge::{
    PackageProgramCertificateKindV1, PackageProgramOperationV1, PackageProgramOwnerV1,
    PackageProgramScenarioV1, PackageProgramStateKindV1, PackageProgramUpdateErrorKindV1,
    PackageProgramUpdateV1,
};
use crate::program_session::{
    CompiledCoreProgramV1, CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    CoreProgramConstraintInvocationV1, CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1,
    CoreProgramV1, CoreProgramViolationEvidenceV1, DeclaredJointSelectionV1, JointCandidateStateV1,
    ObservationGroup, Occurrence, OutputBinding, OutputSlotId, Paint, Program,
    ProgramConstraintResultV1, Source, SourceId, Surface, Target, TargetCandidateChoiceV1,
    TargetCandidateId, TargetCandidateV1, TargetId,
};
use crate::session::SessionState;
use crate::wcag22::{Wcag22CriterionV1, wcag22_profile_v1};

const SOURCE: SourceId = SourceId::new(1);
const TARGET: TargetId = TargetId::new(2);
const PAINT: PaintId = PaintId::new(3);
const SURFACE: SurfaceId = SurfaceId::new(4);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(5);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(6);
const EXACT_CONSTRAINT: ConstraintId = ConstraintId::new(7);
const WCAG_CONSTRAINT: ConstraintId = ConstraintId::new(8);
const OUTPUT: OutputSlotId = OutputSlotId::new(9);
const GROUP: ObservationGroupId = ObservationGroupId::new(10);
const STREAM: ObservationStreamId = ObservationStreamId::new(11);

fn signal(bytes: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(bytes))
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

fn observed_white() -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal([0xFF; 3]))],
            }],
        }),
    }
}

fn observed_backdrops(backdrops: &[[u8; 3]]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: backdrops
                .iter()
                .enumerate()
                .map(|(index, backdrop)| ScenarioInput {
                    id: ScenarioId::new(index as u32 + 1),
                    bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal(*backdrop))],
                })
                .collect(),
        }),
    }
}

fn finite_program(candidate_signals: [[u8; 3]; 2]) -> CompiledCoreProgramV1 {
    const FIRST: TargetCandidateId = TargetCandidateId::new(1);
    const SECOND: TargetCandidateId = TargetCandidateId::new(2);
    let program: CoreProgramV1 = Program::new(
        vec![Source::new(SOURCE, signal(candidate_signals[0]))],
        vec![Target::finite(
            TARGET,
            SOURCE,
            vec![
                TargetCandidateV1::new(FIRST, signal(candidate_signals[0])),
                TargetCandidateV1::new(SECOND, signal(candidate_signals[1])),
            ],
        )],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                WCAG_CONSTRAINT,
                OCCURRENCE,
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(Wcag22CriterionV1::Sc143TextDefault),
            )],
            vec![ConstraintInvocation::report_only(
                EXACT_CONSTRAINT,
                OCCURRENCE,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
            )],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        CoreProgramEvaluatorsV1,
    );
    program
        .with_joint_selection(DeclaredJointSelectionV1::new(vec![
            JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, FIRST)]),
            JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, SECOND)]),
        ]))
        .compile()
        .unwrap()
}

#[test]
fn one_program_retains_typed_exact_and_wcag22_outcomes() {
    let program: CoreProgramV1 = Program::new(
        vec![Source::new(SOURCE, signal([0; 3]))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![
                ConstraintInvocation::hard(
                    EXACT_CONSTRAINT,
                    OCCURRENCE,
                    CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
                ),
                ConstraintInvocation::hard(
                    WCAG_CONSTRAINT,
                    OCCURRENCE,
                    CoreProgramConstraintInvocationV1::Wcag22Srgb8(
                        Wcag22CriterionV1::Sc143TextDefault,
                    ),
                ),
            ],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        CoreProgramEvaluatorsV1,
    );
    let compiled = program.compile().unwrap();

    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_white()).unwrap() else {
        panic!("opaque black on white must satisfy both authored hard constraints");
    };
    let [exact, wcag] = current.report().cells() else {
        panic!("one case times two heterogeneous constraints must produce two cells");
    };

    let ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(evidence)) =
        exact.result()
    else {
        panic!("the first cell must retain Exact-specific pass evidence");
    };
    assert_eq!(
        evidence.identity(),
        &ExactConstraintIdentityV1::FinalSrgb8IdentityV1,
    );
    assert_eq!(evidence.release(), &ExactIdentityReleaseV1::V1);
    assert_eq!(
        evidence.capability(),
        &ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1,
    );
    assert_eq!(
        evidence.binding().modeled_lcs(),
        exact.modeled_lcs_occurrence(),
    );

    let ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(evidence)) =
        wcag.result()
    else {
        panic!("the second cell must retain WCAG22-specific pass evidence");
    };
    assert_eq!(evidence.release(), &wcag22_profile_v1().profile_id);
    assert_eq!(
        evidence.binding().modeled_lcs(),
        wcag.modeled_lcs_occurrence(),
    );
    assert_ne!(
        core::any::type_name_of_val(evidence.identity()),
        core::any::type_name_of_val(exact.result()),
    );
}

#[test]
fn mixed_families_select_only_a_state_that_passes_every_case_then_recheck_it() {
    let compiled = finite_program([[0x80; 3], [0; 3]]);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session
        .update(observed_backdrops(&[[0xFF; 3], [0x80; 3]]))
        .unwrap()
    else {
        panic!("the later black state must pass WCAG22 over both physical cases");
    };

    assert_eq!(current.selected_state_index(), Some(1));
    let cells = current.report().cells();
    assert_eq!(cells.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| (cell.case_index(), cell.constraint(), cell.is_hard()))
            .collect::<Vec<_>>(),
        vec![
            (0, EXACT_CONSTRAINT, false),
            (0, WCAG_CONSTRAINT, true),
            (1, EXACT_CONSTRAINT, false),
            (1, WCAG_CONSTRAINT, true),
        ],
    );
    for cell in [cells[0].result(), cells[2].result()] {
        assert!(matches!(
            cell,
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(_))
        ));
    }
    for cell in [cells[1].result(), cells[3].result()] {
        assert!(matches!(
            cell,
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(_))
        ));
    }
}

#[test]
fn mixed_family_conflict_is_exhaustive_and_keeps_report_only_non_gating() {
    let compiled = finite_program([[0x80; 3], [0xFF; 3]]);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Failed { cause, previous } = session.update(observed_white()).unwrap() else {
        panic!("neither gray nor white satisfies default text contrast on white");
    };
    assert!(previous.is_none());
    assert_eq!(cause.considered_state_count(), 2);
    let cells = cause.report().cells();
    assert_eq!(cells.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| (
                cell.candidate_state_index(),
                cell.constraint(),
                cell.is_hard()
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, EXACT_CONSTRAINT, false),
            (0, WCAG_CONSTRAINT, true),
            (1, EXACT_CONSTRAINT, false),
            (1, WCAG_CONSTRAINT, true),
        ],
    );
    assert!(cells.iter().all(|cell| cell.result().is_violation()));
    assert!(matches!(
        cells[0].result(),
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::ExactSrgb8(_))
    ));
    assert!(matches!(
        cells[1].result(),
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::Wcag22Srgb8(_))
    ));
}

#[test]
fn concrete_package_bridge_projects_total_ready_and_stale_operations() {
    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    assert_eq!(owner.surface_input_count(), 1);
    assert_eq!(owner.output_slots().collect::<Vec<_>>(), [OUTPUT.value()]);

    let mut session = owner.instantiate(11).unwrap();
    let initial = session.state();
    assert_eq!(initial.kind(), PackageProgramStateKindV1::Waiting);
    assert_eq!(initial.revision(), None);
    assert_eq!(initial.certificates().len(), 0);
    assert_eq!(initial.operations().len(), 0);

    let white = [Srgb8::new([0xFF; 3])];
    let gray = [Srgb8::new([0x80; 3])];
    let scenarios = [
        PackageProgramScenarioV1::new(2, &gray),
        PackageProgramScenarioV1::new(1, &white),
    ];
    let ready = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        })
        .unwrap();
    assert_eq!(ready.kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(ready.revision(), Some(1));
    assert_eq!(ready.cause_certificate_index(), None);
    let certificates = ready.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert_eq!(
        certificates[0].kind(),
        PackageProgramCertificateKindV1::Verified
    );
    assert_eq!(certificates[0].revision(), 1);
    let ready_backing = certificates[0].observation_backing_ptr_for_test();
    assert_eq!(
        ready.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Set {
            output_slot: OUTPUT.value(),
            source: Srgb8::new([0; 3]),
            opacity: 1.0,
            certificate_index: 0,
        }]
    );

    let reordered = [
        PackageProgramScenarioV1::new(1, &white),
        PackageProgramScenarioV1::new(2, &gray),
    ];
    let replay = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &reordered,
        })
        .unwrap();
    let replay_certificate = replay.certificates().next().unwrap();
    assert_eq!(
        replay_certificate.observation_backing_ptr_for_test(),
        ready_backing,
        "scenario permutation at the same revision must be exact idempotence"
    );

    let changed_same_revision = [PackageProgramScenarioV1::new(1, &white)];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &changed_same_revision,
    }) {
        Ok(_) => panic!("changed payload at the same revision must be rejected"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::RevisionConflict
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(session.state().revision(), Some(1));

    let stale = session
        .update(PackageProgramUpdateV1::Unknown {
            revision: 2,
            reason_id: 7,
        })
        .unwrap();
    assert_eq!(stale.kind(), PackageProgramStateKindV1::Stale);
    assert_eq!(stale.revision(), Some(2));
    let certificates = stale.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert_eq!(
        certificates[0].kind(),
        PackageProgramCertificateKindV1::Verified
    );
    assert_eq!(certificates[0].revision(), 1);
    assert_eq!(
        stale.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Hold {
            output_slot: OUTPUT.value(),
            certificate_index: 0,
        }]
    );
}

#[test]
fn concrete_package_bridge_distinguishes_failed_remove_from_failed_hold() {
    let white = [Srgb8::new([0xFF; 3])];
    let black = [Srgb8::new([0; 3])];
    let white_only = [PackageProgramScenarioV1::new(1, &white)];

    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let failed = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &white_only,
        })
        .unwrap();
    assert_eq!(failed.kind(), PackageProgramStateKindV1::Failed);
    assert_eq!(failed.cause_certificate_index(), Some(0));
    let certificates = failed.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert_eq!(
        certificates[0].kind(),
        PackageProgramCertificateKindV1::Conflict
    );
    assert_eq!(certificates[0].revision(), 1);
    assert_eq!(
        failed.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Remove {
            output_slot: OUTPUT.value(),
        }]
    );

    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(12).unwrap();
    session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &white_only,
        })
        .unwrap();
    let both = [
        PackageProgramScenarioV1::new(1, &white),
        PackageProgramScenarioV1::new(2, &black),
    ];
    let failed = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 2,
            scenarios: &both,
        })
        .unwrap();
    assert_eq!(failed.kind(), PackageProgramStateKindV1::Failed);
    assert_eq!(failed.cause_certificate_index(), Some(0));
    let certificates = failed.certificates().collect::<Vec<_>>();
    assert_eq!(
        certificates
            .iter()
            .map(|certificate| (certificate.kind(), certificate.revision()))
            .collect::<Vec<_>>(),
        [
            (PackageProgramCertificateKindV1::Conflict, 2),
            (PackageProgramCertificateKindV1::Verified, 1),
        ]
    );
    assert_eq!(
        failed.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Hold {
            output_slot: OUTPUT.value(),
            certificate_index: 1,
        }]
    );
}

#[test]
fn concrete_package_bridge_rejects_transport_shape_before_core_admission() {
    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let empty_values = [];
    let malformed = [PackageProgramScenarioV1::new(1, &empty_values)];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &malformed,
    }) {
        Ok(_) => panic!("schema-short package input must fail before Core admission"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::InvalidObservation
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Waiting);
    assert_eq!(session.state().revision(), None);

    let white = [Srgb8::new([0xFF; 3])];
    let valid = [PackageProgramScenarioV1::new(1, &white)];
    session
        .update(PackageProgramUpdateV1::Observed {
            revision: 2,
            scenarios: &valid,
        })
        .unwrap();
    let duplicate = [
        PackageProgramScenarioV1::new(7, &white),
        PackageProgramScenarioV1::new(7, &white),
    ];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &duplicate,
    }) {
        Ok(_) => panic!("duplicate scenario IDs must precede revision admission"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::InvalidObservation
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(session.state().revision(), Some(2));
}
