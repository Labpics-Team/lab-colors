use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1, ProgramDiagnosticSubjectV1,
    ProgramDiagnosticV1, ProgramObservationV1, ProgramScenarioV1, ProgramSnapshotStateV1,
    compile_program_wire,
};

// Независимый neutral-ray oracle: linear sRGB gray имеет L = cbrt(linear).
// Опубликованные округлённые матрицы Oklab отклоняют эту идеальную ось <2e-8;
// границы fixture не прилегают к этому численному интервалу.
fn gray_lightness(gray: u8) -> f64 {
    let encoded = f64::from(gray) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    };
    linear.cbrt()
}

fn oracle(backgrounds: &[u8]) -> Option<u8> {
    (0..=255_u8)
        .filter(|&source| {
            backgrounds.iter().all(|&background| {
                let visible = (u16::from(source) + u16::from(background)).div_ceil(2) as u8;
                let distance = (gray_lightness(visible) - gray_lightness(background)).abs();
                (0.18..=0.21).contains(&distance)
            })
        })
        .min_by(|&left, &right| {
            let anchor = gray_lightness(128);
            (gray_lightness(left) - anchor)
                .abs()
                .total_cmp(&(gray_lightness(right) - anchor).abs())
                .then_with(|| left.cmp(&right))
        })
}

#[test]
fn visible_separation_uses_final_composition_and_adapts_to_actual_backdrop() -> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/visible-separation.lcpw"))
        .map_err(|error| format!("separation declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let ready = session
        .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])])
        .map_err(|error| format!("white update: {error:?}"))?;
    assert_eq!(ready.state(), ProgramSnapshotStateV1::Ready);
    assert_eq!(ready.outputs()[0].source(), Srgb8::new([128; 3]));
    let report = ready.assessment_report().ok_or("ready report")?;
    let cell = report.cells().first().ok_or("one separation cell")?;
    assert_eq!(cell.constraint_id(), 6);
    assert_eq!(cell.case_index(), 0);
    assert!(cell.hard());
    let ProgramConstraintAssessmentV1::VisibleSeparation {
        verdict,
        measurement,
    } = cell.assessment()
    else {
        return Err("typed separation evidence required".into());
    };
    assert_eq!(verdict, ProgramAssessmentVerdictV1::Pass);
    assert_eq!(measurement.visible(), Srgb8::new([192; 3]));
    assert_eq!(measurement.backdrop(), Srgb8::new([255; 3]));
    assert_eq!(measurement.minimum(), 0.18);
    assert_eq!(measurement.maximum(), 0.21);
    assert!((measurement.minimum()..=measurement.maximum()).contains(&measurement.distance()));
    assert!(
        (measurement.distance() - (gray_lightness(192) - gray_lightness(255)).abs()).abs() < 2e-8
    );
    let changed = session
        .update_observed(2, &[ProgramScenarioV1::new(9, vec![Srgb8::new([128; 3])])])
        .map_err(|error| format!("gray update: {error:?}"))?;
    assert_eq!(changed.state(), ProgramSnapshotStateV1::Ready);
    assert_ne!(changed.outputs()[0].source(), Srgb8::new([128; 3]));
    assert_eq!(
        changed.outputs()[0].source(),
        Srgb8::new([oracle(&[128]).ok_or("oracle feasible")?; 3])
    );
    assert_eq!(oracle(&[255, 128]), None);
    let conflict = session
        .update_observed(
            3,
            &[
                ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])]),
                ProgramScenarioV1::new(10, vec![Srgb8::new([128; 3])]),
            ],
        )
        .map_err(|error| format!("joint support: {error:?}"))?;
    assert_eq!(conflict.state(), ProgramSnapshotStateV1::Unsatisfied);
    assert!(conflict.outputs().is_empty());
    let diagnostics = conflict
        .diagnostics()
        .ok_or("unsatisfied requires typed diagnostics")?;
    let witness = diagnostics
        .witness()
        .ok_or("authored attempt was evaluated")?;
    assert!(!witness.violations().is_empty());
    assert!(diagnostics.search_visited() <= diagnostics.search_budget());
    assert_eq!(witness.case_scenario_ids(), &[vec![10], vec![9]]);
    assert!(
        witness
            .violations()
            .contains(&ProgramDiagnosticV1::HardConstraint {
                constraint_id: 6,
                case_index: 0,
                field_case: None,
                subject: ProgramDiagnosticSubjectV1::Occurrence { occurrence_id: 5 },
            })
    );
    assert_eq!(ready.outputs()[0].source(), Srgb8::new([128; 3]));
    Ok(())
}

#[test]
fn diagnostic_candidate_can_be_refused_atomically_and_never_becomes_a_fresh_output()
-> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/visible-separation.lcpw"))
        .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let white = [ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])];
    let initial = session
        .update_observed(1, &white)
        .map_err(|error| format!("initial: {error:?}"))?;
    assert!(initial.diagnostics().is_none());
    let ids = [9, 10];
    let samples = [255, 255, 255, 128, 128, 128];
    let observation = ProgramObservationV1::from_rgb8(&ids, &samples, 1)
        .map_err(|error| format!("observation: {error:?}"))?;
    let mut refused = None;
    let accepted = session
        .prepare_observed_view(2, observation)
        .map_err(|error| format!("prepare: {error:?}"))?
        .commit_with(|snapshot| {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Unsatisfied);
            assert!(snapshot.outputs().is_empty());
            assert!(
                snapshot
                    .diagnostics()
                    .and_then(|value| value.witness())
                    .is_some()
            );
            refused = Some(snapshot);
            Ok::<_, String>(false)
        })?;
    assert!(!accepted);
    assert_eq!(
        session
            .update_observed(1, &white)
            .map_err(|error| format!("replay: {error:?}"))?,
        initial
    );
    let conflict = session
        .prepare_observed_view(2, observation)
        .map_err(|error| format!("retry: {error:?}"))?
        .commit();
    assert_eq!(Some(&conflict), refused.as_ref());
    let unknown = session
        .update_unknown(3, 17)
        .map_err(|error| format!("unknown: {error:?}"))?;
    assert!(unknown.outputs().is_empty());
    assert!(unknown.diagnostics().is_none());
    assert_eq!(unknown.reason(), None);
    let recovered = session
        .update_observed(4, &white)
        .map_err(|error| format!("recover: {error:?}"))?;
    assert_eq!(recovered.state(), ProgramSnapshotStateV1::Ready);
    assert!(recovered.diagnostics().is_none());
    assert!(
        conflict
            .diagnostics()
            .and_then(|value| value.witness())
            .is_some()
    );
    Ok(())
}

#[test]
fn public_separation_matches_finite_independent_oracle_after_context_changes() -> Result<(), String>
{
    let owner = compile_program_wire(include_bytes!("fixtures/visible-separation.lcpw"))
        .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    for (index, background) in [0_u8, 63, 127, 128, 192, 255, 128].into_iter().enumerate() {
        let ready = session
            .update_observed(
                index as u64 + 1,
                &[ProgramScenarioV1::new(9, vec![Srgb8::new([background; 3])])],
            )
            .map_err(|error| format!("update: {error:?}"))?;
        let expected = oracle(&[background]).ok_or("fixture must be feasible")?;
        assert_eq!(ready.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(ready.outputs()[0].source(), Srgb8::new([expected; 3]));
    }
    Ok(())
}
