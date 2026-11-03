use super::*;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
    IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    ObservationPayloadInput, ObservationUpdateInput, ObservedScenarioSetInput, Revision,
    ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::session::{SessionState, SessionUpdateError};

// Только сборка декларации; исполнение принадлежит обычному Program/Session.
pub(super) fn compile_assignment<E: ProgramConstraintEvaluatorSetV1>(
    mut program: Program<E>,
    budget: u64,
) -> Result<CompiledProgram<E>, ProgramCompileError> {
    let mut order: Vec<_> = program
        .targets
        .iter()
        .filter_map(|target| match &target.intent {
            TargetIntentV1::Computed(value) => Some((value.preference, target.id)),
            _ => None,
        })
        .collect();
    order.sort_unstable_by_key(|entry| entry.0);
    program.assignment = Some(order.into_iter().map(|entry| entry.1).collect());
    program.computed_budget = Some(budget);
    program.compile()
}

fn context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0)
            .expect("тестовый контекст находится в допустимой области"),
        BackgroundLuminanceRatio::try_new(0.2)
            .expect("тестовый контекст находится в допустимой области"),
        SurroundProfileId::AverageV1,
    )
}

pub(super) fn target(id: u32, anchor: u8, lower: u8, upper: u8, preference: u64) -> Target {
    Target::new(
        TargetId::new(id),
        TargetIntentV1::Computed(
            ComputedTargetV1::new(
                AssignmentDomainV1::Gray { lower, upper },
                Srgb8::new([anchor; 3]),
                1.0,
                preference,
            )
            .expect("тестовая декларация допускает вычисление"),
        ),
    )
}

pub(super) fn program<Evaluation: ProgramConstraintEvaluatorSetV1>(
    target: Target,
    invocation: Evaluation::Invocation,
    evaluator: Evaluation,
) -> Program<Evaluation> {
    let id = target.id;
    Program::new(
        vec![],
        vec![target],
        ObservationGroup::new(ObservationGroupId::new(1), vec![SurfaceInputPortId::new(2)]),
        vec![],
        vec![Paint::Solid {
            id: PaintId::new(3),
            target: id,
        }],
        vec![Surface::Input {
            id: SurfaceId::new(4),
            input: SurfaceInputPortId::new(2),
        }],
        vec![Occurrence::new(
            OccurrenceId::new(5),
            PaintId::new(3),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6),
                OccurrenceId::new(5),
                invocation,
            )],
            vec![],
        ),
        vec![OutputBinding::new(OutputSlotId::new(7), PaintId::new(3))],
        evaluator,
    )
}

pub(super) fn update(revision: u64, backdrop: u8) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: ObservationStreamId::new(8),
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(9),
                bindings: vec![SurfaceInputBinding::new(
                    SurfaceInputPortId::new(2),
                    ColorSignal::from_srgb8(Srgb8::new([backdrop; 3])),
                )],
            }],
        }),
    }
}

pub(super) fn rgb<Evaluation: ProgramConstraintEvaluatorSetV1>(
    session: &Session<ProgramSessionPlan<Evaluation>>,
) -> Srgb8 {
    session
        .state()
        .current_verified()
        .expect("ожидается current output")
        .computed_for_test()
        .arena
        .storage
        .outputs[0]
        .paint
        .source()
}

#[test]
fn computed_new_rgb_and_valid_anchor_use_real_program() {
    let owner = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        300,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([118; 3]));
    let current = session
        .state()
        .current_verified()
        .expect("готовое состояние содержит текущий результат")
        .computed_for_test();
    assert_eq!(
        owner.owner_generation.content_identity,
        ProgramOwnerIdentityV1::Computed(current.identity)
    );
    assert_eq!(&*current.values, &[Srgb8::new([118; 3])]);
    assert_eq!(current.search.completion, CompletionV1::Complete);
    assert_eq!(current.search.open_regions, 0);
    assert_eq!(current.execution_profile, execution_profile_identity(300));
    assert_eq!(current.arena.storage.cells.len(), 1);
    session
        .prepare_update(update(2, 0))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([150; 3]));
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("готовое состояние содержит текущий результат")
            .computed_for_test()
            .search
            .completion,
        CompletionV1::AuthoredExact
    );
}

