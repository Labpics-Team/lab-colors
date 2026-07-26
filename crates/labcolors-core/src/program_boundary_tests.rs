//! Internal compile-and-runtime contract for the staged concrete Core Program seam.
//!
//! These tests deliberately exercise only the closed concrete boundary types;
//! the module stays crate-private until the terminal public cut is complete.

use core::iter::FusedIterator;

use crate::Srgb8;
use crate::program::{
    AppearanceContextErrorKindV1, AppearanceContextFieldV1, AppearanceContextV1, AssessmentV1,
    CertificateV1, CompileErrorHandleV1, CompileErrorKindV1, CompileErrorV1, ConstraintIdV1,
    DraftErrorV1, DraftV1, EvidenceBoundsErrorV1, InstantiateErrorV1, JointChoiceV1,
    JointOrderErrorV1, JointStateV1, NumericDomainErrorV1, ObservationHeadV1, OccurrenceIdV1,
    OpacityInputIdV1, OperationV1, OutputSlotIdV1, OwnerV1, PaintIdV1, PhysicalPointV1,
    ProjectionV1, ScenarioV1, SessionV1, SignalV1, SourceIdV1, StateKindV1, SurfaceIdV1,
    SurfaceInputPortIdV1, SurroundV1, TargetCandidateIdV1, TargetCandidateV1, TargetIdV1,
    UpdateErrorKindV1, UpdateErrorV1, UpdateV1, VerdictV1,
};
use crate::wcag22::Wcag22CriterionV1;

fn exact_size<I: ExactSizeIterator + FusedIterator>(iterator: I) -> I {
    iterator
}

fn compile_error(draft: DraftV1) -> CompileErrorV1 {
    match draft.compile() {
        Ok(_) => panic!("the invalid authored program must not compile"),
        Err(error) => error,
    }
}

