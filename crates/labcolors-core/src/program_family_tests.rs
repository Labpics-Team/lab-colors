//! Интеграция точного допущенного образа family в Program.

use crate::family::FAMILY_MEMBERSHIP_ASSESS_CALLS;
use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::{Srgb8, program};

const TARGET: program::TargetIdV1 = program::TargetIdV1::new(1);
const FAMILY: program::FamilyIdV1 = program::FamilyIdV1::new(2);
const SECOND_FAMILY: program::FamilyIdV1 = program::FamilyIdV1::new(102);
const PAINT: program::PaintIdV1 = program::PaintIdV1::new(3);
const PORT: program::SurfaceInputPortIdV1 = program::SurfaceInputPortIdV1::new(4);
const SURFACE: program::SurfaceIdV1 = program::SurfaceIdV1::new(5);
const OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(6);
const FAMILY_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(7);
const VISIBLE_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(8);
const OUTPUT: program::OutputSlotIdV1 = program::OutputSlotIdV1::new(9);
const ROOT: program::PresentationRootIdV1 = program::PresentationRootIdV1::new(11);

fn context() -> program::AppearanceContextV1 {
    program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap()
}

fn family(values: &[[u8; 3]]) -> program::FamilySetV1 {
    program::FamilySetV1::try_from_srgb8_image(
        values.iter().copied().map(Srgb8::new).collect::<Vec<_>>(),
    )
    .unwrap()
}

fn finite_base_draft(
    candidates: &[(program::TargetCandidateIdV1, Srgb8, f64)],
) -> program::DraftV1 {
    let domain = program::FinitePaintDomainV1::try_new(
        candidates
            .iter()
            .map(|(id, source, alpha)| {
                program::TargetCandidateV1::new(
                    *id,
                    program::PaintValueV1::try_new(*source, *alpha).unwrap(),
                )
            })
            .collect(),
    )
    .unwrap();
    let mut draft = program::DraftV1::new();
    draft.push_finite_target(TARGET, domain);
    draft
        .set_joint_selection(
            candidates
                .iter()
                .map(|(candidate, _, _)| {
                    program::JointStateV1::new(vec![program::JointChoiceV1::new(
                        TARGET, *candidate,
                    )])
                })
                .collect(),
        )
        .unwrap();
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_surface_input_port(PORT);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_output(OUTPUT, PAINT);
    draft
}

fn finite_family_draft(
    candidates: &[(program::TargetCandidateIdV1, Srgb8, f64)],
    family_values: &[[u8; 3]],
    declare_family: bool,
    family_mode_hard: bool,
    expected_visible: Srgb8,
) -> program::DraftV1 {
    let mut draft = finite_base_draft(candidates);
    if declare_family {
        draft.push_family(FAMILY, family(family_values));
    }
    if family_mode_hard {
        draft.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, TARGET, FAMILY);
    } else {
        draft.push_intrinsic_family_membership_report_only(FAMILY_CONSTRAINT, TARGET, FAMILY);
    }
    draft.push_exact_visible_unary_hard(VISIBLE_CONSTRAINT, OCCURRENCE, expected_visible);
    draft
}

#[test]
fn hard_family_membership_selects_a_member_rechecks_it_and_retains_alpha() {
    let rejected = program::TargetCandidateIdV1::new(10);
    let accepted = program::TargetCandidateIdV1::new(11);
    let red = Srgb8::new([255, 0, 0]);
    let blue = Srgb8::new([0, 0, 255]);
    let expected_family_content = family(&[[0, 0, 0], [0, 0, 254], [0, 0, 255]]).content_identity();
    let owner = finite_family_draft(
        &[(rejected, red, 0.25), (accepted, blue, 0.5)],
        &[[0, 0, 0], [0, 0, 254], [0, 0, 255]],
        true,
        true,
        Srgb8::new([0, 0, 128]),
    )
    .compile()
    .unwrap();
    let mut session = owner.instantiate(12).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(13, &black)];
    FAMILY_MEMBERSHIP_ASSESS_CALLS.with(|calls| calls.set(0));
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
        panic!("the first family member must be selected");
    };
    assert_eq!(verified.selected_state_index(), Some(1));
    assert_eq!(verified.outputs().next().unwrap().source(), blue);
    assert_eq!(
        FAMILY_MEMBERSHIP_ASSESS_CALLS.with(core::cell::Cell::get),
        3,
        "one rejection, one search pass and one fresh final pass are required",
    );
    let intrinsic = verified
        .cells()
        .find_map(|cell| match cell.assessment() {
            program::AssessmentV1::IntrinsicUnary(evidence) => Some(evidence),
            _ => None,
        })
        .expect("family evidence must use the existing intrinsic-unary cell");
    assert_eq!(intrinsic.binding().target(), TARGET);
    assert_eq!(intrinsic.binding().value().source(), blue);
    assert_eq!(
        intrinsic.binding().value().opacity().to_bits(),
        0.5_f64.to_bits()
    );
    let program::IntrinsicUnaryMeasurementV1::FamilyMembership(measurement) =
        intrinsic.measurement()
    else {
        panic!("family invocation must retain typed membership measurement");
    };
    assert_eq!(measurement.family(), FAMILY);
    assert_eq!(measurement.signal(), blue);
    assert_eq!(measurement.content(), expected_family_content);
    let program::IntrinsicUnaryProofV1::FamilyMembershipPass = intrinsic.proof() else {
        panic!("selected family member must carry an inclusion witness");
    };
}

