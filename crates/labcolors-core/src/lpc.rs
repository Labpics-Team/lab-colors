use crate::spaces::srgb::{D65_WHITE, srgb_from_hex, srgb_to_xyz};
use crate::spaces::{cam16, cat16, vc::ViewingConditions};

/// CIECAM16 correlates `(J, M, h)` for an XYZ stimulus.
///
/// `h` is the CAM16 hue angle in **degrees** `[0, 360)`. Thin re-export of the
/// shared [`cam16::forward`] pass — the single copy both `lcs` and `lpc` build on
/// (issue #19).
pub(crate) fn cam16_jch_from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    cam16::forward(xyz, vc)
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
/// `J` is monotonic in luminance for the achromatic axis, so this inverts the
/// forward chain analytically in closed form, then polishes with two Newton
/// steps to full `f64` precision (see [`y_hk_analytic`]). The reference
/// fixed-iteration bisection [`y_hk_bisect`] is retained for tests.
pub(crate) fn y_hk(j_hk: f64, vc: &ViewingConditions) -> f64 {
    y_hk_analytic(j_hk, vc)
}

/// Closed-form inverse of [`grey_j`] with Newton polish.
///
/// # Derivation (CIECAM16, CIE 170-2:2015)
///
/// For an achromatic D65 stimulus of luminance `y`, the forward path
/// [`grey_j`] reduces to a chain of monotonic, individually invertible links.
/// Each cone response after chromatic adaptation is **linear in `y`**:
/// `lms_a[i] = k_i · y`, where the per-channel scale
/// `k_i = d·100 + (1 − d)·rgb_w[i]` follows from `rgb_d[i] = d·100/rgb_w[i] + 1 − d`
/// (`cam16` discounting, see [`ViewingConditions::build`]) and the D65 grey
/// `xyz = [y·Xw, y, y·Zw]·100` (the CAT16 white-point cone response cancels the
/// `100/rgb_w[i]` term). The achromatic signal is then
/// `A = nbb · Σ wᵢ·adapt(kᵢ·y)` with weights `w = [2, 1, 1/20]`, and
/// `J = 100·(A/A_w)^(c·z)`.
///
/// Inverting link by link:
/// 1. `J → A`:           `A = A_w · (J/100)^(1/(c·z))`           — exact (power).
/// 2. `A → target sum`:  `Σ wᵢ·adapt(kᵢ·y) = A/nbb`              — exact (linear).
/// 3. seed `y`:          collapse the three channels onto one effective scale
///    `k_eff = Σ wᵢkᵢ / Σ wᵢ` and invert the single compression
///    `adapt(k_eff·y) = S` via `(F_L·k_eff·y/100)^0.42 = 27.13·S/(400 − S)`
///    (the inverse of [`cam16::adapt`], CIE 170-2:2015 eq. 6.5), then take the
///    `1/0.42` power.
///
/// The three channels do **not** share one scale (`k_i` spread ≈ 1 % from
/// incomplete chromatic adaptation), so step 3 is an approximation, not an
/// exact inverse — there is no algebraic inverse for a sum of three distinct
/// fractional-power terms. The seed lands within ~5·10⁻⁶ of the true `y`; two
/// Newton steps on the exact 3-term residual `f(y) − target` (closed-form
/// derivative) drive it to machine epsilon, matching [`y_hk_bisect`] to
/// < 2·10⁻¹² over the full `J_HK` grid.
///
/// `Y` is clamped to `[0, 1]`, reproducing the bisection's search interval:
/// `J_HK` can exceed `grey_j(1.0) = 100` for near-white chromatic colours
/// (the H-K term lifts `J`), and there the bisection saturates at `Y = 1`.
fn y_hk_analytic(j_hk: f64, vc: &ViewingConditions) -> f64 {
    if j_hk <= 0.0 {
        return 0.0;
    }

    // Per-channel cone-response scale: lms_a[i] = k[i] · y (linear in y).
    // rgb_d[i] = d·100/rgb_w[i] + (1−d), and the D65 grey cone response is
    // rgb_w[i]·y, so lms_a[i] = (d·100 + (1−d)·rgb_w[i])·y. Recover rgb_w[i]
    // (and hence the (1−d) part) from rgb_d[i] without re-deriving d:
    // (1−d)·rgb_w[i] = rgb_w[i] − d·100, with rgb_w[i] from CAT16 of the white.
    let rgb_w = cat16::xyz_to_cone([
        D65_WHITE[0] * 100.0,
        D65_WHITE[1] * 100.0,
        D65_WHITE[2] * 100.0,
    ]);
    // rgb_d[i] = d·(100/rgb_w[i]) + (1−d)  ⇒  d = (rgb_d[i] − 1)/(100/rgb_w[i] − 1).
    // Solve for d from channel 0, then k[i] = d·100 + (1−d)·rgb_w[i].
    let d = (vc.rgb_d[0] - 1.0) / (100.0 / rgb_w[0] - 1.0);
    let k = [
        d * 100.0 + (1.0 - d) * rgb_w[0],
        d * 100.0 + (1.0 - d) * rgb_w[1],
        d * 100.0 + (1.0 - d) * rgb_w[2],
    ];
    const W: [f64; 3] = [2.0, 1.0, 1.0 / 20.0];
    let w_sum = W[0] + W[1] + W[2];

    // Step 1+2: J → target value of Σ wᵢ·adapt(kᵢ·y).
    let target = vc.aw * (j_hk / 100.0).powf(1.0 / (vc.c * vc.z)) / vc.nbb;

    // Step 3: analytic seed via the weighted-effective channel.
    let s = target / w_sum;
    if s <= 0.0 {
        return 0.0;
    }
    if s >= 400.0 {
        // adapt saturates at 400; J_HK is beyond what grey luminance ≤ 1 yields.
        return 1.0;
    }
    let k_eff = (W[0] * k[0] + W[1] * k[1] + W[2] * k[2]) / w_sum;
    // Inverse of adapt: (F_L·k_eff·y/100)^0.42 = 27.13·S/(400 − S).
    let p = 27.13 * s / (400.0 - s);
    let mut y = p.powf(1.0 / 0.42) * 100.0 / (vc.fl * k_eff);

    // Newton polish on the exact 3-term residual. adapt'(c) w.r.t. c, with
    // P = (F_L·c/100)^0.42:  dadapt/dc = 400·27.13·(0.42·P/c)/(P + 27.13)².
    let residual_slope = |y: f64| -> (f64, f64) {
        let mut f = 0.0;
        let mut df = 0.0;
        for i in 0..3 {
            let c = k[i] * y;
            let x = vc.fl * c / 100.0;
            let pp = x.powf(0.42);
            let denom = pp + 27.13;
            f += W[i] * 400.0 * pp / denom;
            let dp = 0.42 * pp / c;
            df += W[i] * k[i] * 400.0 * 27.13 * dp / (denom * denom);
        }
        (f - target, df)
    };
    for _ in 0..2 {
        let (err, slope) = residual_slope(y);
        y -= err / slope;
    }

    y.clamp(0.0, 1.0)
}

