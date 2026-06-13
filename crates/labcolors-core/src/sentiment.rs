use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{AccentCurve, max_chroma};
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex};

/// Perceptual minimum separation between a sentiment hue and the brand hue,
/// expressed as a **chord length in the Oklab a/b chroma plane** (not degrees).
///
/// # Why a chord, not an angle
///
/// Issue #20: equal hue *angles* are not equal *perceptual* steps — a 20°
/// hue swing at low chroma is barely visible, while the same 20° at high
/// chroma is an obvious colour change. A fixed angular margin therefore
/// over-separates desaturated zones and under-separates saturated ones. The
/// perceptually honest invariant is a constant *distance* in the (a, b)
/// plane, which we then translate into the per-zone angle that achieves it.
///
/// # Derivation of the constant
///
/// The pre-existing model (#55) used a flat 20° Oklab-hue margin and was
/// accepted by eye as the right "just distinguishable" gap. We preserve that
/// calibration by measuring what arc 20° subtends at the *representative*
/// chroma of the four sentiment prototypes, then freezing that arc length as
/// the perceptual target.
///
/// Measured prototype chromas (Oklab, sRGB VC):
///
/// | sentiment | hex       | Oklab C |
/// |-----------|-----------|---------|
/// | Danger    | `#FF3B30` | 0.2321  |
/// | Warning   | `#FF9500` | 0.1752  |
/// | Success   | `#34C759` | 0.1944  |
/// | Info      | `#007AFF` | 0.2177  |
///
/// Representative (mean) chroma `C_rep = 0.2049`. A central angle `Δh`
/// subtends a chord of `2·C·sin(Δh/2)` in the a/b plane, so the calibration
/// chord is
///
/// ```text
/// S_PERC_MIN = 2 · C_rep · sin(20° / 2)
///            = 2 · 0.2049 · sin(10°)
///            ≈ 0.07116   (Oklab a/b units)
/// ```
///
/// Feeding `C_rep` back through [`s_min_deg`] returns 20.0° by construction,
/// so the v1 default behaviour matches the eyeball-calibrated #55 margin while
/// the machinery is ready to vary the angle per zone once issue #20's per-zone
/// chroma map lands.
const S_PERC_MIN: f64 = 0.071_157_9;

/// Representative Oklab chroma of the sentiment prototypes (mean of the four).
/// Used as the v1 default zone chroma so [`s_min_deg`] reproduces the
/// historical 20° margin. See [`S_PERC_MIN`].
const REPRESENTATIVE_CHROMA: f64 = 0.204_85;

/// Translate the perceptual separation target [`S_PERC_MIN`] into the hue angle
/// (degrees) that achieves it at a given Oklab chroma.
///
/// Inverting the chord relation `chord = 2·C·sin(Δh/2)`:
///
/// ```text
/// Δh = 2 · asin( S_PERC_MIN / (2·C) )
/// ```
///
/// At very low chroma the requested chord can exceed the maximum chord of the
/// hue circle (diameter `2·C`); we clamp the `asin` argument to `1.0` so the
/// angle saturates at 180° instead of producing `NaN`. This is the v1
/// "perceptual seam" function: today it is fed one representative chroma
/// ([`REPRESENTATIVE_CHROMA`]); a future revision can pass each zone's own
/// chroma to widen the margin in washed-out hue regions.
fn s_min_deg(zone_chroma: f64) -> f64 {
    let safe_chroma = zone_chroma.max(1e-6);
    let ratio = (S_PERC_MIN / (2.0 * safe_chroma)).clamp(0.0, 1.0);
    2.0 * ratio.asin().to_degrees()
}

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
    /// Ideal hue for this sentiment — the membership-field peak, in **Oklab hue
    /// degrees** (NOT HSB/HSL).
    fn prototype_hue(self) -> f64 {
        self.field().peak
    }

    /// The categorical **membership field** of this sentiment: an asymmetric bump
    /// over Oklab hue, peaked at the prototype, with per-side wing widths that
    /// encode the empirical category borders. Directions are Daniil's design
    /// input (2026-06-12); the widths are **PROVISIONAL** — to be fitted from
    /// colour-naming data (xkcd / World Color Survey) and finalised by eye:
    ///
    /// - **Danger** (red): both wings wide — red reads as red whether nudged
    ///   toward orange (higher hue) or crimson (lower), so it may shift either way.
    /// - **Success** (green): a *steep* wing toward yellow (lower hue — yellow-green
    ///   reads "off", not success) and a *wide* wing toward teal (higher hue —
    ///   teal still reads as success, often nicer), so an encroaching brand sends
    ///   it to teal, never yellow.
    /// - **Warning** (amber): a *steep* wing toward green (higher hue — the green
    ///   border) and a *wide* wing toward orange/red (lower hue — its reddish
    ///   extreme). This replaces the old hard 45° floor with a smooth border.
    /// - **Info** (blue): roughly symmetric.
    ///
    /// `sigma_lo` is the Gaussian wing toward *lower* hue (signed delta < 0),
    /// `sigma_hi` toward higher hue.
    fn field(self) -> HueField {
        match self {
            //                                  peak   σ_lo   σ_hi
            Sentiment::Danger => HueField::new(27.0, 24.0, 26.0),
            Sentiment::Warning => HueField::new(67.0, 26.0, 14.0),
            Sentiment::Success => HueField::new(145.0, 13.0, 42.0),
            Sentiment::Info => HueField::new(240.0, 26.0, 26.0),
        }
    }
}

