// oklab_hue_of: единая реализация формулы якорного оттенка живёт в палитре
// акцентов — сентименты потребляют её, не держат вторую копию физики.
use crate::accent::{Accent, oklab_hue_of};
use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{jp_to_oklab_l, max_chroma};
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

/// Перцептивный минимум разделения между оттенком сентимента и брендовым
/// оттенком, выраженный как **длина хорды в плоскости Oklab a/b** (не в
/// градусах).
///
/// # Почему хорда, а не угол
///
/// Issue #20: одинаковые угловые сдвиги хроматически не равноценны — поворот
/// на 20° при низкой хроме почти незаметен, тогда как при высокой хроме это
/// очевидная смена цвета. Фиксированный угловой порог поэтому создаёт
/// избыточное разделение в десатурированных зонах и недостаточное в
/// насыщенных. Перцептивно честный инвариант — постоянное *расстояние* в
/// плоскости (a, b), которое затем переводится в зонный угол.
///
/// # Деривация
///
/// ```text
/// S_PERC_MIN = 2 · C_rep_figma · sin(20° / 2)
/// ```
///
/// где:
/// - **C_rep_figma = 0.1978** — среднеарифметическое Oklab-хромы четырёх
///   якорей, взятых из Figma CONTENTS (коллекция «🔵 4.1 Primitives»,
///   Light-mode, обход переменных через figma-console, 2026-06-30):
///
///   | сентимент | hex Figma  | Oklab C |
///   |-----------|-----------|---------|
///   | Danger    | `#FF3B30` | 0.2321  |
///   | Warning   | `#FFA100` | 0.1717  |
///   | Success   | `#34C759` | 0.1944  |
///   | Info      | `#3E87FF` | 0.1931  |
///
///   `C_rep_figma = (0.2321 + 0.1717 + 0.1944 + 0.1931) / 4 = 0.1978`
///
/// - **20°** — нижний предел категориального восприятия оттенка по
///   Witzel & Gegenfurtner (2013), JOSA A 30(7):1501, Table 1: средняя
///   граница категорий ≈ 18–22° Oklab-hue при типичной насыщенности.
///   Значение 20° — нижний предел этого диапазона (консервативный выбор
///   для разделения семантических категорий).
///
/// Итог: `2 × 0.1978 × sin(10°) ≈ 0.068_703_9`
// Выведено: 2 × C_rep_figma × sin(20°/2); C_rep_figma из Figma CONTENTS 2026-06-30;
// 20° — категориальный порог по Witzel & Gegenfurtner (2013) JOSA A 30(7):1501.
const S_PERC_MIN: f64 = 0.068_703_9;

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
    /// as an open extension seam; no category currently needs it.
    fn hardness(self) -> (f64, f64) {
        let _ = self;
        (DEFAULT_HARDNESS, DEFAULT_HARDNESS)
    }

    /// Categorical hue floor (Oklab degrees) below which the sentiment loses its
    /// meaning — Warning must never slide into the red region it would otherwise
    /// share with Danger. Applied as a hard legality constraint, never a soft
    /// preference. This is the guarantee #65 dropped (and #66 inherited), whose
    /// loss let Warning resolve ~3.9° from Danger; restored here.
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

    /// Семейство палитры акцентов, чей якорь несёт этот сентимент. Сентимент —
    /// это *семантическая роль* цвета (Danger, Warning, …), а его прототипный
    /// оттенок — это фиксированное семейство палитры ([`Accent`]). Отображение
    /// заземлено в Figma (`Labels/Danger/Primary` → `Accent/Red` и т.д.,
    /// коллекция «🔵 4.1 Primitives», Light-mode):
    ///
    /// | сентимент | семейство       | Figma-переменная  |
    /// |-----------|-----------------|-------------------|
    /// | Danger    | [`Accent::Red`]    | `Accent/Red`    |
    /// | Warning   | [`Accent::Orange`] | `Accent/Orange` |
    /// | Success   | [`Accent::Green`]  | `Accent/Green`  |
    /// | Info      | [`Accent::Blue`]   | `Accent/Blue`   |
    pub fn accent(self) -> Accent {
        match self {
            Sentiment::Danger => Accent::Red,
            Sentiment::Warning => Accent::Orange,
            Sentiment::Success => Accent::Green,
            Sentiment::Info => Accent::Blue,
        }
    }

    /// Якорный цвет сентимента, чей **Oklab-оттенок** используется как прототип.
    ///
    /// SSOT якорного hex — палитра акцентов ([`Accent::anchor_hex`]); сентимент
    /// лишь ссылается на своё семейство ([`accent`](Self::accent)), а не хранит
    /// собственную копию значения. Это устраняет дублирование hex между модулями
    /// (задача «акценты как данные, не 10 копий кода»): при изменении якоря в
    /// Figma правится ОДНА строка в `accent.rs`.
    ///
    /// Только Oklab-оттенок используется как прототип; хрома и светлота якоря
    /// не применяются — рампа строится из общей perceived-lightness лестницы на
    /// фиксированной доле граничной хромы гамута (см. [`SentimentCurve::at`]).
    fn anchor_hex(self) -> &'static str {
        self.accent().anchor_hex()
    }

    /// All four sentiment categories — the property-sweep surface for the tests.
    /// Currently consumed only by tests, so it is test-gated until the
    /// brand/sentiment table wiring (issue #59) consumes it.
    #[cfg(test)]
    pub(crate) const ALL: [Sentiment; 4] = [
        Sentiment::Danger,
        Sentiment::Warning,
        Sentiment::Success,
        Sentiment::Info,
    ];
}

