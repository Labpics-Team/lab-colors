//! Internal compile-and-runtime contract for the staged concrete Core Program seam.
//!
//! These tests deliberately exercise only the closed concrete boundary types;
//! the module stays crate-private until the terminal public cut is complete.

use core::iter::FusedIterator;

use crate::Srgb8;
use crate::program::{
    AppearanceContextErrorKindV1, AppearanceContextFieldV1, AppearanceContextV1, AssessmentV1,
    CertificateV1, CompileErrorHandleV1, CompileErrorKindV1, CompileErrorV1, ConstraintIdV1,
    ConstraintSubjectV1, ContentIdentityV9, DraftErrorV1, DraftV1, EvidenceBoundsErrorV1,
    EvidenceViewV1, FinitePaintDomainV1, InstantiateErrorV1, JointChoiceV1, JointOrderErrorV1,
    JointStateV1, NumericDomainErrorV1, ObservationHeadV1, OccurrenceIdV1, OpacityInputIdV1,
    OutputSlotIdV1, OwnerV1, PaintIdV1, PaintValueV1, PhysicalPointV1, PresentationRootIdV1,
    ScenarioV1, SessionV1, SignalV1, SourceIdV1, StateKindV1, SurfaceIdV1, SurfaceInputPortIdV1,
    SurroundV1, TargetCandidateIdV1, TargetCandidateV1, TargetIdV1, UpdateErrorKindV1,
    UpdateErrorV1, UpdateV1, VerdictV1,
};
use crate::program_session::{
    JointCandidateStateV1 as CoreJointCandidateStateV1,
    TargetCandidateChoiceV1 as CoreTargetCandidateChoiceV1,
    TargetCandidateId as CoreTargetCandidateId, TargetId as CoreTargetId,
};
use crate::selection_release::{
    MaterialisedSelectionV1, SelectionCandidateKeyV1, SelectionReleaseIdentityV1,
    SelectionReleaseV1, admit_selection_release_v1, materialise_joint_selection_v1,
};
use crate::wcag22::Wcag22CriterionV1;

/// Existing Program characterizations commit the prepared lifecycle through
/// this test-only extension; production exposes no immediate update method.
pub(crate) trait CommitProgramUpdateForTest {
    fn commit<'session>(
        &self,
        session: &'session mut SessionV1,
        update: UpdateV1<'_>,
    ) -> Result<EvidenceViewV1<'session>, UpdateErrorV1>;
}

impl CommitProgramUpdateForTest for OwnerV1 {
    fn commit<'session>(
        &self,
        session: &'session mut SessionV1,
        update: UpdateV1<'_>,
    ) -> Result<EvidenceViewV1<'session>, UpdateErrorV1> {
        self.prepare_update(session, update)
            .map(|prepared| prepared.commit())
    }
}

fn exact_size<I: ExactSizeIterator + FusedIterator>(iterator: I) -> I {
    iterator
}

fn finite_domain(candidates: Vec<TargetCandidateV1>) -> FinitePaintDomainV1 {
    FinitePaintDomainV1::try_new(candidates).unwrap()
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
        .commit(session, update)
        .expect("well-formed owner-bound update");
    assert_evidence_snapshot(view);
    Ok(())
}

fn assert_evidence_snapshot(view: EvidenceViewV1<'_>) {
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
                match cell.subject() {
                    ConstraintSubjectV1::VisibleUnary {
                        occurrence,
                        context,
                    } => {
                        let _ = occurrence.value();
                        let _ = context.adapting_luminance_cd_m2();
                    }
                    ConstraintSubjectV1::PointPresentation {
                        root,
                        occurrence,
                        terminal,
                    } => {
                        let _ = root.value();
                        let _ = occurrence.value();
                        let _ = terminal.value();
                    }
                    ConstraintSubjectV1::IntrinsicUnary { target } => {
                        let _ = target.value();
                    }
                    ConstraintSubjectV1::IntrinsicRelation { reference } => {
                        let _ = reference.value();
                    }
                    ConstraintSubjectV1::VisibleRelation { reference, context } => {
                        let _ = reference.value();
                        let _ = context.adapting_luminance_cd_m2();
                    }
                }
                let _ = cell.mode();
                let assessment = cell.assessment();
                let _: VerdictV1 = assessment.verdict();
                let binding = match assessment {
                    AssessmentV1::ExactSrgb8(evidence) => {
                        let _: Srgb8 = evidence.expected();
                        Some(evidence.binding())
                    }
                    AssessmentV1::Wcag22Srgb8(evidence) => {
                        let _ = evidence.profile_id();
                        let _ = evidence.criterion();
                        let _ = evidence.foreground_luminance();
                        let _ = evidence.background_luminance();
                        let _ = evidence.numerical_evidence();
                        Some(evidence.binding())
                    }
                    AssessmentV1::DeclaredSrgb8CleanSet(evidence) => {
                        let _ = evidence.visible();
                        let _ = evidence.violation();
                        let _ = evidence.rejected_blue_interval();
                        None
                    }
                    AssessmentV1::IntrinsicUnary(evidence) => {
                        let _ = evidence.verdict();
                        None
                    }
                    AssessmentV1::Relation(evidence) => {
                        let _ = evidence.member_count();
                        None
                    }
                };
                if let Some(binding) = binding {
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
                }
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
    let _ = owner.commit(session, update);
}