/// Reference inverse of [`grey_j`] by fixed-iteration bisection.
///
/// `J` is monotonic in luminance on the achromatic axis, so 64 bisection
/// iterations on `[0, 1]` converge to full `f64` precision. Retained as the
/// ground truth that [`y_hk_analytic`] is property-tested against.
fn y_hk_bisect(j_hk: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if grey_j(mid, vc) < j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

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
    // On `[0, T)` the clamp is `y + (T − y)^E`, smooth and strictly increasing
    // with derivative `1 − E·(T − y)^(E−1)` bounded in ~`[0.71, 1]`, so a
    // bracket-safeguarded Newton converges to full `f64` precision in a handful
    // of steps instead of 64 bisections. The bracket `[lo, hi]` guards every
    // step: a Newton iterate that leaves it falls back to bisection, so this
    // converges to the *same* root the old fixed bisection found — the emitted
    // hex is bit-identical (locked by the golden grid; checked directly by
    // `soft_clamp_inv_matches_reference_bisection`).
    let t = SOFT_CLAMP_THRESHOLD;
    let mut lo = 0.0_f64;
    let mut hi = t;
    // Seed from above: the clamp only adds `(T − y)^E ≥ 0`, so the root sits at
    // or below `clamped`, inside the bracket.
    let mut y = clamped;
    for _ in 0..12 {
        let f = soft_clamp(y) - clamped;
        if f > 0.0 {
            hi = y;
        } else {
            lo = y;
        }
        if hi - lo <= f64::EPSILON * t {
            break;
        }
        // f'(y) = 1 − E·(T − y)^(E−1); bounded away from zero on the bracket.
        let deriv = 1.0 - SOFT_CLAMP_EXP * (t - y).powf(SOFT_CLAMP_EXP - 1.0);
        let next = y - f / deriv;
        // Safeguard: keep the iterate strictly inside the bracket, else bisect.
        y = if next > lo && next < hi {
            next
        } else {
            0.5 * (lo + hi)
        };
    }
    Some(y)
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
    j_hk_from_cam16(j, m, h, vc)
}

