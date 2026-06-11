use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

#[derive(Debug, Clone)]
pub struct AccentCurve {
    neutral: NeutralCurve,
    h_canonical: f64,
    sat_ratio: f64,
    slope: f64,
    canonical_hex: String,
    vc: ViewingConditions,
}

impl AccentCurve {
    pub fn new(canonical_hex: &str, neutral: &NeutralCurve) -> Result<Self, String> {
        let color = LcsColor::from_hex(canonical_hex)?;
        let h_canonical = color.h_ok;

        let rgb = srgb_from_hex(canonical_hex)?;
        let lab = srgb_linear_to_oklab(rgb);
        let l_ok = lab[0];

        let c_canonical = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
        let c_max = max_chroma(l_ok, h_canonical);
        let sat_ratio = if c_max > 1e-6 {
            c_canonical / c_max
        } else {
            0.0
        };

        Ok(Self {
            neutral: neutral.clone(),
            h_canonical,
            sat_ratio: sat_ratio.clamp(0.0, 1.0),
            slope: 5.0,
            canonical_hex: canonical_hex.to_uppercase(),
            vc: *neutral.vc(),
        })
    }

    pub fn at(&self, t: f64) -> LcsColor {
        let t = t.clamp(0.0, 1.0);
        let neutral_color = self.neutral.at(t);
        let jp = neutral_color.jp;

        let l_ok = jp_to_oklab_l(jp, &self.vc);

        let h_optimal = self.find_optimal_hue(l_ok);

        let c_max = max_chroma(l_ok, h_optimal);
        let c_use = self.sat_ratio * c_max;

        let h_rad = h_optimal.to_radians();
        let a_ok = c_use * h_rad.cos();
        let b_ok = c_use * h_rad.sin();

        let rgb = oklab_to_srgb_linear([l_ok, a_ok, b_ok]);
        let rgb_clamped = [
            rgb[0].clamp(0.0, 1.0),
            rgb[1].clamp(0.0, 1.0),
            rgb[2].clamp(0.0, 1.0),
        ];

        let xyz = srgb_to_xyz(rgb_clamped);
        let h_ok = b_ok.atan2(a_ok).to_degrees().rem_euclid(360.0);

        let (j, m, h_cam) = crate::lpc::cam16_jch_from_xyz(xyz, &self.vc);

        // CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131); see lcs.rs.
        let jp_actual = 1.7 * j / (1.0 + 0.007 * j);
        let mp = (1.0 + 0.0228 * m).ln() / 0.0228;
        let s = if jp_actual + 1.0 > 1e-9 {
            mp / (jp_actual + 1.0)
        } else {
            0.0
        };

        LcsColor::new(jp_actual, h_ok, s.max(0.0), h_cam)
    }

    pub fn sample(&self, n: usize) -> Vec<LcsColor> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.at(0.5)];
        }
        (0..n).map(|i| self.at(i as f64 / (n - 1) as f64)).collect()
    }

    pub fn sample_hex(&self, n: usize) -> Vec<String> {
        self.sample(n)
            .iter()
            .map(|c| c.to_hex_with_vc(&self.vc))
            .collect()
    }

    /// The viewing conditions inherited from the neutral curve.
    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    pub fn canonical_hue(&self) -> f64 {
        self.h_canonical
    }

    pub fn sat_ratio(&self) -> f64 {
        self.sat_ratio
    }

    /// The original hex string passed to [`AccentCurve::new`], normalised to uppercase.
    pub fn canonical_hex(&self) -> &str {
        &self.canonical_hex
    }

    fn find_optimal_hue(&self, l_ok: f64) -> f64 {
        let c_at_canonical = max_chroma(l_ok, self.h_canonical);

        if c_at_canonical > 1e-6 {
            return self.h_canonical;
        }

        let best = (0..36)
            .map(|i| {
                let h = self.h_canonical + (i as f64 - 18.0) * 10.0;
                let c = max_chroma(l_ok, h);
                let dh = ((h - self.h_canonical + 180.0).rem_euclid(360.0)) - 180.0;
                let cost = self.slope / (1.0 - dh.abs() / 180.0).max(0.01);
                let score = c - cost;
                (h, c, score)
            })
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        best.map(|(h, _, _)| h).unwrap_or(self.h_canonical)
    }
}