#[allow(dead_code)] // R-07 G4: boundary test helper retained for owner-mismatch contract verification
fn owner_mismatch_is_a_closed_boundary_error(error: UpdateErrorV1) {
    assert_eq!(error.kind(), UpdateErrorKindV1::OwnerMismatch);
}

#[test]
fn staged_boundary_uses_only_closed_concrete_types() {
    // Reaching this test means the concrete seam compiled without importing
    // Program<E>, evaluator traits, Session<Plan>, or numeric generations.
    assert_eq!(core::mem::size_of::<Srgb8>(), 3);
}

const NESTED_SOLID_PAINT: PaintIdV1 = PaintIdV1::new(4);
const NESTED_INPUT_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(6);
const NESTED_INNER_OCCURRENCE: OccurrenceIdV1 = OccurrenceIdV1::new(8);
const NESTED_TERMINAL_OCCURRENCE: OccurrenceIdV1 = OccurrenceIdV1::new(9);

fn fixed_nested_draft(
    opacity_value: f64,
    target_source: SourceIdV1,
    declared_input: SurfaceInputPortIdV1,
    used_input: SurfaceInputPortIdV1,
) -> DraftV1 {
    let source = SourceIdV1::new(1);
    let target = TargetIdV1::new(2);
    let opacity = OpacityInputIdV1::new(3);
    let translucent = PaintIdV1::new(5);
    let derived_surface = SurfaceIdV1::new(7);
    let exact = ConstraintIdV1::new(10);
    let wcag = ConstraintIdV1::new(11);
    let output = OutputSlotIdV1::new(12);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim).unwrap();

    let mut draft = DraftV1::new();
    draft.push_source(source, Srgb8::new([0; 3]));
    draft.push_fixed_target(target, target_source);
    draft.push_surface_input_port(declared_input);
    draft.push_opacity_input(opacity, opacity_value);
    draft.push_solid_paint(NESTED_SOLID_PAINT, target);
    draft.push_opacity_paint(translucent, NESTED_SOLID_PAINT, opacity);
    draft.push_input_surface(NESTED_INPUT_SURFACE, used_input);
    draft.push_source_over_occurrence(
        NESTED_INNER_OCCURRENCE,
        translucent,
        NESTED_INPUT_SURFACE,
        context,
    );
    draft.push_occurrence_surface(derived_surface, NESTED_INNER_OCCURRENCE);
    draft.push_source_over_occurrence(
        NESTED_TERMINAL_OCCURRENCE,
        NESTED_SOLID_PAINT,
        derived_surface,
        context,
    );
    draft.push_exact_visible_unary_hard(exact, NESTED_INNER_OCCURRENCE, Srgb8::new([0; 3]));
    draft.push_wcag22_visible_unary_report_only(
        wcag,
        NESTED_TERMINAL_OCCURRENCE,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    draft.push_output(output, translucent);
    draft
}

#[test]
fn program_compiler_admits_only_explicit_terminal_point_presentations() {
    let input = SurfaceInputPortIdV1::new(50);
    let root = PresentationRootIdV1::new(51);
    let mut valid = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    valid.push_point_presentation_root(root, NESTED_TERMINAL_OCCURRENCE);
    valid.push_point_presentation_target(root, NESTED_INNER_OCCURRENCE);
    let owner = valid.compile().unwrap();
    assert_eq!(owner.point_presentation_count(), 1);

    let mut intermediate = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    intermediate.push_point_presentation_root(root, NESTED_INNER_OCCURRENCE);
    intermediate.push_point_presentation_target(root, NESTED_INNER_OCCURRENCE);
    assert_eq!(
        compile_error(intermediate),
        CompileErrorV1::PresentationRootConsumedDownstream {
            root,
            occurrence: NESTED_INNER_OCCURRENCE,
        }
    );
}

#[test]
fn presentation_compile_failures_preserve_typed_root_and_occurrence_handles() {
    let input = SurfaceInputPortIdV1::new(50);
    let root = PresentationRootIdV1::new(51);

    let mut missing_root = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    missing_root.push_point_presentation_target(root, NESTED_INNER_OCCURRENCE);
    let error = compile_error(missing_root);
    assert_eq!(
        error.kind(),
        CompileErrorKindV1::MissingPointPresentationRoot
    );
    assert_eq!(
        error.primary_handle(),
        Some(CompileErrorHandleV1::PresentationRoot(root))
    );

    let mut unused_root = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    unused_root.push_point_presentation_root(root, NESTED_TERMINAL_OCCURRENCE);
    assert_eq!(
        compile_error(unused_root),
        CompileErrorV1::UnusedPresentationRoot { root }
    );

    let missing_occurrence = OccurrenceIdV1::new(999);
    let mut missing_target = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    missing_target.push_point_presentation_root(root, NESTED_TERMINAL_OCCURRENCE);
    missing_target.push_point_presentation_target(root, missing_occurrence);
    let error = compile_error(missing_target);
    assert_eq!(
        error,
        CompileErrorV1::MissingPointPresentationOccurrence {
            root,
            occurrence: missing_occurrence,
        }
    );
    assert_eq!(
        error.related_handle(),
        Some(CompileErrorHandleV1::Occurrence(missing_occurrence))
    );
}

