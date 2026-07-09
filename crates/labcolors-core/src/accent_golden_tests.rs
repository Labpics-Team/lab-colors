//! Контрактные тесты конечных AccentCurve и SentimentCurve.
//!
//! Старые массивы байтов фиксировали одну непрерывную реализацию и краснели при
//! любом научно обоснованном изменении нейтрального скелета. Здесь фиксируется
//! более сильный инвариант: каждый байт должен быть детерминированным оптимумом
//! содержащей непрерывный iso-HK-идеал sRGB8-ячейки.

use crate::accent::oklab_hue_of;
use crate::curve::ColorCurve;
use crate::neutral::NeutralCurve;
use crate::scale::{AccentCurve, iso_hk_identity_from_anchor, quantized_iso_hk_for_neutral};
use crate::sentiment::{Sentiment, SentimentCurve};

fn neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("канонические нейтральные якоря валидны")
}

#[test]
fn accent_curve_matches_the_finite_srgb8_objective_at_every_sample() {
    let neutral = neutral();
    let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
    let identity = iso_hk_identity_from_anchor("#007AFF", neutral.vc()).unwrap();
    let first = curve.sample_hex(13);
    let second = AccentCurve::new("#007AFF", &neutral)
        .unwrap()
        .sample_hex(13);
    assert_eq!(first, second, "одинаковые входы обязаны дать те же байты");

    for (index, actual) in first.iter().enumerate() {
        let t = index as f64 / 12.0;
        let expected = quantized_iso_hk_for_neutral(
            &neutral.at(t),
            identity.h_cam,
            identity.chroma_ratio,
            neutral.vc(),
        )
        .unwrap()
        .color
        .to_hex_with_vc(neutral.vc());
        assert_eq!(
            actual, &expected,
            "ступень {index} обошла конечный objective"
        );
    }
    assert_eq!(first[0], "#FFFFFF");
}

#[test]
fn sentiment_curve_uses_only_its_anchor_and_the_same_srgb8_objective() {
    let neutral = neutral();
    let from_anchor = SentimentCurve::from_anchor("#3E87FF", &neutral).unwrap();
    let legacy_a =
        SentimentCurve::from_sentiment(Sentiment::Info, 200.0, "#3E87FF", &neutral).unwrap();
    let legacy_b =
        SentimentCurve::from_sentiment(Sentiment::Info, 33.5, "#3E87FF", &neutral).unwrap();

    assert_eq!(from_anchor.sample_hex(13), legacy_a.sample_hex(13));
    assert_eq!(legacy_a.sample_hex(13), legacy_b.sample_hex(13));
    assert_eq!(
        from_anchor.resolved_hue.to_bits(),
        oklab_hue_of("#3E87FF").to_bits()
    );
    assert!(!from_anchor.was_displaced);
    assert_eq!(from_anchor.displacement, 0.0);

    let identity = iso_hk_identity_from_anchor("#3E87FF", neutral.vc()).unwrap();
    for index in 0..13 {
        let t = index as f64 / 12.0;
        let expected = quantized_iso_hk_for_neutral(
            &neutral.at(t),
            identity.h_cam,
            identity.chroma_ratio,
            neutral.vc(),
        )
        .unwrap()
        .color
        .to_hex_with_vc(neutral.vc());
        assert_eq!(from_anchor.sample_hex(13)[index], expected);
    }
}
