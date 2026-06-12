//! Semantic role table: a named contrast contract resolved from any background
//! in one [`solve`] call.
//!
//! Where [`solve`](crate::solve) answers "what colour meets *this* signed
//! contrast against *this* background", this module answers the product-level
//! question one layer up: "give me the whole set of named colours a UI needs
//! against this background". A [`Role`] is a stable string key plus a recipe for
//! a [`Contract`]; [`RoleTable`] is the default recipe set, overridable per role;
//! [`resolve`] solves one role and [`resolve_set`] solves the whole table in a
//! single sweep. Serialising the result to CSS custom properties is the
//! runtime-engine chapter's job — this module returns a structured
//! `role → Solved` map and nothing else.
//!
//! # Polarity is read from the background, never from the role
//!
//! [`solve`] takes a *signed* `Lc` (positive = dark-on-light, negative =
//! light-on-dark). A role stores only the *magnitude* of the contrast it wants;
//! this module picks the sign from the background, so the same role table
//! resolves correctly on a light or a dark background without the caller
//! choosing a theme. That is what "resolved from any background" means.
//!
//! The sign is chosen in two stages, and — crucially — from the *WCAG* gate the
//! text roles actually have to clear, not from the perceptual maximum:
//!
//! 1. **WCAG reachability first.** A text role floors at the legal AA ratio
//!    (4.5:1 for text). Which polarity can reach that floor is a property of the
//!    background alone — `contrast_ratio(black, bg)` vs `contrast_ratio(white,
//!    bg)` — and is independent of the viewing conditions, because the WCAG
//!    formula is. So the polarity that clears the strict 4.5:1 floor wins. This
//!    is what stops a light-grey background (`#808080`, `#999999`) from reporting
//!    every text role unreachable while *black* text on it passes AA with room to
//!    spare: the old "pick the larger LPC maximum" rule flipped polarity near
//!    `#999999`, far from the WCAG flip near `#747474`, and chose the side the
//!    legal floor could not reach.
//! 2. **Tie-break on headroom.** When both polarities clear the strict floor
//!    (near the flip, e.g. `#767676`), the side with the larger WCAG margin wins;
//!    if that too is level, the larger LPC headroom breaks it. When *neither*
//!    polarity can clear the floor (a true mid-grey with no readable side), the
//!    side that comes *closest* is chosen, so the [`Unreachable`] a role surfaces
//!    carries the honest best-case `max_ratio`, not a worse one.
//!
//! Because the criterion is VC-independent, a role's polarity never flips between
//! the light and dim viewing conditions for the same background — no per-theme
//! coin-flip on a near-tie like `#3478F6`.
//!
//! # Sanity over arithmetic: the anchor principle
//!
//! Text contrast magnitudes are **not fixed deltas**. A fixed delta is how
//! `text-primary` once came out grey: a mid contrast number satisfies the
//! contract arithmetically but violates the design intent that primary text on
//! white reads as *black*. Instead, a text role anchors its target to a
//! **fraction of the maximum contrast the background can supply**
//! ([`TextAnchor`]). Primary asks for ~97 % of that maximum — almost the
//! strongest the background allows — so on white it lands near-black and on
//! black near-white, by construction, on *any* background. The fractions are
//! calibrated against Daniel's Figma anchors (see [`RoleTable::default`]) and
//! stay marked "calibrates" until his eye signs off.
//!
//! Because every text role is a fraction of the *same* per-background maximum,
//! the hierarchy primary > secondary > muted > disabled is **strict wherever the
//! background physically allows it** — symmetric by construction across both
//! polarities. This is the deliberate fix for the asymmetry baked into the
//! hand-tuned Figma tokens, where equal opacity steps produced a dark-theme
//! hierarchy ~40 % weaker than the light one (see the module tests).
//!
//! # Hierarchy compression is flagged, never silent
//!
//! On a background whose readable window is *narrower than the hierarchy's own
//! steps* — a near-AA mid-grey such as `#747474`, where the only readable
//! polarity has barely any room above 4.5:1 — two adjacent text roles can be
//! forced by the legal floor onto the same point. The old code let primary and
//! secondary collapse to an identical hex silently, falsifying the "strict
//! hierarchy by construction" claim. This module instead degrades *honestly*:
//! the order is kept non-strict (primary ≥ secondary ≥ muted ≥ disabled), a
//! subordinate role is nudged to the smallest distinguishable quantisation step
//! below its senior **only while it still clears its own floor**, and any role
//! whose target was lifted by the floor into this squeeze is marked
//! [`Resolved::compressed`]. A consumer can read the flag and know the hierarchy
//! is compressed here, rather than discovering two roles share a colour.
//!
//! # The zero token
//!
//! "Empty" is a value, not a missing entry. A role that means "no colour here"
//! ([`Role::None`]) is part of the table and resolves to an explicit
//! [`Resolved::None`] — an honest zero (transparent / no contrast), never a
//! skipped key. Swapping a literal for a token later is then a change of value,
//! not the insertion of a token where a hole used to be.
//!
//! # Out of scope for v1 (extension seams, not implementations)
//!
//! - **Decorative JND values are provisional.** `separator` / `border` /
//!   `surface` / `shadow` carry placeholder ranges held above the solver's
//!   reliable floor; their real just-noticeable-difference calibration is the
//!   `surface-jnd` chapter (blocked on the quantisation gap, issue #44).
//! - **Brand / sentiment roles are not here.** v1 carries one *neutral*
//!   undertone for the whole table (the cool tint of Daniel's neutral ladder,
//!   see [`RoleChroma`]); per-role brand/accent hues are a later chapter. The
//!   chroma seam ([`RoleTable::with_chroma`]) is left open so that chapter can
//!   swap the policy over the existing sentiment machinery without reshaping
//!   this table.
//!
//! # The neutral undertone: identity, not sterile grey
//!
//! Daniel's neutral is tinted — `#101012` carries a cool blue-violet undertone,
//! not a pure grey. A role table resolved with zero chroma threw that identity
//! away: `text-primary` on white came out the sterile `#141414`. The default
//! table instead carries the neutral's Oklab hue ([`NEUTRAL_HUE_DEG`]) at a
//! small relative chroma ([`NEUTRAL_TINT_RATIO`]), so every resolved role is a
//! *relative* of the neutral family — `text-primary` on white lands as a cool
//! near-black in the `#101012` family. The chroma is small enough that the WCAG
//! floors, the strict hierarchy, and the near-black/near-white primary all hold
//! exactly as before (the solver re-solves lightness to the same target with the
//! tint applied). A caller who wants pure grey overrides it with
//! [`RoleTable::with_chroma`]`(`[`RoleChroma::Neutral`]`)`.

use crate::solve::{self, BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, Solved, Unreachable};
use crate::spaces::srgb::srgb_gamma;
use crate::spaces::vc::ViewingConditions;
use crate::wcag;

/// The reliable lower bound on a decorative role's contrast magnitude.
///
/// Below roughly this `Lc` the solver hits its quantisation cliff (issue #44)
/// and reports zero contrast, so a `Contract::range` floor beneath it would come
/// back [`Unreachable::BelowContrastFloor`]. Every PROVISIONAL decorative floor
/// is held strictly above this until the real JND calibration lands.
const DECORATIVE_FLOOR_MIN: f64 = 7.6;