#[test]
fn malformed_presentation_target_precedes_an_aggregate_unused_root_error() {
    let input = SurfaceInputPortIdV1::new(50);
    let root = PresentationRootIdV1::new(51);
    let orphan = PresentationRootIdV1::new(52);
    let missing = OccurrenceIdV1::new(999);
    let mut draft = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    draft.push_point_presentation_root(root, NESTED_TERMINAL_OCCURRENCE);
    draft.push_point_presentation_root(orphan, NESTED_TERMINAL_OCCURRENCE);
    draft.push_point_presentation_target(root, missing);

    assert_eq!(
        compile_error(draft),
        CompileErrorV1::MissingPointPresentationOccurrence {
            root,
            occurrence: missing,
        }
    );
}

#[test]
fn presentation_declarations_fail_closed_on_duplicates_and_unrelated_nodes() {
    let input = SurfaceInputPortIdV1::new(50);
    let root = PresentationRootIdV1::new(51);
    let terminal = NESTED_TERMINAL_OCCURRENCE;
    let target = NESTED_INNER_OCCURRENCE;

    let mut duplicate_root = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    duplicate_root.push_point_presentation_root(root, terminal);
    duplicate_root.push_point_presentation_root(root, terminal);
    duplicate_root.push_point_presentation_target(root, target);
    assert_eq!(
        compile_error(duplicate_root),
        CompileErrorV1::DuplicatePresentationRoot { root }
    );

    let mut duplicate_target = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    duplicate_target.push_point_presentation_root(root, terminal);
    duplicate_target.push_point_presentation_target(root, target);
    duplicate_target.push_point_presentation_target(root, target);
    assert_eq!(
        compile_error(duplicate_target),
        CompileErrorV1::DuplicatePointPresentationTarget {
            root,
            occurrence: target,
        }
    );

    let missing_terminal = OccurrenceIdV1::new(998);
    let mut missing_root_occurrence = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    missing_root_occurrence.push_point_presentation_root(root, missing_terminal);
    missing_root_occurrence.push_point_presentation_target(root, target);
    assert_eq!(
        compile_error(missing_root_occurrence),
        CompileErrorV1::MissingPresentationRootOccurrence {
            root,
            occurrence: missing_terminal,
        }
    );

    let unrelated = OccurrenceIdV1::new(997);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim).unwrap();
    let mut outside_ancestry = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input);
    outside_ancestry.push_source_over_occurrence(
        unrelated,
        NESTED_SOLID_PAINT,
        NESTED_INPUT_SURFACE,
        context,
    );
    outside_ancestry.push_point_presentation_root(root, terminal);
    outside_ancestry.push_point_presentation_target(root, unrelated);
    assert_eq!(
        compile_error(outside_ancestry),
        CompileErrorV1::PointPresentationOccurrenceOutsideRootAncestry {
            root,
            terminal,
            occurrence: unrelated,
        }
    );
}

fn attach_target_assessment(draft: &mut DraftV1, target: TargetIdV1) {
    let paint = PaintIdV1::new(770);
    let occurrence = OccurrenceIdV1::new(771);
    let constraint = ConstraintIdV1::new(772);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();
    draft.push_solid_paint(paint, target);
    draft.push_source_over_occurrence(occurrence, paint, NESTED_INPUT_SURFACE, context);
    draft.push_exact_visible_unary_report_only(constraint, occurrence, Srgb8::new([0; 3]));
}

fn materialised_joint_selection(
    revision: u64,
    target: u32,
    first: u32,
    second: u32,
) -> (MaterialisedSelectionV1, SelectionReleaseIdentityV1) {
    let key = |bytes: &[u8]| SelectionCandidateKeyV1::new(bytes.to_vec().into_boxed_slice());
    let release = SelectionReleaseV1::new(
        revision,
        vec![
            vec![key(b"first")].into_boxed_slice(),
            vec![key(b"second")].into_boxed_slice(),
        ]
        .into_boxed_slice(),
    );
    let admitted = admit_selection_release_v1(release).expect("joint test release must admit");
    let materialised = materialise_joint_selection_v1(
        &admitted,
        &[
            (
                CoreJointCandidateStateV1::new(vec![CoreTargetCandidateChoiceV1::new(
                    CoreTargetId::new(target),
                    CoreTargetCandidateId::new(second),
                )]),
                key(b"second"),
            ),
            (
                CoreJointCandidateStateV1::new(vec![CoreTargetCandidateChoiceV1::new(
                    CoreTargetId::new(target),
                    CoreTargetCandidateId::new(first),
                )]),
                key(b"first"),
            ),
        ],
    )
    .expect("complete joint test bindings must materialise");
    let identity = materialised.release_identity();
    (materialised, identity)
}

