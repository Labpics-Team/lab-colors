//! R3 byte-identity differential tests.
//!
//! REGIME R3: semantic-extraction / value-preserving identity. The perceptual
//! const-extraction in `semantic.rs` (the `// NEEDS-SCIENCE` / `// GROUNDED`
//! marker commits) must be value-preserving — no emitted accent or sentiment hex
//! value may change as a result of adding markers or restructuring comments.
//!
//! BUG CLASS these tests guard: a comment-marker commit silently shifts a curve
//! coefficient or const RHS (e.g. reformats `0.10` → `0.1` as a side effect of
//! an editor save, or introduces a whitespace change that the const parser
//! mislabels). The emitted perceptual values (sRGB hex) would change byte-for-byte
//! while every property test (in-gamut, monotone, contrast) still passes — the
//! regression is invisible without pinned byte outputs.
//!
//! HOW THESE TESTS BITE (mutation proof — characterization / pin scope):
//!
//! R3 tests are characterization locks of ALREADY-CORRECT behaviour: they are
//! GREEN at birth BY DESIGN (the golden constants match the current computation).
//! Per the constitution ("CHARACTERIZATION / PIN / regression-lock of
//! ALREADY-CORRECT behavior → prove it bites by deliberately BREAKING the
//! asserted invariant in a THROWAWAY copy"), we prove each bites by:
//!
//!   (A) `r3_sample_hex_13_byte_identity`: mutating one entry in the local GOLDEN
//!       copy → the `assert_eq!` on the hex ladder fails, naming the drifted stop.
//!
//!   (B) `r3_resolve_set_240_cell_byte_identity`: mutating one cell in a local
//!       GOLDEN_SPOT const → `assert_eq!` on the matching (vc, bg, role) row
//!       fails naming the triple.
//!
//! RELATIONSHIP TO EXISTING TESTS:
//!   • `accent_golden.rs` / `sentiment_info_curve_sample_hex_13_matches_golden`
//!     already pin the 13-stop ladders for the same inputs under the same goldens.
//!     These R3 tests are separate test IDs that carry the R3 regime label and
//!     run independently, so a rebase that removes `accent_golden.rs` would still
//!     leave R3 coverage intact.
//!   • `semantic.rs::resolve_set_golden_hex_is_byte_for_byte_stable` already pins
//!     the 240-cell grid as an INTERNAL `#[test]`. The external R3 test here pins
//!     a SUBSET (one representative cell per vc × bg pair) and carries the regime
//!     label, so the R3 gate is independently reachable from `--test r3_byte_identity`.
//!
//! INVARIANTS asserted (INV from the testPlan):
//!   INV (1): zero emitted accent/sentiment hex values change.
//!   INV (1): zero resolved-token values change across the full grid (representative).

use labcolors_core::{
    BgInput, Resolved, Role, RoleTable, ViewingConditions,
    neutral::NeutralCurve,
    resolve_set,
    scale::AccentCurve,
    sentiment::{Sentiment, SentimentCurve},
};

// ─────────────────────────────────────────────────────────────────────────────
// R3-A: sample_hex(13) golden ladder for two representative curves.
//
// These constants mirror the goldens in `accent_golden.rs` and
// `accent_golden.rs::SENTIMENT_INFO_GOLDEN`. They are DUPLICATED here (not
// imported) so this R3 test is self-contained and independent: if the source of
// truth golden is renamed or the accent_golden.rs file is deleted, this test
// continues to assert the invariant.
//
// GOLDEN SOURCE: captured 2026-06-12 at main@f21aac7 via `sample_hex(13)`.
// ─────────────────────────────────────────────────────────────────────────────

/// AccentCurve("#007AFF").sample_hex(13) — byte-identical to main@f21aac7.
/// Any change to this ladder is a REGIME-R3 regression.
const R3_ACCENT_007AFF_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F4F8FF", "#DAE9FF", "#B6D4FF", "#88B9FF", "#4F98FF", "#0072F0", "#006BE2",
    "#005FC9", "#004FAA", "#003C85", "#00275B", "#000F2B",
];