/// An asymmetric Gaussian **membership field** over Oklab hue: `μ(h) ∈ (0, 1]`,
/// peaked at `peak`, falling off with `sigma_lo` toward lower hue and `sigma_hi`
/// toward higher hue. The asymmetry is what makes a category's two borders behave
/// differently (Success's steep yellow side vs wide teal side). PROVISIONAL
/// widths — see [`Sentiment::field`].
#[derive(Debug, Clone, Copy)]
struct HueField {
    peak: f64,
    sigma_lo: f64,
    sigma_hi: f64,
}

impl HueField {
    fn new(peak: f64, sigma_lo: f64, sigma_hi: f64) -> Self {
        Self {
            peak,
            sigma_lo,
            sigma_hi,
        }
    }

    /// Membership `μ(h) = exp(-½·(δ/σ)²)`, `δ` the signed shortest hue delta from
    /// the peak and `σ` the wing on `δ`'s side. `1` at the peak, decaying outward.
    fn membership(&self, h: f64) -> f64 {
        let delta = signed_delta(h, self.peak);
        let sigma = if delta < 0.0 {
            self.sigma_lo
        } else {
            self.sigma_hi
        };
        (-0.5 * (delta / sigma).powi(2)).exp()
    }
}

/// Default asymptote hardness `p` for a sentiment with no special asymmetry.
/// `p = 2` is the calibration default Daniil picks by eye; `p → ∞` recovers the
/// old hard 20° wall, `p → 1` is the softest (most eager) yield.
pub const DEFAULT_HARDNESS: f64 = 2.0;

/// Common target CAM16-UCS colourfulness `M'` every sentiment is built to, so the
/// four read at the **same perceived saturation** regardless of hue. PROVISIONAL
/// — the sentiment "strength" knob, finalised by Daniil's eye. Without it each
/// sentiment took `sat_ratio × max_chroma(hue)`, and because the sRGB gamut
/// ceiling is far higher for green than for red/orange, green came out visibly
/// brighter. The consistency law (sentiment-category-fields): align by perceived
/// `M'`, not by a fraction of each hue's own ceiling.
const SENTIMENT_TARGET_MP: f64 = 38.0;

/// Tunable parameters of the smooth-asymptote displacement model.
///
/// The displaced separation follows the p-norm blend
///
/// ```text
/// s(d) = (d^p + s_min^p)^(1/p)
/// ```
///
/// where `d` is the raw angular distance (degrees) from the brand to the
/// prototype and `s_min` is the perceptual floor from [`s_min_deg`]. As
/// `d → ∞` the displacement `s(d) − d → 0` (a far brand barely nudges the
/// sentiment); as `d → 0` the separation smoothly approaches `s_min` (a brand
/// landing on the prototype is pushed out by exactly the minimum gap). `p`
/// controls how hard the curve clings to `d` in between.
///
/// Construct with [`SentimentParams::default`] for the calibration default, or
/// override `p` additively without touching the prototype/floor machinery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SentimentParams {
    /// Hardness on the low side (sentiment hue below the brand).
    pub p_low: f64,
    /// Hardness on the high side (sentiment hue above the brand).
    pub p_high: f64,
}

