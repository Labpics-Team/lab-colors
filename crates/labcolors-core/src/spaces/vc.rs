use crate::spaces::srgb::D65_WHITE;

use super::{cam16::adapt, cat16::xyz_to_cone};

/// Viewing conditions for the CIECAM16 colour appearance model.
///
/// Defaults match the sRGB standard (D65, 20 % grey background,
/// average surround, no discounting).
#[derive(Debug, Clone, Copy)]
pub struct ViewingConditions {
    /// Background luminance factor (Yb / Yw).
    pub n: f64,
    /// Achromatic response to the reference white.
    pub aw: f64,
    /// Chromatic induction factor.
    pub nbb: f64,
    pub ncb: f64,
    /// Luminance-level adaptation factor.
    pub fl: f64,
    /// Base exponential nonlinearity.
    pub z: f64,
    /// Degree of chromatic adaptation.
    pub c: f64,
    /// Chromatic induction factor.
    pub nc: f64,
    /// RGB discounting factors.
    pub rgb_d: [f64; 3],
}

impl Default for ViewingConditions {
    fn default() -> Self {
        Self::srgb()
    }
}

impl ViewingConditions {
    /// Standard sRGB viewing conditions (average surround).
    ///
    /// Parameters: D65 illuminant, L_A = 64 cd/m², Y_b = 20 %,
    /// average surround (F = 1.0, c = 0.69, N_c = 1.0).
    ///
    /// The surround triplet matches colorjs.io `surroundMap["average"]`,
    /// but the adapting luminance does NOT match colorjs.io, whose default
    /// is `(64/π)·0.2 ≈ 4.07 cd/m²` — lab-colors deliberately uses 64
    /// (L_A policy: docs/decisions/theme-invariant.md). The forward path
    /// at these exact parameters is cross-validated against colour-science
    /// in `golden_tests`.
    pub fn srgb() -> Self {
        // colour-science / colorjs.io surroundMap["average"] = [1.0, 0.69, 1.0]
        Self::build(64.0, 20.0, 1.0, 0.69, 1.0)
    }

    /// Dim surround viewing conditions for dark-theme colour resolution.
    ///
    /// Same illuminant (D65) and adapting luminance as sRGB average,
    /// but with reduced surround contrast per CIECAM16 Table 1:
    /// F = 0.9, c = 0.59, N_c = 0.9.
    ///
    /// Produces lower J' for the same stimulus compared to average surround,
    /// which matches human perception in darkened viewing environments.
    pub fn dim_surround() -> Self {
        // colour-science / colorjs.io surroundMap["dim"] = [0.9, 0.59, 0.9]
        Self::build(64.0, 20.0, 0.9, 0.59, 0.9)
    }

    /// Dark surround viewing conditions (CIECAM16 Table 1: F = 0.8, c = 0.525,
    /// N_c = 0.8). Not a precompiled LUT target — used in tests to exercise the
    /// grey-axis LUT's fall-back-to-bisection path for an unsupported VC.
    #[cfg(test)]
    pub(crate) fn dark_surround() -> Self {
        Self::build(64.0, 20.0, 0.8, 0.525, 0.8)
    }

    /// Core constructor shared by all surround presets.
    ///
    /// * `la`  — adapting field luminance (cd/m²), typically 64.
    /// * `y_b` — background luminance factor (%), typically 20.
    /// * `f`   — surround factor (1.0 average, 0.9 dim, 0.8 dark).
    /// * `c`   — chromatic adaptation induction factor from surround table.
    /// * `nc`  — chromatic induction factor from surround table.
    fn build(la: f64, y_b: f64, f: f64, c: f64, nc: f64) -> Self {
        let k = 1.0_f64 / (5.0 * la + 1.0);
        let k4 = k * k * k * k;
        let fl = k4 * la + 0.1_f64 * (1.0 - k4).powi(2) * (5.0 * la).cbrt();

        let n = y_b / 100.0_f64;
        let nbb = 0.725_f64 * n.powf(-0.2);
        let z = 1.48_f64 + n.sqrt();

        let xyz_w = [
            D65_WHITE[0] * 100.0,
            D65_WHITE[1] * 100.0,
            D65_WHITE[2] * 100.0,
        ];
        let rgb_w = xyz_to_cone(xyz_w);
        let d = (f * (1.0 - (1.0 / 3.6) * ((-la - 42.0) / 92.0).exp())).clamp(0.0, 1.0);
        let rgb_d = [
            d * (100.0 / rgb_w[0]) + 1.0 - d,
            d * (100.0 / rgb_w[1]) + 1.0 - d,
            d * (100.0 / rgb_w[2]) + 1.0 - d,
        ];

        let rgb_w_adapted = [
            rgb_w[0] * rgb_d[0],
            rgb_w[1] * rgb_d[1],
            rgb_w[2] * rgb_d[2],
        ];
        let rgb_aw = [
            adapt(rgb_w_adapted[0], fl),
            adapt(rgb_w_adapted[1], fl),
            adapt(rgb_w_adapted[2], fl),
        ];
        let aw = (2.0 * rgb_aw[0] + rgb_aw[1] + rgb_aw[2] / 20.0) * nbb;

        Self {
            n,
            aw,
            nbb,
            ncb: nbb,
            fl,
            z,
            c,
            nc,
            rgb_d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_c_is_069() {
        let vc = ViewingConditions::srgb();
        assert!(
            (vc.c - 0.69).abs() < 1e-10,
            "srgb c = {}, expected 0.69",
            vc.c
        );
    }

    #[test]
    fn dim_surround_c_is_059() {
        let vc = ViewingConditions::dim_surround();
        assert!(
            (vc.c - 0.59).abs() < 1e-10,
            "dim c = {}, expected 0.59",
            vc.c
        );
    }

    #[test]
    fn dim_surround_nc_is_09() {
        let vc = ViewingConditions::dim_surround();
        assert!(
            (vc.nc - 0.9).abs() < 1e-10,
            "dim nc = {}, expected 0.9",
            vc.nc
        );
    }

    #[test]
    fn dim_has_lower_aw_than_average() {
        // Dim surround reduces adaptation → lower achromatic response
        let avg = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        assert!(
            dim.aw < avg.aw,
            "dim aw ({}) should be < average aw ({})",
            dim.aw,
            avg.aw
        );
    }

    #[test]
    fn dim_has_different_rgb_d() {
        let avg = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        assert_ne!(
            avg.rgb_d, dim.rgb_d,
            "different surround → different discounting factors"
        );
    }
}
