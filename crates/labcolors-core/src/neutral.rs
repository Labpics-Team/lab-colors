use crate::lcs::LcsColor;
use crate::spaces::vc::ViewingConditions;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveParams {
    pub gamma_light: f64,
    pub gamma_dark: f64,
    pub chroma_peak_t: f64,
}

impl Default for CurveParams {
    fn default() -> Self {
        Self {
            gamma_light: 1.75,
            gamma_dark: 1.5,
            chroma_peak_t: 0.35,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NeutralCurve {
    a_light: LcsColor,
    a_base: LcsColor,
    a_dark: LcsColor,
    h_ok_base: f64,
    h_cam_base: f64,
    params: CurveParams,
    vc: ViewingConditions,
}

impl NeutralCurve {
    /// Build a neutral curve using standard sRGB viewing conditions (average surround).
    pub fn new(light: &str, base: &str, dark: &str) -> Result<Self, String> {
        Self::with_vc(light, base, dark, &CurveParams::default(), &ViewingConditions::srgb())
    }

    pub fn with_params(
        light: &str,
        base: &str,
        dark: &str,
        params: CurveParams,
    ) -> Result<Self, String> {
        Self::with_vc(light, base, dark, &params, &ViewingConditions::srgb())
    }

    /// Build a neutral curve for the given viewing conditions.
    ///
    /// Anchor colours are parsed through `vc`, so J' and saturation reflect
    /// the perceptual environment (e.g. dim-surround for dark themes).
    /// Use [`ViewingConditions::srgb()`] for light themes and
    /// [`ViewingConditions::dim_surround()`] for dark themes.
    pub fn with_vc(
        light: &str,
        base: &str,
        dark: &str,
        params: &CurveParams,
        vc: &ViewingConditions,
    ) -> Result<Self, String> {
        let a_light = LcsColor::from_hex_with_vc(light, vc)?;
        let a_base = LcsColor::from_hex_with_vc(base, vc)?;
        let a_dark = LcsColor::from_hex_with_vc(dark, vc)?;

        if a_light.jp <= a_base.jp {
            return Err("light anchor must be lighter than base".into());
        }
        if a_base.jp <= a_dark.jp {
            return Err("base anchor must be lighter than dark".into());
        }

        let h_ok_base = a_base.h_ok;
        let h_cam_base = a_base.h_cam();

        // Achromatic anchors have unreliable h_ok (atan2 of ~0 values).
        // CAM16 viewing-condition adaptation produces non-zero M' even for
        // nominally achromatic stimuli (mp ≈ 1.5 for white, ≈ 2.3 for
        // near-black).  Threshold 5.0 catches model noise while preserving
        // genuinely chromatic anchors.
        let a_light = if a_light.mp() < 5.0 {
            LcsColor::new(a_light.jp, h_ok_base, a_light.s, a_light.h_cam())
        } else {
            a_light
        };
        let a_dark = if a_dark.mp() < 5.0 {
            LcsColor::new(a_dark.jp, h_ok_base, a_dark.s, a_dark.h_cam())
        } else {
            a_dark
        };

        Ok(Self {
            a_light,
            a_base,
            a_dark,
            h_ok_base,
            h_cam_base,
            params: *params,
            vc: *vc,
        })
    }

    pub fn at(&self, t: f64) -> LcsColor {
        let t = t.clamp(0.0, 1.0);

        if (t - 0.0).abs() < 1e-12 {
            return self.a_light;
        }
        if (t - 0.5).abs() < 1e-12 {
            return self.a_base;
        }
        if (t - 1.0).abs() < 1e-12 {
            return self.a_dark;
        }

        let jp = if t <= 0.5 {
            let u = t / 0.5;
            let j0 = self.effective_hue_anchor_jp(&self.a_light);
            let j6 = self.effective_hue_anchor_jp(&self.a_base);
            j0 + (j6 - j0) * u.powf(self.params.gamma_light)
        } else {
            let u = (t - 0.5) / 0.5;
            let j6 = self.effective_hue_anchor_jp(&self.a_base);
            let j12 = self.effective_hue_anchor_jp(&self.a_dark);
            j6 + (j12 - j6) * u.powf(self.params.gamma_dark)
        };

        let mp_base = self.a_base.mp();
        let mp_dark = self.a_dark.mp();
        let env = sine_env(t, self.params.chroma_peak_t);
        let mp = mp_dark + (mp_base - mp_dark) * env;
        let s = mp / (jp + 1.0);

        let h_ok = self.interpolate_hue_ok(t);
        let h_cam = self.interpolate_hue_cam(t);

        LcsColor::new(jp, h_ok, s, h_cam)
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

    /// The viewing conditions used to build this curve.
    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    pub fn light_anchor(&self) -> &LcsColor {
        &self.a_light
    }

    pub fn base_anchor(&self) -> &LcsColor {
        &self.a_base
    }

    pub fn dark_anchor(&self) -> &LcsColor {
        &self.a_dark
    }

    fn effective_hue_anchor_jp(&self, anchor: &LcsColor) -> f64 {
        anchor.jp
    }

    fn interpolate_hue_ok(&self, t: f64) -> f64 {
        let h_start = self.hue_or(&self.a_light, self.h_ok_base);
        let h_end = self.hue_or(&self.a_dark, self.h_ok_base);

        if t <= 0.5 {
            let u = t / 0.5;
            lerp_angle(h_start, self.h_ok_base, u)
        } else {
            let u = (t - 0.5) / 0.5;
            lerp_angle(self.h_ok_base, h_end, u)
        }
    }

    fn interpolate_hue_cam(&self, t: f64) -> f64 {
        let h_start = self.hue_or_cam(&self.a_light);
        let h_end = self.hue_or_cam(&self.a_dark);

        if t <= 0.5 {
            let u = t / 0.5;
            lerp_angle(h_start, self.h_cam_base, u)
        } else {
            let u = (t - 0.5) / 0.5;
            lerp_angle(self.h_cam_base, h_end, u)
        }
    }

    fn hue_or(&self, anchor: &LcsColor, fallback: f64) -> f64 {
        let mp_ref = self.mp_ref();
        let purity = hue_purity(anchor.mp(), mp_ref);
        lerp_angle(fallback, anchor.h_ok, purity)
    }

    fn hue_or_cam(&self, anchor: &LcsColor) -> f64 {
        let mp_ref = self.mp_ref();
        let purity = hue_purity(anchor.mp(), mp_ref);
        lerp_angle(self.h_cam_base, anchor.h_cam(), purity)
    }

    /// Reference chroma for hue-purity normalisation.
    ///
    /// Set to 1.5× the base anchor's M' so that the base itself retains most
    /// of its own hue while near-achromatic anchors are strongly corrected.
    fn mp_ref(&self) -> f64 {
        self.a_base.mp() * 1.5
    }
}

fn sine_env(t: f64, t_peak: f64) -> f64 {
    if t <= t_peak {
        ((std::f64::consts::PI * t) / (2.0 * t_peak)).sin()
    } else {
        ((std::f64::consts::PI * (1.0 - t)) / (2.0 * (1.0 - t_peak))).sin()
    }
}

fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let diff = b - a;
    let shortest = ((diff + 180.0) % 360.0) - 180.0;
    a + shortest * t
}

/// Abney-effect hue-purity weight.
///
/// Returns a value in `[0, 1]` indicating how much of the anchor's own hue
/// to retain. Low `mp` (near-achromatic) → low purity → strong correction
/// toward the base hue. The power exponent 0.6 gives aggressive correction
/// for very desaturated colours while releasing smoothly as chroma increases.
///
/// ```text
/// mp/mp_ref = 0.1 → purity ≈ 0.25  (75 % corrected)
/// mp/mp_ref = 0.3 → purity ≈ 0.46  (54 % corrected)
/// mp/mp_ref = 0.5 → purity ≈ 0.66  (34 % corrected)
/// mp/mp_ref = 1.0 → purity = 1.00  (0   % corrected)
/// ```
fn hue_purity(mp: f64, mp_ref: f64) -> f64 {
    if mp >= mp_ref {
        return 1.0;
    }
    (mp / mp_ref).powf(0.6).clamp(0.0, 1.0)
}

impl crate::curve::ColorCurve for NeutralCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_curve() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn anchors_exact_at_endpoints() {
        let curve = default_curve();
        let c0 = curve.at(0.0);
        let cm = curve.at(0.5);
        let c1 = curve.at(1.0);

        assert!(
            (c0.jp - curve.light_anchor().jp).abs() < 1e-9,
            "t=0 jp mismatch"
        );
        assert!(
            (cm.jp - curve.base_anchor().jp).abs() < 1e-9,
            "t=0.5 jp mismatch"
        );
        assert!(
            (c1.jp - curve.dark_anchor().jp).abs() < 1e-9,
            "t=1.0 jp mismatch"
        );
    }

    #[test]
    fn jp_monotonically_decreasing() {
        let curve = default_curve();
        let steps = curve.sample(100);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 1e-9,
                "jp increased: {} -> {}",
                w[0].jp,
                w[1].jp
            );
        }
    }

