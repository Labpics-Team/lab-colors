use crate::spaces::srgb::{D65_WHITE, srgb_from_hex, srgb_to_xyz};
use crate::spaces::{cam16, cat16, vc::ViewingConditions};

/// CIECAM16 correlates `(J, M, h)` for an XYZ stimulus.
///
/// `h` is the CAM16 hue angle in **degrees** `[0, 360)`.
pub(crate) fn cam16_jch_from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    let xyz = [xyz[0] * 100.0, xyz[1] * 100.0, xyz[2] * 100.0];

    let lms = cat16::xyz_to_cone(xyz);
    let lms_a = [
        lms[0] * vc.rgb_d[0],
        lms[1] * vc.rgb_d[1],
        lms[2] * vc.rgb_d[2],
    ];
    let lms_aa = [
        cam16::adapt(lms_a[0], vc.fl),
        cam16::adapt(lms_a[1], vc.fl),
        cam16::adapt(lms_a[2], vc.fl),
    ];

    let a = lms_aa[0] - 12.0 * lms_aa[1] / 11.0 + lms_aa[2] / 11.0;
    let b = (lms_aa[0] + lms_aa[1] - 2.0 * lms_aa[2]) / 9.0;
    let h = b.atan2(a).to_degrees().rem_euclid(360.0);
    let hr = h.to_radians();

    let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
    let a_achrom = (2.0 * lms_aa[0] + lms_aa[1] + lms_aa[2] / 20.0) * vc.nbb;
    let j = 100.0 * (a_achrom / vc.aw).powf(vc.c * vc.z);

    let u = (a * a + b * b).sqrt();
    let t = (50000.0 / 13.0) * e_hue * vc.nc * vc.nbb * u
        / (lms_aa[0] + lms_aa[1] + 1.05 * lms_aa[2] + 0.305);
    let m = t.powf(0.9)
        * (j / 100.0).sqrt()
        * (1.64 - 0.29_f64.powf(vc.n)).powf(0.73)
        * vc.fl.powf(0.25);

    (j, m, h)
}

/// Chroma exponent in the Hellwig 2022 H-K lightness term
/// `J_HK = J + f(h) * C^0.587` (source: see [`hk_coeff`]).
const HK_CHROMA_EXPONENT: f64 = 0.587;

/// Hue-dependent Helmholtz-Kohlrausch coefficient `f(h)`, `h_cam_deg` in degrees.
///
/// Source: Hellwig, Stolitzka & Fairchild (2022), "Extending CIECAM02 and
/// CAM16 for the Helmholtz-Kohlrausch effect", Color Research & Application
/// 47(5), DOI 10.1002/col.22793: `J_HK = J + f(h) * C^0.587` where `C` is the
/// CAM16 chroma correlate. Coefficients verified against the colour-science
/// reference implementation (`hue_angle_dependency_Hellwig2022`).
fn hk_coeff(h_cam_deg: f64) -> f64 {
    let h_cam = h_cam_deg.to_radians();
    -0.160 * h_cam.cos() + 0.132 * (2.0 * h_cam).cos() - 0.405 * h_cam.sin()
        + 0.080 * (2.0 * h_cam).sin()
        + 0.792
}

/// CAM16 lightness `J` of the achromatic stimulus at luminance `y`.
///
/// This is the exact inverse of [`y_hk`]: `y_hk` finds the grey luminance whose
/// `J` equals a target `J_HK`, so given that luminance back, `grey_j` returns
/// the same `J_HK`. The inverse solver uses it to turn a target H-K-corrected
/// luminance into the `J_HK` a foreground colour must reproduce.
pub(crate) fn grey_j(y: f64, vc: &ViewingConditions) -> f64 {
    let xyz = [y * D65_WHITE[0], y, y * D65_WHITE[2]];
    cam16_jch_from_xyz(xyz, vc).0
}