/// The strict WCAG 2.1 AA *text* ratio (4.5:1) — the tightest legal gate any
/// role in the table imposes, and therefore the one polarity is chosen against.
/// Selecting against the strictest floor keeps a single polarity for the whole
/// set: a side that clears 4.5:1 trivially clears the laxer 3:1 UI floor too.
const POLARITY_FLOOR_RATIO: f64 = wcag::AA_TEXT_RATIO;

/// The contrast polarity a background hosts: dark foreground on a light
/// background, or light foreground on a dark one.
///
/// Replaces the old bare `f64` sign (`+1.0` / `-1.0`): the two valid states are
/// named, illegal ones (a zero or non-unit sign) are unrepresentable, and the
/// `sign()` accessor is the single place the enum becomes the signed `Lc` the
/// solver consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    /// Dark foreground on a light background — positive signed `Lc`.
    DarkOnLight,
    /// Light foreground on a dark background — negative signed `Lc`.
    LightOnDark,
}

impl Polarity {
    /// The signed multiplier this polarity applies to a contrast magnitude:
    /// `+1` for dark-on-light, `-1` for light-on-dark.
    fn sign(self) -> f64 {
        match self {
            Polarity::DarkOnLight => 1.0,
            Polarity::LightOnDark => -1.0,
        }
    }
}

/// One semantic colour slot: a stable key plus the recipe for its contract.
///
/// The key is the public contract with downstream consumers (CSS custom
/// properties in the runtime-engine chapter); the variants are the v1 role set.
/// [`None`](Role::None) is a first-class member, not the absence of a role — see
/// the module docs on the zero token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Role {
    /// Body / primary text — anchored near the strongest contrast the
    /// background allows, so it reads black-on-light or white-on-dark.
    TextPrimary,
    /// Secondary text — clearly subordinate to primary, still comfortably
    /// readable.
    TextSecondary,
    /// Muted / tertiary text — the weakest text still meant to be read.
    TextMuted,
    /// Disabled text — deliberately low contrast; not for reading, so it
    /// carries no readability floor (WCAG excludes inactive controls).
    TextDisabled,
    /// Meaningful icons and graphical UI objects — non-text 3:1 floor.
    Icon,
    /// Hairline separator between content — a decorative JND contract.
    Separator,
    /// Container outline — a decorative JND contract.
    Border,
    /// Elevation step between surfaces — a decorative JND contract.
    Surface,
    /// Shadow against its surface — a decorative JND contract.
    Shadow,
    /// The explicit zero token: "no colour here". Resolves to
    /// [`Resolved::None`], an honest zero, never a skipped key.
    None,
}

impl Role {
    /// Every v1 role, in strict visual-weight order (strongest text first), so a
    /// resolved set iterates deterministically and ordering invariants read off
    /// the sequence directly.
    pub const ALL: [Role; 10] = [
        Role::TextPrimary,
        Role::TextSecondary,
        Role::TextMuted,
        Role::TextDisabled,
        Role::Icon,
        Role::Separator,
        Role::Border,
        Role::Surface,
        Role::Shadow,
        Role::None,
    ];

    /// The stable string key for this role — the contract with CSS custom
    /// properties downstream. These never change without a versioned migration.
    pub fn key(self) -> &'static str {
        match self {
            Role::TextPrimary => "text-primary",
            Role::TextSecondary => "text-secondary",
            Role::TextMuted => "text-muted",
            Role::TextDisabled => "text-disabled",
            Role::Icon => "icon",
            Role::Separator => "separator",
            Role::Border => "border",
            Role::Surface => "surface",
            Role::Shadow => "shadow",
            Role::None => "none",
        }
    }
}

/// How a text/UI role expresses its target contrast against a background.
///
/// A fraction of the background's maximum achievable contrast — *not* a fixed
/// `Lc` delta. See the module docs on the anchor principle for why. `fraction`
/// is in `(0, 1)`: at `1.0` the target equals the unreachable extreme, so the
/// strongest meaningful anchor sits just below it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnchor {
    fraction: f64,
    conformance: Floor,
}

impl TextAnchor {
    /// A text anchor at `fraction` of the background's maximum contrast, with the
    /// given WCAG conformance floor. `fraction` is clamped into `(0, 1)`.
    pub fn new(fraction: f64, conformance: Floor) -> Self {
        Self {
            fraction: fraction.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON),
            conformance,
        }
    }

    /// The fraction of maximum contrast this anchor targets, in `(0, 1)`.
    pub fn fraction(self) -> f64 {
        self.fraction
    }

    /// The WCAG conformance floor applied after the perceptual target.
    pub fn conformance(self) -> Floor {
        self.conformance
    }
}

/// The contrast recipe behind a role — the shape this module solves.
///
/// Text/UI roles ([`Anchor`](RoleSpec::Anchor)) target a fraction of the
/// background's maximum; decorative roles ([`Decorative`](RoleSpec::Decorative))
/// target a provisional JND magnitude with no readability floor; the zero token
/// ([`Zero`](RoleSpec::Zero)) resolves to nothing. Construct these through
/// [`RoleTable`]; they are exposed so a caller can read or override a recipe.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleSpec {
    /// Anchored text/UI contrast: a fraction of the background's maximum.
    Anchor(TextAnchor),
    /// Decorative just-noticeable-difference contrast: a provisional `Lc`
    /// magnitude, held above [`DECORATIVE_FLOOR_MIN`], with [`Floor::None`].
    ///
    /// PROVISIONAL: calibrated in `surface-jnd` against Figma after #44. The
    /// magnitude here is a working seam, not a design decision.
    Decorative { magnitude: f64 },
    /// The zero token: resolves to [`Resolved::None`].
    Zero,
}

/// The Oklab hue (degrees) the system neutral is tinted with.
///
/// Daniel's neutral ladder is not a pure grey: it carries a cool blue-violet
/// undertone. Measured in Oklab on the owner's anchors, the hue is stable across
/// the whole ladder — `#101012` → 285.97°, `#3C3C43` (Figma secondary) → 285.78°,
/// `#787880` (mid) → 286.01° — so a single constant captures it. Resolved roles
/// inherit this hue, which is what makes `text-primary` on white land as a
/// relative of `#101012` (a cool near-black) rather than the sterile grey
/// `#141414`.
const NEUTRAL_HUE_DEG: f64 = 286.0;

/// The fraction of the in-gamut maximum chroma a tinted role carries.
///
/// Deliberately small: the undertone must be *felt*, never *seen* as colour. The
/// absolute chroma the solver applies is `ratio · max_chroma(L)`
/// ([`build_color`](crate::solve)), and `max_chroma` peaks at mid lightness and
/// falls to ~0 at both the dark and the light extreme. So a single flat ratio
/// reproduces the neutral curve's envelope spirit *for free*: the strongest tint
/// lands on the mid-weight roles, the faintest on the near-black / near-white
/// ends of the text ladder — "меньше у тёмных/светлых краёв, больше к середине".
/// At `0.08` on white, `text-primary` resolves to `#141418` (a cool near-black in
/// the `#101012` family), not pure grey.
const NEUTRAL_TINT_RATIO: f64 = 0.08;

