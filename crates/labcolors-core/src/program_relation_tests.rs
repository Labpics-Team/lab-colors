//! Контракт типизированных направленных отношений Program.

use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::program_session::compiled_occurrence_coordinate_pair_matches_for_test;
use crate::{Srgb8, program};

#[test]
fn compiled_occurrence_context_requires_both_exact_coordinates() {
    assert!(compiled_occurrence_coordinate_pair_matches_for_test(
        1_u8, 2_u8, 1_u8, 2_u8,
    ));
    for actual in [(9, 2), (1, 9), (9, 9)] {
        assert!(
            !compiled_occurrence_coordinate_pair_matches_for_test(1_u8, 2_u8, actual.0, actual.1,),
            "a single mismatched coordinate must invalidate {actual:?}",
        );
    }
}

#[test]
fn directed_relation_rejects_invalid_topology_before_draft() {
    let reference = program::OccurrenceIdV1::new(1);
    let candidate = program::OccurrenceIdV1::new(2);

    assert!(matches!(
        program::DirectedRelationV1::try_new(reference, Vec::new()),
        Err(program::DirectedRelationErrorV1::EmptyCandidates),
    ));
    assert!(matches!(
        program::DirectedRelationV1::try_new(reference, vec![candidate, candidate]),
        Err(program::DirectedRelationErrorV1::DuplicateCandidate { candidate: duplicate })
            if duplicate == candidate,
    ));
    assert!(matches!(
        program::DirectedRelationV1::try_new(reference, vec![candidate, reference]),
        Err(program::DirectedRelationErrorV1::ReferenceInCandidates {
            reference: repeated,
        }) if repeated == reference,
    ));
}

#[test]
fn exact_relation_draft_methods_encode_the_physical_level_in_their_id_types() {
    let intrinsic = program::DirectedRelationV1::try_new(
        program::TargetIdV1::new(1),
        vec![program::TargetIdV1::new(2)],
    )
    .unwrap();
    let visible = program::DirectedRelationV1::try_new(
        program::OccurrenceIdV1::new(3),
        vec![program::OccurrenceIdV1::new(4)],
    )
    .unwrap();
    let mut draft = program::DraftV1::new();

    let _: &mut program::DraftV1 =
        draft.push_exact_intrinsic_relation_hard(program::ConstraintIdV1::new(5), intrinsic);
    let _: &mut program::DraftV1 =
        draft.push_exact_visible_relation_hard(program::ConstraintIdV1::new(6), visible);
}

fn finite_intrinsic_unary_draft(include_visible_assessment: bool) -> program::DraftV1 {
    let target = program::TargetIdV1::new(21);
    let candidate = program::TargetCandidateIdV1::new(22);
    let paint = program::PaintIdV1::new(23);
    let port = program::SurfaceInputPortIdV1::new(24);
    let surface = program::SurfaceIdV1::new(25);
    let occurrence = program::OccurrenceIdV1::new(26);
    let expected = Srgb8::new([0xFF; 3]);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let domain = program::FinitePaintDomainV1::try_new(vec![program::TargetCandidateV1::new(
        candidate,
        program::PaintValueV1::try_new(expected, 0.5).unwrap(),
    )])
    .unwrap();
    let mut draft = program::DraftV1::new();
    draft.push_finite_target(target, domain);
    draft
        .set_joint_selection(vec![program::JointStateV1::new(vec![
            program::JointChoiceV1::new(target, candidate),
        ])])
        .unwrap();
    draft.push_solid_paint(paint, target);
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    draft.push_exact_intrinsic_unary_hard(program::ConstraintIdV1::new(27), target, expected);
    if include_visible_assessment {
        draft.push_exact_visible_unary_hard(
            program::ConstraintIdV1::new(28),
            occurrence,
            Srgb8::new([0x80; 3]),
        );
    }
    draft.push_output(program::OutputSlotIdV1::new(29), paint);
    draft
}