fn jp_to_oklab_l(jp: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let xyz = [
            mid * crate::spaces::srgb::D65_WHITE[0],
            mid,
            mid * crate::spaces::srgb::D65_WHITE[2],
        ];
        let (j, _, _) = crate::lpc::cam16_jch_from_xyz(xyz, vc);
        let jp_mid = 1.7 * j / (1.0 + 0.007 * j);
        if jp_mid < jp {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let y = (lo + hi) * 0.5;
    let lab = srgb_linear_to_oklab([y, y, y]);
    lab[0]
}

pub(crate) fn max_chroma(l_ok: f64, h_ok_deg: f64) -> f64 {
    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let a = mid * cos_h;
        let b = mid * sin_h;
        let rgb = oklab_to_srgb_linear([l_ok, a, b]);

        if rgb[0] >= -1e-6
            && rgb[0] <= 1.0 + 1e-6
            && rgb[1] >= -1e-6
            && rgb[1] <= 1.0 + 1e-6
            && rgb[2] >= -1e-6
            && rgb[2] <= 1.0 + 1e-6
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    (lo + hi) * 0.5
}

impl crate::curve::ColorCurve for AccentCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        &self.vc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn accent_jp_monotonically_decreasing() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "jp increased: {} -> {}",
                w[0].jp,
                w[1].jp
            );
        }
    }

    #[test]
    fn accent_s_non_negative() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let c = curve.at(i as f64 / 50.0);
            assert!(c.s >= -1e-6, "negative s at t={}: {}", i as f64 / 50.0, c.s);
        }
    }

    #[test]
    fn accent_all_in_gamut() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex();
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn max_chroma_white_is_small() {
        let c = max_chroma(1.0, 0.0);
        assert!(c < 0.01, "max chroma at L=1 should be ~0: {}", c);
    }

    #[test]
    fn max_chroma_mid_has_room() {
        let c = max_chroma(0.5, 30.0);
        assert!(c > 0.1, "max chroma at L=0.5, h=30 should be > 0.1: {}", c);
    }

    #[test]
    fn sat_ratio_for_saturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            curve.sat_ratio() > 0.5,
            "red should have high sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn sat_ratio_for_desaturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#CC8888", &neutral).unwrap();
        assert!(
            curve.sat_ratio() < 0.5,
            "desaturated should have low sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn sample_hex_produces_valid_colors() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes.len(), 13);
        for hex in &hexes {
            assert!(LcsColor::from_hex(hex).is_ok(), "invalid hex: {}", hex);
        }
    }

    #[test]
    fn rejects_bad_hex() {
        let neutral = default_neutral();
        assert!(AccentCurve::new("#GGGGGG", &neutral).is_err());
    }

    // ── Dark-theme (dim-surround) accent tests ────────────────

    fn dim_neutral() -> NeutralCurve {
        use crate::neutral::CurveParams;
        use crate::spaces::vc::ViewingConditions;
        let vc = ViewingConditions::dim_surround();
        NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &vc,
        )
        .unwrap()
    }

    #[test]
    fn dim_accent_jp_monotonically_decreasing() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "dim accent jp increased: {} -> {}",
                w[0].jp,
                w[1].jp,
            );
        }
    }

    #[test]
    fn dim_accent_all_in_gamut() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex_with_vc(&curve.vc);
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "dim accent out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn dim_accent_inherits_vc_from_neutral() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            (curve.vc().c - 0.59).abs() < 1e-10,
            "accent vc.c should match dim neutral: {}",
            curve.vc().c,
        );
    }
}