#[test]
fn ready_to_incomplete_commits_new_revision_and_replay_does_not_search() {
    let owner = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        1,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 0))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([150; 3]));
    drop(
        session
            .prepare_update(update(2, 255))
            .expect("наблюдение допускает подготовку перехода"),
    );
    assert_eq!(rgb(&session), Srgb8::new([150; 3]));
    session
        .prepare_update(update(2, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert!(session.state().current_verified().is_none());
    let SessionState::Incomplete { cause, previous } = session.state() else {
        panic!("новая ревизия должна быть Incomplete")
    };
    assert_eq!(cause.observation.revision(), Revision::new(2));
    assert_eq!(cause.search.completion, CompletionV1::BudgetExhausted);
    assert_eq!(cause.execution_profile, execution_profile_identity(1));
    assert_eq!(
        owner.owner_generation.content_identity,
        ProgramOwnerIdentityV1::Computed(cause.identity)
    );
    assert_eq!(
        previous
            .as_ref()
            .expect("тестовая декларация допускает вычисление")
            .computed_for_test()
            .observation
            .revision(),
        Revision::new(1)
    );
    let observation = cause.observation.clone();
    session
        .prepare_update(update(2, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    let SessionState::Incomplete { cause, .. } = session.state() else {
        panic!()
    };
    assert!(cause.observation.is_same_binding_as(&observation));
    assert!(matches!(
        session.prepare_update(update(2, 0)),
        Err(SessionUpdateError::Observation(_))
    ));
    assert!(matches!(
        session.prepare_update(update(1, 0)),
        Err(SessionUpdateError::Observation(_))
    ));
    session
        .prepare_update(update(3, 0))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([150; 3]));
}

#[test]
fn opposite_consumer_ids_and_computed_identity_are_data_driven() {
    let make = |id, budget| {
        compile_assignment(
            program(
                target(id, 100, 100, 255, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            ),
            budget,
        )
        .expect("допустимая декларация компилируется")
    };
    let owner = make(10, 300);
    let renamed = make(91, 300);
    assert_eq!(
        owner.owner_generation.content_identity,
        renamed.owner_generation.content_identity
    );
    assert_eq!(
        owner.owner_generation.content_identity,
        make(10, 301).owner_generation.content_identity
    );
    assert_ne!(
        execution_profile_identity(300),
        execution_profile_identity(301)
    );
    for owner in [&owner, &renamed] {
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("Session создаётся с допустимым stream");
        session
            .prepare_update(update(1, 0))
            .expect("наблюдение допускает подготовку перехода")
            .commit();
        assert_eq!(rgb(&session), Srgb8::new([117; 3]));
    }
    let mut altered = target(10, 101, 100, 255, 0);
    let TargetIntentV1::Computed(declaration) = &mut altered.intent else {
        panic!()
    };
    declaration.max_delta_bits = 0.25_f64.to_bits();
    let altered = compile_assignment(
        program(altered, Wcag22CriterionV1::Sc143TextDefault, Wcag22Srgb8V1),
        300,
    )
    .expect("допустимая декларация компилируется");
    assert_ne!(
        owner.owner_generation.content_identity,
        altered.owner_generation.content_identity
    );
}

#[test]
fn empty_domain_proof_and_unfinished_search_are_distinct() {
    for budget in [0, 1, 300] {
        let owner = compile_assignment(
            program(
                target(10, 150, 130, 180, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            ),
            budget,
        )
        .expect("допустимая декларация компилируется");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("Session создаётся с допустимым stream");
        session
            .prepare_update(update(1, 255))
            .expect("наблюдение допускает подготовку перехода")
            .commit();
        assert!(session.state().current_verified().is_none());
        assert!(session.state().last_verified().is_none());
        match session.state() {
            SessionState::Incomplete { cause, .. } => {
                assert!(budget < 300);
                assert_eq!(cause.search.completion, CompletionV1::BudgetExhausted);
                assert_eq!(cause.search.visited, budget);
            }
            SessionState::Failed {
                cause: ProgramViolationOutcomeV1::Computed(cause),
                ..
            } => {
                assert_eq!(budget, 300);
                assert_eq!(cause.search.completion, CompletionV1::Complete);
                assert_eq!(cause.search.open_regions, 0);
            }
            _ => panic!("пустая область не даёт output"),
        }
    }
}

#[test]
fn corrupted_final_encoded_source_never_gets_output() {
    let owner = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        300,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 0))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    CORRUPT_FINAL_SOURCE.with(|value| value.set(Some(Srgb8::new([255; 3]))));
    assert!(matches!(
        session.prepare_update(update(2, 255)),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::FinalRecheckViolation { .. }
        ))
    ));
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("готовое состояние содержит текущий результат")
            .computed_for_test()
            .observation
            .revision(),
        Revision::new(1)
    );
    session
        .prepare_update(update(2, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([118; 3]));
}

