use crate::spaces::{cam16, cat16, vc::ViewingConditions};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz, D65_WHITE};

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

    (j, m, hr)
}

fn hk_coeff(h_cam: f64) -> f64 {
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

fn apca(y_fg: f64, y_bg: f64) -> f64 {
    let clamp = |y: f64| -> f64 {
        if y < 0.022 {
            y + (0.022 - y).powf(1.414)
        } else {
            y
        }
    };
    let fg = clamp(y_fg);
    let bg = clamp(y_bg);

    if bg >= fg {
        (bg.powf(0.56) - fg.powf(0.57)) * 1.14 * 100.0
    } else {
        (bg.powf(0.65) - fg.powf(0.62)) * 1.14 * 100.0
    }
}

fn hex_to_y_hk(hex: &str, vc: &ViewingConditions) -> f64 {
    let rgb = srgb_from_hex(hex).unwrap_or([0.0, 0.0, 0.0]);
    let xyz = srgb_to_xyz(rgb);
    let (j, m, hr) = cam16_jch_from_xyz(xyz, vc);
    let j_hk = j + hk_coeff(hr) * m.powf(0.587);
    y_hk(j_hk.max(0.0), vc)
}

pub fn lpc(fg_hex: &str, bg_hex: &str) -> f64 {
    let vc = ViewingConditions::srgb();
    let y_fg = hex_to_y_hk(fg_hex, &vc);
    let y_bg = hex_to_y_hk(bg_hex, &vc);
    apca(y_fg, y_bg)
}

pub fn lpc_surface(c1_hex: &str, c2_hex: &str) -> f64 {
    let c1 = crate::lcs::LcsColor::from_hex(c1_hex).expect("invalid hex");
    let c2 = crate::lcs::LcsColor::from_hex(c2_hex).expect("invalid hex");
    let dj = c1.jp - c2.jp;
    let m1 = c1.s * (c1.jp + 1.0);
    let m2 = c2.s * (c2.jp + 1.0);
    let da = m1 * c1.h_ok.cos() - m2 * c2.h_ok.cos();
    let db = m1 * c1.h_ok.sin() - m2 * c2.h_ok.sin();
    (dj * dj + da * da + db * db).sqrt()
}

/// LPC contrast between two [`LcsColor`] values.
///
/// Uses the pre-computed CAM16 J' and Oklab hue stored in each colour,
/// avoiding re-parsing hex strings. Delegates to the same APCA normalised
/// luminance contrast as [`lpc`].
pub fn lpc_lcs(fg: &crate::lcs::LcsColor, bg: &crate::lcs::LcsColor) -> f64 {
    let vc = ViewingConditions::srgb();
    let y_fg = y_hk_from_lcs(fg, &vc);
    let y_bg = y_hk_from_lcs(bg, &vc);
    apca(y_fg, y_bg)
}

/// Derive hk-adjusted luminance from an existing [`LcsColor`].
///
/// Reconstructs the J_hk value from the stored J' and hue, then
/// binary-searches for Y (same as `y_hk` but skips XYZ→CAM16).
fn y_hk_from_lcs(c: &crate::lcs::LcsColor, vc: &ViewingConditions) -> f64 {
    let hk = hk_coeff(c.h_cam());
    let j_hk = c.jp + hk * c.mp().powf(0.587);
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
}