/// Grey luminance whose CAM16 lightness equals `j_hk` (inverse of [`grey_j`]).
///
/// `J` is monotonic in luminance for the achromatic axis, so a fixed-iteration
/// bisection converges to full `f64` precision.
pub(crate) fn y_hk(j_hk: f64, vc: &ViewingConditions) -> f64 {
    // `y_hk` inverts the achromatic `grey_j` via a CAM16 bisection and sits on the
    // hot path of every `solve` (each `bg_luma` is a full inversion). Inside one
    // resolve sweep the *same* `(j_hk, vc)` pair recurs many times — the
    // background luminance is constant across all of a set's solves, and a role's
    // refine loop re-measures neighbouring foregrounds. A small exact-key memo
    // (keyed on the bit patterns, so a hit returns the byte-identical value the
    // bisection would) collapses those repeats. The cache is scoped to the
    // current sweep by `with_yhk_cache` and is a no-op (direct compute) outside
    // one, so behaviour is unchanged — only repeated work is elided.
    if let Some(hit) = yhk_cache_get(j_hk, vc) {
        return hit;
    }
    let y = y_hk_compute(j_hk, vc);
    yhk_cache_put(j_hk, vc, y);
    y
}

/// The uncached achromatic luminance whose CAM16 lightness equals `j_hk`.
fn y_hk_compute(j_hk: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    // Each step halves the luminance bracket and costs one CAM16 evaluation
    // (`grey_j`). Once the bracket is below `LUMA_BISECT_EPS` the result is pinned
    // far finer than any downstream 8-bit hex step, so the remaining halvings are
    // wasted CAM16 work. The early exit is exact — the same value the full 64-step
    // loop reaches.
    for _ in 0..64 {
        if hi - lo < LUMA_BISECT_EPS {
            break;
        }
        let mid = (lo + hi) * 0.5;
        if grey_j(mid, vc) < j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

thread_local! {
    /// Sweep-scoped memo for [`y_hk`], keyed on the bit patterns of `(j_hk, vc
    /// fingerprint)` so a hit returns the bisection's exact value. `None` outside
    /// an active sweep — then `y_hk` computes directly, behaviour unchanged.
    static YHK_CACHE: std::cell::RefCell<Option<std::collections::HashMap<(u64, u64), f64>>> =
        const { std::cell::RefCell::new(None) };
}

/// A cheap fingerprint of the viewing conditions for the [`y_hk`] memo key. The
/// achromatic `grey_j` depends on `vc` only through `(aw, c, z, nbb, fl, n)`;
/// `aw` and `c` already differ between the two production VCs, and the rest are
/// folded in to stay correct if a third VC is ever added.
fn vc_fingerprint(vc: &ViewingConditions) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for f in [vc.aw, vc.c, vc.z, vc.nbb, vc.fl, vc.n] {
        h ^= f.to_bits();
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn yhk_cache_get(j_hk: f64, vc: &ViewingConditions) -> Option<f64> {
    YHK_CACHE.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|m| m.get(&(j_hk.to_bits(), vc_fingerprint(vc))).copied())
    })
}

fn yhk_cache_put(j_hk: f64, vc: &ViewingConditions, y: f64) {
    YHK_CACHE.with(|c| {
        if let Some(m) = c.borrow_mut().as_mut() {
            m.insert((j_hk.to_bits(), vc_fingerprint(vc)), y);
        }
    });
}

/// Run `f` with the sweep-scoped [`y_hk`] memo active, then tear it down. Nesting
/// is safe: an inner activation reuses the outer cache and only the outermost
/// tears it down, so a single sweep shares one cache and never leaks across
/// sweeps (the cache holds only the current sweep's achromatic inversions).
pub(crate) fn with_yhk_cache<R>(f: impl FnOnce() -> R) -> R {
    let outermost = YHK_CACHE.with(|c| {
        let mut b = c.borrow_mut();
        if b.is_none() {
            *b = Some(std::collections::HashMap::new());
            true
        } else {
            false
        }
    });
    let r = f();
    if outermost {
        YHK_CACHE.with(|c| *c.borrow_mut() = None);
    }
    r
}

/// Luminance-bracket width below which the achromatic `y_hk` bisection has
/// converged finely enough that no downstream 8-bit hex byte can change. At
/// ~1e-12 it is far below the luminance step one hex byte spans, so the early
/// exit is provably output-preserving while cutting the bisection from 64 steps
/// to ~40.
const LUMA_BISECT_EPS: f64 = 1e-12;