#[test]
fn admissible_but_unselected_final_source_cannot_claim_preference() {
    let owner = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        300,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    assert!(contrast_passes(
        117,
        255,
        Wcag22CriterionV1::Sc143TextDefault
    ));
    CORRUPT_FINAL_SOURCE.with(|value| value.set(Some(Srgb8::new([117; 3]))));
    assert!(matches!(
        session.prepare_update(update(1, 255)),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::InternalInvariant
        ))
    ));
    assert!(session.state().current_verified().is_none());
}

#[test]
fn computed_physical_recheck_owns_verdict_after_search_pass() {
    let evaluator = crate::constraints::FinalRecheckMutantProgramEvaluatorV1::default();
    let owner = compile_assignment(
        program(
            target(10, 118, 118, 118, 0),
            Srgb8::new([118; 3]),
            evaluator.clone(),
        ),
        2,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    evaluator.arm();
    assert!(matches!(
        session.prepare_update(update(1, 255)),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::FinalRecheckViolation { .. }
        ))
    ));
    assert!(session.state().current_verified().is_none());
}

#[test]
fn cube_search_and_tolerance_never_expand_authored_domain() {
    let declaration = ComputedTargetV1::new(
        AssignmentDomainV1::Cube {
            lower: [60, 80, 90],
            upper: [64, 84, 94],
        },
        Srgb8::new([70, 90, 100]),
        1.0,
        0,
    )
    .expect("тестовая декларация допускает вычисление");
    let owner = compile_assignment(
        program(
            Target::new(
                TargetId::new(10),
                TargetIntentV1::Computed(declaration.clone()),
            ),
            Srgb8::new([63, 82, 91]),
            ExactSrgb8IdentityV1,
        ),
        200,
    )
    .expect("тестовая декларация допускает вычисление");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(rgb(&session), Srgb8::new([63, 82, 91]));
    let mut strict = declaration;
    strict.max_delta_bits = 0;
    let owner = compile_assignment(
        program(
            Target::new(TargetId::new(10), TargetIntentV1::Computed(strict)),
            Srgb8::new([63, 82, 91]),
            ExactSrgb8IdentityV1,
        ),
        200,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert!(matches!(session.state(), SessionState::Failed { .. }));
}

pub(super) fn nested(reverse: bool) -> Program<Wcag22Srgb8V1> {
    let mut targets = vec![target(10, 150, 120, 180, 0), target(20, 160, 0, 255, 1)];
    if reverse {
        targets.reverse();
    }
    Program::new(
        vec![],
        targets,
        ObservationGroup::new(ObservationGroupId::new(1), vec![SurfaceInputPortId::new(2)]),
        vec![],
        vec![
            Paint::Solid {
                id: PaintId::new(3),
                target: TargetId::new(10),
            },
            Paint::Solid {
                id: PaintId::new(13),
                target: TargetId::new(20),
            },
        ],
        vec![
            Surface::Input {
                id: SurfaceId::new(4),
                input: SurfaceInputPortId::new(2),
            },
            Surface::FromOccurrence {
                id: SurfaceId::new(14),
                occurrence: OccurrenceId::new(5),
            },
        ],
        vec![
            Occurrence::new(
                OccurrenceId::new(5),
                PaintId::new(3),
                SurfaceId::new(4),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            ),
            Occurrence::new(
                OccurrenceId::new(15),
                PaintId::new(13),
                SurfaceId::new(14),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6),
                OccurrenceId::new(15),
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OutputSlotId::new(7), PaintId::new(3)),
            OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)),
        ],
        Wcag22Srgb8V1,
    )
}

