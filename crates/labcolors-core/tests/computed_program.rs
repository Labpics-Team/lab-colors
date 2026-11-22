use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

fn luminance(gray: u8) -> f64 {
    let encoded = f64::from(gray) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast(first: u8, second: u8) -> f64 {
    let a = luminance(first);
    let b = luminance(second);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn public_computed_program_solves_fill_and_label_on_their_actual_composition() -> Result<(), String>
{
    let owner = compile_program_wire(include_bytes!("fixtures/computed-coupled.lcpw"))
        .map_err(|error| format!("computed declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(8)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let update = [ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])];
    let ready = session
        .update_observed(1, &update)
        .map_err(|error| format!("update: {error:?}"))?;
    assert_eq!(ready.state(), ProgramSnapshotStateV1::Ready);
    assert_eq!(ready.snapshot_version(), 2);
    assert_eq!(ready.revision(), 1);
    assert_eq!(
        ready.optimality(),
        Some(labcolors_core::program_wire::ProgramOptimalityV1::Exact)
    );
    let report = ready
        .assessment_report()
        .ok_or("ready must expose its final assessments")?;
    assert_eq!(report.revision(), 1);
    assert_eq!(report.cells().len(), 1);
    assert_eq!(report.cells()[0].constraint_id(), 6);
    let read = |slot| {
        ready
            .outputs()
            .iter()
            .find(|output| output.slot() == slot)
            .map(|output| output.source())
            .ok_or_else(|| format!("missing slot {slot}"))
    };
    let fill = read(7)?.bytes();
    let label = read(17)?.bytes();
    assert_eq!(fill, [150; 3]);
    assert_ne!(label, [160; 3]);
    assert!(contrast(label[0], fill[0]) >= 3.0);
    assert_eq!(
        session
            .update_observed(1, &update)
            .map_err(|error| format!("replay: {error:?}"))?,
        ready
    );
    let stale = session
        .update_unknown(2, 44)
        .map_err(|error| format!("unknown: {error:?}"))?;
    assert_eq!(stale.state(), ProgramSnapshotStateV1::Stale);
    assert_eq!(stale.revision(), 2);
    assert!(stale.outputs().is_empty());
    assert!(stale.assessment_report().is_none());
    assert!(stale.optimality().is_none());
    assert_eq!(ready.outputs().len(), 2);
    Ok(())
}