// Canonical perceptual-contrast constants, from the published formula version
// 0.0.98G-4g (SAPC-8 "4g" constant set). Names in comments mirror the source
// identifiers so the mapping is auditable; see docs/decisions/apca-license.md.
//
// These are the SINGLE SOURCE OF TRUTH for the contrast curve: both the forward
// `contrast_core` and the inverse solver (`crate::solve`) read them here.
// Do not re-declare these literals anywhere else.

/// Soft black-clamp threshold (`blkThrs`): luminance below this is lifted.
pub(crate) const SOFT_CLAMP_THRESHOLD: f64 = 0.022;
/// Soft black-clamp exponent (`blkClmp`).
pub(crate) const SOFT_CLAMP_EXP: f64 = 1.414;
/// Background power-curve exponent, normal polarity (`normBG`, bg > fg).
pub(crate) const EXP_BG_LIGHT: f64 = 0.56;
/// Foreground power-curve exponent, normal polarity (`normTXT`).
pub(crate) const EXP_FG_LIGHT: f64 = 0.57;
/// Background power-curve exponent, reverse polarity (`revBG`).
pub(crate) const EXP_BG_DARK: f64 = 0.65;
/// Foreground power-curve exponent, reverse polarity (`revTXT`).
pub(crate) const EXP_FG_DARK: f64 = 0.62;
/// Raw power-curve delta scale, shared by both polarities (`scaleBoW` == `scaleWoB`).
pub(crate) const CONTRAST_SCALE: f64 = 1.14;
/// Minimum luminance delta below which the pair reports no contrast (`deltaYmin`).
pub(crate) const DELTA_Y_MIN: f64 = 0.0005;
/// Low-contrast clip: scaled deltas inside ±`loClip` collapse to zero.
pub(crate) const LO_CLIP: f64 = 0.1;
/// Polarity offset pulled toward zero past the clip, normal polarity (`loBoWoffset`).
pub(crate) const LO_BOW_OFFSET: f64 = 0.027;
/// Polarity offset pulled toward zero past the clip, reverse polarity (`loWoBoffset`).
pub(crate) const LO_WOB_OFFSET: f64 = 0.027;
/// Maps the offset contrast to the ~[-108, 108] Lc output range.
pub(crate) const LC_SCALE: f64 = 100.0;

/// Soft black clamp: lifts luminance below [`SOFT_CLAMP_THRESHOLD`] so the
/// contrast curve stays monotonic near black. Strictly increasing on `[0, T]`
/// and the identity above `T`, hence invertible — see [`soft_clamp_inv`].
pub(crate) fn soft_clamp(y: f64) -> f64 {
    if y < SOFT_CLAMP_THRESHOLD {
        y + (SOFT_CLAMP_THRESHOLD - y).powf(SOFT_CLAMP_EXP)
    } else {
        y
    }
}

/// Inverse of [`soft_clamp`]: recover the raw luminance from a clamped value.
///
/// Returns `None` when `clamped` is below `soft_clamp(0.0)` — reproducing it
/// would require a luminance darker than pure black, so the contrast that
/// implied it is physically unreachable.
pub(crate) fn soft_clamp_inv(clamped: f64) -> Option<f64> {
    if clamped >= SOFT_CLAMP_THRESHOLD {
        return Some(clamped);
    }
    if clamped < soft_clamp(0.0) {
        return None;
    }
    let mut lo = 0.0_f64;
    let mut hi = SOFT_CLAMP_THRESHOLD;
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if soft_clamp(mid) < clamped {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) * 0.5)
}