fn joint_draft_with_release(revision: u64, hard: bool) -> (DraftV1, SelectionReleaseIdentityV1) {
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
    draft.push_finite_target(
        target,
        finite_domain(vec![
            TargetCandidateV1::new(black, PaintValueV1::opaque(Srgb8::new([0; 3]))),
            TargetCandidateV1::new(white, PaintValueV1::opaque(Srgb8::new([255; 3]))),
        ]),
    );
    let (selection, release_identity) =
        materialised_joint_selection(revision, target.value(), black.value(), white.value());
    draft.set_materialised_joint_selection(selection).unwrap();
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    if hard {
        draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([128; 3]));
    } else {
        draft.push_exact_visible_unary_report_only(constraint, occurrence, Srgb8::new([0; 3]));
    }
    draft.push_output(output, paint);
    (draft, release_identity)
}

fn joint_draft(hard: bool) -> DraftV1 {
    joint_draft_with_release(1, hard).0
}

#[test]
fn finite_paint_candidates_are_not_a_cartesian_source_opacity_domain() {
    let target = TargetIdV1::new(2);
    let translucent_white = TargetCandidateIdV1::new(3);
    let opaque_gray = TargetCandidateIdV1::new(4);
    let input = SurfaceInputPortIdV1::new(5);
    let paint = PaintIdV1::new(6);
    let surface = SurfaceIdV1::new(7);
    let occurrence = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let mut draft = DraftV1::new();
    draft.push_finite_target(
        target,
        finite_domain(vec![
            TargetCandidateV1::new(
                translucent_white,
                PaintValueV1::try_new(Srgb8::new([0xFF; 3]), 0.25).unwrap(),
            ),
            TargetCandidateV1::new(
                opaque_gray,
                PaintValueV1::try_new(Srgb8::new([0x40; 3]), 1.0).unwrap(),
            ),
        ]),
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(target, translucent_white)]),
            JointStateV1::new(vec![JointChoiceV1::new(target, opaque_gray)]),
        ])
        .unwrap();
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([0xFF; 3]));
    draft.push_output(output, paint);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(1).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [ScenarioV1::new(1, &black)];
    let projection = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Conflict(conflict)) = projection.certificates().next() else {
        panic!("only the two declared atomic Paint values may be considered");
    };
    assert_eq!(conflict.considered_state_count(), 2);
}

/// Robust alpha at the closed staged boundary: one revision with two unique
/// correlated backdrops rejects the translucent candidate that is exact only
/// over black, while the identical authored order legitimately selects that
/// same candidate when the hostile backdrop is not observed.
#[test]
fn staged_alpha_certification_requires_every_unique_backdrop_scenario() {
    let target = TargetIdV1::new(2);
    let translucent_white = TargetCandidateIdV1::new(3);
    let opaque_mid = TargetCandidateIdV1::new(4);
    let input = SurfaceInputPortIdV1::new(5);
    let paint = PaintIdV1::new(6);
    let surface = SurfaceIdV1::new(7);
    let occurrence = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let build = || {
        let mut draft = DraftV1::new();
        draft.push_finite_target(
            target,
            finite_domain(vec![
                TargetCandidateV1::new(
                    translucent_white,
                    PaintValueV1::try_new(Srgb8::new([0xFF; 3]), 0.5).unwrap(),
                ),
                TargetCandidateV1::new(opaque_mid, PaintValueV1::opaque(Srgb8::new([0x80; 3]))),
            ]),
        );
        draft
            .set_joint_selection(vec![
                JointStateV1::new(vec![JointChoiceV1::new(target, translucent_white)]),
                JointStateV1::new(vec![JointChoiceV1::new(target, opaque_mid)]),
            ])
            .unwrap();
        draft.push_surface_input_port(input);
        draft.push_solid_paint(paint, target);
        draft.push_input_surface(surface, input);
        draft.push_source_over_occurrence(occurrence, paint, surface, context);
        draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([0x80; 3]));
        draft.push_output(output, paint);
        draft.compile().unwrap()
    };

    let owner = build();
    let mut session = owner.instantiate(31).unwrap();
    let black = [Srgb8::new([0x00; 3])];
    let white = [Srgb8::new([0xFF; 3])];
    let correlated = [ScenarioV1::new(1, &black), ScenarioV1::new(2, &white)];
    let projection = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &correlated,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
        panic!("the opaque candidate must certify over the complete backdrop set");
    };
    assert_eq!(certificate.selected_state_index(), Some(1));
    let cells = certificate.cells().collect::<Vec<_>>();
    assert_eq!(cells.len(), 2);
    for cell in &cells {
        assert_eq!(cell.assessment().verdict(), VerdictV1::Pass);
        let AssessmentV1::ExactSrgb8(evidence) = cell.assessment() else {
            panic!("the authored exact constraint must retain Exact evidence");
        };
        let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = evidence.binding().physical();
        assert_eq!(physical.opacity().to_bits(), 1.0_f64.to_bits());
        assert_eq!(physical.visible(), Srgb8::new([0x80; 3]));
    }
    let selected = certificate.outputs().next().unwrap();
    assert_eq!(selected.source(), Srgb8::new([0x80; 3]));
    assert_eq!(selected.opacity().to_bits(), 1.0_f64.to_bits());

    // The same authored program over black alone selects the translucent
    // candidate, attributing the rejection above to the correlated scenario.
    let single_owner = build();
    let mut single_session = single_owner.instantiate(32).unwrap();
    let single = [ScenarioV1::new(1, &black)];
    let projection = single_owner
        .commit(
            &mut single_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &single,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
        panic!("the translucent candidate must certify over the black backdrop alone");
    };
    assert_eq!(certificate.selected_state_index(), Some(0));
    let selected = certificate.outputs().next().unwrap();
    assert_eq!(selected.source(), Srgb8::new([0xFF; 3]));
    assert_eq!(selected.opacity().to_bits(), 0.5_f64.to_bits());
}

