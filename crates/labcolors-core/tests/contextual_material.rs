use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1, ProgramScenarioV1,
    ProgramSnapshotStateV1, compile_program_wire,
};

#[test]
fn independent_exact_witness_satisfies_original_material_requirements() -> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/contextual-material-witness.lcpw"))
        .map_err(|error| format!("declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("session: {error:?}"))?;
    let frame = session
        .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([0; 3])])])
        .map_err(|error| format!("resolve: {error:?}"))?;
    assert_eq!(
        frame.state(),
        ProgramSnapshotStateV1::Ready,
        "{:?}",
        frame.diagnostics()
    );
    let report = frame.assessment_report().ok_or("witness assessment")?;
    assert_eq!(report.cells().len(), 6);
    for cell in report.cells() {
        let verdict = match cell.assessment() {
            ProgramConstraintAssessmentV1::VisibleSeparation { verdict, .. }
            | ProgramConstraintAssessmentV1::ExactSrgb8 { verdict }
            | ProgramConstraintAssessmentV1::Wcag22Srgb8 { verdict } => verdict,
            _ => return Err("unexpected assessment".into()),
        };
        assert_eq!(verdict, ProgramAssessmentVerdictV1::Pass);
    }
    assert_eq!(frame.outputs()[1].source().bytes(), [255; 3]);
    assert_eq!(frame.outputs()[2].source().bytes(), [0, 108, 248]);
    Ok(())
}

#[test]
fn material_text_and_independent_text_resolve_on_each_observed_surface() -> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/contextual-material.lcpw"))
        .map_err(|error| format!("declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("session: {error:?}"))?;
    for (revision, background) in [(1, [255; 3]), (2, [0; 3]), (3, [107, 127, 114])] {
        let frame = session
            .update_observed(
                revision,
                &[ProgramScenarioV1::new(9, vec![Srgb8::new(background)])],
            )
            .map_err(|error| format!("resolve: {error:?}"))?;
        assert_eq!(
            frame.state(),
            ProgramSnapshotStateV1::Ready,
            "background={background:?}; {:?}",
            frame.diagnostics()
        );
        assert_eq!(frame.outputs().len(), 3);
        assert_eq!(frame.outputs()[0].opacity(), 0.72);
        assert_eq!(
            frame.assessment_report().ok_or("assessment")?.cells().len(),
            3
        );
    }
    Ok(())
}