/// Default asymptote hardness `p` for a sentiment with no special asymmetry.
/// `p = 5` is the calibration default (Sticky Potential Well); `p → ∞` recovers
/// the old hard 20° wall, `p → 1` is the softest (most eager) yield.
// SSOT-TRACKED — p-norm hardness default (#55).
pub const DEFAULT_HARDNESS: f64 = 5.0;

/// Fraction of the in-gamut maximum chroma every sentiment colour carries at its
/// perceived-lightness-matched point — the single "strength" knob. `< 1` so a
/// sentiment sits just inside its gamut wall rather than on it (the edge can
/// read neon). Applied identically to every hue: there is no per-hue cap. See
/// [`SentimentCurve::hex_at`].
// SSOT-TRACKED — gamut-fraction chroma strength knob.
const CHROMA_FRACTION: f64 = 0.88;

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
    /// The neutral curve this sentiment rides — its lightness ladder and viewing
    /// conditions drive the shared perceived-lightness ramp.
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
    ///   below the hue floor. When the floor blocks the prototype-ward
    ///   displacement, the resolver flips to the opposite (preferred) side —
    ///   keeping the hue in amber/yellow rather than wrapping the long way round
    ///   into red — and only if neither side is legal does it scan outward to the
    ///   nearest legal hue. If the legal arc is geometrically empty it returns an
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

        // The ramp itself is built on demand from the neutral curve's perceived
        // lightness and the resolved hue (one chroma law for every hue); the
        // caller's `prototype_hex` only informs the perceptual separation floor
        // above, never the chroma.
        Ok(Self {
            resolved_hue,
            was_displaced,
            displacement,
            neutral: neutral.clone(),
        })
    }

    /// The sentiment colour at ramp position `t ∈ [0, 1]`. The four sentiments
    /// share one **perceived-lightness** (`j_hk`) ladder — the neutral grey's
    /// H-K lightness — and each hue is placed at that perceived lightness at a
    /// fixed fraction of the in-gamut maximum chroma. Equal `j_hk` means equal
    /// perceived brightness *and* equal contrast at every step (none out-shouts);
    /// max chroma means none is dull. One rule for every hue, no per-hue cap —
    /// the green "cap" of the old model falls out of the maths (a saturated green
    /// must sit at a lower base lightness to land on the same `j_hk`).
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
        let h = self.resolved_hue;
        // The shared ladder is the neutral grey's *perceived* (H-K) lightness at
        // `t` — a grey has no chroma, so its `j_hk` is just its CAM16 lightness.
        let l_grey = jp_to_oklab_l(self.neutral.at(t).jp, vc);
        let target_jhk = jhk_at(l_grey, 0.0, h, vc);
        // Place this hue at that perceived lightness, at a fixed fraction of the
        // gamut-edge chroma. Identical rule for every hue; a saturated hue lands
        // at a lower base lightness (its H-K boost is what makes `j_hk` match).
        let l = l_for_jhk(target_jhk, h, vc);
        let c = CHROMA_FRACTION * max_chroma(l, h);
        oklab_lc_to_hex(l, c, h)
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
    resolve_smooth_hue_explicit(
        sentiment.preferred_side(),
        sentiment.hue_floor(),
        prototype,
        brand_hue,
        params,
        s_min,
    )
}

