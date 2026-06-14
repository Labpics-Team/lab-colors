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

    /// Whether these conditions describe a dimmed/darkened viewing environment —
    /// the surround a dark theme resolves under, as opposed to the bright
    /// average surround a light theme uses.
    ///
    /// The discriminator is the surround chromatic-induction factor `c`: the
    /// average (light) preset fixes it at `0.69`; every dimmer preset (`dim`
    /// 0.59, `dark` 0.525) sits below it. A single midpoint threshold (`0.64`)
    /// separates them with float headroom on both sides, so the classification is
    /// stable against rounding. This keeps the viewing conditions the single
    /// source of truth for theme-ness: a role contract that calibrates per theme
    /// (the dJ' decorative anchors, which the owner measured separately for light
    /// and dark) reads the theme from the VC it is resolved under, never from a
    /// flag duplicated elsewhere.
    pub fn is_dark_theme(&self) -> bool {
        const AVERAGE_DIM_MIDPOINT_C: f64 = 0.64;
        self.c < AVERAGE_DIM_MIDPOINT_C
    }

    /// Exact identity fingerprint over **every** field that affects a resolved
    /// colour. Two viewing conditions with equal fingerprints produce
    /// bit-identical output, so a fast-path cache may key on it: any difference
    /// — even in a field the surround pair `(c, nc)` does not capture — forces a
    /// distinct slot (a cold rebuild), never a wrong-colour memo collision. This
    /// is why the grey/chroma fast paths match a VC on the full fingerprint, not
    /// just `(c, nc)`: a caller-built VC that aliases the surround pair but
    /// differs in adaptation (`aw`/`fl`/`n`/…) must fall through to the live
    /// solver, not be served another condition's cached set.
    pub(crate) fn fingerprint(&self) -> u64 {
        // Destructure rather than list `self.field`s: a field added to
        // `ViewingConditions` then becomes a COMPILE error here until it is
        // folded into the hash below. The fingerprint can never silently omit a
        // field and revive the subset-aliasing bug (#73) — the compiler enforces
        // completeness, not a comment.
        let &ViewingConditions {
            n,
            aw,
            nbb,
            ncb,
            fl,
            z,
            c,
            nc,
            rgb_d: [d0, d1, d2],
        } = self;
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for f in [n, aw, nbb, ncb, fl, z, c, nc, d0, d1, d2] {
            h ^= f.to_bits();
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
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

    #[test]
    fn is_dark_theme_classifies_presets() {
        // The 0.64 midpoint must land the average (light) surround above it and
        // both dimmed surrounds below it — the contract role resolution relies on.
        assert!(
            !ViewingConditions::srgb().is_dark_theme(),
            "srgb (average surround, c≈0.69) is a light theme"
        );
        assert!(
            ViewingConditions::dim_surround().is_dark_theme(),
            "dim_surround (c≈0.59) is a dark theme"
        );
        assert!(
            ViewingConditions::dark_surround().is_dark_theme(),
            "dark_surround (c≈0.525) is a dark theme"
        );
    }

    #[test]
    fn fingerprint_separates_presets_and_surround_pair_aliases() {
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        // The two precompiled conditions are distinct.
        assert_ne!(srgb.fingerprint(), dim.fingerprint());
        // Stable: a fresh construction fingerprints identically (so the fast-path
        // exact match recognises the live preset).
        assert_eq!(srgb.fingerprint(), ViewingConditions::srgb().fingerprint());

        // The whole point: a VC that ALIASES sRGB's surround pair (c, nc) but
        // differs in an adaptation field must fingerprint differently, so a
        // fingerprint-keyed cache can never serve it sRGB's set.
        let mut alias = srgb;
        alias.aw += 1.0;
        assert_eq!(alias.c, srgb.c);
        assert_eq!(alias.nc, srgb.nc);
        assert_ne!(
            alias.fingerprint(),
            srgb.fingerprint(),
            "an aw-perturbed VC must not collide with sRGB's fingerprint"
        );
    }
}