/// Encoded output-domain boundary `#010000` at the staged seam: `0.5/255` is
/// below the continuous strict floor `1/255`, yet its final encoded value
/// rounds exactly to the target byte, while the immediate binary64
/// predecessor still encodes to `#000000`. The certificate must carry the
/// exact admitted alpha bits and the exact encoded visible.
#[test]
fn staged_boundary_certifies_half_code_alpha_on_the_final_encoded_value() {
    let half_code = 0.5_f64 / 255.0;
    let below_half_code = f64::from_bits(half_code.to_bits() - 1);
    assert!(half_code < 1.0_f64 / 255.0);

    let target = TargetIdV1::new(2);
    let below = TargetCandidateIdV1::new(3);
    let half = TargetCandidateIdV1::new(4);
    let input = SurfaceInputPortIdV1::new(5);
    let paint = PaintIdV1::new(6);
    let surface = SurfaceIdV1::new(7);
    let occurrence = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();
    let red = Srgb8::new([0xFF, 0x00, 0x00]);

    let mut draft = DraftV1::new();
    draft.push_finite_target(
        target,
        finite_domain(vec![
            TargetCandidateV1::new(below, PaintValueV1::try_new(red, below_half_code).unwrap()),
            TargetCandidateV1::new(half, PaintValueV1::try_new(red, half_code).unwrap()),
        ]),
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(target, below)]),
            JointStateV1::new(vec![JointChoiceV1::new(target, half)]),
        ])
        .unwrap();
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([0x01, 0x00, 0x00]));
    draft.push_output(output, paint);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(33).unwrap();
    let black = [Srgb8::new([0x00; 3])];
    let scenarios = [ScenarioV1::new(1, &black)];
    let projection = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
        panic!("half-code alpha must reach the exact encoded boundary target");
    };
    assert_eq!(certificate.selected_state_index(), Some(1));
    let AssessmentV1::ExactSrgb8(assessment) = certificate.cells().next().unwrap().assessment()
    else {
        panic!("the authored exact constraint must retain Exact evidence");
    };
    let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = assessment.binding().physical();
    assert_eq!(physical.opacity().to_bits(), half_code.to_bits());
    assert_eq!(physical.visible(), Srgb8::new([0x01, 0x00, 0x00]));
    let selected = certificate.outputs().next().unwrap();
    assert_eq!(selected.source(), red);
    assert_eq!(selected.opacity().to_bits(), half_code.to_bits());
}