fn contrast_passes(foreground: u8, background: u8, criterion: Wcag22CriterionV1) -> bool {
    matches!(
        crate::wcag22::evaluate_wcag22_srgb8([foreground; 3], [background; 3], criterion)
            .expect("конечная sRGB8-пара допускает оценку"),
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
            ..
        }
    )
}

// Независимый полный перебор серой области: oracle не пользуется search,
// pruning, подготовленными scores, Paint slots или порядком кандидатов Core.
fn oracle_lab_gray(value: u8) -> [f64; 3] {
    let channel = f64::from(value) / 255.0;
    let linear = if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    };
    let l = (0.4122214708 * linear + 0.5363325363 * linear + 0.0514459929 * linear).cbrt();
    let m = (0.2119034982 * linear + 0.6806995451 * linear + 0.1073969566 * linear).cbrt();
    let s = (0.0883024619 * linear + 0.2817188376 * linear + 0.6299787005 * linear).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

#[test]
fn coupled_pair_uses_whole_composition_and_joint_lexicographic_preference() {
    let mut expected: Option<([f64; 2], [u8; 2])> = None;
    for lower in 120..=180_u8 {
        for upper in 0..=255_u8 {
            if !contrast_passes(upper, lower, Wcag22CriterionV1::Sc143TextLargeScale) {
                continue;
            }
            let scores = [
                squared_distance(oracle_lab_gray(lower), oracle_lab_gray(150)),
                squared_distance(oracle_lab_gray(upper), oracle_lab_gray(160)),
            ];
            let tuple = [lower, upper];
            if expected.as_ref().is_none_or(|(best_scores, best_tuple)| {
                compare_scores(&scores, best_scores).then_with(|| tuple.cmp(best_tuple))
                    == Ordering::Less
            }) {
                expected = Some((scores, tuple));
            }
        }
    }
    let (_, expected) = expected.expect("полный oracle нашёл допустимый цвет");
    assert_eq!(expected[0], 150);
    let independent_upper = (0..=255_u8)
        .filter(|&upper| contrast_passes(upper, 255, Wcag22CriterionV1::Sc143TextLargeScale))
        .min_by(|&a, &b| {
            squared_distance(oracle_lab_gray(a), oracle_lab_gray(160))
                .total_cmp(&squared_distance(oracle_lab_gray(b), oracle_lab_gray(160)))
        })
        .expect("тестовая декларация допускает вычисление");
    assert!(!contrast_passes(
        independent_upper,
        150,
        Wcag22CriterionV1::Sc143TextLargeScale
    ));
    let first =
        compile_assignment(nested(false), 20_000).expect("допустимая декларация компилируется");
    let second =
        compile_assignment(nested(true), 20_000).expect("допустимая декларация компилируется");
    assert_eq!(
        first.owner_generation.content_identity,
        second.owner_generation.content_identity
    );
    for owner in [&first, &second] {
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("Session создаётся с допустимым stream");
        session
            .prepare_update(update(1, 255))
            .expect("наблюдение допускает подготовку перехода")
            .commit();
        let current = session
            .state()
            .current_verified()
            .expect("готовое состояние содержит текущий результат")
            .computed_for_test();
        let outputs: Vec<_> = current
            .arena
            .storage
            .outputs
            .iter()
            .map(|output| output.paint.source().bytes()[0])
            .collect();
        assert_eq!(outputs, expected);
        assert_eq!(current.search.open_regions, 0);
        assert_eq!(current.arena.storage.cells.len(), 1);
    }
}