/// The chroma policy a role table carries.
///
/// The v1 default is [`Tinted`](RoleChroma::Tinted) with the neutral's cool
/// undertone (see [`NEUTRAL_HUE_DEG`]). [`Neutral`](RoleChroma::Neutral) is the
/// achromatic override: a caller who wants the old pure-grey behaviour (or any
/// other policy) replaces the table's chroma wholesale via
/// [`RoleTable::with_chroma`]. The enum is the seam a later chapter extends for
/// brand/sentiment-tinted roles without reshaping this type.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleChroma {
    /// Achromatic (grey): zero chroma, hue ignored. The explicit override that
    /// reproduces the pre-tint behaviour.
    Neutral,
    /// A small undertone at a fixed Oklab `hue_deg`, carried as `ratio` of the
    /// in-gamut maximum chroma at each role's resolved lightness. The envelope
    /// (less tint at the extremes, more in the middle) emerges from the
    /// lightness-dependence of that maximum — see [`NEUTRAL_TINT_RATIO`].
    Tinted { hue_deg: f64, ratio: f64 },
}

impl RoleChroma {
    /// The v1 default: the neutral's cool undertone at a small flat ratio.
    fn neutral_tint() -> Self {
        RoleChroma::Tinted {
            hue_deg: NEUTRAL_HUE_DEG,
            ratio: NEUTRAL_TINT_RATIO,
        }
    }

    /// Translate to the solver's `(hue, chroma)` inputs. For the achromatic
    /// override the hue is irrelevant (the solver ignores it at zero chroma); a
    /// tinted policy passes its own hue and a relative chroma the solver caps at
    /// the in-gamut maximum.
    fn to_solve(self) -> (Hue, ChromaPolicy) {
        match self {
            RoleChroma::Neutral => (Hue::deg(0.0), ChromaPolicy::Neutral),
            RoleChroma::Tinted { hue_deg, ratio } => {
                (Hue::deg(hue_deg), ChromaPolicy::Relative(ratio))
            }
        }
    }
}

/// The default, overridable recipe set mapping every [`Role`] to a [`RoleSpec`].
///
/// [`default`](RoleTable::default) is the calibrated v1 table; override any
/// single role with [`with`](RoleTable::with) and the rest stay at their
/// defaults. A custom table is how a caller tunes one role's target without
/// touching the others.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleTable {
    specs: [(Role, RoleSpec); 10],
    chroma: RoleChroma,
}

impl RoleTable {
    /// The recipe for `role` in this table.
    pub fn spec(&self, role: Role) -> RoleSpec {
        self.specs
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, s)| *s)
            // Every variant of the (closed-construction) v1 enum is present in
            // `specs`; the table is built from `Role::ALL`.
            .unwrap_or(RoleSpec::Zero)
    }

    /// The chroma policy this table applies to every role (v1: always neutral).
    pub fn chroma(&self) -> RoleChroma {
        self.chroma
    }

    /// Return a copy with `role`'s recipe replaced — every other role keeps its
    /// default. This is the role-table override seam.
    pub fn with(mut self, role: Role, spec: RoleSpec) -> Self {
        if let Some(entry) = self.specs.iter_mut().find(|(r, _)| *r == role) {
            entry.1 = spec;
        }
        self
    }

    /// Return a copy with the chroma policy replaced wholesale.
    ///
    /// The default table carries the neutral's cool undertone
    /// ([`RoleChroma::Tinted`]); this is the seam that overrides it completely —
    /// pass [`RoleChroma::Neutral`] for the achromatic pure-grey behaviour, or a
    /// different [`RoleChroma::Tinted`] hue/ratio for another undertone. The
    /// override is total: it replaces the policy for *every* role, including
    /// dropping the tint to zero.
    pub fn with_chroma(mut self, chroma: RoleChroma) -> Self {
        self.chroma = chroma;
        self
    }
}

impl Default for RoleTable {
    /// The v1 role table.
    ///
    /// Text fractions are calibrated against Daniel's Figma "Labels/Neutral"
    /// anchors on white, where the maximum achievable contrast is ~106 Lc:
    ///
    /// | Role | Figma Lc (light) | fraction of max |
    /// |------|------------------|-----------------|
    /// | primary | 102.6 | 0.968 |
    /// | secondary | 66.5 | 0.627 |
    /// | muted (tertiary) | 48.9 | 0.461 |
    /// | disabled (quaternary) | 29.3 | 0.276 |
    ///
    /// Primary's 0.968 makes it "almost the maximum the background allows" — the
    /// anchor principle, not a fixed delta — so it reads black/white on the
    /// extremes rather than grey. The fractions are equal across polarities by
    /// design, which is the deliberate correction of the asymmetry in the
    /// hand-tuned Figma tokens (dark anchors were −105.4/−40.9/−26.2/−13.1: a
    /// dark hierarchy ~40 % weaker than light). All values are marked
    /// "calibrates" — the final word is Daniel's eye.
    ///
    /// Conformance: primary/secondary carry the AA text floor (4.5:1), muted and
    /// icon the AA UI floor (3:1), disabled carries none (WCAG excludes inactive
    /// controls). Decorative roles carry PROVISIONAL magnitudes with no floor.
    fn default() -> Self {
        let anchor =
            |fraction, conformance| RoleSpec::Anchor(TextAnchor::new(fraction, conformance));
        // PROVISIONAL decorative magnitudes — working seam above the reliable
        // floor, not final JND values. Calibrated in surface-jnd after #44.
        let decorative = |magnitude| RoleSpec::Decorative { magnitude };
        Self {
            specs: [
                (Role::TextPrimary, anchor(0.968, Floor::AaText)),
                (Role::TextSecondary, anchor(0.627, Floor::AaText)),
                (Role::TextMuted, anchor(0.461, Floor::AaUi)),
                (Role::TextDisabled, anchor(0.276, Floor::None)),
                (Role::Icon, anchor(0.461, Floor::AaUi)),
                (Role::Separator, decorative(8.0)),
                (Role::Border, decorative(9.0)),
                (Role::Surface, decorative(8.0)),
                (Role::Shadow, decorative(10.0)),
                (Role::None, RoleSpec::Zero),
            ],
            chroma: RoleChroma::neutral_tint(),
        }
    }
}

/// The outcome of resolving one role: a solved colour, an honest zero, or a
/// principled reason it is unreachable on this background.
///
/// Unreachability is surfaced per role, never masked — a role on an extreme
/// background (e.g. muted text on a mid-grey that cannot supply enough contrast)
/// returns [`Unreachable`], it is not silently clipped to a wrong colour.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// A solved colour for a text/UI or decorative role. `compressed` is `true`
    /// when the legal floor squeezed this role's target against its senior's so
    /// the strict hierarchy could not hold and the role was demoted to the
    /// smallest distinguishable step below — an honest, flagged degradation
    /// rather than a silent two-roles-one-colour collapse. See the module docs.
    Color { solved: Solved, compressed: bool },
    /// The honest zero of the [`Role::None`] token: no colour, no contrast.
    None,
    /// No colour can satisfy this role against this background, with the reason.
    Unreachable(Unreachable),
}

