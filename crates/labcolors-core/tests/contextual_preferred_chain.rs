//! Public V3 quality witness: locally repairing a grandchild must not make
//! an attainable preferred surface disappear from the budgeted search.
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

fn contrast(left: u8, right: u8) -> f64 {
    let (left, right) = (luminance(left), luminance(right));
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

#[test]
fn three_level_gray_composition_reaches_its_feasible_preferred_surface() -> Result<(), String> {
    // An achromatic Oklab tone L=1/4 has linear RGB=(1/4)^3. Encode it
    // independently of Core's tone helper. The feasible tuple below proves
    // that the first preference need not be sacrificed to satisfy either edge.
    let linear = 0.25_f64.powi(3);
    let preferred = ((1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8;
    assert_eq!(preferred, 34);
    assert!(contrast(preferred, 180) >= 4.5);
    assert!(contrast(180, 36) >= 4.5);

    let owner = compile_program_wire(include_bytes!("fixtures/preferred-gray-chain.lcpw"))
        .map_err(|error| format!("declaration: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let frame = session
        .update_observed(
            1,
            &[ProgramScenarioV1::new(9, vec![Srgb8::new([40, 53, 72])])],
        )
        .map_err(|error| format!("resolve: {error:?}"))?;
    assert_eq!(frame.state(), ProgramSnapshotStateV1::Ready);
    let source = |slot| {
        frame
            .outputs()
            .iter()
            .find(|output| output.slot() == slot)
            .map(|output| output.source().bytes())
            .ok_or_else(|| format!("missing output {slot}"))
    };
    let [panel, middle, inner] = [source(100)?, source(101)?, source(102)?];
    for rgb in [panel, middle, inner] {
        assert_eq!(rgb, [rgb[0]; 3]);
    }
    assert!(contrast(panel[0], middle[0]) >= 4.5);
    assert!(contrast(middle[0], inner[0]) >= 4.5);
    assert_eq!(
        panel, [preferred; 3],
        "a feasible contextual tone must survive local repairs"
    );
    Ok(())
}