    #[test]
    fn hue_drift_under_30_degrees() {
        let curve = default_curve();
        let base_hue = curve.base_anchor().h_ok;
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            let drift = (c.h_ok - base_hue + 180.0).rem_euclid(360.0) - 180.0;
            assert!(
                drift.abs() < 30.0,
                "hue drift at t={}: {}° (base={})",
                i as f64 / 100.0,
                drift,
                base_hue
            );
        }
    }

    #[test]
    fn sample_13_matches_old_api() {
        let curve = default_curve();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes.len(), 13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[6].to_uppercase(), "#787880");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }

    #[test]
    fn all_sampled_steps_unique() {
        let curve = default_curve();
        let hexes = curve.sample_hex(13);
        let mut seen = std::collections::HashSet::new();
        for hex in &hexes {
            assert!(seen.insert(hex.to_uppercase()), "duplicate: {}", hex);
        }
    }

    #[test]
    fn jp_within_anchor_bounds() {
        let curve = default_curve();
        let j_max = curve.light_anchor().jp;
        let j_min = curve.dark_anchor().jp;
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            assert!(
                c.jp <= j_max + 1e-9 && c.jp >= j_min - 1e-9,
                "t={}: jp={} out of [{}, {}]",
                i as f64 / 100.0,
                c.jp,
                j_min,
                j_max
            );
        }
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(NeutralCurve::new("#GGGGGG", "#787880", "#101012").is_err());
    }

    #[test]
    fn rejects_light_not_lighter_than_base() {
        assert!(NeutralCurve::new("#787880", "#FFFFFF", "#101012").is_err());
    }

    #[test]
    fn rejects_base_not_lighter_than_dark() {
        assert!(NeutralCurve::new("#FFFFFF", "#101012", "#787880").is_err());
    }

    #[test]
    fn s_non_negative_everywhere() {
        let curve = default_curve();
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            assert!(c.s >= -1e-9, "negative s at t={}: {}", i as f64 / 100.0, c.s);
        }
    }

    // ── Dark-theme (dim-surround) tests ────────────────────────

    fn dim_curve() -> NeutralCurve {
        let vc = ViewingConditions::dim_surround();
        NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &CurveParams::default(), &vc).unwrap()
    }

    #[test]
    fn dim_base_jp_higher_than_srgb() {
        // CIECAM16 dim surround: lower c (0.59 vs 0.69) → smaller exponent
        // for J = 100·(A/Aw)^(c·Z).  When A/Aw < 1 (any non-white stimulus),
        // a smaller exponent pushes the result closer to 1, yielding a higher J.
        // Physically correct: mid-grey appears lighter relative to the
        // adapted white point in dim surroundings.
        let avg = default_curve();
        let dim = dim_curve();
        assert!(
            dim.base_anchor().jp > avg.base_anchor().jp,
            "dim J'={} should be > avg J'={} (dim surround lifts mid-tones)",
            dim.base_anchor().jp,
            avg.base_anchor().jp,
        );
    }

    #[test]
    fn dim_jp_monotonically_decreasing() {
        let curve = dim_curve();
        let steps = curve.sample(100);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 1e-9,
                "dim jp increased: {} -> {}",
                w[0].jp,
                w[1].jp,
            );
        }
    }

    #[test]
    fn dim_roundtrip_base() {
        let curve = dim_curve();
        let hex = curve.base_anchor().to_hex_with_vc(&curve.vc);
        assert!(
            hex.eq_ignore_ascii_case("#787880"),
            "dim roundtrip drift: expected #787880, got {}",
            hex,
        );
    }

    #[test]
    fn dim_sample_hex_endpoints_match() {
        let curve = dim_curve();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }

    #[test]
    fn dim_all_steps_unique() {
        let curve = dim_curve();
        let hexes = curve.sample_hex(13);
        let mut seen = std::collections::HashSet::new();
        for hex in &hexes {
            assert!(seen.insert(hex.to_uppercase()), "dim duplicate: {}", hex);
        }
    }
}