/// Config-facing sibling of [`resolve_smooth_hue`] that takes the categorical
/// policy (`preferred_side`, `hue_floor`) explicitly instead of reading it off the
/// fixed [`Sentiment`] enum — so an arbitrary consumer sentiment category
/// ([`crate::config::SentimentCategory`]) resolves through the identical smooth
/// p-norm displacement + legality guard, no second copy of the physics.
///
/// `prototype`, `brand_hue` and the result are **Oklab hue degrees**. See
/// [`resolve_smooth_hue`] / [`SentimentCurve::with_params`] for the model.
///
/// # Errors
///
/// `Err` if no hue satisfies both the floor and the separation invariant
/// (empty legal arc) — never a silent breach.
pub fn resolve_smooth_hue_explicit(
    preferred_side: f64,
    hue_floor: Option<f64>,
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
        let p = if preferred_side >= 0.0 {
            params.p_high
        } else {
            params.p_low
        };
        (preferred_side, p)
    };

    let s = smooth_separation(d, s_min, p);
    let floor = hue_floor;

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

/// Категориальный порог оттенка `S_PERC_MIN` (длина хорды Oklab a/b),
/// пересчитанный из хром сентимент-якорей конфига по закону
/// `2·C_rep·sin(20°/2)`, где `C_rep` — среднее хром (поправка t2 №д).
///
/// `20°` — нижний предел категориального восприятия (Witzel & Gegenfurtner 2013,
/// JOSA A 30(7):1501). При labui-якорях (хромы Red/Orange/Green/Blue) результат
/// совпадает с замороженной константой [`S_PERC_MIN`] (`0.068_703_9`,
/// деривационная идентичность — тестом, допуск 1e-4): формула остаётся законом
/// при произвольных якорях клиента, а сегодняшнее значение — её частный случай.
///
/// Пустой срез хром даёт `0.0` (нет сентиментов — нет порога разделения).
pub fn s_perc_min_from_chromas(chromas: &[f64]) -> f64 {
    if chromas.is_empty() {
        return 0.0;
    }
    let c_rep = chromas.iter().sum::<f64>() / chromas.len() as f64;
    // Хорда длины 2·C·sin(Δh/2) при Δh = 20° — тот же категориальный порог
    // (Witzel & Gegenfurtner 2013), что в деривации [`S_PERC_MIN`]; инлайн
    // (не именованная const), т.к. это derivation-identity вход, не новый
    // POLICY-литерал — provenance держит doc [`S_PERC_MIN`].
    2.0 * c_rep * (20.0_f64.to_radians() / 2.0).sin()
}

/// Замороженное значение `S_PERC_MIN` (для деривационной идентичности теста t2).
/// Возвращается функцией (не `const`), чтобы не заводить второй POLICY-литерал в
/// аудите реестра — это тот же derivation-identity, что [`S_PERC_MIN`].
pub fn s_perc_min_frozen() -> f64 {
    S_PERC_MIN
}