#[test]
fn missing_assignment_order_and_legacy_identity_reject_computed_capability() {
    let make = || {
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        )
    };
    assert!(matches!(
        make().compile(),
        Err(ProgramCompileError::InvalidComputedAssignment(
            ComputedAdmissionErrorV1::InvalidPreferenceOrder
        ))
    ));
    assert!(matches!(
        identity::compile_program_content_identity_v9(&make()),
        Err(ProgramCompileError::InternalInvariant)
    ));
    assert!(
        ComputedTargetV1::new(
            AssignmentDomainV1::Gray {
                lower: 20,
                upper: 10
            },
            Srgb8::new([15; 3]),
            1.0,
            0
        )
        .is_err()
    );
    assert!(
        ComputedTargetV1::new(
            AssignmentDomainV1::Gray {
                lower: 0,
                upper: 255
            },
            Srgb8::new([15; 3]),
            f64::NAN,
            0
        )
        .is_err()
    );
}

#[test]
fn compiled_preferences_keep_target_payloads_under_declaration_permutations() {
    let owner =
        compile_assignment(nested(false), 20_000).expect("допустимая декларация компилируется");
    for reversed in [false, true] {
        let mut targets = vec![target(10, 150, 120, 180, 1), target(20, 160, 0, 255, 0)];
        if reversed {
            targets.reverse();
        }
        let selection = compile_selection(&owner.owner_generation.graph, &targets, None, 20_000)
            .expect("плотные уникальные ранги допускаются в любом порядке деклараций");
        for (actual, (id, anchor, count)) in selection
            .targets
            .iter()
            .zip([(20, 160, 256), (10, 150, 61)])
        {
            assert_eq!(actual.declaration.anchor, Srgb8::new([anchor; 3]));
            assert_eq!(actual.declaration.domain.cardinality(), Some(count));
            assert_eq!(actual.anchor_lab, lab(Srgb8::new([anchor; 3])));
            assert_eq!(
                Some(actual.binding),
                owner
                    .owner_generation
                    .graph
                    .bind_paint_input(target_paint_input_id(TargetId::new(id)))
            );
        }
    }
}

#[test]
fn compiled_preferences_reject_duplicate_missing_and_out_of_range_ranks() {
    let owner =
        compile_assignment(nested(false), 20_000).expect("допустимая декларация компилируется");
    for ranks in [
        [0, 0],
        [1, 1],
        [1, 2],
        [0, 2],
        [0, u64::MAX],
        [1_u64 << 32, 1],
        [0, (1_u64 << 32) + 1],
    ] {
        for reversed in [false, true] {
            let mut targets = vec![
                target(10, 150, 120, 180, ranks[0]),
                target(20, 160, 0, 255, ranks[1]),
            ];
            if reversed {
                targets.reverse();
            }
            assert!(matches!(
                compile_selection(&owner.owner_generation.graph, &targets, None, 20_000),
                Err(ProgramCompileError::InvalidComputedAssignment(
                    ComputedAdmissionErrorV1::InvalidPreferenceOrder
                ))
            ));
        }
    }
}

fn four_preference_targets() -> Program<Wcag22Srgb8V1> {
    let mut declaration = nested(false);
    let mut previous = OccurrenceId::new(15);
    for (id, anchor, rank) in [(30, 170, 2), (40, 180, 3)] {
        declaration.targets.push(target(id, anchor, 0, 0, rank));
        declaration.paints.push(Paint::Solid {
            id: PaintId::new(id + 3),
            target: TargetId::new(id),
        });
        declaration.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(id + 4),
            occurrence: previous,
        });
        previous = OccurrenceId::new(id + 5);
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(id + 5),
            PaintId::new(id + 3),
            SurfaceId::new(id + 4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        ));
    }
    declaration.constraints = ConstraintSet::new(
        vec![ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(6),
            previous,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![],
    );
    declaration
}

