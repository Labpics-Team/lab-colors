//! RED contract for the sole concrete Core package seam.
//!
//! This integration crate deliberately has no access to Core-private generic
//! evaluator/session machinery. It must compile using only one hidden,
//! concrete package module once that seam is linked after the P3 + weak-owner
//! rebase.

use labcolors_core::Srgb8;
use labcolors_core::package_bridge::{
    PackageProgramAppearanceContextErrorKindV1, PackageProgramAppearanceContextFieldV1,
    PackageProgramAppearanceContextV1, PackageProgramCertificateV1,
    PackageProgramCompileErrorHandleV1, PackageProgramCompileErrorKindV1,
    PackageProgramCompileErrorV1, PackageProgramConstraintIdV1, PackageProgramDraftErrorV1,
    PackageProgramDraftV1, PackageProgramInstantiateErrorV1, PackageProgramJointChoiceV1,
    PackageProgramJointOrderErrorV1, PackageProgramJointStateV1,
    PackageProgramNumericDomainErrorV1, PackageProgramOccurrenceIdV1,
    PackageProgramOpacityInputIdV1, PackageProgramOperationV1, PackageProgramOutputSlotIdV1,
    PackageProgramOwnerV1, PackageProgramPaintIdV1, PackageProgramScenarioV1,
    PackageProgramSessionV1, PackageProgramSourceIdV1, PackageProgramStateKindV1,
    PackageProgramStateViewV1, PackageProgramSurfaceIdV1, PackageProgramSurfaceInputPortIdV1,
    PackageProgramSurroundV1, PackageProgramTargetCandidateIdV1, PackageProgramTargetCandidateV1,
    PackageProgramTargetIdV1, PackageProgramUpdateErrorKindV1, PackageProgramUpdateV1,
};
use labcolors_core::wcag22::Wcag22CriterionV1;

fn exact_size<I: ExactSizeIterator>(iterator: I) -> I {
    iterator
}

fn compile_error(draft: PackageProgramDraftV1) -> PackageProgramCompileErrorV1 {
    match draft.compile() {
        Ok(_) => panic!("the invalid authored program must not compile"),
        Err(error) => error,
    }
}

#[allow(dead_code)]
fn wasm_can_use_only_the_concrete_owner_and_session(
    owner: &PackageProgramOwnerV1,
    session: &mut PackageProgramSessionV1,
    scenarios: &[PackageProgramScenarioV1<'_>],
) -> Result<(), PackageProgramInstantiateErrorV1> {
    let _independent_session = owner.instantiate(0xA11CE)?;
    let update = PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios,
    };
    let view = session.update(update).expect("well-formed update");
    assert_projection_is_linear(view);
    Ok(())
}

fn assert_projection_is_linear(view: PackageProgramStateViewV1<'_>) {
    let _kind: PackageProgramStateKindV1 = view.kind();
    let _revision: Option<u64> = view.revision();
    let certificates = exact_size(view.certificates());
    let certificate_count = certificates.len();
    for certificate in certificates {
        let _: PackageProgramCertificateV1<'_> = certificate;
    }
    for operation in exact_size(view.operations()) {
        match operation {
            PackageProgramOperationV1::Set {
                output_slot,
                source,
                opacity,
                certificate_index,
            } => {
                let _: PackageProgramOutputSlotIdV1 = output_slot;
                let _: Srgb8 = source;
                assert!(opacity.is_finite() && (0.0..=1.0).contains(&opacity));
                assert!(certificate_index < certificate_count);
            }
            PackageProgramOperationV1::Remove { output_slot } => {
                let _: PackageProgramOutputSlotIdV1 = output_slot;
            }
            PackageProgramOperationV1::Hold {
                output_slot,
                certificate_index,
            } => {
                let _: PackageProgramOutputSlotIdV1 = output_slot;
                assert!(certificate_index < certificate_count);
            }
        }
    }
}

#[allow(dead_code)]
fn unknown_is_revision_bound_without_a_stream_or_generation_field(
    session: &mut PackageProgramSessionV1,
) {
    let update = PackageProgramUpdateV1::Unknown {
        revision: 2,
        reason_id: 7,
    };
    let _ = session.update(update);
}