#[test]
fn intrinsic_unary_filters_finite_target_without_pretending_to_assess_its_output() {
    let error = match finite_intrinsic_unary_draft(false).compile() {
        Err(error) => error,
        Ok(_) => panic!("intrinsic coverage must not masquerade as visible assessment"),
    };
    assert_eq!(error.kind(), program::CompileErrorKindV1::UnassessedOutput);

    let owner = finite_intrinsic_unary_draft(true).compile().unwrap();
    let mut session = owner.instantiate(30).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(31, &black)];
    let evidence = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(program::CertificateV1::Verified(verified)) = evidence.certificates().next() else {
        panic!("the only finite candidate satisfies both typed constraints");
    };
    assert_eq!(verified.selected_state_index(), Some(0));
    let cell = verified
        .cells()
        .find(|cell| {
            matches!(
                cell.subject(),
                program::ConstraintSubjectV1::IntrinsicUnary { .. }
            )
        })
        .expect("intrinsic unary cell must be retained");
    let program::AssessmentV1::IntrinsicUnary(evidence) = cell.assessment() else {
        panic!("intrinsic unary must retain its own evidence family");
    };
    assert_eq!(evidence.verdict(), program::VerdictV1::Pass);
    assert_eq!(
        evidence.proof(),
        program::IntrinsicUnaryProofV1::ExactSrgb8Pass
    );
    let binding = evidence.binding();
    assert_eq!(binding.target(), program::TargetIdV1::new(21));
    assert_eq!(binding.value().source(), Srgb8::new([0xFF; 3]));
    assert_eq!(binding.value().opacity().to_bits(), 0.5_f64.to_bits());
    let program::IntrinsicUnaryMeasurementV1::ExactSrgb8(measurement) = evidence.measurement()
    else {
        panic!("the exact constraint must retain exact evidence");
    };
    assert_eq!(measurement.expected(), Srgb8::new([0xFF; 3]));
    assert_eq!(measurement.actual(), Srgb8::new([0xFF; 3]));
}

#[test]
fn intrinsic_relation_compares_source_but_retains_each_endpoints_full_alpha() {
    let source = program::SourceIdV1::new(32);
    let reference_target = program::TargetIdV1::new(33);
    let candidate_target = program::TargetIdV1::new(34);
    let candidate = program::TargetCandidateIdV1::new(35);
    let reference_paint = program::PaintIdV1::new(36);
    let candidate_paint = program::PaintIdV1::new(37);
    let port = program::SurfaceInputPortIdV1::new(38);
    let surface = program::SurfaceIdV1::new(39);
    let occurrence = program::OccurrenceIdV1::new(40);
    let white = Srgb8::new([0xFF; 3]);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let mut draft = program::DraftV1::new();
    draft.push_source(source, white);
    draft.push_fixed_target(reference_target, source);
    draft.push_finite_target(
        candidate_target,
        program::FinitePaintDomainV1::try_new(vec![program::TargetCandidateV1::new(
            candidate,
            program::PaintValueV1::try_new(white, 0.5).unwrap(),
        )])
        .unwrap(),
    );
    draft
        .set_joint_selection(vec![program::JointStateV1::new(vec![
            program::JointChoiceV1::new(candidate_target, candidate),
        ])])
        .unwrap();
    draft.push_solid_paint(reference_paint, reference_target);
    draft.push_solid_paint(candidate_paint, candidate_target);
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    draft.push_source_over_occurrence(occurrence, candidate_paint, surface, context);
    draft.push_exact_intrinsic_relation_hard(
        program::ConstraintIdV1::new(41),
        program::DirectedRelationV1::try_new(reference_target, vec![candidate_target]).unwrap(),
    );
    draft.push_exact_visible_unary_hard(
        program::ConstraintIdV1::new(42),
        occurrence,
        Srgb8::new([0x80; 3]),
    );
    draft.push_output(program::OutputSlotIdV1::new(43), candidate_paint);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(44).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(45, &black)];
    let evidence = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(program::CertificateV1::Verified(verified)) = evidence.certificates().next() else {
        panic!("equal sources must pass independently of endpoint alpha");
    };
    let relation = verified
        .cells()
        .find_map(|cell| match cell.assessment() {
            program::AssessmentV1::Relation(evidence) => Some(evidence),
            _ => None,
        })
        .expect("relation evidence must be retained");
    let mut members = relation.members();
    let Some(program::RelationMemberV1::Intrinsic(member)) = members.next() else {
        panic!("intrinsic relation must retain intrinsic endpoint bindings");
    };
    assert!(members.next().is_none());
    assert_eq!(member.reference().target(), reference_target);
    assert_eq!(member.reference().value().source(), white);
    assert_eq!(
        member.reference().value().opacity().to_bits(),
        1.0_f64.to_bits()
    );
    assert_eq!(member.candidate().target(), candidate_target);
    assert_eq!(member.candidate().value().source(), white);
    assert_eq!(
        member.candidate().value().opacity().to_bits(),
        0.5_f64.to_bits()
    );
    let projected = program::RelationMemberV1::Intrinsic(member);
    let program::RelationMeasurementV1::ExactSrgb8(measurement) = projected.measurement() else {
        panic!("the exact relation must retain exact pair measurement");
    };
    assert_eq!(measurement.reference(), white);
    assert_eq!(measurement.candidate(), white);
    assert_eq!(projected.verdict(), program::VerdictV1::Pass);
    assert_eq!(
        projected.proof(),
        program::RelationMemberProofV1::ExactSrgb8Pass
    );
}