/// The translucent terminal composes over its declared derived surface, not
/// over the observation root. The inner opaque layer pins the derived surface
/// to `0x20` for every backdrop, so the exact target `0x90` is reachable only
/// through the derived value; substituting the root backdrop yields `0x80` or
/// `0xFF` and must conflict on every state.
#[test]
fn staged_translucent_terminal_composes_over_the_derived_surface_not_the_root() {
    let inner_source = SourceIdV1::new(1);
    let fixed = TargetIdV1::new(2);
    let target = TargetIdV1::new(3);
    let selected = TargetCandidateIdV1::new(4);
    let decoy = TargetCandidateIdV1::new(5);
    let input = SurfaceInputPortIdV1::new(6);
    let inner_paint = PaintIdV1::new(7);
    let terminal_paint = PaintIdV1::new(8);
    let input_surface = SurfaceIdV1::new(9);
    let derived_surface = SurfaceIdV1::new(10);
    let inner_occurrence = OccurrenceIdV1::new(11);
    let terminal_occurrence = OccurrenceIdV1::new(12);
    let inner_constraint = ConstraintIdV1::new(13);
    let terminal_constraint = ConstraintIdV1::new(14);
    let output = OutputSlotIdV1::new(15);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let mut draft = DraftV1::new();
    draft.push_source(inner_source, Srgb8::new([0x20; 3]));
    draft.push_fixed_target(fixed, inner_source);
    draft.push_finite_target(
        target,
        finite_domain(vec![
            TargetCandidateV1::new(
                selected,
                PaintValueV1::try_new(Srgb8::new([0xFF; 3]), 0.5).unwrap(),
            ),
            TargetCandidateV1::new(decoy, PaintValueV1::opaque(Srgb8::new([0x00; 3]))),
        ]),
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(target, selected)]),
            JointStateV1::new(vec![JointChoiceV1::new(target, decoy)]),
        ])
        .unwrap();
    draft.push_surface_input_port(input);
    draft.push_solid_paint(inner_paint, fixed);
    draft.push_solid_paint(terminal_paint, target);
    draft.push_input_surface(input_surface, input);
    draft.push_source_over_occurrence(inner_occurrence, inner_paint, input_surface, context);
    draft.push_occurrence_surface(derived_surface, inner_occurrence);
    draft.push_source_over_occurrence(
        terminal_occurrence,
        terminal_paint,
        derived_surface,
        context,
    );
    draft.push_exact_visible_unary_hard(inner_constraint, inner_occurrence, Srgb8::new([0x20; 3]));
    // 0x20 + 0.5 * (0xFF - 0x20) = 143.5 -> 0x90 over the derived surface;
    // the same paint over the root backdrops yields 0x80 (black) / 0xFF (white).
    draft.push_exact_visible_unary_hard(
        terminal_constraint,
        terminal_occurrence,
        Srgb8::new([0x90; 3]),
    );
    draft.push_output(output, terminal_paint);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(34).unwrap();
    let black = [Srgb8::new([0x00; 3])];
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &black), ScenarioV1::new(2, &white)];
    let projection = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
        panic!("the translucent terminal must certify only through its derived surface");
    };
    assert_eq!(certificate.selected_state_index(), Some(0));
    for cell in certificate.cells() {
        assert_eq!(cell.assessment().verdict(), VerdictV1::Pass);
        let AssessmentV1::ExactSrgb8(evidence) = cell.assessment() else {
            panic!("the authored exact constraints must retain Exact evidence");
        };
        let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = evidence.binding().physical();
        if cell.subject()
            == (ConstraintSubjectV1::VisibleUnary {
                occurrence: terminal_occurrence,
                context,
            })
        {
            assert_eq!(physical.backdrop(), Srgb8::new([0x20; 3]));
            assert_eq!(physical.visible(), Srgb8::new([0x90; 3]));
            assert_eq!(physical.opacity().to_bits(), 0.5_f64.to_bits());
        }
    }
    let selected_output = certificate.outputs().next().unwrap();
    assert_eq!(selected_output.source(), Srgb8::new([0xFF; 3]));
    assert_eq!(selected_output.opacity().to_bits(), 0.5_f64.to_bits());
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
fn evidence_cell_bounds_are_independent_of_internal_causal_replay_storage() {
    let source = SourceIdV1::new(1);
    let target = TargetIdV1::new(2);
    let input = SurfaceInputPortIdV1::new(3);
    let paint = PaintIdV1::new(4);
    let input_surface = SurfaceIdV1::new(5);
    let derived_surface = SurfaceIdV1::new(6);
    let inner = OccurrenceIdV1::new(7);
    let terminal = OccurrenceIdV1::new(8);
    let constraint = ConstraintIdV1::new(9);
    let output = OutputSlotIdV1::new(10);
    let root = PresentationRootIdV1::new(11);
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Average).unwrap();

    let mut draft = DraftV1::new();
    draft.push_source(source, Srgb8::new([0; 3]));
    draft.push_fixed_target(target, source);
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(input_surface, input);
    draft.push_source_over_occurrence(inner, paint, input_surface, context);
    draft.push_occurrence_surface(derived_surface, inner);
    draft.push_source_over_occurrence(terminal, paint, derived_surface, context);
    draft.push_exact_visible_unary_hard(constraint, terminal, Srgb8::new([0; 3]));
    draft.push_point_presentation_root(root, terminal);
    draft.push_point_presentation_target(root, inner);
    draft.push_output(output, paint);
    let owner = draft.compile().unwrap();

    // One cell per scenario still fits. The two-step causal replay would not,
    // but that private arena is outside this cell-only prospective query.
    let scenario_count = usize::MAX / 2 + 1;
    let bounds = owner.evidence_cell_bounds(scenario_count).unwrap();
    assert_eq!(bounds.verified_cells(), scenario_count);
    assert_eq!(bounds.conflict_cells(), scenario_count);
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
        .commit(
            &mut joint_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &unique_scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Conflict(certificate)) = projection.certificates().next() else {
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
        .commit(
            &mut fixed_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &duplicate_scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
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
            .commit(
                &mut session,
                UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap();
        let Some(CertificateV1::Verified(certificate)) = projection.certificates().next() else {
            panic!("the fixed admissible program must produce Verified evidence");
        };
        assert_eq!(certificate.cells().len(), expected.verified_cells());
    }

    let after_update = owner.evidence_cell_bounds(1).unwrap();
    assert_eq!(after_update.verified_cells(), expected.verified_cells());
    assert_eq!(after_update.conflict_cells(), expected.conflict_cells());
    let projection = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(projection.kind(), StateKindV1::Ready);
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
        finite_domain(vec![
            TargetCandidateV1::new(gray, PaintValueV1::opaque(Srgb8::new([0x80; 3]))),
            TargetCandidateV1::new(black, PaintValueV1::opaque(Srgb8::new([0; 3]))),
        ]),
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
    draft.push_exact_visible_unary_report_only(exact, high_occurrence, Srgb8::new([0; 3]));
    draft.push_wcag22_visible_unary_hard(
        high_wcag,
        high_occurrence,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    draft.push_wcag22_visible_unary_hard(
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
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(ready.kind(), StateKindV1::Ready);
    let Some(CertificateV1::Verified(certificate)) = ready.certificates().next() else {
        panic!("Ready must retain one Verified certificate");
    };
    let outputs = certificate.outputs().collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].output_slot(), output);
    assert_eq!(outputs[0].source(), Srgb8::new([0; 3]));
    assert_eq!(outputs[0].opacity(), 1.0);
    assert_eq!(certificate.observation().revision(), 1);
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
    assert_eq!(owner.selection_release_identity(), None);
    assert_eq!(owner.surface_input_ports().collect::<Vec<_>>(), [input]);

    let mut session = owner.instantiate(13).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    let state = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(state.kind(), StateKindV1::Ready);
    let Some(CertificateV1::Verified(certificate)) = state.certificates().next() else {
        panic!("a fixed target must produce one Verified certificate");
    };
    assert_eq!(certificate.selection_release_identity(), None);
    assert_eq!(certificate.selected_state_index(), None);
    let outputs = certificate.outputs().collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].output_slot(), OutputSlotIdV1::new(12));
    assert_eq!(outputs[0].source(), Srgb8::new([0; 3]));
    assert_eq!(outputs[0].opacity(), 1.0);
    assert_eq!(certificate.observation().revision(), 1);
}

