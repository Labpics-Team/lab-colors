//! Публичный контракт сентиментной геометрии V2.

use labcolors_core::sentiment::{
    SENTIMENT_GEOMETRY_V2, minimum_hue_separation_deg, resolve_config_sentiment_solid_v2,
};

#[test]
fn v2_preserves_the_client_anchor_instead_of_classifying_its_name() {
    for anchor in ["#FF3B30", "#FFA100", "#34C759", "#3E87FF", "#808080"] {
        assert_eq!(resolve_config_sentiment_solid_v2(anchor).unwrap(), anchor);
    }
    assert_eq!(SENTIMENT_GEOMETRY_V2, "anchor-distance-v2");
}

#[test]
fn unequal_radii_use_the_full_law_of_cosines() {
    let c1 = 0.23_f64;
    let c2 = 0.11_f64;
    let angle = 73.0_f64;
    let distance = (c1 * c1 + c2 * c2 - 2.0 * c1 * c2 * angle.to_radians().cos()).sqrt();
    let recovered = minimum_hue_separation_deg(distance, c1, c2).unwrap();
    assert!((recovered - angle).abs() < 1.0e-12);
}

#[test]
fn an_impossible_pair_is_an_error_not_a_skipped_constraint() {
    assert!(minimum_hue_separation_deg(0.41, 0.20, 0.20).is_err());
}
