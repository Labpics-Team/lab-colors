use crate::lcs_occurrence::MODELED_TRISTIMULUS_DERIVATION_CALLS;
use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::{Srgb8, program};
use proptest::prelude::*;

const SOURCE: program::SourceIdV1 = program::SourceIdV1::new(1);
const TARGET: program::TargetIdV1 = program::TargetIdV1::new(2);
const PORT: program::SurfaceInputPortIdV1 = program::SurfaceInputPortIdV1::new(3);
const PAINT: program::PaintIdV1 = program::PaintIdV1::new(4);
const SURFACE: program::SurfaceIdV1 = program::SurfaceIdV1::new(5);
const OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(6);
const CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(7);
const OUTPUT: program::OutputSlotIdV1 = program::OutputSlotIdV1::new(8);
const ROOT: program::PresentationRootIdV1 = program::PresentationRootIdV1::new(9);

fn finite_domain(candidates: Vec<program::TargetCandidateV1>) -> program::FinitePaintDomainV1 {
    program::FinitePaintDomainV1::try_new(candidates).unwrap()
}

fn context() -> program::AppearanceContextV1 {
    program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap()
}

fn fixed_draft(source: Srgb8) -> program::DraftV1 {
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, source);
    draft.push_fixed_target(TARGET, SOURCE);
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_point_presentation_root(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_output(OUTPUT, PAINT);
    draft
}

fn one_case(backdrop: &[Srgb8; 1]) -> [program::ScenarioV1<'_>; 1] {
    [program::ScenarioV1::new(1, backdrop)]
}

#[test]
fn undeclared_exact_presentation_target_is_a_typed_compile_error() {
    let mut draft = fixed_draft(Srgb8::new([0, 200, 71]));
    draft.push_declared_srgb8_clean_set_hard(
        CONSTRAINT,
        program::PresentationRootIdV1::new(10),
        OCCURRENCE,
    );

    let error = match draft.compile() {
        Err(error) => error,
        Ok(_) => panic!("undeclared presentation target must not compile"),
    };
    assert_eq!(
        error,
        program::CompileErrorV1::MissingConstraintPresentationTarget {
            constraint: CONSTRAINT,
            root: program::PresentationRootIdV1::new(10),
            occurrence: OCCURRENCE,
        },
    );
}

#[test]
fn report_only_dirty_terminal_is_retained_as_a_typed_rejected_violation() {
    let dirty = Srgb8::new([0, 200, 71]);
    let mut draft = fixed_draft(dirty);
    draft.push_declared_srgb8_clean_set_report_only(CONSTRAINT, ROOT, OCCURRENCE);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    assert_eq!(projection.kind(), program::StateKindV1::Ready);
    let Some(program::CertificateV1::Verified(certificate)) = projection.certificates().next()
    else {
        panic!("report-only rejection must retain a Verified certificate");
    };
    let cell = certificate.cells().next().unwrap();
    assert_eq!(
        cell.subject(),
        program::ConstraintSubjectV1::PointPresentation {
            root: ROOT,
            occurrence: OCCURRENCE,
            terminal: OCCURRENCE,
        },
    );
    let program::AssessmentV1::DeclaredSrgb8CleanSet(evidence) = cell.assessment() else {
        panic!("clean-set constraint must retain clean-set evidence");
    };
    assert_eq!(evidence.verdict(), program::VerdictV1::Violation);
    assert_eq!(
        evidence.violation(),
        Some(program::DeclaredSrgb8CleanSetViolationKindV1::Rejected),
    );
    assert_eq!(evidence.visible(), Some(dirty));
    assert_eq!(evidence.rejected_blue_interval(), Some([71, 101]));
}

#[test]
fn hard_absent_final_owned_domain_is_a_violation_not_a_pass() {
    let mut draft = fixed_draft(Srgb8::new([0; 3]));
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    assert_eq!(projection.kind(), program::StateKindV1::Failed);
    let Some(program::CertificateV1::Conflict(certificate)) = projection.certificates().next()
    else {
        panic!("absent final-owned domain must reject a hard fixed candidate");
    };
    let cell = certificate.cells().next().unwrap();
    let program::AssessmentV1::DeclaredSrgb8CleanSet(evidence) = cell.assessment() else {
        panic!("clean-set constraint must retain clean-set evidence");
    };
    assert_eq!(
        evidence.violation(),
        Some(program::DeclaredSrgb8CleanSetViolationKindV1::FinalOwnedDomainAbsent),
    );
    assert_eq!(evidence.visible(), None);
    assert_eq!(evidence.rejected_blue_interval(), None);
}

