//! External compile-and-runtime contract for the sole concrete Core Program seam.
//!
//! This integration crate deliberately has no access to Core-private generic
//! evaluator/session machinery. Every reachable path uses only the closed
//! concrete boundary types.

use core::iter::FusedIterator;

use labcolors_core::Srgb8;
use labcolors_core::package_bridge::{
    PackageProgramAppearanceContextErrorKindV1, PackageProgramAppearanceContextFieldV1,
    PackageProgramAppearanceContextV1, PackageProgramAssessmentV1, PackageProgramCertificateV1,
    PackageProgramCompileErrorHandleV1, PackageProgramCompileErrorKindV1,
    PackageProgramCompileErrorV1, PackageProgramConstraintIdV1, PackageProgramDraftErrorV1,
    PackageProgramDraftV1, PackageProgramInstantiateErrorV1, PackageProgramJointChoiceV1,
    PackageProgramJointOrderErrorV1, PackageProgramJointStateV1, PackageProgramModeledPointV1,
    PackageProgramNumericDomainErrorV1, PackageProgramObservationHeadV1,
    PackageProgramOccurrenceIdV1, PackageProgramOpacityInputIdV1, PackageProgramOperationV1,
    PackageProgramOutputSlotIdV1, PackageProgramOwnerV1, PackageProgramPaintIdV1,
    PackageProgramPhysicalPointV1, PackageProgramProjectionV1, PackageProgramScenarioV1,
    PackageProgramSessionV1, PackageProgramSignalV1, PackageProgramSourceIdV1,
    PackageProgramStateKindV1, PackageProgramSurfaceIdV1, PackageProgramSurfaceInputPortIdV1,
    PackageProgramSurroundV1, PackageProgramTargetCandidateIdV1, PackageProgramTargetCandidateV1,
    PackageProgramTargetIdV1, PackageProgramUpdateErrorKindV1, PackageProgramUpdateV1,
    PackageProgramVerdictV1,
};
use labcolors_core::wcag22::Wcag22CriterionV1;

fn exact_size<I: ExactSizeIterator + FusedIterator>(iterator: I) -> I {
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
    let view = owner
        .update(session, update)
        .expect("well-formed owner-bound update");
    assert_projection_is_owner_bound(view);
    Ok(())
}