#[allow(dead_code)]
fn wasm_can_use_only_the_concrete_owner_and_session(
    owner: &OwnerV1,
    session: &mut SessionV1,
    scenarios: &[ScenarioV1<'_>],
) -> Result<(), InstantiateErrorV1> {
    let _independent_session = owner.instantiate(0xA11CE)?;
    let update = UpdateV1::Observed {
        revision: 1,
        scenarios,
    };
    let view = owner
        .update(session, update)
        .expect("well-formed owner-bound update");
    assert_projection_is_owner_bound(view);
    Ok(())
}

fn assert_projection_is_owner_bound(projection: ProjectionV1<'_, '_>) {
    let view = projection.evidence();
    let _kind: StateKindV1 = view.kind();
    match view.observation_head() {
        ObservationHeadV1::Empty => {}
        ObservationHeadV1::Unknown {
            stream,
            revision,
            reason_id,
        } => {
            let _ = stream.value();
            let _: u64 = revision;
            let _: u32 = reason_id;
        }
        ObservationHeadV1::Observed { stream, revision } => {
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
            let _: SurfaceInputPortIdV1 = port;
        }
        for case in exact_size(observation.physical_cases()) {
            for value in exact_size(case.values()) {
                let SignalV1::Iec61966Srgb8D65(value) = value;
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
                let _: VerdictV1 = assessment.verdict();
                match assessment {
                    AssessmentV1::ExactSrgb8(evidence) => {
                        let _: Srgb8 = evidence.expected();
                    }
                    AssessmentV1::Wcag22Srgb8(evidence) => {
                        let _ = evidence.profile_id();
                        let _ = evidence.criterion();
                        let _ = evidence.foreground_luminance();
                        let _ = evidence.background_luminance();
                        let _ = evidence.numerical_evidence();
                    }
                }
                let binding = assessment.binding();
                let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = binding.physical();
                let _ = physical.subject_paint().value();
                let _ = physical.backdrop_surface().value();
                let _: Srgb8 = physical.subject();
                let _ = physical.opacity();
                let _: Srgb8 = physical.backdrop();
                let _: Srgb8 = physical.visible();
                let context = binding.appearance_context();
                let _ = context.adapting_luminance_cd_m2();
                let _ = context.background_luminance_ratio_yb_yw();
                let _ = context.surround();
            }};
        }
        match certificate {
            CertificateV1::Verified(verified) => {
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
            CertificateV1::Conflict(conflict) => {
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
            OperationV1::Set(set) => {
                let _: OutputSlotIdV1 = set.output_slot();
                let _: Srgb8 = set.source();
                assert!(set.opacity().is_finite() && (0.0..=1.0).contains(&set.opacity()));
                let _ = set.certificate().content_identity();
            }
            OperationV1::Remove(remove) => {
                let _: OutputSlotIdV1 = remove.output_slot();
            }
        }
    }
    let _ = certificate_count;
}

#[allow(dead_code)]
fn unknown_is_revision_bound_without_a_stream_or_generation_field(
    owner: &OwnerV1,
    session: &mut SessionV1,
) {
    let update = UpdateV1::Unknown {
        revision: 2,
        reason_id: 7,
    };
    let _ = owner.update(session, update);
}

#[allow(dead_code)]
fn owner_mismatch_is_a_closed_boundary_error(error: UpdateErrorV1) {
    assert_eq!(error.kind(), UpdateErrorKindV1::OwnerMismatch);
}

#[test]
fn staged_boundary_uses_only_closed_concrete_types() {
    // Reaching this test means the concrete seam compiled without importing
    // Program<E>, evaluator traits, Session<Plan>, or numeric generations.
    assert_eq!(core::mem::size_of::<Srgb8>(), 3);
}

fn fixed_nested_draft(
    opacity_value: f64,
    target_source: SourceIdV1,
    declared_input: SurfaceInputPortIdV1,
    used_input: SurfaceInputPortIdV1,
) -> DraftV1 {
    let source = SourceIdV1::new(1);
    let target = TargetIdV1::new(2);
    let opacity = OpacityInputIdV1::new(3);
    let solid = PaintIdV1::new(4);
    let translucent = PaintIdV1::new(5);
    let input_surface = SurfaceIdV1::new(6);
    let nested_surface = SurfaceIdV1::new(7);
    let first_occurrence = OccurrenceIdV1::new(8);
    let second_occurrence = OccurrenceIdV1::new(9);
    let exact = ConstraintIdV1::new(10);
    let wcag = ConstraintIdV1::new(11);
    let output = OutputSlotIdV1::new(12);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim).unwrap();

    let mut draft = DraftV1::new();
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

fn attach_target_assessment(draft: &mut DraftV1, target: TargetIdV1) {
    let paint = PaintIdV1::new(770);
    let occurrence = OccurrenceIdV1::new(771);
    let constraint = ConstraintIdV1::new(772);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();
    draft.push_solid_paint(paint, target);
    draft.push_source_over_occurrence(occurrence, paint, SurfaceIdV1::new(6), context);
    draft.push_exact_report_only(constraint, occurrence, Srgb8::new([0; 3]));
}

fn joint_draft(hard: bool) -> DraftV1 {
    let source = SourceIdV1::new(1);
    let target = TargetIdV1::new(2);
    let black = TargetCandidateIdV1::new(3);
    let white = TargetCandidateIdV1::new(4);
    let input = SurfaceInputPortIdV1::new(5);
    let paint = PaintIdV1::new(6);
    let surface = SurfaceIdV1::new(7);
    let occurrence = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let mut draft = DraftV1::new();
    draft.push_source(source, Srgb8::new([0; 3]));
    draft.push_finite_target(
        target,
        source,
        vec![
            TargetCandidateV1::new(black, Srgb8::new([0; 3])),
            TargetCandidateV1::new(white, Srgb8::new([255; 3])),
        ],
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(target, black)]),
            JointStateV1::new(vec![JointChoiceV1::new(target, white)]),
        ])
        .unwrap();
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    if hard {
        draft.push_exact_hard(constraint, occurrence, Srgb8::new([128; 3]));
    } else {
        draft.push_exact_report_only(constraint, occurrence, Srgb8::new([0; 3]));
    }
    draft.push_output(output, paint);
    draft
}