#[test]
fn finite_search_skips_dirty_and_freshly_rechecks_the_first_clean_state() {
    let dirty_id = program::TargetCandidateIdV1::new(20);
    let clean_id = program::TargetCandidateIdV1::new(21);
    let dirty = Srgb8::new([0, 200, 71]);
    let clean = Srgb8::new([0, 200, 70]);
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, dirty);
    draft.push_finite_target(
        TARGET,
        finite_domain(vec![
            program::TargetCandidateV1::new(dirty_id, program::PaintValueV1::opaque(dirty)),
            program::TargetCandidateV1::new(clean_id, program::PaintValueV1::opaque(clean)),
        ]),
    );
    draft
        .set_joint_selection(vec![
            program::JointStateV1::new(vec![program::JointChoiceV1::new(TARGET, dirty_id)]),
            program::JointStateV1::new(vec![program::JointChoiceV1::new(TARGET, clean_id)]),
        ])
        .unwrap();
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_point_presentation_root(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_output(OUTPUT, PAINT);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    let Some(program::CertificateV1::Verified(certificate)) = projection.certificates().next()
    else {
        panic!("the first clean finite state must be selected");
    };
    assert_eq!(certificate.outputs().next().unwrap().source(), clean);
    assert_eq!(certificate.selected_state_index(), Some(1));
    let program::AssessmentV1::DeclaredSrgb8CleanSet(evidence) =
        certificate.cells().next().unwrap().assessment()
    else {
        panic!("final recheck must retain clean-set evidence");
    };
    assert_eq!(evidence.verdict(), program::VerdictV1::Pass);
    assert_eq!(evidence.visible(), Some(clean));
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
        0,
        "an encoded clean-set constraint must not derive LCS",
    );
}

#[test]
fn two_clean_constraints_and_causal_reporting_share_one_phase_materialization() {
    let mut draft = fixed_draft(Srgb8::new([0, 200, 71]));
    draft.push_declared_srgb8_clean_set_report_only(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_report_only(
        program::ConstraintIdV1::new(10),
        ROOT,
        OCCURRENCE,
    );
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    let Some(program::CertificateV1::Verified(certificate)) = projection.certificates().next()
    else {
        panic!("report-only constraints must retain a Verified certificate");
    };
    assert_eq!(certificate.cells().len(), 2);
    assert_eq!(
        owner.point_resolution_count_for_test(&session),
        Some((0, 1))
    );
}

#[test]
fn hard_and_report_phases_do_not_reuse_resolution_authority() {
    let mut draft = fixed_draft(Srgb8::new([255, 0, 0]));
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_report_only(
        program::ConstraintIdV1::new(10),
        ROOT,
        OCCURRENCE,
    );
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    assert_eq!(
        owner.point_resolution_count_for_test(&session),
        Some((1, 1))
    );
}

#[test]
fn downstream_occlusion_is_absent_even_when_the_inner_nominal_color_is_rejected() {
    let clean_source = program::SourceIdV1::new(11);
    let clean_target = program::TargetIdV1::new(12);
    let clean_paint = program::PaintIdV1::new(13);
    let derived_surface = program::SurfaceIdV1::new(14);
    let terminal = program::OccurrenceIdV1::new(15);
    let dirty = Srgb8::new([0, 200, 71]);
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, dirty);
    draft.push_source(clean_source, Srgb8::new([255, 0, 0]));
    draft.push_fixed_target(TARGET, SOURCE);
    draft.push_fixed_target(clean_target, clean_source);
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_solid_paint(clean_paint, clean_target);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_occurrence_surface(derived_surface, OCCURRENCE);
    draft.push_source_over_occurrence(terminal, clean_paint, derived_surface, context());
    draft.push_point_presentation_root(ROOT, terminal);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_output(OUTPUT, clean_paint);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    let Some(program::CertificateV1::Conflict(certificate)) = projection.certificates().next()
    else {
        panic!("opaque downstream replacement must erase the inner final-owned domain");
    };
    let cell = certificate.cells().next().unwrap();
    assert_eq!(
        cell.subject(),
        program::ConstraintSubjectV1::PointPresentation {
            root: ROOT,
            occurrence: OCCURRENCE,
            terminal,
        },
    );
    let program::AssessmentV1::DeclaredSrgb8CleanSet(evidence) = cell.assessment() else {
        panic!("clean-set constraint must retain clean-set evidence");
    };
    assert_eq!(
        evidence.violation(),
        Some(program::DeclaredSrgb8CleanSetViolationKindV1::FinalOwnedDomainAbsent),
    );
}