#[test]
fn missing_family_is_a_typed_atomic_compile_error() {
    let candidate = program::TargetCandidateIdV1::new(20);
    let blue = Srgb8::new([0, 0, 255]);
    let draft = finite_family_draft(&[(candidate, blue, 1.0)], &[[0, 0, 255]], false, true, blue);

    let error = match draft.compile() {
        Ok(_) => panic!("an unresolved family edge must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        program::CompileErrorV1::MissingConstraintFamily {
            constraint: FAMILY_CONSTRAINT,
            family: FAMILY,
        },
    );
    assert_eq!(
        error.kind(),
        program::CompileErrorKindV1::MissingConstraintFamily
    );
    assert_eq!(
        error.primary_handle(),
        Some(program::CompileErrorHandleV1::Constraint(FAMILY_CONSTRAINT)),
    );
    assert_eq!(
        error.related_handle(),
        Some(program::CompileErrorHandleV1::Family(FAMILY)),
    );
}

#[test]
fn missing_family_error_is_canonical_under_constraint_permutation() {
    let candidate = program::TargetCandidateIdV1::new(21);
    let blue = Srgb8::new([0, 0, 255]);
    let smaller_constraint = program::ConstraintIdV1::new(210);
    let larger_constraint = program::ConstraintIdV1::new(211);
    let smaller_family = program::FamilyIdV1::new(212);
    let larger_family = program::FamilyIdV1::new(213);
    let compile = |permuted: bool| {
        let mut draft = finite_base_draft(&[(candidate, blue, 1.0)]);
        let mut declarations = [
            (smaller_constraint, smaller_family),
            (larger_constraint, larger_family),
        ];
        if permuted {
            declarations.reverse();
        }
        for (constraint, family) in declarations {
            draft.push_intrinsic_family_membership_hard(constraint, TARGET, family);
        }
        draft.push_exact_visible_unary_hard(VISIBLE_CONSTRAINT, OCCURRENCE, blue);
        match draft.compile() {
            Ok(_) => panic!("missing family references must not compile"),
            Err(error) => error,
        }
    };
    let expected = program::CompileErrorV1::MissingConstraintFamily {
        constraint: smaller_constraint,
        family: smaller_family,
    };

    assert_eq!(compile(false), expected);
    assert_eq!(compile(true), expected);
}

#[test]
fn missing_intrinsic_target_precedes_the_same_constraints_missing_family() {
    let candidate = program::TargetCandidateIdV1::new(22);
    let blue = Srgb8::new([0, 0, 255]);
    let missing_target = program::TargetIdV1::new(220);
    let mut draft = finite_base_draft(&[(candidate, blue, 1.0)]);
    draft.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, missing_target, SECOND_FAMILY);
    draft.push_exact_visible_unary_hard(VISIBLE_CONSTRAINT, OCCURRENCE, blue);

    let error = match draft.compile() {
        Ok(_) => panic!("missing intrinsic target must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        program::CompileErrorV1::MissingIntrinsicUnaryTarget {
            constraint: FAMILY_CONSTRAINT,
            target: missing_target,
        },
    );
}

#[test]
fn family_id_and_error_handle_preserve_a_nontrivial_opaque_value() {
    assert_eq!(SECOND_FAMILY.value(), 102);
    assert_eq!(
        program::CompileErrorHandleV1::Family(SECOND_FAMILY).value(),
        102,
    );
}