#[test]
fn evidence_cell_bounds_cover_fixed_and_joint_evaluation_laws() {
    let input = SurfaceInputPortIdV1::new(50);
    let fixed = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();

    let empty = fixed.evidence_cell_bounds(0).unwrap();
    assert_eq!(empty.verified_cells(), 0);
    assert_eq!(empty.conflict_cells(), 0);

    // The second fixed constraint is report-only. Counting only hard
    // constraints would under-reserve a successful certificate.
    let fixed_bounds = fixed.evidence_cell_bounds(3).unwrap();
    assert_eq!(fixed_bounds.verified_cells(), 6);
    assert_eq!(fixed_bounds.conflict_cells(), 6);

    let report_only_joint = joint_draft(false).compile().unwrap();
    let report_only_bounds = report_only_joint.evidence_cell_bounds(4).unwrap();
    assert_eq!(report_only_bounds.verified_cells(), 4);
    assert_eq!(report_only_bounds.conflict_cells(), 0);

    let hard_joint = joint_draft(true).compile().unwrap();
    let hard_bounds = hard_joint.evidence_cell_bounds(4).unwrap();
    assert_eq!(hard_bounds.verified_cells(), 4);
    assert_eq!(hard_bounds.conflict_cells(), 8);
}

#[test]
fn evidence_cell_bounds_report_both_checked_multiplication_overflows() {
    let input = SurfaceInputPortIdV1::new(50);
    let two_constraints = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    assert!(matches!(
        two_constraints.evidence_cell_bounds(usize::MAX),
        Err(EvidenceBoundsErrorV1::CardinalityOverflow)
    ));

    // One constraint keeps the first product representable; the two-state
    // joint order forces the independent exhaustive-conflict product to fail.
    let two_states = joint_draft(true).compile().unwrap();
    assert!(matches!(
        two_states.evidence_cell_bounds(usize::MAX),
        Err(EvidenceBoundsErrorV1::CardinalityOverflow)
    ));
}

#[test]
fn evidence_cell_bounds_cover_actual_joint_conflict_and_duplicate_case_reduction() {
    let joint = joint_draft(true).compile().unwrap();
    let mut joint_session = joint.instantiate(21).unwrap();
    let red = [Srgb8::new([255, 0, 0])];
    let blue = [Srgb8::new([0, 0, 255])];
    let unique_scenarios = [ScenarioV1::new(1, &red), ScenarioV1::new(2, &blue)];
    let joint_bounds = joint.evidence_cell_bounds(unique_scenarios.len()).unwrap();
    let projection = joint
        .update(
            &mut joint_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &unique_scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Conflict(certificate)) = projection.evidence().certificates().next()
    else {
        panic!("both authored joint states must violate the hard exact constraint");
    };
    assert_eq!(certificate.cells().len(), joint_bounds.conflict_cells());

    let input = SurfaceInputPortIdV1::new(50);
    let fixed = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut fixed_session = fixed.instantiate(22).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let duplicate_scenarios = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &white)];
    let fixed_bounds = fixed
        .evidence_cell_bounds(duplicate_scenarios.len())
        .unwrap();
    let projection = fixed
        .update(
            &mut fixed_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &duplicate_scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.evidence().certificates().next()
    else {
        panic!("duplicate physical scenarios must preserve a valid certificate");
    };
    assert!(certificate.cells().len() < fixed_bounds.verified_cells());
}

