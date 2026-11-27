use super::tests::{compile_assignment, program, target, update};
use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::observation::{
    ObservationPayloadInput, ObservationUpdateInput, ObservedScenarioSetInput, ScenarioId,
    ScenarioInput, SurfaceInputBinding,
};
use crate::session::SessionState;

mod independent_fields {
    use super::*;
    use crate::observation::Revision;
    use crate::program::wire::ProgramWireBuilderV1;
    use crate::program_diagnostics::ProgramDiagnosticV1;
    use crate::program_wire::{
        ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1, ProgramDeclaredFieldV1,
        ProgramRuntimeErrorV1, ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
    };

    fn two_inputs() -> Program<Wcag22Srgb8V1> {
        let mut declaration = program(
            target(10, 64, 64, 65, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        duplicate_use(&mut declaration, target(20, 80, 80, 81, 1));
        declaration
            .observation_group
            .surface_input_ports
            .push(SurfaceInputPortId::new(12));
        declaration.surfaces.push(Surface::Input {
            id: SurfaceId::new(14),
            input: SurfaceInputPortId::new(12),
        });
        declaration.occurrences[1].against = SurfaceId::new(14);
        declaration
    }

    fn compile_fields(
        mut declaration: Program<Wcag22Srgb8V1>,
        authored: bool,
        reverse: bool,
    ) -> CompiledProgram<Wcag22Srgb8V1> {
        let mut order: Vec<_> = declaration
            .targets
            .iter()
            .filter(|target| matches!(target.intent, TargetIntentV1::Computed(_)))
            .map(|target| target.id)
            .collect();
        if reverse {
            order.reverse();
        }
        declaration.assignment = Some(if authored {
            ComputedSelectionPolicyV1::AuthoredComposition {
                legacy_reference_order: order,
            }
        } else {
            ComputedSelectionPolicyV1::Ordered(order)
        });
        declaration.computed_budget = Some(256);
        declaration.compile().expect("valid two-input declaration")
    }

    fn add_report(declaration: &mut Program<Wcag22Srgb8V1>, occurrence: u32) {
        declaration
            .constraints
            .report_only
            .push(ConstraintInvocation::visible_unary_report_only(
                ConstraintId::new(26),
                OccurrenceId::new(occurrence),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
    }

    #[test]
    fn local_report_only_accepts_both_independent_fields_in_every_target_order() {
        for authored in [false, true] {
            for reverse in [false, true] {
                for occurrence in [5, 15] {
                    let mut declaration = two_inputs();
                    add_report(&mut declaration, occurrence);
                    let owner = compile_fields(declaration, authored, reverse);
                    assert!(
                        owner.accepts_independent_fields(&[0, 1]),
                        "local report must stay local: authored={authored}, reverse={reverse}, occurrence={occurrence}"
                    );
                }
            }
        }
    }

    #[test]
    fn report_only_expands_selected_color_dependencies_before_admitting_fields() {
        for authored in [false, true] {
            for reverse in [false, true] {
                let mut declaration = two_inputs();
                // This opaque occurrence directly reads input 12 for contrast.
                // Its selected source still depends on input 2 through hard 6.
                declaration.occurrences.push(Occurrence::new(
                    OccurrenceId::new(25),
                    PaintId::new(3),
                    SurfaceId::new(14),
                    CompositionProfile::EncodedSrgb8SourceOverV1,
                    declaration.occurrences[0].context,
                ));
                add_report(&mut declaration, 25);
                let owner = compile_fields(declaration, authored, reverse);
                assert!(owner.accepts_independent_fields(&[0]));
                assert!(owner.accepts_independent_fields(&[1]));
                assert!(
                    !owner.accepts_independent_fields(&[0, 1]),
                    "report reads two independently selected inputs: authored={authored}, reverse={reverse}"
                );
            }
        }
    }

    #[test]
    fn report_only_between_opaque_selected_colors_expands_both_components() {
        for authored in [false, true] {
            for reverse in [false, true] {
                let mut declaration = two_inputs();
                declaration.surfaces.push(Surface::FromOccurrence {
                    id: SurfaceId::new(24),
                    occurrence: OccurrenceId::new(15),
                });
                declaration.occurrences.push(Occurrence::new(
                    OccurrenceId::new(25),
                    PaintId::new(3),
                    SurfaceId::new(24),
                    CompositionProfile::EncodedSrgb8SourceOverV1,
                    declaration.occurrences[0].context,
                ));
                add_report(&mut declaration, 25);
                let owner = compile_fields(declaration, authored, reverse);
                let CompiledTargetSelectionV1::Computed(selection) =
                    &owner.owner_generation.target_selection
                else {
                    panic!("computed selection");
                };
                assert_eq!(
                    selection.components.len(),
                    2,
                    "reports preserve separate searches"
                );
                assert!(
                    !owner.accepts_independent_fields(&[0, 1]),
                    "both opaque colors inherit their selecting component's inputs"
                );
            }
        }
    }

    #[test]
    fn fixed_only_local_hard_and_report_constraints_accept_independent_fields() {
        let mut declaration = two_inputs();
        for (index, entry) in declaration.targets.iter_mut().enumerate() {
            let source = SourceId::new(30 + index as u32);
            declaration.sources.push(Source::new(
                source,
                ColorSignal::from_srgb8(Srgb8::new([64 + index as u8; 3])),
            ));
            entry.intent = TargetIntentV1::FixedSource(source);
        }
        add_report(&mut declaration, 5);
        let owner = compile_fields(declaration, true, false);
        assert!(owner.accepts_independent_fields(&[0, 1]));
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("fixed-only session");
        session
            .prepare_schema_ordered_field(Revision::new(1), &field())
            .expect("independent fields for fixed colors")
            .commit();
        let current = session
            .state()
            .current_verified()
            .expect("fixed colors satisfy both physical fields")
            .computed_for_test();
        assert_eq!(current.arena.storage.outputs.len(), 2);
        assert_eq!(
            current.arena.storage.outputs[0].paint.source(),
            Srgb8::new([64; 3])
        );
        assert_eq!(
            current.arena.storage.outputs[1].paint.source(),
            Srgb8::new([65; 3])
        );
        assert!(
            current
                .arena
                .storage
                .cells
                .iter()
                .any(|cell| cell.constraint() == ConstraintId::new(26))
        );
        assert!(
            current
                .arena
                .storage
                .cells
                .iter()
                .all(|cell| !cell.result().is_violation())
        );
    }

    #[test]
    fn fixed_only_failure_has_one_witness_without_a_physical_field() {
        let mut declaration = two_inputs();
        declaration.sources.push(Source::new(
            SourceId::new(30),
            ColorSignal::from_srgb8(Srgb8::new([64; 3])),
        ));
        declaration.targets[0].intent = TargetIntentV1::FixedSource(SourceId::new(30));
        declaration
            .constraints
            .hard
            .push(ConstraintInvocation::exact_intrinsic_unary_hard(
                ConstraintId::new(26),
                TargetId::new(10),
                Srgb8::new([255; 3]),
            ));
        let owner = compile_fields(declaration, true, false);
        assert!(owner.accepts_independent_fields(&[0, 1]));
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_schema_ordered_field(Revision::new(1), &field())
            .expect("admitted independent fields")
            .commit();
        let SessionState::Failed {
            cause: ProgramViolationOutcomeV1::Computed(cause),
            ..
        } = session.state()
        else {
            panic!("the fixed source cannot satisfy the intrinsic requirement");
        };
        let violations = cause
            .diagnostics()
            .witness()
            .expect("fixed-only conflict witness")
            .violations();
        assert!(
            matches!(
                violations,
                [ProgramDiagnosticV1::HardConstraint {
                    constraint_id: 26,
                    field_case: None,
                    ..
                }]
            ),
            "fixed evidence must not be repeated across the field supports: {violations:?}"
        );
    }

    fn hierarchy(cross_input: bool) -> Program<Wcag22Srgb8V1> {
        let mut declaration = two_inputs();
        declaration.reference_inputs = Some(vec![
            (SurfaceInputPortId::new(2), Srgb8::new([255; 3])),
            (SurfaceInputPortId::new(12), Srgb8::new([255; 3])),
        ]);
        if !cross_input {
            declaration.occurrences[1].against = SurfaceId::new(4);
        }
        // The hierarchy endpoints share a physical backdrop, as required by
        // its contract. The second source also has its original hard use.
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(25),
            PaintId::new(13),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            declaration.occurrences[0].context,
        ));
        declaration
            .constraints
            .hard
            .push(ConstraintInvocation::preserve_tonal_hierarchy(
                ConstraintId::new(30),
                vec![OccurrenceId::new(5), OccurrenceId::new(25)],
            ));
        declaration
    }

    #[test]
    fn tonal_hierarchy_includes_its_backdrop_and_selected_endpoint_dependencies() {
        for authored in [false, true] {
            for reverse in [false, true] {
                for cross_input in [false, true] {
                    let owner = compile_fields(hierarchy(cross_input), authored, reverse);
                    assert_eq!(
                        owner.accepts_independent_fields(&[0, 1]),
                        !cross_input,
                        "hierarchy scope: authored={authored}, reverse={reverse}, cross_input={cross_input}"
                    );
                }
            }
        }
    }

    #[test]
    fn tonal_hierarchy_violation_is_attributed_to_its_own_field() {
        let owner = compile_fields(hierarchy(false), false, false);
        assert!(owner.accepts_independent_fields(&[0, 1]));
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        let dark = ProgramDeclaredFieldV1::from_independent_gradients_rgb8(
            &[0, 0, 0, 250, 250, 250],
            &[0, 1],
            &[0, 2, 4],
            &[0.0, 1.0, 0.0, 1.0],
            &[0, 0, 0, 1, 1, 1, 250, 250, 250, 252, 252, 252],
        )
        .expect("unequal independent supports");
        session
            .prepare_schema_ordered_field(Revision::new(1), &dark)
            .expect("admitted independent fields")
            .commit();
        let SessionState::Failed {
            cause: ProgramViolationOutcomeV1::Computed(cause),
            ..
        } = session.state()
        else {
            panic!("the fixed gray domains cannot preserve light-backdrop hierarchy on black");
        };
        let violations = cause
            .diagnostics()
            .witness()
            .expect("attempt witness")
            .violations();
        let cases: Vec<_> = violations
            .iter()
            .filter_map(|violation| match violation {
                ProgramDiagnosticV1::TonalHierarchy {
                    constraint_id: 30,
                    field_case,
                    ..
                } => Some(field_case.expect("hierarchy has a varying physical backdrop")),
                _ => None,
            })
            .collect();
        assert_eq!(
            cases.len(),
            2,
            "one hierarchy violation per local gray byte"
        );
        assert!(cases.iter().all(|case| case.input_index() == 0));
        assert_eq!(
            cases
                .iter()
                .map(|case| case.case_index())
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn visible_identity_max_change_and_hard_failures_keep_their_local_field_scope() {
        let mut declaration = two_inputs();
        let mut first = blue(10, 5, 0);
        let TargetIntentV1::Computed(color) = &mut first.intent else {
            unreachable!();
        };
        color.domain = AssignmentDomainV1::Gray { lower: 0, upper: 0 };
        color.max_delta_bits = 0.01_f64.to_bits();
        declaration.targets[0] = first;
        declaration
            .opacities
            .push(OpacityInput::new(OpacityInputId::new(11), 0.0));
        declaration.paints.push(Paint::Opacity {
            id: PaintId::new(31),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(11),
        });
        declaration.occurrences[0].subject = PaintId::new(31);
        declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(31));
        let owner = compile_fields(declaration, false, false);
        assert!(owner.accepts_independent_fields(&[0, 1]));
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_schema_ordered_field(Revision::new(1), &field())
            .expect("admitted independent fields")
            .commit();
        let SessionState::Failed {
            cause: ProgramViolationOutcomeV1::Computed(cause),
            ..
        } = session.state()
        else {
            panic!("fully transparent blue cannot satisfy visible identity or contrast");
        };
        let violations = cause
            .diagnostics()
            .witness()
            .expect("attempt witness")
            .violations();
        assert_eq!(
            violations
                .iter()
                .filter(|violation| matches!(
                    violation,
                    ProgramDiagnosticV1::TargetDomain { target_id: 10, .. }
                ))
                .count(),
            1,
            "source-domain evidence is independent of physical cases"
        );
        for kind in ["identity", "max-change", "hard"] {
            let cases: Vec<_> = violations
                .iter()
                .filter_map(|violation| {
                    let origin = match (kind, violation) {
                        (
                            "identity",
                            ProgramDiagnosticV1::ColorIdentity {
                                target_id: 10,
                                field_case,
                                ..
                            },
                        )
                        | (
                            "max-change",
                            ProgramDiagnosticV1::MaxChange {
                                target_id: 10,
                                field_case,
                                ..
                            },
                        )
                        | (
                            "hard",
                            ProgramDiagnosticV1::HardConstraint {
                                constraint_id: 6,
                                field_case,
                                ..
                            },
                        ) => field_case,
                        _ => return None,
                    };
                    Some(origin.expect("visible evidence belongs to a physical field"))
                })
                .collect();
            assert_eq!(
                cases.len(),
                2,
                "{kind} must be evaluated once per local byte"
            );
            assert!(cases.iter().all(|case| case.input_index() == 0), "{kind}");
            assert_eq!(
                cases
                    .iter()
                    .map(|case| case.case_index())
                    .collect::<Vec<_>>(),
                vec![0, 1],
                "{kind}"
            );
        }
    }

    fn wire(cross_report: bool) -> Vec<u8> {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .computed_gray_target(10, 0, 2, Srgb8::new([1; 3]), 1.0)
            .computed_gray_target(20, 3, 5, Srgb8::new([4; 3]), 1.0)
            .surface_input_port(2)
            .surface_input_port(12)
            .solid_paint(3, 10)
            .solid_paint(13, 20)
            .input_surface(4, 2)
            .input_surface(14, 12)
            .source_over_occurrence(5, 3, 4, 64.0, 0.2, 1)
            .source_over_occurrence(15, 13, 14, 64.0, 0.2, 1)
            .wcag22_visible_unary(true, 6, 5, 1)
            .wcag22_visible_unary(true, 16, 15, 1)
            .output(7, 3)
            .output(17, 13);
        if cross_report {
            builder
                .source_over_occurrence(25, 3, 14, 64.0, 0.2, 1)
                .wcag22_visible_unary(false, 26, 25, 1);
        } else {
            builder
                .exact_visible_unary(false, 26, 5, Srgb8::new([255; 3]))
                .exact_visible_unary(false, 36, 15, Srgb8::new([255; 3]));
        }
        builder.finish_assignment(&[10, 20]).expect("wire fixture")
    }

    fn field() -> ProgramDeclaredFieldV1<'static> {
        ProgramDeclaredFieldV1::from_independent_gradients_rgb8(
            &[240, 240, 240, 250, 250, 250],
            &[0, 1],
            &[0, 2, 4],
            &[0.0, 1.0, 0.0, 1.0],
            &[240, 240, 240, 241, 241, 241, 250, 250, 250, 251, 251, 251],
        )
        .expect("two independently varying gray gradients")
    }

    fn cartesian_cases() -> Vec<ProgramScenarioV1> {
        let mut scenarios = Vec::new();
        // These adjacent-byte gray gradients have exactly two encoded values
        // each. All four combinations form an exhaustive finite oracle.
        for first in [240, 241] {
            for second in [250, 251] {
                scenarios.push(ProgramScenarioV1::new(
                    scenarios.len() as u32 + 1,
                    vec![Srgb8::new([first; 3]), Srgb8::new([second; 3])],
                ));
            }
        }
        scenarios
    }

    #[test]
    fn local_report_violations_keep_independent_field_execution_ready() {
        let owner = compile_program_wire(&wire(false)).expect("wire compiles");
        let mut session = owner.instantiate(8).expect("field session");
        let actual = session
            .update_declared_field(1, field())
            .expect("independent fields");
        assert_eq!(actual.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(actual.snapshot_version(), 2);
        assert_eq!(actual.outputs().len(), 2);
        assert_eq!(actual.outputs()[0].source(), Srgb8::new([1; 3]));
        assert_eq!(actual.outputs()[1].source(), Srgb8::new([4; 3]));
        assert_eq!(actual.field_assessments().len(), 2);
        let cells = actual
            .assessment_report()
            .expect("final assessment")
            .cells();
        for id in [26, 36] {
            let reports: Vec<_> = cells
                .iter()
                .filter(|cell| cell.constraint_id() == id)
                .collect();
            assert!(!reports.is_empty(), "report {id} must be evaluated");
            assert!(reports.iter().all(|cell| {
                !cell.hard()
                    && cell.assessment()
                        == ProgramConstraintAssessmentV1::ExactSrgb8 {
                            verdict: ProgramAssessmentVerdictV1::Violation,
                        }
            }));
        }
        let mut oracle = compile_program_wire(&wire(false))
            .expect("oracle program")
            .instantiate(9)
            .expect("finite oracle session");
        let expected = oracle
            .update_observed(1, &cartesian_cases())
            .expect("all input combinations");
        assert_eq!(expected.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(actual.outputs(), expected.outputs());
        assert_eq!(actual.optimality(), expected.optimality());
    }

    #[test]
    fn cross_input_report_is_refused_before_committing_the_field_revision() {
        let owner = compile_program_wire(&wire(true)).expect("wire compiles");
        let mut session = owner.instantiate(8).expect("session");
        assert!(matches!(
            session.update_declared_field(1, field()),
            Err(ProgramRuntimeErrorV1::FieldCorrelationRequired)
        ));
        let observed = session
            .update_observed(1, &cartesian_cases())
            .expect("the refused field must leave revision 1 available");
        assert_eq!(observed.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(observed.revision(), 1);
    }
}

fn blue(id: u32, occurrence: u32, preference: u64) -> Target {
    let anchor = Srgb8::new([0, 103, 244]);
    let mut declaration = ComputedTargetV1::color(
        anchor,
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                .expect("valid blue identity"),
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        },
    );
    declaration.preference = preference;
    Target::new(TargetId::new(id), TargetIntentV1::Computed(declaration))
}

fn observe(revision: u64, input: u32, backdrop: [u8; 3]) -> ObservationUpdateInput {
    let mut result = update(revision, 0);
    result.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
        scenarios: vec![ScenarioInput {
            id: ScenarioId::new(9),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(input),
                ColorSignal::from_srgb8(Srgb8::new(backdrop)),
            )],
        }],
    });
    result
}

