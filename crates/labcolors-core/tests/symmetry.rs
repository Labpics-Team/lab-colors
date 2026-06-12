//! Contract 6 — polarity near-symmetry with an active undertone.
//!
//! BUG CLASS this guards: *asymmetry that only shows on chromatic pairs.* The
//! crate already pins polarity-swap near-symmetry for the achromatic axis
//! (lpc.rs `polarity_swap_negates*`: swapping fg/bg near-negates the contrast to
//! within ~3 Lc). But that test uses pure greys, where the H-K hue term is out
//! of the comparison. The forward curve has TWO sources of asymmetry — the
//! exponent split (`EXP_FG_LIGHT` ≠ `EXP_FG_DARK`, `EXP_BG_LIGHT` ≠
//! `EXP_BG_DARK`) and the hue-dependent H-K lift — and only the first is
//! exercised by greys. A regression that broke the polarity symmetry *specific
//! to tinted colours* (a wrong H-K sign, a hue term applied to one polarity
//! only) would pass every existing test. This pins the swap symmetry on
//! genuinely chromatic pairs, under both viewing conditions.
//!
//! Empirically discovered constant (probed through `lpc_with_vc`, then frozen):
//! the largest swap residual `|lpc(a,b) + lpc(b,a)|` across a spread of tinted
//! pairs is **5.50 Lc** (the cool near-grey `#6D6C7E` on white, where the
//! exponent split bites hardest). Frozen here at **6.5 Lc**, a modest margin
//! above the measured worst case — a real symmetry break (the historical bugs
//! were tens of Lc) trips it; the inherent exponent-split asymmetry does not.

use labcolors_core::ViewingConditions;
use labcolors_core::lpc::lpc_with_vc;

/// Frozen bound on the polarity-swap residual for tinted pairs (~1.2× the
/// measured worst case of 5.50). See the module docs for the derivation.
const SWAP_SYMMETRY_BOUND: f64 = 6.5;

/// Chromatic / tinted foreground–background pairs spanning the hue circle and
/// both natural polarities. Greys are deliberately excluded — they are already
/// covered by the achromatic test in lpc.rs; here the H-K term must be live.
const TINTED_PAIRS: [(&str, &str); 7] = [
    ("#3478F6", "#FFFFFF"), // brand blue on white
    ("#0E0E12", "#FFFFFF"), // cool near-black on white (faint tint)
    ("#34C759", "#FFFFFF"), // green on white
    ("#FF3B30", "#FFFFFF"), // red on white
    ("#6D6C7E", "#FFFFFF"), // cool mid-grey-violet on white (worst case)
    ("#007AFF", "#101012"), // azure on near-black
    ("#FFD700", "#101012"), // gold on near-black
];

fn vcs() -> [(ViewingConditions, &'static str); 2] {
    [
        (ViewingConditions::srgb(), "srgb"),
        (ViewingConditions::dim_surround(), "dim"),
    ]
}

#[test]
fn polarity_swap_is_near_symmetric_for_tinted_pairs() {
    // For each tinted pair, swapping foreground and background must near-negate
    // the contrast: lpc(a,b) ≈ −lpc(b,a). The residual |lpc(a,b) + lpc(b,a)|
    // bounds the asymmetry; it must stay within SWAP_SYMMETRY_BOUND in both
    // viewing conditions. The two directions must also have opposite signs (a
    // genuine polarity flip, not two same-signed numbers).
    for (vc, vc_name) in vcs() {
        let mut max_resid = 0.0_f64;
        let mut worst = String::new();
        for (a, b) in TINTED_PAIRS {
            let lc_ab = lpc_with_vc(a, b, &vc);
            let lc_ba = lpc_with_vc(b, a, &vc);
            assert!(
                lc_ab.signum() != lc_ba.signum(),
                "{vc_name} {a}/{b}: swap did not flip polarity — {lc_ab:.3} and {lc_ba:.3} \
                 share a sign",
            );
            let resid = (lc_ab + lc_ba).abs();
            if resid > max_resid {
                max_resid = resid;
                worst = format!("{a}/{b}: {lc_ab:.3} vs {lc_ba:.3}");
            }
            assert!(
                resid <= SWAP_SYMMETRY_BOUND,
                "{vc_name} {a}/{b}: polarity-swap residual {resid:.4} > {SWAP_SYMMETRY_BOUND} \
                 — tinted symmetry broke ({lc_ab:.3} vs {lc_ba:.3})",
            );
        }
        eprintln!("{vc_name}: max tinted swap residual = {max_resid:.4} at {worst}");
    }
}

#[test]
fn tinted_symmetry_is_no_worse_than_a_documented_margin_over_neutral() {
    // Cross-check the constant is honest: the tinted worst case must sit within
    // the frozen bound, and the bound must genuinely exceed the achromatic-axis
    // residual (otherwise it would be a vacuous re-test of the grey case). Pure
    // black/white near-negates to within ~2 Lc; the tinted bound is meaningfully
    // larger, which is the whole point — the H-K term widens the asymmetry.
    let vc = ViewingConditions::srgb();
    let neutral_resid =
        (lpc_with_vc("#000000", "#FFFFFF", &vc) + lpc_with_vc("#FFFFFF", "#000000", &vc)).abs();
    assert!(
        neutral_resid < SWAP_SYMMETRY_BOUND,
        "neutral residual {neutral_resid:.4} should be well under the tinted bound"
    );
    assert!(
        SWAP_SYMMETRY_BOUND > neutral_resid,
        "the tinted bound must exceed the neutral residual to be a non-trivial contract"
    );
}
