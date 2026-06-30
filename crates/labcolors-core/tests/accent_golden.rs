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
    "#FFFFFF", "#F4F8FF", "#DAE9FF", "#B6D4FF", "#88B9FF", "#4F98FF", "#0A6CFF", "#0060FC",
    "#0C41FF", "#0500F9", "#0300C4", "#010089", "#000043",
];

/// SentimentCurve(Info, brand=200°, prototype "#3E87FF", neutral).sample_hex(13)
/// — frozen.
///
/// CONSCIOUS SNAPSHOT CHANGE (Zone D, 2026-06-30) — anchor hex updated from
/// `#007AFF` (Apple HIG / `Accent/Brand`) to `#3E87FF` (`Accent/Blue`) as
/// confirmed by Figma CONTENTS traversal (Lab UI v.1, collection `4.1 Primitives`,
/// Light-mode mode `7644:0`). The semantic collection maps Info to `Accent/Blue`,
/// not to `Accent/Brand` (which is the brand/primary colour, a separate role).
///
/// With the new anchor (engine Oklab h≈259.9°) and brand=200°, the
/// smooth-asymptote resolver nudges the prototype only slightly (≈0.07°
/// displacement) — the prototype sits ~60° from the brand, which is far enough
/// for the displacement to be negligible. The resulting ladder is marginally
/// different from the prior snapshot: at mid step ~`#2F78F0` vs prior `#257BEC`,
/// reflecting the small hue shift between `#3E87FF` (h≈259.9°) and `#007AFF`
/// (h≈257.5° in the engine).
const SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#ECF3FD", "#CCDEFB", "#A1C2F8", "#6EA1F4", "#2F78F0", "#1858BE", "#1551B0",
    "#114598", "#0B3579", "#052456", "#021130", "#000108",
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
    // Prototype hex updated to Figma CONTENTS anchor (Accent/Blue = #3E87FF,
    // confirmed via collection 4.1 Primitives Light-mode traversal 2026-06-30).
    let curve = SentimentCurve::new(Sentiment::Info, 200.0, "#3E87FF", &neutral)
        .expect("Info sentiment with a far brand hue resolves");
    // Pin the resolution: Info prototype engine h≈259.9°, brand 200° (≈60° away)
    // nudges it only slightly — smooth displacement decays to near-zero.
    assert!(
        curve.was_displaced && curve.displacement < 5.0,
        "a far brand should nudge Info only slightly: displaced={}, delta={}",
        curve.was_displaced,
        curve.displacement
    );
    assert!(
        (curve.resolved_hue - 259.96).abs() < 0.1,
        "Info resolved hue should be ~259.96° (Figma Accent/Blue anchor, engine Oklab): {}",
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
