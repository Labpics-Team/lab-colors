use super::tests::{compile_assignment, program, target, update};
use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::observation::{
    ObservationPayloadInput, ObservationUpdateInput, ObservedScenarioSetInput, ScenarioId,
    ScenarioInput, SurfaceInputBinding,
};
use crate::session::SessionState;

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