#[test]
fn facade_family_content_bytes_change_with_the_admitted_image() {
    let blue = family(&[[0, 0, 255]]).content_identity();
    let red = family(&[[255, 0, 0]]).content_identity();

    assert_ne!(blue.as_bytes(), red.as_bytes());
}

#[test]
fn report_only_family_violation_is_retained_but_does_not_steer_selection() {
    let first = program::TargetCandidateIdV1::new(30);
    let second = program::TargetCandidateIdV1::new(31);
    let red = Srgb8::new([255, 0, 0]);
    let blue = Srgb8::new([0, 0, 255]);
    let expected_family_content = family(&[[0, 0, 255]]).content_identity();
    let owner = finite_family_draft(
        &[(first, red, 0.25), (second, blue, 0.5)],
        &[[0, 0, 255]],
        true,
        false,
        Srgb8::new([64, 0, 0]),
    )
    .compile()
    .unwrap();
    let mut session = owner.instantiate(32).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(33, &black)];
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
        panic!("a report-only exclusion must not reject the first hard-valid state");
    };
    assert_eq!(verified.selected_state_index(), Some(0));
    let output = verified.outputs().next().unwrap();
    assert_eq!(output.source(), red);
    assert_eq!(output.opacity().to_bits(), 0.25_f64.to_bits());
    let cell = verified
        .cells()
        .find(|cell| cell.constraint() == FAMILY_CONSTRAINT)
        .expect("report-only family evidence must be retained");
    assert_eq!(cell.mode(), program::ConstraintModeV1::ReportOnly);
    let program::AssessmentV1::IntrinsicUnary(intrinsic) = cell.assessment() else {
        panic!("family membership must remain intrinsic-unary evidence");
    };
    assert_eq!(intrinsic.verdict(), program::VerdictV1::Violation);
    let program::IntrinsicUnaryMeasurementV1::FamilyMembership(measurement) =
        intrinsic.measurement()
    else {
        panic!("family violation must retain typed membership measurement");
    };
    assert_eq!(measurement.family(), FAMILY);
    assert_eq!(measurement.signal(), red);
    assert_eq!(measurement.content(), expected_family_content);
    let program::IntrinsicUnaryProofV1::FamilyMembershipViolation = intrinsic.proof() else {
        panic!("the non-member must retain an exact exclusion witness");
    };
}

#[test]
fn compiled_family_index_resolves_the_requested_nonzero_declaration() {
    let candidate = program::TargetCandidateIdV1::new(34);
    let blue = Srgb8::new([0, 0, 255]);
    let mut draft = finite_base_draft(&[(candidate, blue, 1.0)]);
    draft.push_family(FAMILY, family(&[[255, 0, 0]]));
    draft.push_family(SECOND_FAMILY, family(&[[0, 0, 255]]));
    draft.push_intrinsic_family_membership_report_only(FAMILY_CONSTRAINT, TARGET, FAMILY);
    let second_constraint = program::ConstraintIdV1::new(35);
    draft.push_intrinsic_family_membership_hard(second_constraint, TARGET, SECOND_FAMILY);
    draft.push_exact_visible_unary_hard(VISIBLE_CONSTRAINT, OCCURRENCE, blue);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(36).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(37, &black)];
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
        panic!("constraint must classify against the requested second family");
    };
    let cell = verified
        .cells()
        .find(|cell| cell.constraint() == second_constraint)
        .expect("second-family evidence must be retained");
    let program::AssessmentV1::IntrinsicUnary(intrinsic) = cell.assessment() else {
        panic!("family evidence must remain intrinsic-unary");
    };
    let program::IntrinsicUnaryMeasurementV1::FamilyMembership(measurement) =
        intrinsic.measurement()
    else {
        panic!("second-family measurement must remain typed");
    };
    assert_eq!(measurement.family(), SECOND_FAMILY);
    assert_eq!(measurement.signal(), blue);
    assert_eq!(intrinsic.verdict(), program::VerdictV1::Pass);
}