#[test]
fn evidence_cell_bounds_query_is_pure_across_session_updates() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let expected = owner.evidence_cell_bounds(1).unwrap();
    assert_eq!(expected.verified_cells(), 2);
    assert_eq!(expected.conflict_cells(), 2);

    let mut session = owner.instantiate(13).unwrap();
    let after_instantiation = owner.evidence_cell_bounds(1).unwrap();
    assert_eq!(
        after_instantiation.verified_cells(),
        expected.verified_cells()
    );
    assert_eq!(
        after_instantiation.conflict_cells(),
        expected.conflict_cells()
    );

    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    {
        let projection = owner
            .update(
                &mut session,
                UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap();
        let Some(CertificateV1::Verified(certificate)) =
            projection.evidence().certificates().next()
        else {
            panic!("the fixed admissible program must produce Verified evidence");
        };
        assert_eq!(certificate.cells().len(), expected.verified_cells());
    }

    let after_update = owner.evidence_cell_bounds(1).unwrap();
    assert_eq!(after_update.verified_cells(), expected.verified_cells());
    assert_eq!(after_update.conflict_cells(), expected.conflict_cells());
    let projection = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(projection.evidence().kind(), StateKindV1::Ready);
}

#[test]
fn staged_authoring_lowers_the_actual_closed_program_and_returns_canonical_input_ports() {
    let source = SourceIdV1::new(91);
    let target = TargetIdV1::new(72);
    let gray = TargetCandidateIdV1::new(8);
    let black = TargetCandidateIdV1::new(3);
    let paint = PaintIdV1::new(54);
    let high_input = SurfaceInputPortIdV1::new(900);
    let low_input = SurfaceInputPortIdV1::new(2);
    let high_surface = SurfaceIdV1::new(401);
    let low_surface = SurfaceIdV1::new(400);
    let high_occurrence = OccurrenceIdV1::new(301);
    let low_occurrence = OccurrenceIdV1::new(300);
    let exact = ConstraintIdV1::new(201);
    let high_wcag = ConstraintIdV1::new(203);
    let low_wcag = ConstraintIdV1::new(202);
    let output = OutputSlotIdV1::new(101);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let mut draft = DraftV1::new();
    draft.push_source(source, Srgb8::new([0x80; 3]));
    draft.push_finite_target(
        target,
        source,
        vec![
            TargetCandidateV1::new(gray, Srgb8::new([0x80; 3])),
            TargetCandidateV1::new(black, Srgb8::new([0; 3])),
        ],
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(target, gray)]),
            JointStateV1::new(vec![JointChoiceV1::new(target, black)]),
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
    let scenarios = [ScenarioV1::new(7, &white)];
    let ready = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(ready.evidence().kind(), StateKindV1::Ready);
    let mut operations = ready.operations();
    let Some(OperationV1::Set(set)) = operations.next() else {
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
    let source = SourceIdV1::new(1);
    let input = SurfaceInputPortIdV1::new(50);
    let mut draft = fixed_nested_draft(1.0, source, input, input);
    draft.push_source(source, Srgb8::new([0xFF; 3]));

    let error = match draft.compile() {
        Ok(_) => panic!("duplicate declaration must not compile"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), CompileErrorKindV1::DuplicateSource);
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::Source(source))
    );
    assert_eq!(error.related_handle(), None);
}

#[test]
fn every_physical_constructor_and_both_remaining_constraint_modes_execute() {
    let input = SurfaceInputPortIdV1::new(50);
    let draft = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    let owner = draft.compile().unwrap();
    assert_eq!(owner.surface_input_ports().collect::<Vec<_>>(), [input]);

    let mut session = owner.instantiate(13).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    let state = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(state.evidence().kind(), StateKindV1::Ready);
    let Some(CertificateV1::Verified(certificate)) = state.evidence().certificates().next() else {
        panic!("a fixed target must produce one Verified certificate");
    };
    assert_eq!(certificate.selected_state_index(), None);
    let mut operations = state.operations();
    let Some(OperationV1::Set(set)) = operations.next() else {
        panic!("Ready must emit one Set operation");
    };
    assert_eq!(set.output_slot(), OutputSlotIdV1::new(12));
    assert_eq!(set.source(), Srgb8::new([0; 3]));
    assert_eq!(set.opacity(), 1.0);
    assert_eq!(set.certificate().observation().revision(), 1);
    assert!(operations.next().is_none());
}