impl Resolved {
    /// A non-compressed solved colour — the common case where the hierarchy holds
    /// strictly and no floor squeeze was needed.
    fn color(solved: Solved) -> Self {
        Resolved::Color {
            solved,
            compressed: false,
        }
    }

    /// The solved colour, if this role resolved to one.
    pub fn solved(&self) -> Option<&Solved> {
        match self {
            Resolved::Color { solved, .. } => Some(solved),
            _ => None,
        }
    }

    /// Whether the hierarchy was compressed at this role: the legal floor forced
    /// it onto (or just below) its senior, so its place in the order is
    /// non-strict. `false` for the zero token and unreachable roles.
    pub fn compressed(&self) -> bool {
        matches!(
            self,
            Resolved::Color {
                compressed: true,
                ..
            }
        )
    }

    /// The signed perceptual contrast `Lc` of a resolved colour, if any. The
    /// zero token reports `0.0`; an unreachable role reports `None`.
    pub fn lc(&self) -> Option<f64> {
        match self {
            Resolved::Color { solved, .. } => Some(solved.lc()),
            Resolved::None => Some(0.0),
            Resolved::Unreachable(_) => Option::None,
        }
    }
}

/// Everything about a `(background, viewing-conditions)` pair that every role in
/// a set shares: the one polarity the whole table resolves in, and the maximum
/// contrast magnitude that polarity can supply.
///
/// Computing this once is what makes [`resolve_set`] solve the table in a single
/// sweep instead of re-deriving polarity (two probe solves) and the maximum (one
/// more) per role — 32 `solve` calls collapse to 12. It also *guarantees* a
/// uniform polarity across the set: every role reads its sign from the same
/// `polarity` field, so they cannot disagree.
#[derive(Debug, Clone, Copy)]
struct ResolveContext {
    /// The single polarity the whole set resolves in (chosen WCAG-first).
    polarity: Polarity,
    /// The maximum contrast magnitude the background supplies in `polarity`, or
    /// `None` if the background has no headroom in it at all (a pathological
    /// extreme). Anchored roles need this to take their fraction of it.
    max_contrast: Option<f64>,
}

impl ResolveContext {
    /// Derive the shared context for `bg` under `vc`: pick the polarity, then read
    /// the maximum contrast in it back from the solver.
    fn new(bg: &BgInput, vc: &ViewingConditions) -> Self {
        let polarity = choose_polarity(bg);
        let max_contrast = max_contrast(bg, polarity, vc).ok();
        Self {
            polarity,
            max_contrast,
        }
    }

    /// The signed `Lc` target for an anchored text/UI role: the chosen polarity's
    /// sign times `fraction` of the background's maximum contrast. `Err` when the
    /// background has no headroom in the chosen polarity (the honest max-ratio is
    /// reported by the role's solve).
    fn anchored_contract(&self, anchor: TextAnchor) -> Result<Contract, Unreachable> {
        let max = self.max_contrast.ok_or(Unreachable::FloorUnreachable {
            floor: POLARITY_FLOOR_RATIO,
            max_ratio: 0.0,
        })?;
        let target = self.polarity.sign() * anchor.fraction() * max;
        Ok(Contract::text(target).with_conformance(anchor.conformance()))
    }

    /// The signed range contract for a decorative JND role: the chosen polarity's
    /// sign times a magnitude held above [`DECORATIVE_FLOOR_MIN`], no readability
    /// floor.
    fn decorative_contract(&self, magnitude: f64) -> Contract {
        let target = self.polarity.sign() * magnitude.abs().max(DECORATIVE_FLOOR_MIN);
        // `range` already carries `Floor::None`; the degenerate band [t, t] targets t.
        Contract::range(target, target)
    }
}

/// Resolve one [`Role`] against `bg` under `vc`, using `table`'s recipe.
///
/// Polarity is chosen from the background (WCAG-first, see the module docs), so
/// the same role resolves on light or dark backgrounds. Returns:
///
/// * [`Resolved::Color`] — the solved colour for a text/UI or decorative role;
/// * [`Resolved::None`] — for the [`Role::None`] zero token;
/// * [`Resolved::Unreachable`] — when no colour can meet the role's contract on
///   this background (an extreme background, never a silent clip).
///
/// This solves the single role in isolation; the hierarchy-compression flag is a
/// *set* property and is only raised by [`resolve_set`], which sees a role's
/// seniors. A role resolved here therefore always reports `compressed == false`.
///
/// * `bg` — the background to resolve against.
/// * `role` — which semantic slot to solve.
/// * `table` — the recipe set; pass [`RoleTable::default`] for the v1 table.
/// * `vc` — viewing conditions (light vs dim/dark); pass the same VC the theme
///   resolves under.
pub fn resolve(bg: &BgInput, role: Role, table: &RoleTable, vc: &ViewingConditions) -> Resolved {
    let ctx = ResolveContext::new(bg, vc);
    resolve_in(bg, role, table, vc, &ctx)
}

