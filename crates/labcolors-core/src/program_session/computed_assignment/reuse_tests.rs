use super::tests::{compile_assignment, program, rgb, target, update};
use super::*;
use crate::observation::{ObservationPayloadInput, Revision, ScenarioId};
use crate::session::SessionUpdateError;

fn independent_program(
    evaluator: crate::constraints::CountingProgramWcag22Srgb8V1,
    shared_input: bool,
) -> Program<crate::constraints::CountingProgramWcag22Srgb8V1> {
    let mut p = program(
        target(10, 150, 0, 255, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        evaluator,
    );
    p.targets.push(target(20, 68, 0, 255, 1));
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    if !shared_input {
        p.observation_group
            .surface_input_ports
            .push(SurfaceInputPortId::new(12));
        p.surfaces.push(Surface::Input {
            id: SurfaceId::new(14),
            input: SurfaceInputPortId::new(12),
        });
    }
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(if shared_input { 4 } else { 14 }),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    p
}

fn independent_update(
    revision: u64,
    rows: &[(u8, u8)],
) -> crate::observation::ObservationUpdateInput {
    use crate::observation::{ObservedScenarioSetInput, ScenarioInput, SurfaceInputBinding};
    let mut observed = update(revision, 0);
    observed.payload = ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
        scenarios: rows
            .iter()
            .enumerate()
            .map(|(index, &(a, b))| ScenarioInput {
                id: ScenarioId::new(index as u32 + 90),
                bindings: [(2, a), (12, b)]
                    .into_iter()
                    .map(|(port, gray)| {
                        SurfaceInputBinding::new(
                            SurfaceInputPortId::new(port),
                            ColorSignal::from_srgb8(Srgb8::new([gray; 3])),
                        )
                    })
                    .collect(),
            })
            .collect(),
    });
    observed
}

#[test]
fn changed_component_only_searches_its_own_inputs() {
    let calls = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let solo_calls = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let owner = compile_assignment(independent_program(calls.clone(), false), 300)
        .expect("independent program");
    let solo = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            solo_calls.clone(),
        ),
        300,
    )
    .expect("standalone A");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    let mut standalone = solo
        .instantiate(ObservationStreamId::new(8))
        .expect("standalone session");
    session
        .prepare_update(independent_update(1, &[(0, 0)]))
        .expect("initial")
        .commit();
    standalone
        .prepare_update(update(1, 0))
        .expect("initial A")
        .commit();
    let old = session
        .state()
        .current_verified()
        .expect("verified")
        .computed_for_test();
    let old_values = old.values.clone();
    let before = calls.calls().len();
    let solo_before = solo_calls.calls().len();
    session
        .prepare_update(independent_update(2, &[(255, 0)]))
        .expect("only A changes")
        .commit();
    standalone
        .prepare_update(update(2, 255))
        .expect("A changes")
        .commit();
    let current = session
        .state()
        .current_verified()
        .expect("verified")
        .computed_for_test();
    assert_ne!(current.values[0], old_values[0]);
    assert_eq!(current.values[0], rgb(&standalone));
    assert_eq!(current.values[1], old_values[1]);
    assert_eq!(
        calls.calls().len() - before,
        solo_calls.calls().len() - solo_before + 1,
        "unchanged B performs only final verification, even while A searches"
    );

    drop(
        session
            .prepare_update(independent_update(3, &[(255, 255)]))
            .expect("discarded B proposal"),
    );
    let before = calls.calls().len();
    let solo_before = solo_calls.calls().len();
    session
        .prepare_update(independent_update(3, &[(0, 0)]))
        .expect("restore committed tuple")
        .commit();
    standalone
        .prepare_update(update(3, 0))
        .expect("A changes again")
        .commit();
    assert_eq!(
        calls.calls().len() - before,
        solo_calls.calls().len() - solo_before + 1,
        "partial reuse must restore B after a discarded proposal, while A searches"
    );
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("verified")
            .computed_for_test()
            .values[1],
        old_values[1]
    );
    CORRUPT_FINAL_SOURCE.with(|value| value.set(Some(Srgb8::new([255; 3]))));
    assert!(matches!(
        session.prepare_update(independent_update(4, &[(255, 0)])),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::FinalRecheckViolation { .. }
        ))
    ));
    let retained = session
        .state()
        .current_verified()
        .expect("failed proposal retains the committed snapshot")
        .computed_for_test();
    assert_eq!(retained.observation.revision(), Revision::new(3));
    assert_eq!(retained.values[0], rgb(&standalone));
    assert_eq!(retained.values[1], old_values[1]);
    let before = calls.calls().len();
    let solo_before = solo_calls.calls().len();
    session
        .prepare_update(independent_update(4, &[(255, 0)]))
        .expect("recover failed partial recheck")
        .commit();
    standalone
        .prepare_update(update(4, 255))
        .expect("standalone A changes")
        .commit();
    assert_eq!(
        calls.calls().len() - before,
        solo_calls.calls().len() - solo_before + 1,
        "failed partial proposal cannot cache A; B only needs final verification"
    );
}

