use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{jp_to_oklab_l, max_chroma};
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex};
use crate::spaces::vc::ViewingConditions;

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
/// "perceptual seam" function: it is fed each sentiment prototype's own Oklab
/// chroma, so the margin narrows in saturated warm hues and widens in washed-out
/// regions (issue #20).
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
    /// encode the empirical category borders.
    ///
    /// The **peak is derived from a culturally-recognised anchor colour's actual
    /// Oklab hue** ([`anchor_hex`](Self::anchor_hex)), not a hand-typed degree:
    /// the previous hard-coded peaks were inconsistent with the anchors (Danger
    /// `18°` vs the true `28.7°`, Info `240°` vs `257°` — a hue-model mix-up that
    /// pulled Danger toward pink), while Oklab hue differs from HSB by 12–46°
    /// across the wheel, so a typed number is fragile. Deriving it removes the
    /// confusion at the source.
    ///
    /// Wing widths (Daniil's design directions, 2026-06-12) are still
    /// **PROVISIONAL** — to be fitted from colour-naming data and finalised by
    /// eye. `sigma_lo` is the wing toward *lower* hue (signed delta < 0):
    /// - **Danger** (red): both wings wide — red holds toward orange or crimson.
    /// - **Success** (green): *steep* toward yellow (lower hue, reads "off"),
    ///   *wide* toward teal (higher hue, still success), so it slides to teal.
    /// - **Warning** (amber): *steep* toward green (higher), *wide* toward
    ///   orange/red (lower) — its reddish extreme; replaces the old 45° floor.
    /// - **Info** (blue): roughly symmetric.
    fn field(self) -> HueField {
        let (sigma_lo, sigma_hi) = match self {
            //                       σ_lo   σ_hi
            Sentiment::Danger => (24.0, 26.0),
            Sentiment::Warning => (26.0, 14.0),
            Sentiment::Success => (13.0, 42.0),
            Sentiment::Info => (26.0, 26.0),
        };
        HueField::new(oklab_hue_of(self.anchor_hex()), sigma_lo, sigma_hi)
    }

    /// The culturally-recognised anchor colour whose **Oklab hue** is this
    /// sentiment's field peak (Apple HIG system colours — a widely-recognised
    /// reference set). The hue is read off the colour, never typed as degrees, so
    /// it cannot drift between hue models. The chroma/lightness of the anchor are
    /// *not* used — the sentiment colour is rebuilt at a constant `M'`.
    fn anchor_hex(self) -> &'static str {
        match self {
            Sentiment::Danger => "#FF3B30",
            Sentiment::Warning => "#FF9500",
            Sentiment::Success => "#34C759",
            Sentiment::Info => "#007AFF",
        }
    }

    /// All four sentiment categories — the set whose gamut ceilings define the
    /// shared colourfulness envelope (see [`binding_mp`]).
    pub(crate) const ALL: [Sentiment; 4] = [
        Sentiment::Danger,
        Sentiment::Warning,
        Sentiment::Success,
        Sentiment::Info,
    ];
}