/// SentimentCurve(Info, 200°, "#007AFF").sample_hex(13) — byte-identical to main@f21aac7.
/// Any change to this ladder is a REGIME-R3 regression.
const R3_SENTIMENT_INFO_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#EDF3FE", "#CCDEFB", "#A2C2F8", "#6FA1F4", "#3278F0", "#1756C0", "#1550B2",
    "#104499", "#0B357B", "#052357", "#021030", "#000108",
];

fn canonical_neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("R3: canonical neutral anchors are valid")
}

/// R3: AccentCurve::new("#007AFF", &neutral).sample_hex(13) produces byte-identical
/// output to the golden captured at main@f21aac7.
///
/// This test is GREEN at birth (characterization lock). It bites on mutation:
/// change any entry in `R3_ACCENT_007AFF_GOLDEN` → `assert_eq!` fails naming
/// the stop index and the drifted value.
#[test]
fn r3_sample_hex_13_accent_007aff_byte_identity() {
    let neutral = canonical_neutral();
    let accent = AccentCurve::new("#007AFF", &neutral)
        .expect("R3: #007AFF is a valid accent seed at main@f21aac7");
    let got = accent.sample_hex(13);
    assert_eq!(
        got.as_slice(),
        R3_ACCENT_007AFF_GOLDEN.as_slice(),
        "R3 REGRESSION — AccentCurve('#007AFF') sample_hex(13) is NOT byte-identical to \
         the golden captured at main@f21aac7. Either a perceptual const RHS changed as a \
         side-effect of a marker commit, or a deliberate recalibration occurred (which \
         requires an explicit golden update and owner sign-off, not a silent edit)."
    );
}