#[allow(dead_code)]
fn owner_expiry_is_a_closed_package_error(
    error: labcolors_core::package_bridge::PackageProgramUpdateErrorV1,
) {
    assert_eq!(error.kind(), PackageProgramUpdateErrorKindV1::OwnerExpired);
}

#[test]
fn red_contract_is_linked_by_the_concrete_package_module() {
    // Reaching this test means the external crate compiled without importing
    // Program<E>, evaluator traits, Session<Plan>, or numeric generations.
    assert_eq!(core::mem::size_of::<Srgb8>(), 3);
}

fn fixed_nested_draft(
    opacity_value: f64,
    target_source: PackageProgramSourceIdV1,
    declared_input: PackageProgramSurfaceInputPortIdV1,
    used_input: PackageProgramSurfaceInputPortIdV1,
) -> PackageProgramDraftV1 {
    let source = PackageProgramSourceIdV1::new(1);
    let target = PackageProgramTargetIdV1::new(2);
    let opacity = PackageProgramOpacityInputIdV1::new(3);
    let solid = PackageProgramPaintIdV1::new(4);
    let translucent = PackageProgramPaintIdV1::new(5);
    let input_surface = PackageProgramSurfaceIdV1::new(6);
    let nested_surface = PackageProgramSurfaceIdV1::new(7);
    let first_occurrence = PackageProgramOccurrenceIdV1::new(8);
    let second_occurrence = PackageProgramOccurrenceIdV1::new(9);
    let exact = PackageProgramConstraintIdV1::new(10);
    let wcag = PackageProgramConstraintIdV1::new(11);
    let output = PackageProgramOutputSlotIdV1::new(12);
    let context =
        PackageProgramAppearanceContextV1::try_new(64.0, 0.2, PackageProgramSurroundV1::Dim)
            .unwrap();

    let mut draft = PackageProgramDraftV1::new();
    draft.push_source(source, Srgb8::new([0; 3]));
    draft.push_fixed_target(target, target_source);
    draft.push_surface_input_port(declared_input);
    draft.push_opacity_input(opacity, opacity_value);
    draft.push_solid_paint(solid, target);
    draft.push_opacity_paint(translucent, solid, opacity);
    draft.push_input_surface(input_surface, used_input);
    draft.push_source_over_occurrence(first_occurrence, translucent, input_surface, context);
    draft.push_occurrence_surface(nested_surface, first_occurrence);
    draft.push_source_over_occurrence(second_occurrence, solid, nested_surface, context);
    draft.push_exact_hard(exact, first_occurrence, Srgb8::new([0; 3]));
    draft.push_wcag22_report_only(wcag, second_occurrence, Wcag22CriterionV1::Sc143TextDefault);
    draft.push_output(output, translucent);
    draft
}

fn attach_target_assessment(draft: &mut PackageProgramDraftV1, target: PackageProgramTargetIdV1) {
    let paint = PackageProgramPaintIdV1::new(770);
    let occurrence = PackageProgramOccurrenceIdV1::new(771);
    let constraint = PackageProgramConstraintIdV1::new(772);
    let context =
        PackageProgramAppearanceContextV1::try_new(64.0, 0.2, PackageProgramSurroundV1::Average)
            .unwrap();
    draft.push_solid_paint(paint, target);
    draft.push_source_over_occurrence(
        occurrence,
        paint,
        PackageProgramSurfaceIdV1::new(6),
        context,
    );
    draft.push_exact_report_only(constraint, occurrence, Srgb8::new([0; 3]));
}