#[test]
fn observed_violation_removes_outputs_but_retains_previous_certificate_as_evidence() {
    let input = SurfaceInputPortIdV1::new(50);
    let output = OutputSlotIdV1::new(12);
    let owner = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut session = owner.instantiate(13).unwrap();

    let black = [Srgb8::new([0; 3])];
    let black_scenarios = [ScenarioV1::new(1, &black)];
    let ready = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &black_scenarios,
            },
        )
        .unwrap();
    assert!(matches!(
        ready.operations().next(),
        Some(OperationV1::Set(_))
    ));

    let white = [Srgb8::new([0xFF; 3])];
    let white_scenarios = [ScenarioV1::new(2, &white)];
    let failed = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &white_scenarios,
            },
        )
        .unwrap();
    assert_eq!(failed.evidence().kind(), StateKindV1::Failed);
    let certificates = failed.evidence().certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 2);
    assert!(matches!(certificates[0], CertificateV1::Conflict(_)));
    let CertificateV1::Verified(previous) = certificates[1] else {
        panic!("the previous certificate must remain available as diagnostics");
    };
    assert_eq!(previous.observation().revision(), 1);

    let mut operations = failed.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("a known violation of the current context must remove the old output");
    };
    assert_eq!(remove.output_slot(), output);
    assert!(operations.next().is_none());
}

#[test]
fn unknown_context_removes_outputs_but_retains_previous_certificate_as_evidence() {
    let input = SurfaceInputPortIdV1::new(50);
    let output = OutputSlotIdV1::new(12);
    let owner = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut session = owner.instantiate(13).unwrap();

    let black = [Srgb8::new([0; 3])];
    let black_scenarios = [ScenarioV1::new(1, &black)];
    let ready = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &black_scenarios,
            },
        )
        .unwrap();
    assert!(matches!(
        ready.operations().next(),
        Some(OperationV1::Set(_))
    ));

    let stale = owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 9,
            },
        )
        .unwrap();
    assert_eq!(stale.evidence().kind(), StateKindV1::Stale);
    let certificates = stale.evidence().certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    let CertificateV1::Verified(previous) = certificates[0] else {
        panic!("the previous certificate must remain available as diagnostics");
    };
    assert_eq!(previous.observation().revision(), 1);
    assert!(matches!(
        stale.evidence().observation_head(),
        ObservationHeadV1::Unknown { revision: 2, .. }
    ));

    let mut operations = stale.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("unknown current context cannot authorize the old output");
    };
    assert_eq!(remove.output_slot(), output);
    assert!(operations.next().is_none());
}

