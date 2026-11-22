use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramObservationV1, ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

fn observation(rgb: &[u8; 3]) -> Result<ProgramObservationV1<'_>, String> {
    ProgramObservationV1::from_rgb8(&[9], rgb, 1).map_err(|error| format!("observation: {error:?}"))
}

#[test]
fn host_acceptance_controls_one_complete_computed_update() -> Result<(), String> {
    let mut session = compile_program_wire(include_bytes!("fixtures/computed-coupled.lcpw"))
        .map_err(|error| format!("compile: {error:?}"))?
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let initial = session
        .update_observed_view(1, observation(&[255; 3])?)
        .map_err(|error| format!("initial update: {error:?}"))?;
    let mut candidate = None;
    let applied = session
        .prepare_observed_view(2, observation(&[0; 3])?)
        .map_err(|error| format!("prepare candidate: {error:?}"))?
        .commit_with(|snapshot| {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.revision(), 2);
            assert_eq!(
                snapshot
                    .assessment_report()
                    .ok_or("the ready candidate must have its assessment report")?
                    .revision(),
                2
            );
            assert_eq!(snapshot.outputs().len(), 2);
            candidate = Some(snapshot);
            Ok::<_, String>(false)
        })?;
    assert!(!applied);
    assert_eq!(
        session
            .update_observed_view(1, observation(&[255; 3])?)
            .map_err(|error| format!("replay after rejection: {error:?}"))?,
        initial,
        "a refused candidate must leave the accepted revision replayable"
    );
    let accepted = session
        .prepare_observed_view(2, observation(&[0; 3])?)
        .map_err(|error| format!("prepare accepted update: {error:?}"))?
        .commit();
    assert_eq!(
        accepted,
        candidate.ok_or("the host must retain the candidate")?
    );
    assert_eq!(
        session
            .update_observed_view(2, observation(&[0; 3])?)
            .map_err(|error| format!("replay accepted update: {error:?}"))?,
        accepted
    );
    assert_eq!(initial.revision(), 1);
    assert_eq!(
        initial
            .assessment_report()
            .ok_or("the initial ready snapshot must retain its report")?
            .revision(),
        1
    );
    Ok(())
}

#[test]
fn callback_error_and_dropped_preparation_preserve_the_previous_revision() -> Result<(), String> {
    let mut session = compile_program_wire(include_bytes!("fixtures/computed-coupled.lcpw"))
        .map_err(|error| format!("compile: {error:?}"))?
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let scenarios = [ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])];
    let initial = session
        .update_observed(1, &scenarios)
        .map_err(|error| format!("initial update: {error:?}"))?;
    let error = session
        .prepare_observed_view(2, observation(&[0; 3])?)
        .map_err(|error| format!("prepare callback error: {error:?}"))?
        .commit_with(|_| Err::<bool, _>("host rejected"));
    assert_eq!(error, Err("host rejected"));
    assert_eq!(
        session
            .update_observed(1, &scenarios)
            .map_err(|error| format!("replay after callback error: {error:?}"))?,
        initial
    );
    drop(
        session
            .prepare_observed_view(2, observation(&[0; 3])?)
            .map_err(|error| format!("prepare dropped update: {error:?}"))?,
    );
    assert_eq!(
        session
            .update_observed(1, &scenarios)
            .map_err(|error| format!("replay after drop: {error:?}"))?,
        initial
    );
    let retry = session
        .update_observed_view(2, observation(&[0; 3])?)
        .map_err(|error| format!("retry: {error:?}"))?;
    assert_eq!(retry.revision(), 2);
    Ok(())
}