#[test]
fn exact_selection_release_identity_reaches_owner_verified_conflict_and_stale_evidence() {
    let (first_draft, first_release) = joint_draft_with_release(7, false);
    let (second_draft, second_release) = joint_draft_with_release(8, false);
    let first_owner = first_draft.compile().unwrap();
    let second_owner = second_draft.compile().unwrap();
    assert_ne!(first_release, second_release);
    assert_eq!(
        first_owner.selection_release_identity(),
        Some(first_release)
    );
    assert_eq!(
        second_owner.selection_release_identity(),
        Some(second_release)
    );
    assert_ne!(
        first_owner.content_identity(),
        second_owner.content_identity()
    );

    let black = [Srgb8::new([0; 3])];
    let scenarios = [ScenarioV1::new(1, &black)];
    let mut first_session = first_owner.instantiate(70).unwrap();
    let mut second_session = second_owner.instantiate(80).unwrap();
    let first_ready = first_owner
        .commit(
            &mut first_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let second_ready = second_owner
        .commit(
            &mut second_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let first_certificate = first_ready.certificates().next().unwrap();
    let second_certificate = second_ready.certificates().next().unwrap();
    assert_eq!(
        first_certificate.selection_release_identity(),
        Some(first_release),
    );
    assert_eq!(
        second_certificate.selection_release_identity(),
        Some(second_release),
    );
    assert_ne!(
        first_certificate.content_identity(),
        second_certificate.content_identity(),
    );

    let stale = first_owner
        .commit(
            &mut first_session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 9,
            },
        )
        .unwrap();
    assert_eq!(stale.kind(), StateKindV1::Stale);
    assert_eq!(
        stale
            .certificates()
            .next()
            .unwrap()
            .selection_release_identity(),
        Some(first_release),
    );

    let (conflict_draft, conflict_release) = joint_draft_with_release(9, true);
    let conflict_owner = conflict_draft.compile().unwrap();
    let mut conflict_session = conflict_owner.instantiate(90).unwrap();
    let conflict = conflict_owner
        .commit(
            &mut conflict_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Conflict(conflict_certificate)) = conflict.certificates().next() else {
        panic!("hard finite test must retain exhaustive conflict evidence");
    };
    assert_eq!(
        conflict_certificate.selection_release_identity(),
        Some(conflict_release),
    );
}

#[test]
fn observed_violation_retains_previous_certificate_only_as_evidence() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut session = owner.instantiate(13).unwrap();

    let black = [Srgb8::new([0; 3])];
    let black_scenarios = [ScenarioV1::new(1, &black)];
    let ready = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &black_scenarios,
            },
        )
        .unwrap();
    assert_eq!(ready.kind(), StateKindV1::Ready);

    let white = [Srgb8::new([0xFF; 3])];
    let white_scenarios = [ScenarioV1::new(2, &white)];
    let failed = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &white_scenarios,
            },
        )
        .unwrap();
    assert_eq!(failed.kind(), StateKindV1::Failed);
    let certificates = failed.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 2);
    assert!(matches!(certificates[0], CertificateV1::Conflict(_)));
    let CertificateV1::Verified(previous) = certificates[1] else {
        panic!("the previous certificate must remain available as diagnostics");
    };
    assert_eq!(previous.observation().revision(), 1);
}