#[test]
fn two_invalid_states_produce_an_exhaustive_conflict_with_exact_family_witnesses() {
    let below = program::TargetCandidateIdV1::new(40);
    let between = program::TargetCandidateIdV1::new(41);
    let below_signal = Srgb8::new([5; 3]);
    let between_signal = Srgb8::new([15; 3]);
    let expected_family_content = family(&[[10; 3], [20; 3]]).content_identity();
    let mut draft =
        finite_base_draft(&[(below, below_signal, 1.0), (between, between_signal, 1.0)]);
    draft.push_family(FAMILY, family(&[[10; 3], [20; 3]]));
    draft.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, TARGET, FAMILY);
    draft.push_exact_visible_unary_report_only(VISIBLE_CONSTRAINT, OCCURRENCE, Srgb8::new([0; 3]));
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(42).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(43, &black)];
    FAMILY_MEMBERSHIP_ASSESS_CALLS.with(|calls| calls.set(0));
    let evidence = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    let Some(program::CertificateV1::Conflict(conflict)) = evidence.certificates().next() else {
        panic!("every state violates the hard family, so the result must be Conflict");
    };
    assert_eq!(conflict.considered_state_count(), 2);
    let cells = conflict
        .cells()
        .filter(|cell| cell.constraint() == FAMILY_CONSTRAINT)
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        2,
        "the family constraint must retain one witness for each state",
    );
    assert_eq!(
        FAMILY_MEMBERSHIP_ASSESS_CALLS.with(core::cell::Cell::get),
        4,
        "two search classifications and two exhaustive conflict rechecks are required",
    );
    for cell in cells {
        assert_eq!(cell.case_index(), 0);
        assert_eq!(cell.mode(), program::ConstraintModeV1::Hard);
        let program::AssessmentV1::IntrinsicUnary(intrinsic) = cell.assessment() else {
            panic!("every retained cell must carry family evidence");
        };
        let program::IntrinsicUnaryMeasurementV1::FamilyMembership(measurement) =
            intrinsic.measurement()
        else {
            panic!("every family violation must retain typed membership measurement");
        };
        assert_eq!(measurement.family(), FAMILY);
        assert_eq!(measurement.content(), expected_family_content);
        let expected_signal = match cell.state_index() {
            0 => below_signal,
            1 => between_signal,
            state => panic!("unexpected state {state}"),
        };
        assert_eq!(measurement.signal(), expected_signal);
        let program::IntrinsicUnaryProofV1::FamilyMembershipViolation = intrinsic.proof() else {
            panic!("both candidate signals are absent from the family");
        };
    }
}

#[test]
fn equal_sources_with_distinct_opacity_remain_distinct_candidate_states() {
    let translucent = program::TargetCandidateIdV1::new(50);
    let opaque = program::TargetCandidateIdV1::new(51);
    let blue = Srgb8::new([0, 0, 255]);
    let owner = finite_family_draft(
        &[(translucent, blue, 0.5), (opaque, blue, 1.0)],
        &[[0, 0, 255]],
        true,
        true,
        blue,
    )
    .compile()
    .unwrap();
    let mut session = owner.instantiate(52).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(53, &black)];
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
        panic!("the opaque candidate must remain selectable after the translucent candidate");
    };
    assert_eq!(verified.selected_state_index(), Some(1));
    let output = verified.outputs().next().unwrap();
    assert_eq!(output.source(), blue);
    assert_eq!(output.opacity().to_bits(), 1.0_f64.to_bits());
}

fn family_and_declared_set_verdicts(
    signal: Srgb8,
    admitted_family: &[[u8; 3]],
) -> (program::StateKindV1, [program::VerdictV1; 2]) {
    let candidate = program::TargetCandidateIdV1::new(60);
    let mut draft = finite_base_draft(&[(candidate, signal, 1.0)]);
    draft.push_family(FAMILY, family(admitted_family));
    draft.push_point_presentation_root(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, TARGET, FAMILY);
    draft.push_declared_srgb8_clean_set_hard(VISIBLE_CONSTRAINT, ROOT, OCCURRENCE);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(61).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(62, &black)];
    let evidence = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let kind = evidence.kind();
    let mut verdicts = [program::VerdictV1::Violation; 2];
    match evidence.certificates().next().unwrap() {
        program::CertificateV1::Verified(certificate) => {
            assert_eq!(certificate.cells().len(), 2);
            for cell in certificate.cells() {
                let index = verdict_index(cell.constraint());
                verdicts[index] = cell.assessment().verdict();
            }
        }
        program::CertificateV1::Conflict(certificate) => {
            assert_eq!(certificate.cells().len(), 2);
            for cell in certificate.cells() {
                let index = verdict_index(cell.constraint());
                verdicts[index] = cell.assessment().verdict();
            }
        }
    }
    (kind, verdicts)
}

fn verdict_index(constraint: program::ConstraintIdV1) -> usize {
    if constraint == FAMILY_CONSTRAINT {
        0
    } else if constraint == VISIBLE_CONSTRAINT {
        1
    } else {
        panic!("unexpected constraint");
    }
}

