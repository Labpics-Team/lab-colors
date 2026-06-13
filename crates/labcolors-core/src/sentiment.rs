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
    /// Ideal hue for this sentiment — the **Oklab hue of its anchor colour**, in
    /// degrees (NOT HSB/HSL).
    ///
    /// The prototype is derived from a culturally-recognised anchor colour's
    /// actual Oklab hue ([`anchor_hex`](Self::anchor_hex)), not a hand-typed
    /// degree: the original hard-coded peaks were inconsistent with the anchors
    /// (Danger `18°` vs the true `28.7°`, Info `240°` vs `257°` — a hue-model
    /// mix-up that pulled Danger toward pink), while Oklab hue differs from HSB by
    /// 12–46° across the wheel, so a typed number is fragile. Deriving it removes
    /// the confusion at the source (the #65 fix, kept).
    fn prototype_hue(self) -> f64 {
        oklab_hue_of(self.anchor_hex())
    }

    /// Per-side asymptote hardness `(p_low, p_high)` — the exponent `p` of the
    /// smooth displacement [`SentimentParams`]. `p_low` governs the side where the
    /// sentiment hue sits *below* the brand (toward 0°), `p_high` the side above
    /// it. A lower `p` yields sooner (pushes out toward `s_min` earlier); a higher
    /// `p` clings to the brand-distance and stays nearer the prototype.
    ///
    /// All four categories use the **symmetric** default. A per-side asymmetry
    /// makes the two sides' far-field overshoot decay at different rates, which
    /// injects a small spurious discontinuity at the prototype's *antipode* — and
    /// Warning's red-avoidance is already handled exactly by its [`hue_floor`], so
    /// no asymmetry is needed. The hook is kept (and `with_params` still tunes it)
    /// for a future per-zone calibration. **PROVISIONAL** (Daniil's eye).
    fn hardness(self) -> (f64, f64) {
        let _ = self;
        (DEFAULT_HARDNESS, DEFAULT_HARDNESS)
    }

    /// Categorical hue floor (Oklab degrees) below which the sentiment loses its
    /// meaning — Warning must never slide into the red region it would otherwise
    /// share with Danger. Applied as a hard legality constraint, never a soft
    /// preference. This is the guarantee #65 dropped (and #66 inherited), whose
    /// loss let Warning resolve ~3.9° from Danger; restored here. **PROVISIONAL**.
    fn hue_floor(self) -> Option<f64> {
        match self {
            Sentiment::Warning => Some(45.0),
            _ => None,
        }
    }

    /// Preferred side for the degenerate `brand == prototype` seam. `+1.0` pushes
    /// the resolved hue up (higher degrees), `-1.0` down. Warning climbs away from
    /// red toward its hard side; the symmetric-hardness categories use it only to
    /// fix the seam direction deterministically.
    fn preferred_side(self) -> f64 {
        match self {
            Sentiment::Warning => 1.0,
            _ => 1.0,
        }
    }

    /// The culturally-recognised anchor colour whose **Oklab hue** is this
    /// sentiment's prototype (Apple HIG system colours — a widely-recognised
    /// reference set). The hue is read off the colour, never typed as degrees, so
    /// it cannot drift between hue models. The anchor's chroma/lightness are *not*
    /// used — the ramp is rebuilt per-hue to its own gamut budget (see
    /// [`target_mp`]).
    fn anchor_hex(self) -> &'static str {
        match self {
            Sentiment::Danger => "#FF3B30",
            Sentiment::Warning => "#FF9500",
            Sentiment::Success => "#34C759",
            Sentiment::Info => "#007AFF",
        }
    }

    /// All four sentiment categories. The warm pair (Danger/Warning) defines the
    /// colourfulness budget the green band is held to (see [`warm_budget`]); the
    /// full set is the property-sweep surface for the tests. Currently consumed
    /// only by tests — `target_mp` reaches the warm pair by name — so it is
    /// test-gated until the brand/sentiment table wiring (issue #59) consumes it.
    #[cfg(test)]
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

/// Default asymptote hardness `p` for a sentiment with no special asymmetry.
/// `p = 2` is the calibration default Daniil picks by eye; `p → ∞` recovers the
/// old hard 20° wall, `p → 1` is the softest (most eager) yield.
pub const DEFAULT_HARDNESS: f64 = 2.0;