fn duplicate_use(declaration: &mut Program<Wcag22Srgb8V1>, target: Target) {
    let target_id = target.id;
    declaration.targets.push(target);
    declaration.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: target_id,
    });
    declaration.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        declaration.occurrences[0].context,
    ));
    declaration
        .constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    declaration
        .outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
}

#[test]
fn independent_roles_execute_only_their_physical_cones() -> Result<(), String> {
    let mut single = None;
    for count in [1_u32, 1000] {
        let mut declaration = program(
            blue(10, 5, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        let context = declaration.occurrences[0].context;
        declaration.targets.clear();
        declaration.paints.clear();
        declaration.occurrences.clear();
        declaration.constraints.hard.clear();
        declaration.outputs.clear();
        for index in 0..count {
            let offset = index * 10;
            declaration
                .targets
                .push(blue(10 + offset, 5 + offset, u64::from(index)));
            declaration.paints.push(Paint::Solid {
                id: PaintId::new(3 + offset),
                target: TargetId::new(10 + offset),
            });
            declaration.occurrences.push(Occurrence::new(
                OccurrenceId::new(5 + offset),
                PaintId::new(3 + offset),
                SurfaceId::new(4),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context,
            ));
            declaration
                .constraints
                .hard
                .push(ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(6 + offset),
                    OccurrenceId::new(5 + offset),
                    Wcag22CriterionV1::Sc143TextDefault,
                ));
            declaration.outputs.push(OutputBinding::new(
                OutputSlotId::new(7 + offset),
                PaintId::new(3 + offset),
            ));
        }
        let owner = compile_assignment(declaration, 65_536)
            .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        crate::appearance::reset_paint_node_evaluation_count();
        crate::composition::reset_source_over_evaluation_count();
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        let work = (
            crate::appearance::paint_node_evaluation_count(),
            crate::composition::source_over_evaluation_count(),
        );
        let verified = session
            .state()
            .current_verified()
            .ok_or("every role must be ready")?
            .computed_for_test();
        assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
        assert_eq!(verified.arena.storage.outputs.len(), count as usize);
        for (index, output) in verified.arena.storage.outputs.iter().enumerate() {
            assert_eq!(output.output.value(), 7 + index as u32 * 10);
            assert_eq!(output.paint.source(), Srgb8::new([0, 103, 244]));
        }
        if let Some((paint, render)) = single {
            assert!(
                work.0 <= paint * count as usize,
                "paint work must scale with independent roles: {work:?}"
            );
            assert!(
                work.1 <= render * count as usize,
                "render work must scale with independent roles: {work:?}"
            );
        } else {
            single = Some(work);
        }
    }
    Ok(())
}

#[test]
fn presentation_cache_clears_only_the_queried_slots_without_allocating() -> Result<(), String> {
    let owner = compile_assignment(
        program(
            blue(10, 5, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        100,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let epoch = &owner.owner_generation;
    let root = epoch
        .graph
        .compile_point_presentation_root(OccurrenceId::new(5))
        .map_err(|error| format!("root: {error:?}"))?;
    let path = epoch
        .graph
        .compile_point_presentation_path(OccurrenceId::new(5), &root)
        .map_err(|error| format!("path: {error:?}"))?;
    let presentations = CompiledPointPresentationsV1 {
        entries: (0..1000)
            .map(|id| CompiledPointPresentationV1 {
                root: PresentationRootId::new(id),
                terminal: OccurrenceId::new(5),
                target: OccurrenceId::new(5),
                absence_release: PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
                path: path.clone(),
            })
            .collect(),
        steps_per_case: 1000,
    };
    let mut cache =
        ProgramPresentationCacheV1::try_new(&presentations).map_err(|()| "cache allocation")?;
    let mut workspace = epoch
        .graph
        .new_workspace()
        .map_err(|error| format!("workspace: {error:?}"))?;
    let evaluation = epoch
        .graph
        .evaluate_admitted_into(&epoch.binding_template, &mut workspace)
        .map_err(|error| format!("physical: {error:?}"))?;
    let (result, allocations) =
        crate::test_support::measured_allocations(|| -> Result<(), String> {
            for (ordinal, phase) in [
                (999, ProgramEvaluationPhaseV1::Hard),
                (5, ProgramEvaluationPhaseV1::ReportOnly),
            ] {
                cache.begin_case(phase);
                let presentation = &presentations.entries[ordinal];
                let first = cache
                    .resolve(&evaluation, ordinal, presentation, None)
                    .map_err(|()| "first resolve")?;
                let cached = cache
                    .resolve(&evaluation, ordinal, presentation, None)
                    .map_err(|()| "cached resolve")?;
                assert_eq!(first.domain, cached.domain);
                assert_eq!(cache.touched, [ordinal]);
            }
            cache.begin_case(ProgramEvaluationPhaseV1::Hard);
            assert_eq!(cache.cleared_domains, 2);
            assert_eq!(cache.replay_counts(), (1, 1));
            assert!(cache.domains.iter().all(Option::is_none));
            Ok(())
        });
    result?;
    assert_eq!(allocations, 0);
    Ok(())
}

#[test]
fn identical_uses_match_the_standalone_color_on_a_shared_input() {
    for backdrop in [[244, 234, 219], [230, 221, 208]] {
        let standalone = compile_assignment(
            program(
                blue(10, 5, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            ),
            65_536,
        )
        .expect("standalone compiles");
        let mut single = standalone
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        single
            .prepare_update(observe(1, 2, backdrop))
            .expect("resolve")
            .commit();
        let expected = single
            .state()
            .current_verified()
            .expect("standalone ready")
            .computed_for_test();
        for ids in [[10, 90], [90, 10]] {
            let mut declaration = program(
                blue(ids[0], 5, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            );
            duplicate_use(&mut declaration, blue(ids[1], 15, 1));
            let combined = compile_assignment(declaration, 65_536).expect("combined compiles");
            let mut session = combined
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            session
                .prepare_update(observe(1, 2, backdrop))
                .expect("resolve")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("both uses ready")
                .computed_for_test();
            assert_eq!(current.values.as_ref(), &[expected.values[0]; 2]);
            assert_eq!(current.search.completion, expected.search.completion);
            assert_eq!(current.search.visited, expected.search.visited * 2);
            assert!(
                current
                    .arena
                    .storage
                    .cells
                    .iter()
                    .all(|cell| !cell.result().is_violation())
            );
        }
    }
}

#[test]
fn independent_raw_targets_keep_their_feasible_incumbent_at_the_same_local_budget() {
    let mut declaration = program(
        target(10, 150, 0, 3, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    duplicate_use(&mut declaration, target(20, 150, 0, 3, 1));
    let owner = compile_assignment(declaration, 3).expect("raw computed program");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 255))
        .expect("resolve")
        .commit();
    let current = session
        .state()
        .current_verified()
        .expect("independent feasible incumbents")
        .computed_for_test();
    // Each singleton has the same first feasible value as the existing
    // standalone four-value regression. Neither search has to be complete.
    assert_eq!(current.values.as_ref(), &[Srgb8::new([0; 3]); 2]);
    assert_eq!(current.search.completion, CompletionV1::BudgetExhausted);
    assert_eq!(current.search.visited, 6);
}

fn island(parent_id: Option<u32>, input_id: u32) -> Program<Wcag22Srgb8V1> {
    let mut declaration = program(
        blue(10, 5, 1),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    declaration.observation_group.surface_input_ports = vec![SurfaceInputPortId::new(input_id)];
    declaration.surfaces[0] = Surface::Input {
        id: SurfaceId::new(4),
        input: SurfaceInputPortId::new(input_id),
    };
    declaration.sources.push(Source::new(
        SourceId::new(30),
        ColorSignal::from_srgb8(Srgb8::new([244, 234, 219])),
    ));
    declaration
        .targets
        .push(Target::fixed(TargetId::new(30), SourceId::new(30)));
    declaration.paints.push(Paint::Solid {
        id: PaintId::new(23),
        target: TargetId::new(30),
    });
    let context = declaration.occurrences[0].context;
    declaration.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(23),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        context,
    ));
    declaration.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(24),
        occurrence: OccurrenceId::new(25),
    });
    declaration.occurrences[0].against = SurfaceId::new(24);
    if let Some(parent_id) = parent_id {
        duplicate_use(&mut declaration, target(parent_id, 150, 0, 255, 0));
        declaration.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(14),
            occurrence: OccurrenceId::new(15),
        });
        declaration
            .occurrences
            .iter_mut()
            .find(|occurrence| occurrence.id == OccurrenceId::new(25))
            .expect("island")
            .against = SurfaceId::new(14);
        // A diagnostic relationship to a different physical backdrop may fail,
        // but must neither couple the selected colors nor reject their frame.
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(35),
            PaintId::new(3),
            SurfaceId::new(14),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context,
        ));
        declaration
            .constraints
            .report_only
            .push(ConstraintInvocation::visible_unary_report_only(
                ConstraintId::new(36),
                OccurrenceId::new(35),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
    }
    declaration
}

#[test]
fn opaque_island_keeps_its_color_when_the_computed_parent_and_input_names_change() {
    let owner = compile_assignment(island(None, 2), 65_536).expect("standalone island");
    let mut standalone = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    standalone
        .prepare_update(update(1, 255))
        .expect("resolve")
        .commit();
    let expected = standalone
        .state()
        .current_verified()
        .expect("ready")
        .computed_for_test()
        .values[0];
    for (parent_id, input_id) in [(20, 2), (1, 901)] {
        let owner = compile_assignment(island(Some(parent_id), input_id), 65_536)
            .expect("island over computed parent");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        let mut previous_parent = None;
        for (index, backdrop) in [[255; 3], [0; 3]].into_iter().enumerate() {
            session
                .prepare_update(observe(index as u64 + 1, input_id, backdrop))
                .expect("resolve changed parent")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("entire frame remains ready")
                .computed_for_test();
            assert_eq!(current.values[1], expected);
            if let Some(previous) = previous_parent {
                assert_ne!(current.values[0], previous, "the parent really adapted");
            }
            previous_parent = Some(current.values[0]);
            assert!(
                current.arena.storage.cells.iter().any(|cell| {
                    cell.constraint == ConstraintId::new(36) && cell.result().is_violation()
                }),
                "diagnostic-only failure is retained in the final report"
            );
        }
    }
}

fn fixed_constraint() -> Program<Wcag22Srgb8V1> {
    let mut declaration = program(
        target(10, 150, 0, 255, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    declaration.sources.push(Source::new(
        SourceId::new(30),
        ColorSignal::from_srgb8(Srgb8::new([255; 3])),
    ));
    duplicate_use(
        &mut declaration,
        Target::fixed(TargetId::new(30), SourceId::new(30)),
    );
    declaration
}

#[test]
fn composed_objectives_follow_their_own_paints_across_a_fixed_binding() {
    for order in [[10, 40], [40, 10]] {
        let mut declaration = fixed_constraint();
        declaration.targets.push(target(40, 200, 190, 210, 1));
        let paint = PaintId::new(23);
        declaration.paints.push(Paint::Solid {
            id: paint,
            target: TargetId::new(40),
        });
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(25),
            paint,
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            declaration.occurrences[0].context,
        ));
        declaration
            .outputs
            .push(OutputBinding::new(OutputSlotId::new(27), paint));
        declaration.assignment = Some(ComputedSelectionPolicyV1::AuthoredComposition {
            legacy_reference_order: order.map(TargetId::new).to_vec(),
        });
        let compiled = declaration
            .compile()
            .expect("valid mixed fixed and computed declarations");
        let mut session = compiled
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled fixture creates a session");
        session
            .prepare_update(update(1, 0))
            .expect("complete observed context")
            .commit();
        let current = session
            .state()
            .current_verified()
            .expect("committed paints are available")
            .computed_for_test();
        let paints: Vec<_> = current
            .arena
            .storage
            .outputs
            .iter()
            .map(|out| (out.output.value(), out.paint.source()))
            .collect();
        assert_eq!(
            paints,
            [
                (7, Srgb8::new([150; 3])),
                (17, Srgb8::new([255; 3])),
                (27, Srgb8::new([200; 3]))
            ]
        );
    }
}

#[test]
fn fixed_only_failure_rejects_the_frame_before_search_and_keeps_the_previous_snapshot() {
    let owner = compile_assignment(fixed_constraint(), 1).expect("fixed and computed program");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 0))
        .expect("initial resolve")
        .commit();
    assert!(session.state().current_verified().is_some());
    session
        .prepare_update(update(2, 255))
        .expect("fixed failure")
        .commit();
    let SessionState::Failed {
        cause: ProgramViolationOutcomeV1::Computed(cause),
        previous,
    } = session.state()
    else {
        panic!("impossible fixed condition rejects the whole frame")
    };
    assert_eq!(cause.search.visited, 1);
    assert_eq!(cause.search.completion, CompletionV1::Complete);
    assert_eq!(cause.diagnostics.search_budget, 2);
    assert!(cause.diagnostics.witness.is_some());
    assert!(session.state().current_verified().is_none());
    assert_eq!(
        previous
            .as_ref()
            .expect("preserved snapshot")
            .computed_for_test()
            .values[0],
        Srgb8::new([150; 3])
    );
}

