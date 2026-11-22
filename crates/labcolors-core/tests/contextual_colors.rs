use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

#[test]
fn public_wire_resolves_a_surface_and_its_coupled_colors() -> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/contextual-colors.lcpw"))
        .map_err(|error| format!("declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let mut previous = None;
    for (revision, background) in [(1, [255; 3]), (2, [0; 3]), (3, [223, 239, 231])] {
        let frame = session
            .update_observed(
                revision,
                &[ProgramScenarioV1::new(9, vec![Srgb8::new(background)])],
            )
            .map_err(|error| format!("resolve: {error:?}"))?;
        assert_eq!(frame.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(frame.outputs().len(), 4);
        let surface = frame
            .outputs()
            .iter()
            .find(|value| value.slot() == 17)
            .ok_or("surface")?
            .source();
        if let Some(previous) = previous {
            assert_ne!(surface, previous);
        }
        previous = Some(surface);
        assert!(frame.assessment_report().is_some());
        assert!(frame.outputs().iter().all(|paint| paint.opacity() == 1.0));
    }
    Ok(())
}

#[test]
fn explicit_fixed_color_needs_no_artificial_constraint() -> Result<(), String> {
    let owner = compile_program_wire(include_bytes!("fixtures/fixed-contextual.lcpw"))
        .map_err(|error| format!("declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let frame = session
        .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])])
        .map_err(|error| format!("resolve: {error:?}"))?;
    assert_eq!(frame.state(), ProgramSnapshotStateV1::Ready);
    assert_eq!(frame.outputs().len(), 1);
    assert_eq!(frame.outputs()[0].source(), Srgb8::new([18, 52, 86]));
    Ok(())
}