#[test]
fn external_authoring_lowers_the_actual_closed_program_and_returns_canonical_input_ports() {
    let source = PackageProgramSourceIdV1::new(91);
    let target = PackageProgramTargetIdV1::new(72);
    let gray = PackageProgramTargetCandidateIdV1::new(8);
    let black = PackageProgramTargetCandidateIdV1::new(3);
    let paint = PackageProgramPaintIdV1::new(54);
    let high_input = PackageProgramSurfaceInputPortIdV1::new(900);
    let low_input = PackageProgramSurfaceInputPortIdV1::new(2);
    let high_surface = PackageProgramSurfaceIdV1::new(401);
    let low_surface = PackageProgramSurfaceIdV1::new(400);
    let high_occurrence = PackageProgramOccurrenceIdV1::new(301);
    let low_occurrence = PackageProgramOccurrenceIdV1::new(300);
    let exact = PackageProgramConstraintIdV1::new(201);
    let high_wcag = PackageProgramConstraintIdV1::new(203);
    let low_wcag = PackageProgramConstraintIdV1::new(202);
    let output = PackageProgramOutputSlotIdV1::new(101);
    let context =
        PackageProgramAppearanceContextV1::try_new(64.0, 0.2, PackageProgramSurroundV1::Average)
            .unwrap();

    let mut draft = PackageProgramDraftV1::new();
    draft.push_source(source, Srgb8::new([0x80; 3]));
    draft.push_finite_target(
        target,
        source,
        vec![
            PackageProgramTargetCandidateV1::new(gray, Srgb8::new([0x80; 3])),
            PackageProgramTargetCandidateV1::new(black, Srgb8::new([0; 3])),
        ],
    );
    draft
        .set_joint_selection(vec![
            PackageProgramJointStateV1::new(vec![PackageProgramJointChoiceV1::new(target, gray)]),
            PackageProgramJointStateV1::new(vec![PackageProgramJointChoiceV1::new(target, black)]),
        ])
        .unwrap();
    // Deliberately reverse numeric order. Core, not the caller, owns the hot
    // scenario schema order returned below.
    draft.push_surface_input_port(high_input);
    draft.push_surface_input_port(low_input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(high_surface, high_input);
    draft.push_input_surface(low_surface, low_input);
    draft.push_source_over_occurrence(high_occurrence, paint, high_surface, context);
    draft.push_source_over_occurrence(low_occurrence, paint, low_surface, context);
    draft.push_exact_report_only(exact, high_occurrence, Srgb8::new([0; 3]));
    draft.push_wcag22_hard(
        high_wcag,
        high_occurrence,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    draft.push_wcag22_hard(
        low_wcag,
        low_occurrence,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    draft.push_output(output, paint);

    let owner = draft.compile().unwrap();
    assert_eq!(
        owner.surface_input_ports().collect::<Vec<_>>(),
        [low_input, high_input]
    );
    let mut session = owner.instantiate(44).unwrap();
    assert_eq!(
        session.surface_input_ports().collect::<Vec<_>>(),
        [low_input, high_input]
    );
    let white = [Srgb8::new([0xFF; 3]), Srgb8::new([0xFF; 3])];
    let scenarios = [PackageProgramScenarioV1::new(7, &white)];
    let ready = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        })
        .unwrap();
    assert_eq!(ready.kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(
        ready.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Set {
            output_slot: output,
            source: Srgb8::new([0; 3]),
            opacity: 1.0,
            certificate_index: 0,
        }]
    );
}

#[test]
fn authored_compile_is_atomic_and_projects_a_closed_error() {
    let source = PackageProgramSourceIdV1::new(1);
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let mut draft = fixed_nested_draft(1.0, source, input, input);
    draft.push_source(source, Srgb8::new([0xFF; 3]));

    let error = match draft.compile() {
        Ok(_) => panic!("duplicate declaration must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::DuplicateSource
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::Source(source))
    );
    assert_eq!(error.related_handle(), None);
}

#[test]
fn every_physical_constructor_and_both_remaining_constraint_modes_execute() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let draft = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    let owner = draft.compile().unwrap();
    assert_eq!(owner.surface_input_ports().collect::<Vec<_>>(), [input]);

    let mut session = owner.instantiate(13).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [PackageProgramScenarioV1::new(1, &white)];
    let state = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        })
        .unwrap();
    assert_eq!(state.kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(
        state.operations().collect::<Vec<_>>(),
        [PackageProgramOperationV1::Set {
            output_slot: PackageProgramOutputSlotIdV1::new(12),
            source: Srgb8::new([0; 3]),
            opacity: 1.0,
            certificate_index: 0,
        }]
    );
}