#[test]
fn passing_fixed_conditions_do_not_invent_an_anchor_witness_at_zero_search_budget() {
    let owner = compile_assignment(fixed_constraint(), 0).expect("zero search budget");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 0))
        .expect("fixed check")
        .commit();
    let SessionState::Incomplete { cause, .. } = session.state() else {
        panic!("no computed selection was attempted")
    };
    assert_eq!(cause.search.visited, 1);
    assert_eq!(cause.diagnostics.search_budget, 1);
    assert!(cause.diagnostics.witness.is_none());
}

fn visible_relation(with_parent: bool) -> Program<Wcag22Srgb8V1> {
    let mut declaration = program(
        target(10, 150, 0, 3, 1),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    declaration.sources.push(Source::new(
        SourceId::new(30),
        ColorSignal::from_srgb8(Srgb8::new([0; 3])),
    ));
    declaration
        .targets
        .push(Target::fixed(TargetId::new(30), SourceId::new(30)));
    declaration.paints.push(Paint::Solid {
        id: PaintId::new(23),
        target: TargetId::new(30),
    });
    declaration.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(23),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        declaration.occurrences[0].context,
    ));
    declaration.constraints.hard = vec![ConstraintInvocation::exact_visible_relation_hard(
        ConstraintId::new(6),
        DirectedRelationV1::try_new(OccurrenceId::new(25), vec![OccurrenceId::new(5)])
            .expect("valid relation endpoints"),
    )];
    if with_parent {
        duplicate_use(&mut declaration, target(20, 150, 0, 3, 0));
        declaration.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(14),
            occurrence: OccurrenceId::new(15),
        });
        declaration.occurrences[0].against = SurfaceId::new(14);
    }
    declaration
}