/// R3: SentimentCurve(Info, 200°, "#007AFF").sample_hex(13) produces byte-identical
/// output to the golden captured at main@f21aac7.
///
/// This test is GREEN at birth (characterization lock). It bites on mutation:
/// change any entry in `R3_SENTIMENT_INFO_GOLDEN` → `assert_eq!` fails.
#[test]
fn r3_sample_hex_13_sentiment_info_byte_identity() {
    let neutral = canonical_neutral();
    let curve = SentimentCurve::new(Sentiment::Info, 200.0, "#007AFF", &neutral)
        .expect("R3: Info sentiment with far brand hue resolves at main@f21aac7");
    let got = curve.sample_hex(13);
    assert_eq!(
        got.as_slice(),
        R3_SENTIMENT_INFO_GOLDEN.as_slice(),
        "R3 REGRESSION — SentimentCurve(Info, 200°, '#007AFF') sample_hex(13) is NOT \
         byte-identical to the golden captured at main@f21aac7. Check for const RHS \
         drift caused by a marker commit or reformatting of a perceptual coefficient."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// R3-B: resolve_set 240-cell byte-identity (representative subset).
//
// The full 240-cell grid is pinned by `semantic::resolve_set_golden_hex_is_byte_for_byte_stable`
// as an internal `#[test]`. Here we independently pin one representative cell
// per (vc, bg) combination (12 cells = 6 backgrounds × 2 VCs × 1 role each).
// This is sufficient to catch a coefficient drift that moves ALL cells for a
// given (vc, bg) — the class of regression the test plan names as "R3 240-cell
// resolve_set byte-identity". The full grid assertion lives in semantic.rs and
// is still run as part of `cargo test --workspace`.
//
// GOLDEN SOURCE: captured 2026-06-12 at main@f21aac7 from the same GOLDEN
// table in `semantic.rs::resolve_set_golden_hex_is_byte_for_byte_stable`.
// ─────────────────────────────────────────────────────────────────────────────

/// One representative (vc, bg, role, expected_hex) per (vc, bg) combination.
/// Sampled from the 240-cell GOLDEN table in semantic.rs at main@f21aac7.
/// `label-primary` is chosen as the representative because it is the highest-
/// contrast text role and the most sensitive canary for a lightness-shift.
const R3_RESOLVE_SET_SPOTS: [(&str, &str, &str, &str); 12] = [
    // sRGB viewing conditions — sourced verbatim from the 240-cell GOLDEN in
    // semantic.rs::resolve_set_golden_hex_is_byte_for_byte_stable at main@f21aac7.
    ("srgb", "#FFFFFF", "label-primary", "#0A0A10"),
    ("srgb", "#F2F2F7", "label-primary", "#09090F"),
    ("srgb", "#7F7F7F", "label-primary", "#010103"),
    ("srgb", "#1C1C1E", "label-primary", "#F1F1FD"),
    ("srgb", "#101012", "label-primary", "#F2F2FC"),
    ("srgb", "#3478F6", "label-primary", "#020205"),
    // Dim (display / dark-room) viewing conditions — same source.
    ("dim", "#FFFFFF", "label-primary", "#0D0D12"),
    ("dim", "#F2F2F7", "label-primary", "#0C0C12"),
    ("dim", "#7F7F7F", "label-primary", "#030305"),
    ("dim", "#1C1C1E", "label-primary", "#F0F1FA"),
    ("dim", "#101012", "label-primary", "#F0F0FA"),
    ("dim", "#3478F6", "label-primary", "#040408"),
];

/// R3: the representative cells from the 240-cell resolve_set grid are
/// byte-identical to the values at main@f21aac7.
///
/// This test is GREEN at birth (characterization lock). It bites on mutation:
/// change any entry in `R3_RESOLVE_SET_SPOTS` → `assert_eq!` fails naming the
/// (vc, bg, role) triple and the drifted hex.
#[test]
fn r3_resolve_set_240_cell_representative_byte_identity() {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();

    for (vc_name, bg_hex, role_key, expected_hex) in R3_RESOLVE_SET_SPOTS {
        let vc = match vc_name {
            "srgb" => &srgb,
            "dim" => &dim,
            other => panic!("R3: unknown vc name '{other}' in R3_RESOLVE_SET_SPOTS"),
        };
        let bg = BgInput::solid(bg_hex)
            .unwrap_or_else(|_| panic!("R3: invalid bg_hex '{bg_hex}' in R3_RESOLVE_SET_SPOTS"));
        let set = resolve_set(&bg, &table, vc);

        let got = set
            .iter()
            .find(|(role, _)| role.key() == role_key)
            .map(|(_, resolved)| match resolved {
                Resolved::Color { solved, .. } => solved.hex().to_string(),
                Resolved::None => "none".to_string(),
                Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
            })
            .unwrap_or_else(|| {
                panic!(
                    "R3: role '{role_key}' not found in resolve_set output for \
                     vc={vc_name} bg={bg_hex}"
                )
            });

        assert_eq!(
            got, expected_hex,
            "R3 REGRESSION — resolve_set({vc_name}, {bg_hex}, {role_key}) = '{got}', \
             expected '{expected_hex}' (byte-identical to main@f21aac7). A perceptual \
             const RHS changed as a side-effect of a marker/comment commit. Either \
             restore the coefficient or update the golden with owner sign-off."
        );
    }
}

/// R3 sanity: ensure `resolve_set` returns all roles for EVERY spot — a missing
/// role means the find above would silently skip a row and give false green.
/// Asserts the output length matches `Role::ALL.len()` for each spot background.
#[test]
fn r3_resolve_set_returns_all_roles_for_every_spot_background() {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();
    let expected_len = Role::ALL.len();

    for (vc_name, bg_hex, _, _) in R3_RESOLVE_SET_SPOTS {
        let vc = match vc_name {
            "srgb" => &srgb,
            "dim" => &dim,
            other => panic!("R3: unknown vc '{other}'"),
        };
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, vc);
        assert_eq!(
            set.len(),
            expected_len,
            "R3 sanity FAILED — resolve_set returned {} roles for ({vc_name}, {bg_hex}), \
             expected {expected_len}. A role was added or removed without updating the \
             R3 golden spots.",
            set.len()
        );
    }
}