#[test]
fn family_and_declared_point_convention_are_independent_hard_constraints_over_the_full_two_by_two()
{
    let predicate_member = Srgb8::new([0, 200, 70]);
    let predicate_nonmember = Srgb8::new([0, 200, 71]);
    let other = [[255, 0, 255]];
    let cases = [
        (
            predicate_member,
            [[0, 200, 70]].as_slice(),
            program::StateKindV1::Ready,
            [program::VerdictV1::Pass, program::VerdictV1::Pass],
        ),
        (
            predicate_nonmember,
            [[0, 200, 71]].as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Pass, program::VerdictV1::Violation],
        ),
        (
            predicate_member,
            other.as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Violation, program::VerdictV1::Pass],
        ),
        (
            predicate_nonmember,
            other.as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Violation, program::VerdictV1::Violation],
        ),
    ];

    for (signal, admitted, expected_kind, expected_verdicts) in cases {
        let (kind, verdicts) = family_and_declared_set_verdicts(signal, admitted);
        assert_eq!(kind, expected_kind, "unexpected lifecycle for {signal:?}");
        assert_eq!(
            verdicts, expected_verdicts,
            "unexpected verdicts for {signal:?}"
        );
    }
}

#[test]
fn empty_family_image_is_rejected_before_a_draft_can_represent_it() {
    assert_eq!(
        program::FamilySetV1::try_from_srgb8_image(Vec::new()),
        Err(program::FamilySetAdmissionErrorV1::Empty),
    );
}

#[test]
fn duplicate_and_unused_families_are_typed_compile_errors() {
    let candidate = program::TargetCandidateIdV1::new(70);
    let blue = Srgb8::new([0, 0, 255]);

    let mut duplicate = finite_base_draft(&[(candidate, blue, 1.0)]);
    duplicate.push_family(FAMILY, family(&[[0, 0, 255]]));
    duplicate.push_family(FAMILY, family(&[[255, 0, 0]]));
    duplicate.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, TARGET, FAMILY);
    let duplicate_error = match duplicate.compile() {
        Ok(_) => panic!("a duplicate family must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        duplicate_error,
        program::CompileErrorV1::DuplicateFamily { family: FAMILY },
    );
    assert_eq!(
        duplicate_error.kind(),
        program::CompileErrorKindV1::DuplicateFamily,
    );
    assert_eq!(
        duplicate_error.primary_handle(),
        Some(program::CompileErrorHandleV1::Family(FAMILY)),
    );
    assert_eq!(duplicate_error.related_handle(), None);

    let mut unused = finite_base_draft(&[(candidate, blue, 1.0)]);
    unused.push_family(FAMILY, family(&[[0, 0, 255]]));
    unused.push_exact_visible_unary_hard(VISIBLE_CONSTRAINT, OCCURRENCE, blue);
    let unused_error = match unused.compile() {
        Ok(_) => panic!("an unused family must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        unused_error,
        program::CompileErrorV1::UnusedFamily { family: FAMILY },
    );
    assert_eq!(
        unused_error.kind(),
        program::CompileErrorKindV1::UnusedFamily,
    );
    assert_eq!(
        unused_error.primary_handle(),
        Some(program::CompileErrorHandleV1::Family(FAMILY)),
    );
    assert_eq!(unused_error.related_handle(), None);
}

#[test]
fn replay_corruption_is_a_typed_invalid_family_image_compile_error() {
    let candidate = program::TargetCandidateIdV1::new(80);
    let blue = Srgb8::new([0, 0, 255]);
    let mut corrupted = family(&[[0, 0, 255]]);
    corrupted.corrupt_first_member_for_test(Srgb8::new([255, 0, 0]));
    let mut draft = finite_base_draft(&[(candidate, blue, 1.0)]);
    draft.push_family(FAMILY, corrupted);
    draft.push_intrinsic_family_membership_hard(FAMILY_CONSTRAINT, TARGET, FAMILY);

    let error = match draft.compile() {
        Ok(_) => panic!("a corrupted admitted family must fail replay"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        program::CompileErrorV1::InvalidFamilyImage { family: FAMILY },
    );
    assert_eq!(
        error.kind(),
        program::CompileErrorKindV1::InvalidFamilyImage
    );
    assert_eq!(
        error.primary_handle(),
        Some(program::CompileErrorHandleV1::Family(FAMILY)),
    );
    assert_eq!(error.related_handle(), None);
}