#[test]
fn compiled_preferences_preserve_payloads_across_long_cycles() {
    let owner = compile_assignment(four_preference_targets(), 20_000)
        .expect("допустимая декларация компилируется");
    let expected = [
        target(10, 150, 100, 101, 0),
        target(20, 160, 110, 112, 1),
        target(30, 170, 120, 123, 2),
        target(40, 180, 130, 134, 3),
    ];
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let permutation = [a, b, c, d];
                    if (0..4).any(|i| permutation[..i].contains(&permutation[i])) {
                        continue;
                    }
                    let targets = permutation.map(|i| expected[i].clone());
                    let selection =
                        compile_selection(&owner.owner_generation.graph, &targets, None, 20_000)
                            .expect("каждая перестановка плотных рангов допустима");
                    for (index, actual) in selection.targets.iter().enumerate() {
                        let TargetIntentV1::Computed(declaration) = &expected[index].intent else {
                            unreachable!();
                        };
                        assert_eq!(
                            actual.declaration.identity_bytes(),
                            declaration.identity_bytes()
                        );
                        assert_eq!(actual.anchor_lab, lab(declaration.anchor));
                        assert_eq!(
                            actual.declaration.domain.cardinality(),
                            Some([2, 3, 4, 5][index])
                        );
                        assert_eq!(
                            Some(actual.binding),
                            owner
                                .owner_generation
                                .graph
                                .bind_paint_input(target_paint_input_id(expected[index].id))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn compiled_preferences_keep_declaration_errors_before_order_validation() {
    let owner =
        compile_assignment(nested(false), 20_000).expect("допустимая декларация компилируется");
    for (domain, max_delta_bits, expected) in [
        (
            AssignmentDomainV1::Gray { lower: 2, upper: 1 },
            1.0_f64.to_bits(),
            ComputedAdmissionErrorV1::InvalidDomain,
        ),
        (
            AssignmentDomainV1::Gray { lower: 0, upper: 1 },
            f64::NAN.to_bits(),
            ComputedAdmissionErrorV1::InvalidTolerance,
        ),
    ] {
        let invalid = Target::new(
            TargetId::new(20),
            TargetIntentV1::Computed(ComputedTargetV1 {
                domain,
                anchor: Srgb8::new([160; 3]),
                max_delta_bits,
                preference: 1,
            }),
        );
        let mut targets = [target(10, 150, 120, 180, u64::MAX), invalid];
        for _ in 0..2 {
            assert!(matches!(
                compile_selection(&owner.owner_generation.graph, &targets, None, 20_000),
                Err(ProgramCompileError::InvalidComputedAssignment(error)) if error == expected
            ));
            targets.reverse();
        }
    }
}

#[test]
fn compiled_preferences_validate_order_without_product_cardinality_bound() {
    let owner = compile_assignment(four_preference_targets(), 20_000)
        .expect("допустимая декларация компилируется");
    for (ranks, expected) in [
        ([0, 1, 2], None),
        (
            [0, 1, 1],
            Some(ComputedAdmissionErrorV1::InvalidPreferenceOrder),
        ),
        (
            [0, 1, u64::MAX],
            Some(ComputedAdmissionErrorV1::InvalidPreferenceOrder),
        ),
    ] {
        let mut targets = ranks.map(|rank| {
            Target::new(
                TargetId::new(10),
                TargetIntentV1::Computed(
                    ComputedTargetV1::new(
                        AssignmentDomainV1::Cube {
                            lower: [0; 3],
                            upper: [255; 3],
                        },
                        Srgb8::new([150; 3]),
                        1.0,
                        rank,
                    )
                    .expect("каждый domain допустим отдельно"),
                ),
            )
        });
        for (index, target) in targets.iter_mut().enumerate() {
            target.id = TargetId::new((index as u32 + 1) * 10);
        }
        for _ in 0..2 {
            let result = compile_selection(&owner.owner_generation.graph, &targets, None, 20_000);
            if let Some(expected) = expected {
                assert!(matches!(
                    result,
                    Err(ProgramCompileError::InvalidComputedAssignment(error)) if error == expected
                ));
            } else {
                assert!(
                    result.is_ok(),
                    "валидный порядок не зависит от мощности RGB8-произведения"
                );
            }
            targets.reverse();
        }
    }
}

#[test]
fn computed_revisions_recheck_all_support_and_never_reuse_fixed_evidence() {
    let evaluator = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let owner = compile_assignment(
        program(
            target(10, 118, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            evaluator.clone(),
        ),
        300,
    )
    .expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    let both = |revision| {
        let mut value = update(revision, 0);
        let ObservationPayloadInput::Scenarios(scenarios) = &mut value.payload else {
            panic!()
        };
        scenarios.scenarios.push(ScenarioInput {
            id: ScenarioId::new(10),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new([255; 3])),
            )],
        });
        value
    };
    session
        .prepare_update(both(1))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(evaluator.calls().len(), 4);
    session
        .prepare_update(both(2))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(evaluator.calls().len(), 8);
    session
        .prepare_update(both(2))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    assert_eq!(evaluator.calls().len(), 8);
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("готовое состояние содержит текущий результат")
            .computed_for_test()
            .arena
            .storage
            .cells
            .len(),
        2
    );
    let failure = fail_program_preflight_reservation_for_test(0);
    let mut larger_support = both(3);
    let ObservationPayloadInput::Scenarios(scenarios) = &mut larger_support.payload else {
        panic!()
    };
    scenarios.scenarios.push(ScenarioInput {
        id: ScenarioId::new(11),
        bindings: vec![SurfaceInputBinding::new(
            SurfaceInputPortId::new(2),
            ColorSignal::from_srgb8(Srgb8::new([128; 3])),
        )],
    });
    assert!(matches!(
        session.prepare_update(larger_support),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::ResourceExhausted
        ))
    ));
    drop(failure);
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("готовое состояние содержит текущий результат")
            .computed_for_test()
            .observation
            .revision(),
        Revision::new(2)
    );
    drop(owner);
    assert!(matches!(
        session.prepare_update(both(3)),
        Err(SessionUpdateError::OwnerExpired)
    ));
}