/// Config-facing сентимент-солид: якорь семейства, чей оттенок разведён с брендом
/// сентимент-солвером, при СОХРАНЁННЫХ светлоте и хроме якоря.
///
/// Тинт лестницы сентимента (поправка t2 №г): берётся оттенок семейства,
/// смещённый от бренда через [`resolve_smooth_hue_explicit`] (тот же C¹-солвер,
/// что у [`SentimentCurve`]), но светлота/хрома — исходного якоря. Когда
/// смещение не нужно (`resolved_hue == prototype`, случай labui-бренда), солид
/// воспроизводит СЫРОЙ якорь семейства — это и есть деривационная идентичность,
/// которую фиксирует тест. `brand_hue` — Oklab-оттенок бренда (градусы).
///
/// # Errors
///
/// `Err`, если якорь невалиден или легальный оттенок геометрически пуст
/// (см. [`resolve_smooth_hue_explicit`]).
pub fn resolve_config_sentiment_solid(
    family_anchor_hex: &str,
    brand_hue: f64,
    hardness: f64,
    chroma_fraction: f64,
    hue_floor: Option<f64>,
    preferred_side: f64,
    s_perc_min: f64,
) -> Result<String, String> {
    let _ = chroma_fraction; // хрома тинта = хрома якоря (сохраняем солид якоря);
    // chroma_fraction — ручка рампы SentimentCurve, не тинта; принимается для
    // единообразия сигнатуры конфига, но тинт держит фактическую хрому якоря.
    let anchor_lab = srgb_linear_to_oklab(srgb_from_hex(family_anchor_hex)?);
    let prototype = oklab_hue_of(family_anchor_hex);
    let l_anchor = anchor_lab[0];
    let c_anchor = (anchor_lab[1].powi(2) + anchor_lab[2].powi(2)).sqrt();
    let s_min = s_min_deg(c_anchor);
    // Порог разделения — max из перцептивного (от хромы якоря) и конфиг-порога:
    // конфиг S_PERC_MIN задаёт минимум для КАТЕГОРИИ, s_min_deg — для этой хромы.
    let params = SentimentParams::uniform(hardness)?;
    let effective_s_min = s_min.max(s_min_deg_from_chord(s_perc_min, c_anchor));
    let resolved_hue = resolve_smooth_hue_explicit(
        preferred_side,
        hue_floor,
        prototype,
        brand_hue,
        params,
        effective_s_min,
    )?;
    // Солид на исходных L/C якоря, смещённый оттенок.
    Ok(oklab_lc_to_hex(l_anchor, c_anchor, resolved_hue))
}