/// Fraction of a hue's gamut ceiling the sentiment ramp carries at each lightness
/// — the "strength" knob (PROVISIONAL, Daniil's eye). `< 1` so a sentiment sits
/// just inside its gamut wall rather than on it. Applied per hue to its own
/// ceiling (rich warm/blue) and, for the green band, to the warm budget it is
/// capped against (see [`target_mp`]).
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
    /// Which category this curve is — the colourfulness target reads it so the
    /// Helmholtz–Kohlrausch-bright green band ([`Sentiment::Success`]) can be
    /// capped to the warm anchors' budget while the other hues run to their own
    /// gamut ceiling. See [`target_mp`].
    sentiment: Sentiment,
    /// The neutral curve this sentiment rides — its lightness ladder, viewing
    /// conditions, and chroma helpers drive the per-hue colourfulness ramp.
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
        let (p_low, p_high) = sentiment.hardness();
        let params = SentimentParams::new(p_low, p_high)?;
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

        // Smooth-asymptote displacement around the anchor-derived prototype,
        // with the categorical floor (Warning) as the final legality net. This is
        // the C¹ resolver that keeps the hue continuous in the brand and holds
        // Warning clear of Danger.
        let resolved_hue = resolve_smooth_hue(sentiment, prototype, brand_hue, params, s_min)?;

        let displacement = angular_distance(resolved_hue, prototype);
        // The hue is "displaced" whenever the smooth model moved it off the
        // prototype by a perceptible amount. There is no hard threshold any
        // more, so this is a reporting flag, not a branch.
        let was_displaced = displacement > 1e-6;

        // The ramp itself (lightness ladder + chroma) is built on demand from the
        // neutral curve and the resolved hue at the per-hue [`target_mp`]
        // colourfulness; the caller's `prototype_hex` only informs the perceptual
        // separation floor above, never the chroma.
        Ok(Self {
            resolved_hue,
            was_displaced,
            displacement,
            sentiment,
            neutral: neutral.clone(),
        })
    }

    /// The sentiment colour at ramp position `t ∈ [0, 1]`. Lightness comes from
    /// the neutral curve (the four sentiments share one lightness ladder) and the
    /// chroma is solved to the per-hue [`target_mp`] colourfulness at that
    /// lightness — each hue runs to its own gamut budget, except the
    /// Helmholtz–Kohlrausch-bright green band, which is capped to the warm
    /// anchors' colourfulness so it cannot out-shout the warm sentiments.
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
        let target = target_mp(self.sentiment, l_ok, self.resolved_hue, vc);
        let c = chroma_for_mp(l_ok, self.resolved_hue, target, vc);
        oklab_lc_to_hex(l_ok, c, self.resolved_hue)
    }
}

/// The smooth displaced separation `s(d) = (d^p + s_min^p)^(1/p)`.
///
/// `d` and `s_min` are non-negative angular degrees; `p >= 1`. The result is
/// always `>= max(d, s_min)` and is C¹ in `d` for `p > 1`. As `d → ∞` the
/// displacement `s(d) − d → 0`, so a distant brand barely nudges the hue.
fn smooth_separation(d: f64, s_min: f64, p: f64) -> f64 {
    (d.powf(p) + s_min.powf(p)).powf(1.0 / p)
}