#[test]
fn program_prepare_drop_is_invisible_and_commit_returns_the_new_evidence() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut session = owner.instantiate(13).unwrap();
    let black = [Srgb8::new([0; 3])];
    let scenarios = [ScenarioV1::new(1, &black)];
    owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();

    let prepared = owner
        .prepare_update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 9,
            },
        )
        .unwrap();
    drop(prepared);

    let unchanged = session.evidence();
    assert_eq!(unchanged.kind(), StateKindV1::Ready);
    assert!(matches!(
        unchanged.observation_head(),
        ObservationHeadV1::Observed { revision: 1, .. }
    ));

    let committed = owner
        .prepare_update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 9,
            },
        )
        .unwrap()
        .commit();
    assert_eq!(committed.kind(), StateKindV1::Stale);
    assert!(matches!(
        committed.observation_head(),
        ObservationHeadV1::Unknown { revision: 2, .. }
    ));
}

#[test]
fn unknown_context_retains_previous_certificate_only_as_evidence() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(0.5, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let mut session = owner.instantiate(13).unwrap();

    let black = [Srgb8::new([0; 3])];
    let black_scenarios = [ScenarioV1::new(1, &black)];
    let ready = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &black_scenarios,
            },
        )
        .unwrap();
    assert_eq!(ready.kind(), StateKindV1::Ready);

    let stale = owner
        .commit(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 9,
            },
        )
        .unwrap();
    assert_eq!(stale.kind(), StateKindV1::Stale);
    let certificates = stale.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    let CertificateV1::Verified(previous) = certificates[0] else {
        panic!("the previous certificate must remain available as diagnostics");
    };
    assert_eq!(previous.observation().revision(), 1);
    assert!(matches!(
        stale.observation_head(),
        ObservationHeadV1::Unknown { revision: 2, .. }
    ));
}

#[test]
fn owner_and_update_errors_preserve_content_and_input_identity() {
    let input = SurfaceInputPortIdV1::new(50);
    let owner = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input)
        .compile()
        .unwrap();
    let owner_identity: ContentIdentityV9 = owner.content_identity();
    let mut session = owner.instantiate(13).unwrap();

    let no_scenarios = [];
    let error = match owner.commit(
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
    let error = match owner.commit(
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
    let error = match owner.commit(
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
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &valid,
            },
        )
        .unwrap();
    let certificate = projection.certificates().next().unwrap();
    assert_eq!(owner_identity, certificate.content_identity());

    let error = match owner.commit(
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

    let error = match owner.commit(
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
    let error = match foreign.commit(
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
    draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([0x80; 3]));
    draft.push_output(output, translucent);

    let owner = draft.compile().unwrap();
    let mut session = owner.instantiate(17).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    let state = owner
        .commit(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = state.certificates().next() else {
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
    assert_eq!(error.kind(), CompileErrorKindV1::MissingFixedSource);
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
        Some(CompileErrorHandleV1::Surface(NESTED_INPUT_SURFACE))
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
            first: NESTED_INPUT_SURFACE,
            duplicate: duplicate_surface,
        }
    );
}

#[test]
fn duplicate_candidate_value_preserves_both_candidates_and_exact_stimulus() {
    let input = SurfaceInputPortIdV1::new(50);
    let target = TargetIdV1::new(70);
    let first = TargetCandidateIdV1::new(701);
    let duplicate = TargetCandidateIdV1::new(702);
    let encoded_srgb8 = Srgb8::new([17, 33, 65]);
    let mut draft = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    draft.push_finite_target(
        target,
        finite_domain(vec![
            TargetCandidateV1::new(first, PaintValueV1::opaque(encoded_srgb8)),
            TargetCandidateV1::new(duplicate, PaintValueV1::opaque(encoded_srgb8)),
        ]),
    );
    attach_target_assessment(&mut draft, target);

    assert_eq!(
        compile_error(draft),
        CompileErrorV1::DuplicateTargetCandidateValue {
            target,
            first,
            duplicate,
            value: PaintValueV1::opaque(encoded_srgb8),
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
        TargetCandidateV1::new(first, PaintValueV1::opaque(Srgb8::new([0; 3]))),
        TargetCandidateV1::new(second, PaintValueV1::opaque(Srgb8::new([255; 3]))),
    ];

    let mut duplicate_target = fixed_nested_draft(1.0, SourceIdV1::new(1), input, input);
    duplicate_target.push_finite_target(target, finite_domain(candidates.clone()));
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
    incomplete.push_finite_target(target, finite_domain(candidates));
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
        NESTED_SOLID_PAINT,
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
    draft
        .set_joint_selection(vec![JointStateV1::new(Vec::new())])
        .unwrap();
    let error = match draft.set_joint_selection(vec![JointStateV1::new(Vec::new())]) {
        Ok(_) => panic!("a singleton declaration must not be replaced"),
        Err(error) => error,
    };
    assert_eq!(error, DraftErrorV1::JointSelectionAlreadyDeclared);
}
