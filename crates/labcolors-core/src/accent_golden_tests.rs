//! Contract 7 — AccentCurve golden snapshot.
//!
//! BUG CLASS this guards: *silent value drift.* Every other test in this crate
//! checks a *property* — in-gamut, monotone J', non-negative saturation, hits a
//! contrast target. None of them pins the *actual emitted colours*. A change to
//! a curve coefficient, the chroma envelope, the hue-optimisation search, or the
//! CAM16-UCS rescaling could shift every swatch by a few bytes while keeping all
//! the properties true — and no test would notice. The Bracket-path LUT seam
//! (#50/#53) was exactly this shape: a value that moved without a property
//! breaking. This file freezes the exact byte output of one representative
//! curve sampled at 13 stops.
//!
//! A failure here is NOT automatically a bug: a deliberate recalibration of a
//! curve is a legitimate, intentional change of the snapshot. The rule is that
//! it must be a *conscious* swap — read the diff, confirm the new ladder is the
//! intended one, and update the constant. Drift that nobody chose is the
//! regression; the snapshot makes the difference visible instead of invisible.
//!
//! Snapshot captured 2026-06-12 from the curve's own `sample_hex(13)` through
//! its inherited (srgb) viewing conditions.

use crate::curve::ColorCurve;
use crate::neutral::NeutralCurve;
use crate::scale::AccentCurve;

/// The system neutral ladder the accent curve is built on.
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
fn golden_endpoints_anchor_to_white_and_near_black() {
    // A cheap structural guard so an accidental wholesale replacement of the
    // golden constant (e.g. all-white) can't pass: the ladder starts at pure
    // white and descends to a dark near-black.
    assert_eq!(ACCENT_007AFF_GOLDEN[0], "#FFFFFF");
    let luma = |hex: &str| -> u32 {
        let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
        ((v >> 16) & 0xFF) + ((v >> 8) & 0xFF) + (v & 0xFF)
    };
    assert!(
        luma(ACCENT_007AFF_GOLDEN[0]) > luma(ACCENT_007AFF_GOLDEN[12]),
        "golden ladder must darken from first to last stop"
    );
}
