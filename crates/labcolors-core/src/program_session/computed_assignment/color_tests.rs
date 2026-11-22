use super::tests::{compile_assignment, program, rgb, update};
use super::*;
use crate::color_intent::{ColorIdentityDomainV1, IdentityOptionsV1, ToneCurveV1};
use crate::observation::{
    ObservationPayloadInput, ObservedScenarioSetInput, ScenarioId, ScenarioInput,
    SurfaceInputBinding,
};

fn color(source: [u8; 3], max_change: f64, tone: Option<ToneCurveV1>) -> Target {
    let source = Srgb8::new(source);
    let mut value = ComputedTargetV1::color(
        source,
        max_change,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                .unwrap(),
            appearance: Some(OccurrenceId::new(5)),
            tone,
        },
    );
    value.preference = 0;
    Target::new(TargetId::new(10), TargetIntentV1::Computed(value))
}

fn unconstrained(target: Target) -> Program<Wcag22Srgb8V1> {
    let mut result = program(target, Wcag22CriterionV1::Sc143TextDefault, Wcag22Srgb8V1);
    result.constraints = ConstraintSet::new(vec![], vec![]);
    result
}

#[test]
fn opaque_blue_changes_locally_and_keeps_color_character() {
    let source = Srgb8::new([0, 103, 244]);
    let identity = ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default()).unwrap();
    let owner = compile_assignment(
        program(
            color(source.bytes(), 2.0, None),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        65_536,
    )
    .unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    session.prepare_update(update(1, 255)).unwrap().commit();
    assert_eq!(rgb(&session), source);
    session.prepare_update(update(2, 0)).unwrap().commit();
    let dark = rgb(&session);
    assert_ne!(dark, source);
    assert!(identity.contains(dark));
    let verified = session
        .state()
        .current_verified()
        .unwrap()
        .computed_for_test();
    assert!(
        verified
            .arena
            .storage
            .cells
            .iter()
            .all(|cell| !cell.result().is_violation())
    );
    assert_eq!(
        verified.arena.storage.outputs[0].paint.opacity().value(),
        1.0
    );
}

#[test]
fn client_tone_curve_changes_the_whole_gray_surface_without_theme_names() {
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.15], [1.0, 0.96]]).unwrap();
    let owner = compile_assignment(
        unconstrained(color([128; 3], 2.0, Some(curve.clone()))),
        1024,
    )
    .unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    for (revision, backdrop) in [(1, 0), (2, 255)] {
        session
            .prepare_update(update(revision, backdrop))
            .unwrap()
            .commit();
        assert_eq!(
            rgb(&session),
            curve.preferred_color(Srgb8::new([128; 3]), Srgb8::new([backdrop; 3]))
        );
        assert_eq!(
            session
                .state()
                .current_verified()
                .unwrap()
                .computed_for_test()
                .optimality(),
            ComputedOptimalityV1::Exact
        );
    }
}

#[test]
fn neutral_material_compensates_colored_backdrop_in_its_source() {
    let mut declaration = unconstrained(color([128; 3], 2.0, None));
    declaration
        .opacities
        .push(OpacityInput::new(OpacityInputId::new(11), 0.75));
    declaration.paints.push(Paint::Opacity {
        id: PaintId::new(12),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(11),
    });
    declaration.occurrences[0].subject = PaintId::new(12);
    declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(12));
    let owner = compile_assignment(declaration, 4096).unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    let mut observed = update(1, 0);
    observed.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
        scenarios: vec![ScenarioInput {
            id: ScenarioId::new(9),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new([64, 128, 192])),
            )],
        }],
    });
    session.prepare_update(observed).unwrap().commit();
    let source = rgb(&session);
    assert!(
        !source.is_achromatic(),
        "source compensates the actual colored material backdrop"
    );
    let visible = std::array::from_fn(|i| {
        crate::composition::source_over_channel_srgb8(source.bytes()[i], 0.75, [64, 128, 192][i])
    });
    assert_eq!(visible, [128; 3]);
}

#[test]
fn max_change_is_relative_to_authored_color_even_with_a_distant_tone_preference() {
    let source = Srgb8::new([128; 3]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.95], [1.0, 0.95]]).unwrap();
    let owner = compile_assignment(
        unconstrained(color(source.bytes(), 0.05, Some(curve))),
        2048,
    )
    .unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    session.prepare_update(update(1, 0)).unwrap().commit();
    let output = rgb(&session);
    assert!(squared_distance(lab(output), lab(source)) <= 0.05 * 0.05);
    assert!(lab(output)[0] > lab(source)[0]);
}

#[test]
fn multi_case_tone_optimizes_worst_case_and_matches_all_gray_values() {
    let source = Srgb8::new([128; 3]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.2], [1.0, 0.8]]).unwrap();
    let preferred = [0, 255].map(|g| lab(curve.preferred_color(source, Srgb8::new([g; 3]))));
    let oracle = (0..=255_u8)
        .map(|g| Srgb8::new([g; 3]))
        .min_by(|a, b| {
            let cost = |rgb| {
                preferred
                    .iter()
                    .map(|p| squared_distance(lab(rgb), *p))
                    .fold(0.0_f64, f64::max)
            };
            cost(*a).total_cmp(&cost(*b)).then(a.cmp(b))
        })
        .unwrap();
    let owner =
        compile_assignment(unconstrained(color(source.bytes(), 2.0, Some(curve))), 4096).unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    for (revision, backgrounds) in [(1, [0, 255]), (2, [255, 0])] {
        let mut observed = update(revision, 0);
        observed.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: backgrounds
                .iter()
                .enumerate()
                .map(|(i, g)| ScenarioInput {
                    id: ScenarioId::new(20 + i as u32),
                    bindings: vec![SurfaceInputBinding::new(
                        SurfaceInputPortId::new(2),
                        ColorSignal::from_srgb8(Srgb8::new([*g; 3])),
                    )],
                })
                .collect(),
        });
        session.prepare_update(observed).unwrap().commit();
        assert_eq!(rgb(&session), oracle);
        assert_eq!(
            session
                .state()
                .current_verified()
                .unwrap()
                .computed_for_test()
                .optimality(),
            ComputedOptimalityV1::Exact
        );
    }
}
