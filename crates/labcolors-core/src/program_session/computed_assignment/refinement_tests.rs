use super::tests::{compile_assignment, program, update};
use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::observation::{
    ObservationPayloadInput, ObservationUpdateInput, ObservedScenarioSetInput, ScenarioId,
    ScenarioInput, SurfaceInputBinding,
};

fn observed(rgb: [u8; 3]) -> ObservationUpdateInput {
    let mut result = update(1, 0);
    result.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
        scenarios: vec![ScenarioInput {
            id: ScenarioId::new(9),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new(rgb)),
            )],
        }],
    });
    result
}

fn role(id: u32, occurrence: u32, preference: u64, source: [u8; 3]) -> Target {
    let source = Srgb8::new(source);
    let mut declaration = ComputedTargetV1::color(
        source,
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                .expect("valid identity"),
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        },
    );
    declaration.preference = preference;
    Target::new(TargetId::new(id), TargetIntentV1::Computed(declaration))
}

fn twins(criterion: Wcag22CriterionV1) -> Program<Wcag22Srgb8V1> {
    let mut declaration = program(role(10, 5, 0, [244, 234, 219]), criterion, Wcag22Srgb8V1);
    declaration.constraints = ConstraintSet::new(vec![], vec![]);
    declaration.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(14),
        occurrence: OccurrenceId::new(5),
    });
    for offset in [10, 20] {
        declaration.targets.push(role(
            10 + offset,
            5 + offset,
            u64::from(offset / 10),
            [0, 103, 244],
        ));
        declaration.paints.push(Paint::Solid {
            id: PaintId::new(3 + offset),
            target: TargetId::new(10 + offset),
        });
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(5 + offset),
            PaintId::new(3 + offset),
            SurfaceId::new(14),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            declaration.occurrences[0].context,
        ));
        declaration
            .constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6 + offset),
                OccurrenceId::new(5 + offset),
                criterion,
            ));
        declaration.outputs.push(OutputBinding::new(
            OutputSlotId::new(7 + offset),
            PaintId::new(3 + offset),
        ));
    }
    declaration
}

#[test]
fn distinct_identical_uses_stay_consistent_on_the_same_computed_parent() {
    let owner = compile_assignment(twins(Wcag22CriterionV1::Sc143TextDefault), 65_536)
        .expect("shared computed parent");
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
        .expect("whole composition ready")
        .computed_for_test();
    assert_eq!(current.values[0], Srgb8::new([244, 234, 219]));
    assert_eq!(
        current.values[1], current.values[2],
        "equivalent uses must not disagree because DFS ended between them"
    );
    let anchor = lab(Srgb8::new([0, 103, 244]));
    assert!(
        squared_distance(lab(current.values[1]), anchor)
            < squared_distance(lab(Srgb8::new([53, 95, 127])), anchor),
        "restore the role's character instead of copying the washed-out incumbent"
    );
    assert!(current.search.visited <= 65_536);
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
fn different_downstream_conditions_keep_distinct_uses_free_to_differ() {
    let mut declaration = twins(Wcag22CriterionV1::Sc1411UiComponentOrState);
    for (offset, source, backdrop) in [(30, [0; 3], 15), (40, [255; 3], 25)] {
        let id = 10 + offset;
        declaration.sources.push(Source::new(
            SourceId::new(id),
            ColorSignal::from_srgb8(Srgb8::new(source)),
        ));
        declaration
            .targets
            .push(Target::fixed(TargetId::new(id), SourceId::new(id)));
        declaration.paints.push(Paint::Solid {
            id: PaintId::new(3 + offset),
            target: TargetId::new(id),
        });
        declaration.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(4 + offset),
            occurrence: OccurrenceId::new(backdrop),
        });
        declaration.occurrences.push(Occurrence::new(
            OccurrenceId::new(5 + offset),
            PaintId::new(3 + offset),
            SurfaceId::new(4 + offset),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            declaration.occurrences[0].context,
        ));
        declaration
            .constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6 + offset),
                OccurrenceId::new(5 + offset),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
    }
    let owner = compile_assignment(declaration, 65_536).expect("different dependent labels");
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
        .expect("both branches remain feasible")
        .computed_for_test();
    assert_ne!(
        current.values[1], current.values[2],
        "one shared role does not impose an equality constraint on distinct uses"
    );
    assert_eq!(current.values[2], Srgb8::new([0, 103, 244]));
    assert!(current.search.visited <= 65_536);
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
fn an_already_optimal_visible_color_keeps_its_authored_source() {
    for source in [[255; 3], [0, 103, 244]] {
        for opacity in [0.0, 0.5, 1.0] {
            let mut declaration = program(
                role(10, 5, 0, source),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            );
            declaration.constraints = ConstraintSet::new(vec![], vec![]);
            declaration
                .opacities
                .push(OpacityInput::new(OpacityInputId::new(11), opacity));
            declaration.paints.push(Paint::Opacity {
                id: PaintId::new(13),
                source: PaintId::new(3),
                opacity: OpacityInputId::new(11),
            });
            declaration.occurrences[0].subject = PaintId::new(13);
            declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(13));
            let owner =
                compile_assignment(declaration, 65_536).expect("valid unchanged appearance");
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            session
                .prepare_update(observed(source))
                .expect("resolve")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("valid authored color")
                .computed_for_test();
            assert_eq!(current.values[0], Srgb8::new(source));
            assert_eq!(current.search.completion, CompletionV1::AuthoredExact);
            assert_eq!(
                current.search.visited, 1,
                "no search is needed after proving the least possible objective and unchanged source"
            );
        }
    }
}

#[test]
fn equal_visible_candidates_preserve_source_even_away_from_the_preferred_color() {
    let source = [0, 103, 244];
    let mut target = role(10, 5, 0, source);
    let TargetIntentV1::Computed(declaration) = &mut target.intent else {
        unreachable!()
    };
    declaration.domain = AssignmentDomainV1::Cube {
        lower: [0, 102, 243],
        upper: source,
    };
    let mut declaration = program(target, Wcag22CriterionV1::Sc143TextDefault, Wcag22Srgb8V1);
    declaration.constraints = ConstraintSet::new(vec![], vec![]);
    declaration
        .opacities
        .push(OpacityInput::new(OpacityInputId::new(11), 0.5));
    declaration.paints.push(Paint::Opacity {
        id: PaintId::new(13),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(11),
    });
    declaration.occurrences[0].subject = PaintId::new(13);
    let owner = compile_assignment(declaration, 512).expect("finite visible tie");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(observed([0, 101, 242]))
        .expect("resolve")
        .commit();
    let current = session
        .state()
        .current_verified()
        .expect("bounded material ready")
        .computed_for_test();
    // Independent integer source-over: all four candidates round to [0,102,243].
    // None has zero visible distance to the blue anchor; source flags break the tie.
    for green in 102..=103_u16 {
        for blue in 243..=244_u16 {
            assert_eq!((green + 101).div_ceil(2), 102);
            assert_eq!((blue + 242).div_ceil(2), 243);
        }
    }
    assert_eq!(current.values[0], Srgb8::new(source));
    assert_eq!(current.search.completion, CompletionV1::Complete);
}
