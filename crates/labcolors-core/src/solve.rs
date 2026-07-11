//! Inverse perceptual-contrast solver: `solve(bg, contract, …) → colour`.
//!
//! The forward path maps a colour to the WCAG relative luminance `Ys` of its
//! quantised display value and through [`contrast_core`](crate::lpc) to a
//! contrast value `Lc` — ось читаемости считает в домене `Ys`, в котором
//! определены константы SAPC-8 (ADR-0003; активировано главой #64). This
//! module runs that path backwards: given a background and a target contrast
//! it recovers the foreground luminance analytically (the contrast core is
//! invertible), then searches `(lightness, chroma, hue)` for an in-gamut
//! colour whose display luminance reproduces that target.
//!
//! ## Algorithm
//!
//! 1. **Background → luminance interval.** [`BgInput`] reduces to `[Y_lo, Y_hi]`
//!    in `Ys` space (WCAG relative luminance of the quantised display colour);
//!    a [`Solid`](BgInput::Solid) colour is the degenerate interval `[Y, Y]`.
//!    The contract is checked at both ends.
//! 2. **Invert the contrast core.** From the target `Lc` and a background
//!    luminance, recover the clamped foreground luminance for the matching
//!    polarity, then invert the soft black clamp to a raw `Ys` — using the
//!    same canonical constants the forward curve uses (no duplicated literals).
//! 3. **`Ys` → colour.** Bisect Oklab lightness so that, after the chroma the
//!    policy requests (capped at the in-gamut maximum via
//!    [`max_chroma`](crate::scale)), the display encoding of the colour lands
//!    on `Ys`. The step is VC-free and CAM16-free: WCAG luminance is a
//!    display-domain quantity.
//!
//! An unreachable contract returns [`Unreachable`] with a reason — never a
//! silent clip.
//!
//! All canonical contrast constants are reused from [`crate::lpc`]; this module
//! declares none of them (формула APCA SAPC-8 версии 0.0.98G-4g; метрика LPC, не APCA).

use crate::lcs::LcsColor;
use crate::lpc::{
    self, CONTRAST_SCALE, EXP_BG_DARK, EXP_BG_LIGHT, EXP_FG_DARK, EXP_FG_LIGHT, LC_SCALE,
    LO_BOW_OFFSET, LO_CLIP, LO_WOB_OFFSET,
};
use crate::scale::max_chroma;
use crate::spaces::oklab::{oklab_hue, oklab_to_srgb_linear};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_gamma, srgb_gamma_inv, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;
use crate::wcag;

/// Oklab hue angle in degrees, normalised to `[0, 360)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hue(f64);

impl Hue {
    /// Build a hue from an angle in degrees (any real value, wrapped into `[0, 360)`).
    pub fn deg(degrees: f64) -> Self {
        Self(degrees.rem_euclid(360.0))
    }

    /// The hue angle in degrees, in `[0, 360)`.
    pub fn degrees(self) -> f64 {
        self.0
    }
}

/// How much chroma the solved colour should carry.
///
/// Chroma is always capped at the in-gamut maximum for the resolved lightness
/// and hue, so every policy yields a colour inside the target gamut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromaPolicy {
    /// Achromatic (grey): zero chroma; the hue is ignored.
    Neutral,
    /// A fraction `[0, 1]` of the maximum in-gamut chroma at the resolved lightness.
    Relative(f64),
}

/// Output colour gamut. The solver produces colours inside this gamut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Gamut {
    /// Standard sRGB.
    Srgb,
    /// Display P3. Reserved: the wider-gamut chroma boundary lands in a later
    /// chapter, so v1 returns [`Unreachable::GamutUnsupported`] rather than
    /// silently solving in sRGB.
    DisplayP3,
}

/// Reserved typographic context for a future target resolver.
///
/// A later chapter will map font size/weight to a target `Lc` (large or bold
/// text tolerates lower contrast). v1 does **not** resolve it — callers pass an
/// explicit target via [`Contract::text`]. This type only reserves the seam so
/// that adding the resolver later is not a breaking change. Advisory inputs
/// (glyph shape, line length, tracking) are intentionally not modelled yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypographicContext {
    /// Font size in CSS pixels.
    pub size_px: f64,
    /// Font weight (100–900).
    pub weight: u16,
}

/// The WCAG 2.1 AA legal contrast floor a contract must clear.
///
/// EAA / EN 301 549 mandate WCAG 2.1 level AA: a relative-luminance contrast
/// ratio of 4.5:1 for normal text (success criterion 1.4.3) and 3:1 for
/// user-interface components and graphical objects (1.4.11). The floor is the
/// legal minimum *beneath* the perceptual LPC target: if the LPC solution does
/// not clear it, [`solve`] pushes the colour until it does and flags the result
/// via [`Solved::floor_override`], so the caller can see where law overrode
/// perception. Decorative / just-noticeable-difference contracts (shadows,
/// separators) carry [`None`](Floor::None) — readability law does not apply to
/// them, and `solve` leaves them on their perceptual target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Floor {
    /// WCAG 2.1 AA normal text — contrast ratio ≥ 4.5:1.
    AaText,
    /// WCAG 2.1 AA UI components / graphical objects — contrast ratio ≥ 3:1.
    AaUi,
    /// No legal floor (decorative / JND contracts).
    None,
}

impl Floor {
    /// The minimum WCAG 2.1 contrast ratio this floor enforces, if any.
    pub(crate) fn min_ratio(self) -> Option<f64> {
        match self {
            Floor::AaText => Some(wcag::AA_TEXT_RATIO),
            Floor::AaUi => Some(wcag::AA_UI_RATIO),
            Floor::None => Option::None,
        }
    }
}

/// A contrast contract: the band of acceptable contrast against the background.
///
/// Expressed as a signed `Lc` range `[floor, ceiling]`, where the sign encodes
/// polarity (positive is dark-on-light, negative is light-on-dark). v1 text
/// contracts use a degenerate range (`floor == ceiling`); the range type is
/// reserved for future just-noticeable-difference contracts (shadows,
/// separators, borders) where a band — "visible enough to be felt, no more" —
/// matters. `solve` targets `floor`.
///
/// Every contract also carries a WCAG 2.1 [`Floor`]: text and UI contracts get
/// the AA legal minimum by default (4.5:1 / 3:1); range (decorative / JND)
/// contracts get [`Floor::None`]. Disable or change it explicitly with
/// [`with_conformance`](Contract::with_conformance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contract {
    floor: f64,
    ceiling: f64,
    typography: Option<TypographicContext>,
    conformance: Floor,
}

impl Contract {
    /// A text contract for an explicit signed target `Lc` (degenerate range).
    ///
    /// Carries the WCAG 2.1 AA *normal-text* floor ([`Floor::AaText`], 4.5:1) by
    /// default — disable it explicitly with [`with_conformance`](Self::with_conformance).
    pub fn text(target_lc: f64) -> Self {
        Self {
            floor: target_lc,
            ceiling: target_lc,
            typography: None,
            conformance: Floor::AaText,
        }
    }

    /// A UI-component contract for an explicit signed target `Lc` (degenerate
    /// range).
    ///
    /// Carries the WCAG 2.1 AA *non-text* floor ([`Floor::AaUi`], 3:1) by
    /// default — for icons, controls, focus rings and graphical objects.
    pub fn ui(target_lc: f64) -> Self {
        Self {
            floor: target_lc,
            ceiling: target_lc,
            typography: None,
            conformance: Floor::AaUi,
        }
    }

    /// A range contract `[floor, ceiling]` of signed `Lc`. `solve` targets `floor`.
    ///
    /// Reserved for decorative / just-noticeable-difference contracts, so it
    /// carries [`Floor::None`]: no legal readability floor applies.
    pub fn range(floor: f64, ceiling: f64) -> Self {
        Self {
            floor,
            ceiling,
            typography: None,
            conformance: Floor::None,
        }
    }

    /// Attach a reserved [`TypographicContext`]. Not consulted by `solve` in v1.
    pub fn with_typography(mut self, ctx: TypographicContext) -> Self {
        self.typography = Some(ctx);
        self
    }

    /// Override the WCAG 2.1 conformance [`Floor`]. The default depends on the
    /// constructor ([`text`](Self::text) → AA text, [`ui`](Self::ui) → AA UI,
    /// [`range`](Self::range) → none); pass [`Floor::None`] to disable the legal
    /// floor explicitly.
    pub fn with_conformance(mut self, conformance: Floor) -> Self {
        self.conformance = conformance;
        self
    }

    /// The targeted contrast (`floor`).
    pub fn floor(self) -> f64 {
        self.floor
    }

    /// The upper bound of the contract band.
    pub fn ceiling(self) -> f64 {
        self.ceiling
    }

    /// The reserved typographic context, if any (unused by `solve` in v1).
    pub fn typography(self) -> Option<TypographicContext> {
        self.typography
    }

    /// The WCAG 2.1 conformance floor this contract enforces.
    pub fn conformance(self) -> Floor {
        self.conformance
    }
}

/// A background descriptor, reduced to a luminance interval before solving.
///
/// SEAM (a): any background reduces to a luminance interval `[Y_lo, Y_hi]` in
/// `Y_hk` space, and the contract is checked at both ends. A
/// [`Solid`](BgInput::Solid) colour is the degenerate interval `[Y, Y]` — zero
/// extra cost in v1. Future translucent-composite or area-distribution
/// backgrounds (a later chapter) add variants that widen the interval;
/// `#[non_exhaustive]` keeps that purely additive, so `solve`'s signature never
/// changes. Their interval derivation is intentionally not invented here.
// No `Copy`: future variants (translucent composites, area distributions)
// carry heap data, and removing `Copy` later would be a breaking change.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BgInput {
    /// A single opaque background colour, stored as a linear-sRGB stimulus so
    /// its luminance is resolved under the solve-time viewing conditions.
    Solid([f64; 3]),
}

impl BgInput {
    /// A solid background from an `#RRGGBB` hex colour.
    pub fn solid(hex: &str) -> Result<Self, Unreachable> {
        let rgb = srgb_from_hex(hex).map_err(Unreachable::InvalidInput)?;
        Ok(Self::Solid(rgb))
    }

    /// Reduce the descriptor to its readability luminance interval — `Ys`,
    /// WCAG relative luminance of the quantised display colour (ADR-0003:
    /// ось читаемости считает в `Ys`; активировано главой #64).
    ///
    /// New variants plug in here without touching `solve`'s signature (SEAM a).
    ///
    /// Background-dependency invariant: `resolve_set(bg, table, vc)` depends on
    /// the background **only** through two scalars derived here from `bg` — the
    /// WCAG 2.1 relative luminance `Y_wcag` of the quantised display colour
    /// (readability contract + polarity + the legal floor: один домен, одно
    /// число) and the CAM16-UCS lightness `J'_bg` (needed only by the dJ'
    /// roles). Бывший третий скаляр — H-K-люминанс `Y_hk` — покинул ось
    /// читаемости вместе с ADR-0003 и живёт только на яркостной оси
    /// ([`bg_luma`]: сторона пары, свечение, сентимент). Verified by an
    /// exhaustive trace of every `bg` read on the `resolve_set_live` path.
    /// This is what lets the grey fast path (256 codes) and the chromatic memo
    /// (keyed on the exact display colour, a superset of the two) stay
    /// bit-identical to the solver.
    pub(crate) fn luma_interval(
        &self,
        _vc: &ViewingConditions,
    ) -> Result<LumaInterval, Unreachable> {
        match self {
            BgInput::Solid(rgb) => {
                let y = wcag::relative_luminance(quantised_display(*rgb));
                Ok(LumaInterval { lo: y, hi: y })
            }
        }
    }

    /// Gamma-encoded (8-bit-quantised) sRGB of the endpoint the WCAG floor is
    /// checked against — the background colour with the least luminance contrast
    /// for the target's polarity. For a [`Solid`](BgInput::Solid) background this
    /// is just the colour. Future interval backgrounds resolve their worst-case
    /// endpoint here, keeping `solve` free of variant matching (SEAM a).
    fn governing_display(&self, _target: f64) -> [f64; 3] {
        match self {
            BgInput::Solid(rgb) => quantised_display(*rgb),
        }
    }

    /// Гамма-кодированный 8-битный sRGB фона (`[0,1]³`, byte/255) — домен
    /// reference-профиля [`crate::alpha`], заземлённого Figma-якорями без
    /// универсального обещания browser pipeline. Альфа-роль
    /// ([`crate::semantic::RoleSpec::Ladder`] /
    /// [`AlphaAnalog`](crate::semantic::RoleSpec::AlphaAnalog)) композитит свой
    /// тинт на этом фоне для честного замера контраста солид-эквивалента. Для
    /// [`Solid`](BgInput::Solid) это квантованный дисплей-цвет фона; будущие
    /// интервальные фоны выберут здесь свой представительный край, оставляя
    /// физику резолва свободной от матчинга вариантов (SEAM a).
    pub(crate) fn encoded_display(&self) -> [f64; 3] {
        match self {
            BgInput::Solid(rgb) => quantised_display(*rgb),
        }
    }
}

/// A background luminance interval in `Ys` space (WCAG relative luminance of
/// the quantised display colour — домен оси читаемости, ADR-0003).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LumaInterval {
    lo: f64,
    hi: f64,
}

impl LumaInterval {
    /// The two luminance endpoints the contract is checked against.
    fn endpoints(self) -> [f64; 2] {
        [self.lo, self.hi]
    }

    /// The worst-case background for a target's polarity — the end that yields
    /// the least contrast for a fixed foreground, so meeting the contract there
    /// meets it across the whole interval. Dark-on-light (`target ≥ 0`) is
    /// hardest against the darkest background; light-on-dark against the
    /// brightest. Degenerate for [`BgInput::Solid`] (`lo == hi`).
    fn governing(self, target: f64) -> f64 {
        if target >= 0.0 { self.lo } else { self.hi }
    }
}

/// A solved foreground colour and the two contrasts it actually achieves.
///
/// The perceptual [`lc`](Solved::lc) (signed LPC) and the legal
/// [`wcag_ratio`](Solved::wcag_ratio) (symmetric WCAG 2.1) are reported as
/// separate numbers — they measure different things and are never conflated.
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    color: LcsColor,
    hex: String,
    lc: f64,
    wcag_ratio: f64,
    floor_override: bool,
}

impl Solved {
    /// The resolved colour, decoded under the solve-time viewing conditions.
    pub fn color(&self) -> LcsColor {
        self.color
    }

    /// The resolved colour as an `#RRGGBB` hex string.
    pub fn hex(&self) -> &str {
        &self.hex
    }

    /// The signed perceptual contrast `Lc` the resolved colour achieves against
    /// the background, measured on the quantised hex — what the caller actually
    /// gets. This is the LPC metric, not WCAG; see [`wcag_ratio`](Self::wcag_ratio).
    pub fn lc(&self) -> f64 {
        self.lc
    }

    /// The WCAG 2.1 relative-luminance contrast ratio (1–21) of the resolved
    /// colour against the background, measured on the quantised hex. For a
    /// text/UI contract this is guaranteed to meet the contract's [`Floor`]
    /// (≥ 4.5 or ≥ 3.0); for a [`Floor::None`] contract it is reported for
    /// transparency but not enforced.
    pub fn wcag_ratio(&self) -> f64 {
        self.wcag_ratio
    }

    /// `true` when the WCAG legal floor overrode the perceptual target: the LPC
    /// solution did not clear the floor, so the colour was pushed (darker for
    /// dark-on-light, lighter for light-on-dark) until it did. Lets the caller
    /// surface where the law won over perception.
    pub fn floor_override(&self) -> bool {
        self.floor_override
    }
}

