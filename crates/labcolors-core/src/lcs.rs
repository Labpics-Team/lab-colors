use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_to_xyz, xyz_to_srgb};
use crate::spaces::{cam16, cat16, oklab, vc::ViewingConditions};

/// All hue fields (`h_ok`, `h_cam`) are stored in **degrees** `[0, 360)`.
/// Convert to radians only at trigonometric call sites — never store radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LcsColor {
    pub jp: f64,
    pub h_ok: f64,
    pub s: f64,
    h_cam: f64,
}

impl LcsColor {
    /// Parse from hex using standard sRGB viewing conditions (average surround).
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        Self::from_hex_with_vc(hex, &ViewingConditions::srgb())
    }

    /// Parse from hex using the given viewing conditions.
    ///
    /// The resulting J', saturation, and CAM16 hue reflect perception under
    /// the provided VC (e.g. [`ViewingConditions::dim_surround`] for dark
    /// themes).
    pub fn from_hex_with_vc(hex: &str, vc: &ViewingConditions) -> Result<Self, String> {
        let rgb = srgb_from_hex(hex)?;
        let xyz = srgb_to_xyz(rgb);
        let h_ok = oklab::oklab_hue(rgb);
        Ok(Self::from_xyz_with_hok(xyz, h_ok, vc))
    }

    /// Convert to hex using standard sRGB viewing conditions.
    pub fn to_hex(&self) -> String {
        self.to_hex_with_vc(&ViewingConditions::srgb())
    }

    /// Convert to hex using the given viewing conditions.
    ///
    /// Must use the same VC that was used to construct this colour, otherwise
    /// the round-trip will introduce drift.
    pub fn to_hex_with_vc(&self, vc: &ViewingConditions) -> String {
        let xyz = self.to_xyz(vc);
        let rgb = xyz_to_srgb(xyz);
        hex_from_srgb(rgb)
    }

    pub(crate) fn new(jp: f64, h_ok: f64, s: f64, h_cam: f64) -> Self {
        Self { jp, h_ok, s, h_cam }
    }

    pub(crate) fn mp(&self) -> f64 {
        self.s * (self.jp + 1.0)
    }

    pub(crate) fn h_cam(&self) -> f64 {
        self.h_cam
    }

    pub(crate) fn from_xyz_with_hok(xyz: [f64; 3], h_ok: f64, vc: &ViewingConditions) -> Self {
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

        // CAM16-UCS rescaling — Li et al. 2017, DOI 10.1002/col.22131:
        //   J' = 1.7·J / (1 + 0.007·J),  M' = ln(1 + 0.0228·M) / 0.0228.
        // Maps raw CIECAM16 J/M onto perceptually uniform J'/M' (J'=50 reads
        // as half-lightness). Inverse in `to_xyz`.
        let jp = 1.7 * j / (1.0 + 0.007 * j);
        let mp = (1.0 + 0.0228 * m).ln() / 0.0228;
        let s = mp / (jp + 1.0);

        Self {
            jp,
            h_ok,
            s,
            h_cam: h,
        }
    }

    pub(crate) fn to_xyz(self, vc: &ViewingConditions) -> [f64; 3] {
        // Inverse CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131):
        //   J = J' / (1.7 - 0.007·J'),  M = (e^(0.0228·M') - 1) / 0.0228.
        let j = self.jp / (1.7 - 0.007 * self.jp);
        let mp = self.mp();
        let m = (0.0228 * mp).exp_m1() / 0.0228;
        let hr = self.h_cam.to_radians();

        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        let t_inner = (1.64 - 0.29_f64.powf(vc.n)).powf(0.73);
        let t = (m / ((j / 100.0).sqrt() * t_inner * vc.fl.powf(0.25))).powf(1.0 / 0.9);

        let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.nbb;
        let p2 = (vc.aw * (j / 100.0).powf(1.0 / (vc.c * vc.z))) / vc.nbb;
        let gamma =
            23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * hr.cos() + 108.0 * t * hr.sin());

        let a = gamma * hr.cos();
        let b = gamma * hr.sin();

        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

        let r_c = cam16::unadapt(r_a, vc.fl);
        let g_c = cam16::unadapt(g_a, vc.fl);
        let b_c = cam16::unadapt(b_a, vc.fl);

        let lms = [r_c / vc.rgb_d[0], g_c / vc.rgb_d[1], b_c / vc.rgb_d[2]];
        let xyz = cat16::cone_to_xyz(lms);

        [xyz[0] / 100.0, xyz[1] / 100.0, xyz[2] / 100.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_neutral_base() {
        let original = "#787880";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_white() {
        let original = "#FFFFFF";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_dark() {
        let original = "#101012";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn from_hex_rejects_short_string() {
        assert!(LcsColor::from_hex("#fff").is_err());
    }

    #[test]
    fn h_ok_stable_across_roundtrip() {
        let original = "#787880";
        let lcs1 = LcsColor::from_hex(original).unwrap();
        let back = lcs1.to_hex();
        let lcs2 = LcsColor::from_hex(&back).unwrap();
        assert!(
            (lcs1.h_ok - lcs2.h_ok).abs() < 1e-6,
            "h_ok drift: {} vs {}",
            lcs1.h_ok,
            lcs2.h_ok
        );
    }

    #[test]
    fn roundtrip_dim_surround_midgrey() {
        let vc = ViewingConditions::dim_surround();
        let original = "#787880";
        let lcs = LcsColor::from_hex_with_vc(original, &vc).unwrap();
        let back = lcs.to_hex_with_vc(&vc);
        assert!(
            back.eq_ignore_ascii_case(original),
            "dim roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn dim_jp_differs_from_srgb() {
        let vc = ViewingConditions::dim_surround();
        let avg = LcsColor::from_hex("#787880").unwrap();
        let dim = LcsColor::from_hex_with_vc("#787880", &vc).unwrap();
        assert!(
            (avg.jp - dim.jp).abs() > 0.1,
            "same stimulus should produce different J' across VCs: avg={} dim={}",
            avg.jp,
            dim.jp,
        );
    }

    #[test]
    fn wrong_vc_roundtrip_drifts() {
        // Construct with dim VC, convert with srgb VC → should drift
        let dim_vc = ViewingConditions::dim_surround();
        let lcs = LcsColor::from_hex_with_vc("#787880", &dim_vc).unwrap();
        let wrong_hex = lcs.to_hex(); // uses srgb VC — mismatch!
        // The hex will still be valid sRGB, just not matching the original
        assert!(
            !wrong_hex.eq_ignore_ascii_case("#787880"),
            "VC mismatch should cause drift, got {}",
            wrong_hex,
        );
    }

    #[test]
    fn h_cam_stored_in_degrees() {
        // CAM16 hue of sRGB red is tens of degrees; a value below 2π would
        // mean radians leaked into storage.
        let red = LcsColor::from_hex("#FF0000").expect("#FF0000 is a valid hex colour");
        let h = red.h_cam();
        assert!((0.0..360.0).contains(&h), "h_cam out of range: {}", h);
        assert!(
            h > 7.0,
            "red CAM16 hue should be tens of degrees, got {} — radians leak?",
            h
        );
    }
}
