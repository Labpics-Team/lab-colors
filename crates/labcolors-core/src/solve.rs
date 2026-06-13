//! Inverse perceptual-contrast solver: `solve(bg, contract, …) → colour`.
//!
//! The forward path maps a colour to its H-K-corrected luminance `Y_hk` and
//! through [`contrast_core`](crate::lpc) to a contrast value `Lc`. This module
//! runs that path backwards: given a background and a target contrast it
//! recovers the foreground luminance analytically (the contrast core is
//! invertible), then searches `(lightness, chroma, hue)` for an in-gamut colour
//! whose H-K-corrected lightness reproduces that luminance.
//!
//! ## Algorithm
//!
//! 1. **Background → luminance interval.** [`BgInput`] reduces to `[Y_lo, Y_hi]`
//!    in `Y_hk` space; a [`Solid`](BgInput::Solid) colour is the degenerate
//!    interval `[Y, Y]`. The contract is checked at both ends.
//! 2. **Invert the contrast core.** From the target `Lc` and a background
//!    luminance, recover the clamped foreground luminance for the matching
//!    polarity, then invert the soft black clamp to a raw `Y_hk` — using the
//!    same canonical constants the forward curve uses (no duplicated literals).
//! 3. **`Y_hk` → `J_HK`.** [`grey_j`](crate::lpc) is the exact inverse of the
//!    forward `y_hk` binary search, so this step is analytic.
//! 4. **`J_HK` → colour.** Bisect Oklab lightness so that, after the H-K
//!    correction and the chroma the policy requests (capped at the in-gamut
//!    maximum via [`max_chroma`](crate::scale)), the colour lands on `J_HK`.
//!
//! An unreachable contract returns [`Unreachable`] with a reason — never a
//! silent clip.
//!
//! All canonical contrast constants are reused from [`crate::lpc`]; this module
//! declares none of them (see `docs/decisions/apca-license.md`).