/// Why a solve could not return a colour. Physical/domain variants explain a
/// contract failure; [`Self::InternalInvariant`] reports core drift and must be
/// failed closed by bindings rather than projected as a physical outcome.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Unreachable {
    /// `|target|` is below the low-contrast clip floor (`loClip`): the contrast
    /// curve reports zero there, so no colour can reproduce such a target.
    BelowContrastFloor { target: f64 },
    /// The background cannot supply the target even at the luminance extreme
    /// (pure black for dark-on-light, pure white for light-on-dark).
    ExceedsRange { target: f64, max_achievable: f64 },
    /// The target falls in an 8-bit quantisation gap: the analytic foreground is
    /// reachable in principle, but every hex colour the solver can emit near it
    /// lands either short of the target or inside the low-contrast dead zone, so
    /// no on-grid colour reproduces it within the ±1 Lc budget. Distinct from
    /// [`Self::ExceedsRange`] (where the *background* is the limit): here the
    /// background can supply the target, the discrete sRGB grid cannot.
    /// `nearest` is the closest |Lc| an adjacent hex step actually achieves.
    QuantizationGap { target: f64, nearest: f64 },
    /// The WCAG legal floor cannot be met on this background even at the
    /// achromatic extreme (pure black for dark-on-light, pure white for
    /// light-on-dark). `max_ratio` is the most contrast this background can
    /// supply in that polarity; `floor` is the ratio the contract required.
    FloorUnreachable { floor: f64, max_ratio: f64 },
    /// The target's polarity disagrees with the background's luminance, e.g. a
    /// dark-on-light target against a background that is already dark.
    ///
    /// Defensive guard: with the canonical constant set the low-contrast floor
    /// rejects such targets first (they surface as [`Self::BelowContrastFloor`]
    /// or [`Self::ExceedsRange`]), so this variant is not produced in practice.
    PolarityMismatch { target: f64 },
    /// The requested gamut is not supported yet (Display P3 arrives later).
    GamutUnsupported,
    /// Malformed input, such as an invalid hex colour or a non-finite target.
    InvalidInput(String),
    /// A value produced and validated by the core later violated an internal
    /// postcondition. This is never client-input blame: bindings must fail the
    /// enclosing call closed instead of projecting it as a physical role
    /// outcome.
    InternalInvariant(String),
}

impl core::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BelowContrastFloor { target } => write!(
                f,
                "target Lc {target:.2} is inside the low-contrast dead zone; no colour reaches it"
            ),
            Self::ExceedsRange {
                target,
                max_achievable,
            } => write!(
                f,
                "target Lc {target:.2} exceeds the most this background can supply ({max_achievable:.2})"
            ),
            Self::QuantizationGap { target, nearest } => write!(
                f,
                "target Lc {target:.2} falls in an 8-bit quantisation gap; the nearest on-grid colour reaches only {nearest:.2}"
            ),
            Self::FloorUnreachable { floor, max_ratio } => write!(
                f,
                "WCAG floor {floor:.1}:1 is unreachable on this background (max {max_ratio:.2}:1)"
            ),
            Self::PolarityMismatch { target } => write!(
                f,
                "target Lc {target:.2} has the wrong polarity for this background's luminance"
            ),
            Self::GamutUnsupported => {
                write!(
                    f,
                    "requested gamut is not supported yet (Display P3 is future work)"
                )
            }
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::InternalInvariant(msg) => write!(f, "internal invariant failure: {msg}"),
        }
    }
}

impl std::error::Error for Unreachable {}

/// Solve for a foreground colour that meets `contract` against `bg`.
///
/// Returns the resolved colour together with the contrast it achieves, or
/// [`Unreachable`] explaining why no colour can satisfy the contract. See the
/// [module documentation](self) for the algorithm.
///
/// * `bg` — the background (reduced to a luminance interval).
/// * `contract` — the contrast band; `solve` targets its [`floor`](Contract::floor).
/// * `hue` — the Oklab hue for the foreground (ignored when chroma is zero).
/// * `chroma_policy` — how saturated the foreground should be.
/// * `vc` — viewing conditions; pass the same VC the theme resolves under.
/// * `gamut` — the output gamut.
pub fn solve(
    bg: BgInput,
    contract: Contract,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
    gamut: Gamut,
) -> Result<Solved, Unreachable> {
    // The Display P3 chroma boundary is future work (chapter 5); fail loudly.
    if gamut != Gamut::Srgb {
        return Err(Unreachable::GamutUnsupported);
    }
    validate_job(contract, hue, chroma_policy)?;
    // The background side costs exactly one CIECAM16 forward — its H-K luminance
    // interval. Compute it here and hand it to [`solve_in`]; [`solve_many`] and
    // [`resolve_set`](crate::resolve_set) compute it once and reuse it across a
    // whole batch instead of re-deriving the same background forward per target.
    let interval = bg.luma_interval(vc)?;
    solve_in(&bg, contract, hue, chroma_policy, vc, interval)
}

/// One foreground request in a [`solve_many`] batch: the contract to meet plus
/// the foreground's hue and chroma policy. The background, viewing conditions,
/// and gamut are shared across the batch, so the background's H-K luminance
/// forward is paid once for the whole slice rather than once per request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveJob {
    /// The contrast contract this foreground must meet against the background.
    pub contract: Contract,
    /// The Oklab hue of the foreground (ignored when chroma is zero).
    pub hue: Hue,
    /// How saturated the foreground should be.
    pub chroma_policy: ChromaPolicy,
}

/// Solve a batch of foreground requests against one shared background.
///
/// Equivalent to calling [`solve`] once per [`SolveJob`], but the background's
/// luminance interval — the only CIECAM16 forward the background side costs — is
/// computed once for the whole slice. The returned vector is positional: entry
/// `i` is the result for `jobs[i]`, each carrying its own `Result` so one
/// unreachable request never fails the batch. A whole-batch failure (unsupported
/// gamut, or a background that cannot be reduced) is the outer `Err`.
pub fn solve_many(
    bg: BgInput,
    jobs: &[SolveJob],
    vc: &ViewingConditions,
    gamut: Gamut,
) -> Result<Vec<Result<Solved, Unreachable>>, Unreachable> {
    if gamut != Gamut::Srgb {
        return Err(Unreachable::GamutUnsupported);
    }
    // Background side: one forward for the whole batch (see [`solve`]).
    let interval = bg.luma_interval(vc)?;
    Ok(jobs
        .iter()
        .map(|job| {
            validate_job(job.contract, job.hue, job.chroma_policy)?;
            solve_in(&bg, job.contract, job.hue, job.chroma_policy, vc, interval)
        })
        .collect())
}

/// Reject a non-finite contract target, hue, or chroma ratio before solving —
/// the per-request guard [`solve`] and [`solve_many`] share. Cheap; runs per
/// request, never touching the background side.
fn validate_job(
    contract: Contract,
    hue: Hue,
    chroma_policy: ChromaPolicy,
) -> Result<(), Unreachable> {
    let target = contract.floor();
    if !target.is_finite() {
        return Err(Unreachable::InvalidInput(format!(
            "target Lc is not finite: {target}"
        )));
    }

    let hue_deg = hue.degrees();
    if !hue_deg.is_finite() {
        return Err(Unreachable::InvalidInput(format!(
            "hue is not finite: {hue_deg}"
        )));
    }
    if let ChromaPolicy::Relative(ratio) = chroma_policy {
        if !ratio.is_finite() {
            return Err(Unreachable::InvalidInput(format!(
                "chroma ratio is not finite: {ratio}"
            )));
        }
        if !(0.0..=1.0).contains(&ratio) {
            return Err(Unreachable::InvalidInput(format!(
                "chroma ratio must be inside [0, 1], got {ratio}"
            )));
        }
    }
    Ok(())
}

/// Solve one foreground against a background whose luminance `interval` is
/// already computed — the shared core of [`solve`], [`solve_many`], and the
/// per-role solves in [`resolve_set`](crate::resolve_set). Inputs are assumed
/// validated (finite target/hue/ratio, sRGB gamut); the public entry points
/// guard that before calling in. See the [module documentation](self) for the
/// algorithm.
pub(crate) fn solve_in(
    bg: &BgInput,
    contract: Contract,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
    interval: LumaInterval,
) -> Result<Solved, Unreachable> {
    let target = contract.floor();
    let y_gov = interval.governing(target);

    // Stage 1 — perceptual target. Invert the LPC core for the Oklab lightness
    // that reproduces the contract's target against the governing endpoint.
    let l_lpc = solve_lpc_lightness(y_gov, target, hue, chroma_policy)?;

    // Stage 2 — legal floor. Text/UI contracts carry a WCAG 2.1 AA floor; if the
    // perceptual solution falls short of it, push the colour until it clears the
    // floor and flag the override. Decorative ([`Floor::None`]) contracts skip
    // this entirely and keep their perceptual target. The resolved Oklab
    // lightness (not just the colour) is returned so the quantisation-gap search
    // below can step to neighbouring hex grid points from it.
    let bg_disp = bg.governing_display(target);
    let (l_final, floor_override) = match contract.conformance().min_ratio() {
        Some(floor_ratio) => apply_floor(l_lpc, floor_ratio, target, hue, chroma_policy, bg_disp)?,
        Option::None => (l_lpc, false),
    };

    // Stage 3 — quantise, measure, verify. Build the colour at the resolved
    // lightness, emit its hex, and confirm the dual gate (perceptual floor at
    // both interval ends, plus the legal WCAG floor for text/UI) still holds on
    // the *quantised* colour. If it does not, the analytic solution may have
    // fallen into an 8-bit quantisation gap — the emitted hex lands inside the
    // low-contrast dead zone even though the background can supply the target —
    // so walk a bounded number of neighbouring hex steps toward larger `|Lc|`
    // before giving up. Every candidate is re-measured honestly: no silent clip.
    let evaluate = |l_ok: f64| -> Result<Candidate, Unreachable> {
        let rgb = build_color(l_ok, hue, chroma_policy);
        let solved = finish(rgb, y_gov, bg_disp, floor_override, vc)?;
        // Perceptual floor at every interval endpoint. The governing endpoint's
        // contrast is exactly `solved.lc()` (it is the `y_bg` `finish` measured
        // against), so reuse it instead of re-deriving the foreground luminance —
        // that recovery is the costly H-K forward. Only a *distinct* endpoint
        // (genuine luminance intervals, a future background variant) pays for a
        // fresh measurement; a [`Solid`] background's endpoints all coincide with
        // the governing one, so it measures the foreground exactly once.
        let perceptual_ok = interval.endpoints().into_iter().all(|y_end| {
            if y_end == y_gov {
                meets_floor_lc(solved.lc(), target)
            } else {
                meets_floor(&solved, y_end, target, vc)
            }
        });
        // The walk only moves toward the achromatic extreme, which raises (never
        // lowers) WCAG contrast, but re-verify the legal floor explicitly rather
        // than lean on an unproven monotonicity assumption.
        let legal_ok = contract
            .conformance()
            .min_ratio()
            .is_none_or(|floor_ratio| solved.wcag_ratio() + 1e-9 >= floor_ratio);
        Ok(Candidate {
            passes: perceptual_ok && legal_ok,
            lc: solved.lc(),
            solved,
        })
    };

    let primary = evaluate(l_final)?;
    if primary.passes {
        return Ok(primary.solved);
    }
    solve_quantization_neighbor(l_final, target, hue, chroma_policy, primary.lc, evaluate)
}

/// The quantisation budget: a solved colour is accepted only when its measured
/// `Lc` lands within this *symmetric* distance of the target. The analytic
/// primary path lands close by construction; the neighbour walk below moves
/// *away* from the target toward larger `|Lc|`, so without the upper bound a
/// step could overshoot — this constant makes the `±1` contract explicit and
/// symmetric for the neighbour search (mirrors the test tolerance `TOL`).
///
/// Терминал **(c) INTERVAL-INSENSITIVE**: `QUANT_BUDGET` ≈ 2–3× медианного
/// Lc-шага 8-бит серой сетки (замер ≈0.44) — на дискретной сетке любой бюджет
/// в этом диапазоне принимает тот же ближайший узел
/// (`quant_budget_is_a_couple_of_grid_steps`). Экспозиция (доля целей, чья
/// приёмка флипает при свипе ±50%) — **1.84%** (`exposure_quant_and_dj_budgets`).
// SSOT-TRACKED — допуск приёмки Lc в единицах шага сетки (±1 Lc), терминал (c) interval-insensitive (exposure 1.84%), см. docs/empirical-inventory.md.
const QUANT_BUDGET: f64 = 1.0;

/// One on-grid candidate the quantisation-gap search evaluates: the solved
/// colour, the perceptual `Lc` it actually achieves on the quantised hex, and
/// whether it clears the dual gate (perceptual floor at both interval ends +
/// legal WCAG floor). `passes` is the *lower*-bound floor check the primary
/// solution uses; the neighbour walk additionally enforces the upper bound so a
/// step can never overshoot the `±1` budget.
struct Candidate {
    solved: Solved,
    lc: f64,
    passes: bool,
}

impl Candidate {
    /// Distance of the achieved `Lc` from the target — the symmetric error the
    /// `±1` budget bounds and the neighbour search minimises for its near-miss.
    fn error(&self, target: f64) -> f64 {
        (self.lc - target).abs()
    }
}

/// Maximum distinct hex steps the quantisation-gap search explores from the
/// analytic solution. Two steps is enough to cross the single dead-zone band the
/// 8-bit grid opens just above the low-contrast clip; this is a gap-bridge, not
/// an optimiser, so the reach is deliberately tiny (issue #44).
const NEIGHBOR_STEPS: u32 = 2;

/// Walk up to [`NEIGHBOR_STEPS`] *distinct* hex grid points toward larger `|Lc|`
/// — darker for dark-on-light (`target ≥ 0`), lighter for light-on-dark — and
/// return the first that clears the dual gate **and** lands within the symmetric
/// [`QUANT_BUDGET`] of the target.
///
/// Two honesty guarantees:
/// * *Distinct* — a step counts only when the emitted hex actually changes, so
///   the search can never silently re-clip to the colour it started from.
/// * *Bounded both ways* — `evaluate.passes` rejects steps that fall short of
///   the floor; the `±QUANT_BUDGET` check here rejects steps that overshoot it.
///   A neighbour is returned only when it is genuinely within budget.
///
/// If no neighbour qualifies, the target sits in a real quantisation gap and
/// [`Unreachable::QuantizationGap`] is returned, reporting the `|Lc|` of the
/// *closest* colour explored (the start plus every neighbour) — the true
/// near-miss, never a fabricated bound.
fn solve_quantization_neighbor(
    l_start: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    start_lc: f64,
    evaluate: impl Fn(f64) -> Result<Candidate, Unreachable>,
) -> Result<Solved, Unreachable> {
    // Toward larger contrast: dark-on-light needs a darker foreground (lower
    // Oklab lightness), light-on-dark a lighter one. The probe increment is well
    // below one 8-bit grid step so neighbours are visited in order, not skipped.
    // For a `Relative` chroma policy `build_color` also moves chroma with
    // lightness, so a single probe can in principle cross more than one
    // `#RRGGBB`; correctness does not rely on perfect grid-adjacency, because a
    // step is *accepted* only when it lands within the symmetric `QUANT_BUDGET`
    // below — an over-jump that overshoots the target is rejected, not clipped.
    let direction = if target >= 0.0 { -1.0 } else { 1.0 };
    // Oklab-lightness step per probe. 0.001 is ~¼ of one 8-bit sRGB grid step
    // near mid-tones, so consecutive `#RRGGBB` values are visited in order
    // (never skipped) while keeping the walk bounded — the loop below caps it at
    // `NEIGHBOR_STEPS` probes and accepts a step only inside `QUANT_BUDGET`.
    const PROBE: f64 = 0.001;

    let mut last_hex = hex_from_srgb(build_color(l_start, hue, chroma_policy));
    let mut steps_taken = 0_u32;
    let mut l_probe = l_start;
    // Track the colour closest to the target across the start and every
    // neighbour, so the gap error reports the true near-miss (not a max).
    let mut nearest_lc = start_lc;
    let mut nearest_err = (start_lc - target).abs();

    while steps_taken < NEIGHBOR_STEPS && (0.0..=1.0).contains(&l_probe) {
        l_probe += direction * PROBE;
        let hex = hex_from_srgb(build_color(l_probe, hue, chroma_policy));
        if hex == last_hex {
            continue; // same grid point — not yet a distinct neighbour step
        }
        last_hex = hex;
        steps_taken += 1;

        let candidate = evaluate(l_probe)?;
        let err = candidate.error(target);
        if err < nearest_err {
            nearest_err = err;
            nearest_lc = candidate.lc;
        }
        // Accept only when the floor holds AND the step has not overshot the
        // symmetric budget — an honest neighbour, in band on both sides.
        if candidate.passes && err <= QUANT_BUDGET {
            return Ok(candidate.solved);
        }
    }

    Err(Unreachable::QuantizationGap {
        target,
        nearest: nearest_lc.abs(),
    })
}