#[test]
fn shared_observed_input_invalidates_each_dependent_component() {
    let calls = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let owner = compile_assignment(independent_program(calls.clone(), true), 300)
        .expect("shared input program");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 255))
        .expect("initial")
        .commit();
    let before = session
        .state()
        .current_verified()
        .expect("verified")
        .computed_for_test()
        .values
        .clone();
    let call_count = calls.calls().len();
    session
        .prepare_update(update(2, 0))
        .expect("shared change")
        .commit();
    let current = session
        .state()
        .current_verified()
        .expect("verified")
        .computed_for_test();
    assert!(
        current
            .values
            .iter()
            .zip(before.iter())
            .all(|(new, old)| new != old)
    );
    assert!(
        calls.calls().len() > call_count + 2,
        "both dependent components must search"
    );
}

#[test]
fn reference_reuse_projects_current_cases_and_preserves_unfinished_capacity() {
    use crate::color_intent::IdentityOptionsV1;
    let calls = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let mut p = independent_program(calls.clone(), false);
    for (index, target) in p.targets.iter_mut().enumerate() {
        let anchor = Srgb8::new([if index == 0 { 100 } else { 68 }; 3]);
        let mut declaration = ComputedTargetV1::color(
            anchor,
            2.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                    .expect("identity"),
                appearance: Some(OccurrenceId::new(if index == 0 { 5 } else { 15 })),
                tone: None,
            },
        );
        declaration.reference_appearance = true;
        declaration.preference = index as u64;
        target.intent = TargetIntentV1::Computed(declaration);
    }
    // A's unchanged authored paint remains feasible on these very dark inputs.
    p.constraints = ConstraintSet::new(
        vec![
            ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6),
                OccurrenceId::new(5),
                Wcag22CriterionV1::Sc1411UiComponentOrState,
            ),
            ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(16),
                OccurrenceId::new(15),
                Wcag22CriterionV1::Sc1411UiComponentOrState,
            ),
        ],
        vec![],
    );
    p.reference_inputs = Some(vec![
        (SurfaceInputPortId::new(2), Srgb8::new([255; 3])),
        (SurfaceInputPortId::new(12), Srgb8::new([255; 3])),
    ]);
    let owner = compile_assignment(p, 1).expect("bounded reference program");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(independent_update(1, &[(0, 255), (2, 220)]))
        .expect("initial")
        .commit();
    let initial = session
        .state()
        .current_verified()
        .expect("verified")
        .computed_for_test();
    assert_eq!(initial.optimality(), ComputedOptimalityV1::Unfinished);
    let old_reports = initial
        .reference_selections()
        .expect("reference reports")
        .clone();
    let old_cases = old_reports[1].cases().to_vec();
    let capacity = old_reports[1].capacity().expect("capacity");
    assert!(!capacity.complete());
    for (index, rows) in [
        vec![(2, 255), (0, 220)],
        vec![(2, 255), (0, 220), (1, 255)],
        vec![(1, 255), (0, 220), (2, 255), (1, 255)],
    ]
    .iter()
    .enumerate()
    {
        let before = calls.calls().len();
        session
            .prepare_update(independent_update(index as u64 + 2, rows))
            .expect("foreign cases change")
            .commit();
        let current = session
            .state()
            .current_verified()
            .expect("verified")
            .computed_for_test();
        let reports = current.reference_selections().expect("reports");
        let physical_cases = current.observation.physical_case_count();
        assert_eq!(
            calls.calls()[before..]
                .iter()
                .filter(|&&value| value == Srgb8::new([68; 3]))
                .count(),
            physical_cases,
            "B has no search or capacity evaluations beyond fresh final certification"
        );
        assert_eq!(reports[1].capacity(), Some(capacity));
        assert_eq!(current.optimality(), ComputedOptimalityV1::Unfinished);
        assert_eq!(reports[1].cases().len(), physical_cases);
        for case in reports[1].cases() {
            assert_eq!(
                case.actual_backdrop(),
                current
                    .observation
                    .physical_values(case.case_index())
                    .expect("current case")[1]
                    .srgb8()
            );
            assert_eq!(case.actual_visible(), Srgb8::new([68; 3]));
        }
        assert_eq!(
            old_reports[1].cases(),
            old_cases,
            "historical report remains immutable"
        );
    }
}