/// Resolve the sentiment hue under the smooth-asymptote model, then pass it
/// through the legality guard (separation + optional floor) as the final stage.
///
/// The prototype is pushed away from the brand, along the side it already sits on
/// relative to the brand, by `s(d)` — a displacement that grows to the perceptual
/// floor `s_min` as the brand lands on the prototype and decays to zero as the
/// brand recedes. Because `s(d)` is C¹ in the brand-distance, the resolved hue is
/// continuous (no side-flip discontinuity), and the categorical [`hue_floor`]
/// (Warning) keeps it out of Danger's red. This is the resolver that fixes the
/// Warning↔Danger collision and the 46° jump the membership-field picker caused.
fn resolve_smooth_hue(
    sentiment: Sentiment,
    prototype: f64,
    brand_hue: f64,
    params: SentimentParams,
    s_min: f64,
) -> Result<f64, String> {
    // Signed shortest delta from prototype to brand. Its sign tells us which side
    // of the brand the prototype sits on; we push the resolved hue out along that
    // same side, away from the brand.
    let u = signed_delta(brand_hue, prototype);
    let d = u.abs();

    let (side, p) = if u > 0.0 {
        // Brand above the prototype → prototype sits below it → low-side hardness.
        (-1.0, params.p_low)
    } else if u < 0.0 {
        (1.0, params.p_high)
    } else {
        // Degenerate seam: brand exactly on the prototype. Pick the preferred side.
        let pref = sentiment.preferred_side();
        let p = if pref >= 0.0 {
            params.p_high
        } else {
            params.p_low
        };
        (pref, p)
    };

    let s = smooth_separation(d, s_min, p);
    let floor = sentiment.hue_floor();

    // The prototype-ward displacement is the natural target (it decays to the
    // prototype as the brand recedes).
    let natural = normalize_hue(brand_hue + side * s);
    if is_legal_hue(natural, brand_hue, floor, s_min) {
        return Ok(natural);
    }

    // The floor blocks the prototype-ward side near the seam (Warning's downward
    // dip would land in Danger's red). Flip to the opposite side so the sentiment
    // climbs *away* from the forbidden zone — never wrap the long way around the
    // circle into it (the bug a blind nearest-legal scan would commit here).
    let flipped = normalize_hue(brand_hue - side * s);
    if is_legal_hue(flipped, brand_hue, floor, s_min) {
        return Ok(flipped);
    }

    // Neither side legal as constructed: the scan net is the last resort.
    legalize_hue(natural, brand_hue, floor, s_min)
}

/// Snap a candidate hue to the nearest hue legal under both the separation
/// invariant (`>= s_min` from the brand) and the optional categorical floor.
///
/// A legal candidate returns unchanged; otherwise scan outward in fine steps and
/// return the closest legal hue, preserving smoothness as much as the constraints
/// allow. If no legal hue exists on the whole circle (the floor and the brand
/// zone leave no room) return an `Err` rather than silently breaching an invariant.
fn legalize_hue(
    candidate: f64,
    brand_hue: f64,
    floor: Option<f64>,
    s_min: f64,
) -> Result<f64, String> {
    if is_legal_hue(candidate, brand_hue, floor, s_min) {
        return Ok(normalize_hue(candidate));
    }

    let mut step = 0.05_f64;
    while step <= 360.0 {
        for cand in [
            normalize_hue(candidate + step),
            normalize_hue(candidate - step),
        ] {
            if is_legal_hue(cand, brand_hue, floor, s_min) {
                return Ok(cand);
            }
        }
        step += 0.05;
    }

    Err(format!(
        "no legal hue exists for brand={brand_hue}, floor={floor:?}, s_min={s_min}: \
         the separation invariant and the floor leave no room on the hue circle"
    ))
}