fn assert_projection_is_owner_bound(projection: PackageProgramProjectionV1<'_, '_>) {
    let view = projection.evidence();
    let _kind: PackageProgramStateKindV1 = view.kind();
    match view.observation_head() {
        PackageProgramObservationHeadV1::Empty => {}
        PackageProgramObservationHeadV1::Unknown {
            stream,
            revision,
            reason_id,
        } => {
            let _ = stream.value();
            let _: u64 = revision;
            let _: u32 = reason_id;
        }
        PackageProgramObservationHeadV1::Observed { stream, revision } => {
            let _ = stream.value();
            let _: u64 = revision;
        }
    }
    let certificates = exact_size(view.certificates());
    let certificate_count = certificates.len();
    for certificate in certificates {
        let _: &[u8; 32] = certificate.content_identity().as_bytes();
        let observation = certificate.observation();
        let _stream_id = observation.stream().value();
        let _revision = observation.revision();
        for port in exact_size(observation.surface_input_ports()) {
            let _: PackageProgramSurfaceInputPortIdV1 = port;
        }
        for case in exact_size(observation.physical_cases()) {
            for value in exact_size(case.values()) {
                let PackageProgramSignalV1::Iec61966Srgb8D65(value) = value;
                let _: Srgb8 = value;
            }
            for scenario in exact_size(case.provenance()) {
                let _ = scenario.value();
            }
        }
        macro_rules! inspect_cell {
            ($cell:expr) => {{
                let cell = $cell;
                let _ = cell.case_index();
                let _ = cell.constraint().value();
                let _ = cell.occurrence().value();
                let _ = cell.mode();
                let assessment = cell.assessment();
                let _: PackageProgramVerdictV1 = assessment.verdict();
                match assessment {
                    PackageProgramAssessmentV1::ExactSrgb8(evidence) => {
                        let _: Srgb8 = evidence.expected();
                    }
                    PackageProgramAssessmentV1::Wcag22Srgb8(evidence) => {
                        let _ = evidence.profile_id();
                        let _ = evidence.criterion();
                        let _ = evidence.foreground_luminance();
                        let _ = evidence.background_luminance();
                        let _ = evidence.numerical_evidence();
                    }
                }
                let binding = assessment.binding();
                let PackageProgramPhysicalPointV1::EncodedSrgb8SourceOver(physical) =
                    binding.physical();
                let _ = physical.subject_paint().value();
                let _ = physical.backdrop_surface().value();
                let _: Srgb8 = physical.subject();
                let _ = physical.opacity();
                let _: Srgb8 = physical.backdrop();
                let _: Srgb8 = physical.visible();
                let PackageProgramModeledPointV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1(
                    modeled,
                ) = binding.modeled();
                let _: [f64; 3] = modeled.xyz();
                let context = modeled.appearance_context();
                let _ = context.adapting_luminance_cd_m2();
                let _ = context.background_luminance_ratio_yb_yw();
                let _ = context.surround();
            }};
        }
        match certificate {
            PackageProgramCertificateV1::Verified(verified) => {
                let _ = verified.selected_state_index();
                for cell in exact_size(verified.cells()) {
                    inspect_cell!(cell);
                }
                for output in exact_size(verified.outputs()) {
                    let _ = output.output_slot().value();
                    let _ = output.paint().value();
                    let _: Srgb8 = output.source();
                    let _ = output.opacity();
                }
            }
            PackageProgramCertificateV1::Conflict(conflict) => {
                let _ = conflict.considered_state_count();
                for cell in exact_size(conflict.cells()) {
                    let _ = cell.state_index();
                    inspect_cell!(cell);
                }
            }
        }
    }
    for operation in exact_size(projection.operations()) {
        match operation {
            PackageProgramOperationV1::Set(set) => {
                let _: PackageProgramOutputSlotIdV1 = set.output_slot();
                let _: Srgb8 = set.source();
                assert!(set.opacity().is_finite() && (0.0..=1.0).contains(&set.opacity()));
                let _ = set.certificate().content_identity();
            }
            PackageProgramOperationV1::Remove(remove) => {
                let _: PackageProgramOutputSlotIdV1 = remove.output_slot();
            }
            PackageProgramOperationV1::Hold(hold) => {
                let _: PackageProgramOutputSlotIdV1 = hold.output_slot();
                let _ = hold.certificate().content_identity();
            }
        }
    }
    let _ = certificate_count;
}

#[allow(dead_code)]
fn unknown_is_revision_bound_without_a_stream_or_generation_field(
    owner: &PackageProgramOwnerV1,
    session: &mut PackageProgramSessionV1,
) {
    let update = PackageProgramUpdateV1::Unknown {
        revision: 2,
        reason_id: 7,
    };
    let _ = owner.update(session, update);
}

#[allow(dead_code)]
fn owner_mismatch_is_a_closed_package_error(
    error: labcolors_core::package_bridge::PackageProgramUpdateErrorV1,
) {
    assert_eq!(error.kind(), PackageProgramUpdateErrorKindV1::OwnerMismatch);
}