/// Stage 1: invert the LPC core to the Oklab lightness reproducing `target`
/// against a single background luminance (`Ys` domain, ADR-0003). VC-free:
/// обе стороны инверсии — display-доменные величины.
fn solve_lpc_lightness(
    y_bg: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
) -> Result<f64, Unreachable> {
    let y_fg = invert_contrast(y_bg, target)?;
    Ok(match_lightness_ys(y_fg, hue, chroma_policy))
}

/// The CAM16-UCS lightness `J'` of a **linear**-sRGB colour under `vc`.
fn jp_of_linear(rgb_linear: [f64; 3], vc: &ViewingConditions) -> f64 {
    LcsColor::from_xyz_with_hok(srgb_to_xyz(rgb_linear), 0.0, vc).jp
}

/// The acceptance budget, in CAM16-UCS `J'` units, for a decorative dJ' solve: a
/// colour is accepted when its measured `|dJ'|` lands within this distance of the
/// target magnitude. It is the dJ' analogue of [`QUANT_BUDGET`] (the `±1 Lc`
/// contrast budget): on the light end one 8-bit grey step is worth ~0.3–0.5 `J'`,
/// so `0.6` is just over one grid step — wide enough that a reachable target is
/// not rejected for landing on the neighbouring pixel, tight enough that the
/// emitted colour is honestly within a pixel of the requested separation.
///
/// Терминал **(c) INTERVAL-INSENSITIVE**: `DJ_BUDGET` ≈ 1.2–2× медианного
/// dJ'-шага 8-бит серой сетки (замер ≈0.39) — тот же класс, что
/// [`QUANT_BUDGET`] (`dj_budget_tracks_grid_step`). Экспозиция — **1.55%**
/// (`exposure_quant_and_dj_budgets`).
// SSOT-TRACKED — допуск приёмки dJ' (J'-единицы), ~1 шаг сетки, терминал (c) interval-insensitive (exposure 1.55%); см. docs/empirical-inventory.md.
const DJ_BUDGET: f64 = 0.6;

/// Maximum distinct hex steps the dJ' search walks from the analytic seed toward
/// the target `J'`. Like [`NEIGHBOR_STEPS`] this is a tiny grid-bridge, not an
/// optimiser: the analytic seed lands within a pixel by construction, and a
/// couple of steps cross the one grid cell quantisation can misplace it into.
const DJ_NEIGHBOR_STEPS: u32 = 2;

