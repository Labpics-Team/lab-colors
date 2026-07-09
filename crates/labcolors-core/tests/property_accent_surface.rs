//! Свойства акцентных поверхностей после конечной sRGB8-квантизации.
//!
//! Тесты не сравнивают непрерывные `J′` и не вводят визуальный допуск. Целевой и
//! достигнутый H-K-уровни заново измеряются из тех hex, которые реально покидают
//! API; идентичность сравнивается как точная доля физического iso-HK-радиуса.

use labcolors_core::curve::ColorCurve;
use labcolors_core::neutral::{CurveParams, NeutralCurve};
use labcolors_core::scale::{AccentCurve, emitted_perceived_lightness};
use labcolors_core::{LcsColor, ViewingConditions, derive_accent_surface_ramp};

const CLIENT_ANCHORS: [&str; 11] = [
    "#FF3B30", "#FFA100", "#FFD000", "#34C759", "#5AC8FA", "#00C7BE", "#3E87FF", "#5856D6",
    "#AF52DE", "#FF2D55", "#B36A65",
];

fn neutral(vc: &ViewingConditions) -> NeutralCurve {
    NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &CurveParams::default(), vc).unwrap()
}

#[test]
fn metadata_equals_remeasurement_of_every_emitted_hex() {
    for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
        let curve = neutral(&vc);
        let neutral_levels = curve.sample(17);
        for anchor in CLIENT_ANCHORS {
            let levels = derive_accent_surface_ramp(&neutral_levels, anchor, &vc).unwrap();
            assert_eq!(levels.len(), neutral_levels.len());
            for (neutral, accent) in neutral_levels.iter().zip(&levels) {
                let neutral_hex = neutral.to_hex_with_vc(&vc);
                let accent_hex = accent.color.to_hex_with_vc(&vc);
                assert_eq!(
                    accent.target_hk.to_bits(),
                    emitted_perceived_lightness(&neutral_hex, &vc)
                        .unwrap()
                        .to_bits()
                );
                assert_eq!(
                    accent.achieved_hk.to_bits(),
                    emitted_perceived_lightness(&accent_hex, &vc)
                        .unwrap()
                        .to_bits()
                );
                assert!(LcsColor::from_hex_with_vc(&accent_hex, &vc).is_ok());
            }
        }
    }
}

#[test]
fn every_level_preserves_the_same_anchor_radius_fraction() {
    for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
        let curve = neutral(&vc);
        let neutral_levels = curve.sample(13);
        for anchor in CLIENT_ANCHORS {
            let expected = AccentCurve::new(anchor, &curve).unwrap().sat_ratio();
            let levels = derive_accent_surface_ramp(&neutral_levels, anchor, &vc).unwrap();
            assert!(levels.iter().all(|level| {
                level.chroma_ratio.to_bits() == expected.to_bits()
                    && (0.0..=1.0).contains(&level.chroma_ratio)
            }));
        }
    }
}

#[test]
fn muted_anchor_is_not_promoted_to_the_gamut_wall() {
    let vc = ViewingConditions::srgb();
    let curve = neutral(&vc);
    let muted = AccentCurve::new("#B36A65", &curve).unwrap().sat_ratio();
    let boundary = AccentCurve::new("#3E87FF", &curve).unwrap().sat_ratio();
    assert!((0.0..1.0).contains(&muted));
    assert_eq!(boundary.to_bits(), 1.0_f64.to_bits());
}

#[test]
fn invalid_anchor_is_an_error_not_a_partial_ramp() {
    let vc = ViewingConditions::srgb();
    let curve = neutral(&vc);
    assert!(derive_accent_surface_ramp(&curve.sample(5), "#GGGGGG", &vc).is_err());
}