fn nested_identity_draft(clean_target_occurrence: program::OccurrenceIdV1) -> program::DraftV1 {
    let upper_source = program::SourceIdV1::new(11);
    let upper_target = program::TargetIdV1::new(12);
    let upper_paint = program::PaintIdV1::new(13);
    let derived_surface = program::SurfaceIdV1::new(14);
    let terminal = program::OccurrenceIdV1::new(15);
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, Srgb8::new([0, 200, 71]));
    draft.push_source(upper_source, Srgb8::new([255, 0, 0]));
    draft.push_fixed_target(TARGET, SOURCE);
    draft.push_fixed_target(upper_target, upper_source);
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_solid_paint(upper_paint, upper_target);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_occurrence_surface(derived_surface, OCCURRENCE);
    draft.push_source_over_occurrence(terminal, upper_paint, derived_surface, context());
    draft.push_point_presentation_root(ROOT, terminal);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, terminal);
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, clean_target_occurrence);
    draft.push_output(OUTPUT, upper_paint);
    draft
}

#[test]
fn clean_constraint_identity_binds_the_whole_presentation_subject() {
    let inner = nested_identity_draft(OCCURRENCE).compile().unwrap();
    let terminal_id = program::OccurrenceIdV1::new(15);
    let terminal = nested_identity_draft(terminal_id).compile().unwrap();

    assert_ne!(inner.content_identity(), terminal.content_identity());
}

#[test]
fn clean_constraint_mode_is_content_bound() {
    let hard = fixed_draft(Srgb8::new([255, 0, 0]));
    let mut hard = hard;
    hard.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    let mut report = fixed_draft(Srgb8::new([255, 0, 0]));
    report.push_declared_srgb8_clean_set_report_only(CONSTRAINT, ROOT, OCCURRENCE);

    assert_ne!(
        hard.compile().unwrap().content_identity(),
        report.compile().unwrap().content_identity(),
    );
}

#[test]
fn clean_family_fresh_recheck_failure_retains_the_presentation_subject() {
    let candidate = program::TargetCandidateIdV1::new(20);
    let clean = Srgb8::new([255, 0, 0]);
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, clean);
    draft.push_finite_target(
        TARGET,
        finite_domain(vec![program::TargetCandidateV1::new(
            candidate,
            program::PaintValueV1::opaque(clean),
        )]),
    );
    draft
        .set_joint_selection(vec![program::JointStateV1::new(vec![
            program::JointChoiceV1::new(TARGET, candidate),
        ])])
        .unwrap();
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_point_presentation_root(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_final_recheck_mutant(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_output(OUTPUT, PAINT);
    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);
    let error = match owner.commit(
        &mut session,
        program::UpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        },
    ) {
        Err(error) => error,
        Ok(_) => panic!("mutant must diverge only at the fresh hard recheck"),
    };

    let program::UpdateErrorV1::InternalInvariant {
        source:
            program::UpdateInvariantFailureV1::SelectionRecheck {
                state_index,
                case_index,
                constraint,
                subject,
                hard_violation_count,
            },
    } = error
    else {
        panic!("fresh clean-set divergence must use the typed recheck invariant");
    };
    assert_eq!((state_index, case_index, constraint), (0, 0, CONSTRAINT));
    assert_eq!(hard_violation_count, 1);
    assert_eq!(
        subject,
        program::ConstraintSubjectV1::PointPresentation {
            root: ROOT,
            occurrence: OCCURRENCE,
            terminal: OCCURRENCE,
        },
    );
}