#[test]
fn visible_pair_separation_is_symmetric_and_selects_both_computed_endpoints() {
    let pair = |reverse: bool, minimum: f64| {
        let mut p = program(
            target(10, 61, 60, 62, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        duplicate_use(&mut p, target(20, 61, 60, 62, 1));
        let (a, b) = if reverse { (15, 5) } else { (5, 15) };
        p.constraints.hard = vec![ConstraintInvocation::pair_separation(
            ConstraintId::new(6),
            DirectedRelationV1::try_new(OccurrenceId::new(a), vec![OccurrenceId::new(b)])
                .expect("different uses"),
            VisibleSeparationBoundsV1::new(minimum, 2.0).expect("valid interval"),
        )];
        compile_assignment(p, 1024).expect("two computed endpoints")
    };
    // Only the outer pair fits. Adjusting either source alone never satisfies it.
    let distance =
        |a: u8, b: u8| squared_distance(lab(Srgb8::new([a; 3])), lab(Srgb8::new([b; 3]))).sqrt();
    let threshold = distance(60, 62);
    assert!(threshold > distance(60, 61).max(distance(61, 62)));
    let owner = pair(false, threshold);
    let reversed = pair(true, threshold);
    assert_eq!(owner.content_identity(), reversed.content_identity());
    assert_ne!(
        owner.content_identity(),
        pair(false, 0.0).content_identity()
    );
    for compiled in [owner, reversed] {
        let mut session = compiled
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_update(update(1, 255))
            .expect("resolve")
            .commit();
        let ready = session.state().current_verified().expect("feasible pair");
        let values = &ready.computed_for_test().values;
        assert_eq!(
            distance(values[0].bytes()[0], values[1].bytes()[0]),
            threshold
        );
        assert!(values.iter().all(|rgb| rgb.bytes()[0] != 61));
    }
}

#[test]
fn opaque_visible_relation_endpoints_do_not_depend_on_their_backdrop() {
    let owner = compile_assignment(visible_relation(false), 3).expect("standalone relation");
    let mut standalone = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    standalone
        .prepare_update(update(1, 255))
        .expect("resolve")
        .commit();
    let expected = standalone
        .state()
        .current_verified()
        .expect("standalone ready")
        .computed_for_test();
    assert_eq!(expected.values[0], Srgb8::new([0; 3]));

    let owner =
        compile_assignment(visible_relation(true), 3).expect("relation over computed parent");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 255))
        .expect("resolve")
        .commit();
    let current = session
        .state()
        .current_verified()
        .expect("both independent solutions ready")
        .computed_for_test();
    assert_eq!(current.values.as_ref(), &[expected.values[0]; 2]);
    assert_eq!(current.search.visited, expected.search.visited * 2);
    assert_eq!(current.search.completion, CompletionV1::BudgetExhausted);
    assert!(
        current
            .arena
            .storage
            .cells
            .iter()
            .all(|cell| !cell.result().is_violation())
    );
}

