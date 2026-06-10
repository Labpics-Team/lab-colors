use crate::spaces::{cam16, cat16, vc::ViewingConditions};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz, D65_WHITE};

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
    -0.160 * h_cam.cos()
        + 0.132 * (2.0 * h_cam).cos()
        - 0.405 * h_cam.sin()
        + 0.080 * (2.0 * h_cam).sin()
        + 0.792
}

fn y_hk(j_hk: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let xyz = [mid * D65_WHITE[0], mid, mid * D65_WHITE[2]];
        let (j, _, _) = cam16_jch_from_xyz(xyz, vc);
        if j < j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

/// Perceptual-contrast core curve (asymmetric power contrast on luminance).
///
/// Constants are from the published generic perceptual-contrast formula;
/// naming policy and attribution: docs/decisions/apca-license.md.
fn contrast_core(y_fg: f64, y_bg: f64) -> f64 {
    // Soft black clamp: luminance below the threshold is lifted so the
    // curve stays monotonic near black.
    const SOFT_CLAMP_THRESHOLD: f64 = 0.022;
    const SOFT_CLAMP_EXP: f64 = 1.414;
    // Polarity-dependent power-curve exponents (bg >= fg is dark-on-light).
    const EXP_BG_LIGHT: f64 = 0.56;
    const EXP_FG_LIGHT: f64 = 0.57;
    const EXP_BG_DARK: f64 = 0.65;
    const EXP_FG_DARK: f64 = 0.62;
    // Maps the raw power-curve delta to the ~[-110, 110] output range.
    const CONTRAST_SCALE: f64 = 1.14 * 100.0;

    let clamp = |y: f64| -> f64 {
        if y < SOFT_CLAMP_THRESHOLD {
            y + (SOFT_CLAMP_THRESHOLD - y).powf(SOFT_CLAMP_EXP)
        } else {
            y
        }
    };
    let fg = clamp(y_fg);
    let bg = clamp(y_bg);

    if bg >= fg {
        (bg.powf(EXP_BG_LIGHT) - fg.powf(EXP_FG_LIGHT)) * CONTRAST_SCALE
    } else {
        (bg.powf(EXP_BG_DARK) - fg.powf(EXP_FG_DARK)) * CONTRAST_SCALE
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

pub fn lpc(fg_hex: &str, bg_hex: &str) -> f64 {
    let vc = ViewingConditions::srgb();
    let y_fg = hex_to_y_hk(fg_hex, &vc);
    let y_bg = hex_to_y_hk(bg_hex, &vc);
    contrast_core(y_fg, y_bg)
}

pub fn lpc_surface(c1_hex: &str, c2_hex: &str) -> f64 {
    let c1 = crate::lcs::LcsColor::from_hex(c1_hex).expect("invalid hex");
    let c2 = crate::lcs::LcsColor::from_hex(c2_hex).expect("invalid hex");
    let dj = c1.jp - c2.jp;
    let m1 = c1.s * (c1.jp + 1.0);
    let m2 = c2.s * (c2.jp + 1.0);
    let h1 = c1.h_ok.to_radians();
    let h2 = c2.h_ok.to_radians();
    let da = m1 * h1.cos() - m2 * h2.cos();
    let db = m1 * h1.sin() - m2 * h2.sin();
    (dj * dj + da * da + db * db).sqrt()
}

/// LPC contrast between two [`LcsColor`] values.
///
/// Uses the pre-computed CAM16 J' and Oklab hue stored in each colour,
/// avoiding re-parsing hex strings. Delegates to the same
/// perceptual-contrast core curve as [`lpc`].
pub fn lpc_lcs(fg: &crate::lcs::LcsColor, bg: &crate::lcs::LcsColor) -> f64 {
    let vc = ViewingConditions::srgb();
    let y_fg = y_hk_from_lcs(fg, &vc);
    let y_bg = y_hk_from_lcs(bg, &vc);
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
    fn black_on_white_near_apca() {
        let lc = lpc("#000000", "#ffffff");
        assert!((lc - 108.7).abs() < 1.0, "LPC for black on white: {}", lc);
    }

    #[test]
    fn gray_on_white_near_60() {
        let lc = lpc("#888888", "#ffffff");
        assert!((lc - 60.0).abs() < 5.0, "LPC for gray on white: {}", lc);
    }

    #[test]
    fn blue_on_white_less_than_apca() {
        let lc = lpc("#0000ff", "#ffffff");
        assert!(lc < 80.0, "LPC for blue on white should be < 80: {}", lc);
        assert!(lc > 50.0, "LPC for blue on white should be > 50: {}", lc);
    }

    #[test]
    fn polarity_swap_negates() {
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
    fn neutral_hk_boost_is_zero() {
        let lc = lpc("#444444", "#ffffff");
        assert!((lc - 89.0).abs() < 5.0, "achromatic LPC: {}", lc);
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
}