fn fixed_relation_draft() -> (
    program::DraftV1,
    [program::TargetIdV1; 2],
    [program::OccurrenceIdV1; 2],
) {
    let sources = [program::SourceIdV1::new(40), program::SourceIdV1::new(41)];
    let targets = [program::TargetIdV1::new(42), program::TargetIdV1::new(43)];
    let paints = [program::PaintIdV1::new(44), program::PaintIdV1::new(45)];
    let occurrences = [
        program::OccurrenceIdV1::new(46),
        program::OccurrenceIdV1::new(47),
    ];
    let port = program::SurfaceInputPortIdV1::new(48);
    let surface = program::SurfaceIdV1::new(49);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let mut draft = program::DraftV1::new();
    for (source, value) in sources.into_iter().zip([[0x10; 3], [0x20; 3]]) {
        draft.push_source(source, Srgb8::new(value));
    }
    for index in 0..2 {
        draft.push_fixed_target(targets[index], sources[index]);
        draft.push_solid_paint(paints[index], targets[index]);
    }
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    for index in 0..2 {
        draft.push_source_over_occurrence(occurrences[index], paints[index], surface, context);
    }
    draft.push_exact_visible_unary_hard(
        program::ConstraintIdV1::new(50),
        occurrences[0],
        Srgb8::new([0x10; 3]),
    );
    draft.push_output(program::OutputSlotIdV1::new(51), paints[0]);
    (draft, targets, occurrences)
}

fn compile_error_kind(draft: program::DraftV1) -> program::CompileErrorKindV1 {
    match draft.compile() {
        Err(error) => error.kind(),
        Ok(_) => panic!("fixture must be rejected by the typed compiler invariant"),
    }
}

#[test]
fn relation_endpoints_and_intrinsic_unary_target_fail_with_typed_errors() {
    let (mut draft, _targets, _) = fixed_relation_draft();
    draft.push_exact_intrinsic_unary_hard(
        program::ConstraintIdV1::new(52),
        program::TargetIdV1::new(999),
        Srgb8::new([0; 3]),
    );
    assert_eq!(
        compile_error_kind(draft),
        program::CompileErrorKindV1::MissingIntrinsicUnaryTarget
    );

    let (mut draft, targets, _) = fixed_relation_draft();
    draft.push_exact_intrinsic_relation_hard(
        program::ConstraintIdV1::new(52),
        program::DirectedRelationV1::try_new(program::TargetIdV1::new(999), vec![targets[0]])
            .unwrap(),
    );
    assert_eq!(
        compile_error_kind(draft),
        program::CompileErrorKindV1::MissingIntrinsicRelationReference
    );

    let (mut draft, targets, _) = fixed_relation_draft();
    draft.push_exact_intrinsic_relation_hard(
        program::ConstraintIdV1::new(52),
        program::DirectedRelationV1::try_new(targets[0], vec![program::TargetIdV1::new(999)])
            .unwrap(),
    );
    assert_eq!(
        compile_error_kind(draft),
        program::CompileErrorKindV1::MissingIntrinsicRelationCandidate
    );

    let (mut draft, _, occurrences) = fixed_relation_draft();
    draft.push_exact_visible_relation_hard(
        program::ConstraintIdV1::new(52),
        program::DirectedRelationV1::try_new(
            program::OccurrenceIdV1::new(999),
            vec![occurrences[0]],
        )
        .unwrap(),
    );
    assert_eq!(
        compile_error_kind(draft),
        program::CompileErrorKindV1::MissingVisibleRelationReference
    );

    let (mut draft, _, occurrences) = fixed_relation_draft();
    draft.push_exact_visible_relation_hard(
        program::ConstraintIdV1::new(52),
        program::DirectedRelationV1::try_new(
            occurrences[0],
            vec![program::OccurrenceIdV1::new(999)],
        )
        .unwrap(),
    );
    assert_eq!(
        compile_error_kind(draft),
        program::CompileErrorKindV1::MissingVisibleRelationCandidate
    );
}