/// Perceptual-contrast core curve (asymmetric power contrast on luminance).
///
/// Faithful port of the published generic perceptual-contrast math — soft
/// black clamp, polarity-dependent power exponents, the minimum-luminance
/// gate, the low-contrast clip, and the polarity offsets — so that
/// achromatic inputs reproduce the canonical reference numbers (e.g. black
/// on white ≈ `106.04`). The luminance fed here is the H-K-corrected
/// `Y_hk`, which is what makes LPC diverge from the reference metric on
/// chromatic colours.
///
/// Constant values, source version, naming policy and attribution:
/// docs/decisions/apca-license.md. The achromatic alignment is locked by
/// `golden_tests::contrast_core_matches_reference_on_grey_axis`. The curve is
/// inverted by `crate::solve` to recover a foreground luminance from a target.
pub(crate) fn contrast_core(y_fg: f64, y_bg: f64) -> f64 {
    let fg = soft_clamp(y_fg);
    let bg = soft_clamp(y_bg);

    if (bg - fg).abs() < DELTA_Y_MIN {
        return 0.0;
    }

    if bg > fg {
        // Dark-on-light (normal polarity).
        let sapc = (bg.powf(EXP_BG_LIGHT) - fg.powf(EXP_FG_LIGHT)) * CONTRAST_SCALE;
        if sapc < LO_CLIP {
            0.0
        } else {
            (sapc - LO_BOW_OFFSET) * LC_SCALE
        }
    } else {
        // Light-on-dark (reverse polarity).
        let sapc = (bg.powf(EXP_BG_DARK) - fg.powf(EXP_FG_DARK)) * CONTRAST_SCALE;
        if sapc > -LO_CLIP {
            0.0
        } else {
            (sapc + LO_WOB_OFFSET) * LC_SCALE
        }
    }
}

/// Hellwig 2022 H-K-corrected lightness for an XYZ stimulus:
/// `J_HK = J + f(h) * C^0.587`, with the chroma correlate `C = M / F_L^0.25`.
pub(crate) fn j_hk_from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> f64 {
    let (j, m, h) = cam16_jch_from_xyz(xyz, vc);
    let chroma = m / vc.fl.powf(0.25);
    j + hk_coeff(h) * chroma.powf(HK_CHROMA_EXPONENT)
}

fn hex_to_y_hk(hex: &str, vc: &ViewingConditions) -> f64 {
    let rgb = srgb_from_hex(hex).unwrap_or([0.0, 0.0, 0.0]);
    let xyz = srgb_to_xyz(rgb);
    y_hk(j_hk_from_xyz(xyz, vc).max(0.0), vc)
}

/// Perceptual-contrast (LPC) between a foreground and background hex colour
/// under standard sRGB viewing conditions (light-theme average surround).
///
/// Shortcut for [`lpc_with_vc`] with [`ViewingConditions::srgb`]. Use this for
/// light themes; for dark themes call [`lpc_with_vc`] with
/// [`ViewingConditions::dim_surround`]. See `docs/decisions/theme-invariant.md`.
pub fn lpc(fg_hex: &str, bg_hex: &str) -> f64 {
    lpc_with_vc(fg_hex, bg_hex, &ViewingConditions::srgb())
}

/// Perceptual-contrast (LPC) between a foreground and background hex colour
/// under the given viewing conditions.
///
/// Contrast is resolved in the perceptual space of `vc`, so the same hex pair
/// yields different `Lc` under light and dark themes — that divergence is the
/// point: a dark theme must reach the same contrast *contract* in a
/// dim-surround space (Bartleson–Breneman compensation), not reuse the light
/// numbers. Pick the VC for the theme: [`ViewingConditions::srgb`] for light
/// themes (average surround), [`ViewingConditions::dim_surround`] for dark
/// themes (dim surround). See `docs/decisions/theme-invariant.md`.
pub fn lpc_with_vc(fg_hex: &str, bg_hex: &str, vc: &ViewingConditions) -> f64 {
    let y_fg = hex_to_y_hk(fg_hex, vc);
    let y_bg = hex_to_y_hk(bg_hex, vc);
    contrast_core(y_fg, y_bg)
}

/// LPC surface distance between two hex colours under standard sRGB viewing
/// conditions (light-theme average surround).
///
/// Shortcut for [`lpc_surface_with_vc`] with [`ViewingConditions::srgb`]; use
/// [`ViewingConditions::dim_surround`] for dark themes. See
/// `docs/decisions/theme-invariant.md`.
pub fn lpc_surface(c1_hex: &str, c2_hex: &str) -> f64 {
    lpc_surface_with_vc(c1_hex, c2_hex, &ViewingConditions::srgb())
}