#[test]
fn owner_and_update_errors_preserve_content_and_input_identity() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let owner_identity = owner.content_identity();
    let mut session = owner.instantiate(13).unwrap();

    let no_scenarios = [];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &no_scenarios,
        },
    ) {
        Ok(_) => panic!("an empty physical domain must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::InvalidObservation);
    assert_eq!(error, UpdateErrorV1::EmptyScenarioSet);

    let white = [Srgb8::new([0xFF; 3])];
    let duplicate = [ScenarioV1::new(70, &white), ScenarioV1::new(70, &white)];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &duplicate,
        },
    ) {
        Ok(_) => panic!("duplicate scenario provenance must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::InvalidObservation);
    let UpdateErrorV1::DuplicateScenarioId { scenario } = error else {
        panic!("duplicate provenance must retain its scenario ID");
    };
    assert_eq!(scenario.value(), 70);

    let no_values = [];
    let malformed = [ScenarioV1::new(71, &no_values)];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &malformed,
        },
    ) {
        Ok(_) => panic!("schema-short boundary input must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::InvalidObservation);
    let UpdateErrorV1::ScenarioValueCountMismatch {
        scenario,
        expected,
        actual,
    } = error
    else {
        panic!("schema mismatch must retain its exact scenario and arity");
    };
    assert_eq!(scenario.value(), 71);
    assert_eq!(expected, 1);
    assert_eq!(actual, 0);
    assert_eq!(session.evidence().kind(), StateKindV1::Waiting);
    assert_eq!(
        session.evidence().observation_head(),
        ObservationHeadV1::Empty
    );

    let valid = [ScenarioV1::new(72, &white)];
    let projection = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &valid,
            },
        )
        .unwrap();
    let certificate = projection.evidence().certificates().next().unwrap();
    assert_eq!(owner_identity, certificate.content_identity());

    let error = match owner.update(
        &mut session,
        UpdateV1::Unknown {
            revision: 1,
            reason_id: 9,
        },
    ) {
        Ok(_) => panic!("older revision must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::RevisionOutOfOrder);
    assert_eq!(
        error,
        UpdateErrorV1::RevisionOutOfOrder {
            current: 2,
            incoming: 1,
        }
    );

    let error = match owner.update(
        &mut session,
        UpdateV1::Unknown {
            revision: 2,
            reason_id: 9,
        },
    ) {
        Ok(_) => panic!("changed payload at the same revision must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::RevisionConflict);
    assert_eq!(error, UpdateErrorV1::RevisionConflict { revision: 2 });
    assert_eq!(session.evidence().kind(), StateKindV1::Ready);
    let ObservationHeadV1::Observed { stream, revision } = session.evidence().observation_head()
    else {
        panic!("failed update must keep the last admitted observation");
    };
    assert_eq!(stream.value(), 13);
    assert_eq!(revision, 2);

    let foreign = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    assert_eq!(foreign.content_identity(), owner.content_identity());
    let error = match foreign.update(
        &mut session,
        UpdateV1::Unknown {
            revision: 3,
            reason_id: 9,
        },
    ) {
        Ok(_) => panic!("equal content must not confer owner authority"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::OwnerMismatch);
    assert_eq!(error, UpdateErrorV1::OwnerMismatch);
}

#[test]
fn certificate_and_set_retain_the_same_nonunit_opacity() {
    let source = SourceIdV1::new(1);
    let target = TargetIdV1::new(2);
    let opacity = OpacityInputIdV1::new(3);
    let solid = PaintIdV1::new(4);
    let translucent = PaintIdV1::new(5);
    let input = SurfaceInputPortIdV1::new(6);
    let surface = SurfaceIdV1::new(7);
    let occurrence = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();
    let mut draft = DraftV1::new();
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
    let scenarios = [ScenarioV1::new(1, &white)];
    let state = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = state.evidence().certificates().next() else {
        panic!("the exact emitted midpoint must be verified");
    };
    let AssessmentV1::ExactSrgb8(assessment) = certificate.cells().next().unwrap().assessment()
    else {
        panic!("the authored exact constraint must retain Exact evidence");
    };
    let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = assessment.binding().physical();
    assert_eq!(physical.opacity().to_bits(), 0.5_f64.to_bits());
    assert_eq!(physical.visible(), Srgb8::new([0x80; 3]));
    assert_eq!(
        certificate.outputs().next().unwrap().opacity().to_bits(),
        physical.opacity().to_bits()
    );

    let Some(OperationV1::Set(set)) = state.operations().next() else {
        panic!("the verified output must emit one Set");
    };
    assert_eq!(set.opacity().to_bits(), physical.opacity().to_bits());
}

#[test]
fn invalid_context_and_opacity_are_typed_and_fail_closed() {
    let context_error = AppearanceContextV1::try_new(64.0, 1.01, SurroundV1::Dark).unwrap_err();
    assert_eq!(context_error.kind(), AppearanceContextErrorKindV1::Domain);
    assert_eq!(
        context_error.field(),
        Some(AppearanceContextFieldV1::BackgroundLuminanceRatioYbYw)
    );
    assert_eq!(context_error.reason(), Some(NumericDomainErrorV1::AboveOne));

    let input = SurfaceInputPortIdV1::new(50);
    let error = match fixed_nested_draft(f64::NAN, SourceIdV1::new(1), input, input).compile() {
        Ok(_) => panic!("non-finite opacity must not compile"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), CompileErrorKindV1::OpacityOutOfDomain);
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::OpacityInput(OpacityInputIdV1::new(3)))
    );
    assert_eq!(error.related_handle(), None);
}

#[test]
fn relational_compile_errors_keep_both_typed_handles() {
    let input = SurfaceInputPortIdV1::new(50);
    let missing_source = SourceIdV1::new(99);
    let error = match fixed_nested_draft(1.0, missing_source, input, input).compile() {
        Ok(_) => panic!("a target cannot reference an undeclared source"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), CompileErrorKindV1::MissingTargetSource);
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::Target(TargetIdV1::new(2)))
    );
    assert_eq!(
        error.related_handle(),
        Some(CompileErrorHandleV1::Source(missing_source))
    );
}