#[test]
fn invalid_context_and_opacity_are_typed_and_fail_closed() {
    let context_error =
        PackageProgramAppearanceContextV1::try_new(64.0, 1.01, PackageProgramSurroundV1::Dark)
            .unwrap_err();
    assert_eq!(
        context_error.kind(),
        PackageProgramAppearanceContextErrorKindV1::Domain
    );
    assert_eq!(
        context_error.field(),
        Some(PackageProgramAppearanceContextFieldV1::BackgroundLuminanceRatioYbYw)
    );
    assert_eq!(
        context_error.reason(),
        Some(PackageProgramNumericDomainErrorV1::AboveOne)
    );

    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let error = match fixed_nested_draft(f64::NAN, PackageProgramSourceIdV1::new(1), input, input)
        .compile()
    {
        Ok(_) => panic!("non-finite opacity must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::OpacityOutOfDomain
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::OpacityInput(
            PackageProgramOpacityInputIdV1::new(3)
        ))
    );
    assert_eq!(error.related_handle(), None);
}

#[test]
fn relational_compile_errors_keep_both_typed_handles() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let missing_source = PackageProgramSourceIdV1::new(99);
    let error = match fixed_nested_draft(1.0, missing_source, input, input).compile() {
        Ok(_) => panic!("a target cannot reference an undeclared source"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::MissingTargetSource
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::Target(
            PackageProgramTargetIdV1::new(2)
        ))
    );
    assert_eq!(
        error.related_handle(),
        Some(PackageProgramCompileErrorHandleV1::Source(missing_source))
    );
}

#[test]
fn declared_and_referenced_surface_inputs_cannot_drift() {
    let declared = PackageProgramSurfaceInputPortIdV1::new(50);
    let missing = PackageProgramSurfaceInputPortIdV1::new(51);
    let error = match fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), declared, missing)
        .compile()
    {
        Ok(_) => panic!("an undeclared physical input must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::MissingSurfaceInputPort
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::Surface(
            PackageProgramSurfaceIdV1::new(6)
        ))
    );
    assert_eq!(
        error.related_handle(),
        Some(PackageProgramCompileErrorHandleV1::SurfaceInputPort(
            missing
        ))
    );
}

#[test]
fn declared_input_ports_form_an_exact_bijection_with_input_surfaces() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let extra = PackageProgramSurfaceInputPortIdV1::new(51);
    let mut unused = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    unused.push_surface_input_port(extra);
    let error = match unused.compile() {
        Ok(_) => panic!("an unused declared input must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::UnusedSurfaceInputPort
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::SurfaceInputPort(extra))
    );

    let duplicate_surface = PackageProgramSurfaceIdV1::new(60);
    let mut duplicate = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    duplicate.push_input_surface(duplicate_surface, input);
    let error = match duplicate.compile() {
        Ok(_) => panic!("two input surfaces must not bind one declared port"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramCompileErrorKindV1::DuplicateSurfaceInputBinding
    );
    assert_eq!(
        error.primary_handle(),
        Some(PackageProgramCompileErrorHandleV1::SurfaceInputPort(input))
    );
    assert_eq!(
        error.related_handle(),
        Some(PackageProgramCompileErrorHandleV1::Surface(
            duplicate_surface
        ))
    );
    assert_eq!(
        error,
        PackageProgramCompileErrorV1::DuplicateSurfaceInputBinding {
            input,
            first: PackageProgramSurfaceIdV1::new(6),
            duplicate: duplicate_surface,
        }
    );
}

#[test]
fn duplicate_candidate_signal_preserves_both_candidates_and_exact_stimulus() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let target = PackageProgramTargetIdV1::new(70);
    let first = PackageProgramTargetCandidateIdV1::new(701);
    let duplicate = PackageProgramTargetCandidateIdV1::new(702);
    let encoded_srgb8 = Srgb8::new([17, 33, 65]);
    let mut draft = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    draft.push_finite_target(
        target,
        PackageProgramSourceIdV1::new(1),
        vec![
            PackageProgramTargetCandidateV1::new(first, encoded_srgb8),
            PackageProgramTargetCandidateV1::new(duplicate, encoded_srgb8),
        ],
    );
    attach_target_assessment(&mut draft, target);

    assert_eq!(
        compile_error(draft),
        PackageProgramCompileErrorV1::DuplicateTargetCandidateSignal {
            target,
            first,
            duplicate,
            encoded_srgb8,
        }
    );
}

