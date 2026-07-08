//! Contract 7 — AccentCurve / SentimentCurve golden snapshots.
//!
//! BUG CLASS this guards: *silent value drift.* Every other test in this crate
//! checks a *property* — in-gamut, monotone J', non-negative saturation, hits a
//! contrast target. None of them pins the *actual emitted colours*. A change to
//! a curve coefficient, the chroma envelope, the hue-optimisation search, or the
//! CAM16-UCS rescaling could shift every swatch by a few bytes while keeping all
//! the properties true — and no test would notice. The Bracket-path LUT seam
//! (#50/#53) was exactly this shape: a value that moved without a property
//! breaking. This file freezes the exact byte output of two representative
//! curves, sampled at 13 stops.
//!
//! A failure here is NOT automatically a bug: a deliberate recalibration of a
//! curve is a legitimate, intentional change of the snapshot. The rule is that
//! it must be a *conscious* swap — read the diff, confirm the new ladder is the
//! intended one, and update the constant. Drift that nobody chose is the
//! regression; the snapshot makes the difference visible instead of invisible.
//!
//! Snapshots captured 2026-06-12 from the curves' own `sample_hex(13)` through
//! their inherited (srgb) viewing conditions.

use crate::curve::ColorCurve;
use crate::neutral::NeutralCurve;
use crate::scale::AccentCurve;
use crate::sentiment::{Sentiment, SentimentCurve};

/// The system neutral ladder all accent/sentiment curves are built on.
fn neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("the canonical neutral anchors are valid")
}

/// AccentCurve::new("#007AFF", neutral).sample_hex(13) — frozen.
/// Recalibration = a conscious, reviewed change to this constant.
const ACCENT_007AFF_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F4F8FF", "#DAE9FF", "#B6D4FF", "#88B9FF", "#4F98FF", "#0A6CFF", "#0060FC",
    "#0C41FF", "#0500F9", "#0300C4", "#010089", "#000043",
];

/// SentimentCurve(Info, brand=200°, prototype "#3E87FF", neutral).sample_hex(13)
/// — frozen.
///
/// СОЗНАТЕЛЬНЫЙ ДРЕЙФ Волны 1 (закон категориальных зон). Прежде brand=200°
/// смещал Info плавно-асимптотически до resolved_hue ≈ 259.96°. Под новым законом
/// бренд НЕ смещает сентимент — Info ОТДЫХАЕТ на своём синем фокусе 259.89°
/// (Figma `Accent/Blue`), поэтому рампа чуть сдвинулась. Это следствие закона (info
/// теперь покоится на синем фокусе), а не тихая регрессия; массив перегенерирован
/// из ФАКТИЧЕСКОГО вывода нового закона.
const SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#ECF3FD", "#CCDEFB", "#A1C2F8", "#6EA1F4", "#2F78F0", "#1858BD", "#1551B0",
    "#114597", "#0B3579", "#052456", "#02112F", "#000108",
];

#[test]
fn accent_curve_007af_sample_hex_13_matches_golden() {
    let neutral = neutral();
    let accent = AccentCurve::new("#007AFF", &neutral).expect("#007AFF is a valid accent seed");
    let got = accent.sample_hex(13);
    assert_eq!(
        got, ACCENT_007AFF_GOLDEN,
        "AccentCurve('#007AFF') ladder drifted from its golden snapshot. If this was a \
         deliberate recalibration, update ACCENT_007AFF_GOLDEN consciously; otherwise it is \
         a silent value regression."
    );
}

#[test]
fn sentiment_info_curve_sample_hex_13_matches_golden() {
    let neutral = neutral();
    // prototype_hex = Figma Accent/Blue (#3E87FF), Oklab-оттенок 259.89°.
    // СОЗНАТЕЛЬНЫЙ ДРЕЙФ Волны 1: бренд (200°) больше НЕ смещает Info — сентимент
    // ОТДЫХАЕТ на своём синем фокусе 259.89° (прежде brand-displacement уводил его
    // до ≈259.96°). Следствие закона категориальных зон, не тихая регрессия.
    let curve = SentimentCurve::from_sentiment(Sentiment::Info, 200.0, "#3E87FF", &neutral)
        .expect("Info sentiment resolves (brand ignored by the categorical-zone law)");
    // Info покоится: смещения от прототипа нет (was_displaced == false, Δ ≈ 0).
    assert!(
        !curve.was_displaced && curve.displacement < 1e-6,
        "Info должен ОТДЫХАТЬ на фокусе (бренд игнорируется): displaced={}, Δ={}",
        curve.was_displaced,
        curve.displacement
    );
    assert!(
        (curve.resolved_hue - 259.89).abs() < 0.1,
        "Info resolved hue должен быть ~259.89° (синий фокус Figma Accent/Blue; \
         бренд не смещает): {}",
        curve.resolved_hue
    );
    let got = curve.sample_hex(13);
    assert_eq!(
        got, SENTIMENT_INFO_GOLDEN,
        "SentimentCurve(Info) ladder drifted from its golden snapshot. Golden перегенерирован \
         под закон Волны 1 (Info покоится на синем фокусе); будущий дрейф = регрессия."
    );
}

#[test]
fn golden_endpoints_anchor_to_white_and_near_black() {
    // A cheap structural guard so an accidental wholesale replacement of the
    // golden constants (e.g. all-white) can't pass: both ladders start at pure
    // white and descend to a dark near-black, monotonically darkening overall.
    assert_eq!(ACCENT_007AFF_GOLDEN[0], "#FFFFFF");
    assert_eq!(SENTIMENT_INFO_GOLDEN[0], "#FFFFFF");
    for golden in [&ACCENT_007AFF_GOLDEN, &SENTIMENT_INFO_GOLDEN] {
        let luma = |hex: &str| -> u32 {
            let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
            ((v >> 16) & 0xFF) + ((v >> 8) & 0xFF) + (v & 0xFF)
        };
        assert!(
            luma(golden[0]) > luma(golden[12]),
            "golden ladder must darken from first to last stop"
        );
    }
}