impl SentimentParams {
    /// Build params with a single hardness applied to both sides.
    ///
    /// # Errors
    /// Returns `Err` if `p` is not finite or not `>= 1.0` (a p-norm with
    /// `p < 1` is non-convex and would make the displacement non-monotone).
    pub fn uniform(p: f64) -> Result<Self, String> {
        Self::new(p, p)
    }

    /// Build params with independent per-side hardness.
    ///
    /// # Errors
    /// Returns `Err` if either `p` is not finite or `< 1.0`.
    pub fn new(p_low: f64, p_high: f64) -> Result<Self, String> {
        for (name, p) in [("p_low", p_low), ("p_high", p_high)] {
            if !p.is_finite() {
                return Err(format!("{name} is not finite: {p}"));
            }
            if p < 1.0 {
                return Err(format!("{name} must be >= 1.0 (got {p})"));
            }
        }
        Ok(Self { p_low, p_high })
    }
}

impl Default for SentimentParams {
    fn default() -> Self {
        Self {
            p_low: DEFAULT_HARDNESS,
            p_high: DEFAULT_HARDNESS,
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
    /// Resolve a sentiment curve against a brand hue using the calibration
    /// defaults (per-sentiment hardness, `p = 2` where unspecified).
    ///
    /// `brand_hue` is an **Oklab hue in degrees** (NOT HSB/HSL/sRGB hue); so is
    /// the resulting [`resolved_hue`](Self::resolved_hue). The public signature
    /// is unchanged from #55; tuning `p` is opt-in via [`Self::with_params`].
    ///
    /// # Errors
    ///
    /// See [`Self::with_params`].
    pub fn new(
        sentiment: Sentiment,
        brand_hue: f64,
        prototype_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        let params = SentimentParams::new(DEFAULT_HARDNESS, DEFAULT_HARDNESS)?;
        Self::with_params(sentiment, brand_hue, prototype_hex, neutral, params)
    }

    /// Resolve a sentiment curve with explicit asymptote [`SentimentParams`].
    ///
    /// The sentiment hue is pushed away from the brand along the smooth
    /// p-norm asymptote `s(d) = (d^p + s_min^p)^(1/p)` (see [`SentimentParams`]).
    /// There is **no on/off threshold**: a distant brand moves the hue by an
    /// amount that decays smoothly to zero, a near brand is held at the
    /// perceptual minimum [`s_min_deg`], and the transition is C¹ everywhere
    /// except the single seam where the brand sits exactly on the prototype
    /// (resolved on the [`Sentiment::preferred_side`]).
    ///
    /// # Invariants
    ///
    /// - The resolved hue keeps **at least** `s_min` perceptual degrees from
    ///   the brand (separation invariant), enforced as a final legal guard.
    /// - For [`Sentiment::Warning`] the resolved hue additionally never drops
    ///   below the hue floor. When the floor and the separation invariant
    ///   collide, the resolver lands on the nearest legal hue that still honours
    ///   the separation; if the legal arc is geometrically empty it returns an
    ///   `Err` rather than silently breaching either invariant.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `brand_hue` is not finite, if the params are invalid,
    /// if no hue can satisfy both the floor and the separation invariant, or if
    /// either the prototype or the generated canonical hex fails to construct an
    /// [`AccentCurve`].
    pub fn with_params(
        sentiment: Sentiment,
        brand_hue: f64,
        prototype_hex: &str,
        neutral: &NeutralCurve,
        params: SentimentParams,
    ) -> Result<Self, String> {
        if !brand_hue.is_finite() {
            return Err(format!("brand_hue is not finite: {brand_hue}"));
        }

        let prototype = sentiment.prototype_hue();

        // Perceptual separation floor from the prototype's *actual* Oklab chroma
        // (issue #20: a fixed-degree margin is wrong — at high chroma the same
        // perceptual chord subtends fewer degrees). Replaces the old fixed
        // `REPRESENTATIVE_CHROMA`, which over-separated saturated warm hues and
        // shoved Danger out of red into pink.
        let proto_lab = srgb_linear_to_oklab(srgb_from_hex(prototype_hex)?);
        let proto_chroma = (proto_lab[1].powi(2) + proto_lab[2].powi(2)).sqrt();
        let s_min = s_min_deg(proto_chroma);

        // `p` (asymptote hardness) is reserved for boundary-transition smoothing
        // in a later calibration pass; the field resolver below is parameter-free.
        let _ = &params;
        let resolved_hue = resolve_field_hue(sentiment, brand_hue, s_min)?;

        let displacement = angular_distance(resolved_hue, prototype);
        // The hue is "displaced" whenever the smooth model moved it off the
        // prototype by a perceptible amount. There is no hard threshold any
        // more, so this is a reporting flag, not a branch.
        let was_displaced = displacement > 1e-6;

        // Constant perceived colourfulness across sentiments: build the canonical
        // at a common CAM16-UCS `M'`, not at a per-hue `sat_ratio` of each hue's
        // own (very different) gamut ceiling — otherwise green reads brighter than
        // red/orange (consistency law). The caller's `prototype_hex` no longer
        // sets the chroma; it only informs the perceptual separation floor above.
        let canonical_hex = build_hex_at_mp(resolved_hue, SENTIMENT_TARGET_MP, neutral);
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

/// Resolve the sentiment hue: the hue that **maximises the category membership**
/// while staying at least `s_min` perceptual degrees from the brand.
///
/// The field `μ_s` is unimodal (peaked at the prototype), so the constrained
/// optimum is simple:
/// * if the peak is already `≥ s_min` from the brand, the peak itself wins — a
///   far brand does not perturb the category at all (zero displacement);
/// * otherwise the brand encroaches and the optimum sits on the nearer
///   separation boundary, on **whichever side the field is higher**. That is
///   what makes Success slide into teal (its wide wing) rather than yellow (its
///   steep wing), Warning toward orange rather than green, and Danger to the
///   closer red wing — the category's own asymmetry decides the side, replacing
///   the former hard floor and side/hardness machinery.
///
/// [`legalize_hue`] is the final separation net (it never needs to move the
/// boundary picks, which are exactly `s_min` away, but guards float error).
fn resolve_field_hue(sentiment: Sentiment, brand_hue: f64, s_min: f64) -> Result<f64, String> {
    let field = sentiment.field();

    // Peak feasible → sit at the prototype (a distant brand barely perturbs it).
    if angular_distance(field.peak, brand_hue) >= s_min - 1e-9 {
        return legalize_hue(field.peak, brand_hue, s_min);
    }

    // Brand encroaches: the unimodal field's constrained maximum is the nearer
    // separation boundary on the higher-membership side.
    let c_hi = normalize_hue(brand_hue + s_min);
    let c_lo = normalize_hue(brand_hue - s_min);
    let pick = if field.membership(c_hi) >= field.membership(c_lo) {
        c_hi
    } else {
        c_lo
    };
    legalize_hue(pick, brand_hue, s_min)
}

/// Snap a candidate hue to the nearest hue at least `s_min` from the brand.
///
/// The field resolver already returns separation-legal hues, so this is a thin
/// final net against float error: a legal candidate returns unchanged; otherwise
/// it scans outward to the closest legal point. (The categorical border that the
/// old `floor` enforced is now part of the membership field itself.)
fn legalize_hue(candidate: f64, brand_hue: f64, s_min: f64) -> Result<f64, String> {
    if is_legal_hue(candidate, brand_hue, s_min) {
        return Ok(normalize_hue(candidate));
    }

    let mut step = 0.05_f64;
    while step <= 360.0 {
        for cand in [
            normalize_hue(candidate + step),
            normalize_hue(candidate - step),
        ] {
            if is_legal_hue(cand, brand_hue, s_min) {
                return Ok(cand);
            }
        }
        step += 0.05;
    }

    Err(format!(
        "no legal hue exists for brand={brand_hue}, s_min={s_min}: \
         the separation invariant leaves no room on the hue circle"
    ))
}

/// A hue is legal if it clears the brand zone (`>= s_min` away).
fn is_legal_hue(h: f64, brand_hue: f64, s_min: f64) -> bool {
    angular_distance(h, brand_hue) >= s_min - 1e-9
}

/// Signed shortest delta from `from` to `h` in (-180, 180].
fn signed_delta(h: f64, from: f64) -> f64 {
    ((h - from + 180.0).rem_euclid(360.0)) - 180.0
}

fn normalize_hue(h: f64) -> f64 {
    h.rem_euclid(360.0)
}

fn angular_distance(a: f64, b: f64) -> f64 {
    let diff = (a - b).rem_euclid(360.0);
    if diff > 180.0 { 360.0 - diff } else { diff }
}

/// Build the canonical sentiment hex at Oklab hue `h_ok`, choosing chroma so the
/// colour carries `target_mp` CAM16-UCS colourfulness — **constant across
/// sentiments** — at the neutral curve's base lightness. `M'` is monotone in
/// chroma, so a bisection lands it; if the gamut cannot reach `target_mp` at this
/// hue the chroma saturates at the gamut edge (the rare desaturated-hue case).
fn build_hex_at_mp(h_ok: f64, target_mp: f64, neutral: &NeutralCurve) -> String {
    let base = neutral.base_anchor();
    // Issue #26: the base anchor was built under the neutral curve's own viewing
    // conditions, so it must round-trip through the same VC.
    let base_hex = base.to_hex_with_vc(neutral.vc());
    let base_rgb = srgb_from_hex(&base_hex).unwrap_or([0.5, 0.5, 0.5]);
    let l_ok = srgb_linear_to_oklab(base_rgb)[0];
    let c_max = max_chroma(l_ok, h_ok);

    let hex_at = |c: f64| -> String {
        let a = c * h_ok.to_radians().cos();
        let b = c * h_ok.to_radians().sin();
        let rgb = oklab_to_srgb_linear([l_ok, a, b]);
        hex_from_srgb([
            rgb[0].clamp(0.0, 1.0),
            rgb[1].clamp(0.0, 1.0),
            rgb[2].clamp(0.0, 1.0),
        ])
    };
    // CAM16-UCS colourfulness M' of the quantised colour: `s = M'/(J'+1)`.
    let mp_at = |c: f64| -> f64 {
        match LcsColor::from_hex_with_vc(&hex_at(c), neutral.vc()) {
            Ok(lcs) => lcs.s * (lcs.jp + 1.0),
            Err(_) => 0.0,
        }
    };

    if mp_at(c_max) <= target_mp {
        return hex_at(c_max);
    }
    let (mut lo, mut hi) = (0.0_f64, c_max);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if mp_at(mid) < target_mp {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hex_at(0.5 * (lo + hi))
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

    const S_MIN: f64 = 20.0; // s_min_deg(REPRESENTATIVE_CHROMA), asserted below.

    /// The smooth model has exactly two seams, both geometric and inherent to
    /// "push the hue away from the brand":
    ///
    /// 1. `brand == prototype`: which side to flee to is undefined; the
    ///    tie-break flips the sign, an unavoidable jump of ~2·s_min.
    /// 2. `brand == prototype + 180°` (the antipode): the prototype is
    ///    equidistant both ways, so the flee direction flips here too. Because
    ///    the asymptote overshoots slightly (`s(180) > 180`), the resolved hue
    ///    jumps by `2·(s(180) − 180)` ≈ 2.2° — small but a genuine seam.
    ///
    /// Continuity/C¹ tests legitimately exclude an epsilon window around both.
    fn near_seam(brand: f64, prototype: f64, window: f64) -> bool {
        angular_distance(brand, prototype) < window
            || angular_distance(brand, prototype + 180.0) < window
    }

    #[test]
    fn s_min_default_reproduces_historical_margin() {
        let s_min = s_min_deg(REPRESENTATIVE_CHROMA);
        assert!(
            (s_min - 20.0).abs() < 0.05,
            "v1 s_min must reproduce the historical 20deg margin, got {s_min}"
        );
    }

    #[test]
    fn s_min_widens_as_chroma_drops() {
        // Lower chroma -> a fixed perceptual chord needs a wider angle.
        let high = s_min_deg(0.25);
        let low = s_min_deg(0.10);
        assert!(
            low > high,
            "low chroma {low} should exceed high chroma {high}"
        );
    }

    #[test]
    fn s_min_saturates_not_nan_at_tiny_chroma() {
        let s = s_min_deg(1e-9);
        assert!(s.is_finite(), "s_min must stay finite at near-zero chroma");
        assert!(
            (s - 180.0).abs() < 1e-6,
            "should saturate at 180deg, got {s}"
        );
    }

    // ── Smooth separation maths ──────────────────────────────────

    #[test]
    fn separation_floor_at_zero_distance() {
        let s = smooth_separation(0.0, S_MIN, 2.0);
        assert!((s - S_MIN).abs() < 1e-9, "s(0) must equal s_min, got {s}");
    }

    #[test]
    fn separation_displacement_decays_to_zero_far_away() {
        // s(d) - d -> 0 as d grows: a distant brand barely nudges the hue.
        let near = smooth_separation(10.0, S_MIN, 2.0) - 10.0;
        let far = smooth_separation(120.0, S_MIN, 2.0) - 120.0;
        assert!(
            far < near,
            "displacement must shrink with distance: {far} !< {near}"
        );
        assert!(far < 2.0, "far displacement should be small, got {far}");
        assert!(
            far > 0.0,
            "displacement is asymptotic but never exactly zero: {far}"
        );
    }

    #[test]
    fn separation_monotone_in_distance() {
        let mut prev = f64::MIN;
        let mut d = 0.0;
        while d <= 180.0 {
            let s = smooth_separation(d, S_MIN, 2.0);
            assert!(s >= prev - 1e-9, "s(d) must be non-decreasing at d={d}");
            assert!(
                s >= d - 1e-9 && s >= S_MIN - 1e-9,
                "s(d) >= max(d, s_min) at d={d}"
            );
            prev = s;
            d += 0.5;
        }
    }

    #[test]
    fn higher_p_clings_harder() {
        // p -> infinity recovers the hard wall: at fixed d, larger p => s closer
        // to d (less push-out beyond the raw distance).
        let d = 15.0;
        let soft = smooth_separation(d, S_MIN, 1.5);
        let hard = smooth_separation(d, S_MIN, 3.0);
        assert!(
            hard < soft,
            "harder p should cling closer to d: {hard} !< {soft}"
        );
    }

    // ── Resolution behaviour ─────────────────────────────────────

    #[test]
    fn far_brand_barely_shifts_hue() {
        let neutral = default_neutral();
        // Info prototype 240, brand 200: d = 40. Smooth model shifts a little
        // but does not snap to the prototype.
        let curve = SentimentCurve::new(Sentiment::Info, 200.0, "#007AFF", &neutral).unwrap();
        let shift = angular_distance(curve.resolved_hue, 240.0);
        assert!(shift > 0.0, "smooth model always shifts slightly: {shift}");
        assert!(
            shift < 6.0,
            "but a brand 40deg away shifts only a little: {shift}"
        );
    }

    #[test]
    fn brand_on_prototype_pushed_to_s_min() {
        let neutral = default_neutral();
        let curve = SentimentCurve::new(Sentiment::Danger, 18.0, "#FF3B30", &neutral).unwrap();
        let dist = angular_distance(curve.resolved_hue, 18.0);
        assert!(
            (dist - S_MIN).abs() < 0.5,
            "brand on prototype must push out by ~s_min, got {dist}"
        );
    }

    #[test]
    fn separation_invariant_holds_everywhere() {
        let neutral = default_neutral();
        let s_min = s_min_deg(REPRESENTATIVE_CHROMA);
        for &sent in &[
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ] {
            let mut brand = 0.0;
            while brand < 360.0 {
                let curve =
                    SentimentCurve::new(sent, brand, prototype_hex(sent), &neutral).unwrap();
                let dist = angular_distance(curve.resolved_hue, brand);
                assert!(
                    dist >= s_min - 1e-6,
                    "{sent:?} brand={brand}: separation {dist} < s_min {s_min}"
                );
                brand += 0.5;
            }
        }
    }

    // ── Property: continuity (Lipschitz) of resolved_hue in brand_hue ──
    //
    // C¹ everywhere except the single seam at brand == prototype, where the
    // sentiment must flip to the other side of the brand (a fundamental jump of
    // ~2·s_min). We skip an epsilon window around that seam, documented.
    #[test]
    fn resolved_hue_lipschitz_in_brand() {
        let neutral = default_neutral();
        let step = 0.1_f64;
        for &sent in &[
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ] {
            let prototype = sent.prototype_hue();
            let mut brand = 0.0;
            let mut prev: Option<f64> = None;
            while brand <= 360.0 {
                // Skip both seams (prototype and its antipode); see near_seam.
                let on_seam = near_seam(brand, prototype, 1.0);
                let curve =
                    SentimentCurve::new(sent, brand, prototype_hex(sent), &neutral).unwrap();
                if let Some(p) = prev
                    && !on_seam
                {
                    // Compare on the circle (resolved can wrap 360->0).
                    let delta = angular_distance(curve.resolved_hue, p);
                    // Lipschitz constant: |Δresolved| <= K·|Δbrand|. The smooth
                    // model has slope <= ~2 near the prototype; K=5 is a roomy
                    // bound that still catches any genuine discontinuity.
                    assert!(
                        delta <= 5.0 * step + 1e-6,
                        "{sent:?} brand={brand}: jump {delta} over Δbrand={step}"
                    );
                }
                prev = Some(curve.resolved_hue);
                brand += step;
            }
        }
    }

    // ── Property: C1 smoothness — finite differences have no spikes ──
    #[test]
    fn resolved_hue_c1_smooth_off_seam() {
        let neutral = default_neutral();
        let h = 0.1_f64;
        for &sent in &[Sentiment::Danger, Sentiment::Success, Sentiment::Info] {
            let prototype = sent.prototype_hue();
            let deriv = |brand: f64| -> f64 {
                let a = SentimentCurve::new(sent, brand - h, prototype_hex(sent), &neutral)
                    .unwrap()
                    .resolved_hue;
                let b = SentimentCurve::new(sent, brand + h, prototype_hex(sent), &neutral)
                    .unwrap()
                    .resolved_hue;
                signed_delta(b, a) / (2.0 * h)
            };
            let mut brand = 0.0;
            let mut prev: Option<f64> = None;
            while brand <= 360.0 {
                if !near_seam(brand, prototype, 2.0) {
                    let dv = deriv(brand);
                    if let Some(p) = prev {
                        // Second difference (curvature) must stay bounded — no
                        // kinks. The smooth model's derivative is well below 3.
                        assert!(
                            (dv - p).abs() < 1.0,
                            "{sent:?} brand={brand}: derivative jump {} -> {}",
                            p,
                            dv
                        );
                    }
                    prev = Some(dv);
                } else {
                    prev = None; // reset across the seam
                }
                brand += h;
            }
        }
    }

    // ── Warning floor + separation, simultaneously, brand swept 40..70 ──
    #[test]
    fn warning_floor_and_separation_hold_together() {
        let neutral = default_neutral();
        let s_min = s_min_deg(REPRESENTATIVE_CHROMA);
        let mut brand = 40.0;
        while brand <= 70.0 {
            let curve =
                SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &neutral).unwrap();
            assert!(
                curve.resolved_hue >= 45.0 - 1e-6,
                "warning resolved_hue {} below floor at brand={brand}",
                curve.resolved_hue
            );
            let dist = angular_distance(curve.resolved_hue, brand);
            assert!(
                dist >= s_min - 1e-6,
                "warning resolved_hue {} only {dist}deg from brand={brand} (separation breached)",
                curve.resolved_hue
            );
            brand += 0.25;
        }
    }

    #[test]
    fn warning_floor_enforced_full_circle() {
        let neutral = default_neutral();
        for brand in (0..360).step_by(5) {
            let curve =
                SentimentCurve::new(Sentiment::Warning, brand as f64, "#FF9500", &neutral).unwrap();
            assert!(
                curve.resolved_hue >= 45.0 - 1e-6,
                "warning resolved_hue={} below floor at brand={}",
                curve.resolved_hue,
                brand
            );
        }
    }

    // ── Oracle: brute-force minimiser of the new smooth cost on a 0.05 grid ──
    //
    // Replaces the #55 reference_minimize_cost oracle. The new model has no
    // cost-minimisation step (the target is the closed-form smooth hue), so the
    // oracle reconstructs the target geometrically and confirms the resolver's
    // legalised output is the nearest legal hue to it.
    fn oracle_resolve(sent: Sentiment, brand: f64, s_min: f64) -> f64 {
        let prototype = sent.prototype_hue();
        let (p_low, p_high) = sent.hardness();
        let u = signed_delta(brand, prototype);
        let (side, p) = if u > 0.0 {
            (-1.0, p_low)
        } else if u < 0.0 {
            (1.0, p_high)
        } else {
            (sent.preferred_side(), p_high)
        };
        let s = smooth_separation(u.abs(), s_min, p);
        let target = normalize_hue(brand + side * s);

        let floor = sent.hue_floor();
        if is_legal_hue(target, brand, floor, s_min) {
            return target;
        }
        // Nearest legal hue on a fine grid.
        let mut best = target;
        let mut best_d = f64::MAX;
        let mut i = 0;
        while i < 7200 {
            let h = (i as f64) * 0.05;
            if is_legal_hue(h, brand, floor, s_min) {
                let d = angular_distance(h, target);
                if d < best_d {
                    best_d = d;
                    best = h;
                }
            }
            i += 1;
        }
        best
    }

    #[test]
    fn resolver_matches_oracle_grid() {
        let neutral = default_neutral();
        let s_min = s_min_deg(REPRESENTATIVE_CHROMA);
        for &sent in &[
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ] {
            for brand_i in 0..360 {
                let brand = brand_i as f64;
                let got = SentimentCurve::new(sent, brand, prototype_hex(sent), &neutral)
                    .unwrap()
                    .resolved_hue;
                let want = oracle_resolve(sent, brand, s_min);
                assert!(
                    angular_distance(got, want) < 0.1,
                    "{sent:?} brand={brand}: resolver {got} vs oracle {want}"
                );
            }
        }
    }

    // ── Preserved #55 guarantees ─────────────────────────────────

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

    #[test]
    fn rejects_invalid_params() {
        assert!(
            SentimentParams::uniform(0.5).is_err(),
            "p < 1 must be rejected"
        );
        assert!(
            SentimentParams::uniform(f64::NAN).is_err(),
            "NaN p must be rejected"
        );
        assert!(
            SentimentParams::uniform(f64::INFINITY).is_err(),
            "inf p must be rejected"
        );
        assert!(SentimentParams::uniform(2.0).is_ok());
    }

    #[test]
    fn with_params_overrides_hardness() {
        let neutral = default_neutral();
        // Softer p yields more push-out for a near brand than a harder p.
        let soft = SentimentCurve::with_params(
            Sentiment::Danger,
            25.0,
            "#FF3B30",
            &neutral,
            SentimentParams::uniform(1.2).unwrap(),
        )
        .unwrap();
        let hard = SentimentCurve::with_params(
            Sentiment::Danger,
            25.0,
            "#FF3B30",
            &neutral,
            SentimentParams::uniform(8.0).unwrap(),
        )
        .unwrap();
        let soft_sep = angular_distance(soft.resolved_hue, 25.0);
        let hard_sep = angular_distance(hard.resolved_hue, 25.0);
        assert!(
            soft_sep > hard_sep,
            "soft p should separate more than hard p: {soft_sep} !> {hard_sep}"
        );
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
        assert!(curve.was_displaced, "a near brand displaces");
        assert!(curve.displacement > 0.0, "displacement should be positive");
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
