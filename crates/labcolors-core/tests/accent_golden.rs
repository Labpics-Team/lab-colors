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
/// CONSCIOUS SNAPSHOT CHANGE — category membership-field model
/// (sentiment-category-fields). The Info field peak is now the *anchor colour's*
/// Oklab hue (`#007AFF` → 257.42°), not a hand-typed 240°: brand 200° sits 57.4°
/// away, well beyond the perceptual floor `s_min`, so the field's peak is
/// feasible and the hue resolves to **257.42° un-displaced** (`was_displaced ==
/// false`) — a far brand no longer perturbs the category at all. The ladder is
/// also built at constant, hue-independent colourfulness (`binding_mp`), so its
/// chroma profile matches the other sentiments. Intentional contract change.
const SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F7F8FA", "#E1E9F5", "#BFD4F2", "#8FB9F5", "#609AED", "#4B79BC", "#4672B1",
    "#3C659E", "#325485", "#254068", "#172A46", "#06101F",
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
    // Pin the resolution decision under the membership-field model: the Info peak
    // is the anchor's Oklab hue (257.42°), and brand 200° is 57.4° away — beyond
    // s_min — so the peak is feasible and the hue resolves there *un-displaced*.
    assert!(
        !curve.was_displaced,
        "a far brand (57° away) must not perturb the category: was_displaced should be false"
    );
    assert!(
        (curve.resolved_hue - 257.42).abs() < 0.1,
        "Info resolved hue should be the anchor Oklab hue 257.42°: {}",
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