#[test]
fn declared_and_referenced_surface_inputs_cannot_drift() {
    let declared = SurfaceInputPortIdV1::new(50);
    let missing = SurfaceInputPortIdV1::new(51);
    let error = match fixed_nested_draft(1.0, SourceIdV1::new(1), declared, missing).compile() {
        Ok(_) => panic!("an undeclared physical input must not compile"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), CompileErrorKindV1::MissingSurfaceInputPort);
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::Surface(SurfaceIdV1::new(6)))
    );
    assert_eq!(
        error.related_handle(),
        Some(CompileErrorHandleV1::SurfaceInputPort(missing))
    );
}

#[test]
fn declared_input_ports_form_an_exact_bijection_with_input_surfaces() {
    let input = SurfaceInputPortIdV1::new(50);
    let extra = SurfaceInputPortIdV1::new(51);
    let mut unused = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    unused.push_surface_input_port(extra);
    let error = match unused.compile() {
        Ok(_) => panic!("an unused declared input must not compile"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), CompileErrorKindV1::UnusedSurfaceInputPort);
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::SurfaceInputPort(extra))
    );

    let duplicate_surface = SurfaceIdV1::new(60);
    let mut duplicate = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    duplicate.push_input_surface(duplicate_surface, input);
    let error = match duplicate.compile() {
        Ok(_) => panic!("two input surfaces must not bind one declared port"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        CompileErrorKindV1::DuplicateSurfaceInputBinding
    );
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::SurfaceInputPort(input))
    );
    assert_eq!(
        error.related_handle(),
        Some(CompileErrorHandleV1::Surface(duplicate_surface))
    );
    assert_eq!(
        error,
        CompileErrorV1::DuplicateSurfaceInputBinding {
            input,
            first: SurfaceIdV1::new(6),
            duplicate: duplicate_surface,
        }
    );
}

#[test]
fn duplicate_candidate_signal_preserves_both_candidates_and_exact_stimulus() {
    let input = SurfaceInputPortIdV1::new(50);
    let target = TargetIdV1::new(70);
    let first = TargetCandidateIdV1::new(701);
    let duplicate = TargetCandidateIdV1::new(702);
    let encoded_srgb8 = Srgb8::new([17, 33, 65]);
    let mut draft = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    draft.push_finite_target(
        target,
        SourceIdV1::new(1),
        vec![
            TargetCandidateV1::new(first, encoded_srgb8),
            TargetCandidateV1::new(duplicate, encoded_srgb8),
        ],
    );
    attach_target_assessment(&mut draft, target);

    assert_eq!(
        compile_error(draft),
        CompileErrorV1::DuplicateTargetCandidateSignal {
            target,
            first,
            duplicate,
            encoded_srgb8,
        }
    );
}