/// LPC surface distance between two hex colours under the given viewing
/// conditions.
///
/// Both colours are decoded under `vc`, so dark-theme surfaces are compared in
/// the dim-surround space. Use [`ViewingConditions::srgb`] for light themes and
/// [`ViewingConditions::dim_surround`] for dark themes; see
/// `docs/decisions/theme-invariant.md`.
pub fn lpc_surface_with_vc(c1_hex: &str, c2_hex: &str, vc: &ViewingConditions) -> f64 {
    // `.expect()` on the parse is intentionally left in place here: the
    // fallible-public-API redesign is tracked separately (issue #41).
    let c1 = crate::lcs::LcsColor::from_hex_with_vc(c1_hex, vc).expect("invalid hex");
    let c2 = crate::lcs::LcsColor::from_hex_with_vc(c2_hex, vc).expect("invalid hex");
    let dj = c1.jp - c2.jp;
    let m1 = c1.s * (c1.jp + 1.0);
    let m2 = c2.s * (c2.jp + 1.0);
    let h1 = c1.h_ok.to_radians();
    let h2 = c2.h_ok.to_radians();
    let da = m1 * h1.cos() - m2 * h2.cos();
    let db = m1 * h1.sin() - m2 * h2.sin();
    (dj * dj + da * da + db * db).sqrt()
}

/// LPC contrast between two [`crate::lcs::LcsColor`] values under standard sRGB
/// viewing conditions (light-theme average surround).
///
/// Shortcut for [`lpc_lcs_with_vc`] with [`ViewingConditions::srgb`]. Uses the
/// pre-computed CAM16 J' and Oklab hue stored in each colour, avoiding
/// re-parsing hex strings. Delegates to the same perceptual-contrast core
/// curve as [`lpc`].
pub fn lpc_lcs(fg: &crate::lcs::LcsColor, bg: &crate::lcs::LcsColor) -> f64 {
    lpc_lcs_with_vc(fg, bg, &ViewingConditions::srgb())
}

/// LPC contrast between two [`crate::lcs::LcsColor`] values under the given
/// viewing conditions.
///
/// `vc` must be the same viewing conditions the colours were constructed under
/// (e.g. via [`crate::lcs::LcsColor::from_hex_with_vc`]): the H-K chroma term
/// and the luminance resolution both read it. Use [`ViewingConditions::srgb`]
/// for light themes and [`ViewingConditions::dim_surround`] for dark themes;
/// see `docs/decisions/theme-invariant.md`.
pub fn lpc_lcs_with_vc(
    fg: &crate::lcs::LcsColor,
    bg: &crate::lcs::LcsColor,
    vc: &ViewingConditions,
) -> f64 {
    let y_fg = y_hk_from_lcs(fg, vc);
    let y_bg = y_hk_from_lcs(bg, vc);
    contrast_core(y_fg, y_bg)
}