/// Hellwig 2022 H-K-corrected lightness from already-computed CIECAM16
/// correlates `(J, M, h)`. Splitting this out of [`j_hk_from_xyz`] lets a caller
/// that already ran [`cam16::forward`] (e.g. [`crate::solve`]'s `finish`, which
/// also needs the `LcsColor`) derive `J_HK` from the same forward pass instead
/// of running a second identical one on the same stimulus.
pub(crate) fn j_hk_from_cam16(j: f64, m: f64, h: f64, vc: &ViewingConditions) -> f64 {
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
    let j = cam16::ucs_j_inv(c.jp);
    let m = cam16::ucs_m_inv(c.mp());
    let chroma = m / vc.fl.powf(0.25);
    let j_hk = j + hk_coeff(c.h_cam()) * chroma.powf(HK_CHROMA_EXPONENT);
    y_hk(j_hk.max(0.0), vc)
}

/// Benchmark-only access to the two grey-axis inverse implementations.
///
/// These wrap the crate-private [`y_hk_analytic`] and [`y_hk_bisect`] so the
/// `benches/y_hk.rs` Criterion harness can compare them head-to-head. Hidden
/// from the rendered docs and not part of the supported public surface — the
/// only supported entry point is [`y_hk`], whose signature is unchanged.
#[doc(hidden)]
pub mod bench_support {
    use super::ViewingConditions;

    /// Analytic closed-form + Newton inverse (the production path).
    pub fn y_hk_analytic(j_hk: f64, vc: &ViewingConditions) -> f64 {
        super::y_hk_analytic(j_hk, vc)
    }