#[test]
fn repeated_physical_context_keeps_search_quality_and_refreshes_provenance() -> Result<(), String> {
    for (upper, budget, completion) in [
        (3, 3, CompletionV1::BudgetExhausted),
        (255, 300, CompletionV1::Complete),
    ] {
        let evaluator = crate::constraints::CountingProgramWcag22Srgb8V1::default();
        let owner = compile_assignment(
            program(
                target(10, 150, 0, upper, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                evaluator.clone(),
            ),
            budget,
        )
        .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("initial update: {error:?}"))?
            .commit();
        let first = session
            .state()
            .current_verified()
            .ok_or("initial search must find a usable color")?
            .computed_for_test();
        let selected = rgb(&session);
        let search = first.search;
        assert_eq!(search.completion, completion);
        let calls = evaluator.calls().len();

        let mut observed = update(2, 255);
        let ObservationPayloadInput::Scenarios(scenarios) = &mut observed.payload else {
            return Err("the fixture must contain scenarios".into());
        };
        scenarios.scenarios[0].id = ScenarioId::new(55);
        session
            .prepare_update(observed)
            .map_err(|error| format!("repeated update: {error:?}"))?
            .commit();
        let repeated = session
            .state()
            .current_verified()
            .ok_or("repeated physical context must remain ready")?
            .computed_for_test();
        assert_eq!(rgb(&session), selected);
        assert_eq!(repeated.search, search);
        assert_eq!(repeated.observation.revision(), Revision::new(2));
        assert_eq!(
            repeated.observation.provenance(0),
            Some([ScenarioId::new(55)].as_slice())
        );
        assert_eq!(
            evaluator.calls().len(),
            calls + 1,
            "only fresh physical verification remains"
        );
    }
    Ok(())
}

#[test]
fn repeated_context_restores_discarded_bindings_and_cannot_skip_final_recheck() -> Result<(), String>
{
    let evaluator = crate::constraints::CountingProgramWcag22Srgb8V1::default();
    let owner = compile_assignment(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            evaluator.clone(),
        ),
        300,
    )
    .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(|error| format!("instantiate: {error:?}"))?;
    session
        .prepare_update(update(1, 255))
        .map_err(|error| format!("initial update: {error:?}"))?
        .commit();
    let selected = rgb(&session);
    drop(
        session
            .prepare_update(update(2, 0))
            .map_err(|error| format!("discarded update: {error:?}"))?,
    );
    let calls = evaluator.calls().len();
    session
        .prepare_update(update(2, 255))
        .map_err(|error| format!("restored context: {error:?}"))?
        .commit();
    assert_eq!(rgb(&session), selected);
    assert_eq!(evaluator.calls().len(), calls + 1);

    CORRUPT_FINAL_SOURCE.with(|value| value.set(Some(Srgb8::new([255; 3]))));
    assert!(matches!(
        session.prepare_update(update(3, 255)),
        Err(SessionUpdateError::Plan(
            ProgramSessionEvaluationError::FinalRecheckViolation { .. }
        ))
    ));
    assert_eq!(
        session
            .state()
            .current_verified()
            .ok_or("rejected update must retain revision")?
            .computed_for_test()
            .observation
            .revision(),
        Revision::new(2)
    );
    session
        .prepare_update(update(3, 255))
        .map_err(|error| format!("recovery: {error:?}"))?
        .commit();
    assert_eq!(rgb(&session), selected);

    let calls = evaluator.calls().len();
    session
        .prepare_update(update(4, 0))
        .map_err(|error| format!("new physical context: {error:?}"))?
        .commit();
    assert_ne!(rgb(&session), selected);
    assert!(
        evaluator.calls().len() > calls + 1,
        "changed input must run selection"
    );
    Ok(())
}
