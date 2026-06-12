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

use labcolors_core::neutral::NeutralCurve;
use labcolors_core::scale::AccentCurve;
use labcolors_core::sentiment::{Sentiment, SentimentCurve};

/// The system neutral ladder all accent/sentiment curves are built on.
fn neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("the canonical neutral anchors are valid")
}

/// AccentCurve::new("#007AFF", neutral).sample_hex(13) — frozen.
/// Recalibration = a conscious, reviewed change to this constant.
const ACCENT_007AFF_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F4F8FF", "#DAE9FF", "#B6D4FF", "#88B9FF", "#4F98FF", "#0072F0", "#006BE2",
    "#005FC9", "#004FAA", "#003C85", "#00275B", "#000F2B",
];

/// SentimentCurve(Info, brand=200°, prototype "#007AFF", neutral).sample_hex(13)
/// — frozen.
///
/// CONSCIOUS SNAPSHOT CHANGE — smooth-asymptote model, by owner decision
/// (sentiment-asymptote, 2026-06-12). The previous model used a hard 20°
/// conflict threshold: brand 200° is 40° from the Info prototype (240°), beyond
/// the threshold, so the resolved hue snapped exactly to 240° and was *not*
/// displaced. The new model has NO on/off threshold — the displacement decays
/// asymptotically toward (but never reaches) zero. At d = 40° the smooth
/// separation is s(40) = (40² + 20²)^(1/2) ≈ 44.72°, so the resolved hue is
/// 200° + 44.72° = 244.72° (a deliberate +4.72° nudge), and `was_displaced` is
/// now `true`. The ladder below is regenerated from that resolved hue. This is
/// an intentional contract change, not silent drift.
const SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F2F9FF", "#D5EBFF", "#AAD8FF", "#6FBFFF", "#00A1FA", "#007FC7", "#0077BB",
    "#0069A6", "#00588C", "#00446D", "#002D4A", "#001222",
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
    let curve = SentimentCurve::new(Sentiment::Info, 200.0, "#007AFF", &neutral)
        .expect("Info sentiment with a far brand hue resolves");
    // Pin the resolution decision under the smooth-asymptote model: brand 200°
    // is 40° from the 240° prototype, so the hue is displaced by the asymptotic
    // nudge s(40) − 40 ≈ 4.72°, landing at ≈244.72°. (Conscious change from the
    // old hard-threshold behaviour, which left it un-displaced at 240°.)
    assert!(
        curve.was_displaced,
        "smooth model has no threshold: a 40° brand still nudges the hue"
    );
    assert!(
        (curve.resolved_hue - 244.72).abs() < 0.1,
        "Info resolved hue should be the smooth-asymptote 244.72°: {}",
        curve.resolved_hue
    );
    let got = curve.sample_hex(13);
    assert_eq!(
        got, SENTIMENT_INFO_GOLDEN,
        "SentimentCurve(Info) ladder drifted from its golden snapshot. If this was a deliberate \
         recalibration, update SENTIMENT_INFO_GOLDEN consciously; otherwise it is a silent \
         value regression."
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