/// A hue is legal if it clears the brand zone (`>= s_min` away) and, where a floor
/// is set, sits at or above it.
fn is_legal_hue(h: f64, brand_hue: f64, floor: Option<f64>, s_min: f64) -> bool {
    if angular_distance(h, brand_hue) < s_min - 1e-9 {
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

/// The colourfulness (CAM16-UCS `M'`) a sentiment of category `sentiment` and
/// resolved hue `h_ok` is built to at Oklab lightness `l_ok`.
///
/// Each sentiment runs to [`SENTIMENT_SAT_FRACTION`] of **its own** gamut ceiling
/// — so Danger, Warning and Info are as rich as the sRGB gamut allows at that
/// lightness. This is the deliberate reversal of the old equal-`M'` "min-binding"
/// envelope, which capped *every* hue to the single most gamut-pinched one (always
/// a warm hue — red in the light tints, orange in the mids) and so left the whole
/// palette muted and the mid-reds muddy.
///
/// The lone exception is the green band ([`Sentiment::Success`]). The
/// Helmholtz–Kohlrausch effect makes green/chartreuse read **louder** than a warm
/// or blue hue of equal `M'` and lightness, so at its own (very high) green
/// ceiling Success out-shouts the warm sentiments. Green is therefore capped to
/// the **warm anchors' budget** — the lower of the Danger/Warning gamut ceilings
/// at this lightness — so it carries no more colourfulness than the least-colourful
/// warm sentiment at every step, preserving the "green never out-shouts the warm
/// sentiments" invariant while the rest of the palette is freed to its full
/// richness. Warm and blue hues are not H-K-bright and need no such cap.
fn target_mp(sentiment: Sentiment, l_ok: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    let own = max_mp_at(l_ok, h_ok, vc);
    let ceiling = if sentiment == Sentiment::Success {
        own.min(warm_budget(l_ok, vc))
    } else {
        own
    };
    SENTIMENT_SAT_FRACTION * ceiling
}

/// The warm anchors' colourfulness budget at lightness `l_ok`: the lower of the
/// Danger and Warning gamut ceilings. The green band is held to this so it cannot
/// carry more perceived colourfulness than the least-colourful warm sentiment.
fn warm_budget(l_ok: f64, vc: &ViewingConditions) -> f64 {
    max_mp_at(l_ok, Sentiment::Danger.prototype_hue(), vc).min(max_mp_at(
        l_ok,
        Sentiment::Warning.prototype_hue(),
        vc,
    ))
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
    fn prototype_is_the_anchor_oklab_hue() {
        // The prototype is read off the anchor colour's Oklab hue, never typed — so
        // it cannot drift between hue models (the bug that pulled Danger to pink).
        for s in Sentiment::ALL {
            let want = oklab_hue_of(s.anchor_hex());
            assert!(
                (s.prototype_hue() - want).abs() < 1e-9,
                "{s:?}: prototype {} != anchor Oklab hue {want}",
                s.prototype_hue()
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
        // Loudness invariant: Success (green) is capped to the warm anchors'
        // colourfulness budget (the H-K cap in `target_mp`), so it is never more
        // colourful than the warm sentiments at any step — the user's "green too
        // bright" complaint, gone by construction — while still riding the shared
        // neutral lightness ladder. The warm hues themselves now run to their own
        // (richer) gamut ceilings; this only pins green relative to them. Swept
        // across brand hues.
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
        // A brand on the yellow side of green pushes Success toward teal (higher
        // hue), never into yellow-green — the smooth resolver displaces along the
        // side the prototype sits on relative to the brand.
        let n = neutral();
        let peak = Sentiment::Success.prototype_hue();
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

    #[test]
    fn warm_and_blue_run_to_their_own_gamut_ceiling() {
        // Vividness fix: Danger, Warning and Info build to `SENTIMENT_SAT_FRACTION`
        // of their OWN gamut ceiling — not pinned down to the most-pinched hue as
        // the old min-binding did. So each non-green sentiment's rendered M' tracks
        // its own ceiling (within the fraction), which is what un-muddies the reds
        // and the mid oranges. Checked on the mid-tone steps where the gamut has
        // real chroma to give (the near-white/near-black ends pinch shut for all).
        let n = neutral();
        let brand = oklab_hue_of("#F93800");
        for s in [Sentiment::Danger, Sentiment::Warning, Sentiment::Info] {
            let curve = SentimentCurve::new(s, brand, s.anchor_hex(), &n).unwrap();
            let vc = n.vc();
            for i in 3..=7 {
                let t = i as f64 / 9.0;
                let l_ok = jp_to_oklab_l(n.at(t).jp, vc);
                let ceil = max_mp_at(l_ok, curve.resolved_hue, vc);
                let got = mp(&curve.at(t));
                assert!(
                    got >= SENTIMENT_SAT_FRACTION * ceil - 1.0,
                    "{s:?} step {i}: rendered M' {got:.1} far below own ceiling \
                     fraction {:.1} (ceiling {ceil:.1})",
                    SENTIMENT_SAT_FRACTION * ceil
                );
            }
        }
    }

    #[test]
    fn danger_is_richer_than_green_in_the_mids() {
        // The concrete user complaint, pinned: with the fiery brand the mid-tone
        // red must read clearly more colourful than the (H-K-capped) green, not
        // muddied down to it. Sampled at the mid steps where red's gamut ceiling
        // towers over the warm budget green is held to.
        let n = neutral();
        let brand = oklab_hue_of("#F93800");
        let d = SentimentCurve::new(Sentiment::Danger, brand, "#FF3B30", &n).unwrap();
        let s = SentimentCurve::new(Sentiment::Success, brand, "#34C759", &n).unwrap();
        for i in 4..=6 {
            let t = i as f64 / 9.0;
            assert!(
                mp(&d.at(t)) > mp(&s.at(t)) + 2.0,
                "step {i}: danger M' {:.1} should clearly exceed green {:.1}",
                mp(&d.at(t)),
                mp(&s.at(t))
            );
        }
    }

    #[test]
    fn warning_floor_enforced_full_circle() {
        // Restored guard (#65 dropped it, #66 inherited the gap): Warning must
        // never slide below its categorical floor into the red region, for ANY
        // brand on the circle. This is the hard half of the Warning↔Danger fix.
        let n = neutral();
        let mut brand = 0.0;
        while brand < 360.0 {
            let h = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n)
                .unwrap()
                .resolved_hue;
            assert!(
                normalize_hue(h) >= 45.0 - 1e-6,
                "Warning resolved {h:.2}° is below the 45° floor at brand {brand}"
            );
            brand += 0.25;
        }
    }

    #[test]
    fn warning_stays_distinguishable_from_danger_full_circle() {
        // The machine-proven defect this PR fixes: with the membership-field
        // picker Warning could resolve ~3.9° from Danger (perceptually one colour)
        // at brand≈56°. The smooth resolver + floor keep a clear gap everywhere.
        let n = neutral();
        let mut brand = 0.0;
        let mut worst = f64::INFINITY;
        while brand < 360.0 {
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n)
                .unwrap()
                .resolved_hue;
            let d = SentimentCurve::new(Sentiment::Danger, brand, "#FF3B30", &n)
                .unwrap()
                .resolved_hue;
            worst = worst.min(angular_distance(w, d));
            brand += 0.25;
        }
        assert!(
            worst >= 10.0,
            "Warning↔Danger closest approach {worst:.2}° (must stay >= 10° apart)"
        );
    }

    #[test]
    fn resolved_hue_is_smooth_between_its_two_seams() {
        // Continuity guard. A single-valued hue that always clears the brand by
        // `s_min` has exactly TWO topological seams on the circle: the prototype
        // handoff (large, where the sentiment crosses the brand) and the prototype
        // *antipode* (small, where the smooth displacement's far-field overshoot
        // flips side). Both are inherent to the smooth-asymptote model — pre-#65
        // skip-windowed both. So we skip a window around each seam and require the
        // resolved hue to be Lipschitz-smooth everywhere else. This catches any
        // SPURIOUS discontinuity (the membership-field picker's 46° flip lived far
        // from either seam) while accepting the two unavoidable ones. Seam
        // placement is PROVISIONAL (owner's perceptual eye).
        let n = neutral();
        let step = 0.05_f64;
        for s in Sentiment::ALL {
            let mut brand = 0.0;
            let mut prev: Option<f64> = None;
            let mut jumps: Vec<f64> = Vec::new();
            while brand <= 360.0 {
                let h = SentimentCurve::new(s, brand, s.anchor_hex(), &n)
                    .unwrap()
                    .resolved_hue;
                if let Some(p) = prev {
                    jumps.push(angular_distance(h, p));
                }
                prev = Some(h);
                brand += step;
            }
            // Detect seams empirically (their location is floor-shifted, not fixed
            // at the prototype): the largest jump is the handoff, the second is the
            // antipode. Everything else must be Lipschitz-smooth — off-seam the
            // slope is <= ~2, so a 0.05° brand step moves the hue well under 0.5°;
            // a roomy 1.0° bound flags any genuine spurious discontinuity.
            jumps.sort_by(|a, b| b.partial_cmp(a).unwrap());
            assert!(
                jumps[1] <= 5.0,
                "{s:?}: second discontinuity {:.2}° too large (antipode should be small)",
                jumps[1]
            );
            assert!(
                jumps[2] <= 1.0,
                "{s:?}: a THIRD discontinuity of {:.2}° exists — only the handoff and \
                 antipode seams are allowed",
                jumps[2]
            );
        }
    }
}