#[test]
fn fixed_sources_are_not_part_of_computed_freedom() {
    let mut declaration = nested(false);
    declaration.sources.push(Source::new(
        SourceId::new(30),
        ColorSignal::from_srgb8(Srgb8::new([150; 3])),
    ));
    declaration.targets[0] = Target::fixed(TargetId::new(10), SourceId::new(30));
    let TargetIntentV1::Computed(upper) = &mut declaration.targets[1].intent else {
        panic!()
    };
    upper.preference = 0;
    let owner = compile_assignment(declaration, 300).expect("допустимая декларация компилируется");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("Session создаётся с допустимым stream");
    session
        .prepare_update(update(1, 255))
        .expect("наблюдение допускает подготовку перехода")
        .commit();
    let outputs = &session
        .state()
        .current_verified()
        .expect("готовое состояние содержит текущий результат")
        .computed_for_test()
        .arena
        .storage
        .outputs;
    assert_eq!(outputs[0].paint.source(), Srgb8::new([150; 3]));
    assert!(contrast_passes(
        outputs[1].paint.source().bytes()[0],
        150,
        Wcag22CriterionV1::Sc143TextLargeScale
    ));
}


fn cube_product(
    domains: [AssignmentDomainV1; 3],
    anchors: [u8; 3],
    max_delta: f64,
    exact: Option<[u8; 3]>,
) -> Program<CoreProgramEvaluatorsV1> {
    let targets: Vec<_> = domains
        .into_iter()
        .enumerate()
        .map(|(i, domain)| {
            Target::new(
                TargetId::new(10 * (i as u32 + 1)),
                TargetIntentV1::Computed(
                    ComputedTargetV1::new(domain, Srgb8::new([anchors[i]; 3]), max_delta, i as u64)
                        .expect("каждая отдельная RGB8-область допустима"),
                ),
            )
        })
        .collect();
    let invocation = |i: usize| match exact {
        Some(values) => CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([values[i]; 3])),
        None => CoreProgramConstraintInvocationV1::Wcag22Srgb8(Wcag22CriterionV1::Sc143TextDefault),
    };
    let mut declaration = program(targets[0].clone(), invocation(0), CoreProgramEvaluatorsV1);
    declaration.targets = targets;
    declaration.paints = (0..3)
        .map(|i| Paint::Solid {
            id: PaintId::new(3 + 10 * i),
            target: TargetId::new(10 * (i + 1)),
        })
        .collect();
    declaration.occurrences = (0..3)
        .map(|i| {
            Occurrence::new(
                OccurrenceId::new(5 + 10 * i),
                PaintId::new(3 + 10 * i),
                SurfaceId::new(4),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(),
            )
        })
        .collect();
    declaration.constraints = ConstraintSet::new(
        (0..3)
            .map(|i| {
                ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(6 + 10 * i),
                    OccurrenceId::new(5 + 10 * i),
                    invocation(i as usize),
                )
            })
            .collect(),
        vec![],
    );
    declaration.outputs = (0..3)
        .map(|i| OutputBinding::new(OutputSlotId::new(7 + 10 * i), PaintId::new(3 + 10 * i)))
        .collect();
    declaration
}

