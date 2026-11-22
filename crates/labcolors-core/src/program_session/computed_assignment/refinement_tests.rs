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
fn distinct_identical_uses_stay_consistent_on_the_same_computed_parent() -> Result<(), String> {
    for opacity in [1.0, 0.5] {
        for background in [[244, 234, 219], [230, 221, 208], [0; 3], [255; 3]] {
            identical_material_uses(opacity, background)?;
        }
    }
    Ok(())
}

fn material_parent(
    mut declaration: Program<Wcag22Srgb8V1>,
    opacity: f64,
) -> Program<Wcag22Srgb8V1> {
    declaration
        .opacities
        .push(OpacityInput::new(OpacityInputId::new(91), opacity));
    declaration.paints.push(Paint::Opacity {
        id: PaintId::new(93),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(91),
    });
    declaration.occurrences[0].subject = PaintId::new(93);
    declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(93));
    declaration
}

fn identical_material_uses(opacity: f64, background: [u8; 3]) -> Result<(), String> {
    let owner = compile_assignment(
        material_parent(twins(Wcag22CriterionV1::Sc143TextDefault), opacity),
        65_536,
    )
    .map_err(|error| format!("shared computed parent: {error:?}"))?;
    let mut standalone = twins(Wcag22CriterionV1::Sc143TextDefault);
    standalone.targets.truncate(2);
    standalone.paints.truncate(2);
    standalone.occurrences.truncate(2);
    standalone.outputs.truncate(2);
    standalone.constraints.hard.truncate(1);
    let standalone = compile_assignment(material_parent(standalone, opacity), 65_536)
        .map_err(|error| format!("one child: {error:?}"))?;
    let mut local = standalone
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("local session: {error:?}"))?;
    local
        .prepare_update(observed(background))
        .map_err(|error| format!("local resolve: {error:?}"))?
        .commit();
    let local = local
        .state()
        .current_verified()
        .ok_or("one child must be ready")?
        .computed_for_test();
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("session: {error:?}"))?;
    session
        .prepare_update(observed(background))
        .map_err(|error| format!("resolve: {error:?}"))?
        .commit();
    let current = session
        .state()
        .current_verified()
        .ok_or("whole composition ready")?
        .computed_for_test();
    assert_eq!(
        current.values[0], local.values[0],
        "the selected parent must be the same at opacity {opacity}, background {background:?}"
    );
    assert_eq!(
        current.values[1], local.values[1],
        "the same chosen parent leaves the same independent local problem"
    );
    assert_eq!(
        current.values[1], current.values[2],
        "equivalent uses must not disagree because DFS ended between them"
    );
    if opacity == 1.0 || background == [244, 234, 219] {
        assert_eq!(current.values[0], Srgb8::new([244, 234, 219]));
        let anchor = lab(Srgb8::new([0, 103, 244]));
        assert!(
            squared_distance(lab(current.values[1]), anchor)
                < squared_distance(lab(Srgb8::new([53, 95, 127])), anchor),
            "restore the role's character instead of copying the washed-out incumbent"
        );
    }
    if opacity == 0.5 && background == [0; 3] {
        // The light visible anchor is unreachable through this material. Its
        // best local source nevertheless permits the complete composition;
        // adding an equivalent child must not consume that prefix's search.
        let mut prefix = twins(Wcag22CriterionV1::Sc143TextDefault);
        prefix.targets.truncate(1);
        prefix.paints.truncate(1);
        prefix.occurrences.truncate(1);
        prefix.outputs.truncate(1);
        prefix.constraints.hard.clear();
        let prefix = compile_assignment(material_parent(prefix, opacity), 65_536)
            .map_err(|error| format!("parent only: {error:?}"))?;
        let mut prefix_session = prefix
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("parent session: {error:?}"))?;
        prefix_session
            .prepare_update(observed(background))
            .map_err(|error| format!("parent resolve: {error:?}"))?
            .commit();
        let prefix = prefix_session
            .state()
            .current_verified()
            .ok_or("parent-only result must be ready")?
            .computed_for_test();
        assert_eq!(prefix.search.completion, CompletionV1::Complete);
        let anchor = lab(Srgb8::new([244, 234, 219]));
        let first_score = |source: Srgb8| {
            let visible =
                Srgb8::new(source.bytes().map(|channel| {
                    crate::composition::source_over_channel_srgb8(channel, opacity, 0)
                }));
            squared_distance(lab(visible), anchor)
        };
        assert_eq!(
            first_score(current.values[0]),
            first_score(prefix.values[0]),
            "the feasible local prefix optimum remains available to the whole composition"
        );
    }
    if opacity == 1.0 {
        assert_eq!(current.search.completion, CompletionV1::Complete);
    }
    assert!(current.search.visited <= 4 * 65_536);
    assert!(local.search.visited <= 3 * 65_536);
    assert!(
        current
            .arena
            .storage
            .cells
            .iter()
            .all(|cell| !cell.result().is_violation())
    );
    Ok(())
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
    assert!(current.search.visited <= 4 * 65_536);
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
