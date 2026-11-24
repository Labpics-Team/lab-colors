use super::tests::{compile_assignment, program, rgb, update};
use super::*;
use crate::color_intent::{ColorIdentityDomainV1, IdentityOptionsV1, ToneCurveV1};
use crate::observation::{
    ObservationPayloadInput, ObservedScenarioSetInput, ScenarioId, ScenarioInput,
    SurfaceInputBinding,
};
use crate::session::SessionState;

fn color(source: [u8; 3], max_change: f64, tone: Option<ToneCurveV1>) -> Result<Target, String> {
    let source = Srgb8::new(source);
    let mut value = ComputedTargetV1::color(
        source,
        max_change,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                .map_err(|error| format!("color identity: {error:?}"))?,
            appearance: Some(OccurrenceId::new(5)),
            tone,
        },
    );
    value.preference = 0;
    Ok(Target::new(
        TargetId::new(10),
        TargetIntentV1::Computed(value),
    ))
}

fn unconstrained(target: Target) -> Program<Wcag22Srgb8V1> {
    let mut result = program(target, Wcag22CriterionV1::Sc143TextDefault, Wcag22Srgb8V1);
    result.constraints = ConstraintSet::new(vec![], vec![]);
    result
}

#[test]
fn a_feasible_authored_point_stays_unfinished_when_its_budget_expires_without_a_proof()
-> Result<(), String> {
    let tone = ToneCurveV1::new(2, vec![[0.0, 0.9], [1.0, 0.9]])
        .map_err(|error| format!("tone: {error:?}"))?;
    let mut declared = unconstrained(color([128; 3], 2.0, Some(tone))?);
    declared.assignment = Some(ComputedSelectionPolicyV1::AuthoredComposition {
        legacy_reference_order: vec![TargetId::new(10)],
    });
    declared.computed_budget = Some(1);
    let compiled = declared
        .compile()
        .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = compiled
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 255))
        .map_err(|error| format!("light update: {error:?}"))?
        .commit();
    let result = session
        .state()
        .current_verified()
        .ok_or("feasible authored color should yield a snapshot")?
        .computed_for_test();
    assert_eq!(result.values.as_ref(), &[Srgb8::new([128; 3])]);
    assert_eq!(result.search.completion, CompletionV1::BudgetExhausted);
    assert_eq!(result.optimality(), ComputedOptimalityV1::Unfinished);
    Ok(())
}

#[test]
fn opaque_blue_changes_locally_and_keeps_color_character() -> Result<(), String> {
    let source = Srgb8::new([0, 103, 244]);
    let identity = ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
        .map_err(|error| format!("blue identity: {error:?}"))?;
    let owner = compile_assignment(
        program(
            color(source.bytes(), 2.0, None)?,
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        65_536,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 255))
        .map_err(|error| format!("light update: {error:?}"))?
        .commit();
    assert_eq!(rgb(&session), source);
    session
        .prepare_update(update(2, 0))
        .map_err(|error| format!("dark update: {error:?}"))?
        .commit();
    let dark = rgb(&session);
    assert_ne!(dark, source);
    assert!(identity.contains(dark));
    let verified = session
        .state()
        .current_verified()
        .ok_or("the dark blue tuple must be ready")?
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
    Ok(())
}

#[test]
fn repeated_context_reuses_blue_selection_with_fresh_physical_verification() -> Result<(), String> {
    let evaluator = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let owner = compile_assignment(
        program(
            color([0, 103, 244], 2.0, None)?,
            Wcag22CriterionV1::Sc143TextDefault,
            evaluator.clone(),
        ),
        65_536,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 0))
        .map_err(|error| format!("initial dark context: {error:?}"))?
        .commit();
    let selected = rgb(&session);
    let calls = evaluator.calls().len();
    assert!(
        calls > 1,
        "the initial blue needs search and final verification"
    );
    session
        .prepare_update(update(2, 0))
        .map_err(|error| format!("repeated dark context: {error:?}"))?
        .commit();
    assert_eq!(rgb(&session), selected);
    assert_eq!(evaluator.calls().len(), calls + 1);
    Ok(())
}

#[test]
fn client_tone_curve_changes_the_whole_gray_surface_without_theme_names() -> Result<(), String> {
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.15], [1.0, 0.96]])
        .map_err(|error| format!("tone curve: {error:?}"))?;
    let owner = compile_assignment(
        unconstrained(color([128; 3], 2.0, Some(curve.clone()))?),
        1024,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let mut actual = Vec::new();
    for (revision, backdrop) in [(1, 0), (2, 255)] {
        session
            .prepare_update(update(revision, backdrop))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        assert_eq!(
            rgb(&session),
            curve.preferred_color(Srgb8::new([128; 3]), Srgb8::new([backdrop; 3]))
        );
        assert_eq!(
            session
                .state()
                .current_verified()
                .ok_or("the contextual gray tuple must be ready")?
                .computed_for_test()
                .optimality(),
            ComputedOptimalityV1::Exact
        );
        actual.push(rgb(&session));
    }
    assert!(actual[1].bytes()[0] > actual[0].bytes()[0]);
    Ok(())
}

