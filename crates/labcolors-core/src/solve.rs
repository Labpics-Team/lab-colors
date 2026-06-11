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
use crate::spaces::oklab::oklab_to_srgb_linear;
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
    fn luma_interval(&self, vc: &ViewingConditions) -> Result<LumaInterval, Unreachable> {
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
struct LumaInterval {
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

    let interval = bg.luma_interval(vc)?;
    let y_gov = interval.governing(target);

    // Stage 1 — perceptual target. Invert the LPC core for the Oklab lightness
    // that reproduces the contract's target against the governing endpoint.
    let l_lpc = solve_lpc_lightness(y_gov, target, hue, chroma_policy, vc)?;

    // Stage 2 — legal floor. Text/UI contracts carry a WCAG 2.1 AA floor; if the
    // perceptual solution falls short of it, push the colour until it clears the
    // floor and flag the override. Decorative ([`Floor::None`]) contracts skip
    // this entirely and keep their perceptual target.
    let bg_disp = bg.governing_display(target);
    let (rgb_final, floor_override) = match contract.conformance().min_ratio() {
        Some(floor_ratio) => apply_floor(l_lpc, floor_ratio, target, hue, chroma_policy, bg_disp)?,
        Option::None => (build_color(l_lpc, hue, chroma_policy), false),
    };

    // Quantise and measure both metrics on the emitted hex, then confirm the
    // perceptual floor still holds at both ends of the interval. For Solid the
    // ends coincide, so the check is trivial; the loop is the seam genuine
    // intervals reuse.
    let solved = finish(rgb_final, y_gov, bg_disp, floor_override, vc)?;
    for y_end in interval.endpoints() {
        verify_meets_floor(&solved, y_end, target, vc)?;
    }
    Ok(solved)
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
/// contrast is greatest, by the smallest amount that still clears the floor on
/// the quantised hex. If even the extreme cannot reach the floor, the contract
/// is [`Unreachable::FloorUnreachable`].
fn apply_floor(
    l_lpc: f64,
    floor_ratio: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    bg_disp: [f64; 3],
) -> Result<([f64; 3], bool), Unreachable> {
    let rgb_lpc = build_color(l_lpc, hue, chroma_policy);
    if floor_ratio_of(rgb_lpc, bg_disp) >= floor_ratio {
        return Ok((rgb_lpc, false));
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
    // returned colour is guaranteed to meet the floor even after quantisation.
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
    Ok((build_color(l_final, hue, chroma_policy), true))
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

/// Bisect Oklab lightness for an in-gamut colour whose H-K-corrected lightness
/// equals `target_j_hk`, applying `chroma_policy` at `hue`.
///
/// `J_HK` runs from ~0 at black to ~100 at white, so the target is bracketed by
/// the lightness endpoints; bisection on the continuous curve converges to the
/// lightness that reproduces it. Returns the Oklab lightness; the colour itself
/// is built from it via [`build_color`].
fn match_lightness(
    target_j_hk: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> f64 {
    let j_hk_of =
        |l_ok: f64| lpc::j_hk_from_xyz(srgb_to_xyz(build_color(l_ok, hue, chroma_policy)), vc);

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
    let rgb_quantised = srgb_from_hex(&hex).map_err(Unreachable::InvalidInput)?;
    let color = LcsColor::from_hex_with_vc(&hex, vc).map_err(Unreachable::InvalidInput)?;
    let y_fg = bg_luma(rgb_quantised, vc);
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

/// Confirm the solved colour still meets the (signed) floor at one interval
/// endpoint, within the 1-Lc quantisation budget. Trivial for a Solid
/// background; the real guard for genuine luminance intervals.
fn verify_meets_floor(
    solved: &Solved,
    y_bg: f64,
    target: f64,
    vc: &ViewingConditions,
) -> Result<(), Unreachable> {
    let rgb = srgb_from_hex(solved.hex()).map_err(Unreachable::InvalidInput)?;
    let y_fg = bg_luma(rgb, vc);
    let lc = lpc::contrast_core(y_fg, y_bg);
    let met = if target >= 0.0 {
        lc >= target - 1.0
    } else {
        lc <= target + 1.0
    };
    if met {
        Ok(())
    } else {
        Err(Unreachable::ExceedsRange {
            target,
            max_achievable: max_lc(y_bg, target),
        })
    }
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
                    let bg = BgInput::solid(bg_hex).unwrap();
                    let res = solve(
                        bg,
                        Contract::text(target),
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
                            ratio >= crate::wcag::AA_TEXT_RATIO - 1e-9,
                            "{vc_name} {bg_hex} t={target}: ratio {ratio}, hex {}",
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
}