fn opaque_named_clean_identity(name: u32) -> program::ContentIdentityV6 {
    let source = program::SourceIdV1::new(name);
    let target = program::TargetIdV1::new(name);
    let port = program::SurfaceInputPortIdV1::new(name);
    let paint = program::PaintIdV1::new(name);
    let surface = program::SurfaceIdV1::new(name);
    let occurrence = program::OccurrenceIdV1::new(name);
    let root = program::PresentationRootIdV1::new(name);
    let constraint = program::ConstraintIdV1::new(name);
    let output = program::OutputSlotIdV1::new(name);
    let mut draft = program::DraftV1::new();
    draft.push_source(source, Srgb8::new([255, 0, 0]));
    draft.push_fixed_target(target, source);
    draft.push_surface_input_port(port);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, port);
    draft.push_source_over_occurrence(occurrence, paint, surface, context());
    draft.push_point_presentation_root(root, occurrence);
    draft.push_point_presentation_target(root, occurrence);
    draft.push_declared_srgb8_clean_set_hard(constraint, root, occurrence);
    draft.push_output(output, paint);
    draft.compile().unwrap().content_identity()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn clean_constraint_identity_is_invariant_under_opaque_renaming(name in any::<u32>()) {
        prop_assert_eq!(opaque_named_clean_identity(1), opaque_named_clean_identity(name));
    }
}

#[test]
fn clean_constraint_declaration_order_is_not_policy() {
    let mut first = fixed_draft(Srgb8::new([255, 0, 0]));
    first.push_declared_srgb8_clean_set_report_only(CONSTRAINT, ROOT, OCCURRENCE);
    first.push_declared_srgb8_clean_set_report_only(
        program::ConstraintIdV1::new(10),
        ROOT,
        OCCURRENCE,
    );
    let mut second = fixed_draft(Srgb8::new([255, 0, 0]));
    second.push_declared_srgb8_clean_set_report_only(
        program::ConstraintIdV1::new(10),
        ROOT,
        OCCURRENCE,
    );
    second.push_declared_srgb8_clean_set_report_only(CONSTRAINT, ROOT, OCCURRENCE);

    assert_eq!(
        first.compile().unwrap().content_identity(),
        second.compile().unwrap().content_identity(),
    );
}

fn finite_clean_owner(colors: &[Srgb8]) -> program::OwnerV1 {
    let candidates = colors
        .iter()
        .copied()
        .enumerate()
        .map(|(index, color)| {
            program::TargetCandidateV1::new(
                program::TargetCandidateIdV1::new(index as u32 + 20),
                program::PaintValueV1::opaque(color),
            )
        })
        .collect::<Vec<_>>();
    let states = (0..colors.len())
        .map(|index| {
            program::JointStateV1::new(vec![program::JointChoiceV1::new(
                TARGET,
                program::TargetCandidateIdV1::new(index as u32 + 20),
            )])
        })
        .collect::<Vec<_>>();
    let mut draft = program::DraftV1::new();
    draft.push_source(SOURCE, colors[0]);
    draft.push_finite_target(TARGET, finite_domain(candidates));
    draft.set_joint_selection(states).unwrap();
    draft.push_surface_input_port(PORT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(OCCURRENCE, PAINT, SURFACE, context());
    draft.push_point_presentation_root(ROOT, OCCURRENCE);
    draft.push_point_presentation_target(ROOT, OCCURRENCE);
    draft.push_declared_srgb8_clean_set_hard(CONSTRAINT, ROOT, OCCURRENCE);
    draft.push_output(OUTPUT, PAINT);
    draft.compile().unwrap()
}

#[test]
fn rejected_clean_search_states_do_not_add_hot_path_allocations() {
    let direct = finite_clean_owner(&[Srgb8::new([255, 0, 0])]);
    let rejected = finite_clean_owner(&[
        Srgb8::new([0, 200, 71]),
        Srgb8::new([0, 200, 72]),
        Srgb8::new([0, 200, 73]),
        Srgb8::new([255, 0, 0]),
    ]);
    let mut direct_session = direct.instantiate(1).unwrap();
    let mut rejected_session = rejected.instantiate(2).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = one_case(&backdrop);

    let (_, direct_allocations) = crate::test_support::measured_allocations(|| {
        direct
            .commit(
                &mut direct_session,
                program::UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap()
            .kind()
    });
    let (_, rejected_allocations) = crate::test_support::measured_allocations(|| {
        rejected
            .commit(
                &mut rejected_session,
                program::UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap()
            .kind()
    });

    assert_eq!(rejected_allocations, direct_allocations);
}