use crate::lcs::LcsColor;
use crate::lpc::{
    self, CONTRAST_SCALE, EXP_BG_DARK, EXP_BG_LIGHT, EXP_FG_DARK, EXP_FG_LIGHT, LC_SCALE,
    LO_BOW_OFFSET, LO_CLIP, LO_WOB_OFFSET,
};
use crate::scale::max_chroma;
use crate::spaces::oklab::{oklab_hue, oklab_to_srgb_linear};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_gamma, srgb_to_xyz};
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
    fn min_ratio(self) -> Option<f64> {
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

    /// Reduce the descriptor to its `Y_hk` luminance interval under `vc`.
    ///
    /// New variants plug in here without touching `solve`'s signature (SEAM a).
    pub(crate) fn luma_interval(
        &self,
        vc: &ViewingConditions,
    ) -> Result<LumaInterval, Unreachable> {
        match self {
            BgInput::Solid(rgb) => {
                let y = bg_luma(*rgb, vc);
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
}

/// A background luminance interval in `Y_hk` space (H-K-corrected luminance).
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

/// Why a contract cannot be satisfied. Returned instead of silently clipping.
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
    /// A background descriptor that cannot yet be reduced (future inputs).
    UnsupportedBackground,
    /// Malformed input, such as an invalid hex colour or a non-finite target.
    InvalidInput(String),
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
            Self::UnsupportedBackground => {
                write!(f, "this background descriptor cannot be resolved yet")
            }
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
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
    if let ChromaPolicy::Relative(ratio) = chroma_policy
        && !ratio.is_finite()
    {
        return Err(Unreachable::InvalidInput(format!(
            "chroma ratio is not finite: {ratio}"
        )));
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
    let l_lpc = solve_lpc_lightness(y_gov, target, hue, chroma_policy, vc)?;

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
/// against a single background luminance.
fn solve_lpc_lightness(
    y_bg: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> Result<f64, Unreachable> {
    let y_fg = invert_contrast(y_bg, target)?;
    let target_j_hk = lpc::grey_j(y_fg, vc);
    Ok(match_lightness(target_j_hk, hue, chroma_policy, vc))
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
    let l_final = l_lpc + (l_extreme - l_lpc) * hi;
    Ok((l_final, true))
}

/// WCAG 2.1 contrast ratio of a linear-sRGB foreground (quantised to the hex it
/// will be emitted as) against the gamma-encoded background.
fn floor_ratio_of(rgb_linear: [f64; 3], bg_disp: [f64; 3]) -> f64 {
    wcag::contrast_ratio(quantised_display(rgb_linear), bg_disp)
}

/// Gamma-encoded sRGB of a linear stimulus, quantised to 8-bit — the display
/// values WCAG 2.1 measures, matching the emitted `#RRGGBB` hex exactly.
fn quantised_display(rgb_linear: [f64; 3]) -> [f64; 3] {
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

/// Recover the Oklab lightness whose H-K-corrected lightness equals
/// `target_j_hk`, applying `chroma_policy` at `hue`.
///
/// `J_HK` runs from ~0 at black to ~100 at white and is strictly monotone in
/// `l_ok`, so the lightness endpoints bracket the target and a search on the
/// continuous curve converges to the reproducing lightness. Returns the Oklab
/// lightness; the colour itself is built from it via [`build_color`].
///
/// ## Fast path: grey-axis LUT seed
///
/// For the neutral core — `ChromaPolicy::Neutral`, or a small undertone
/// (`ratio ≤ `[`MAX_LUT_CHROMA`](crate::lut::MAX_LUT_CHROMA)) under one of the
/// two precompiled viewing conditions — [`seed_bracket`](crate::lut::seed_bracket)
/// supplies a *validated* lightness bracket from the precompiled grey-axis table
/// (see [`crate::lut`]). Refining inside that narrow bracket reaches full
/// precision in a handful of bisection steps instead of 64, collapsing the
/// per-`solve` cost from ~64 CAM16 forward passes to a few. The result is
/// bit-compatible with the cold bisection: the seed is only a starting bracket,
/// the refinement converges the same root, and the empirical final-`Lc` delta
/// over the solver grid is `0.00000` — gated `< 0.01 Lc` by the LUT golden
/// tests (`lut_adds_…`, `lut_bracket_path_…`), ten times tighter than the
/// solver's `0.1 Lc` budget.
///
/// ## Slow path: cold bisection
///
/// Any other viewing condition, or a chroma past the LUT threshold, takes the
/// original full-`[0, 1]` 64-iteration bisection verbatim — correctness is
/// never traded for the seed, only speed. The threshold is a performance gate;
/// the bracket [`seed_bracket`](crate::lut::seed_bracket) returns is
/// re-validated against the real (possibly tinted) curve before use, so the
/// fast path is taken only when the bracket provably contains the root.
fn match_lightness(
    target_j_hk: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> f64 {
    let j_hk_of =
        |l_ok: f64| lpc::j_hk_from_xyz(srgb_to_xyz(build_color(l_ok, hue, chroma_policy)), vc);

    match crate::lut::seed_bracket(target_j_hk, hue, chroma_policy, vc) {
        // Pure neutral: the direct table inverse is the answer.
        Some(crate::lut::LutSeed::Exact(l_ok)) => l_ok,
        // Small chroma: refine the validated bracket on the real curve.
        Some(crate::lut::LutSeed::Bracket(bracket)) => {
            refine_in_bracket(target_j_hk, bracket, j_hk_of)
        }
        // Unsupported VC or large chroma: the original cold bisection.
        None => cold_bisect(target_j_hk, j_hk_of),
    }
}

/// The Oklab-lightness resolution the bracket refinement converges to. At
/// `1e-12` the residual is far below one 8-bit output step (`≈ 3.9e-3`), so the
/// emitted hex — and the measured `Solved.lc()` — matches the cold bisection;
/// validated to a `0.00000` final-`Lc` delta on the solver grid. The `64`-step
/// cap mirrors the cold path so a degenerate bracket can never spin.
const L_OK_EPSILON: f64 = 1e-12;

/// Refine a LUT-seeded lightness bracket to [`L_OK_EPSILON`] by bisection on the
/// real curve. The bracket is guaranteed to contain the root, so this only
/// tightens it — typically in far fewer than 64 steps.
fn refine_in_bracket(
    target_j_hk: f64,
    bracket: crate::lut::LightnessBracket,
    j_hk_of: impl Fn(f64) -> f64,
) -> f64 {
    let mut lo = bracket.lo;
    let mut hi = bracket.hi;
    // Endpoint short-circuits mirror the cold path's, so a target at or beyond
    // the gamut extremes returns the same boundary lightness.
    if target_j_hk <= j_hk_of(lo) {
        return lo;
    }
    if target_j_hk >= j_hk_of(hi) {
        return hi;
    }
    let mut iterations = 0;
    while hi - lo > L_OK_EPSILON && iterations < 64 {
        let mid = (lo + hi) * 0.5;
        if j_hk_of(mid) < target_j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
        iterations += 1;
    }
    (lo + hi) * 0.5
}

/// The original cold bisection over the full `[0, 1]` lightness range, used when
/// no LUT seed applies. Kept byte-for-byte equivalent to the pre-LUT solver so
/// unsupported-VC and large-chroma results are unchanged.
fn cold_bisect(target_j_hk: f64, j_hk_of: impl Fn(f64) -> f64) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    if target_j_hk <= j_hk_of(lo) {
        return lo;
    }
    if target_j_hk >= j_hk_of(hi) {
        return hi;
    }
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if j_hk_of(mid) < target_j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

/// Crate-internal re-export of [`build_color`] for the grey-axis LUT generator
/// and its small-chroma bracket validation. Keeps the table bound to the *same*
/// forward path `solve` uses, so the LUT can never tabulate a different colour
/// than the one the solver emits (single source of truth, issue #29).
pub(crate) fn build_color_for_lut(l_ok: f64, hue: Hue, chroma_policy: ChromaPolicy) -> [f64; 3] {
    build_color(l_ok, hue, chroma_policy)
}

/// Build the in-gamut linear-sRGB colour at Oklab lightness `l_ok`, applying
/// `chroma_policy` at `hue`. Chroma is capped at [`max_chroma`], so the result
/// is always inside the sRGB gamut.
fn build_color(l_ok: f64, hue: Hue, chroma_policy: ChromaPolicy) -> [f64; 3] {
    let h = hue.degrees();
    let hr = h.to_radians();
    let chroma = match chroma_policy {
        ChromaPolicy::Neutral => 0.0,
        ChromaPolicy::Relative(ratio) => ratio.clamp(0.0, 1.0) * max_chroma(l_ok, h),
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
/// perceptual `lc` is measured in `Y_hk` space against `y_bg`; the legal
/// `wcag_ratio` on the quantised display colour against `bg_disp`.
fn finish(
    rgb_ideal: [f64; 3],
    y_bg: f64,
    bg_disp: [f64; 3],
    floor_override: bool,
    vc: &ViewingConditions,
) -> Result<Solved, Unreachable> {
    let hex = hex_from_srgb(rgb_ideal);
    // The quantised colour, decoded once to linear sRGB, drives both perceptual
    // measurements that follow — the H-K luminance and the CAM16 appearance
    // correlates. Both previously ran the CIECAM16 forward on this *same* XYZ
    // independently (`from_xyz_with_hok` and `bg_luma` → `j_hk_from_xyz`),
    // doubling the hottest pass on every candidate. Run the forward once and feed
    // both: `LcsColor::from_cam16` is exactly what `from_xyz_with_hok` builds, and
    // `j_hk_from_cam16` is the same `J_HK` `bg_luma` derives — bit-identical, one
    // forward instead of two.
    let rgb_quantised = srgb_from_hex(&hex).map_err(Unreachable::InvalidInput)?;
    let xyz = srgb_to_xyz(rgb_quantised);
    let (j, m, h) = crate::spaces::cam16::forward(xyz, vc);
    let color = LcsColor::from_cam16(j, m, h, oklab_hue(rgb_quantised));
    let y_fg = lpc::y_hk(lpc::j_hk_from_cam16(j, m, h, vc).max(0.0), vc);
    let lc = lpc::contrast_core(y_fg, y_bg);
    let wcag_ratio = wcag::contrast_ratio(quantised_display(rgb_ideal), bg_disp);
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
fn meets_floor(solved: &Solved, y_bg: f64, target: f64, vc: &ViewingConditions) -> bool {
    let Ok(rgb) = srgb_from_hex(solved.hex()) else {
        // `solved.hex()` is produced by `hex_from_srgb`, so it always parses;
        // an unparsable hex here is a contradiction — treat it as not meeting.
        return false;
    };
    let y_fg = bg_luma(rgb, vc);
    let lc = lpc::contrast_core(y_fg, y_bg);
    meets_floor_lc(lc, target)
}

/// H-K-corrected background luminance (`Y_hk`) of a linear-sRGB stimulus.
fn bg_luma(rgb: [f64; 3], vc: &ViewingConditions) -> f64 {
    let j_hk = lpc::j_hk_from_xyz(srgb_to_xyz(rgb), vc).max(0.0);
    lpc::y_hk(j_hk, vc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lpc::lpc_with_vc;

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

        // (vc name, bg hex) -> (cold forwards, warm forwards), measured.
        //
        // UPDATED for the per-set forward cache (`cam16::ForwardCacheGuard`): the
        // counter now records only *distinct* CIECAM16 computations, because a
        // repeated `XYZ` within a set is served from the cache. The refine
        // fixed-point and the hierarchy pass re-measure the same candidate
        // colours, so 25–33% (WARM) / 44–47% (COLD, which also re-probes the
        // curve-plan bisection) of the forwards were exact repeats — now elided.
        // These counts equal the unique-`XYZ` measurement and are the honest
        // "real CAM16 work" metric. (Prior pin, bg-hoist only: srgb/#FFFFFF
        // 1991/1433 → 1063/958, etc.)
        let expected = [
            (("srgb", "#FFFFFF"), (1063u64, 958u64)),
            (("srgb", "#7F7F7F"), (691, 605)),
            (("srgb", "#101012"), (785, 689)),
            (("dim", "#FFFFFF"), (976, 869)),
            (("dim", "#7F7F7F"), (658, 567)),
            (("dim", "#101012"), (796, 689)),
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
                let _ = crate::resolve_set(&bgi, &tbl, &vc);
                let cold = FORWARD_CALLS.with(|c| c.get());
                assert_eq!(
                    cold, cold_exp,
                    "{name}/{bg}: COLD CAM16 forwards/set = {cold}, expected {cold_exp}"
                );

                // WARM: same theme re-resolved, curve plans now cached.
                FORWARD_CALLS.with(|c| c.set(0));
                let _ = crate::resolve_set(&bgi, &tbl, &vc);
                let warm = FORWARD_CALLS.with(|c| c.get());
                assert_eq!(
                    warm, warm_exp,
                    "{name}/{bg}: WARM CAM16 forwards/set = {warm}, expected {warm_exp}"
                );
            }
        }
    }

    /// Solve and return both the solved value and the contrast measured
    /// independently through the public `lpc_with_vc` on the resolved hex.
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
        let measured = lpc_with_vc(solved.hex(), bg_hex, vc);
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
        let measured = lpc_with_vc(solved.hex(), "#FFFFFF", &vc);
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
        let measured = lpc_with_vc(solved.hex(), "#FFFFFF", &vc);
        assert!(
            measured > 15.0,
            "pushed darker means more contrast, got {measured}"
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
        let measured = lpc_with_vc(solved.hex(), "#FFFFFF", &vc);
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
        let measured = lpc_with_vc(solved.hex(), "#FFFFFF", &vc);
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
    fn non_finite_chroma_ratio_is_rejected() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
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
        // to a hex inside the low-contrast dead zone (Lc 0), but the next darker
        // grid step (#E9E9E9, Lc ≈ 7.85) is within the ±1 budget. The neighbour
        // walk must find it instead of returning a (lying) ExceedsRange.
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
            "#E9E9E9",
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
                    // Symmetric budget — this is the guard CodeRabbit flagged: a
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

    // ── Grey-axis LUT: bit-compatibility with the cold bisection ──────────────

    /// Reference: the pre-LUT cold bisection of `match_lightness`, kept here as
    /// the golden oracle the LUT path is measured against. Bit-identical to the
    /// loop `match_lightness` falls back to when no LUT seed applies.
    fn reference_match_lightness(
        target_j_hk: f64,
        hue: Hue,
        chroma_policy: ChromaPolicy,
        vc: &ViewingConditions,
    ) -> f64 {
        let j_hk_of =
            |l_ok: f64| lpc::j_hk_from_xyz(srgb_to_xyz(build_color(l_ok, hue, chroma_policy)), vc);
        cold_bisect(target_j_hk, j_hk_of)
    }

    #[test]
    fn lut_match_lightness_matches_bisection_on_the_grey_grid() {
        // Golden: across a dense l_ok grid under both precompiled VCs, the LUT
        // `match_lightness` (neutral → direct interp; the fast path) reproduces
        // the cold bisection's lightness to within the interpolation bound, and
        // wherever the 8-bit hex differs the perceptual Lc cost stays under the
        // tightened `MAX_LC_AT_MISMATCH` gate. (A handful of grid points sit
        // exactly on an 8-bit rounding boundary, where the sub-3e-4 lightness
        // difference flips one hex step either way — both colours are within
        // budget; the boundary assignment is not a regression.)
        const MAX_L_INTERP: f64 = 5e-4; // K=257 inverse-interp bound, with margin
        // Measured worst case is 0.0003 Lc at one of those boundary flips; gate
        // at 0.01 — ~33× the fact, 10× tighter than the old 0.1 budget — so a
        // regression that crept the cost toward 0.1 fails instead of passing
        // green. The eprintln still reports the exact measured number.
        const MAX_LC_AT_MISMATCH: f64 = 0.01;
        for (vc, vc_name) in vcs() {
            let mut max_l_err = 0.0_f64;
            let mut max_lc_at_mismatch = 0.0_f64;
            let mut hex_mismatches = 0usize;
            let n = 4096usize;
            for i in 0..=n {
                let l = i as f64 / n as f64;
                let target_j_hk = lpc::j_hk_from_xyz(
                    srgb_to_xyz(build_color(l, Hue::deg(0.0), ChromaPolicy::Neutral)),
                    &vc,
                );
                let l_lut = match_lightness(target_j_hk, Hue::deg(0.0), ChromaPolicy::Neutral, &vc);
                let l_ref = reference_match_lightness(
                    target_j_hk,
                    Hue::deg(0.0),
                    ChromaPolicy::Neutral,
                    &vc,
                );
                max_l_err = max_l_err.max((l_lut - l_ref).abs());

                let rgb_lut = build_color(l_lut, Hue::deg(0.0), ChromaPolicy::Neutral);
                let rgb_ref = build_color(l_ref, Hue::deg(0.0), ChromaPolicy::Neutral);
                if hex_from_srgb(rgb_lut) != hex_from_srgb(rgb_ref) {
                    hex_mismatches += 1;
                    // Cost of the boundary flip, measured against a fixed white
                    // reference in this VC — bounds the Lc the caller could see.
                    let y_lut = bg_luma(rgb_lut, &vc);
                    let y_ref = bg_luma(rgb_ref, &vc);
                    let lc_lut = lpc::contrast_core(y_lut, 1.0);
                    let lc_ref = lpc::contrast_core(y_ref, 1.0);
                    max_lc_at_mismatch = max_lc_at_mismatch.max((lc_lut - lc_ref).abs());
                }
            }
            eprintln!(
                "[{vc_name}] LUT vs bisection: max|Δl_ok|={max_l_err:.2e}, hex mismatches={hex_mismatches}/{} (max ΔLc at mismatch {max_lc_at_mismatch:.4})",
                n + 1
            );
            assert!(
                max_l_err < MAX_L_INTERP,
                "{vc_name}: LUT lightness drifted {max_l_err} from bisection (> {MAX_L_INTERP})"
            );
            assert!(
                max_lc_at_mismatch < MAX_LC_AT_MISMATCH,
                "{vc_name}: a hex boundary flip cost {max_lc_at_mismatch} Lc (> {MAX_LC_AT_MISMATCH} gate)"
            );
        }
    }

    #[test]
    fn lut_adds_under_a_tenth_of_an_lc_across_the_solver_grid() {
        // The contract tolerance the task pins: the LUT must not widen the
        // solver's error budget. Run the REAL solve path and compare the final
        // `Solved.lc()` against a run that forces the cold bisection, over the
        // full neutral background × magnitude × polarity × VC grid. The added
        // error is empirically 0.00000 Lc; the gate is pinned at 0.01 — 10×
        // tighter than the old 0.1 budget — so any nonzero regression in the
        // emitted hex fails here instead of passing under 0.1. The exact
        // measured add is still reported via eprintln below.
        const MAX_ADD: f64 = 0.01;
        let backgrounds = ["#FFFFFF", "#E8E8E8", "#BFBFBF", "#5A5A5A", "#101012"];
        let mut max_add = 0.0_f64;
        let mut compared = 0usize;
        for (vc, vc_name) in vcs() {
            for bg_hex in backgrounds {
                let bg = BgInput::solid(bg_hex).unwrap();
                for magnitude in MAGNITUDES {
                    for target in [magnitude, -magnitude] {
                        // The live solver uses the LUT path internally.
                        let lut = solve(
                            bg.clone(),
                            Contract::text(target).with_conformance(Floor::None),
                            Hue::deg(0.0),
                            ChromaPolicy::Neutral,
                            &vc,
                            Gamut::Srgb,
                        );
                        // Reference: reconstruct the same solve but resolve the
                        // lightness with the cold bisection oracle, then finish
                        // through the identical quantise/measure path.
                        let interval = bg.luma_interval(&vc).unwrap();
                        let y_gov = interval.governing(target);
                        let reference = invert_contrast(y_gov, target).ok().map(|y_fg| {
                            let tj = lpc::grey_j(y_fg, &vc);
                            let l = reference_match_lightness(
                                tj,
                                Hue::deg(0.0),
                                ChromaPolicy::Neutral,
                                &vc,
                            );
                            finish(
                                build_color(l, Hue::deg(0.0), ChromaPolicy::Neutral),
                                y_gov,
                                bg.governing_display(target),
                                false,
                                &vc,
                            )
                            .map(|s| s.lc())
                        });
                        if let (Ok(s_lut), Some(Ok(lc_ref))) = (lut, reference) {
                            let add = (s_lut.lc() - lc_ref).abs();
                            max_add = max_add.max(add);
                            compared += 1;
                            assert!(
                                add < MAX_ADD,
                                "{vc_name} {bg_hex} t={target}: LUT added {add} Lc (> {MAX_ADD} gate)"
                            );
                        }
                    }
                }
            }
        }
        eprintln!("LUT final-Lc add over {compared} solver cases: max={max_add:.5}");
        assert!(compared >= 30, "grid exercised too few cases: {compared}");
    }

    #[test]
    fn lut_bracket_path_matches_bisection_at_small_chroma() {
        // Golden for the small-chroma SEED path (`LutSeed::Bracket` →
        // `refine_in_bracket`), the branch the neutral tests above never reach:
        // both `lut_adds…` and `lut_match_lightness…` run `ChromaPolicy::Neutral`
        // (ratio = 0), which only exercises the direct-interp `Exact` arm. The
        // main default role policy is tinted (hue 286°, ratio 0.10), so the
        // Bracket arm is the production hot path — and until now it was checked
        // only indirectly. Two arms drive it directly:
        //   1. UNIT: a dense l_ok grid × {srgb, dim} × {Relative(0.05),
        //      Relative(0.10)} at hue 286° asserts the LUT-seeded
        //      `match_lightness` reproduces the cold bisection bit-for-bit on the
        //      emitted hex wherever it can, any 8-bit flip costing under gate.
        //   2. END-TO-END: the real `solve` with BOTH polarities (+mag, -mag)
        //      across the background grid, final `Solved.lc()` vs the
        //      cold-bisection oracle — the same shape as `lut_adds_…`, for the
        //      Bracket arm.
        //
        // Both paths bisect the SAME real tinted curve to `L_OK_EPSILON` (1e-12);
        // the seed only changes the starting bracket, not the root. So the
        // measured agreement is far tighter than the neutral `Exact` path's
        // interp bound: max|Δl_ok| ≈ 5e-13 (a couple of bisection ULPs), with
        // ZERO 8-bit hex mismatches over the grid — hence max ΔLc at mismatch is
        // a hard 0.0000. The two gates are pinned just above those measured
        // facts so a real seed regression (a mis-padded bracket, a refine that
        // stops short, a bracket that excludes the root) fails them, instead of
        // sliding under the old 0.1 budget:
        //   * MAX_L_BRACKET = 1e-9: ~2000× the measured 5e-13, still 5e5× under
        //     the `Exact` interp bound — a bracket off by even one node would
        //     blow past it.
        //   * MAX_LC_AT_MISMATCH = 0.01: 10× tighter than the old 0.1; with zero
        //     mismatches today, any future hex flip is gated hard.
        const MAX_L_BRACKET: f64 = 1e-9;
        const MAX_LC_AT_MISMATCH: f64 = 0.01;
        let hue = Hue::deg(286.0);
        let policies = [ChromaPolicy::Relative(0.05), ChromaPolicy::Relative(0.10)];
        let mut max_l_err = 0.0_f64;
        let mut max_lc_at_mismatch = 0.0_f64;
        let mut hex_mismatches = 0usize;
        let mut compared = 0usize;
        let n = 1024usize;
        for (vc, vc_name) in vcs() {
            for policy in policies {
                let mut took_bracket = false;
                for i in 0..=n {
                    let l = i as f64 / n as f64;
                    // Build the target J_HK on the *tinted* curve so the root the
                    // solver must invert genuinely sits on the small-chroma path,
                    // not the neutral axis. This grid is the unit check on
                    // `match_lightness`; the end-to-end both-polarity arm below
                    // drives the same Bracket path through the real `solve`.
                    let target_j_hk =
                        lpc::j_hk_from_xyz(srgb_to_xyz(build_color(l, hue, policy)), &vc);

                    // Confirm this case actually takes the Bracket arm — otherwise
                    // the test would silently pass on the cold path and prove
                    // nothing about the seed. (Endpoints can fall through to the
                    // cold bisection via the bracket-widening guard; interior
                    // targets must seed.)
                    if matches!(
                        crate::lut::seed_bracket(target_j_hk, hue, policy, &vc),
                        Some(crate::lut::LutSeed::Bracket(_))
                    ) {
                        took_bracket = true;
                    }

                    let l_lut = match_lightness(target_j_hk, hue, policy, &vc);
                    let l_ref = reference_match_lightness(target_j_hk, hue, policy, &vc);
                    max_l_err = max_l_err.max((l_lut - l_ref).abs());
                    compared += 1;

                    let rgb_lut = build_color(l_lut, hue, policy);
                    let rgb_ref = build_color(l_ref, hue, policy);
                    if hex_from_srgb(rgb_lut) != hex_from_srgb(rgb_ref) {
                        hex_mismatches += 1;
                        let y_lut = bg_luma(rgb_lut, &vc);
                        let y_ref = bg_luma(rgb_ref, &vc);
                        let lc_lut = lpc::contrast_core(y_lut, 1.0);
                        let lc_ref = lpc::contrast_core(y_ref, 1.0);
                        max_lc_at_mismatch = max_lc_at_mismatch.max((lc_lut - lc_ref).abs());
                    }
                }
                assert!(
                    took_bracket,
                    "{vc_name} {policy:?}: no grid point took the Bracket seed — the test is not exercising the small-chroma path it claims to"
                );
            }
        }

        // End-to-end arm: drive the SAME Bracket path through the real `solve`
        // with BOTH polarities (+mag light-on-dark, -mag dark-on-light) at the
        // tinted policies, and compare the final `Solved.lc()` against the
        // cold-bisection oracle finished through the identical quantise/measure
        // path — exactly the comparison `lut_adds_…` makes for the neutral arm,
        // here for the small-chroma Bracket arm the production default uses.
        let backgrounds = ["#FFFFFF", "#E8E8E8", "#BFBFBF", "#5A5A5A", "#101012"];
        let mut max_add = 0.0_f64;
        let mut e2e_compared = 0usize;
        for (vc, vc_name) in vcs() {
            for policy in policies {
                for bg_hex in backgrounds {
                    let bg = BgInput::solid(bg_hex).unwrap();
                    for magnitude in MAGNITUDES {
                        for target in [magnitude, -magnitude] {
                            let lut = solve(
                                bg.clone(),
                                Contract::text(target).with_conformance(Floor::None),
                                hue,
                                policy,
                                &vc,
                                Gamut::Srgb,
                            );
                            let interval = bg.luma_interval(&vc).unwrap();
                            let y_gov = interval.governing(target);
                            let reference = invert_contrast(y_gov, target).ok().map(|y_fg| {
                                let tj = lpc::grey_j(y_fg, &vc);
                                let l = reference_match_lightness(tj, hue, policy, &vc);
                                finish(
                                    build_color(l, hue, policy),
                                    y_gov,
                                    bg.governing_display(target),
                                    false,
                                    &vc,
                                )
                                .map(|s| s.lc())
                            });
                            if let (Ok(s_lut), Some(Ok(lc_ref))) = (lut, reference) {
                                let add = (s_lut.lc() - lc_ref).abs();
                                max_add = max_add.max(add);
                                e2e_compared += 1;
                                assert!(
                                    add < MAX_LC_AT_MISMATCH,
                                    "{vc_name} {bg_hex} {policy:?} t={target}: Bracket-path solve added {add} Lc (> {MAX_LC_AT_MISMATCH} gate)"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(
            e2e_compared >= 60,
            "end-to-end Bracket arm exercised too few cases: {e2e_compared}"
        );

        eprintln!(
            "Bracket-path LUT vs bisection: max|Δl_ok|={max_l_err:.2e} over {compared} cases, hex mismatches={hex_mismatches} (max ΔLc at mismatch {max_lc_at_mismatch:.4}); end-to-end both-polarity max add={max_add:.5} over {e2e_compared} solve cases"
        );
        assert!(
            max_l_err < MAX_L_BRACKET,
            "Bracket-path lightness drifted {max_l_err} from bisection (> {MAX_L_BRACKET})"
        );
        assert!(
            max_lc_at_mismatch < MAX_LC_AT_MISMATCH,
            "a Bracket-path hex boundary flip cost {max_lc_at_mismatch} Lc (> {MAX_LC_AT_MISMATCH} gate)"
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
    const RESOLVE_SET_GOLDEN: &[&str] = &[
        "srgb|#FFFFFF|Neutral|text-primary=#141414,text-secondary=#767676,text-muted=#949494,text-disabled=#C2C2C2,icon=#949494,separator=#E9E9E9,border=#E7E7E7,surface=#E9E9E9,shadow=#E5E5E5,none=none",
        "srgb|#FFFFFF|Tinted|text-primary=#0C0C11,text-secondary=#6D6D7E,text-muted=#9493A0,text-disabled=#BEBEC6,icon=#9493A0,separator=#E8E8EA,border=#E6E6E9,surface=#E8E8EA,shadow=#E4E4E7,none=none",
        "srgb|#F2F2F7|Neutral|text-primary=#131313,text-secondary=#6F6F6F,text-muted=#8C8C8C,text-disabled=#BCBCBC,icon=#8C8C8C,separator=#E1E1E1,border=#E0E0E0,surface=#E1E1E1,shadow=#DEDEDE,none=none",
        "srgb|#F2F2F7|Tinted|text-primary=#0C0C10,text-secondary=#69697B,text-muted=#8C8B99,text-disabled=#B8B8C0,icon=#8C8B99,separator=#E0E0E3,border=#DEDEE2,surface=#E0E0E3,shadow=#DCDCE0,none=none",
        "srgb|#7F7F7F|Neutral|text-primary=#070707,text-secondary=#161616,text-muted=#363636,text-disabled=#616161,icon=#363636,separator=#696969,border=#666666,surface=#696969,shadow=#646464,none=none",
        "srgb|#7F7F7F|Tinted|text-primary=#030304,text-secondary=#16161B,text-muted=#363541,text-disabled=#575667,icon=#363541,separator=#5F5E70,border=#5C5C6E,surface=#5F5E70,shadow=#5A5A6B,none=none",
        "srgb|#1C1C1E|Neutral|text-primary=#F6F6F6,text-secondary=#BABABA,text-muted=#9A9A9A,text-disabled=#727272,icon=#9A9A9A,separator=#3F3F3F,border=#424242,surface=#3F3F3F,shadow=#444444,none=none",
        "srgb|#1C1C1E|Tinted|text-primary=#F6F6F7,text-secondary=#B6B6BF,text-muted=#9494A0,text-disabled=#68687A,icon=#9494A0,separator=#363541,border=#383844,surface=#363541,shadow=#3B3B47,none=none",
        "srgb|#101012|Neutral|text-primary=#F6F6F6,text-secondary=#B9B9B9,text-muted=#989898,text-disabled=#6F6F6F,icon=#989898,separator=#393939,border=#3C3C3C,surface=#393939,shadow=#3F3F3F,none=none",
        "srgb|#101012|Tinted|text-primary=#F6F6F7,text-secondary=#B5B5BD,text-muted=#91919E,text-disabled=#646477,icon=#91919E,separator=#30303A,border=#33323D,surface=#30303A,shadow=#353540,none=none",
        "srgb|#3478F6|Neutral|text-primary=#0A0A0A,text-secondary=#141414,text-muted=#353535,text-disabled=#757575,icon=#353535,separator=#848484,border=#828282,surface=#848484,shadow=#808080,none=none",
        "srgb|#3478F6|Tinted|text-primary=#050406,text-secondary=#15141A,text-muted=#35343F,text-disabled=#6C6C7D,icon=#35343F,separator=#7C7C8C,border=#7A7A8A,surface=#7C7C8C,shadow=#787787,none=none",
        "dim|#FFFFFF|Neutral|text-primary=#131313,text-secondary=#757575,text-muted=#949494,text-disabled=#C0C0C0,icon=#949494,separator=#E7E7E7,border=#E5E5E5,surface=#E7E7E7,shadow=#E3E3E3,none=none",
        "dim|#FFFFFF|Tinted|text-primary=#0E0E12,text-secondary=#6D6C7E,text-muted=#9493A0,text-disabled=#BDBDC5,icon=#9493A0,separator=#E6E6E8,border=#E4E4E7,surface=#E6E6E8,shadow=#E2E2E5,none=none",
        "dim|#F2F2F7|Neutral|text-primary=#131313,text-secondary=#6F6F6F,text-muted=#8C8C8C,text-disabled=#BCBCBC,icon=#8C8C8C,separator=#E1E1E1,border=#DFDFDF,surface=#E1E1E1,shadow=#DEDEDE,none=none",
        "dim|#F2F2F7|Tinted|text-primary=#0D0D12,text-secondary=#6A6A7B,text-muted=#8C8B99,text-disabled=#B8B8C1,icon=#8C8B99,separator=#E0E0E3,border=#DEDEE2,surface=#E0E0E3,shadow=#DCDCE0,none=none",
        "dim|#7F7F7F|Neutral|text-primary=#070707,text-secondary=#161616,text-muted=#363636,text-disabled=#616161,icon=#363636,separator=#696969,border=#676767,surface=#696969,shadow=#646464,none=none",
        "dim|#7F7F7F|Tinted|text-primary=#040406,text-secondary=#16161B,text-muted=#363541,text-disabled=#585868,icon=#363541,separator=#606072,border=#5E5D6F,surface=#606072,shadow=#5C5B6D,none=none",
        "dim|#1C1C1E|Neutral|text-primary=#F4F4F4,text-secondary=#B8B8B8,text-muted=#989898,text-disabled=#707070,icon=#989898,separator=#3D3D3D,border=#404040,surface=#3D3D3D,shadow=#434343,none=none",
        "dim|#1C1C1E|Tinted|text-primary=#F3F3F5,text-secondary=#B5B5BD,text-muted=#93939F,text-disabled=#686779,icon=#93939F,separator=#363641,border=#393844,surface=#363641,shadow=#3B3B47,none=none",
        "dim|#101012|Neutral|text-primary=#F4F4F4,text-secondary=#B7B7B7,text-muted=#969696,text-disabled=#6D6D6D,icon=#969696,separator=#373737,border=#3A3A3A,surface=#373737,shadow=#3D3D3D,none=none",
        "dim|#101012|Tinted|text-primary=#F3F3F5,text-secondary=#B3B3BC,text-muted=#90909D,text-disabled=#646476,icon=#90909D,separator=#30303A,border=#33333D,surface=#30303A,shadow=#363541,none=none",
        "dim|#3478F6|Neutral|text-primary=#0A0A0A,text-secondary=#141414,text-muted=#353535,text-disabled=#757575,icon=#353535,separator=#848484,border=#828282,surface=#848484,shadow=#808080,none=none",
        "dim|#3478F6|Tinted|text-primary=#060608,text-secondary=#15141A,text-muted=#35343F,text-disabled=#6C6C7E,icon=#35343F,separator=#7D7D8C,border=#7B7A8A,surface=#7D7D8C,shadow=#787888,none=none",
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

    #[test]
    fn unsupported_vc_takes_the_cold_path_unchanged() {
        // A third surround (neither srgb nor dim) has no table, so the LUT must
        // step aside and the cold bisection governs — identical to pre-LUT.
        let dark = ViewingConditions::dark_surround();
        let target_j_hk = lpc::j_hk_from_xyz(
            srgb_to_xyz(build_color(0.5, Hue::deg(0.0), ChromaPolicy::Neutral)),
            &dark,
        );
        let l_lut = match_lightness(target_j_hk, Hue::deg(0.0), ChromaPolicy::Neutral, &dark);
        let l_ref =
            reference_match_lightness(target_j_hk, Hue::deg(0.0), ChromaPolicy::Neutral, &dark);
        assert_eq!(
            l_lut, l_ref,
            "unsupported VC must yield the identical cold-bisection lightness"
        );
    }
}