/// Resolve one role through an already-derived [`ResolveContext`], so a whole set
/// shares one polarity and one maximum-contrast computation.
fn resolve_in(
    bg: &BgInput,
    role: Role,
    table: &RoleTable,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Resolved {
    let (hue, chroma) = table.chroma().to_solve();
    let contract = match table.spec(role) {
        RoleSpec::Zero => return Resolved::None,
        RoleSpec::Anchor(anchor) => match ctx.anchored_contract(anchor) {
            Ok(c) => c,
            Err(reason) => return Resolved::Unreachable(reason),
        },
        RoleSpec::Decorative { magnitude } => ctx.decorative_contract(magnitude),
    };

    match solve::solve(bg.clone(), contract, hue, chroma, vc, Gamut::Srgb) {
        Ok(solved) => Resolved::color(solved),
        Err(reason) => Resolved::Unreachable(reason),
    }
}

/// Resolve every [`Role`] in [`Role::ALL`] against `bg` in one sweep, in strict
/// visual-weight order (strongest text first, then decorative, then the zero
/// token). The returned pairs preserve that order, so a consumer can read the
/// hierarchy off the sequence and a serialiser emits stable output.
///
/// Polarity and maximum contrast are computed once for the whole set (see
/// [`ResolveContext`]); every role shares them. After the per-role solve a
/// hierarchy pass walks the text roles strongest-first and, where the legal floor
/// squeezed a role onto its senior, demotes it to the smallest distinguishable
/// step below if one still clears its floor, flagging it [`Resolved::compressed`]
/// — an honest, visible degradation rather than a silent identical-colour
/// collapse.
pub fn resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Vec<(Role, Resolved)> {
    let ctx = ResolveContext::new(bg, vc);
    let mut set: Vec<(Role, Resolved)> = Role::ALL
        .iter()
        .map(|&role| (role, resolve_in(bg, role, table, vc, &ctx)))
        .collect();
    enforce_text_hierarchy(&mut set, bg, table, vc, &ctx);
    set
}

/// Walk the text roles strongest-first and keep the order non-strict but honest.
///
/// The anchor principle already orders the *targets* strictly, but the legal
/// floor can lift two adjacent roles onto the same colour where the readable
/// window is narrower than the hierarchy steps (a near-AA mid-grey). For each
/// junior text role that did not come out strictly weaker than the senior above
/// it, try to demote it by the smallest number of quantisation steps that makes
/// it strictly weaker *while it still clears its own WCAG floor*; if none does,
/// the junior becomes a copy of the senior (equality — never stronger). Either
/// way, flag it [`Resolved::compressed`] so the squeeze is visible, not silent.
fn enforce_text_hierarchy(
    set: &mut [(Role, Resolved)],
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) {
    let (hue, chroma) = table.chroma().to_solve();

    // Strongest-first text order; each junior is compared against its senior.
    for window in TEXT_HIERARCHY.windows(2) {
        let [senior_role, junior_role] = [window[0], window[1]];
        let Some(senior_mag) = solved_magnitude(set, senior_role) else {
            continue; // senior unreachable — nothing to compress against
        };
        let Some(junior_mag) = solved_magnitude(set, junior_role) else {
            continue; // junior unreachable — surfaced honestly already
        };
        if junior_mag + STRICT_STEP <= senior_mag {
            continue; // strictly weaker already — hierarchy holds here
        }

        // The floor squeezed this junior onto (or above) its senior. The junior's
        // own conformance governs how far down it may move and still be legal.
        let floor = match table.spec(junior_role) {
            RoleSpec::Anchor(a) => a.conformance(),
            _ => Floor::None,
        };
        let demoted = demote_below(senior_mag, ctx, hue, chroma, floor, bg, vc);
        // The senior's colour is the legal ceiling for the junior: when no
        // distinguishable step below exists, the junior becomes a *copy* of the
        // senior — never a stronger colour. (The floor can lift the junior onto
        // a grid point above the senior; copying restores `senior ≥ junior`.)
        let senior_solved = set.iter().find_map(|(r, res)| match res {
            Resolved::Color { solved, .. } if *r == senior_role => Some(solved.clone()),
            _ => None,
        });
        let Some(entry) = set.iter_mut().find(|(r, _)| *r == junior_role) else {
            continue;
        };
        entry.1 = match (demoted, senior_solved, &entry.1) {
            // A distinguishable, still-legal step below the senior.
            (Some(solved), _, _) => Resolved::Color {
                solved,
                compressed: true,
            },
            // No room to separate: equal to the senior by copy, flagged.
            (None, Some(solved), Resolved::Color { .. }) => Resolved::Color {
                solved,
                compressed: true,
            },
            (None, _, other) => other.clone(),
        };
    }
}

/// The smallest separation in `|Lc|` that counts as "strictly weaker". Note:
/// near the extremes a single quantisation step can be worth only ~0.2–0.3 Lc,
/// so a demotion may need several grid steps to clear it — and when even the
/// laxest legal target cannot, the junior is set equal to its senior instead.
/// The 0.5 threshold separates real visual distinction from float noise.
const STRICT_STEP: f64 = 0.5;

/// Try to solve a junior text role at the strongest target that is still
/// *strictly weaker* than its senior (`senior_mag − STRICT_STEP`) and still
/// clears `floor`. Returns the demoted colour, or `None` if even the laxest
/// distinguishable target cannot stay legal — in which case the caller keeps the
/// floored colour and only flags the compression.
fn demote_below(
    senior_mag: f64,
    ctx: &ResolveContext,
    hue: Hue,
    chroma: ChromaPolicy,
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> Option<Solved> {
    // Target just under the senior. The solve still applies the junior's own legal
    // floor, so if that floor lifts the colour right back onto the senior there is
    // no room to distinguish — detected by re-measuring the result below.
    let target = ctx.polarity.sign() * (senior_mag - STRICT_STEP).max(0.0);
    let contract = Contract::text(target).with_conformance(floor);
    let solved = solve::solve(bg.clone(), contract, hue, chroma, vc, Gamut::Srgb).ok()?;
    if solved.lc().abs() + STRICT_STEP <= senior_mag {
        Some(solved)
    } else {
        None
    }
}

/// The `|Lc|` of a role's solved colour in `set`, if it resolved to one.
fn solved_magnitude(set: &[(Role, Resolved)], role: Role) -> Option<f64> {
    set.iter()
        .find(|(r, _)| *r == role)
        .and_then(|(_, res)| res.solved())
        .map(|s| s.lc().abs())
}

/// The text roles in strict visual-weight order — the sequence the hierarchy
/// invariant and the compression pass walk. Disabled is included: it is still
/// part of the order even though it carries no floor.
const TEXT_HIERARCHY: [Role; 4] = [
    Role::TextPrimary,
    Role::TextSecondary,
    Role::TextMuted,
    Role::TextDisabled,
];

/// The maximum contrast magnitude the background can supply in `polarity`, read
/// back from the solver's own [`Unreachable::ExceedsRange`].
///
/// Probing a deliberately unreachable target makes `solve` report the true
/// forward-curve maximum, so the anchor fraction is taken against the same number
/// the solver would clip at — no duplicated contrast constants. A background with
/// genuinely zero headroom in this polarity returns its reason.
fn max_contrast(
    bg: &BgInput,
    polarity: Polarity,
    vc: &ViewingConditions,
) -> Result<f64, Unreachable> {
    let sign = polarity.sign();
    // 300 Lc is comfortably past the ~106 ceiling of any sRGB background.
    let probe = Contract::text(sign * 300.0).with_conformance(Floor::None);
    match solve::solve(
        bg.clone(),
        probe,
        Hue::deg(0.0),
        ChromaPolicy::Neutral,
        vc,
        Gamut::Srgb,
    ) {
        // The probe is unreachable by design; ExceedsRange carries the ceiling.
        Err(Unreachable::ExceedsRange { max_achievable, .. }) => Ok(max_achievable.abs()),
        // A reachable 300 Lc is physically impossible; treat anything else as the
        // background having no usable headroom in this polarity.
        Ok(_) => Err(Unreachable::ExceedsRange {
            target: sign * 300.0,
            max_achievable: 0.0,
        }),
        Err(other) => Err(other),
    }
}

/// Choose the polarity the whole set resolves in, WCAG-first and VC-independent.
///
/// Stage 1 — *legal reachability*: a text role floors at [`POLARITY_FLOOR_RATIO`]
/// (4.5:1), so the polarity that clears that floor wins. The reachability of each
/// polarity is `contrast_ratio(extreme_fg, bg)` — black for dark-on-light, white
/// for light-on-dark — which is a property of the background alone and does not
/// depend on `vc`, because the WCAG formula does not. This is the fix for the
/// false-unreachable stripe: the old "larger LPC maximum" rule flipped near
/// `#999999`, but the legal floor flips near `#747474`, and on the band between
/// them the LPC rule chose the side that could not reach 4.5:1.
///
/// Stage 2 — *tie-break*: when both sides clear the floor, the larger WCAG margin
/// wins; an exact margin tie prefers dark-on-light (fixed convention), keeping
/// the whole decision VC-independent. When neither clears it, the side that
/// comes *closest* wins, so the role's [`Unreachable`] reports the honest
/// best-case `max_ratio`.
fn choose_polarity(bg: &BgInput) -> Polarity {
    let bg_disp = bg_display(bg);
    // Dark-on-light is hosted by a black foreground; light-on-dark by white.
    let ratio_dark_on_light = wcag::contrast_ratio([0.0, 0.0, 0.0], bg_disp);
    let ratio_light_on_dark = wcag::contrast_ratio([1.0, 1.0, 1.0], bg_disp);

    let dol_clears = ratio_dark_on_light + 1e-9 >= POLARITY_FLOOR_RATIO;
    let lod_clears = ratio_light_on_dark + 1e-9 >= POLARITY_FLOOR_RATIO;

    match (dol_clears, lod_clears) {
        // Exactly one side is legal — take it.
        (true, false) => Polarity::DarkOnLight,
        (false, true) => Polarity::LightOnDark,
        // Both legal (near the flip) — larger WCAG margin, then LPC headroom.
        (true, true) => break_tie(ratio_dark_on_light, ratio_light_on_dark),
        // Neither legal — the closest side, so the diagnostic is the honest best.
        (false, false) => {
            if ratio_dark_on_light >= ratio_light_on_dark {
                Polarity::DarkOnLight
            } else {
                Polarity::LightOnDark
            }
        }
    }
}

/// Break a polarity tie when both sides clear the legal floor: larger WCAG margin
/// wins; at an exact margin tie (the knife-edge background near L ≈ 0.179)
/// dark-on-light is preferred — a fixed, documented convention. Every input to
/// this decision is a property of the background bytes alone, so the choice is
/// VC-independent by construction (no LPC fallback: LPC reads the viewing
/// conditions and would re-open the theme-flip seam this module promises away).
fn break_tie(ratio_dark_on_light: f64, ratio_light_on_dark: f64) -> Polarity {
    const RATIO_EPS: f64 = 1e-6;
    if (ratio_light_on_dark - ratio_dark_on_light) > RATIO_EPS {
        Polarity::LightOnDark
    } else {
        Polarity::DarkOnLight
    }
}

/// The quantised 8-bit *display* sRGB the WCAG formula is measured against — the
/// exact bytes of the background's hex.
///
/// [`BgInput::Solid`] stores *linear*-light sRGB (from `srgb_from_hex`), so it is
/// gamma-encoded back to display space and rounded to the 8-bit grid, matching
/// the quantisation `solve` uses internally so both sides of the WCAG comparison
/// are on the same grid.
fn bg_display(bg: &BgInput) -> [f64; 3] {
    match bg {
        BgInput::Solid(rgb_linear) => {
            let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() / 255.0;
            [q(rgb_linear[0]), q(rgb_linear[1]), q(rgb_linear[2])]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vcs() -> [(ViewingConditions, &'static str); 2] {
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ]
    }

    /// Backgrounds with enough headroom in both VCs that every text role is
    /// reachable — the grid the ordering and polarity invariants run on.
    const REACHABLE_BGS: [&str; 6] = [
        "#FFFFFF", "#F7F8FA", "#EBEBF5", // light end of the neutral ladder
        "#101012", "#1C1C1E", "#242426", // dark end
    ];

    /// The four text roles, strongest first — the visual-weight order the
    /// hierarchy invariant asserts on.
    const TEXT_ORDER: [Role; 4] = [
        Role::TextPrimary,
        Role::TextSecondary,
        Role::TextMuted,
        Role::TextDisabled,
    ];

    /// The neutral band where the WCAG flip lives (~#747474) and where the old
    /// LPC-flip rule (~#999999) chose an unreachable polarity — the stripe
    /// BLOCKER 1 was about. Stepped one 8-bit quantum at a time, plus the two
    /// off-neutral cases (#93939C, #3478F6) from the diagnosis.
    #[test]
    fn hierarchy_never_inverts_on_found_counterexamples() {
        // Verification counterexamples: the floor used to lift the junior onto a
        // grid point ABOVE its senior (#727272/srgb, #0066FF/dim). The senior-copy
        // rule must keep `primary >= secondary` (equality allowed, flagged).
        for (bg_hex, vc) in [
            ("#727272", ViewingConditions::srgb()),
            ("#0066FF", ViewingConditions::dim_surround()),
            ("#6666CC", ViewingConditions::dim_surround()),
        ] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let set = resolve_set(&bg, &RoleTable::default(), &vc);
            let mag = |role: Role| -> Option<f64> {
                set.iter().find_map(|(r, res)| match res {
                    Resolved::Color { solved, .. } if *r == role => Some(solved.lc().abs()),
                    _ => None,
                })
            };
            if let (Some(p), Some(sec)) = (mag(Role::TextPrimary), mag(Role::TextSecondary)) {
                assert!(
                    p + 1e-9 >= sec,
                    "{bg_hex}: primary {p} must not be weaker than secondary {sec}"
                );
            }
        }
    }

    #[test]
    fn polarity_tie_break_is_vc_independent_at_the_seam() {
        // #757575/#767676 straddle the equal-ratio crossover; the chosen
        // polarity must be identical under both viewing conditions.
        for bg_hex in ["#757575", "#767676", "#747474"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let srgb = ResolveContext::new(&bg, &ViewingConditions::srgb()).polarity;
            let dim = ResolveContext::new(&bg, &ViewingConditions::dim_surround()).polarity;
            assert_eq!(srgb, dim, "{bg_hex}: polarity must not depend on VC");
        }
    }

    fn band_hexes() -> Vec<String> {
        let mut v: Vec<String> = (0x70u32..=0x9F)
            .map(|g| format!("#{g:02X}{g:02X}{g:02X}"))
            .collect();
        v.push("#93939C".to_string());
        v.push("#3478F6".to_string());
        v
    }

    fn solved_lc(bg: &BgInput, role: Role, vc: &ViewingConditions) -> f64 {
        let table = RoleTable::default();
        match resolve(bg, role, &table, vc) {
            Resolved::Color { solved, .. } => solved.lc(),
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        }
    }

    fn table_default() -> RoleTable {
        RoleTable::default()
    }

    /// The signed `lc` of `role` in a set, if it resolved to a colour.
    fn set_lc_opt(set: &[(Role, Resolved)], role: Role) -> Option<f64> {
        set.iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| res.solved())
            .map(|s| s.lc())
    }

    /// The emitted hex and the compression flag of `role` in a set, if it
    /// resolved to a colour.
    fn set_hex_and_flag(set: &[(Role, Resolved)], role: Role) -> Option<(String, bool)> {
        set.iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| match res {
                Resolved::Color { solved, compressed } => {
                    Some((solved.hex().to_string(), *compressed))
                }
                _ => None,
            })
    }

    #[test]
    fn strict_text_hierarchy_holds_on_every_reachable_background() {
        // primary > secondary > muted > disabled in |Lc|, on every background,
        // both VCs — the anchor principle makes this hold by construction.
        for (vc, vc_name) in vcs() {
            for bg_hex in REACHABLE_BGS {
                let bg = BgInput::solid(bg_hex).unwrap();
                let mags: Vec<f64> = TEXT_ORDER
                    .iter()
                    .map(|&r| solved_lc(&bg, r, &vc).abs())
                    .collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] > pair[1],
                        "{vc_name} {bg_hex}: hierarchy broken, |Lc| {:?}",
                        mags
                    );
                }
            }
        }
    }

    #[test]
    fn primary_is_near_extreme_on_white_and_black() {
        // The sanity precedent: primary on white/black must read black/white,
        // not grey — |Lc| >= 95 on both extremes, both VCs.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let lc = solved_lc(&bg, Role::TextPrimary, &vc).abs();
                assert!(
                    lc >= 95.0,
                    "{vc_name} {bg_hex}: primary |Lc| {lc} < 95 — reads grey, not black/white"
                );
            }
        }
    }

    #[test]
    fn polarity_is_uniform_across_a_background_and_read_from_it() {
        // Every text role on a light background is dark-on-light (lc > 0); on a
        // dark background light-on-dark (lc < 0). The whole set shares one
        // polarity, chosen from the background, not the role.
        for (vc, _) in vcs() {
            for (bg_hex, expect_positive) in [("#FFFFFF", true), ("#101012", false)] {
                let bg = BgInput::solid(bg_hex).unwrap();
                for &role in &TEXT_ORDER {
                    let lc = solved_lc(&bg, role, &vc);
                    assert_eq!(
                        lc > 0.0,
                        expect_positive,
                        "{bg_hex} {}: polarity not read from background, lc {lc}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn primary_matches_figma_light_anchor_within_tolerance() {
        // Snapshot: primary on white should land near Daniel's Figma anchor
        // 102.6 Lc (the 0.968 fraction of ~106). A few Lc of tolerance absorbs
        // quantisation and the max-probe.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let lc = solved_lc(&bg, Role::TextPrimary, &vc);
        assert!(
            (lc - 102.6).abs() <= 2.5,
            "primary on white {lc} should match Figma anchor 102.6 within 2.5"
        );
    }

    #[test]
    fn light_ladder_matches_figma_anchors() {
        // Snapshot: the light text ladder lands near Daniel's Figma "Labels"
        // anchors. Primary/disabled match closely (no floor in play); secondary
        // and muted sit a few Lc *above* their anchor because the WCAG AA floor
        // legitimately lifts them on white (see `dark_ladder_is_symmetric_…`),
        // so they get a wider tolerance — an explained shift, not silent drift.
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();
        let anchors = [
            (Role::TextPrimary, 102.6, 2.5),
            (Role::TextSecondary, 66.5, 1.0),
            (Role::TextMuted, 48.9, 4.5), // floored up to ~52.7 to clear 3:1
            (Role::TextDisabled, 29.3, 1.0),
        ];
        for (role, anchor, tol) in anchors {
            let lc = solved_lc(&white, role, &vc);
            assert!(
                (lc - anchor).abs() <= tol,
                "{}: light {lc} vs Figma anchor {anchor} (tol {tol})",
                role.key()
            );
        }
    }

    #[test]
    fn dark_ladder_is_symmetric_not_figma_asymmetric() {
        // The crux fix: contracts make the dark ladder the *mirror* of the light
        // one, NOT the hand-tuned Figma dark anchors (−105.4/−40.9/−26.2/−13.1),
        // which were ~40 % weaker than light because equal opacity steps were
        // never compensated. Symmetry holds on the underlying targets; where the
        // measured light/dark values diverge, it is the WCAG floor lifting the
        // light side (flagged by `floor_override`), never silent drift.
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();
        let black = BgInput::solid("#101012").unwrap();
        let table = RoleTable::default();
        // Figma's asymmetric dark anchors — what we deliberately do NOT reproduce.
        let figma_dark_asymmetric: [f64; 4] = [-105.4, -40.9, -26.2, -13.1];

        for (i, role) in TEXT_ORDER.iter().enumerate() {
            let light = match resolve(&white, *role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let dark = match resolve(&black, *role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let (light_lc, dark_lc) = (light.lc().abs(), dark.lc().abs());
            // Either the two polarities agree (true symmetry), or the gap is
            // accounted for by the WCAG floor overriding one side.
            let symmetric = (light_lc - dark_lc).abs() <= 1.5;
            let floor_explains = light.floor_override() || dark.floor_override();
            assert!(
                symmetric || floor_explains,
                "{}: light |Lc| {light_lc} vs dark {dark_lc} diverge without a floor override",
                role.key()
            );
            if i >= 1 {
                // Secondary and weaker: the symmetric dark result is meaningfully
                // stronger than Figma's weak asymmetric dark anchor.
                assert!(
                    dark_lc > figma_dark_asymmetric[i].abs() + 5.0,
                    "{}: symmetric dark {dark_lc} should beat Figma's weak {}",
                    role.key(),
                    figma_dark_asymmetric[i].abs()
                );
            }
        }
    }

    #[test]
    fn none_role_resolves_to_an_honest_zero() {
        // The zero token is a value, not a missing key: it resolves explicitly
        // and reports zero contrast.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        let resolved = resolve(&bg, Role::None, &table, &vc);
        assert_eq!(resolved, Resolved::None);
        assert_eq!(resolved.lc(), Some(0.0));
        assert!(resolved.solved().is_none());
    }

    #[test]
    fn text_roles_meet_their_wcag_floor() {
        // Each text/UI role's solved colour clears its conformance floor.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let table = RoleTable::default();
                for (role, min_ratio) in [
                    (Role::TextPrimary, 4.5),
                    (Role::TextSecondary, 4.5),
                    (Role::TextMuted, 3.0),
                    (Role::Icon, 3.0),
                ] {
                    let solved = match resolve(&bg, role, &table, &vc) {
                        Resolved::Color { solved, .. } => solved,
                        other => panic!("{} {bg_hex}: {other:?}", role.key()),
                    };
                    assert!(
                        solved.wcag_ratio() >= min_ratio - 1e-9,
                        "{vc_name} {bg_hex} {}: ratio {} < {min_ratio}",
                        role.key(),
                        solved.wcag_ratio()
                    );
                }
            }
        }
    }

    #[test]
    fn decorative_roles_use_provisional_floor_and_no_override() {
        // Decorative roles solve through a range contract (no WCAG floor): their
        // magnitude sits above the reliable floor, floor_override is never set.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        for role in [Role::Separator, Role::Border, Role::Surface, Role::Shadow] {
            let solved = match resolve(&bg, role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{} expected colour, got {other:?}", role.key()),
            };
            assert!(
                !solved.floor_override(),
                "{}: decorative role must not trip the WCAG floor",
                role.key()
            );
            assert!(
                solved.lc().abs() >= DECORATIVE_FLOOR_MIN - 1.0,
                "{}: decorative |Lc| {} below reliable floor",
                role.key(),
                solved.lc().abs()
            );
        }
    }

    #[test]
    fn provisional_magnitudes_drive_the_decorative_result() {
        // The decorative result is driven by the table's PROVISIONAL magnitude,
        // not a hardcoded final value: change the magnitude, the result follows.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let default_table = RoleTable::default();
        let stronger = default_table
            .clone()
            .with(Role::Separator, RoleSpec::Decorative { magnitude: 20.0 });

        let base = resolve(&bg, Role::Separator, &default_table, &vc);
        let bumped = resolve(&bg, Role::Separator, &stronger, &vc);
        let (b, s) = (base.lc().unwrap().abs(), bumped.lc().unwrap().abs());
        assert!(s > b, "bumped magnitude must raise |Lc|: {b} -> {s}");
    }

    #[test]
    fn overriding_one_role_leaves_the_others_untouched() {
        // Custom target for one role changes only its output; the rest stay at
        // their defaults, and default() restores everything.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let default_table = RoleTable::default();
        let custom = default_table.clone().with(
            Role::TextPrimary,
            RoleSpec::Anchor(TextAnchor::new(0.5, Floor::AaText)),
        );

        // Primary changed.
        let p_default = solved_lc(&bg, Role::TextPrimary, &vc);
        let p_custom = match resolve(&bg, Role::TextPrimary, &custom, &vc) {
            Resolved::Color { solved, .. } => solved.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (p_default - p_custom).abs() > 10.0,
            "override should move primary: {p_default} vs {p_custom}"
        );
        // Secondary unchanged.
        let s_default = solved_lc(&bg, Role::TextSecondary, &vc);
        let s_custom = match resolve(&bg, Role::TextSecondary, &custom, &vc) {
            Resolved::Color { solved, .. } => solved.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (s_default - s_custom).abs() < 1e-9,
            "override of primary must not touch secondary"
        );
    }

    #[test]
    fn resolve_set_is_complete_and_ordered() {
        // The full sweep returns every role exactly once, in Role::ALL order,
        // with no key skipped (the zero token included as Resolved::None).
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        let set = resolve_set(&bg, &table, &vc);
        let roles: Vec<Role> = set.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            roles,
            Role::ALL.to_vec(),
            "set must cover Role::ALL in order"
        );
        let none = set.iter().find(|(r, _)| *r == Role::None).unwrap();
        assert_eq!(none.1, Resolved::None, "zero token present as honest zero");
    }

    #[test]
    fn light_grey_band_has_a_readable_text_polarity_not_a_false_unreachable() {
        // BLOCKER 1 regression: the light-grey band (#777777..#999999, incl.
        // #93939C and #3478F6) must NOT report text roles unreachable. Black text
        // on these backgrounds clears AA with room (#999999: 7.37:1; #808080:
        // 5.32:1; #3478F6: 5.16:1) — the old "larger LPC maximum" polarity rule
        // chose the white side, which cannot reach 4.5:1, and floored every text
        // role. With the WCAG-first polarity the readable side is chosen, so
        // primary/secondary/muted/icon all resolve on the whole band, both VCs.
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                for role in [
                    Role::TextPrimary,
                    Role::TextSecondary,
                    Role::TextMuted,
                    Role::Icon,
                ] {
                    let r = &set.iter().find(|(rr, _)| *rr == role).unwrap().1;
                    assert!(
                        matches!(r, Resolved::Color { .. }),
                        "{vc_name} {bg_hex} {}: must resolve, got {r:?}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn no_false_unreachable_when_the_opposite_polarity_is_reachable() {
        // The core invariant of the two-stage polarity: on the whole band, no
        // text/UI role is FloorUnreachable, because the polarity is chosen to be
        // the one that clears the floor. (On solid sRGB the AA floor is always
        // reachable in *some* polarity — there is no background where both black
        // and white text fall below 4.5:1 — so a FloorUnreachable here would be a
        // false negative by construction.)
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                for (role, r) in &set {
                    if let Resolved::Unreachable(Unreachable::FloorUnreachable {
                        floor,
                        max_ratio,
                    }) = r
                    {
                        panic!(
                            "{vc_name} {bg_hex} {}: false FloorUnreachable (floor {floor}, max {max_ratio})",
                            role.key()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn polarity_is_vc_independent_across_the_band() {
        // The WCAG-first criterion is the VC-independent relative-luminance
        // formula, so a role's polarity (sign of lc) must be identical under the
        // light and dim viewing conditions for the same background — no per-theme
        // coin-flip on a near-tie like #3478F6.
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        for bg_hex in band_hexes() {
            let bg = BgInput::solid(&bg_hex).unwrap();
            let s = resolve_set(&bg, &table_default(), &srgb);
            let d = resolve_set(&bg, &table_default(), &dim);
            for role in TEXT_ORDER {
                let (Some(ls), Some(ld)) = (set_lc_opt(&s, role), set_lc_opt(&d, role)) else {
                    continue;
                };
                assert_eq!(
                    ls > 0.0,
                    ld > 0.0,
                    "{bg_hex} {}: polarity flipped between VCs (srgb {ls}, dim {ld})",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn hierarchy_is_non_strict_and_compression_is_flagged_on_the_band() {
        // BLOCKER 2: where the readable window is narrower than the hierarchy
        // steps (#747474: the only readable polarity barely clears 4.5:1),
        // primary and secondary used to collapse to an identical hex silently.
        // Now: the order stays non-strict (|Lc| primary >= secondary >= muted >=
        // disabled) everywhere on the band, and any role squeezed onto its senior
        // is flagged compressed — never a silent two-roles-one-colour identity.
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                let mags: Vec<f64> = TEXT_ORDER
                    .iter()
                    .filter_map(|&r| set_lc_opt(&set, r).map(f64::abs))
                    .collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] + 1e-9 >= pair[1],
                        "{vc_name} {bg_hex}: order broken (junior stronger), |Lc| {mags:?}"
                    );
                }
                // No two adjacent *distinct* roles may share an identical hex
                // without the junior being flagged compressed.
                for window in TEXT_ORDER.windows(2) {
                    let [senior, junior] = [window[0], window[1]];
                    let (Some((sh, _)), Some((jh, jc))) = (
                        set_hex_and_flag(&set, senior),
                        set_hex_and_flag(&set, junior),
                    ) else {
                        continue;
                    };
                    if sh == jh {
                        assert!(
                            jc,
                            "{vc_name} {bg_hex}: {} == {} ({sh}) but not flagged compressed",
                            senior.key(),
                            junior.key()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hierarchy_holds_strictly_on_white_with_no_compression_flag() {
        // On a background with full headroom the hierarchy is strict and nothing
        // is compressed — the flag is reserved for genuine squeezes.
        for (vc, _) in vcs() {
            let bg = BgInput::solid("#FFFFFF").unwrap();
            let set = resolve_set(&bg, &table_default(), &vc);
            for role in TEXT_ORDER {
                let r = &set.iter().find(|(rr, _)| *rr == role).unwrap().1;
                assert!(
                    !r.compressed(),
                    "{}: must not be compressed on white",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn no_silent_clip_anywhere_on_the_band() {
        // Every resolved colour carries real contrast; the zero token is the only
        // legitimate zero; an unreachable role surfaces a reason. Nothing clips.
        for (vc, _) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                let no_silent_clip = set.iter().all(|(role, r)| match r {
                    Resolved::Color { solved, .. } => solved.lc().abs() >= 1.0,
                    Resolved::None => *role == Role::None,
                    Resolved::Unreachable(_) => true,
                });
                assert!(
                    no_silent_clip,
                    "{bg_hex}: a role resolved to a zero-contrast clip"
                );
            }
        }
    }

    #[test]
    fn role_keys_are_stable_and_unique() {
        // The string keys are the downstream contract; they must be unique.
        let mut seen = std::collections::HashSet::new();
        for role in Role::ALL {
            assert!(seen.insert(role.key()), "duplicate key {}", role.key());
        }
    }
}
