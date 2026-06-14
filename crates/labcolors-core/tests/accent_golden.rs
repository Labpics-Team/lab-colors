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
/// CONSCIOUS SNAPSHOT CHANGE — the **unified perceived-lightness law**. The old
/// per-hue `target_mp` colourfulness model (each hue to a fraction of its own
/// gamut ceiling, with a hand-tuned green cap) is replaced by ONE rule for every
/// hue, with no per-hue caps: the four sentiments share a single **perceived
/// (H-K) lightness `j_hk` ladder** (the neutral grey's), and each hue is placed at
/// that perceived lightness at `CHROMA_FRACTION` (0.88) of the in-gamut maximum
/// chroma. Equal `j_hk` ⇒ equal perceived brightness and contrast at each step
/// (nothing out-shouts); max chroma ⇒ nothing dull. A saturated hue therefore
/// sits at a *lower base lightness* so its H-K brightness boost lands it on the
/// shared ladder — which is why this Info blue is deeper and more saturated than
/// the previous snapshot (e.g. mid step `#3278F0` vs the old `#6498F1`). The hue
/// resolution is unchanged (smooth-asymptote model; brand 200° → ~260.4°).
const SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#EDF3FE", "#CCDEFB", "#A2C2F8", "#6FA1F4", "#3278F0", "#1756C0", "#1550B2",
    "#104499", "#0B357B", "#052357", "#021030", "#000108",
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
    // Pin the resolution under the restored smooth-asymptote model: the Info
    // prototype is the anchor's Oklab hue (257.42°), and brand 200° (57.4° away)
    // nudges it only slightly — the smooth displacement decays but never reaches
    // exactly zero, so the hue settles at ~260.43° with a small (~3°) displacement.
    assert!(
        curve.was_displaced && curve.displacement < 5.0,
        "a far brand should nudge Info only slightly: displaced={}, Δ={}",
        curve.was_displaced,
        curve.displacement
    );
    assert!(
        (curve.resolved_hue - 260.43).abs() < 0.1,
        "Info resolved hue should be ~260.43° (257.42° prototype + ~3° smooth nudge): {}",
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