fn local_scenarios(relevant: u32, foreign: u32) -> Program<Wcag22Srgb8V1> {
    let mut target = blue(10, 5, 0);
    let TargetIntentV1::Computed(value) = &mut target.intent else {
        unreachable!()
    };
    value.intent.as_mut().expect("color intent").tone = Some(
        ToneCurveV1::new(relevant, vec![[0.0, 0.22], [0.5, 0.58], [1.0, 0.97]])
            .expect("valid tone curve"),
    );
    let mut declaration = program(
        target,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22Srgb8V1,
    );
    declaration.observation_group.surface_input_ports = vec![
        SurfaceInputPortId::new(relevant),
        SurfaceInputPortId::new(foreign),
    ];
    declaration.surfaces[0] = Surface::Input {
        id: SurfaceId::new(4),
        input: SurfaceInputPortId::new(relevant),
    };
    declaration.surfaces.push(Surface::Input {
        id: SurfaceId::new(24),
        input: SurfaceInputPortId::new(foreign),
    });
    declaration.sources.push(Source::new(
        SourceId::new(30),
        ColorSignal::from_srgb8(Srgb8::new([244, 234, 219])),
    ));
    declaration
        .targets
        .push(Target::fixed(TargetId::new(30), SourceId::new(30)));
    declaration.paints.push(Paint::Solid {
        id: PaintId::new(23),
        target: TargetId::new(30),
    });
    declaration.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(23),
        SurfaceId::new(24),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        declaration.occurrences[0].context,
    ));
    declaration
        .outputs
        .push(OutputBinding::new(OutputSlotId::new(27), PaintId::new(23)));
    declaration
}

