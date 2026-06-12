use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{AccentCurve, max_chroma};
use crate::spaces::oklab::oklab_to_srgb_linear;
use crate::spaces::srgb::hex_from_srgb;

/// Minimum angular separation (degrees) the resolved sentiment hue must keep
/// from the brand hue. Below this distance the prototype and the brand would
/// read as "the same colour" and the sentiment loses its semantic signal, so
/// the hue is displaced until it clears this margin.
///
/// This single constant is both the conflict trigger (displacement engages
/// when `dist(prototype, brand) < NO_CONFLICT_MIN_DIST`) and the floor of the
/// displacement result (the displaced hue lands exactly on this margin). Using
/// one value removes the former 15/20 gap where a brand 15–20° from the
/// prototype triggered no displacement yet still sat closer than 20° to brand.
const NO_CONFLICT_MIN_DIST: f64 = 20.0;

/// Sentiment categories. Each maps to a prototype hue expressed in
/// **Oklab hue degrees** (NOT HSB/HSL/sRGB hue). The resolved hue produced by
/// [`SentimentCurve`] is likewise an Oklab hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sentiment {
    Danger,
    Warning,
    Success,
    Info,
}

impl Sentiment {
    /// Ideal hue for this sentiment, in **Oklab hue degrees** (NOT HSB/HSL).
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
    /// Resolve a sentiment curve against a brand hue.
    ///
    /// `brand_hue` is an **Oklab hue in degrees** (NOT HSB/HSL/sRGB hue); so is
    /// the resulting [`resolved_hue`](Self::resolved_hue).
    ///
    /// # Invariants
    ///
    /// - If the prototype already sits at least [`NO_CONFLICT_MIN_DIST`] degrees
    ///   from `brand_hue`, no displacement happens and the prototype hue is used.
    /// - Otherwise the hue is displaced so that `resolved_hue` is **always** at
    ///   least [`NO_CONFLICT_MIN_DIST`] degrees from `brand_hue`. For
    ///   [`Sentiment::Warning`] the resolved hue additionally never drops below
    ///   the warning hue floor; the floor is applied as a constraint *inside*
    ///   the search, so it can never pull the hue back into the brand zone.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `brand_hue` is not finite, or if either the prototype or
    /// the generated canonical hex fails to construct an [`AccentCurve`].
    pub fn new(
        sentiment: Sentiment,
        brand_hue: f64,
        prototype_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        if !brand_hue.is_finite() {
            return Err(format!("brand_hue is not finite: {brand_hue}"));
        }

        let prototype = sentiment.prototype_hue();
        let dist = angular_distance(prototype, brand_hue);

        let proto_accent = AccentCurve::new(prototype_hex, neutral)?;
        let sat_ratio = proto_accent.sat_ratio();

        let (resolved_hue, was_displaced) = if dist >= NO_CONFLICT_MIN_DIST {
            (normalize_hue(prototype), false)
        } else {
            (resolve_displaced_hue(sentiment, prototype, brand_hue), true)
        };

        let displacement = angular_distance(resolved_hue, prototype);

        let canonical_hex = build_hex_from_hue(resolved_hue, sat_ratio, neutral);
        let accent = AccentCurve::new(&canonical_hex, neutral).map_err(|e| {
            format!("failed to build accent from generated hex {canonical_hex}: {e}")
        })?;

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

/// Cost of placing the resolved hue at displacement `dh` (degrees from the
/// prototype) on the given slope side.
///
/// The cost is monotonically increasing in `dh` on each side, so the cheapest
/// legal hue on a side is always the one with the *smallest* legal `dh` — i.e.
/// the boundary of the brand zone (or the warning floor) nearest the prototype.
/// This monotonicity is what lets [`resolve_displaced_hue`] evaluate a handful
/// of analytic candidates instead of scanning 721 sampled hues.
fn placement_cost(slope: f64, dh: f64) -> f64 {
    slope / (1.0 - dh / 180.0).max(0.01)
}

/// Analytically resolve the displaced hue.
///
/// The hue circle has up to two forbidden arcs: the brand zone (within
/// [`NO_CONFLICT_MIN_DIST`] of `brand_hue`) and, for [`Sentiment::Warning`],
/// the arc below the hue floor (`[0, floor)`). [`placement_cost`] is monotone
/// in the displacement on each slope side, so on any legal arc the cheapest
/// point sits at the arc edge nearest the prototype — i.e. just outside a
/// forbidden arc. The candidate set is therefore the legal hues immediately
/// flanking each forbidden arc's two edges; we score them all and keep the
/// cheapest. This replaces the former 721-iteration sampling sweep (Issue #8)
/// with a fixed handful of candidates while landing on the exact boundary
/// rather than the nearest 0.5deg grid sample.
fn resolve_displaced_hue(sentiment: Sentiment, prototype: f64, brand_hue: f64) -> f64 {
    let (left_slope, right_slope) = sentiment.slope();
    let floor = sentiment.hue_floor();

    // The brand and floor edges are themselves legal (the legality test uses
    // `>=`), so they are exact candidates: both brand-zone edges and the floor.
    let mut candidates: Vec<f64> = vec![
        normalize_hue(brand_hue - NO_CONFLICT_MIN_DIST),
        normalize_hue(brand_hue + NO_CONFLICT_MIN_DIST),
    ];

    // The floor forbidden arc `[0, floor)` also has a wrap edge at 360 ≡ 0. The
    // value 0 itself is below the floor (illegal), but the hue just below the
    // wrap (360⁻) is legal — and it is the cheap-slope candidate the brand-only
    // set used to miss. A tiny step below 360 selects that legal point; the
    // legality re-check still gates it, so a genuinely illegal sliver is
    // discarded rather than accepted.
    if let Some(f) = floor {
        candidates.push(normalize_hue(f));
        candidates.push(360.0 - 1e-6);
    }

    let mut best_h: Option<f64> = None;
    let mut best_cost = f64::MAX;

    for &h in &candidates {
        if !is_legal_hue(h, brand_hue, floor) {
            continue;
        }
        let dh = angular_distance(h, prototype);
        let slope = if signed_delta(h, prototype) >= 0.0 {
            right_slope
        } else {
            left_slope
        };
        let cost = placement_cost(slope, dh);
        if cost < best_cost {
            best_cost = cost;
            best_h = Some(h);
        }
    }

    if let Some(h) = best_h {
        return h;
    }

    // Fallback: no flanking candidate is legal (the forbidden arcs leave only a
    // sliver). Sweep outward from the prototype on both sides and take the first
    // legal hue. The brand arc is only 2*NO_CONFLICT_MIN_DIST wide, so a legal
    // hue always exists within one revolution.
    let mut step = 0.0_f64;
    while step <= 360.0 {
        for cand in [
            normalize_hue(prototype + step),
            normalize_hue(prototype - step),
        ] {
            if is_legal_hue(cand, brand_hue, floor) {
                return cand;
            }
        }
        step += 0.1;
    }

    // Unreachable in practice (a legal hue always exists), but never panic.
    normalize_hue(prototype)
}

/// A hue is legal if it clears the brand zone and, for Warning, sits at or above
/// the hue floor.
fn is_legal_hue(h: f64, brand_hue: f64, floor: Option<f64>) -> bool {
    if angular_distance(h, brand_hue) < NO_CONFLICT_MIN_DIST {
        return false;
    }
    if let Some(f) = floor
        && normalize_hue(h) < f
    {
        return false;
    }
    true
}

/// Signed shortest delta from `from` to `h` in (-180, 180].
fn signed_delta(h: f64, from: f64) -> f64 {
    ((h - from + 180.0).rem_euclid(360.0)) - 180.0
}

fn normalize_hue(h: f64) -> f64 {
    ((h % 360.0) + 360.0) % 360.0
}

fn angular_distance(a: f64, b: f64) -> f64 {
    let diff = ((a - b) % 360.0 + 360.0) % 360.0;
    if diff > 180.0 { 360.0 - diff } else { diff }
}

fn build_hex_from_hue(h_ok: f64, sat_ratio: f64, neutral: &NeutralCurve) -> String {
    let base = neutral.base_anchor();
    // Issue #26: the base anchor was built under the neutral curve's own viewing
    // conditions, so it must round-trip through the same VC. Using the sRGB VC
    // (`to_hex`) drifts the lightness reference in dark themes.
    let base_hex = base.to_hex_with_vc(neutral.vc());
    let base_rgb = crate::spaces::srgb::srgb_from_hex(&base_hex).unwrap_or([0.5, 0.5, 0.5]);
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
        assert!(
            !curve.was_displaced,
            "danger prototype=18, brand=240 — no conflict"
        );
        assert!((curve.resolved_hue - 18.0).abs() < 1.0);
    }

    #[test]
    fn displacement_when_brand_near_prototype() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 20.0, "#FF3B30", &neutral).unwrap();
        assert!(
            curve.was_displaced,
            "danger prototype=18, brand=20 — conflict"
        );
    }

    #[test]
    fn resolved_hue_distant_from_brand() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 20.0, "#FF3B30", &neutral).unwrap();
        let dist = angular_distance(curve.resolved_hue, 20.0);
        // Contract change: the displaced hue now lands exactly on the unified
        // NO_CONFLICT_MIN_DIST boundary (20deg), so the guarantee is tight.
        assert!(
            dist >= NO_CONFLICT_MIN_DIST - 1e-6,
            "resolved_hue={} too close to brand=20: dist={}",
            curve.resolved_hue,
            dist
        );
    }

    // Regression for the former 15/20 threshold gap: a brand 17deg from the
    // prototype used to skip displacement (dist >= 15) yet ended up only ~17deg
    // from brand, violating the 20deg margin. With the unified threshold it now
    // displaces and clears 20deg.
    #[test]
    fn displaces_in_former_threshold_gap() {
        let neutral = default_neutral();
        // Danger prototype = 18deg; brand = 35deg -> dist = 17deg (in [15, 20)).
        let curve = SentimentCurve::new(Sentiment::Danger, 35.0, "#FF3B30", &neutral).unwrap();
        assert!(
            curve.was_displaced,
            "brand 17deg from prototype must displace under unified threshold"
        );
        let dist = angular_distance(curve.resolved_hue, 35.0);
        assert!(
            dist >= NO_CONFLICT_MIN_DIST - 1e-6,
            "resolved_hue={} only {}deg from brand=35",
            curve.resolved_hue,
            dist
        );
    }

    #[test]
    fn warning_floor_enforced() {
        let neutral = default_neutral();
        for brand in (0..360).step_by(30) {
            let curve =
                SentimentCurve::new(Sentiment::Warning, brand as f64, "#FF9500", &neutral).unwrap();
            assert!(
                curve.resolved_hue >= 45.0,
                "warning resolved_hue={} below floor at brand={}",
                curve.resolved_hue,
                brand
            );
        }
    }

    // Defect #4: the warning floor was applied AFTER minimisation, so a brand
    // near 50deg produced resolved 45 (floor) sitting only ~5deg from brand.
    // The floor is now a constraint inside the search, so the resolved hue is
    // always at or above the floor AND at least NO_CONFLICT_MIN_DIST from brand.
    #[test]
    fn warning_floor_never_breaches_brand_zone() {
        let neutral = default_neutral();
        for brand in [50.0_f64, 55.0, 60.0] {
            let curve =
                SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &neutral).unwrap();
            assert!(
                curve.resolved_hue >= 45.0,
                "warning resolved_hue={} below floor at brand={}",
                curve.resolved_hue,
                brand
            );
            let dist = angular_distance(curve.resolved_hue, brand);
            assert!(
                dist >= NO_CONFLICT_MIN_DIST - 1e-6,
                "warning resolved_hue={} only {}deg from brand={} (floor breached the brand zone)",
                curve.resolved_hue,
                dist,
                brand
            );
        }
    }

    #[test]
    fn rejects_non_finite_brand_hue() {
        let neutral = default_neutral();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = SentimentCurve::new(Sentiment::Info, bad, "#007AFF", &neutral);
            assert!(
                result.is_err(),
                "non-finite brand_hue {bad} must be rejected"
            );
        }
    }

    // Reference implementation: the original 721-iteration sampling search,
    // kept only as a test oracle for the analytic resolve_displaced_hue.
    fn reference_minimize_cost(sentiment: Sentiment, prototype: f64, brand_hue: f64) -> f64 {
        let (left_slope, right_slope) = sentiment.slope();
        let floor = sentiment.hue_floor();
        let min_dist_from_brand = NO_CONFLICT_MIN_DIST;

        let mut best_h = prototype;
        let mut best_cost = f64::MAX;
        let mut found = false;

        for i in -360..=360i32 {
            let h = prototype + i as f64 * 0.5;
            if angular_distance(h, brand_hue) < min_dist_from_brand {
                continue;
            }
            if let Some(f) = floor
                && normalize_hue(h) < f
            {
                continue;
            }

            let dh = angular_distance(h, prototype);
            let sign = signed_delta(h, prototype);
            let slope = if sign >= 0.0 { right_slope } else { left_slope };
            let cost = placement_cost(slope, dh);

            if cost < best_cost {
                best_cost = cost;
                best_h = h;
                found = true;
            }
        }

        assert!(found, "reference found no legal hue");
        normalize_hue(best_h)
    }

    // Defect #8: the analytic resolver must agree with the reference sampling
    // search across the full brand-hue grid for all sentiments. The reference
    // samples at 0.5deg granularity, so the resolved cost (not the raw hue,
    // which can differ by sub-degree rounding) must match within that grid.
    #[test]
    fn analytic_matches_reference_grid() {
        let sentiments = [
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ];
        for &sent in &sentiments {
            let prototype = sent.prototype_hue();
            let (left_slope, right_slope) = sent.slope();
            let floor = sent.hue_floor();
            for brand_i in 0..360 {
                let brand = brand_i as f64;
                // Only the displacement path is under test (dist < threshold).
                if angular_distance(prototype, brand) >= NO_CONFLICT_MIN_DIST {
                    continue;
                }

                let analytic = resolve_displaced_hue(sent, prototype, brand);
                let reference = reference_minimize_cost(sent, prototype, brand);

                // Both must be legal.
                assert!(
                    is_legal_hue(analytic, brand, floor),
                    "{sent:?} brand={brand}: analytic hue {analytic} illegal"
                );

                // Cost parity: the analytic optimum must be at least as cheap as
                // the reference (the reference is grid-limited; analytic sits on
                // the exact boundary, so analytic_cost <= reference_cost always).
                let cost_of = |h: f64| {
                    let dh = angular_distance(h, prototype);
                    let slope = if signed_delta(h, prototype) >= 0.0 {
                        right_slope
                    } else {
                        left_slope
                    };
                    placement_cost(slope, dh)
                };
                let analytic_cost = cost_of(analytic);
                let reference_cost = cost_of(reference);
                assert!(
                    analytic_cost <= reference_cost + 1e-6,
                    "{sent:?} brand={brand}: analytic cost {analytic_cost} worse than reference {reference_cost} (analytic={analytic}, reference={reference})"
                );
            }
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
        let sentiments = [
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ];
        let brands = [0.0, 30.0, 60.0, 120.0, 200.0, 300.0];

        for &sent in &sentiments {
            for &brand in &brands {
                let curve =
                    SentimentCurve::new(sent, brand, prototype_hex(sent), &neutral).unwrap();
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
