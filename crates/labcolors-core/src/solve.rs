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
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

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

/// A contrast contract: the band of acceptable contrast against the background.
///
/// Expressed as a signed `Lc` range `[floor, ceiling]`, where the sign encodes
/// polarity (positive is dark-on-light, negative is light-on-dark). v1 text
/// contracts use a degenerate range (`floor == ceiling`); the range type is
/// reserved for future just-noticeable-difference contracts (shadows,
/// separators, borders) where a band — "visible enough to be felt, no more" —
/// matters. `solve` targets `floor`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contract {
    floor: f64,
    ceiling: f64,
    typography: Option<TypographicContext>,
}

impl Contract {
    /// A text contract for an explicit signed target `Lc` (degenerate range).
    pub fn text(target_lc: f64) -> Self {
        Self {
            floor: target_lc,
            ceiling: target_lc,
            typography: None,
        }
    }

    /// A range contract `[floor, ceiling]` of signed `Lc`. `solve` targets `floor`.
    pub fn range(floor: f64, ceiling: f64) -> Self {
        Self {
            floor,
            ceiling,
            typography: None,
        }
    }

    /// Attach a reserved [`TypographicContext`]. Not consulted by `solve` in v1.
    pub fn with_typography(mut self, ctx: TypographicContext) -> Self {
        self.typography = Some(ctx);
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
    fn luma_interval(self, vc: &ViewingConditions) -> Result<LumaInterval, Unreachable> {
        match self {
            BgInput::Solid(rgb) => {
                let y = bg_luma(rgb, vc);
                Ok(LumaInterval { lo: y, hi: y })
            }
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

/// A solved foreground colour and the contrast it actually achieves.
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    color: LcsColor,
    hex: String,
    lc: f64,
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

    /// The signed contrast `Lc` the resolved colour achieves against the
    /// background, measured on the quantised hex — what the caller actually gets.
    pub fn lc(&self) -> f64 {
        self.lc
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
    /// The target's polarity disagrees with the background's luminance, e.g. a
    /// dark-on-light target against a background that is already dark.
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
            Self::PolarityMismatch { target } => write!(
                f,
                "target Lc {target:.2} has the wrong polarity for this background's luminance"
            ),
            Self::GamutUnsupported => {
                write!(f, "requested gamut is not supported yet (Display P3 is future work)")
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

    let interval = bg.luma_interval(vc)?;

    // Solve against the governing endpoint, then confirm the contract holds at
    // both ends of the interval. For Solid the ends coincide, so the check is
    // trivially satisfied; the loop is the seam genuine intervals reuse.
    let solved = solve_against(interval.governing(target), target, hue, chroma_policy, vc)?;
    for y_end in interval.endpoints() {
        verify_meets_floor(&solved, y_end, target, vc)?;
    }
    Ok(solved)
}

/// Solve for a foreground against a single background luminance.
fn solve_against(
    y_bg: f64,
    target: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> Result<Solved, Unreachable> {
    let y_fg = invert_contrast(y_bg, target)?;
    let target_j_hk = lpc::grey_j(y_fg, vc);
    let rgb = match_lightness(target_j_hk, hue, chroma_policy, vc);
    finish(rgb, y_bg, vc)
}

/// Analytic inverse of [`contrast_core`](crate::lpc): the clamp-inverted
/// foreground luminance `Y_hk` that yields `target` against `y_bg`.
fn invert_contrast(y_bg: f64, target: f64) -> Result<f64, Unreachable> {
    // Past the offset and clip, the smallest representable |Lc| is this floor;
    // targets inside the dead zone collapse to zero in the forward curve.
    let lc_floor = (LO_CLIP - LO_BOW_OFFSET) * LC_SCALE;
    if target.abs() < lc_floor {
        return Err(Unreachable::BelowContrastFloor { target });
    }

    let bg_c = lpc::soft_clamp(y_bg);

    if target > 0.0 {
        // Normal polarity (dark-on-light): sapc = (bg^a − fg^b)·scale, then
        // Lc = (sapc − offset)·100. Solve for the clamped foreground fg_c.
        let sapc = target / LC_SCALE + LO_BOW_OFFSET;
        let base = bg_c.powf(EXP_BG_LIGHT);
        let max_achievable = (base * CONTRAST_SCALE - LO_BOW_OFFSET) * LC_SCALE;
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
        let max_achievable = ((base - 1.0) * CONTRAST_SCALE + LO_WOB_OFFSET) * LC_SCALE;
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
/// the lightness endpoints; bisection on the continuous curve converges to a
/// colour that reproduces it. Chroma is capped at [`max_chroma`], so the result
/// is always inside the sRGB gamut.
fn match_lightness(
    target_j_hk: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> [f64; 3] {
    let h = hue.degrees();
    let (cos_h, sin_h) = {
        let hr = h.to_radians();
        (hr.cos(), hr.sin())
    };

    let build = |l_ok: f64| -> [f64; 3] {
        let chroma = match chroma_policy {
            ChromaPolicy::Neutral => 0.0,
            ChromaPolicy::Relative(ratio) => ratio.clamp(0.0, 1.0) * max_chroma(l_ok, h),
        };
        let lab = [l_ok, chroma * cos_h, chroma * sin_h];
        let rgb = oklab_to_srgb_linear(lab);
        [
            rgb[0].clamp(0.0, 1.0),
            rgb[1].clamp(0.0, 1.0),
            rgb[2].clamp(0.0, 1.0),
        ]
    };
    let j_hk_of = |l_ok: f64| lpc::j_hk_from_xyz(srgb_to_xyz(build(l_ok)), vc);

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    if target_j_hk <= j_hk_of(lo) {
        return build(lo);
    }
    if target_j_hk >= j_hk_of(hi) {
        return build(hi);
    }
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if j_hk_of(mid) < target_j_hk {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    build((lo + hi) * 0.5)
}

/// Quantise the ideal colour to hex and report the contrast it actually
/// achieves — what the caller gets, not the pre-quantisation ideal.
fn finish(rgb_ideal: [f64; 3], y_bg: f64, vc: &ViewingConditions) -> Result<Solved, Unreachable> {
    let hex = hex_from_srgb(rgb_ideal);
    let rgb_quantised = srgb_from_hex(&hex).map_err(Unreachable::InvalidInput)?;
    let color = LcsColor::from_hex_with_vc(&hex, vc).map_err(Unreachable::InvalidInput)?;
    let y_fg = bg_luma(rgb_quantised, vc);
    let lc = lpc::contrast_core(y_fg, y_bg);
    Ok(Solved { color, hex, lc })
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
            max_achievable: lc,
        })
    }
}

/// H-K-corrected background luminance (`Y_hk`) of a linear-sRGB stimulus.
fn bg_luma(rgb: [f64; 3], vc: &ViewingConditions) -> f64 {
    let j_hk = lpc::j_hk_from_xyz(srgb_to_xyz(rgb), vc).max(0.0);
    lpc::y_hk(j_hk, vc)
}