#[test]
fn joint_diagnostics_preserve_state_and_total_order_details() {
    let input = SurfaceInputPortIdV1::new(50);
    let target = TargetIdV1::new(70);
    let first = TargetCandidateIdV1::new(701);
    let second = TargetCandidateIdV1::new(702);
    let candidates = vec![
        TargetCandidateV1::new(first, Srgb8::new([0; 3])),
        TargetCandidateV1::new(second, Srgb8::new([255; 3])),
    ];

    let mut duplicate_target = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    duplicate_target.push_finite_target(target, SourceIdV1::new(1), candidates.clone());
    attach_target_assessment(&mut duplicate_target, target);
    duplicate_target
        .set_joint_selection(vec![JointStateV1::new(vec![
            JointChoiceV1::new(target, first),
            JointChoiceV1::new(target, second),
        ])])
        .unwrap();
    assert_eq!(
        compile_error(duplicate_target),
        CompileErrorV1::JointStateDuplicateTarget { state: 0, target }
    );

    let mut incomplete = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    incomplete.push_finite_target(target, SourceIdV1::new(1), candidates);
    attach_target_assessment(&mut incomplete, target);
    incomplete
        .set_joint_selection(vec![JointStateV1::new(vec![JointChoiceV1::new(
            target, first,
        )])])
        .unwrap();
    assert_eq!(
        compile_error(incomplete),
        CompileErrorV1::InvalidJointOrder(JointOrderErrorV1::IncompleteOrder {
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn dependency_cycles_retain_all_typed_core_members_without_reallocation() {
    let input = SurfaceInputPortIdV1::new(50);
    let first_paint = PaintIdV1::new(70);
    let second_paint = PaintIdV1::new(71);
    let mut paint_cycle = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    paint_cycle.push_opacity_paint(first_paint, second_paint, OpacityInputIdV1::new(3));
    paint_cycle.push_opacity_paint(second_paint, first_paint, OpacityInputIdV1::new(3));
    let error = compile_error(paint_cycle);
    let CompileErrorV1::PaintCycle(cycle) = error else {
        panic!("expected an exact paint cycle")
    };
    assert_eq!(
        cycle.paints().collect::<Vec<_>>(),
        [first_paint, second_paint]
    );

    let cyclic_surface = SurfaceIdV1::new(80);
    let cyclic_occurrence = OccurrenceIdV1::new(81);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();
    let mut render_cycle = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    render_cycle.push_occurrence_surface(cyclic_surface, cyclic_occurrence);
    render_cycle.push_source_over_occurrence(
        cyclic_occurrence,
        PaintIdV1::new(4),
        cyclic_surface,
        context,
    );
    let error = compile_error(render_cycle);
    let CompileErrorV1::RenderCycle(cycle) = error else {
        panic!("expected an exact render cycle")
    };
    assert_eq!(cycle.surfaces().collect::<Vec<_>>(), [cyclic_surface]);
    assert_eq!(cycle.occurrences().collect::<Vec<_>>(), [cyclic_occurrence]);
}

#[test]
fn the_code_owned_observation_group_reports_authored_port_semantics() {
    assert_eq!(
        compile_error(DraftV1::new()),
        CompileErrorV1::EmptySurfaceInputPortSet
    );
}

#[test]
fn singleton_joint_order_cannot_be_silently_replaced() {
    let mut draft = DraftV1::new();
    draft.set_joint_selection(vec![]).unwrap();
    let error = match draft.set_joint_selection(vec![]) {
        Ok(_) => panic!("a singleton declaration must not be replaced"),
        Err(error) => error,
    };
    assert_eq!(error, DraftErrorV1::JointSelectionAlreadyDeclared);
}