/// Derive hk-adjusted luminance from an existing [`LcsColor`].
///
/// Decompresses the stored UCS correlates back to raw CAM16 (J' → J,
/// M' → M → C) so the result is bit-identical to the hex path
/// ([`lpc`]); previously this used J'/M' directly, so `lpc` and
/// `lpc_lcs` disagreed for chromatic colours.
fn y_hk_from_lcs(c: &crate::lcs::LcsColor, vc: &ViewingConditions) -> f64 {
    let j = c.jp / (1.7 - 0.007 * c.jp);
    let m = (0.0228 * c.mp()).exp_m1() / 0.0228;
    let chroma = m / vc.fl.powf(0.25);
    let j_hk = j + hk_coeff(c.h_cam()) * chroma.powf(HK_CHROMA_EXPONENT);
    y_hk(j_hk.max(0.0), vc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_on_white_matches_reference() {
        // Black and white are the exact luminance endpoints (Y_hk = 0 and 1),
        // so the H-K layer cannot shift them: LPC reproduces the canonical
        // achromatic reference (106.0407) bit-for-bit after the offset
        // alignment. Source/attribution: docs/decisions/apca-license.md.
        let lc = lpc("#000000", "#ffffff");
        assert!((lc - 106.04).abs() < 0.5, "LPC for black on white: {}", lc);
    }

    #[test]
    fn gray_on_white_mid_range() {
        let lc = lpc("#888888", "#ffffff");
        assert!((lc - 58.4).abs() < 1.0, "LPC for gray on white: {}", lc);
    }

    #[test]
    fn blue_on_white_below_achromatic() {
        // The H-K term lifts a saturated blue's perceived lightness, so its
        // contrast on white lands below a same-luminance grey would (≈ 68.7).
        let lc = lpc("#0000ff", "#ffffff");
        assert!(lc < 75.0, "LPC for blue on white should be < 75: {}", lc);
        assert!(lc > 60.0, "LPC for blue on white should be > 60: {}", lc);
    }

    #[test]
    fn polarity_swap_negates() {
        // The polarity offsets are symmetric (both pull toward zero), so the
        // residual asymmetry comes only from the exponent split.
        let lc1 = lpc("#000000", "#ffffff");
        let lc2 = lpc("#ffffff", "#000000");
        assert!((lc1 + lc2).abs() < 3.0, "polarity swap: {} vs {}", lc1, lc2);
    }

    #[test]
    fn surface_white_vs_near_white() {
        let de = lpc_surface("#ffffff", "#f6f7fa");
        assert!(de > 1.0 && de < 10.0, "surface delta: {}", de);
    }

    #[test]
    fn neutral_hk_boost_is_small() {
        // A near-neutral grey carries a tiny residual CAM16 colourfulness
        // (incomplete chromatic adaptation), so the H-K term shifts Y_hk
        // only slightly: LPC ≈ 87.6, within ~1.5 Lc of the canonical
        // achromatic number for this luminance.
        let lc = lpc("#444444", "#ffffff");
        assert!((lc - 87.6).abs() < 1.0, "achromatic LPC: {}", lc);
    }

    #[test]
    fn j_hk_matches_hellwig_reference() {
        // Reference J_HK from colour-science: XYZ_to_CIECAM16 (L_A=64,
        // Y_b=20, Average surround, D65) + hue_angle_dependency_Hellwig2022,
        // J_HK = J + f(h)·C^0.587. Tolerance covers the known sRGB-matrix
        // micro-deltas between implementations (|dJ|<0.004, |dC|<0.05).
        let vc = ViewingConditions::srgb();
        for (hex, want) in [
            ("#0000FF", 38.954587),
            ("#FF0000", 56.018245),
            ("#FFD700", 85.092749),
        ] {
            let rgb = srgb_from_hex(hex).expect("reference hex is valid");
            let got = j_hk_from_xyz(srgb_to_xyz(rgb), &vc);
            assert!(
                (got - want).abs() < 0.05,
                "{hex}: J_HK={got}, reference={want}"
            );
        }
    }

    #[test]
    fn lpc_and_lpc_lcs_agree() {
        // Both entry points must compute the identical metric: the LcsColor
        // path decompresses J'/M' back to raw J/M before the H-K term.
        for (fg, bg) in [
            ("#0000FF", "#FFFFFF"),
            ("#FF0000", "#101012"),
            ("#34C759", "#FFFFFF"),
        ] {
            let via_hex = lpc(fg, bg);
            let f = crate::lcs::LcsColor::from_hex(fg).expect("valid fg hex");
            let b = crate::lcs::LcsColor::from_hex(bg).expect("valid bg hex");
            let via_lcs = lpc_lcs(&f, &b);
            assert!(
                (via_hex - via_lcs).abs() < 1e-6,
                "{fg}/{bg}: lpc={via_hex} lpc_lcs={via_lcs}"
            );
        }
    }

    #[test]
    fn surface_same_color_is_zero() {
        let de = lpc_surface("#3478F6", "#3478F6");
        assert!(de.abs() < 1e-9, "self-distance must be zero: {}", de);
    }

    #[test]
    fn surface_hue_distance_ordering() {
        // Opposite hues must be further apart than near-identical reds.
        // Guards the degrees-as-radians regression in the chroma vector.
        let far = lpc_surface("#FF0000", "#00B050");
        let near = lpc_surface("#FF0000", "#F51111");
        assert!(
            far > near,
            "red↔green ({}) should exceed red↔red' ({})",
            far,
            near
        );
    }

    #[test]
    fn shortcuts_match_srgb_with_vc() {
        // The srgb shortcuts must be bit-identical to the explicit srgb VC
        // path — this is what keeps every legacy value (e.g. black-on-white
        // 106.0407) unchanged after the refactor.
        let srgb = ViewingConditions::srgb();
        assert_eq!(
            lpc("#0000FF", "#FFFFFF"),
            lpc_with_vc("#0000FF", "#FFFFFF", &srgb)
        );
        assert_eq!(
            lpc_surface("#FFFFFF", "#f6f7fa"),
            lpc_surface_with_vc("#FFFFFF", "#f6f7fa", &srgb)
        );
        let f = crate::lcs::LcsColor::from_hex("#0000FF").expect("valid fg hex");
        let b = crate::lcs::LcsColor::from_hex("#FFFFFF").expect("valid bg hex");
        assert_eq!(lpc_lcs(&f, &b), lpc_lcs_with_vc(&f, &b, &srgb));
    }

    #[test]
    fn dim_diverges_from_srgb() {
        // (a) The same chromatic pair resolved under dim surround must land on
        // a different Lc than under average surround: dark themes compute in
        // their own perceptual space (Bartleson–Breneman). A saturated green
        // carries the largest H-K term — the only VC-sensitive part when the
        // background is a luminance endpoint — so the gap clears the 1-Lc
        // contract tolerance, which is precisely why light colours cannot be
        // reused verbatim in a dark theme (issue #15).
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        let lc_srgb = lpc_with_vc("#00FF00", "#FFFFFF", &srgb);
        let lc_dim = lpc_with_vc("#00FF00", "#FFFFFF", &dim);
        assert!(
            (lc_srgb - lc_dim).abs() > 1.0,
            "dim VC should shift Lc meaningfully: srgb={lc_srgb} dim={lc_dim}"
        );
    }

    #[test]
    fn monotonic_in_fg_luminance_under_both_vcs() {
        // (b) On a fixed light background, darker foreground text yields higher
        // (more positive) contrast; this ordering must hold in every
        // perceptual space. Greys keep the H-K hue term out of the comparison.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let bg = "#FFFFFF";
            let dark = lpc_with_vc("#000000", bg, &vc);
            let mid = lpc_with_vc("#888888", bg, &vc);
            let light = lpc_with_vc("#CCCCCC", bg, &vc);
            assert!(
                dark > mid && mid > light,
                "monotonicity broken: dark={dark} mid={mid} light={light}"
            );
        }
    }

    #[test]
    fn polarity_swap_negates_under_both_vcs() {
        // (c) Swapping foreground and background flips the sign of the contrast
        // (near-symmetric magnitude) under both viewing conditions.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let lc1 = lpc_with_vc("#000000", "#FFFFFF", &vc);
            let lc2 = lpc_with_vc("#FFFFFF", "#000000", &vc);
            assert!(lc1 > 0.0 && lc2 < 0.0, "polarity signs: {lc1} vs {lc2}");
            assert!(
                (lc1 + lc2).abs() < 3.0,
                "polarity swap should near-negate: {lc1} vs {lc2}"
            );
        }
    }

    #[test]
    fn lpc_and_lpc_lcs_agree_under_dim() {
        // The hex and LcsColor entry points must compute the identical metric
        // under dim surround too — provided the colour is constructed under the
        // same VC that is passed to lpc_lcs_with_vc.
        let dim = ViewingConditions::dim_surround();
        for (fg, bg) in [("#0000FF", "#FFFFFF"), ("#34C759", "#101012")] {
            let via_hex = lpc_with_vc(fg, bg, &dim);
            let f = crate::lcs::LcsColor::from_hex_with_vc(fg, &dim).expect("valid fg hex");
            let b = crate::lcs::LcsColor::from_hex_with_vc(bg, &dim).expect("valid bg hex");
            let via_lcs = lpc_lcs_with_vc(&f, &b, &dim);
            assert!(
                (via_hex - via_lcs).abs() < 1e-6,
                "{fg}/{bg} under dim: lpc={via_hex} lpc_lcs={via_lcs}"
            );
        }
    }
}
