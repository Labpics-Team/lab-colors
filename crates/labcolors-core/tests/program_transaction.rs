use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramObservationV1, ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

fn observation(rgb: &[u8; 3]) -> ProgramObservationV1<'_> {
    ProgramObservationV1::from_rgb8(&[9], rgb, 1).unwrap()
}

#[test]
fn host_acceptance_controls_one_complete_computed_update() {
    let mut session = compile_program_wire(include_bytes!("fixtures/computed-coupled.lcpw"))
        .unwrap()
        .instantiate(8)
        .unwrap();
    let initial = session
        .update_observed_view(1, observation(&[255; 3]))
        .unwrap();
    let mut candidate = None;
    let applied = session
        .prepare_observed_view(2, observation(&[0; 3]))
        .unwrap()
        .commit_with(|snapshot| {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.revision(), 2);
            assert_eq!(snapshot.assessment_report().unwrap().revision(), 2);
            assert_eq!(snapshot.outputs().len(), 2);
            candidate = Some(snapshot);
            Ok::<_, ()>(false)
        })
        .unwrap();
    assert!(!applied);
    assert_eq!(
        session
            .update_observed_view(1, observation(&[255; 3]))
            .unwrap(),
        initial,
        "a refused candidate must leave the accepted revision replayable"
    );
    let accepted = session
        .prepare_observed_view(2, observation(&[0; 3]))
        .unwrap()
        .commit();
    assert_eq!(accepted, candidate.unwrap());
    assert_eq!(
        session
            .update_observed_view(2, observation(&[0; 3]))
            .unwrap(),
        accepted
    );
    assert_eq!(initial.revision(), 1);
    assert_eq!(initial.assessment_report().unwrap().revision(), 1);
}

#[test]
fn callback_error_and_dropped_preparation_preserve_the_previous_revision() {
    let mut session = compile_program_wire(include_bytes!("fixtures/computed-coupled.lcpw"))
        .unwrap()
        .instantiate(8)
        .unwrap();
    let scenarios = [ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])];
    let initial = session.update_observed(1, &scenarios).unwrap();
    let error = session
        .prepare_observed_view(2, observation(&[0; 3]))
        .unwrap()
        .commit_with(|_| Err::<bool, _>("host rejected"));
    assert_eq!(error, Err("host rejected"));
    assert_eq!(session.update_observed(1, &scenarios).unwrap(), initial);
    drop(
        session
            .prepare_observed_view(2, observation(&[0; 3]))
            .unwrap(),
    );
    assert_eq!(session.update_observed(1, &scenarios).unwrap(), initial);
    let retry = session
        .update_observed_view(2, observation(&[0; 3]))
        .unwrap();
    assert_eq!(retry.revision(), 2);
}