/// Перевести целевую хорду разделения `chord` в угол оттенка (градусы) при
/// хроме `zone_chroma` — та же инверсия `2·C·sin(Δh/2)`, что [`s_min_deg`], но с
/// произвольной хордой (для конфиг-`S_PERC_MIN`).
fn s_min_deg_from_chord(chord: f64, zone_chroma: f64) -> f64 {
    let safe_chroma = zone_chroma.max(1e-6);
    let ratio = (chord / (2.0 * safe_chroma)).clamp(0.0, 1.0);
    2.0 * ratio.asin().to_degrees()
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

/// The Helmholtz–Kohlrausch perceived lightness (`j_hk`) of the Oklab colour
/// `(l_ok, c, h_ok)` under `vc`. This is the H-K-corrected lightness the LCS
/// contrast pipeline already uses (`lpc::j_hk_from_xyz`): a saturated colour's
/// perceived lightness is boosted above its measured luminance. A grey (`c == 0`)
/// has no boost, so its `j_hk` is just its CAM16 lightness.
fn jhk_at(l_ok: f64, c: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    let a = c * h_ok.to_radians().cos();
    let b = c * h_ok.to_radians().sin();
    let rgb = oklab_to_srgb_linear([l_ok, a, b]);
    let rgb = [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ];
    crate::lpc::j_hk_from_xyz(srgb_to_xyz(rgb), vc)
}

/// The Oklab lightness whose **gamut-edge** colour at hue `h_ok` has perceived
/// lightness `target` (`j_hk`). Bisection: at the gamut edge `j_hk` rises with
/// `l_ok` (a lighter base is perceived lighter even after the saturation boost),
/// so the root is unique. This is what places a saturated hue at a *lower* base
/// lightness so its H-K boost lands it on the shared perceived-lightness ladder.
fn l_for_jhk(target: f64, h_ok: f64, vc: &ViewingConditions) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if jhk_at(mid, max_chroma(mid, h_ok), h_ok, vc) > target {
            hi = mid;
        } else {
            lo = mid;
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

    #[test]
    fn prototype_is_the_anchor_oklab_hue() {
        // Прототип читается с Oklab-оттенка якорного цвета, а не вводится вручную —
        // это исключает дрейф между цветовыми моделями (баг, из-за которого
        // Danger уходил в розовый).
        for s in Sentiment::ALL {
            let want = oklab_hue_of(s.anchor_hex());
            assert!(
                (s.prototype_hue() - want).abs() < 1e-9,
                "{s:?}: prototype {} != anchor Oklab hue {want}",
                s.prototype_hue()
            );
        }
    }

    /// Якорные цвета заземлены в Figma CONTENTS (коллекция «🔵 4.1 Primitives»,
    /// Light-mode, обход узлов через figma-console, дата: 2026-06-30).
    ///
    /// Доказательство из Figma:
    ///   Labels/Danger/Primary  → Accent/Red   = #FF3B30  (Oklab h=28.66°)
    ///   Labels/Warning/Primary → Accent/Orange = #FFA100  (Oklab h=68.61°)
    ///   Labels/Success/Primary → Accent/Green  = #34C759  (Oklab h=147.44°)
    ///   Labels/Info/Primary    → Accent/Blue   = #3E87FF  (Oklab h=259.89°)
    ///
    /// Тест зафиксирует любое отклонение якорного Oklab-оттенка от Figma-примитивов.
    /// Используемый допуск < 0.001° — меньше разрешения 8-бит квантования.
    #[test]
    fn anchor_hues_match_figma_primitives_light_mode() {
        // Figma CONTENTS: коллекция «4.1 Primitives», Light-mode, обход переменных.
        // Значения верифицированы через figma-console figma_execute (2026-06-30).
        let expected: &[(&str, f64)] = &[
            // (figma_hex,        expected_oklab_hue_deg)
            ("#FF3B30", 28.6592),  // Accent/Red   → Danger
            ("#FFA100", 68.6070),  // Accent/Orange → Warning
            ("#34C759", 147.4439), // Accent/Green  → Success
            ("#3E87FF", 259.8918), // Accent/Blue   → Info
        ];
        let sentiments = [
            Sentiment::Danger,
            Sentiment::Warning,
            Sentiment::Success,
            Sentiment::Info,
        ];
        for ((figma_hex, want_hue), s) in expected.iter().zip(sentiments) {
            // anchor_hex() ДОЛЖЕН совпадать точно с Figma-примитивом
            assert_eq!(
                s.anchor_hex(),
                *figma_hex,
                "{s:?}: anchor_hex() дрейфанул от заземлённого Figma-примитива"
            );
            let actual = s.prototype_hue();
            // prototype_hue() ДОЛЖЕН совпадать с Oklab-оттенком Figma-примитива
            assert!(
                (actual - want_hue).abs() < 0.001,
                "{s:?}: prototype_hue() = {actual:.4}° != Figma-оттенок {want_hue}° \
                 (якорный hex Figma: {figma_hex})"
            );
        }
    }

    /// Пин контракта дедупликации, достижимый на уровне значений: (1) маппинг
    /// сентимент→семейство заземлён Figma (Labels/<Sentiment>/Primary →
    /// Accent/<Family>) и запинен явно; (2) якорь сентимента обязан быть равен
    /// якорю его семейства — две поверхности не могут разойтись. Появление
    /// локальной копии hex этот тест не видит в момент появления (значения
    /// равны), но ловит при ПЕРВОМ расхождении копий (правка одной таблицы без
    /// другой) — раньше расхождение было бы тихим.
    #[test]
    fn sentiment_delegates_anchor_to_its_accent_family() {
        use crate::accent::Accent;
        let mapping = [
            (Sentiment::Danger, Accent::Red),
            (Sentiment::Warning, Accent::Orange),
            (Sentiment::Success, Accent::Green),
            (Sentiment::Info, Accent::Blue),
        ];
        for (s, family) in mapping {
            assert_eq!(
                s.accent(),
                family,
                "{s:?}: маппинг сентимент→семейство разошёлся с Figma-заземлением"
            );
            assert_eq!(
                s.anchor_hex(),
                s.accent().anchor_hex(),
                "{s:?}: anchor_hex() не делегирует в палитру (появилась локальная копия)"
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

    /// The H-K perceived lightness of a rendered hex — the same `j_hk` the ramp
    /// matches across hues.
    fn jhk_hex(hex: &str, vc: &ViewingConditions) -> f64 {
        crate::lpc::j_hk_from_xyz(srgb_to_xyz(srgb_from_hex(hex).unwrap()), vc)
    }

    #[test]
    fn all_sentiments_share_one_perceived_lightness_ladder() {
        // The coherence invariant of the unified law: at every ramp step the four
        // sentiments sit at the SAME perceived (H-K) lightness — the neutral
        // grey's `j_hk` — so none out-shouts and all share one contrast level
        // ("одноуровневый по контрасту и светлоте"). The green warm-budget cap
        // used to approximate this by hand for one hue; now it holds for every
        // hue, by construction, for any brand. Swept across brands; the small
        // tolerance only absorbs 8-bit quantisation of the emitted hex.
        let n = neutral();
        let vc = n.vc();
        for brand in (0..360).step_by(13).map(|d| d as f64) {
            let curves: Vec<_> = Sentiment::ALL
                .into_iter()
                .map(|s| SentimentCurve::new(s, brand, s.anchor_hex(), &n).unwrap())
                .collect();
            for i in 0..=10 {
                let t = i as f64 / 10.0;
                // The ladder target: the neutral grey's perceived lightness here.
                let target = jhk_at(jp_to_oklab_l(n.at(t).jp, vc), 0.0, 0.0, vc);
                for (s, curve) in Sentiment::ALL.into_iter().zip(&curves) {
                    let got = jhk_hex(&curve.sample_hex(11)[i], vc);
                    assert!(
                        (got - target).abs() < 1.6,
                        "{s:?} brand {brand} step {i}: j_hk {got:.2} off ladder {target:.2}"
                    );
                }
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
    fn every_hue_carries_the_chroma_fraction_so_nothing_is_dull() {
        // Nothing dull: at each mid step every sentiment — INCLUDING green, which
        // the old warm-budget cap muted to ~0.79 of its own ceiling — sits at
        // (near) `CHROMA_FRACTION` of the gamut-edge chroma for its rendered
        // lightness. The 0.80 floor (target 0.88) is loose enough to absorb 8-bit
        // quantisation while still proving green is no longer capped down. Swept
        // across brands; checked on the mid steps where the gamut has chroma to give.
        let n = neutral();
        for brand in (0..360).step_by(29).map(|d| d as f64) {
            for s in Sentiment::ALL {
                let curve = SentimentCurve::new(s, brand, s.anchor_hex(), &n).unwrap();
                let h = curve.resolved_hue;
                for i in 3..=7 {
                    let hex = curve.sample_hex(11)[i].clone();
                    let lab = srgb_linear_to_oklab(srgb_from_hex(&hex).unwrap());
                    let l_r = lab[0];
                    let c_r = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
                    let c_max = max_chroma(l_r, h);
                    assert!(
                        c_r >= 0.80 * c_max,
                        "{s:?} brand {brand} step {i}: chroma {c_r:.3} dull \
                         (< 0.80 of gamut max {c_max:.3})"
                    );
                }
            }
        }
    }

    #[test]
    fn warning_floor_enforced_full_circle() {
        // Восстановлена защита (#65 её убрала, #66 унаследовал уязвимость):
        // Warning никогда не должен опускаться ниже своего категориального
        // порога в красную зону, при ЛЮБОМ брендовом оттенке на круге.
        // prototype_hex = Figma Accent/Orange (#FFA100, 2026-06-30).
        let n = neutral();
        let mut brand = 0.0;
        while brand < 360.0 {
            let h = SentimentCurve::new(Sentiment::Warning, brand, "#FFA100", &n)
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
        // Доказанный дефект машинным тестом: с picker на основе membership-field
        // Warning мог резолвиться в 3.9° от Danger (перцептивно один цвет) при
        // brand≈56°. Smooth-resolver + floor держат чёткий зазор везде.
        // prototype_hex: Warning = Figma #FFA100; Danger = Figma #FF3B30.
        let n = neutral();
        let mut brand = 0.0;
        let mut worst = f64::INFINITY;
        while brand < 360.0 {
            let w = SentimentCurve::new(Sentiment::Warning, brand, "#FFA100", &n)
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
        // placement follows directly from the smooth-asymptote model's two
        // topological seams (prototype handoff, prototype antipode), not a
        // tuned threshold.
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
            // The handoff seam is the topological flip of the hue across the brand,
            // so it is bounded by ~2·s_min (a hue at one boundary `brand ± s_min`
            // jumping to the other). Bounding its MAGNITUDE — not just allowing one
            // big jump — keeps this guard load-bearing: a future model change that
            // turned the handoff into a genuine large discontinuity (e.g. a 90°
            // wrap) would fail here instead of slipping through as "the one seam".
            let s_min = {
                let lab = srgb_linear_to_oklab(srgb_from_hex(s.anchor_hex()).unwrap());
                s_min_deg((lab[1].powi(2) + lab[2].powi(2)).sqrt())
            };
            let handoff_bound = 2.0 * s_min + 10.0; // 2·s_min + perceptual margin
            assert!(
                jumps[0] <= handoff_bound,
                "{s:?}: handoff seam {:.1}° exceeds the ~2·s_min bound {:.1}° \
                 (a spurious large discontinuity, not the expected topological flip)",
                jumps[0],
                handoff_bound
            );
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

    #[test]
    fn legalize_hue_errs_when_floor_and_separation_leave_no_legal_arc() {
        // The error branch must surface an `Err`, never silently breach an
        // invariant. Construct a pathological floor admitting only the 1° arc
        // [359°, 360°), then place the brand at 359.5° so that whole arc sits
        // inside its ±5° separation zone — no hue can satisfy both at once.
        let r = legalize_hue(0.0, 359.5, Some(359.0), 5.0);
        assert!(
            r.is_err(),
            "expected Err when the floor and separation leave no legal hue, got {r:?}"
        );

        // Sanity: relax the floor and a legal hue exists again (the same inputs
        // otherwise), so the Err above is the *constraint collision*, not a bug.
        assert!(legalize_hue(0.0, 359.5, None, 5.0).is_ok());
    }

    /// Деривационная идентичность: S_PERC_MIN = 2 × C_rep_figma × sin(20°/2),
    /// где C_rep_figma — средняя Oklab-хрома четырёх якорей из Figma CONTENTS
    /// (коллекция «🔵 4.1 Primitives», Light-mode, 2026-06-30).
    /// Порог 20° — нижний предел категориального восприятия оттенка по
    /// Witzel & Gegenfurtner (2013), JOSA A 30(7):1501, Table 1.
    /// Допуск 1e-4: хромы зафиксированы до 4 знаков, итоговая погрешность
    /// деривации существенно меньше — допуск исключает реальный дрейф константы.
    #[test]
    fn s_perc_min_derivation_identity() {
        // Oklab C якорей из Figma CONTENTS, 2026-06-30:
        let c_figma = [0.2321_f64, 0.1717_f64, 0.1944_f64, 0.1931_f64];
        let c_rep = c_figma.iter().sum::<f64>() / c_figma.len() as f64;
        // Геометрическая деривация: 2 × C_rep × sin(20°/2)
        // Источник порога 20°: Witzel & Gegenfurtner (2013), JOSA A 30(7):1501, Table 1
        let derived = 2.0 * c_rep * (10.0_f64.to_radians()).sin();
        assert!(
            (S_PERC_MIN - derived).abs() < 1e-4,
            "S_PERC_MIN = {S_PERC_MIN:.7} != выведено {derived:.7} \
             (разница {:.7} >= 1e-4; значение должно совпадать с Figma-деривацией)",
            (S_PERC_MIN - derived).abs()
        );
    }
}