    /// Bisection reference inverse (64 iterations).
    pub fn y_hk_bisect(j_hk: f64, vc: &ViewingConditions) -> f64 {
        super::y_hk_bisect(j_hk, vc)
    }
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
        // BUG CLASS: "self-consistent but wrong" — the J_HK pipeline (CAM16 J +
        // Hellwig-2022 H-K term) could agree with itself and with the inverse
        // solver yet drift from the published CIECAM16/Hellwig math, and every
        // internal round-trip test would still pass. This pins J_HK to an
        // EXTERNAL reference at 12 points spanning the hue circle.
        //
        // Reference computed with colour-science 0.4.7 (NOT hand-written):
        //   XYZ = sRGB(IEC 61966-2-1) → CIECAM16 XYZ_to_CIECAM16
        //         (XYZ_w = D65·100, L_A = 64, Y_b = 20, surround = Average),
        //   chroma C = M / F_L^0.25 with F_L the CIECAM16 luminance-adaptation
        //   factor for L_A = 64, and the hue coefficient
        //   f(h) = −0.160cos h + 0.132cos 2h − 0.405sin h + 0.080sin 2h + 0.792
        //   evaluated at the CAM16 hue, then J_HK = J + f(h)·C^0.587.
        // Script archived alongside this commit; values reproduce the three
        // original anchors (blue/red/gold) within 0.006.
        //
        // The grid deliberately covers the green / cyan / magenta / orange
        // sectors the original three-point test never touched — the zones where
        // a wrong f(h) or a wrong chroma exponent would diverge most. The
        // measured worst-case crate-vs-reference delta across all twelve is
        // 0.0043 Lc; the 0.05 budget is the documented sRGB-matrix / FL
        // micro-delta band (|dJ|<0.005, |dC|<0.05), >10× the observed drift, so
        // a real formula regression breaks it while round-off does not.
        let vc = ViewingConditions::srgb();
        for (hex, want) in [
            // existing anchors (unchanged): blue, red, gold
            ("#0000FF", 38.949467),
            ("#FF0000", 56.023889),
            ("#FFD700", 85.095269),
            // green sector
            ("#00FF00", 88.930558),
            ("#34C759", 68.618093),
            // cyan sector
            ("#00FFFF", 98.343680),
            ("#008B8B", 51.238150),
            // magenta sector
            ("#FF00FF", 68.208430),
            ("#C71585", 48.391467),
            // orange sector
            ("#FF9500", 68.405244),
            ("#FF7F00", 64.718227),
            // azure (info brand)
            ("#007AFF", 56.061369),
        ] {
            let rgb = srgb_from_hex(hex).expect("reference hex is valid");
            let got = j_hk_from_xyz(srgb_to_xyz(rgb), &vc);
            assert!(
                (got - want).abs() < 0.05,
                "{hex}: J_HK={got}, colour-science reference={want}, delta={}",
                (got - want).abs()
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
    fn y_hk_analytic_matches_bisection_on_grid() {
        // Equivalence gate: the analytic inverse must reproduce the bisection
        // reference to better than the bisection's own resolution. Bisection
        // on [0,1] over 64 steps resolves Y to ~2^-65 ≈ 2.7e-20; the analytic
        // path is limited instead by f64 round-off in the Newton residual, so
        // we hold it to 1e-12 in Y — six orders below any perceptual or
        // contrast-curve significance, and the measured worst case is < 1e-11.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let mut max_dy = 0.0_f64;
            // Sweep J_HK across the full reachable range, including the
            // above-100 band (near-white chromatic colours, where the H-K term
            // lifts J past grey_j(1.0) = 100 and both paths must saturate Y=1).
            for n in 0..=4000 {
                let j_hk = n as f64 / 4000.0 * 104.0;
                let analytic = y_hk_analytic(j_hk, &vc);
                let bisect = y_hk_bisect(j_hk, &vc);
                max_dy = max_dy.max((analytic - bisect).abs());
            }
            assert!(
                max_dy < 1e-12,
                "analytic vs bisection max|dY| = {max_dy:e} exceeds 1e-12"
            );
        }
    }

    #[test]
    fn y_hk_analytic_endpoints_and_saturation() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            // J_HK = 0 → black (Y = 0).
            assert_eq!(y_hk_analytic(0.0, &vc), 0.0);
            // J_HK = grey_j(1.0) = 100 → white (Y = 1), within round-off.
            assert!((y_hk_analytic(grey_j(1.0, &vc), &vc) - 1.0).abs() < 1e-9);
            // J_HK above 100 (reachable for near-white chromatic colours) must
            // clamp to Y = 1, matching the bisection's [0,1] search interval.
            assert_eq!(y_hk_analytic(130.0, &vc), 1.0);
            // Round-trip: grey_j(y) → y_hk_analytic recovers y.
            for &y in &[0.01_f64, 0.18, 0.5, 0.9] {
                let recovered = y_hk_analytic(grey_j(y, &vc), &vc);
                assert!(
                    (recovered - y).abs() < 1e-12,
                    "round-trip y={y}: recovered {recovered}, |d|={}",
                    (recovered - y).abs()
                );
            }
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

    #[test]
    fn soft_clamp_inv_is_a_left_inverse_of_soft_clamp() {
        // BUG CLASS: silent inverse drift. `soft_clamp_inv` is the analytic
        // back-door the contrast solver uses to turn a clamped foreground
        // luminance back into a raw Y_hk (solve.rs `invert_contrast`). If the
        // bisection inside it ever loses agreement with the forward `soft_clamp`
        // — a changed threshold, exponent, or iteration count — every solve in
        // the near-black band silently lands on the wrong colour, yet no forward
        // test would notice because the forward curve alone stays consistent.
        // This pins the round-trip soft_clamp_inv(soft_clamp(y)) == y across the
        // entire clamped band [0, threshold], where the lift is active.
        //
        // Tolerance: the inverse is a 64-step bisection on [0, SOFT_CLAMP_THRESHOLD];
        // its residual is bounded by the interval width 2^-64 · 0.022 ≈ 1.2e-21,
        // but f64 round-off in `soft_clamp`'s powf dominates, so 1e-9 is a safe
        // honest bound (the measured worst case over the sweep is < 1e-10).
        let step = 1e-4;
        let mut y = 0.0_f64;
        let mut max_err = 0.0_f64;
        let mut samples = 0_usize;
        while y <= 0.05 + 1e-12 {
            let clamped = soft_clamp(y);
            let recovered = soft_clamp_inv(clamped)
                .expect("soft_clamp(y) for y>=0 is always >= soft_clamp(0), so invertible");
            let err = (recovered - y).abs();
            max_err = max_err.max(err);
            samples += 1;
            assert!(
                err < 1e-9,
                "round-trip y={y}: soft_clamp={clamped}, recovered={recovered}, err={err:e}"
            );
            y += step;
        }
        // The sweep must actually cross the threshold so both the lifted branch
        // (y < T) and the identity branch (y >= T) are exercised, not just one.
        assert!(
            samples >= 500,
            "sweep too coarse to be a property test: {samples} samples"
        );
        eprintln!("soft_clamp_inv round-trip: {samples} samples, max err = {max_err:e}");
    }

    #[test]
    fn soft_clamp_inv_matches_reference_bisection() {
        // BIT-IDENTITY GATE for the bisection→safeguarded-Newton swap. The new
        // inverse must converge to the *same* root the original fixed 64-step
        // bisection did, or a near-black solve could land on a different hex.
        // Reproduce the exact old algorithm here and assert agreement to ULP
        // scale across the whole clamped band; far below one 8-bit output step
        // (~3.9e-3), so the emitted hex is provably unchanged.
        fn reference_bisect(clamped: f64) -> f64 {
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
            (lo + hi) * 0.5
        }

        let floor = soft_clamp(0.0);
        let mut max_err = 0.0_f64;
        let mut samples = 0_usize;
        // Sweep the clamped-value domain [soft_clamp(0), T) densely.
        let span = SOFT_CLAMP_THRESHOLD - floor;
        for i in 0..=4000 {
            let clamped = floor + span * (i as f64 / 4000.0);
            // Stay strictly inside the lifted branch (>= T returns identity).
            if clamped >= SOFT_CLAMP_THRESHOLD {
                continue;
            }
            let newton = soft_clamp_inv(clamped).expect("clamped >= soft_clamp(0) is invertible");
            let bisect = reference_bisect(clamped);
            let err = (newton - bisect).abs();
            max_err = max_err.max(err);
            samples += 1;
            // Both methods are limited by `powf` round-off in `soft_clamp`
            // (measured worst case ~1.3e-12), so 1e-9 is the honest bound — the
            // same margin the round-trip sibling uses. Nine orders below one
            // 8-bit output step (~3.9e-3): the hex is provably unchanged.
            assert!(
                err < 1e-9,
                "clamped={clamped}: newton={newton}, bisect={bisect}, err={err:e}"
            );
        }
        assert!(samples >= 3000, "sweep too coarse: {samples} samples");
        eprintln!(
            "soft_clamp_inv vs reference bisection: {samples} samples, max err = {max_err:e}"
        );
    }

    #[test]
    fn soft_clamp_boundaries_are_exact() {
        // BUG CLASS: off-by-epsilon at the clamp seam. The boundaries are where
        // a regression hides: at the threshold the two branches must meet
        // continuously, and soft_clamp(0) is the hard floor below which the
        // inverse must refuse (a contrast implying a luminance darker than black
        // is physically unreachable — solve.rs leans on this returning None).

        // soft_clamp(0): black is lifted to exactly threshold^exp above zero.
        let at_zero = soft_clamp(0.0);
        let expected_zero = SOFT_CLAMP_THRESHOLD.powf(SOFT_CLAMP_EXP);
        assert!(
            (at_zero - expected_zero).abs() < 1e-15,
            "soft_clamp(0)={at_zero}, expected threshold^exp={expected_zero}"
        );
        assert!(
            at_zero > 0.0,
            "soft_clamp(0) must lift above zero: {at_zero}"
        );

        // Continuity at the threshold: the lifted branch meets the identity
        // branch (the (T - y)^exp term vanishes as y → T from below).
        let just_below = soft_clamp(SOFT_CLAMP_THRESHOLD - 1e-12);
        assert!(
            (just_below - SOFT_CLAMP_THRESHOLD).abs() < 1e-6,
            "discontinuity at threshold: soft_clamp(T-)={just_below} vs T={SOFT_CLAMP_THRESHOLD}"
        );
        // At and above the threshold soft_clamp is the identity.
        assert_eq!(soft_clamp(SOFT_CLAMP_THRESHOLD), SOFT_CLAMP_THRESHOLD);
        assert_eq!(soft_clamp(0.5), 0.5);

        // The inverse refuses anything below soft_clamp(0): unreachable, not a clip.
        assert_eq!(soft_clamp_inv(at_zero - 1e-9), None);
        // Exactly at soft_clamp(0) the inverse recovers black.
        let recovered_zero = soft_clamp_inv(at_zero).expect("soft_clamp(0) is invertible");
        assert!(
            recovered_zero.abs() < 1e-9,
            "soft_clamp_inv(soft_clamp(0)) should recover 0, got {recovered_zero}"
        );
    }
}