#[test]
fn external_boundary_uses_only_closed_concrete_types() {
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
    let white = [Srgb8::new([0xFF; 3]), Srgb8::new([0xFF; 3])];
    let scenarios = [PackageProgramScenarioV1::new(7, &white)];
    let ready = owner
        .update(
            &mut session,
            PackageProgramUpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(ready.evidence().kind(), PackageProgramStateKindV1::Ready);
    let mut operations = ready.operations();
    let Some(PackageProgramOperationV1::Set(set)) = operations.next() else {
        panic!("Ready must emit one Set operation");
    };
    assert_eq!(set.output_slot(), output);
    assert_eq!(set.source(), Srgb8::new([0; 3]));
    assert_eq!(set.opacity(), 1.0);
    assert_eq!(set.certificate().observation().revision(), 1);
    assert!(operations.next().is_none());
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
    let state = owner
        .update(
            &mut session,
            PackageProgramUpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(state.evidence().kind(), PackageProgramStateKindV1::Ready);
    let Some(PackageProgramCertificateV1::Verified(certificate)) =
        state.evidence().certificates().next()
    else {
        panic!("a fixed target must produce one Verified certificate");
    };
    assert_eq!(certificate.selected_state_index(), None);
    let mut operations = state.operations();
    let Some(PackageProgramOperationV1::Set(set)) = operations.next() else {
        panic!("Ready must emit one Set operation");
    };
    assert_eq!(set.output_slot(), PackageProgramOutputSlotIdV1::new(12));
    assert_eq!(set.source(), Srgb8::new([0; 3]));
    assert_eq!(set.opacity(), 1.0);
    assert_eq!(set.certificate().observation().revision(), 1);
    assert!(operations.next().is_none());
}

#[test]
fn certificate_and_set_retain_the_same_nonunit_opacity() {
    let source = PackageProgramSourceIdV1::new(1);
    let target = PackageProgramTargetIdV1::new(2);
    let opacity = PackageProgramOpacityInputIdV1::new(3);
    let solid = PackageProgramPaintIdV1::new(4);
    let translucent = PackageProgramPaintIdV1::new(5);
    let input = PackageProgramSurfaceInputPortIdV1::new(6);
    let surface = PackageProgramSurfaceIdV1::new(7);
    let occurrence = PackageProgramOccurrenceIdV1::new(8);
    let constraint = PackageProgramConstraintIdV1::new(9);
    let output = PackageProgramOutputSlotIdV1::new(10);
    let context =
        PackageProgramAppearanceContextV1::try_new(64.0, 0.2, PackageProgramSurroundV1::Average)
            .unwrap();
    let mut draft = PackageProgramDraftV1::new();
    draft.push_source(source, Srgb8::new([0; 3]));
    draft.push_fixed_target(target, source);
    draft.push_surface_input_port(input);
    draft.push_opacity_input(opacity, 0.5);
    draft.push_solid_paint(solid, target);
    draft.push_opacity_paint(translucent, solid, opacity);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, translucent, surface, context);
    draft.push_exact_hard(constraint, occurrence, Srgb8::new([0x80; 3]));
    draft.push_output(output, translucent);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(17).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [PackageProgramScenarioV1::new(1, &white)];
    let state = owner
        .update(
            &mut session,
            PackageProgramUpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(PackageProgramCertificateV1::Verified(certificate)) =
        state.evidence().certificates().next()
    else {
        panic!("the exact emitted midpoint must be verified");
    };
    let PackageProgramAssessmentV1::ExactSrgb8(assessment) =
        certificate.cells().next().unwrap().assessment()
    else {
        panic!("the authored exact constraint must retain Exact evidence");
    };
    let PackageProgramPhysicalPointV1::EncodedSrgb8SourceOver(physical) =
        assessment.binding().physical();
    assert_eq!(physical.opacity().to_bits(), 0.5_f64.to_bits());
    assert_eq!(physical.visible(), Srgb8::new([0x80; 3]));
    assert_eq!(
        certificate.outputs().next().unwrap().opacity().to_bits(),
        physical.opacity().to_bits()
    );

    let Some(PackageProgramOperationV1::Set(set)) = state.operations().next() else {
        panic!("the verified output must emit one Set");
    };
    assert_eq!(set.opacity().to_bits(), physical.opacity().to_bits());
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