#[test]
fn foreign_inputs_cannot_reorder_or_duplicate_the_local_search_contexts() {
    let backgrounds = [[244, 234, 219], [230, 221, 208]];
    let mut expected = None;
    for (relevant, foreign) in [(2, 1), (1, 2)] {
        let owner = compile_assignment(local_scenarios(relevant, foreign), 65_536)
            .expect("independent observed inputs");
        let variants = [
            vec![(0, 0), (255, 1)],
            vec![(255, 0), (0, 1)],
            vec![(255, 0), (0, 1), (128, 0)],
        ];
        for cases in variants {
            // A fresh session tests search itself, without committed reuse
            // hiding a dependency on foreign case order or multiplicity.
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            let mut observed = update(1, 0);
            observed.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: cases
                    .iter()
                    .enumerate()
                    .map(|(index, &(other, local))| ScenarioInput {
                        id: ScenarioId::new(50 - index as u32),
                        bindings: vec![
                            SurfaceInputBinding::new(
                                SurfaceInputPortId::new(relevant),
                                ColorSignal::from_srgb8(Srgb8::new(backgrounds[local])),
                            ),
                            SurfaceInputBinding::new(
                                SurfaceInputPortId::new(foreign),
                                ColorSignal::from_srgb8(Srgb8::new([other; 3])),
                            ),
                        ],
                    })
                    .collect(),
            });
            session
                .prepare_update(observed)
                .expect("resolve all cases")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("usable shared color")
                .computed_for_test();
            let result = (
                current.values[0],
                current.search.completion,
                current.search.visited,
            );
            if let Some(expected) = expected {
                assert_eq!(result, expected, "only the local contexts may guide search");
            } else {
                expected = Some(result);
            }
            assert_eq!(
                current.arena.storage.cells.len(),
                cases.len(),
                "all original physical cases remain in the final hard assessment"
            );
            assert!(
                current
                    .arena
                    .storage
                    .cells
                    .iter()
                    .all(|cell| !cell.result().is_violation())
            );
        }
    }
}