const FULL_RGB: AssignmentDomainV1 = AssignmentDomainV1::Cube {
    lower: [0; 3],
    upper: [255; 3],
};

#[test]
fn full_rgb_products_admit_authored_exact_and_singleton_control() {
    let anchors = [24, 36, 48];
    for max_delta in [0.0, 1.0] {
        for domains in [
            [FULL_RGB; 3],
            anchors.map(|gray| AssignmentDomainV1::Cube {
                lower: [gray; 3],
                upper: [gray; 3],
            }),
        ] {
            let owner = compile_assignment(cube_product(domains, anchors, max_delta, None), 1)
                .expect("размер произведения не запрещает точный допустимый authored tuple");
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("stream допустим");
            session
                .prepare_update(update(1, 255))
                .expect("наблюдение допустимо")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("authored tuple допустим")
                .computed_for_test();
            assert_eq!(&*current.values, &anchors.map(|gray| Srgb8::new([gray; 3])));
            assert_eq!(current.search.completion, CompletionV1::AuthoredExact);
            assert_eq!(current.search.visited, 1);
            assert_eq!(current.search.regions, 0);
        }
    }
}

#[test]
fn full_rgb_products_search_and_prove_empty_with_singleton_controls() {
    let anchors = [24, 36, 48];
    let expected = [25, 37, 49];
    for max_delta in [0.0, 1.0] {
        for domains in [
            [FULL_RGB; 3],
            expected.map(|gray| AssignmentDomainV1::Cube {
                lower: [gray; 3],
                upper: [gray; 3],
            }),
        ] {
            let owner = compile_assignment(
                cube_product(domains, anchors, max_delta, Some(expected)),
                EXECUTION_NODE_LIMIT_V1,
            )
            .expect("большое произведение допускается до действительного поиска");
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("stream допустим");
            session
                .prepare_update(update(1, 255))
                .expect("наблюдение допустимо")
                .commit();
            if max_delta > 0.0 {
                let current = session
                    .state()
                    .current_verified()
                    .expect("exact-visible оставляет один допустимый tuple")
                    .computed_for_test();
                assert_eq!(&*current.values, &expected.map(|gray| Srgb8::new([gray; 3])));
                assert_eq!(current.search.completion, CompletionV1::Complete);
                assert!(current.search.regions > 0);
                assert!(current.search.visited <= EXECUTION_NODE_LIMIT_V1);
            } else {
                let SessionState::Failed {
                    cause: ProgramViolationOutcomeV1::Computed(cause),
                    ..
                } = session.state()
                else {
                    panic!("нулевой delta несовместим с exact-visible; нужен полный Unsatisfied");
                };
                assert_eq!(cause.search.completion, CompletionV1::Complete);
            }
        }
    }
}

#[test]
fn full_rgb_product_budget_limits_search_instead_of_admission() {
    for budget in [0, 1, 2] {
        let owner = compile_assignment(
            cube_product([FULL_RGB; 3], [24, 36, 48], 1.0, Some([25, 37, 49])),
            budget,
        )
        .expect("бюджет ограничивает посещения, а не RGB8-произведение");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("stream допустим");
        session
            .prepare_update(update(1, 255))
            .expect("наблюдение допустимо")
            .commit();
        let SessionState::Incomplete { cause, .. } = session.state() else {
            panic!("бюджета недостаточно для полного доказательства");
        };
        assert_eq!(cause.search.completion, CompletionV1::BudgetExhausted);
        assert_eq!(cause.search.visited, budget);
        assert!(session.state().current_verified().is_none());
    }
}