#[test]
fn joint_diagnostics_preserve_state_and_total_order_details() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let target = PackageProgramTargetIdV1::new(70);
    let first = PackageProgramTargetCandidateIdV1::new(701);
    let second = PackageProgramTargetCandidateIdV1::new(702);
    let candidates = vec![
        PackageProgramTargetCandidateV1::new(first, Srgb8::new([0; 3])),
        PackageProgramTargetCandidateV1::new(second, Srgb8::new([255; 3])),
    ];

    let mut duplicate_target =
        fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    duplicate_target.push_finite_target(
        target,
        PackageProgramSourceIdV1::new(1),
        candidates.clone(),
    );
    attach_target_assessment(&mut duplicate_target, target);
    duplicate_target
        .set_joint_selection(vec![PackageProgramJointStateV1::new(vec![
            PackageProgramJointChoiceV1::new(target, first),
            PackageProgramJointChoiceV1::new(target, second),
        ])])
        .unwrap();
    assert_eq!(
        compile_error(duplicate_target),
        PackageProgramCompileErrorV1::JointStateDuplicateTarget { state: 0, target }
    );

    let mut incomplete = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    incomplete.push_finite_target(target, PackageProgramSourceIdV1::new(1), candidates);
    attach_target_assessment(&mut incomplete, target);
    incomplete
        .set_joint_selection(vec![PackageProgramJointStateV1::new(vec![
            PackageProgramJointChoiceV1::new(target, first),
        ])])
        .unwrap();
    assert_eq!(
        compile_error(incomplete),
        PackageProgramCompileErrorV1::InvalidJointOrder(
            PackageProgramJointOrderErrorV1::IncompleteOrder {
                expected: 2,
                actual: 1,
            }
        )
    );
}

#[test]
fn dependency_cycles_retain_all_typed_core_members_without_reallocation() {
    let input = PackageProgramSurfaceInputPortIdV1::new(50);
    let first_paint = PackageProgramPaintIdV1::new(70);
    let second_paint = PackageProgramPaintIdV1::new(71);
    let mut paint_cycle = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    paint_cycle.push_opacity_paint(
        first_paint,
        second_paint,
        PackageProgramOpacityInputIdV1::new(3),
    );
    paint_cycle.push_opacity_paint(
        second_paint,
        first_paint,
        PackageProgramOpacityInputIdV1::new(3),
    );
    let error = compile_error(paint_cycle);
    let PackageProgramCompileErrorV1::PaintCycle(cycle) = error else {
        panic!("expected an exact paint cycle")
    };
    assert_eq!(
        cycle.paints().collect::<Vec<_>>(),
        [first_paint, second_paint]
    );

    let cyclic_surface = PackageProgramSurfaceIdV1::new(80);
    let cyclic_occurrence = PackageProgramOccurrenceIdV1::new(81);
    let context =
        PackageProgramAppearanceContextV1::try_new(64.0, 0.2, PackageProgramSurroundV1::Average)
            .unwrap();
    let mut render_cycle = fixed_nested_draft(1.0, PackageProgramSourceIdV1::new(1), input, input);
    render_cycle.push_occurrence_surface(cyclic_surface, cyclic_occurrence);
    render_cycle.push_source_over_occurrence(
        cyclic_occurrence,
        PackageProgramPaintIdV1::new(4),
        cyclic_surface,
        context,
    );
    let error = compile_error(render_cycle);
    let PackageProgramCompileErrorV1::RenderCycle(cycle) = error else {
        panic!("expected an exact render cycle")
    };
    assert_eq!(cycle.surfaces().collect::<Vec<_>>(), [cyclic_surface]);
    assert_eq!(cycle.occurrences().collect::<Vec<_>>(), [cyclic_occurrence]);
}

#[test]
fn the_code_owned_observation_group_reports_package_authored_port_semantics() {
    assert_eq!(
        compile_error(PackageProgramDraftV1::new()),
        PackageProgramCompileErrorV1::EmptySurfaceInputPortSet
    );
}

#[test]
fn singleton_joint_order_cannot_be_silently_replaced() {
    let mut draft = PackageProgramDraftV1::new();
    draft.set_joint_selection(vec![]).unwrap();
    let error = match draft.set_joint_selection(vec![]) {
        Ok(_) => panic!("a singleton declaration must not be replaced"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        PackageProgramDraftErrorV1::JointSelectionAlreadyDeclared
    );
}