#[test]
fn both_relation_levels_select_the_matching_finite_candidate_before_fresh_capture() {
    for visible in [false, true] {
        let reference_source = program::SourceIdV1::new(80);
        let reference_target = program::TargetIdV1::new(81);
        let finite_target = program::TargetIdV1::new(82);
        let mismatch = program::TargetCandidateIdV1::new(83);
        let matching = program::TargetCandidateIdV1::new(84);
        let reference_paint = program::PaintIdV1::new(85);
        let finite_paint = program::PaintIdV1::new(86);
        let port = program::SurfaceInputPortIdV1::new(87);
        let surface = program::SurfaceIdV1::new(88);
        let reference_occurrence = program::OccurrenceIdV1::new(89);
        let finite_occurrence = program::OccurrenceIdV1::new(90);
        let context =
            program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
        let expected = Srgb8::new([0x20; 3]);
        let mut draft = program::DraftV1::new();
        draft.push_source(reference_source, expected);
        draft.push_fixed_target(reference_target, reference_source);
        draft.push_finite_target(
            finite_target,
            program::FinitePaintDomainV1::try_new(vec![
                program::TargetCandidateV1::new(
                    mismatch,
                    program::PaintValueV1::opaque(Srgb8::new([0x10; 3])),
                ),
                program::TargetCandidateV1::new(matching, program::PaintValueV1::opaque(expected)),
            ])
            .unwrap(),
        );
        draft
            .set_joint_selection(vec![
                program::JointStateV1::new(vec![program::JointChoiceV1::new(
                    finite_target,
                    mismatch,
                )]),
                program::JointStateV1::new(vec![program::JointChoiceV1::new(
                    finite_target,
                    matching,
                )]),
            ])
            .unwrap();
        draft.push_solid_paint(reference_paint, reference_target);
        draft.push_solid_paint(finite_paint, finite_target);
        draft.push_surface_input_port(port);
        draft.push_input_surface(surface, port);
        draft.push_source_over_occurrence(reference_occurrence, reference_paint, surface, context);
        draft.push_source_over_occurrence(finite_occurrence, finite_paint, surface, context);
        let relation_id = program::ConstraintIdV1::new(91);
        if visible {
            draft.push_exact_visible_relation_hard(
                relation_id,
                program::DirectedRelationV1::try_new(reference_occurrence, vec![finite_occurrence])
                    .unwrap(),
            );
        } else {
            draft.push_exact_intrinsic_relation_hard(
                relation_id,
                program::DirectedRelationV1::try_new(reference_target, vec![finite_target])
                    .unwrap(),
            );
        }
        draft.push_exact_visible_unary_report_only(
            program::ConstraintIdV1::new(92),
            finite_occurrence,
            expected,
        );
        draft.push_output(program::OutputSlotIdV1::new(93), finite_paint);

        let owner = draft.compile().unwrap();
        let mut session = owner.instantiate(94).unwrap();
        let backdrop = [Srgb8::new([0; 3])];
        let scenarios = [program::ScenarioV1::new(95, &backdrop)];
        let evidence = owner
            .commit(
                &mut session,
                program::UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap();
        let Some(program::CertificateV1::Verified(verified)) = evidence.certificates().next()
        else {
            panic!("the matching finite candidate must be selected");
        };
        assert_eq!(verified.selected_state_index(), Some(1));
        assert_eq!(verified.outputs().next().unwrap().source(), expected);
        let relation = verified
            .cells()
            .find_map(|cell| match cell.assessment() {
                program::AssessmentV1::Relation(relation) => Some(relation),
                _ => None,
            })
            .expect("fresh selected-state recheck must retain relation evidence");
        assert_eq!(relation.verdict(), program::VerdictV1::Pass);
        assert_eq!(relation.member_count(), 1);
        assert_eq!(
            matches!(
                relation.members().next().unwrap(),
                program::RelationMemberV1::Visible(_)
            ),
            visible,
        );
    }
}

fn solver_dependent_reference_draft(visible: bool) -> program::DraftV1 {
    let fixed_source = program::SourceIdV1::new(60);
    let finite_target = program::TargetIdV1::new(61);
    let fixed_target = program::TargetIdV1::new(62);
    let finite_candidate = program::TargetCandidateIdV1::new(63);
    let finite_paint = program::PaintIdV1::new(64);
    let fixed_paint = program::PaintIdV1::new(65);
    let port = program::SurfaceInputPortIdV1::new(66);
    let surface = program::SurfaceIdV1::new(67);
    let finite_occurrence = program::OccurrenceIdV1::new(68);
    let fixed_occurrence = program::OccurrenceIdV1::new(69);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let mut draft = program::DraftV1::new();
    draft.push_source(fixed_source, Srgb8::new([0x20; 3]));
    draft.push_finite_target(
        finite_target,
        program::FinitePaintDomainV1::try_new(vec![program::TargetCandidateV1::new(
            finite_candidate,
            program::PaintValueV1::opaque(Srgb8::new([0x20; 3])),
        )])
        .unwrap(),
    );
    draft.push_fixed_target(fixed_target, fixed_source);
    draft
        .set_joint_selection(vec![program::JointStateV1::new(vec![
            program::JointChoiceV1::new(finite_target, finite_candidate),
        ])])
        .unwrap();
    draft.push_solid_paint(finite_paint, finite_target);
    draft.push_solid_paint(fixed_paint, fixed_target);
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    draft.push_source_over_occurrence(finite_occurrence, finite_paint, surface, context);
    draft.push_source_over_occurrence(fixed_occurrence, fixed_paint, surface, context);
    draft.push_exact_visible_unary_hard(
        program::ConstraintIdV1::new(70),
        finite_occurrence,
        Srgb8::new([0x20; 3]),
    );
    if visible {
        draft.push_exact_visible_relation_hard(
            program::ConstraintIdV1::new(71),
            program::DirectedRelationV1::try_new(finite_occurrence, vec![fixed_occurrence])
                .unwrap(),
        );
    } else {
        draft.push_exact_intrinsic_relation_hard(
            program::ConstraintIdV1::new(71),
            program::DirectedRelationV1::try_new(finite_target, vec![fixed_target]).unwrap(),
        );
    }
    draft.push_output(program::OutputSlotIdV1::new(72), finite_paint);
    draft
}

#[test]
fn directional_reference_cannot_move_with_solver_state() {
    assert_eq!(
        compile_error_kind(solver_dependent_reference_draft(false)),
        program::CompileErrorKindV1::SolverDependentIntrinsicRelationReference
    );
    assert_eq!(
        compile_error_kind(solver_dependent_reference_draft(true)),
        program::CompileErrorKindV1::SolverDependentVisibleRelationReference
    );
}

fn relation_identity(offset: u32, reverse_candidates: bool) -> program::ContentIdentityV8 {
    let sources = [0, 1, 2].map(|index| program::SourceIdV1::new(offset + index));
    let targets = [0, 1, 2].map(|index| program::TargetIdV1::new(offset + 10 + index));
    let paints = [0, 1, 2].map(|index| program::PaintIdV1::new(offset + 20 + index));
    let occurrences = [0, 1, 2].map(|index| program::OccurrenceIdV1::new(offset + 30 + index));
    let port = program::SurfaceInputPortIdV1::new(offset + 40);
    let surface = program::SurfaceIdV1::new(offset + 41);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let mut draft = program::DraftV1::new();
    for index in 0..3 {
        draft.push_source(sources[index], Srgb8::new([0x20 + index as u8; 3]));
        draft.push_fixed_target(targets[index], sources[index]);
        draft.push_solid_paint(paints[index], targets[index]);
    }
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    for index in 0..3 {
        draft.push_source_over_occurrence(occurrences[index], paints[index], surface, context);
    }
    let mut target_candidates = vec![targets[1], targets[2]];
    let mut occurrence_candidates = vec![occurrences[1], occurrences[2]];
    if reverse_candidates {
        target_candidates.reverse();
        occurrence_candidates.reverse();
    }
    draft.push_exact_intrinsic_relation_hard(
        program::ConstraintIdV1::new(offset + 50),
        program::DirectedRelationV1::try_new(targets[0], target_candidates).unwrap(),
    );
    draft.push_exact_visible_relation_hard(
        program::ConstraintIdV1::new(offset + 51),
        program::DirectedRelationV1::try_new(occurrences[0], occurrence_candidates).unwrap(),
    );
    draft.push_output(program::OutputSlotIdV1::new(offset + 52), paints[0]);
    draft.compile().unwrap().content_identity()
}

#[test]
fn relation_identity_ignores_opaque_names_and_candidate_declaration_order() {
    let canonical = relation_identity(100, false);
    assert_eq!(
        canonical,
        relation_identity(1_000, false),
        "opaque ID renaming must not change content identity",
    );
    assert_eq!(
        canonical,
        relation_identity(100, true),
        "candidate declaration order must not change content identity",
    );
}

#[test]
fn visible_relation_checks_every_candidate_in_every_admitted_scenario() {
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let white_source = program::SourceIdV1::new(1);
    let middle_source = program::SourceIdV1::new(2);
    let white_target = program::TargetIdV1::new(3);
    let middle_target = program::TargetIdV1::new(4);
    let half_opacity = program::OpacityInputIdV1::new(5);
    let white_paint = program::PaintIdV1::new(6);
    let half_white_paint = program::PaintIdV1::new(7);
    let middle_paint = program::PaintIdV1::new(8);
    let surface_port = program::SurfaceInputPortIdV1::new(9);
    let backdrop = program::SurfaceIdV1::new(10);
    let leading_unconstrained_occurrence = program::OccurrenceIdV1::new(11);
    let reference = program::OccurrenceIdV1::new(12);
    let first_candidate = program::OccurrenceIdV1::new(13);
    let interleaved_unconstrained_occurrence = program::OccurrenceIdV1::new(14);
    let second_candidate = program::OccurrenceIdV1::new(15);
    let third_candidate = program::OccurrenceIdV1::new(16);
    let relation = program::DirectedRelationV1::try_new(
        reference,
        vec![first_candidate, second_candidate, third_candidate],
    )
    .unwrap();
    let mut draft = program::DraftV1::new();

    draft.push_source(white_source, Srgb8::new([0xFF; 3]));
    draft.push_source(middle_source, Srgb8::new([0x80; 3]));
    draft.push_fixed_target(white_target, white_source);
    draft.push_fixed_target(middle_target, middle_source);
    draft.push_opacity_input(half_opacity, 0.5);
    draft.push_solid_paint(white_paint, white_target);
    draft.push_opacity_paint(half_white_paint, white_paint, half_opacity);
    draft.push_solid_paint(middle_paint, middle_target);
    draft.push_surface_input_port(surface_port);
    draft.push_input_surface(backdrop, surface_port);
    // Unconstrained occurrences до reference и между candidates делают сдвиг
    // full→compact неаффинным. Evidence ниже поэтому кусает как полный пропуск
    // remap, так и prefix/ordinal mutant для reference и любого candidate.
    draft.push_source_over_occurrence(
        leading_unconstrained_occurrence,
        middle_paint,
        backdrop,
        context,
    );
    draft.push_source_over_occurrence(reference, half_white_paint, backdrop, context);
    draft.push_source_over_occurrence(first_candidate, middle_paint, backdrop, context);
    draft.push_source_over_occurrence(
        interleaved_unconstrained_occurrence,
        half_white_paint,
        backdrop,
        context,
    );
    draft.push_source_over_occurrence(second_candidate, middle_paint, backdrop, context);
    draft.push_source_over_occurrence(third_candidate, half_white_paint, backdrop, context);
    draft.push_exact_visible_relation_hard(program::ConstraintIdV1::new(22), relation);
    draft.push_output(program::OutputSlotIdV1::new(17), half_white_paint);
    draft.push_output(program::OutputSlotIdV1::new(18), middle_paint);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(19).unwrap();
    let black = [Srgb8::new([0x00; 3])];
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [
        program::ScenarioV1::new(20, &black),
        program::ScenarioV1::new(21, &white),
    ];
    let evidence = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    // На чёрном фоне half-white и opaque #808080 оба дают #808080. На белом
    // reference и последний candidate дают #FFFFFF, а первые два candidates —
    // #808080. Вектор violation, violation, pass сохраняет два нарушения для
    // XOR-mutant и одновременно кусает перестановку candidate measurements.
    assert_eq!(evidence.kind(), program::StateKindV1::Failed);
    let mut certificates = evidence.certificates();
    let Some(program::CertificateV1::Conflict(conflict)) = certificates.next() else {
        panic!("the last relation coordinate must produce a hard conflict");
    };
    assert!(certificates.next().is_none());
    assert_eq!(conflict.considered_state_count(), 1);
    assert_eq!(conflict.observation().physical_cases().len(), 2);

    let cells = conflict.cells().collect::<Vec<_>>();
    assert_eq!(cells.len(), 2);
    let mut observed_member_count = 0;
    let mut violating_coordinates = Vec::new();
    for cell in cells {
        let case_index = cell.case_index();
        let program::AssessmentV1::Relation(relation) = cell.assessment() else {
            panic!("directional constraint must expose relation evidence");
        };
        assert_eq!(relation.member_count(), 3);
        let mut cell_has_violation = false;
        for (candidate_index, member) in relation.members().enumerate() {
            observed_member_count += 1;
            let program::RelationMemberV1::Visible(member) = member else {
                panic!("visible relation cannot project intrinsic endpoints");
            };
            assert_eq!(member.reference().occurrence(), reference);
            let expected_candidate =
                [first_candidate, second_candidate, third_candidate][candidate_index];
            assert_eq!(member.candidate().occurrence(), expected_candidate);

            let program::RelationMeasurementV1::ExactSrgb8(measurement) =
                program::RelationMemberV1::Visible(member).measurement()
            else {
                panic!("the exact relation must retain exact pair measurement");
            };
            let program::PhysicalPointV1::EncodedSrgb8SourceOver(reference_physical) =
                member.reference().binding().physical();
            let program::PhysicalPointV1::EncodedSrgb8SourceOver(candidate_physical) =
                member.candidate().binding().physical();
            assert_eq!(measurement.reference(), reference_physical.visible());
            assert_eq!(measurement.candidate(), candidate_physical.visible());

            let is_violation = member.verdict() == program::VerdictV1::Violation;
            assert_eq!(
                program::RelationMemberV1::Visible(member).proof(),
                if is_violation {
                    program::RelationMemberProofV1::ExactSrgb8Violation
                } else {
                    program::RelationMemberProofV1::ExactSrgb8Pass
                }
            );
            if is_violation {
                cell_has_violation = true;
                violating_coordinates.push((case_index, candidate_index));
            }
        }
        assert_eq!(
            relation.verdict(),
            if cell_has_violation {
                program::VerdictV1::Violation
            } else {
                program::VerdictV1::Pass
            },
            "cell passes iff every member passes",
        );
    }
    assert_eq!(observed_member_count, 6);
    assert_eq!(violating_coordinates, vec![(1, 0), (1, 1)]);
}
