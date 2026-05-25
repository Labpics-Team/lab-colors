use crate::neutral::NeutralCurve;
use crate::scale::{AccentCurve, max_chroma};
use crate::spaces::oklab::oklab_to_srgb_linear;
use crate::spaces::srgb::hex_from_srgb;
use crate::lcs::LcsColor;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sentiment {
    Danger,
    Warning,
    Success,
    Info,
}

impl Sentiment {
    fn prototype_hue(self) -> f64 {
        match self {
            Sentiment::Danger => 18.0,
            Sentiment::Warning => 67.0,
            Sentiment::Success => 145.0,
            Sentiment::Info => 240.0,
        }
    }

    fn slope(self) -> (f64, f64) {
        match self {
            Sentiment::Warning => (1.5, 3.0),
            _ => (5.0, 5.0),
        }
    }

    fn hue_floor(self) -> Option<f64> {
        match self {
            Sentiment::Warning => Some(45.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SentimentCurve {
    pub resolved_hue: f64,
    pub was_displaced: bool,
    pub displacement: f64,
    accent: AccentCurve,
}

impl SentimentCurve {
    pub fn new(
        sentiment: Sentiment,
        brand_hue: f64,
        prototype_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        let prototype = sentiment.prototype_hue();
        let dist = angular_distance(prototype, brand_hue);

        let proto_accent = AccentCurve::new(prototype_hex, neutral)?;
        let sat_ratio = proto_accent.sat_ratio();

        let (resolved_hue, was_displaced) = if dist >= 15.0 {
            (prototype, false)
        } else {
            let h = minimize_cost(sentiment, prototype, brand_hue);
            (h, true)
        };

        let resolved_hue = sentiment
            .hue_floor()
            .map(|floor| {
                let h = ((resolved_hue % 360.0) + 360.0) % 360.0;
                if h < floor { floor } else { h }
            })
            .unwrap_or(((resolved_hue % 360.0) + 360.0) % 360.0);

        let displacement = angular_distance(resolved_hue, prototype);

        let canonical_hex = build_hex_from_hue(resolved_hue, sat_ratio, neutral);
        let accent = AccentCurve::new(&canonical_hex, neutral)
            .expect("generated hex must be valid");

        Ok(Self {
            resolved_hue,
            was_displaced,
            displacement,
            accent,
        })
    }

    pub fn at(&self, t: f64) -> LcsColor {
        self.accent.at(t)
    }

    pub fn sample(&self, n: usize) -> Vec<LcsColor> {
        self.accent.sample(n)
    }

    pub fn sample_hex(&self, n: usize) -> Vec<String> {
        self.accent.sample_hex(n)
    }

    pub fn accent(&self) -> &AccentCurve {
        &self.accent
    }
}

fn minimize_cost(sentiment: Sentiment, prototype: f64, brand_hue: f64) -> f64 {
    let (left_slope, right_slope) = sentiment.slope();
    let min_dist_from_brand = 20.0;

    let mut best_h = prototype;
    let mut best_cost = f64::MAX;

    for i in -360..=360i32 {
        let h = prototype + i as f64 * 0.5;
        let dist_from_brand = angular_distance(h, brand_hue);
        if dist_from_brand < min_dist_from_brand {
            continue;
        }

        let dh = angular_distance(h, prototype);
        let sign = ((h - prototype + 180.0).rem_euclid(360.0)) - 180.0;
        let slope = if sign >= 0.0 { right_slope } else { left_slope };
        let cost = slope / (1.0 - dh / 180.0).max(0.01);

        if cost < best_cost {
            best_cost = cost;
            best_h = h;
        }
    }

    ((best_h % 360.0) + 360.0) % 360.0
}

fn angular_distance(a: f64, b: f64) -> f64 {
    let diff = ((a - b) % 360.0 + 360.0) % 360.0;
    if diff > 180.0 { 360.0 - diff } else { diff }
}

fn build_hex_from_hue(h_ok: f64, sat_ratio: f64, neutral: &NeutralCurve) -> String {
    let base = neutral.base_anchor();
    let base_rgb = crate::spaces::srgb::srgb_from_hex(&base.to_hex())
        .unwrap_or([0.5, 0.5, 0.5]);
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(base_rgb);
    let l_ok = lab[0];

    let c_max = max_chroma(l_ok, h_ok);
    let c = c_max * sat_ratio;

    let a = c * h_ok.to_radians().cos();
    let b = c * h_ok.to_radians().sin();

    let rgb = oklab_to_srgb_linear([l_ok, a, b]);
    let rgb_clamped = [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ];

    hex_from_srgb(rgb_clamped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    fn prototype_hex(sent: Sentiment) -> &'static str {
        match sent {
            Sentiment::Danger => "#FF3B30",
            Sentiment::Warning => "#FF9500",
            Sentiment::Success => "#34C759",
            Sentiment::Info => "#007AFF",
        }
    }

    #[test]
    fn no_displacement_when_brand_far() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 240.0, "#FF3B30", &neutral).unwrap();
        assert!(!curve.was_displaced, "danger prototype=18, brand=240 — no conflict");
        assert!((curve.resolved_hue - 18.0).abs() < 1.0);
    }

    #[test]
    fn displacement_when_brand_near_prototype() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 20.0, "#FF3B30", &neutral).unwrap();
        assert!(curve.was_displaced, "danger prototype=18, brand=20 — conflict");
    }

    #[test]
    fn resolved_hue_distant_from_brand() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 20.0, "#FF3B30", &neutral).unwrap();
        let dist = angular_distance(curve.resolved_hue, 20.0);
        assert!(
            dist >= 19.0,
            "resolved_hue={} too close to brand=20: dist={}",
            curve.resolved_hue,
            dist
        );
    }

    #[test]
    fn warning_floor_enforced() {
        let neutral = default_neutral();
        for brand in (0..360).step_by(30) {
            let curve = SentimentCurve::new(Sentiment::Warning, brand as f64, "#FF9500", &neutral).unwrap();
            assert!(
                curve.resolved_hue >= 45.0,
                "warning resolved_hue={} below floor at brand={}",
                curve.resolved_hue,
                brand
            );
        }
    }

    #[test]
    fn warning_no_floor_when_far() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Warning, 300.0, "#FF9500", &neutral).unwrap();
        assert!(!curve.was_displaced);
        assert!((curve.resolved_hue - 67.0).abs() < 1.0);
    }

    #[test]
    fn jp_monotonically_decreasing() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Success, 10.0, "#34C759", &neutral).unwrap();
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
    fn s_non_negative() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Info, 10.0, "#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let c = curve.at(i as f64 / 50.0);
            assert!(c.s >= -1e-6, "negative s at t={}", i as f64 / 50.0);
        }
    }

    #[test]
    fn displacement_value_positive_when_displaced() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 20.0, "#FF3B30", &neutral).unwrap();
        if curve.was_displaced {
            assert!(curve.displacement > 0.0, "displacement should be positive");
        }
    }

    #[test]
    fn all_sentiments_valid_with_various_brands() {
        let neutral = default_neutral();
        let sentiments = [Sentiment::Danger, Sentiment::Warning, Sentiment::Success, Sentiment::Info];
        let brands = [0.0, 30.0, 60.0, 120.0, 200.0, 300.0];

        for &sent in &sentiments {
            for &brand in &brands {
                let curve = SentimentCurve::new(sent, brand, prototype_hex(sent), &neutral).unwrap();
                let hex = curve.at(0.5).to_hex();
                assert!(
                    LcsColor::from_hex(&hex).is_ok(),
                    "{:?} brand={} produced invalid color",
                    sent,
                    brand
                );
            }
        }
    }
}