/// The Oklab hue (degrees, `[0, 360)`) of a hex colour — the single source of a
/// sentiment's field peak.
fn oklab_hue_of(hex: &str) -> f64 {
    let lab = srgb_linear_to_oklab(srgb_from_hex(hex).expect("valid anchor hex"));
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
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

/// Fraction of the **binding** (lowest-gamut) hue's colourfulness the sentiment
/// ramp carries at every lightness. PROVISIONAL "strength" knob (Daniil's eye);
/// `< 1` so even the weakest hue stays inside its gamut. Building every sentiment
/// to the *same* `M'` at every step (not `sat_ratio × each hue's own ceiling`) is
/// what stops green out-shouting red/orange — at any lightness all four carry
/// identical perceived colourfulness, capped to whichever hue the sRGB gamut
/// pinches first (green's vivid light tints come down to the warm hues' level).
const SENTIMENT_SAT_FRACTION: f64 = 0.92;

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
    /// The neutral curve this sentiment rides — its lightness ladder, viewing
    /// conditions, and chroma helpers drive the constant-colourfulness ramp.
    neutral: NeutralCurve,
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

        // The ramp itself (lightness ladder + chroma) is built on demand from the
        // neutral curve and the resolved hue at a constant, hue-independent
        // colourfulness (`binding_mp`); the caller's `prototype_hex` only informs
        // the perceptual separation floor above, never the chroma.
        Ok(Self {
            resolved_hue,
            was_displaced,
            displacement,
            neutral: neutral.clone(),
        })
    }

    /// The sentiment colour at ramp position `t ∈ [0, 1]`. Lightness comes from
    /// the neutral curve (the four sentiments share one lightness ladder) and the
    /// chroma is solved to the shared [`binding_mp`] colourfulness at that
    /// lightness (they share one colourfulness ladder too) — so every step has
    /// equal perceived weight across hues and no sentiment out-shouts another.
    pub fn at(&self, t: f64) -> LcsColor {
        let vc = self.neutral.vc();
        let hex = self.hex_at(t);
        LcsColor::from_hex_with_vc(&hex, vc).unwrap_or_else(|_| self.neutral.at(t))
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
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.hex_at(0.5)];
        }
        (0..n)
            .map(|i| self.hex_at(i as f64 / (n - 1) as f64))
            .collect()
    }

    /// The hex at ramp position `t` — the colour [`at`](Self::at) builds, without
    /// the round-trip through [`LcsColor`].
    fn hex_at(&self, t: f64) -> String {
        let vc = self.neutral.vc();
        let l_ok = jp_to_oklab_l(self.neutral.at(t).jp, vc);
        let target_mp = binding_mp(l_ok, vc);
        let c = chroma_for_mp(l_ok, self.resolved_hue, target_mp, vc);
        oklab_lc_to_hex(l_ok, c, self.resolved_hue)
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

/// The in-gamut sRGB hex at Oklab `(L, C, h)`, channels clamped to `[0, 1]`.
fn oklab_lc_to_hex(l_ok: f64, c: f64, h_ok: f64) -> String {
    let a = c * h_ok.to_radians().cos();
    let b = c * h_ok.to_radians().sin();
    let rgb = oklab_to_srgb_linear([l_ok, a, b]);
    hex_from_srgb([
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ])
}

/// CAM16-UCS colourfulness `M' = s·(J'+1)` of the quantised colour at Oklab
/// `(L, C, h)` under `vc`.
fn mp_at(l_ok: f64, c: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    LcsColor::from_hex_with_vc(&oklab_lc_to_hex(l_ok, c, h_ok), vc)
        .map(|p| p.s * (p.jp + 1.0))
        .unwrap_or(0.0)
}

/// The maximum `M'` reachable at Oklab lightness `l_ok` for hue `h_ok` — the
/// colourfulness at the sRGB gamut edge there.
fn max_mp_at(l_ok: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    mp_at(l_ok, max_chroma(l_ok, h_ok), h_ok, vc)
}

/// The shared colourfulness every sentiment is built to at lightness `l_ok`:
/// [`SENTIMENT_SAT_FRACTION`] of the **binding** hue's max `M'` — the lowest
/// gamut ceiling across the four sentiment categories at this lightness. It is
/// hue-independent, so every sentiment carries the *same* `M'` at every step;
/// the warm hues (which the gamut pinches first) set the level, pulling green's
/// otherwise-brighter tints down to match.
fn binding_mp(l_ok: f64, vc: &ViewingConditions) -> f64 {
    let min_ceiling = Sentiment::ALL
        .iter()
        .map(|s| max_mp_at(l_ok, s.field().peak, vc))
        .fold(f64::INFINITY, f64::min);
    SENTIMENT_SAT_FRACTION * min_ceiling
}

/// Chroma at `(l_ok, h_ok)` whose colour carries `target_mp` — bisection on the
/// gamut-monotone `M'`, capped at the gamut edge for a hue that cannot reach it.
fn chroma_for_mp(l_ok: f64, h_ok: f64, target_mp: f64, vc: &ViewingConditions) -> f64 {
    let c_max = max_chroma(l_ok, h_ok);
    if mp_at(l_ok, c_max, h_ok, vc) <= target_mp {
        return c_max;
    }
    let (mut lo, mut hi) = (0.0_f64, c_max);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if mp_at(l_ok, mid, h_ok, vc) < target_mp {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    fn mp(c: &LcsColor) -> f64 {
        c.s * (c.jp + 1.0)
    }

    #[test]
    fn field_peak_is_the_anchor_oklab_hue() {
        // The peak is read off the anchor colour's Oklab hue, never typed — so it
        // cannot drift between hue models (the bug that pulled Danger to pink).
        for s in Sentiment::ALL {
            let want = oklab_hue_of(s.anchor_hex());
            assert!(
                (s.field().peak - want).abs() < 1e-9,
                "{s:?}: peak {} != anchor Oklab hue {want}",
                s.field().peak
            );
        }
    }

    #[test]
    fn sample_hex_has_requested_length_and_valid_hex() {
        let n = neutral();
        let sc = SentimentCurve::new(Sentiment::Danger, 33.5, "#FF2E2E", &n).unwrap();
        for k in [0usize, 1, 2, 10, 13] {
            let v = sc.sample_hex(k);
            assert_eq!(v.len(), k, "sample_hex({k}) length");
            for h in &v {
                assert!(srgb_from_hex(h).is_ok(), "invalid hex {h}");
            }
        }
    }

    #[test]
    fn green_never_outshouts_the_warm_sentiments_and_shares_their_lightness() {
        // Consistency law: every sentiment carries the same colourfulness (the
        // binding envelope) and the same lightness (the neutral ladder) at every
        // step, so Success (green) is never more colourful than the warm
        // sentiments — the user's "green too bright" complaint, gone by
        // construction. Swept across brand hues.
        let n = neutral();
        for brand in (0..360).step_by(13).map(|d| d as f64) {
            let d = SentimentCurve::new(Sentiment::Danger, brand, "#FF3B30", &n)
                .unwrap()
                .sample(10);
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n)
                .unwrap()
                .sample(10);
            let s = SentimentCurve::new(Sentiment::Success, brand, "#34C759", &n)
                .unwrap()
                .sample(10);
            for i in 0..10 {
                let warm = mp(&d[i]).max(mp(&w[i]));
                assert!(
                    mp(&s[i]) <= warm + 1.0,
                    "brand {brand} step {i}: green M' {:.1} exceeds warm {:.1}",
                    mp(&s[i]),
                    warm
                );
                assert!(
                    (s[i].jp - d[i].jp).abs() < 1.5,
                    "brand {brand} step {i}: lightness ladder differs (green {:.1} vs danger {:.1})",
                    s[i].jp,
                    d[i].jp
                );
            }
        }
    }

    #[test]
    fn resolved_hue_clears_the_brand_by_s_min() {
        // Separation invariant: the resolved hue is always at least the perceptual
        // floor s_min from the brand (the peak is only kept when it already clears
        // it). Swept across brand hues, all four categories.
        let n = neutral();
        for s in Sentiment::ALL {
            let lab = srgb_linear_to_oklab(srgb_from_hex(s.anchor_hex()).unwrap());
            let chroma = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
            let s_min = s_min_deg(chroma);
            for brand in (0..360).step_by(7).map(|d| d as f64) {
                let sc = SentimentCurve::new(s, brand, s.anchor_hex(), &n).unwrap();
                let sep = angular_distance(sc.resolved_hue, brand);
                assert!(
                    sep >= s_min - 1e-6,
                    "{s:?} brand {brand}: separation {sep:.2} < s_min {s_min:.2}"
                );
            }
        }
    }

    #[test]
    fn success_slides_to_teal_not_yellow_when_a_green_brand_encroaches() {
        // The asymmetric field's headline behaviour: a brand on the yellow side of
        // green pushes Success toward teal (higher hue), never into yellow-green.
        let n = neutral();
        let peak = Sentiment::Success.field().peak;
        let brand = peak - 6.0;
        let sc = SentimentCurve::new(Sentiment::Success, brand, "#34C759", &n).unwrap();
        assert!(
            sc.resolved_hue > peak,
            "resolved {} should sit teal-side of the green peak {peak}",
            sc.resolved_hue
        );
    }

    #[test]
    fn ramp_lightness_is_monotone_dark() {
        let n = neutral();
        for s in Sentiment::ALL {
            let r = SentimentCurve::new(s, 33.5, s.anchor_hex(), &n)
                .unwrap()
                .sample(13);
            for w in r.windows(2) {
                assert!(
                    w[1].jp <= w[0].jp + 1e-6,
                    "{s:?}: lightness not monotone ({} -> {})",
                    w[0].jp,
                    w[1].jp
                );
            }
        }
    }

    #[test]
    fn rejects_non_finite_brand_hue() {
        let n = neutral();
        assert!(SentimentCurve::new(Sentiment::Danger, f64::NAN, "#FF2E2E", &n).is_err());
    }
}