/// Solve a decorative perceived-lightness-difference (dJ') contract: find the
/// in-gamut colour whose CAM16-UCS lightness `J'` is `magnitude_dj` away from the
/// background's `J'`, in the direction `sign` selects (negative `J'` offset for
/// dark-on-light `sign = +1` → a darker decorative mark on a light surface;
/// positive for light-on-dark).
///
/// This is **different physics** from the contrast solver above: there is no
/// readability floor and no low-contrast clip — distinguishability of a
/// decorative element (a fill tint, a hairline border) is a *perceived lightness
/// step* on the perceptually-uniform J' axis, not an LPC contrast ratio. The
/// solve is analytic end to end:
///
/// 1. `J'_bg` — measured on the quantised background display colour under `vc`.
/// 2. `J'_target = J'_bg − sign·dJ'` — the owner's literal anchor offset.
/// 3. `J'_target → Oklab L` — the shared grey-axis inverse
///    [`scale::jp_to_oklab_l`](crate::scale), the same one the accent curve uses.
/// 4. `build_color` at the role's undertone plan → quantise → **measure the
///    achieved `|dJ'|` on the emitted hex** (`|J'_fg_quant − J'_bg|`) — an honest
///    finish on the colour the caller actually gets, never the pre-quantisation
///    ideal.
///
/// If the quantised colour lands within [`DJ_BUDGET`] of the target it is
/// returned. Otherwise a bounded walk steps toward the target `J'` across at most
/// [`DJ_NEIGHBOR_STEPS`] distinct hex grid points. If none lands in budget — or
/// the target J' falls off the end of the achievable axis (e.g. a positive dJ'
/// requested above a near-white background) — the contract **деградирует к
/// ближайшему достижимому** (ADR-0002, закон 2): возвращается цвет с
/// минимальной ошибкой `||ΔJ'|−цель|` среди осмотренных грид-точек, помеченный
/// `degraded: true`, с честно замеренным `achieved_dj`. Голый отказ прежней
/// версии (`DjUnreachable`) наказывал владельца контракта ошибкой за
/// физическую стену оси — вместо честного результата (политика Figma-коэрсии:
/// rgb(999) → 255, не exception).
///
/// The reported `lc` on the returned [`Solved`] is still the measured LPC
/// contrast of the emitted colour against the background (so the ladder-order
/// invariants and the golden read a consistent number); only the *target* and
/// the *acceptance metric* are in J' space.
pub(crate) fn solve_dj(
    bg: &BgInput,
    magnitude_dj: f64,
    sign: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> Result<DjSolved, Unreachable> {
    if !magnitude_dj.is_finite() || magnitude_dj < 0.0 {
        return Err(Unreachable::InvalidInput(format!(
            "dJ' magnitude must be finite and non-negative: {magnitude_dj}"
        )));
    }

    // The contract is measured against the *displayed* background — the colour on
    // screen, gamma-quantised then decoded back to linear — so the separation is
    // the one the eye sees, in the same space `finish` measures the foreground in.
    let bg_disp = bg.governing_display(sign);
    // `governing_display` is gamma-encoded (8-bit display values); decode back to
    // linear so the J' forward sees the same space `finish` measures in.
    let bg_disp_linear = [
        srgb_gamma_inv(bg_disp[0]),
        srgb_gamma_inv(bg_disp[1]),
        srgb_gamma_inv(bg_disp[2]),
    ];
    let jp_bg = jp_of_linear(bg_disp_linear, vc);
    // Direction: dark-on-light (`sign = +1`) places the mark *below* the surface
    // in lightness; light-on-dark *above*. "Toward the larger headroom" is exactly
    // the set polarity, so the offset sign mirrors the contrast solver's.
    let jp_target = jp_bg - sign * magnitude_dj;

    // The luminance interval still governs which background endpoint the perceptual
    // LPC measurement uses for the reported `lc` (degenerate for a Solid bg).
    let interval = bg.luma_interval(vc)?;
    let y_gov = interval.governing(sign);

    // Build, quantise, and honestly measure the achieved separation on the emitted
    // hex (decoded to linear — the colour the caller actually gets), not the
    // pre-quantisation ideal.
    let evaluate = |jp_goal: f64| -> Result<DjCandidate, Unreachable> {
        let l_ok = crate::scale::jp_to_oklab_l(jp_goal, vc);
        let rgb = build_color(l_ok, hue, chroma_policy);
        let solved = finish(rgb, y_gov, bg_disp, false, vc)?;
        let rgb_quantised = srgb_from_hex(solved.hex()).map_err(Unreachable::InvalidInput)?;
        let achieved_dj = (jp_of_linear(rgb_quantised, vc) - jp_bg).abs();
        Ok(DjCandidate {
            error: (achieved_dj - magnitude_dj).abs(),
            achieved_dj,
            solved,
        })
    };

    let primary = evaluate(jp_target)?;
    if primary.error <= DJ_BUDGET {
        return Ok(DjSolved {
            achieved_dj: primary.achieved_dj,
            solved: primary.solved,
            degraded: false,
        });
    }

    // The seed missed the budget — walk distinct hex grid points toward larger
    // separation (away from `jp_bg`, in the polarity's direction) so a
    // quantisation undershoot is corrected. Probe well below one grid step so
    // neighbours are visited in order. Track the best candidate (min error)
    // across the seed and every neighbour — it becomes the degraded result if
    // nothing lands in budget.
    let direction = -sign;
    const PROBE: f64 = 0.05;
    // Bound the probe count independently of the distinct-step count so a run of
    // identical grid points can never loop forever; with a J' axis span well
    // under ~243 this reaches the white/black wall long before the cap.
    const MAX_PROBES: u32 = 256;
    let mut last_hex = primary.solved.hex().to_string();
    let mut steps_taken = 0_u32;
    let mut probes = 0_u32;
    let mut jp_probe = jp_target;
    let mut best = primary;

    while steps_taken < DJ_NEIGHBOR_STEPS && probes < MAX_PROBES {
        jp_probe += direction * PROBE;
        probes += 1;
        let candidate = evaluate(jp_probe)?;
        if candidate.solved.hex() == last_hex {
            continue; // same grid point — not yet a distinct neighbour step
        }
        last_hex = candidate.solved.hex().to_string();
        steps_taken += 1;
        let in_budget = candidate.error <= DJ_BUDGET;
        if candidate.error < best.error {
            best = candidate;
        }
        if in_budget {
            return Ok(DjSolved {
                achieved_dj: best.achieved_dj,
                solved: best.solved,
                degraded: false,
            });
        }
    }

    // Закон 2 ADR-0002: цель за стеной оси / в квантовой дыре — ближайший
    // достижимый цвет с флагом, не ошибка.
    Ok(DjSolved {
        achieved_dj: best.achieved_dj,
        solved: best.solved,
        degraded: true,
    })
}

/// One on-grid candidate the dJ' search evaluates: the solved colour, the
/// `|dJ'|` it achieves on the quantised hex, and the distance of that from the
/// requested magnitude (the budget the search minimises).
struct DjCandidate {
    solved: Solved,
    achieved_dj: f64,
    error: f64,
}

/// Результат dJ'-солва: решённый цвет, честно замеренный `|ΔJ'|` на отданном
/// hex и флаг деградации (закон 2 ADR-0002 — цель недостижима, отдан
/// ближайший достижимый).
pub(crate) struct DjSolved {
    pub(crate) solved: Solved,
    /// Честный замер |ΔJ'| на отданном hex — доносится до
    /// `Resolved::Color.achieved_dj` и wasm-DTO (симметрия честности с glow).
    pub(crate) achieved_dj: f64,
    pub(crate) degraded: bool,
}

/// Stage 2: enforce the WCAG legal floor on the quantised colour.
///
/// If the perceptual solution already clears `floor_ratio`, perception governs
/// and the colour is returned unchanged (no override). Otherwise the lightness
/// is pushed toward the achromatic extreme in the contract's polarity — darker
/// for dark-on-light (`target ≥ 0`), lighter for light-on-dark — where WCAG
/// contrast is greatest, by the smallest amount the lightness bisection finds
/// that still clears the floor on the quantised hex. (For chromatic policies
/// the ratio along the path is not formally proven monotonic, so "smallest" is
/// up to the bisection's resolution; the floor guarantee itself never depends
/// on monotonicity — the returned colour is always a verified passing point.) If even the extreme cannot reach the floor, the contract
/// is [`Unreachable::FloorUnreachable`].
/// Lightness-bracket width below which the floor bisection has pinned the
/// lightness finely enough that the emitted 8-bit hex can no longer move. At
/// ~1e-9 it is far tighter than the lightness step one hex byte spans, so the
/// early exit is provably hex-preserving while cutting the bisection from a
/// fixed 48 steps to ~30. Mirrors `semantic::RATIO_BISECT_EPS` (excluded from the
/// perceptual-const gate by `NUMERIC_METHOD_ALLOWLIST`, same numeric-epsilon class).
const FLOOR_BISECT_EPS: f64 = 1e-9;

fn apply_floor(
    l_lpc: f64,
    floor_ratio: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    bg_disp: [f64; 3],
) -> Result<(f64, bool), Unreachable> {
    let rgb_lpc = build_color(l_lpc, hue, chroma_policy);
    if floor_ratio_of(rgb_lpc, bg_disp) >= floor_ratio {
        return Ok((l_lpc, false));
    }

    let l_extreme = if target >= 0.0 { 0.0 } else { 1.0 };
    let max_ratio = floor_ratio_of(build_color(l_extreme, hue, chroma_policy), bg_disp);
    if max_ratio < floor_ratio {
        return Err(Unreachable::FloorUnreachable {
            floor: floor_ratio,
            max_ratio,
        });
    }

    // Bisect the lightness path from the perceptual solution (`t = 0`, below the
    // floor) to the achromatic extreme (`t = 1`, clears it). Invariant: `hi`
    // always names a colour that clears the floor, `lo` one that does not, so the
    // returned lightness is guaranteed to meet the floor even after quantisation.
    //
    // The background's relative luminance is loop-invariant — computed ONCE here.
    // `floor_ratio_of` re-linearised `bg_disp` (three `powf(2.4)`) on every
    // iteration; hoisting it feeds the same `rl_bg` to `ratio_from_luminances`,
    // which is the exact value `contrast_ratio(fg, bg)` would produce (same
    // operands, same order) — byte-identical, pinned by
    // `apply_floor_matches_the_cold_bisection_byte_for_byte`.
    let rl_bg = wcag::relative_luminance(bg_disp);
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..48 {
        // Early exit — the returned `hi` maps a lightness span `|l_extreme -
        // l_lpc| ≤ 1`, so once the bracket collapses below `FLOOR_BISECT_EPS`
        // the fully-converged `hi*` (with `lo ≤ hi* ≤ hi`) differs from the
        // current `hi` by < 1e-9 in lightness — far below the ~1/255 channel
        // move one 8-bit byte spans, so `build_color(l_final)` quantises to the
        // identical hex. Same provably-hex-preserving reasoning as
        // `ratio_for_target_mp`'s `RATIO_BISECT_EPS`; pinned byte-for-byte
        // against the full-48 bisection by
        // `apply_floor_matches_the_cold_bisection_byte_for_byte`.
        if hi - lo < FLOOR_BISECT_EPS {
            break;
        }
        let mid = (lo + hi) * 0.5;
        let l_mid = l_lpc + (l_extreme - l_lpc) * mid;
        if floor_ratio_of_with_bg_rl(build_color(l_mid, hue, chroma_policy), rl_bg) >= floor_ratio {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let l_final = l_lpc + (l_extreme - l_lpc) * hi;
    Ok((l_final, true))
}

/// WCAG 2.1 contrast ratio of a linear-sRGB foreground (quantised to the hex it
/// will be emitted as) against the gamma-encoded background.
fn floor_ratio_of(rgb_linear: [f64; 3], bg_disp: [f64; 3]) -> f64 {
    wcag::contrast_ratio(quantised_display(rgb_linear), bg_disp)
}

/// [`floor_ratio_of`] with the background's relative luminance precomputed, for a
/// loop over many foregrounds against ONE fixed background (the `apply_floor`
/// bisection). Byte-identical to `floor_ratio_of(rgb_linear, bg_disp)` when
/// `rl_bg == relative_luminance(bg_disp)`: `contrast_ratio` is exactly
/// `ratio_from_luminances(relative_luminance(fg), relative_luminance(bg))`, so
/// substituting the cached `rl_bg` changes nothing but the redundant re-linearise.
fn floor_ratio_of_with_bg_rl(rgb_linear: [f64; 3], rl_bg: f64) -> f64 {
    let rl_fg = wcag::relative_luminance(quantised_display(rgb_linear));
    wcag::ratio_from_luminances(rl_fg, rl_bg)
}

/// Gamma-encoded sRGB of a linear stimulus, quantised to 8-bit — the display
/// values WCAG 2.1 measures, matching the emitted `#RRGGBB` hex exactly.
pub(crate) fn quantised_display(rgb_linear: [f64; 3]) -> [f64; 3] {
    let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() / 255.0;
    [q(rgb_linear[0]), q(rgb_linear[1]), q(rgb_linear[2])]
}

/// The largest `Lc` magnitude this background can supply in the polarity of
/// `target`, measured through the forward curve with the extreme foreground
/// (pure black for dark-on-light, pure white for light-on-dark) — the same
/// single source of truth the inversion is derived from.
fn max_lc(y_bg: f64, target: f64) -> f64 {
    let extreme_fg = if target > 0.0 { 0.0 } else { 1.0 };
    lpc::contrast_core(extreme_fg, y_bg)
}

/// Analytic inverse of [`contrast_core`](crate::lpc): the clamp-inverted
/// foreground luminance `Y_hk` that yields `target` against `y_bg`.
fn invert_contrast(y_bg: f64, target: f64) -> Result<f64, Unreachable> {
    // Past the offset and clip, the smallest representable |Lc| is this floor;
    // targets inside the dead zone collapse to zero in the forward curve.
    let offset = if target > 0.0 {
        LO_BOW_OFFSET
    } else {
        LO_WOB_OFFSET
    };
    let lc_floor = (LO_CLIP - offset) * LC_SCALE;
    if target.abs() < lc_floor {
        return Err(Unreachable::BelowContrastFloor { target });
    }

    let bg_c = lpc::soft_clamp(y_bg);

    if target > 0.0 {
        // Normal polarity (dark-on-light): sapc = (bg^a − fg^b)·scale, then
        // Lc = (sapc − offset)·100. Solve for the clamped foreground fg_c.
        let sapc = target / LC_SCALE + LO_BOW_OFFSET;
        let base = bg_c.powf(EXP_BG_LIGHT);
        let max_achievable = max_lc(y_bg, target);
        let fg_pow = base - sapc / CONTRAST_SCALE; // = fg_c^EXP_FG_LIGHT
        if fg_pow < 0.0 {
            // Even a pure-black foreground cannot reach the target.
            return Err(Unreachable::ExceedsRange {
                target,
                max_achievable,
            });
        }
        let fg_c = fg_pow.powf(1.0 / EXP_FG_LIGHT);
        if fg_c > bg_c {
            // Foreground would have to be lighter than the background.
            return Err(Unreachable::PolarityMismatch { target });
        }
        lpc::soft_clamp_inv(fg_c).ok_or(Unreachable::ExceedsRange {
            target,
            max_achievable,
        })
    } else {
        // Reverse polarity (light-on-dark): Lc = (sapc + offset)·100, sapc < 0.
        let sapc = target / LC_SCALE - LO_WOB_OFFSET;
        let base = bg_c.powf(EXP_BG_DARK);
        let max_achievable = max_lc(y_bg, target);
        let fg_pow = base - sapc / CONTRAST_SCALE; // = fg_c^EXP_FG_DARK, > base
        let fg_c = fg_pow.powf(1.0 / EXP_FG_DARK);
        if fg_c > 1.0 {
            // Even a pure-white foreground cannot reach the target.
            return Err(Unreachable::ExceedsRange {
                target,
                max_achievable,
            });
        }
        if fg_c < bg_c {
            return Err(Unreachable::PolarityMismatch { target });
        }
        // fg_c ∈ [bg_c, 1] ≥ soft_clamp(0), so the clamp inverse always exists.
        lpc::soft_clamp_inv(fg_c).ok_or(Unreachable::ExceedsRange {
            target,
            max_achievable,
        })
    }
}

/// Recover the Oklab lightness whose display-encoded WCAG relative luminance
/// equals `target_ys`, applying `chroma_policy` at `hue`.
///
/// `Ys` runs from 0 at black to 1 at white and is monotone in `l_ok` along a
/// hue line under the policy's chroma profile, so the lightness endpoints
/// bracket the target and the bisection converges to the reproducing
/// lightness. Returns the Oklab lightness; the colour itself is built from it
/// via [`build_color`].
///
/// The search is **VC-free and CAM16-free**: WCAG luminance is a
/// display-domain quantity (ADR-0003 — ось читаемости в `Ys`), so each probe
/// costs one Oklab→sRGB conversion plus gamma-encode and a dot product. Это
/// сняло смысл grey-axis LUT (бывший `crate::lut`, удалён этой главой #64):
/// таблица существовала, чтобы не платить ~64 CAM16-форварда за бисекцию
/// `J_HK`; бисекция `Ys` дешевле обслуживания самой таблицы, а её
/// bit-identity-мост стал безпредметен вместе с доменом.
fn match_lightness_ys(target_ys: f64, hue: Hue, chroma_policy: ChromaPolicy) -> f64 {
    let ys_of = |l_ok: f64| {
        let rgb = build_color(l_ok, hue, chroma_policy);
        wcag::relative_luminance([srgb_gamma(rgb[0]), srgb_gamma(rgb[1]), srgb_gamma(rgb[2])])
    };
    cold_bisect(target_ys, ys_of)
}

/// Full-range `[0, 1]` lightness bisection over a monotone luminance curve.
/// 64 halvings take the interval far beyond the 8-bit output grid; endpoint
/// short-circuits return the boundary lightness for out-of-gamut targets.
fn cold_bisect(target: f64, curve_of: impl Fn(f64) -> f64) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    if target <= curve_of(lo) {
        return lo;
    }
    if target >= curve_of(hi) {
        return hi;
    }
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if curve_of(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

/// Build the in-gamut linear-sRGB colour at Oklab lightness `l_ok`, applying
/// `chroma_policy` at `hue`. Chroma is capped at [`max_chroma`], so the result
/// is always inside the sRGB gamut.
fn build_color(l_ok: f64, hue: Hue, chroma_policy: ChromaPolicy) -> [f64; 3] {
    if matches!(chroma_policy, ChromaPolicy::Neutral) {
        // Ахроматика — байт-точный серый ПО ПОСТРОЕНИЮ, не по float-совпадению.
        // Математика точная: при a = b = 0 инверсия Oklab даёт l' = m' = s' = L
        // (прибавляются точные нули), LMS = L³ поканально, а строки матрицы
        // LMS→linear-sRGB суммируются ровно в 1 — линейный серый есть L³ в
        // каждом канале. Прогон через матрицу вносил пер-строчную ошибку ~1 ulp
        // с РАЗНЫМ знаком по каналам; бисекция `apply_floor`, честно меряющая
        // квантованный hex, сходится ровно на байтовый обрыв (x.5/255), где эта
        // асимметрия расщепляет «серый» на целый байт: floored-tertiary на белом
        // эмитил #949595 (148,149,149; ratio 3.0036) вместо документированного
        // #949494. Один общий float на все три канала закрывает класс целиком:
        // любой обрыв флипает каналы синхронно.
        let v = (l_ok * l_ok * l_ok).clamp(0.0, 1.0);
        return [v, v, v];
    }
    let h = hue.degrees();
    let hr = h.to_radians();
    let chroma = match chroma_policy {
        ChromaPolicy::Neutral => 0.0,
        ChromaPolicy::Relative(ratio) => ratio * max_chroma(l_ok, h),
    };
    let lab = [l_ok, chroma * hr.cos(), chroma * hr.sin()];
    let rgb = oklab_to_srgb_linear(lab);
    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ]
}

/// Quantise the ideal colour to hex and report both contrasts it actually
/// achieves — what the caller gets, not the pre-quantisation ideal. The
/// perceptual `lc` is measured in `Ys` space (WCAG relative luminance of the
/// quantised display colour — ADR-0003) against `y_bg`; the legal `wcag_ratio`
/// on the same display colour against `bg_disp`. Обе метрики читают ОДНУ
/// люминансу — домен-мисматч оси читаемости закрыт конструкцией. The CAM16
/// forward remains solely for the returned [`LcsColor`] appearance correlates.
fn finish(
    rgb_ideal: [f64; 3],
    y_bg: f64,
    bg_disp: [f64; 3],
    floor_override: bool,
    vc: &ViewingConditions,
) -> Result<Solved, Unreachable> {
    let hex = hex_from_srgb(rgb_ideal);
    let rgb_quantised = srgb_from_hex(&hex).map_err(Unreachable::InvalidInput)?;
    let xyz = srgb_to_xyz(rgb_quantised);
    let (j, m, h) = crate::spaces::cam16::forward(xyz, vc);
    let color = LcsColor::from_cam16(j, m, h, oklab_hue(rgb_quantised));
    let disp = quantised_display(rgb_ideal);
    let y_fg = wcag::relative_luminance(disp);
    let lc = lpc::contrast_core(y_fg, y_bg);
    let wcag_ratio = wcag::contrast_ratio(disp, bg_disp);
    Ok(Solved {
        color,
        hex,
        lc,
        wcag_ratio,
        floor_override,
    })
}

/// Whether a measured signed perceptual contrast meets the (signed) floor within
/// the 1-Lc quantisation budget. The single comparison both endpoint checks
/// share: the governing endpoint passes its already-measured `solved.lc()` here
/// directly (no re-derivation), a distinct endpoint passes the contrast
/// [`meets_floor`] freshly measured for it.
fn meets_floor_lc(lc: f64, target: f64) -> bool {
    if target >= 0.0 {
        lc >= target - 1.0
    } else {
        lc <= target + 1.0
    }
}

/// Whether the solved colour still meets the (signed) perceptual floor at one
/// interval endpoint, within the 1-Lc quantisation budget. Trivial for a Solid
/// background (its endpoints coincide); the real guard for genuine luminance
/// intervals. Re-measures the contrast on the *quantised* hex — the value the
/// caller actually gets — so the gate reflects the emitted colour, not the
/// pre-quantisation ideal.
fn meets_floor(solved: &Solved, y_bg: f64, target: f64, _vc: &ViewingConditions) -> bool {
    let Ok(disp) = crate::spaces::srgb::srgb_encoded_from_hex(solved.hex()) else {
        // `solved.hex()` is produced by `hex_from_srgb`, so it always parses;
        // an unparsable hex here is a contradiction — treat it as not meeting.
        return false;
    };
    // `byte/255 == quantised_display(decode(byte))` точно (пин
    // `display_equals_quantised_display_on_every_byte`) — читаем ту же `Ys`,
    // которую замерил `finish`.
    let y_fg = wcag::relative_luminance(disp);
    let lc = lpc::contrast_core(y_fg, y_bg);
    meets_floor_lc(lc, target)
}

/// H-K-corrected luminance (`Y_hk`) of a linear-sRGB stimulus — воспринимаемая
/// ЯРКОСТЬ поверхности (Гельмгольц–Кольрауш, серый эквивалент `J_HK`).
///
/// После активации ADR-0003 (глава #64) ось читаемости это НЕ читает: контракт
/// контраста, полы и recheck меряются в `Ys`. Потребители — яркостная ось:
/// выбор стороны пары ([`crate::pair::pair_side`] — H-K-сдвиг кроссовера на
/// насыщенных фонах) и яркостные подсистемы.
pub(crate) fn bg_luma(rgb: [f64; 3], vc: &ViewingConditions) -> f64 {
    let j_hk = lpc::j_hk_from_xyz(srgb_to_xyz(rgb), vc).max(0.0);
    lpc::y_hk(j_hk, vc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_invariant_failure_is_not_reported_as_invalid_client_input() {
        let error = Unreachable::InternalInvariant("generated fixture drift".into());
        assert!(error.to_string().starts_with("internal invariant failure:"));
        assert!(!error.to_string().starts_with("invalid input:"));
    }
    use crate::lpc::lpc_with_vc;

    /// D2(a) страж (аудит 2026-07-03): `Unreachable` не несёт never-constructed
    /// вариантов-заготовок. `UnsupportedBackground` («future inputs», 0 точек
    /// конструирования по grep всех крейтов — только объявление + Display + wasm-
    /// маппинг) удалён; enum остаётся `#[non_exhaustive]`.
    ///
    /// Исчерпывающий `match` БЕЗ `_` внутри defining-крейта (там `#[non_exhaustive]`
    /// не требует wildcard): добавление варианта сломает компиляцию ИМЕННО ЗДЕСЬ,
    /// заставив автора обосновать конструируемость, а не осесть мёртвой заготовкой
    /// как `UnsupportedBackground`. Массив-образцы доказывают конструируемость
    /// каждого живого варианта и покрывают их `Display`.
    #[test]
    fn unreachable_carries_only_constructed_variants_with_display() {
        let samples = [
            Unreachable::BelowContrastFloor { target: 0.1 },
            Unreachable::ExceedsRange {
                target: 1.0,
                max_achievable: 0.5,
            },
            Unreachable::QuantizationGap {
                target: 1.0,
                nearest: 0.9,
            },
            Unreachable::FloorUnreachable {
                floor: 4.5,
                max_ratio: 2.0,
            },
            Unreachable::PolarityMismatch { target: 1.0 },
            Unreachable::GamutUnsupported,
            Unreachable::InvalidInput("x".to_string()),
            Unreachable::InternalInvariant("x".to_string()),
        ];
        for u in &samples {
            assert!(!u.to_string().is_empty(), "Display пуст для {u:?}");
            // Замок исчерпываемости: новый вариант non_exhaustive-enum обязан
            // пройти этот match (нет `_`), иначе компиляция здесь падает.
            match u {
                Unreachable::BelowContrastFloor { .. }
                | Unreachable::ExceedsRange { .. }
                | Unreachable::QuantizationGap { .. }
                | Unreachable::FloorUnreachable { .. }
                | Unreachable::PolarityMismatch { .. }
                | Unreachable::GamutUnsupported
                | Unreachable::InvalidInput(_)
                | Unreachable::InternalInvariant(_) => {}
            }
        }
    }

    const TOL: f64 = 1.0;
    const MAGNITUDES: [f64; 6] = [15.0, 30.0, 45.0, 60.0, 75.0, 90.0];

    fn vcs() -> [(ViewingConditions, &'static str); 2] {
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ]
    }

    #[test]
    fn cam16_forwards_per_set_regression_guard() {
        // DETERMINISTIC PERF METRIC (issue #19 / discrete-exactness). Wall-time on
        // a loaded machine is too noisy to measure a few-percent change, so the
        // honest before/after number is the count of CIECAM16 forward passes a
        // default `resolve_set` runs. This guard pins that count so a change that
        // re-introduces a duplicate forward — or legitimately removes one — fails
        // here until the table below is updated with intent.
        //
        // WHY TWO PINS PER (vc, bg). Post-#52 (undertone v2) a default set no
        // longer costs a single uniform number. v2 added a per-role curve plan:
        // for each role `curve_plan_cached` runs a cusp-attracted-hue scan
        // (Oklab-only — `max_chroma`, ZERO forwards) and a chroma-ratio bisection
        // `ratio_for_target_mp` (each `mp_at` probe is one `cam16::forward` via
        // `mp_of_linear_srgb` → `from_xyz_with_hok`). That bisection is the only
        // forward-heavy work the curve plan does, and it is the ONLY work the
        // thread-local `CURVE_PLAN_CACHE` memoises. So a set has two honest costs:
        //
        //   WARM — the runtime-dominant path. Curve plans already cached (a tool
        //          re-resolving as an unrelated setting is tweaked, or the same
        //          theme served repeatedly). The count is the IRREDUCIBLE per-role
        //          probe/finish + ResolveContext polarity/max work that is never
        //          cached. This is the number that governs steady-state cost; it
        //          gets the hard, low pin.
        //   COLD — the first resolve of a theme on a fresh cache. WARM plus every
        //          distinct curve-plan key's ratio bisection. The COLD−WARM delta
        //          (~520–560 forwards) is exactly the bisection work the cache
        //          elides on the second pass.
        //
        // The cache is reset before each COLD measurement so COLD is deterministic
        // regardless of test/iteration order; WARM is the immediate re-resolve of
        // the same theme, a verified fixed point. Counts measured on the merged
        // tree (main@#52 + perf/discrete-tables), 2026-06-12. They vary by
        // (vc, bg) because each surface reaches a different role mix with different
        // probe-sweep depths — real product behaviour, not noise.
        use crate::spaces::cam16::FORWARD_CALLS;
        let tbl = crate::RoleTable::default();

        // Measures `resolve_set_live` (the solver), not `resolve_set`: the latter
        // now serves a solid grey through the neutral O(1) fast path (zero
        // forwards), so it would not exercise the solver this guard exists to pin.

        // (vc name, bg hex) -> (cold forwards, warm forwards), measured.
        //
        // RE-MEASURED for the readability→`Ys` activation (глава #64, ADR-0003).
        // Ось читаемости покинула домен `Y_hk`: обратный солвер инвертирует `Ys`
        // напрямую (`match_lightness_ys`), и весь CAM16-раундтрип `grey_j ↔ y_hk`
        // на пути читаемости ОТПАЛ — это ровно предсказанный ADR blast radius
        // («solve.rs упростится … отпадает CAM16-раундтрип для читаемости»).
        // Оставшиеся форварды — работа ЯРКОСТНЫХ осей (нейтральная лестница,
        // сентимент, свечение), которые H-K сохраняют; потому dim/тёмные фоны
        // дороже (лестница в dim делает больше H-K-работы). Падение ~10-40×
        // против прежних пинов — не регрессия покрытия, а снятие лишнего домена.
        // Re-measured 2026-07-08 (глава #64 merge).
        let expected = [
            (("srgb", "#FFFFFF"), (103u64, 26u64)),
            (("srgb", "#7F7F7F"), (126, 24)),
            (("srgb", "#101012"), (129, 25)),
            (("dim", "#FFFFFF"), (128, 29)),
            (("dim", "#7F7F7F"), (144, 25)),
            (("dim", "#101012"), (192, 30)),
        ];

        for (vc, name) in vcs() {
            for bg in ["#FFFFFF", "#7F7F7F", "#101012"] {
                let &(_, (cold_exp, warm_exp)) = expected
                    .iter()
                    .find(|((n, b), _)| *n == name && *b == bg)
                    .expect("every (vc, bg) pair has a pinned expectation");
                let bgi = crate::BgInput::solid(bg).unwrap();

                // COLD: fresh cache, first resolve of this theme.
                crate::semantic::reset_curve_plan_cache();
                FORWARD_CALLS.with(|c| c.set(0));
                let _ = crate::semantic::resolve_set_live(&bgi, &tbl, &vc);
                let cold = FORWARD_CALLS.with(|c| c.get());
                assert_eq!(
                    cold, cold_exp,
                    "{name}/{bg}: COLD CAM16 forwards/set = {cold}, expected {cold_exp}"
                );

                // WARM: same theme re-resolved, curve plans now cached.
                FORWARD_CALLS.with(|c| c.set(0));
                let _ = crate::semantic::resolve_set_live(&bgi, &tbl, &vc);
                let warm = FORWARD_CALLS.with(|c| c.get());
                assert_eq!(
                    warm, warm_exp,
                    "{name}/{bg}: WARM CAM16 forwards/set = {warm}, expected {warm_exp}"
                );
            }
        }
    }

    /// Independent re-measure of an emitted hex's signed perceptual contrast in
    /// the READABILITY domain (`Ys`) the solver targets since глава #64. Заменяет
    /// Y_hk-мерило `lpc_with_vc` в round-trip проверках ВЕЛИЧИНЫ (сверять надо в
    /// домене цели; signum-проверки домен-агностичны и остаются на `lpc_with_vc`).
    fn readability_lc(fg_hex: &str, bg_hex: &str) -> f64 {
        let fg = crate::spaces::srgb::srgb_encoded_from_hex(fg_hex).expect("valid emitted hex");
        let bg = crate::spaces::srgb::srgb_encoded_from_hex(bg_hex).expect("valid bg hex");
        crate::lpc::lpc_readability_ys(fg, bg)
    }

    /// Solve and return both the solved value and the contrast measured
    /// independently on the resolved hex — in the SAME readability domain the
    /// solver now targets (`Ys`, ADR-0003 глава #64): `lpc_readability_ys` on the
    /// display bytes. Меряться через `lpc_with_vc` (домен `Y_hk`, apparent
    /// contrast) здесь БОЛЬШЕ НЕЛЬЗЯ: ось читаемости переехала в `Ys`, и
    /// round-trip обязан сверяться в домене цели, иначе Ys≈Y_hk-разрыв на серости
    /// (CAM16-лайтнесс ≠ WCAG-люминанс даже при C=0) даёт ложный недолёт. Третий
    /// ассерт вызывающих (`solved.lc() == measured` до 1e-9) фиксирует именно это:
    /// `solved.lc()` — Ys-замер `finish`, и независимый пересчёт обязан совпасть.
    fn solve_and_measure(
        bg_hex: &str,
        target: f64,
        vc: &ViewingConditions,
    ) -> Result<(Solved, f64), Unreachable> {
        let bg = BgInput::solid(bg_hex)?;
        // Floor::None: these helpers exercise the pure perceptual inversion;
        // the WCAG floor (on by default for text) is tested separately.
        let solved = solve(
            bg,
            Contract::text(target).with_conformance(Floor::None),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            vc,
            Gamut::Srgb,
        )?;
        let fg_disp = crate::spaces::srgb::srgb_encoded_from_hex(solved.hex())
            .expect("solved hex is produced by hex_from_srgb → always parses");
        let bg_disp = crate::spaces::srgb::srgb_encoded_from_hex(bg_hex)
            .expect("test bg hex is a valid literal");
        let measured = crate::lpc::lpc_readability_ys(fg_disp, bg_disp);
        Ok((solved, measured))
    }

    #[test]
    fn round_trip_normal_polarity_on_white() {
        // Dark-on-light: positive target against white, both viewing conditions.
        for (vc, vc_name) in vcs() {
            for target in MAGNITUDES {
                let (solved, measured) =
                    solve_and_measure("#FFFFFF", target, &vc).expect("white must support +Lc");
                assert!(
                    (measured - target).abs() <= TOL,
                    "{vc_name}: target {target}, measured {measured}, hex {}",
                    solved.hex()
                );
                assert!(
                    measured > 0.0,
                    "normal polarity must be positive: {measured}"
                );
                // The reported lc must match an independent measurement.
                assert!(
                    (solved.lc() - measured).abs() < 1e-9,
                    "reported lc {} vs measured {measured}",
                    solved.lc()
                );
            }
        }
    }

    #[test]
    fn round_trip_reverse_polarity_on_dark() {
        // Light-on-dark: negative target against a near-black background.
        for (vc, vc_name) in vcs() {
            for magnitude in MAGNITUDES {
                let target = -magnitude;
                let (solved, measured) =
                    solve_and_measure("#101012", target, &vc).expect("dark bg must support -Lc");
                assert!(
                    (measured - target).abs() <= TOL,
                    "{vc_name}: target {target}, measured {measured}, hex {}",
                    solved.hex()
                );
                assert!(
                    measured < 0.0,
                    "reverse polarity must be negative: {measured}"
                );
            }
        }
    }

    #[test]
    fn property_grid_neutral_and_chromatic_backgrounds() {
        // Grid: neutral + chromatic backgrounds × both polarities × both VCs ×
        // the full magnitude grid. Every reachable target lands within 1 Lc;
        // every unreachable one returns a principled reason, never a clip.
        let backgrounds = [
            "#FFFFFF", "#E8E8E8", "#BFBFBF", "#5A5A5A", "#101012", // neutrals
            "#3478F6", "#1E7D32", "#F2B8C6", "#0A3D62", // chromatic light + dark
        ];
        let mut ok_count = 0_usize;
        let mut max_err = 0.0_f64;
        for (vc, vc_name) in vcs() {
            for bg_hex in backgrounds {
                for magnitude in MAGNITUDES {
                    for target in [magnitude, -magnitude] {
                        match solve_and_measure(bg_hex, target, &vc) {
                            Ok((solved, measured)) => {
                                ok_count += 1;
                                let err = (measured - target).abs();
                                max_err = max_err.max(err);
                                assert!(
                                    err <= TOL,
                                    "{vc_name} {bg_hex}: target {target}, measured {measured}, hex {}",
                                    solved.hex()
                                );
                                assert_eq!(
                                    target > 0.0,
                                    measured > 0.0,
                                    "polarity sign mismatch: target {target}, measured {measured}"
                                );
                            }
                            Err(Unreachable::InvalidInput(msg)) => {
                                panic!("unexpected invalid input for {bg_hex}/{target}: {msg}")
                            }
                            // Out-of-range / wrong-polarity / dead-zone targets are
                            // legitimately unreachable for some bg+polarity pairs.
                            Err(_) => {}
                        }
                    }
                }
            }
        }
        eprintln!("property grid: {ok_count} reachable, max |Lc - target| = {max_err:.4}");
        assert!(max_err <= TOL, "max error {max_err} exceeds {TOL}");
        assert!(
            ok_count >= 60,
            "grid exercised too few reachable combos: {ok_count}"
        );
    }

    #[test]
    fn default_wcag_floor_preserves_polarity_sign() {
        // The property grid above proves sign preservation under `Floor::None`
        // (pure perceptual inversion). The PRODUCTION default for text is the AA
        // WCAG floor, which may RAISE a too-weak |Lc| to the legal minimum — a
        // separate code path (`apply_floor`). This pins the safety invariant on
        // THAT path: the floor override may strengthen contrast but must never
        // flip the foreground to the wrong side of the background. Weak targets
        // (15/30/45 Lc, below the AA floor) force the override branch so the test
        // is not a vacuous re-run of the no-floor grid.
        let backgrounds = [
            "#FFFFFF", "#E8E8E8", "#5A5A5A", "#101012", // neutrals
            "#3478F6", "#0A3D62", // chromatic light + dark
        ];
        let mut reachable = 0_usize;
        // Count override exercise per polarity: a single global counter could be
        // satisfied by one side alone, leaving the other polarity's override path
        // unguarded. We require BOTH below.
        let mut overridden_pos = 0_usize;
        let mut overridden_neg = 0_usize;
        for (vc, vc_name) in vcs() {
            for bg_hex in backgrounds {
                for magnitude in MAGNITUDES {
                    for target in [magnitude, -magnitude] {
                        // Fresh per solve: `solve` consumes the `BgInput`.
                        let bg = BgInput::solid(bg_hex).unwrap();
                        // Default constructor → Floor::AaText (no with_conformance).
                        let solved = match solve(
                            bg,
                            Contract::text(target),
                            Hue::deg(0.0),
                            ChromaPolicy::Neutral,
                            &vc,
                            Gamut::Srgb,
                        ) {
                            Ok(s) => s,
                            // Wrong-polarity / out-of-range for this bg are
                            // legitimately unreachable — skip, never a clip.
                            Err(_) => continue,
                        };
                        reachable += 1;
                        // Independently re-measure the emitted hex's signed Lc.
                        let measured = lpc_with_vc(solved.hex(), bg_hex, &vc);
                        // Compare signum, not `> 0.0`. Under the AA text floor every
                        // reachable result clears 4.5:1, so `measured` is never the
                        // dead-zone zero here — this is belt-and-suspenders. f64
                        // signum is ±1.0 by sign bit (not 0.0 for a zero), so it
                        // still tightens the one seam a bare `measured > 0.0` missed:
                        // a negative target whose measurement collapsed to +0.0 now
                        // mismatches (-1.0 vs +1.0) instead of passing as
                        // `false == false`. target.signum() is always ±1.0 (target
                        // is ±magnitude, never 0).
                        assert_eq!(
                            target.signum(),
                            measured.signum(),
                            "{vc_name} {bg_hex}: default WCAG floor broke polarity — \
                             target {target}, measured {measured}, hex {}",
                            solved.hex()
                        );
                        // Override detection: a weak target (well under the AA text
                        // floor — 15/30/45 Lc all sit below the ~4.5:1 legal minimum,
                        // wcag::AA_TEXT_RATIO) that the floor lifted past its request,
                        // same sign. The `magnitude < 60` gate excludes already-strong
                        // targets (which the floor leaves alone), so a large-but-not-
                        // overridden result cannot be miscounted; the `5.0 * TOL`
                        // margin clears the ±TOL (1 Lc) quantisation tolerance fivefold.
                        if magnitude < 60.0 && measured.abs() > magnitude + 5.0 * TOL {
                            if target > 0.0 {
                                overridden_pos += 1;
                            } else {
                                overridden_neg += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(reachable >= 20, "too few reachable combos: {reachable}");
        // Both polarity override paths must be exercised, or the test is not
        // guarding the invariant it claims on one side.
        assert!(
            overridden_pos > 0 && overridden_neg > 0,
            "floor override not exercised on both polarities (pos {overridden_pos}, neg {overridden_neg}) — \
             test would be vacuous on one side"
        );
    }

    #[test]
    fn below_contrast_floor_is_unreachable() {
        // Inside the loClip dead zone: the forward curve reports zero, so no
        // colour can reproduce it.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let err = solve(
            bg,
            Contract::text(3.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        assert!(
            matches!(err, Unreachable::BelowContrastFloor { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn beyond_gamut_is_unreachable_not_clipped() {
        // White can supply at most ~106 Lc (black foreground); 120 is impossible.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let err = solve(
            bg,
            Contract::text(120.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        assert!(matches!(err, Unreachable::ExceedsRange { .. }), "{err:?}");
    }

    #[test]
    fn high_positive_target_on_dark_background_is_unreachable() {
        // A dark background cannot host a strong dark-on-light contrast: the
        // foreground would have to be darker than black.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#101012").unwrap();
        let err = solve(
            bg,
            Contract::text(60.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                Unreachable::ExceedsRange { .. } | Unreachable::PolarityMismatch { .. }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn dj_degradation_reports_honest_achieved_dj() {
        // Закон 2 ADR-0002: цель за стеной оси J' деградирует к ближайшему
        // достижимому с флагом. `achieved_dj` обязан быть ЗАМЕРОМ на отданном
        // hex (та же честность, что glow.degraded): перечитываем hex и
        // сверяем |ΔJ'| против фона независимо.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#101012").unwrap();
        let d = solve_dj(&bg, 300.0, -1.0, Hue::deg(0.0), ChromaPolicy::Neutral, &vc)
            .expect("degradation returns Ok, not Err");
        assert!(d.degraded, "300 J' на почти-чёрном обязан деградировать");
        assert!(
            d.achieved_dj < 300.0,
            "стена оси ниже цели: achieved {:.2}",
            d.achieved_dj
        );
        // Независимый перезамер на отданном hex.
        let fg = srgb_from_hex(d.solved.hex()).unwrap();
        let bg_disp = bg.governing_display(-1.0);
        let bg_lin = [
            srgb_gamma_inv(bg_disp[0]),
            srgb_gamma_inv(bg_disp[1]),
            srgb_gamma_inv(bg_disp[2]),
        ];
        let measured = (jp_of_linear(fg, &vc) - jp_of_linear(bg_lin, &vc)).abs();
        assert!(
            (measured - d.achieved_dj).abs() < 1e-9,
            "achieved_dj {:.6} must equal the re-measured |dJ'| {:.6} on the emitted hex",
            d.achieved_dj,
            measured
        );

        // Парный контроль: достижимая ступень — точное решение без флага.
        let ok = solve_dj(&bg, 10.0, -1.0, Hue::deg(0.0), ChromaPolicy::Neutral, &vc)
            .expect("in-budget dJ' solves");
        assert!(!ok.degraded);
        assert!(
            (ok.achieved_dj - 10.0).abs() <= DJ_BUDGET,
            "in-budget achieved {:.3}",
            ok.achieved_dj
        );
    }

    #[test]
    fn display_p3_gamut_is_reserved_not_implemented() {
        // SEAM (c): the P3 variant exists in the type but returns a real error,
        // never a panic and never a silent sRGB fallback.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let err = solve(
            bg,
            Contract::text(60.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::DisplayP3,
        )
        .unwrap_err();
        assert_eq!(err, Unreachable::GamutUnsupported);
    }

    #[test]
    fn degenerate_range_matches_explicit_target() {
        // SEAM (b): a degenerate range [t, t] solves identically to text(t).
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let from_text = solve(
            bg.clone(),
            Contract::text(60.0).with_conformance(Floor::None),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        let from_range = solve(
            bg,
            Contract::range(60.0, 60.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert_eq!(from_text, from_range);
    }

    #[test]
    fn reserved_typography_does_not_change_the_result() {
        // SEAM (c): the typographic context is reserved; the v1 solver ignores
        // it and the caller's explicit target governs.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let plain = solve(
            bg.clone(),
            Contract::text(60.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        let with_ctx = solve(
            bg,
            Contract::text(60.0).with_typography(TypographicContext {
                size_px: 32.0,
                weight: 700,
            }),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert_eq!(plain, with_ctx);
    }

    #[test]
    fn chromatic_foreground_hits_target_and_carries_chroma() {
        // A saturated foreground policy still lands on the contrast target,
        // because the H-K boost is compensated by lowering lightness.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let target = 45.0;
        let solved = solve(
            bg,
            Contract::text(target).with_conformance(Floor::None),
            Hue::deg(264.0), // Oklab blue
            ChromaPolicy::Relative(0.8),
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        let measured = readability_lc(solved.hex(), "#FFFFFF");
        assert!(
            (measured - target).abs() <= TOL,
            "chromatic target {target}, measured {measured}, hex {}",
            solved.hex()
        );
        assert!(
            solved.color().s > 0.01,
            "chromatic policy should carry chroma, s = {}",
            solved.color().s
        );
    }

    #[test]
    fn solid_background_reduces_to_a_degenerate_interval() {
        // SEAM (a): every background reduces to a Y_hk interval; a Solid colour
        // is the degenerate interval [Y, Y]. `solve` only ever consumes the
        // interval (never matches BgInput variants), so future composite /
        // distribution variants — enabled by `#[non_exhaustive]` — extend
        // `luma_interval` alone, leaving `solve`'s signature untouched.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let interval = bg.luma_interval(&vc).unwrap();
        assert_eq!(
            interval.lo, interval.hi,
            "a solid background must be a single-point luminance interval"
        );
        // The contract is checked at both (here identical) endpoints.
        assert_eq!(interval.endpoints(), [interval.lo, interval.hi]);
    }

    #[test]
    fn invalid_hex_background_is_rejected() {
        let err = BgInput::solid("#xyz").unwrap_err();
        assert!(matches!(err, Unreachable::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn aa_text_floor_holds_across_grid() {
        // Dual gate: every solvable text contract with the default AA floor
        // emits a colour whose WCAG 2.1 ratio — recomputed from the hex via the
        // spec formula — clears 4.5:1, and the reported ratio matches it.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#E8E8E8", "#101012", "#0A3D62"] {
                for target in [15.0, 30.0, 45.0, 60.0, 75.0, 90.0, -15.0, -45.0, -75.0] {
                    for (contract, min_ratio) in [
                        (Contract::text(target), crate::wcag::AA_TEXT_RATIO),
                        (Contract::ui(target), crate::wcag::AA_UI_RATIO),
                    ] {
                        let bg = BgInput::solid(bg_hex).unwrap();
                        let res = solve(
                            bg,
                            contract,
                            Hue::deg(0.0),
                            ChromaPolicy::Neutral,
                            &vc,
                            Gamut::Srgb,
                        );
                        if let Ok(solved) = res {
                            let fg = srgb_from_hex(solved.hex()).unwrap();
                            let bgc = srgb_from_hex(bg_hex).unwrap();
                            let ratio = crate::wcag::contrast_ratio(
                                quantised_display(fg),
                                quantised_display(bgc),
                            );
                            assert!(
                                ratio >= min_ratio - 1e-9,
                                "{vc_name} {bg_hex} t={target} floor {min_ratio}: ratio {ratio}, hex {}",
                                solved.hex()
                            );
                            assert!(
                                (solved.wcag_ratio() - ratio).abs() < 1e-9,
                                "reported ratio {} vs recomputed {ratio}",
                                solved.wcag_ratio()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn floor_overrides_a_weak_perceptual_target() {
        // Conflict case: Lc 15 text on white is far below 4.5:1 — the law wins,
        // the colour is pushed darker and the override is flagged.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let solved = solve(
            bg,
            Contract::text(15.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(solved.floor_override(), "floor must override Lc 15");
        assert!(solved.wcag_ratio() >= 4.5 - 1e-9);
        let measured = readability_lc(solved.hex(), "#FFFFFF");
        assert!(
            measured > 15.0,
            "pushed darker means more contrast, got {measured}"
        );
    }

    #[test]
    fn neutral_policy_emits_byte_exact_grey_everywhere() {
        // Класс-инвариант ахроматики: ChromaPolicy::Neutral обязан эмитить
        // байт-точный серый (R==G==B) на ЛЮБОМ достижимом контракте — включая
        // подъём законным полом и цели на границе округления байта. Регресс
        // главы #64 (снос grey-LUT): ахроматика пошла через матричный
        // roundtrip Oklab→sRGB, чья микро-асимметрия каналов у целей на
        // ~x.5/255 расщепляет серый (#949595 у floored-tertiary на белом,
        // Y=0.30 → 148.5/255). Инвариант держится конструкцией, не допуском.
        let vc = ViewingConditions::srgb();
        let mut reachable = 0;
        let mut floored = 0;
        for bg_hex in ["#FFFFFF", "#F4F4F4", "#767676", "#101012", "#000000"] {
            for sign in [1.0_f64, -1.0] {
                let mut m = 5.0_f64;
                while m <= 100.0 {
                    for ui in [false, true] {
                        let bg = BgInput::solid(bg_hex).unwrap();
                        let contract = if ui {
                            Contract::ui(sign * m)
                        } else {
                            Contract::text(sign * m)
                        };
                        if let Ok(solved) = solve(
                            bg,
                            contract,
                            Hue::deg(0.0),
                            ChromaPolicy::Neutral,
                            &vc,
                            Gamut::Srgb,
                        ) {
                            reachable += 1;
                            floored += solved.floor_override() as u32;
                            let hex = solved.hex();
                            assert!(
                                hex[1..3] == hex[3..5] && hex[3..5] == hex[5..7],
                                "{bg_hex} target {:+.1} (ui={ui}): Neutral эмитит не-серый {hex}",
                                sign * m
                            );
                        }
                    }
                    m += 2.5;
                }
            }
        }
        // Свип обязан реально проходить и обычный, и floored-режим — иначе
        // тест не сторожит заявленный инвариант.
        assert!(reachable >= 100, "too few reachable combos: {reachable}");
        assert!(floored > 0, "floor-lift режим не задет свипом");

        // Прицельный обрыв: якорный таргет tertiary (0.47572199·max ≈ Lc 50.45,
        // пол 3:1 на белом) кладёт бисекцию `apply_floor` ровно на байтовую
        // границу 148.5/255, где 149-серый ещё падает (2.996 < 3), а флип ОДНОГО
        // канала уже проходит (148,149,149 → 3.0036). Свип шагом 2.5 эту цель
        // минует — регресс #949595 ловится только точным таргетом.
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let cliff = solve(
            bg,
            Contract::ui(50.4459),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(
            cliff.floor_override(),
            "пол 3:1 обязан включиться на Lc 50.45 (белый фон)"
        );
        let hex = cliff.hex();
        assert!(
            hex[1..3] == hex[3..5] && hex[3..5] == hex[5..7],
            "floored-tertiary на белом: Neutral эмитит не-серый {hex}"
        );
    }

    #[test]
    fn ui_floor_is_three_to_one() {
        // The UI floor (3:1) is laxer than the text floor (4.5:1): both push a
        // weak target, but the UI colour keeps a lower ratio.
        let vc = ViewingConditions::srgb();
        let ui = solve(
            BgInput::solid("#FFFFFF").unwrap(),
            Contract::ui(15.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(ui.floor_override());
        assert!(ui.wcag_ratio() >= 3.0 - 1e-9);
        let text = solve(
            BgInput::solid("#FFFFFF").unwrap(),
            Contract::text(15.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(ui.wcag_ratio() < text.wcag_ratio());
    }

    #[test]
    fn decorative_contracts_skip_the_floor() {
        // JND/decorative: range carries Floor::None — perception governs, no
        // flag, and the (sub-AA) ratio is still reported for transparency.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let solved = solve(
            bg,
            Contract::range(15.0, 15.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(!solved.floor_override());
        let measured = readability_lc(solved.hex(), "#FFFFFF");
        assert!((measured - 15.0).abs() <= TOL);
        assert!(solved.wcag_ratio() < 4.5);
    }

    #[test]
    fn satisfied_floor_leaves_perception_in_charge() {
        // Lc 90 on white clears 4.5:1 on its own — no override flag, target met.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let solved = solve(
            bg,
            Contract::text(90.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap();
        assert!(!solved.floor_override());
        assert!(solved.wcag_ratio() >= 4.5);
        let measured = readability_lc(solved.hex(), "#FFFFFF");
        assert!((measured - 90.0).abs() <= TOL);
    }

    #[test]
    fn unreachable_floor_is_a_principled_error() {
        // On a mid-dark background even pure black cannot reach 4.5:1, so a
        // dark-on-light text contract fails loudly rather than under-delivering.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#6E6E6E").unwrap();
        let err = solve(
            bg,
            Contract::text(30.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        match err {
            Unreachable::FloorUnreachable { floor, max_ratio } => {
                assert!((floor - 4.5).abs() < 1e-9, "floor {floor}");
                assert!(max_ratio < 4.5, "max_ratio {max_ratio}");
            }
            other => panic!("expected FloorUnreachable, got {other:?}"),
        }
    }

    #[test]
    fn non_finite_hue_is_rejected() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = solve(
                bg.clone(),
                Contract::text(60.0),
                Hue::deg(bad),
                ChromaPolicy::Relative(1.0),
                &vc,
                Gamut::Srgb,
            )
            .unwrap_err();
            assert!(matches!(err, Unreachable::InvalidInput(_)), "{err:?}");
        }
    }

    #[test]
    fn chroma_ratio_outside_declared_unit_interval_is_rejected() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        for bad in [
            f64::NEG_INFINITY,
            -1.0,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
            2.0,
            f64::INFINITY,
            f64::NAN,
        ] {
            let err = solve(
                bg.clone(),
                Contract::text(60.0),
                Hue::deg(250.0),
                ChromaPolicy::Relative(bad),
                &vc,
                Gamut::Srgb,
            )
            .unwrap_err();
            assert!(matches!(err, Unreachable::InvalidInput(_)), "{err:?}");
        }

        for edge in [0.0, 1.0] {
            let result = solve(
                bg.clone(),
                Contract::text(60.0),
                Hue::deg(250.0),
                ChromaPolicy::Relative(edge),
                &vc,
                Gamut::Srgb,
            );
            assert!(
                result.is_ok(),
                "граница ratio={edge} обязана быть допустимой: {result:?}"
            );
        }
    }

    #[test]
    fn solve_many_validates_each_chroma_ratio_without_shifting_positions() {
        let vc = ViewingConditions::srgb();
        let jobs = [
            SolveJob {
                contract: Contract::text(60.0),
                hue: Hue::deg(250.0),
                chroma_policy: ChromaPolicy::Relative(0.0),
            },
            SolveJob {
                contract: Contract::text(60.0),
                hue: Hue::deg(250.0),
                chroma_policy: ChromaPolicy::Relative(-f64::EPSILON),
            },
            SolveJob {
                contract: Contract::text(60.0),
                hue: Hue::deg(250.0),
                chroma_policy: ChromaPolicy::Relative(1.0 + f64::EPSILON),
            },
            SolveJob {
                contract: Contract::text(60.0),
                hue: Hue::deg(250.0),
                chroma_policy: ChromaPolicy::Relative(1.0),
            },
        ];

        let results = solve_many(BgInput::solid("#FFFFFF").unwrap(), &jobs, &vc, Gamut::Srgb)
            .expect("валидный общий фон не должен ронять весь batch");

        assert_eq!(results.len(), jobs.len());
        assert!(results[0].is_ok(), "первая валидная job потеряна");
        assert!(
            matches!(&results[1], Err(Unreachable::InvalidInput(_))),
            "отрицательный ratio обязан остаться ошибкой в своей позиции"
        );
        assert!(
            matches!(&results[2], Err(Unreachable::InvalidInput(_))),
            "ratio выше единицы обязан остаться ошибкой в своей позиции"
        );
        assert!(results[3].is_ok(), "последняя валидная job потеряна");
    }

    #[test]
    fn exceeds_range_reports_the_true_forward_curve_maximum() {
        // Normal polarity on white: the most the background can supply is the
        // canonical black-on-white value, not the un-clamped analytic bound.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let err = solve(
            bg,
            Contract::text(120.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        match err {
            Unreachable::ExceedsRange { max_achievable, .. } => {
                let black_on_white = crate::lpc::lpc_with_vc("#000000", "#FFFFFF", &vc);
                assert!(
                    (max_achievable - black_on_white).abs() < 0.5,
                    "max_achievable {max_achievable} should match the forward                      curve extreme {black_on_white}"
                );
            }
            other => panic!("expected ExceedsRange, got {other:?}"),
        }
    }

    #[test]
    fn reverse_polarity_max_on_a_light_background_is_not_positive() {
        // Light-on-light has no reverse headroom: the diagnostic must not
        // advertise a positive "maximum" for a negative-polarity target.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let err = solve(
            bg,
            Contract::text(-50.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        match err {
            Unreachable::ExceedsRange { max_achievable, .. } => assert!(
                max_achievable <= 0.0,
                "reverse-polarity max on white must be <= 0, got {max_achievable}"
            ),
            other => panic!("expected ExceedsRange, got {other:?}"),
        }
    }

    #[test]
    fn quantization_gap_target_resolves_via_neighbor_step() {
        // Issue #44: target Lc 7.31 on white. The analytic foreground quantises
        // to a hex inside the low-contrast dead zone (Lc 0); the floor sits at
        // ≈7.3 and the first valid darker grid step is #EDEDED (Lc ≈ 7.604 в
        // домене `Ys`), within the ±1 budget. The neighbour walk must find it
        // instead of returning a (lying) ExceedsRange — механизм issue #44 жив.
        // ГЛАВА #64 (ADR-0003): ось читаемости перешла в `Ys`, и мерило `Lc`
        // hex'а пересчиталось — прежний Y_hk-шаг #E9E9E9 (Lc ≈ 7.85) сменился на
        // #EDEDED (Lc ≈ 7.604). Между полом 7.3 и #EDEDED валидных шагов нет,
        // потому цель 7.31 ГАРАНТИРОВАННО падает в мёртвую зону и обслуживается
        // именно neighbour-walk'ом (не аналитикой) — тест не выхолостился.
        let vc = ViewingConditions::srgb();
        let (solved, measured) =
            solve_and_measure("#FFFFFF", 7.31, &vc).expect("7.31 on white is on-grid reachable");
        assert!(
            (measured - 7.31).abs() <= TOL,
            "target 7.31, measured {measured}, hex {}",
            solved.hex()
        );
        assert_eq!(
            solved.hex(),
            "#EDEDED",
            "expected the first darker on-grid step, got {}",
            solved.hex()
        );
        // The reported lc must match an independent re-measurement on the hex.
        assert!(
            (solved.lc() - measured).abs() < 1e-9,
            "reported lc {} vs measured {measured}",
            solved.lc()
        );
    }

    #[test]
    fn quantization_band_is_fully_resolvable_or_honestly_unreachable() {
        // Scan the JND band (7.3, 7.6) at 0.05 on white (dark-on-light, +Lc) and
        // black (light-on-dark, −Lc). The analytic low-contrast floor sits at
        // exactly 7.3 Lc ((LO_CLIP − offset)·100), so |target| == 7.3 is the
        // honest BelowContrastFloor dead zone; the quantisation gap lives just
        // above it. Inside the band every target must either resolve within ±1
        // Lc, or fail with a *principled* variant — BelowContrastFloor (analytic
        // dead zone) or QuantizationGap (on-grid near-miss) — but NEVER the
        // misleading ExceedsRange, and never a silent clip. Prints a before/after
        // table for the PR.
        let vc = ViewingConditions::srgb();
        let mut t = 7.30_f64;
        let mut resolved = 0_usize;
        let mut gapped = 0_usize;
        let mut below = 0_usize;
        eprintln!("band scan (7.3, 7.6) step 0.05:");
        eprintln!("  target |  white +Lc                 |  black -Lc");
        while t <= 7.60 + 1e-9 {
            let pos = solve_and_measure("#FFFFFF", t, &vc);
            let neg = solve_and_measure("#000000", -t, &vc);

            let describe = |r: &Result<(Solved, f64), Unreachable>| match r {
                Ok((s, m)) => format!("Ok {} Lc {m:+.3}", s.hex()),
                Err(Unreachable::QuantizationGap { nearest, .. }) => {
                    format!("Gap (nearest {nearest:.3})")
                }
                Err(Unreachable::BelowContrastFloor { .. }) => "BelowFloor".to_string(),
                Err(e) => format!("ERR {e:?}"),
            };
            eprintln!("  {t:>6.2} |  {:<26}|  {}", describe(&pos), describe(&neg));

            for (sign, r) in [(1.0_f64, &pos), (-1.0_f64, &neg)] {
                let target = sign * t;
                match r {
                    Ok((solved, measured)) => {
                        resolved += 1;
                        assert!(
                            (measured - target).abs() <= TOL,
                            "band target {target}: measured {measured}, hex {}",
                            solved.hex()
                        );
                        assert_eq!(
                            target > 0.0,
                            *measured > 0.0,
                            "band polarity mismatch: target {target}, measured {measured}"
                        );
                    }
                    Err(Unreachable::QuantizationGap {
                        target: et,
                        nearest,
                    }) => {
                        gapped += 1;
                        assert!((et - target).abs() < 1e-9, "echoed target {et} vs {target}");
                        assert!(
                            nearest.is_finite() && *nearest >= 0.0,
                            "gap near-miss must be a real magnitude, got {nearest}"
                        );
                    }
                    // Honest analytic dead zone at |Lc| <= 7.3 — a different
                    // mechanism from the quantisation gap, and not a clip.
                    Err(Unreachable::BelowContrastFloor { target: et }) => {
                        below += 1;
                        assert!((et - target).abs() < 1e-9, "echoed target {et} vs {target}");
                    }
                    // No other outcome is acceptable inside this band: ExceedsRange
                    // here would be the very semantic lie issue #44 is about.
                    Err(other) => panic!("band target {target}: unexpected {other:?}"),
                }
            }
            t += 0.05;
        }
        eprintln!(
            "band scan: {resolved} resolved, {gapped} honest quant gaps, {below} below-floor"
        );
        // The whole point of the fix: the issue-#44 white case is now resolvable.
        assert!(
            resolved >= 1,
            "expected at least one band target to resolve"
        );
    }

    #[test]
    fn neighbor_acceptance_respects_the_symmetric_budget() {
        // The neighbour walk moves *away* from the target toward larger |Lc|, so
        // a returned colour must still land within ±1 on BOTH sides — never an
        // overshoot that satisfies only the lower floor. Sweep the whole gap band
        // densely on white and black; every resolved colour must be symmetric-in
        // budget, and its reported lc must match an independent measurement.
        let vc = ViewingConditions::srgb();
        let mut t = 7.31_f64;
        let mut checked = 0_usize;
        while t <= 7.59 + 1e-9 {
            for (bg, target) in [("#FFFFFF", t), ("#000000", -t)] {
                if let Ok((solved, measured)) = solve_and_measure(bg, target, &vc) {
                    checked += 1;
                    // Symmetric budget — the guard this protects: a
                    // one-sided "not below floor" check would let an overshoot in.
                    assert!(
                        (measured - target).abs() <= TOL,
                        "{bg} t={target}: measured {measured} outside ±{TOL}, hex {}",
                        solved.hex()
                    );
                    assert!(
                        (solved.lc() - measured).abs() < 1e-9,
                        "{bg} t={target}: reported lc {} vs measured {measured}",
                        solved.lc()
                    );
                }
            }
            t += 0.01;
        }
        assert!(checked >= 1, "expected at least one resolvable target");
    }

    #[test]
    fn quantization_gap_error_is_honest_not_exceeds_range() {
        // The QuantizationGap variant must report a real near-miss magnitude and
        // render a message that names the gap — distinct from ExceedsRange, which
        // would falsely blame the background. Construct one directly to lock the
        // contract (the scan above exercises the live path).
        let err = Unreachable::QuantizationGap {
            target: 7.45,
            nearest: 7.85,
        };
        let msg = err.to_string();
        assert!(msg.contains("quantisation gap"), "message: {msg}");
        assert!(msg.contains("7.45"), "message must echo the target: {msg}");
        assert_ne!(
            err,
            Unreachable::ExceedsRange {
                target: 7.45,
                max_achievable: 7.85,
            },
            "the two variants must be distinguishable"
        );
    }

    // ── `match_lightness_ys`: инверсия оси читаемости (ADR-0003, глава #64) ──

    /// Round-trip lock on the `Ys` matcher that replaced the CAM16 `J_HK`
    /// bisection + grey-axis LUT (глава #64). For a dense lightness grid along
    /// both production chroma profiles, the WCAG relative luminance of the
    /// built colour must be recovered to bisection precision, and out-of-gamut
    /// targets must short-circuit to the gamut endpoints.
    ///
    /// Это единственное место, где солвер инвертирует ось читаемости; тест
    /// охраняет строгую монотонность `Ys` вдоль тонированной кривой
    /// (`build_color` при фиксированных hue/policy), на которой держится
    /// корректность бисекции: плато или излом кривой развалили бы
    /// единственность корня и провалили round-trip задолго до 1e-9.
    #[test]
    fn match_lightness_ys_round_trips_on_both_chroma_profiles() {
        // 64 halvings shrink the bracket to ~5.4e-20; the gate sits at 1e-9 —
        // ~11 decades of headroom, yet any monotonicity break or a bisection
        // that stops short blows past it immediately.
        const MAX_L_ERR: f64 = 1e-9;
        let cases = [
            (Hue::deg(0.0), ChromaPolicy::Neutral),
            (Hue::deg(286.0), ChromaPolicy::Relative(0.05)),
            (Hue::deg(286.0), ChromaPolicy::Relative(0.10)),
            (Hue::deg(30.0), ChromaPolicy::Relative(0.10)),
        ];
        let n = 1024usize;
        for (hue, policy) in cases {
            let mut max_l_err = 0.0_f64;
            for i in 0..=n {
                let l = i as f64 / n as f64;
                let rgb = build_color(l, hue, policy);
                let target_ys = wcag::relative_luminance([
                    srgb_gamma(rgb[0]),
                    srgb_gamma(rgb[1]),
                    srgb_gamma(rgb[2]),
                ]);
                let l_back = match_lightness_ys(target_ys, hue, policy);
                max_l_err = max_l_err.max((l_back - l).abs());
            }
            eprintln!("[{hue:?} {policy:?}] Ys round-trip: max|Δl_ok|={max_l_err:.2e}");
            assert!(
                max_l_err < MAX_L_ERR,
                "{hue:?} {policy:?}: Ys round-trip drifted {max_l_err:.2e} (> {MAX_L_ERR:.0e})"
            );
        }
        // Endpoint short-circuits: targets outside the reachable luminance
        // range clamp to the gamut edge instead of oscillating.
        assert_eq!(
            match_lightness_ys(-0.5, Hue::deg(0.0), ChromaPolicy::Neutral),
            0.0,
            "below-black target must clamp to l_ok = 0"
        );
        assert_eq!(
            match_lightness_ys(1.5, Hue::deg(0.0), ChromaPolicy::Neutral),
            1.0,
            "above-white target must clamp to l_ok = 1"
        );
    }

    /// Frozen `resolve_set` hex output across the owner's golden grid — the
    /// before/after gate for any hot-path refactor in this module. Each line is
    /// `vc|bg|policy|role=hex,…` produced by the live `resolve_set`. The full
    /// set of emitted `#RRGGBB` hexes for every role must be byte-identical
    /// before and after a performance change; if any cell moves, the refactor
    /// altered the colour the caller gets and the test fails loudly.
    ///
    /// Grid: 6 backgrounds (#FFFFFF/#F2F2F7/#7F7F7F/#1C1C1E/#101012/#3478F6) ×
    /// both precompiled viewing conditions × the two production chroma policies
    /// (achromatic Neutral and the v1 Tinted{286°, 0.10}). Regenerate the
    /// expectations with `_emit_resolve_set_golden` (kept below, `#[ignore]`d)
    /// only when a colour change is *intended* and explained.
    ///
    /// UPDATED for the HIG role taxonomy (`role-taxonomy-hig`): the row format now
    /// carries all 19 roles per line (`label-*`, `separator`, `border-*`,
    /// `fill-*`, `shadow-*`, `none`) instead of the old 10. The `label-*` cells are
    /// byte-identical to the prior `text-*` cells — the rename moved keys, not
    /// colours — and `border-strong` mirrors `label-primary` (it shares the
    /// label-primary contract). The new `border-*`/`fill-*`/`shadow-*` cells are
    /// Decorative magnitudes resolved against the current `DECORATIVE_FLOOR_MIN`/dJ'
    /// contract (see `semantic.rs`); will be re-derived in `surface-jnd` (#44).
    /// This is the one allowed touch to this module's golden: `Role::ALL` changed
    /// (словарный канон #92 снёс `icon` и переименовал `border-ghost`→`border-none`),
    /// so the line shape moved with it — colours did not change (`icon` was a byte
    /// dup of `label-tertiary`, `border-none` is the same honest zero).
    const RESOLVE_SET_GOLDEN: &[&str] = &[
        "srgb|#FFFFFF|Neutral|label-primary=#141414,label-secondary=#767676,label-tertiary=#949494,label-quaternary=#C2C2C2,separator=#ECECEC,border-strong=#141414,border-base=#E9E9E9,border-soft=#F4F4F4,border-none=none,fill-primary=#E4E4E4,fill-secondary=#E9E9E9,fill-tertiary=#EFEFEF,fill-quaternary=#F4F4F4,fill-none=none,shadow-minor=#ECECEC,shadow-ambient=#EAEAEA,shadow-penumbra=#E6E6E6,shadow-major=#E2E2E2,none=none",
        "srgb|#FFFFFF|Tinted|label-primary=#141419,label-secondary=#757585,label-tertiary=#9493A0,label-quaternary=#C1C1C9,separator=#ECECEE,border-strong=#141419,border-base=#E9E9EB,border-soft=#F4F4F5,border-none=none,fill-primary=#E4E4E7,fill-secondary=#E9E9EB,fill-tertiary=#EFEFF1,fill-quaternary=#F4F4F5,fill-none=none,shadow-minor=#ECECEE,shadow-ambient=#E9E9EC,shadow-penumbra=#E6E6E9,shadow-major=#E1E1E5,none=none",
        "srgb|#F2F2F7|Neutral|label-primary=#131313,label-secondary=#6F6F6F,label-tertiary=#8C8C8C,label-quaternary=#B8B8B8,separator=#E0E0E0,border-strong=#131313,border-base=#DDDDDD,border-soft=#E8E8E8,border-none=none,fill-primary=#D9D9D9,fill-secondary=#DDDDDD,fill-tertiary=#E3E3E3,fill-quaternary=#E8E8E8,fill-none=none,shadow-minor=#E0E0E0,shadow-ambient=#DDDDDD,shadow-penumbra=#D9D9D9,shadow-major=#D5D5D5,none=none",
        "srgb|#F2F2F7|Tinted|label-primary=#131218,label-secondary=#6E6D7F,label-tertiary=#8C8B99,label-quaternary=#B8B8C0,separator=#DFDFE3,border-strong=#131218,border-base=#DDDDE1,border-soft=#E8E8EA,border-none=none,fill-primary=#D8D8DD,fill-secondary=#DDDDE1,fill-tertiary=#E3E3E6,fill-quaternary=#E8E8EA,fill-none=none,shadow-minor=#DFDFE3,shadow-ambient=#DDDDE0,shadow-penumbra=#D9D9DD,shadow-major=#D4D4D9,none=none",
        "srgb|#7F7F7F|Neutral|label-primary=#080808,label-secondary=#161616,label-tertiary=#363636,label-quaternary=#606060,separator=#696969,border-strong=#080808,border-base=#6F6F6F,border-soft=#777777,border-none=none,fill-primary=#6C6C6C,fill-secondary=#6F6F6F,fill-tertiary=#747474,fill-quaternary=#777777,fill-none=none,shadow-minor=#696969,shadow-ambient=#656565,shadow-penumbra=#606060,shadow-major=#5A5A5A,none=none",
        "srgb|#7F7F7F|Tinted|label-primary=#08080B,label-secondary=#16161B,label-tertiary=#363541,label-quaternary=#5F5E70,separator=#676779,border-strong=#08080B,border-base=#6E6E7F,border-soft=#767686,border-none=none,fill-primary=#6A6A7C,fill-secondary=#6E6E7F,fill-tertiary=#727283,fill-quaternary=#767686,fill-none=none,shadow-minor=#676779,shadow-ambient=#646376,shadow-penumbra=#5F5F71,shadow-major=#59596A,none=none",
        "srgb|#1C1C1E|Neutral|label-primary=#FBFBFB,label-secondary=#C0C0C0,label-tertiary=#9F9F9F,label-quaternary=#787878,separator=#3F3F3F,border-strong=#FBFBFB,border-base=#2B2B2B,border-soft=#242424,border-none=none,fill-primary=#2F2F2F,fill-secondary=#2B2B2B,fill-tertiary=#272727,fill-quaternary=#242424,fill-none=none,shadow-minor=#3F3F3F,shadow-ambient=#434343,shadow-penumbra=#484848,shadow-major=#4F4F4F,none=none",
        "srgb|#1C1C1E|Tinted|label-primary=#FBFBFB,label-secondary=#C0C0C7,label-tertiary=#9E9EAA,label-quaternary=#767686,separator=#3E3D4A,border-strong=#FBFBFB,border-base=#2A2A34,border-soft=#23232B,border-none=none,fill-primary=#2E2E38,fill-secondary=#2A2A34,fill-tertiary=#26262F,fill-quaternary=#23232B,fill-none=none,shadow-minor=#3E3D4A,shadow-ambient=#42424F,shadow-penumbra=#474755,shadow-major=#4E4E5D,none=none",
        "srgb|#101012|Neutral|label-primary=#FAFAFA,label-secondary=#BFBFBF,label-tertiary=#9D9D9D,label-quaternary=#757575,separator=#393939,border-strong=#FAFAFA,border-base=#202020,border-soft=#181818,border-none=none,fill-primary=#242424,fill-secondary=#202020,fill-tertiary=#1C1C1C,fill-quaternary=#181818,fill-none=none,shadow-minor=#393939,shadow-ambient=#3E3E3E,shadow-penumbra=#434343,shadow-major=#4A4A4A,none=none",
        "srgb|#101012|Tinted|label-primary=#FAFAFB,label-secondary=#BFBFC6,label-tertiary=#9D9DA8,label-quaternary=#737384,separator=#383844,border-strong=#FAFAFB,border-base=#1F1F27,border-soft=#18171E,border-none=none,fill-primary=#23232B,fill-secondary=#1F1F27,fill-tertiary=#1B1B22,fill-quaternary=#18171E,fill-none=none,shadow-minor=#383844,shadow-ambient=#3D3D49,shadow-penumbra=#434250,shadow-major=#494958,none=none",
        "srgb|#3478F6|Neutral|label-primary=#080808,label-secondary=#141414,label-tertiary=#353535,label-quaternary=#5F5F5F,separator=#676767,border-strong=#080808,border-base=#6F6F6F,border-soft=#777777,border-none=none,fill-primary=#6B6B6B,fill-secondary=#6F6F6F,fill-tertiary=#737373,fill-quaternary=#777777,fill-none=none,shadow-minor=#676767,shadow-ambient=#646464,shadow-penumbra=#5F5F5F,shadow-major=#595959,none=none",
        "srgb|#3478F6|Tinted|label-primary=#08080B,label-secondary=#15141A,label-tertiary=#35343F,label-quaternary=#5E5D6F,separator=#666678,border-strong=#08080B,border-base=#6D6D7E,border-soft=#757585,border-none=none,fill-primary=#69697B,fill-secondary=#6D6D7E,fill-tertiary=#717182,fill-quaternary=#757585,fill-none=none,shadow-minor=#666678,shadow-ambient=#636275,shadow-penumbra=#5E5E6F,shadow-major=#585868,none=none",
        "dim|#FFFFFF|Neutral|label-primary=#141414,label-secondary=#767676,label-tertiary=#949494,label-quaternary=#C2C2C2,separator=#ECECEC,border-strong=#141414,border-base=#D8D8D8,border-soft=#E8E8E8,border-none=none,fill-primary=#BEBEBE,fill-secondary=#C4C4C4,fill-tertiary=#D1D1D1,fill-quaternary=#DFDFDF,fill-none=none,shadow-minor=#ECECEC,shadow-ambient=#EAEAEA,shadow-penumbra=#E6E6E6,shadow-major=#E2E2E2,none=none",
        "dim|#FFFFFF|Tinted|label-primary=#141419,label-secondary=#757585,label-tertiary=#9493A0,label-quaternary=#C1C1C9,separator=#ECECEE,border-strong=#141419,border-base=#D7D7DC,border-soft=#E8E8EA,border-none=none,fill-primary=#BDBDC4,fill-secondary=#C3C3CA,fill-tertiary=#D1D1D6,fill-quaternary=#DEDFE2,fill-none=none,shadow-minor=#ECECEE,shadow-ambient=#E9E9EC,shadow-penumbra=#E6E6E9,shadow-major=#E1E1E5,none=none",
        "dim|#F2F2F7|Neutral|label-primary=#131313,label-secondary=#6F6F6F,label-tertiary=#8C8C8C,label-quaternary=#B8B8B8,separator=#E0E0E0,border-strong=#131313,border-base=#CDCDCD,border-soft=#DCDCDC,border-none=none,fill-primary=#B3B3B3,fill-secondary=#B9B9B9,fill-tertiary=#C6C6C6,fill-quaternary=#D3D3D3,fill-none=none,shadow-minor=#E0E0E0,shadow-ambient=#DDDDDD,shadow-penumbra=#D9D9D9,shadow-major=#D5D5D5,none=none",
        "dim|#F2F2F7|Tinted|label-primary=#131218,label-secondary=#6E6D7F,label-tertiary=#8C8B99,label-quaternary=#B8B8C0,separator=#DFDFE3,border-strong=#131218,border-base=#CCCCD2,border-soft=#DCDCE0,border-none=none,fill-primary=#B2B2BB,fill-secondary=#B9B9C1,fill-tertiary=#C6C6CC,fill-quaternary=#D3D3D8,fill-none=none,shadow-minor=#DFDFE3,shadow-ambient=#DDDDE0,shadow-penumbra=#D9D9DD,shadow-major=#D4D4D9,none=none",
        "dim|#7F7F7F|Neutral|label-primary=#080808,label-secondary=#161616,label-tertiary=#363636,label-quaternary=#606060,separator=#696969,border-strong=#080808,border-base=#656565,border-soft=#707070,border-none=none,fill-primary=#525252,fill-secondary=#575757,fill-tertiary=#606060,fill-quaternary=#696969,fill-none=none,shadow-minor=#696969,shadow-ambient=#656565,shadow-penumbra=#606060,shadow-major=#5A5A5A,none=none",
        "dim|#7F7F7F|Tinted|label-primary=#08080B,label-secondary=#16161B,label-tertiary=#363541,label-quaternary=#5F5E70,separator=#676779,border-strong=#08080B,border-base=#636375,border-soft=#6E6E7F,border-none=none,fill-primary=#515160,fill-secondary=#555565,fill-tertiary=#5E5E70,fill-quaternary=#68677A,fill-none=none,shadow-minor=#676779,shadow-ambient=#646376,shadow-penumbra=#5F5F71,shadow-major=#59596A,none=none",
        "dim|#1C1C1E|Neutral|label-primary=#FBFBFB,label-secondary=#C0C0C0,label-tertiary=#9F9F9F,label-quaternary=#787878,separator=#3F3F3F,border-strong=#FBFBFB,border-base=#323232,border-soft=#282828,border-none=none,fill-primary=#424242,fill-secondary=#3E3E3E,fill-tertiary=#363636,fill-quaternary=#2E2E2E,fill-none=none,shadow-minor=#3F3F3F,shadow-ambient=#434343,shadow-penumbra=#484848,shadow-major=#4F4F4F,none=none",
        "dim|#1C1C1E|Tinted|label-primary=#FBFBFB,label-secondary=#C0C0C7,label-tertiary=#9E9EAA,label-quaternary=#767686,separator=#3E3D4A,border-strong=#FBFBFB,border-base=#31313B,border-soft=#282730,border-none=none,fill-primary=#41414E,fill-secondary=#3D3D49,fill-tertiary=#353540,fill-quaternary=#2D2C36,fill-none=none,shadow-minor=#3E3D4A,shadow-ambient=#42424F,shadow-penumbra=#474755,shadow-major=#4E4E5D,none=none",
        "dim|#101012|Neutral|label-primary=#FAFAFA,label-secondary=#BFBFBF,label-tertiary=#9D9D9D,label-quaternary=#757575,separator=#393939,border-strong=#FAFAFA,border-base=#252525,border-soft=#1C1C1C,border-none=none,fill-primary=#353535,fill-secondary=#313131,fill-tertiary=#292929,fill-quaternary=#212121,fill-none=none,shadow-minor=#393939,shadow-ambient=#3E3E3E,shadow-penumbra=#434343,shadow-major=#4A4A4A,none=none",
        "dim|#101012|Tinted|label-primary=#FAFAFB,label-secondary=#BFBFC6,label-tertiary=#9D9DA8,label-quaternary=#737384,separator=#383844,border-strong=#FAFAFB,border-base=#25242D,border-soft=#1C1C22,border-none=none,fill-primary=#34343F,fill-secondary=#30303B,fill-tertiary=#282831,fill-quaternary=#212028,fill-none=none,shadow-minor=#383844,shadow-ambient=#3D3D49,shadow-penumbra=#434250,shadow-major=#494958,none=none",
        "dim|#3478F6|Neutral|label-primary=#080808,label-secondary=#141414,label-tertiary=#353535,label-quaternary=#5F5F5F,separator=#676767,border-strong=#080808,border-base=#646464,border-soft=#6F6F6F,border-none=none,fill-primary=#525252,fill-secondary=#565656,fill-tertiary=#5F5F5F,fill-quaternary=#696969,fill-none=none,shadow-minor=#676767,shadow-ambient=#646464,shadow-penumbra=#5F5F5F,shadow-major=#595959,none=none",
        "dim|#3478F6|Tinted|label-primary=#08080B,label-secondary=#15141A,label-tertiary=#35343F,label-quaternary=#5E5D6F,separator=#666678,border-strong=#08080B,border-base=#626275,border-soft=#6D6D7E,border-none=none,fill-primary=#505060,fill-secondary=#555465,fill-tertiary=#5E5D6F,fill-quaternary=#676779,fill-none=none,shadow-minor=#666678,shadow-ambient=#636275,shadow-penumbra=#5E5E6F,shadow-major=#585868,none=none",
    ];

    /// Render one golden grid line for `(vc, bg, policy)` in the frozen format.
    fn resolve_set_golden_line(
        vc: &ViewingConditions,
        vc_name: &str,
        bg_hex: &str,
        pol_name: &str,
        chroma: crate::semantic::RoleChroma,
    ) -> String {
        use crate::semantic::{Resolved, RoleTable, resolve_set};
        let bg = BgInput::solid(bg_hex).unwrap();
        let table = RoleTable::default().with_chroma(chroma);
        let cells: Vec<String> = resolve_set(&bg, &table, vc)
            .iter()
            .map(|(role, res)| {
                let v = match res {
                    Resolved::Color { solved, .. } => solved.hex().to_string(),
                    // Дефолтная таблица не несёт Ladder/AlphaAnalog/Glow — недостижимо здесь.
                    Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
                    Resolved::Glow(g) => format!("glow({},{})", g.halo_hex(), g.alpha()),
                    Resolved::GlowIndeterminate(_) => "glow-indeterminate".to_string(),
                    Resolved::Material(m) => format!("material({},{:.4})", m.tint_hex(), m.alpha()),
                    Resolved::None => "none".to_string(),
                    Resolved::Unreachable(_) => "unreach".to_string(),
                };
                format!("{}={}", role.key(), v)
            })
            .collect();
        format!("{vc_name}|{bg_hex}|{pol_name}|{}", cells.join(","))
    }

    /// The pre-optimisation `apply_floor` crossing search: a fixed 48-iteration
    /// bisection over the whole `[0, 1]` ray, kept as the golden oracle the
    /// closed-form-seeded search is measured against. Byte-for-byte the loop the
    /// shipped `apply_floor` replaced.
    fn reference_apply_floor_l(
        l_lpc: f64,
        floor_ratio: f64,
        target: f64,
        hue: Hue,
        chroma_policy: ChromaPolicy,
        bg_disp: [f64; 3],
    ) -> Option<(f64, bool)> {
        let rgb_lpc = build_color(l_lpc, hue, chroma_policy);
        if floor_ratio_of(rgb_lpc, bg_disp) >= floor_ratio {
            return Some((l_lpc, false));
        }
        let l_extreme = if target >= 0.0 { 0.0 } else { 1.0 };
        let max_ratio = floor_ratio_of(build_color(l_extreme, hue, chroma_policy), bg_disp);
        if max_ratio < floor_ratio {
            return None; // FloorUnreachable in the real path
        }
        let mut lo = 0.0_f64;
        let mut hi = 1.0_f64;
        for _ in 0..48 {
            let mid = (lo + hi) * 0.5;
            let l_mid = l_lpc + (l_extreme - l_lpc) * mid;
            if floor_ratio_of(build_color(l_mid, hue, chroma_policy), bg_disp) >= floor_ratio {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some((l_lpc + (l_extreme - l_lpc) * hi, true))
    }

    #[test]
    fn apply_floor_matches_the_cold_bisection_byte_for_byte() {
        // The closed-form-seeded floor search must emit the *same hex* as the old
        // fixed-48 [0, 1] bisection everywhere — densely, not just on the 24
        // golden rows. Sweep starting lightness, both polarities, both floors,
        // neutral and the production tint, against backgrounds spanning the grey
        // axis plus a chromatic one, under both viewing conditions. A single hex
        // disagreement (a seed that narrowed past the crossing, an early exit that
        // stopped short) fails here.
        let bgs = [
            "#FFFFFF", "#F2F2F7", "#9C9C9C", "#5A5A5A", "#1C1C1E", "#3478F6",
        ];
        let floors = [crate::wcag::AA_TEXT_RATIO, crate::wcag::AA_UI_RATIO];
        let policies = [
            (Hue::deg(0.0), ChromaPolicy::Neutral),
            (Hue::deg(286.0), ChromaPolicy::Relative(0.10)),
        ];
        let mut compared = 0usize;
        let mut floored = 0usize;
        for bg_hex in bgs {
            let bg_disp = {
                let lin = srgb_from_hex(bg_hex).unwrap();
                quantised_display(lin)
            };
            for floor_ratio in floors {
                for (hue, chroma) in policies {
                    for sign in [1.0_f64, -1.0_f64] {
                        // Sweep the perceptual lightness the floor might lift.
                        for i in 0..=200 {
                            let l_lpc = i as f64 / 200.0;
                            let target = sign; // only the sign (polarity) matters here
                            let got = apply_floor(l_lpc, floor_ratio, target, hue, chroma, bg_disp);
                            let want = reference_apply_floor_l(
                                l_lpc,
                                floor_ratio,
                                target,
                                hue,
                                chroma,
                                bg_disp,
                            );
                            match (got, want) {
                                (Ok((l_new, ov_new)), Some((l_ref, ov_ref))) => {
                                    compared += 1;
                                    assert_eq!(
                                        ov_new, ov_ref,
                                        "{bg_hex} floor={floor_ratio} sign={sign} l={l_lpc}: override flag differs"
                                    );
                                    if ov_new {
                                        floored += 1;
                                    }
                                    let hex_new = hex_from_srgb(build_color(l_new, hue, chroma));
                                    let hex_ref = hex_from_srgb(build_color(l_ref, hue, chroma));
                                    assert_eq!(
                                        hex_new, hex_ref,
                                        "{bg_hex} floor={floor_ratio} sign={sign} l={l_lpc}: hex drift (new {hex_new} vs cold {hex_ref})"
                                    );
                                }
                                (Err(_), None) => {
                                    compared += 1; // both FloorUnreachable — agree
                                }
                                (g, w) => panic!(
                                    "{bg_hex} floor={floor_ratio} sign={sign} l={l_lpc}: reachability disagreement {g:?} vs {w:?}"
                                ),
                            }
                        }
                    }
                }
            }
        }
        eprintln!("apply_floor oracle: {compared} cases compared, {floored} actually floored");
        assert!(floored >= 100, "too few floored cases exercised: {floored}");
    }

    #[test]
    fn resolve_set_hex_matches_golden() {
        use crate::semantic::RoleChroma;
        let bgs = [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ];
        let policies = [
            ("Neutral", RoleChroma::Neutral),
            (
                "Tinted",
                RoleChroma::Tinted {
                    hue_deg: 286.0,
                    ratio: 0.10,
                },
            ),
        ];
        let mut idx = 0usize;
        for (vc, vc_name) in vcs() {
            for bg_hex in bgs {
                for (pol_name, chroma) in policies {
                    let got = resolve_set_golden_line(&vc, vc_name, bg_hex, pol_name, chroma);
                    let want = RESOLVE_SET_GOLDEN[idx];
                    assert_eq!(got, want, "golden drift at grid index {idx}");
                    idx += 1;
                }
            }
        }
        assert_eq!(
            idx,
            RESOLVE_SET_GOLDEN.len(),
            "golden grid size changed: covered {idx}, table has {}",
            RESOLVE_SET_GOLDEN.len()
        );
    }

    #[test]
    #[ignore]
    fn _emit_resolve_set_golden() {
        use crate::semantic::RoleChroma;
        let bgs = [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ];
        let policies = [
            ("Neutral", RoleChroma::Neutral),
            (
                "Tinted",
                RoleChroma::Tinted {
                    hue_deg: 286.0,
                    ratio: 0.10,
                },
            ),
        ];
        for (vc, vc_name) in vcs() {
            for bg_hex in bgs {
                for (pol_name, chroma) in policies {
                    eprintln!(
                        "\"{}\",",
                        resolve_set_golden_line(&vc, vc_name, bg_hex, pol_name, chroma)
                    );
                }
            }
        }
    }
}

// Научные локи + EXPOSURE (волна science/constants-objectivization). Бюджеты
// приёмки QUANT_BUDGET (Lc) и DJ_BUDGET (dJ CAM16-UCS) характеризуются как «N x
// медианного шага 8-бит серой сетки»; экспозиция мерит долю целей, чья приёмка
// зависит от точного бюджета.
#[cfg(test)]
mod exposure_locks {
    use super::{DJ_BUDGET, QUANT_BUDGET};
    use crate::lcs::LcsColor;
    use crate::lpc::lpc;

    fn grey(i: u8) -> String {
        format!("#{i:02X}{i:02X}{i:02X}")
    }
    fn grey_lc() -> Vec<f64> {
        (0u16..=255)
            .map(|i| lpc(&grey(i as u8), "#FFFFFF"))
            .collect()
    }
    fn grey_jp() -> Vec<f64> {
        (0u16..=255)
            .map(|i| LcsColor::from_hex(&grey(i as u8)).unwrap().jp)
            .collect()
    }
    fn median_step(vals: &[f64]) -> f64 {
        let mut s: Vec<f64> = vals.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[s.len() / 2]
    }

    /// (c) QUANT_BUDGET (контракт «+-1 Lc») ~ 2x медианного Lc-шага 8-бит серой
    /// сетки (замер ~0.44): на дискретной сетке любой бюджет >= шага принимает
    /// ближайший узел, поэтому точное значение внутри [1x, ~3x шага] нематериально.
    #[test]
    fn quant_budget_is_a_couple_of_grid_steps() {
        let step = median_step(&grey_lc());
        let ratio = QUANT_BUDGET / step;
        assert!(
            (0.35..0.50).contains(&step),
            "Lc-шаг {step:.4} вне [0.35,0.50)"
        );
        assert!(
            (2.0..3.0).contains(&ratio),
            "QUANT_BUDGET/шаг={ratio:.3} вне [2,3)"
        );
    }

    /// (c) DJ_BUDGET ~ 1.5x медианного dJ-шага 8-бит серой сетки (замер ~0.39).
    #[test]
    fn dj_budget_tracks_grid_step() {
        let step = median_step(&grey_jp());
        let ratio = DJ_BUDGET / step;
        assert!(
            (0.30..0.50).contains(&step),
            "dJ-шаг {step:.4} вне [0.30,0.50)"
        );
        assert!(
            (1.2..2.0).contains(&ratio),
            "DJ_BUDGET/шаг={ratio:.3} вне [1.2,2)"
        );
    }

    fn nearest_err(t: f64, grid: &[f64]) -> f64 {
        grid.iter()
            .map(|g| (g - t).abs())
            .fold(f64::INFINITY, f64::min)
    }

    /// EXPOSURE: доля целевого диапазона, чья приёмка ближайшего узла флипает, пока
    /// бюджет ходит в +-50% полосе. Малая ⇒ дискретность сетки поглощает свип.
    #[test]
    fn exposure_quant_and_dj_budgets() {
        let lc = grey_lc();
        let jp = grey_jp();
        let (mut fq, mut tq) = (0usize, 0usize);
        let mut t = 0.0;
        while t <= 106.0 {
            let e = nearest_err(t, &lc);
            if (0.5 * QUANT_BUDGET..1.5 * QUANT_BUDGET).contains(&e) {
                fq += 1;
            }
            tq += 1;
            t += 0.05;
        }
        let (mut fd, mut td) = (0usize, 0usize);
        let mut t = 0.0;
        while t <= 100.0 {
            let e = nearest_err(t, &jp);
            if (0.5 * DJ_BUDGET..1.5 * DJ_BUDGET).contains(&e) {
                fd += 1;
            }
            td += 1;
            t += 0.05;
        }
        eprintln!(
            "EXPOSURE QUANT_BUDGET flip={:.2}% | DJ_BUDGET flip={:.2}%",
            100.0 * fq as f64 / tq as f64,
            100.0 * fd as f64 / td as f64
        );
    }
}
