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

/// Warning's categorical Oklab-hue floor in degrees: below this, amber reads as
/// Danger red. Owner precedent (the pre-#65 `Sentiment::hue_floor` value); see
/// [`Sentiment::hue_floor`] for why it is enforced as a conjunctive legality
/// veto and why this value is PROVISIONAL. Calibrated to the old Warning peak
/// (~67°); with the current field peak (~62.57°) it still leaves ~17.6° of
/// headroom above the floor.
const WARNING_HUE_FLOOR_DEG: f64 = 45.0;

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
    ///   orange/red (lower) — its reddish extreme. The wing shapes the slope;
    ///   the categorical 45° floor ([`hue_floor`](Self::hue_floor)) is a separate,
    ///   conjunctive legality veto that complements (does not replace) it.
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

    /// Categorical **Oklab-hue floor** (degrees) below which this sentiment's
    /// colour stops reading as itself and bleeds into a neighbour. Only
    /// [`Sentiment::Warning`] has one: below ~45° its amber slides into Danger's
    /// red, so Warning and Danger become indistinguishable (the #65 regression —
    /// measured Warning↔Danger gap collapsed to 3.46° at an orange brand).
    ///
    /// This is the field-model port of the pre-#65 hard floor (deleted when the
    /// asymmetric membership field replaced the side/hardness machinery). It is
    /// reintroduced here as a **conjunctive legality constraint** inside the field
    /// resolver — a hard veto on sub-floor boundary picks, never a soft preference
    /// folded into the wings. `None` for Danger/Success/Info leaves their
    /// resolution byte-identical to the field-only model.
    ///
    /// The value is **PROVISIONAL** (like the wing sigmas) — revisit if the owner
    /// moves the Warning anchor. The simple [`normalize_hue`] predicate used to
    /// enforce it (see [`resolve_field_hue`]) is correct *only because* Warning's
    /// encroachment branch is unreachable outside brand ∈ [39.14, 86.00], a range
    /// over which no boundary candidate wraps across 0/360. A future sentiment
    /// gaining both a floor and a peak near 0/360 would need a signed-delta
    /// predicate instead.
    fn hue_floor(self) -> Option<f64> {
        match self {
            Sentiment::Warning => Some(WARNING_HUE_FLOOR_DEG),
            _ => None,
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

    /// Resolve a sentiment curve. The `params` are currently inert — the resolver
    /// is a **field maximiser**, not the old p-norm displacement: it picks the hue
    /// that maximises the category's membership field subject to a conjunctive
    /// floor+separation legality filter (see [`resolve_field_hue`]). `params` is
    /// retained in the signature for API stability and a future calibration pass
    /// that re-introduces per-side boundary smoothing.
    ///
    /// There is **no on/off threshold**: a distant brand keeps the hue at the
    /// field peak (zero displacement), an encroaching brand moves it to the nearer
    /// legal separation boundary on the higher-membership side, and the only
    /// discontinuity is the single intrinsic seam where membership dominance
    /// crosses between the two boundaries.
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
///   closer red wing — the category's own asymmetry decides the side.
///
/// On top of that membership argmax sits a **conjunctive legality filter**: any
/// boundary candidate whose normalized hue falls below the sentiment's
/// [`hue_floor`](Sentiment::hue_floor) is vetoed (its membership treated as
/// `-∞`) and excluded from the pick. This holds Warning on its amber/green wing
/// (≥ 45°) instead of letting the encroachment branch dive into Danger's red,
/// which is what collapsed the Warning↔Danger gap in #65. The chosen boundary is
/// always exactly `brand ± s_min`, so the separation invariant is preserved *by
/// construction* — the red boundary is excluded, never lifted toward the brand.
/// For Danger/Success/Info the floor is `None`, so this is byte-identical to the
/// field-only model.
///
/// [`legalize_hue`] is the final conjoined floor+separation net (it never needs
/// to move the legal boundary picks, which are exactly `s_min` away and already
/// clear the floor, but guards float error and fails loudly if the conjoined
/// legal arc is geometrically empty).
fn resolve_field_hue(sentiment: Sentiment, brand_hue: f64, s_min: f64) -> Result<f64, String> {
    let field = sentiment.field();
    let floor = sentiment.hue_floor();
    let floor_ok = |h: f64| floor.is_none_or(|f| normalize_hue(h) >= f - 1e-9);

    // Peak feasible → sit at the prototype (a distant brand barely perturbs it).
    // Warning's peak (~62.57°) clears its 45° floor trivially, so this is
    // unchanged for every sentiment; the floor clause only bites the encroachment
    // branch below.
    if angular_distance(field.peak, brand_hue) >= s_min - 1e-9 && floor_ok(field.peak) {
        return legalize_hue(field.peak, brand_hue, s_min, floor);
    }

    // Brand encroaches: the unimodal field's constrained maximum is the nearer
    // separation boundary on the higher-membership side — but a sub-floor
    // boundary is vetoed (membership → -∞) so it can never win the argmax.
    let c_hi = normalize_hue(brand_hue + s_min);
    let c_lo = normalize_hue(brand_hue - s_min);
    let mu_hi = if floor_ok(c_hi) {
        field.membership(c_hi)
    } else {
        f64::NEG_INFINITY
    };
    let mu_lo = if floor_ok(c_lo) {
        field.membership(c_lo)
    } else {
        f64::NEG_INFINITY
    };
    let pick = if mu_hi >= mu_lo { c_hi } else { c_lo };
    legalize_hue(pick, brand_hue, s_min, floor)
}

/// Snap a candidate hue to the nearest hue that is **both** at least `s_min`
/// from the brand **and** at or above the sentiment's `floor` (when it has one).
///
/// The field resolver already returns conjoined-legal hues, so this is a thin
/// final net against float error: a legal candidate returns unchanged; otherwise
/// it scans outward to the closest hue satisfying *both* constraints. It returns
/// `Err` only when the conjoined floor+separation arc is geometrically empty — a
/// principled, loud failure, never a silent breach of either invariant.
fn legalize_hue(
    candidate: f64,
    brand_hue: f64,
    s_min: f64,
    floor: Option<f64>,
) -> Result<f64, String> {
    if is_legal_hue(candidate, brand_hue, s_min, floor) {
        return Ok(normalize_hue(candidate));
    }

    let mut step = 0.05_f64;
    while step <= 360.0 {
        for cand in [
            normalize_hue(candidate + step),
            normalize_hue(candidate - step),
        ] {
            if is_legal_hue(cand, brand_hue, s_min, floor) {
                return Ok(cand);
            }
        }
        step += 0.05;
    }

    Err(format!(
        "no legal hue exists for brand={brand_hue}, s_min={s_min}, floor={floor:?}: \
         the floor and the separation invariant leave no room on the hue circle"
    ))
}

/// A hue is legal if it clears the brand zone (`>= s_min` away) AND, when the
/// sentiment has a floor, sits at or above it.
fn is_legal_hue(h: f64, brand_hue: f64, s_min: f64, floor: Option<f64>) -> bool {
    angular_distance(h, brand_hue) >= s_min - 1e-9
        && floor.is_none_or(|f| normalize_hue(h) >= f - 1e-9)
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

    /// s_min (deg) a sentiment enforces, derived from its anchor's own Oklab
    /// chroma — the same quantity [`with_params`] computes internally.
    fn s_min_of(s: Sentiment) -> f64 {
        let lab = srgb_linear_to_oklab(srgb_from_hex(s.anchor_hex()).unwrap());
        let chroma = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
        s_min_deg(chroma)
    }

    /// THE regression pin for #65: Warning and Danger must stay perceptually
    /// distinguishable for **every** brand on the full circle. On the un-fixed
    /// (floor-less) resolver this collapses to ~3.46° at an orange brand (~55.5°),
    /// where the encroachment branch dives Warning down to ~32° red onto Danger's
    /// ~28.7° peak. The 45° floor holds Warning on its amber wing; the measured
    /// floor of the gap after the fix is ~15.57°, so a 10° threshold is a
    /// conservative pin that fails loudly on a future collapse.
    #[test]
    fn warning_and_danger_stay_distinguishable_full_circle() {
        let n = neutral();
        for brand in (0..3600).map(|d| d as f64 / 10.0) {
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n).unwrap();
            let d = SentimentCurve::new(Sentiment::Danger, brand, "#FF3B30", &n).unwrap();
            let gap = angular_distance(w.resolved_hue, d.resolved_hue);
            assert!(
                gap >= 10.0 - 1e-6,
                "brand {brand}: Warning {:.3} and Danger {:.3} only {gap:.3}° apart (< 10°)",
                w.resolved_hue,
                d.resolved_hue
            );
        }
    }

    /// Generalises the contract: no two of the four sentiments may collapse onto
    /// each other at any brand. Guards against a future sigma/field/floor tweak
    /// silently merging any pair (measured global all-pairs min ≈ 15.58°).
    #[test]
    fn all_pairs_distinguishable_full_circle() {
        let n = neutral();
        for brand in (0..720).map(|d| d as f64 / 2.0) {
            let resolved: Vec<f64> = Sentiment::ALL
                .iter()
                .map(|&s| {
                    SentimentCurve::new(s, brand, s.anchor_hex(), &n)
                        .unwrap()
                        .resolved_hue
                })
                .collect();
            for i in 0..resolved.len() {
                for j in (i + 1)..resolved.len() {
                    let gap = angular_distance(resolved[i], resolved[j]);
                    assert!(
                        gap >= 10.0 - 1e-6,
                        "brand {brand}: {:?} {:.3} and {:?} {:.3} only {gap:.3}° apart",
                        Sentiment::ALL[i],
                        resolved[i],
                        Sentiment::ALL[j],
                        resolved[j]
                    );
                }
            }
        }
    }

    /// The categorical floor holds for every brand: Warning never resolves below
    /// 45°. Re-adds the deleted `warning_floor_enforced_full_circle`, ported to
    /// the field model (measured min ≈ 45.006°).
    #[test]
    fn warning_hue_floor_holds_full_circle() {
        let n = neutral();
        for brand in (0..1440).map(|d| d as f64 / 4.0) {
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n).unwrap();
            assert!(
                w.resolved_hue >= WARNING_HUE_FLOOR_DEG - 1e-6,
                "brand {brand}: Warning resolved {:.4}° dropped below the {WARNING_HUE_FLOOR_DEG}° floor",
                w.resolved_hue
            );
        }
    }

    /// In Warning's encroachment zone (~39..86°) the floor and the separation
    /// invariant compete; prove the conjunction is jointly satisfied — both hold
    /// simultaneously for every brand in the wedge.
    #[test]
    fn warning_floor_and_separation_hold_together() {
        let n = neutral();
        let s_min = s_min_of(Sentiment::Warning);
        let mut brand = 39.0_f64;
        while brand <= 86.0 {
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n).unwrap();
            assert!(
                w.resolved_hue >= WARNING_HUE_FLOOR_DEG - 1e-6,
                "brand {brand}: Warning {:.4}° below floor",
                w.resolved_hue
            );
            let sep = angular_distance(w.resolved_hue, brand);
            assert!(
                sep >= s_min - 1e-6,
                "brand {brand}: Warning separation {sep:.4}° < s_min {s_min:.4}°"
            );
            brand += 0.25;
        }
    }

    /// Continuity contract: the floor must RELOCATE the single intrinsic
    /// membership-dominance seam, not multiply it. Counts brands where the
    /// resolved Warning hue jumps > 5° between adjacent 0.05° steps and asserts
    /// exactly one such seam, of magnitude ≤ 47° (measured: 1 seam, ~46.86°).
    #[test]
    fn warning_resolution_has_a_single_seam() {
        let n = neutral();
        let resolve = |brand: f64| {
            SentimentCurve::new(Sentiment::Warning, brand, "#FF9500", &n)
                .unwrap()
                .resolved_hue
        };
        let mut seams = 0usize;
        let mut max_jump = 0.0_f64;
        let mut prev = resolve(0.0);
        let mut i = 1u32;
        while i <= 7200 {
            let brand = i as f64 * 0.05;
            let cur = resolve(brand);
            let jump = angular_distance(cur, prev);
            if jump > 5.0 {
                seams += 1;
                max_jump = max_jump.max(jump);
            }
            prev = cur;
            i += 1;
        }
        assert_eq!(seams, 1, "expected exactly one Warning seam, found {seams}");
        assert!(
            max_jump <= 47.0,
            "Warning seam magnitude {max_jump:.2}° exceeds the documented ≤ 47°"
        );
    }

    /// Floor=None sentiments are inert under the change. For floor=None the
    /// floor clause is vacuously true, so resolution equals the pure field
    /// maximiser: at a far brand the peak is held untouched. Pinned across a
    /// handful of brands for Danger/Success/Info (none has a floor) — a
    /// regression pin against any accidental coupling of the floor into the
    /// floor=None path.
    #[test]
    fn floored_sentiments_unaffected_when_floor_none() {
        let n = neutral();
        for s in [Sentiment::Danger, Sentiment::Success, Sentiment::Info] {
            assert!(s.hue_floor().is_none(), "{s:?} unexpectedly has a floor");
            let peak = s.field().peak;
            for brand in [0.0, 90.0, 200.0, 300.0] {
                let got = SentimentCurve::new(s, brand, s.anchor_hex(), &n)
                    .unwrap()
                    .resolved_hue;
                assert!(
                    (got - peak).abs() < 1e-9,
                    "{s:?} brand {brand}: floor=None must leave the far-brand peak \
                     untouched (got {got}, peak {peak})"
                );
            }
        }
    }

    /// The principled-failure arm: when the conjoined floor+separation arc is
    /// geometrically empty, [`legalize_hue`] returns `Err` rather than silently
    /// breaching an invariant. Contrived so the whole circle is sub-floor or
    /// inside the brand zone. The normal-brand sweeps never hit this (0 empty
    /// arcs measured), but a future s_min/floor change must fail loudly.
    #[test]
    fn legalize_hue_returns_err_when_arc_empty() {
        // s_min = 180° forbids every hue except the antipode; pin the floor so
        // even the antipode is sub-floor → no legal hue anywhere.
        let err = legalize_hue(10.0, 0.0, 180.0, Some(181.0));
        assert!(
            err.is_err(),
            "an empty conjoined legal arc must return Err, got {err:?}"
        );
    }
}