#[test]
fn tone_preserves_the_full_authored_preference_before_gamut_mapping() -> Result<(), String> {
    let source = Srgb8::new([0, 103, 244]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.9], [1.0, 0.9]])
        .map_err(|error| format!("tone curve: {error:?}"))?;
    let backdrop = Srgb8::new([255; 3]);
    let seed = curve.preferred_color(source, backdrop);
    assert_eq!(seed, Srgb8::new([204, 223, 255]));
    let more_chromatic = Srgb8::new([204, 222, 255]);
    let identity = ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
        .map_err(|error| format!("identity: {error:?}"))?;
    assert!(identity.contains(seed) && identity.contains(more_chromatic));
    let [_, a, b] = lab(source);
    let preferred = [curve.preferred_lightness(backdrop), a, b];
    assert!(
        squared_distance(lab(more_chromatic), preferred) < squared_distance(lab(seed), preferred),
        "a slightly darker color better preserves the complete authored preference"
    );
    let mut target = color(source.bytes(), 2.0, Some(curve))?;
    let TargetIntentV1::Computed(value) = &mut target.intent else {
        return Err("computed fixture".into());
    };
    value.domain = AssignmentDomainV1::Cube {
        lower: more_chromatic.bytes(),
        upper: seed.bytes(),
    };
    let owner = compile_assignment(unconstrained(target), 128)
        .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 255))
        .map_err(|error| format!("update: {error:?}"))?
        .commit();
    assert_eq!(rgb(&session), more_chromatic);
    assert_eq!(
        session
            .state()
            .current_verified()
            .ok_or("ready tuple")?
            .computed_for_test()
            .optimality(),
        ComputedOptimalityV1::Exact
    );
    Ok(())
}

#[test]
fn neutral_opaque_search_intersects_the_authored_domain() -> Result<(), String> {
    for source in [[140; 3], [140, 141, 140]] {
        let mut target = color([128; 3], 2.0, None)?;
        let TargetIntentV1::Computed(value) = &mut target.intent else {
            return Err("computed fixture".into());
        };
        value.domain = AssignmentDomainV1::Cube {
            lower: source,
            upper: source,
        };
        let owner = compile_assignment(unconstrained(target), 1024)
            .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        if source == [140; 3] {
            assert_eq!(rgb(&session), Srgb8::new(source));
        } else {
            assert!(matches!(session.state(), SessionState::Failed { .. }));
        }
    }
    Ok(())
}

#[test]
fn neutral_material_compensates_colored_backdrop_in_its_source() -> Result<(), String> {
    let mut declaration = unconstrained(color([128; 3], 2.0, None)?);
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
    let owner =
        compile_assignment(declaration, 4096).map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
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
    session
        .prepare_update(observed)
        .map_err(|error| format!("update: {error:?}"))?
        .commit();
    let source = rgb(&session);
    assert!(
        !source.is_achromatic(),
        "source compensates the actual colored material backdrop"
    );
    let visible = std::array::from_fn(|i| {
        crate::composition::source_over_channel_srgb8(source.bytes()[i], 0.75, [64, 128, 192][i])
    });
    assert_eq!(visible, [128; 3]);
    Ok(())
}

#[test]
fn max_change_is_relative_to_authored_color_even_with_a_distant_tone_preference()
-> Result<(), String> {
    let source = Srgb8::new([128; 3]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.95], [1.0, 0.95]])
        .map_err(|error| format!("tone curve: {error:?}"))?;
    let owner = compile_assignment(
        unconstrained(color(source.bytes(), 0.05, Some(curve))?),
        2048,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 0))
        .map_err(|error| format!("update: {error:?}"))?
        .commit();
    let output = rgb(&session);
    assert!(squared_distance(lab(output), lab(source)) <= 0.05 * 0.05);
    assert!(lab(output)[0] > lab(source)[0]);
    Ok(())
}

#[test]
fn multi_case_tone_optimizes_worst_case_and_matches_all_gray_values() -> Result<(), String> {
    let source = Srgb8::new([128; 3]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.2], [1.0, 0.8]])
        .map_err(|error| format!("tone curve: {error:?}"))?;
    let [_, a, b] = lab(source);
    let preferred = [0, 255].map(|g| [curve.preferred_lightness(Srgb8::new([g; 3])), a, b]);
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
        .ok_or("the gray oracle domain must contain a candidate")?;
    let owner = compile_assignment(
        unconstrained(color(source.bytes(), 2.0, Some(curve))?),
        4096,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
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
        session
            .prepare_update(observed)
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        assert_eq!(rgb(&session), oracle);
        assert_eq!(
            session
                .state()
                .current_verified()
                .ok_or("the joint gray tuple must be ready")?
                .computed_for_test()
                .optimality(),
            ComputedOptimalityV1::Exact
        );
    }
    Ok(())
}
