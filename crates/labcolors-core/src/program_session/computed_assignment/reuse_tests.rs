use super::tests::{compile_assignment, program, rgb, target, update};
use super::*;
use crate::observation::{ObservationPayloadInput, Revision, ScenarioId};
use crate::session::SessionUpdateError;

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
