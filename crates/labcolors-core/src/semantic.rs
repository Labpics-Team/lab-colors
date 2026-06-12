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
//! this module picks the sign from the background's luminance, so the same role
//! table resolves correctly on a light or a dark background without the caller
//! choosing a theme. That is what "resolved from any background" means.
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
//! the hierarchy primary > secondary > muted > disabled holds on every
//! background, in both polarities — symmetric by construction. This is the
//! deliberate fix for the asymmetry baked into the hand-tuned Figma tokens,
//! where equal opacity steps produced a dark-theme hierarchy ~40 % weaker than
//! the light one (see the module tests).
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
//! - **Brand / sentiment roles are not here.** v1 is `ChromaPolicy::Neutral`
//!   only. The chroma seam ([`Role::chroma`]) is left open so a later chapter
//!   can add accent-tinted roles over the existing sentiment machinery without
//!   reshaping this table.

use crate::solve::{self, BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, Solved, Unreachable};
use crate::spaces::vc::ViewingConditions;

/// The reliable lower bound on a decorative role's contrast magnitude.
///
/// Below roughly this `Lc` the solver hits its quantisation cliff (issue #44)
/// and reports zero contrast, so a `Contract::range` floor beneath it would come
/// back [`Unreachable::BelowContrastFloor`]. Every PROVISIONAL decorative floor
/// is held strictly above this until the real JND calibration lands.
const DECORATIVE_FLOOR_MIN: f64 = 7.6;

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

/// The chroma policy a role carries.
///
/// v1 is [`Neutral`](RoleChroma::Neutral) throughout: the table is achromatic.
/// The variant exists so a later chapter can introduce brand/sentiment-tinted
/// roles without changing this type's shape — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleChroma {
    /// Achromatic (grey). The only v1 policy.
    Neutral,
}

impl RoleChroma {
    /// Translate to the solver's [`ChromaPolicy`]. Hue is irrelevant while every
    /// role is neutral; a future tinted variant resolves its own hue here.
    fn to_solve(self) -> (Hue, ChromaPolicy) {
        match self {
            RoleChroma::Neutral => (Hue::deg(0.0), ChromaPolicy::Neutral),
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
            chroma: RoleChroma::Neutral,
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
    /// A solved colour for a text/UI or decorative role.
    Color(Solved),
    /// The honest zero of the [`Role::None`] token: no colour, no contrast.
    None,
    /// No colour can satisfy this role against this background, with the reason.
    Unreachable(Unreachable),
}

impl Resolved {
    /// The solved colour, if this role resolved to one.
    pub fn solved(&self) -> Option<&Solved> {
        match self {
            Resolved::Color(s) => Some(s),
            _ => None,
        }
    }

    /// The signed perceptual contrast `Lc` of a resolved colour, if any. The
    /// zero token reports `0.0`; an unreachable role reports `None`.
    pub fn lc(&self) -> Option<f64> {
        match self {
            Resolved::Color(s) => Some(s.lc()),
            Resolved::None => Some(0.0),
            Resolved::Unreachable(_) => Option::None,
        }
    }
}

/// Resolve one [`Role`] against `bg` under `vc`, using `table`'s recipe.
///
/// Polarity is chosen from the background's luminance, so the same role resolves
/// on light or dark backgrounds. Returns:
///
/// * [`Resolved::Color`] — the solved colour for a text/UI or decorative role;
/// * [`Resolved::None`] — for the [`Role::None`] zero token;
/// * [`Resolved::Unreachable`] — when no colour can meet the role's contract on
///   this background (an extreme background, never a silent clip).
///
/// * `bg` — the background to resolve against.
/// * `role` — which semantic slot to solve.
/// * `table` — the recipe set; pass [`RoleTable::default`] for the v1 table.
/// * `vc` — viewing conditions (light vs dim/dark); pass the same VC the theme
///   resolves under.
pub fn resolve(bg: &BgInput, role: Role, table: &RoleTable, vc: &ViewingConditions) -> Resolved {
    let spec = table.spec(role);
    let (hue, chroma) = table.chroma().to_solve();

    let contract = match spec {
        RoleSpec::Zero => return Resolved::None,
        RoleSpec::Anchor(anchor) => match anchored_contract(bg, anchor, vc) {
            Ok(c) => c,
            Err(reason) => return Resolved::Unreachable(reason),
        },
        RoleSpec::Decorative { magnitude } => decorative_contract(bg, magnitude, vc),
    };

    match solve::solve(bg.clone(), contract, hue, chroma, vc, Gamut::Srgb) {
        Ok(solved) => Resolved::Color(solved),
        Err(reason) => Resolved::Unreachable(reason),
    }
}

/// Resolve every [`Role`] in [`Role::ALL`] against `bg` in one sweep, in strict
/// visual-weight order (strongest text first, then decorative, then the zero
/// token). The returned pairs preserve that order, so a consumer can read the
/// hierarchy off the sequence and a serialiser emits stable output.
pub fn resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Vec<(Role, Resolved)> {
    Role::ALL
        .iter()
        .map(|&role| (role, resolve(bg, role, table, vc)))
        .collect()
}

/// Build the signed text/UI contract for an anchor: the polarity-correct target
/// at `anchor.fraction()` of the background's maximum achievable contrast.
///
/// The maximum is read from the solver itself — the single source of truth — by
/// probing an unreachable extreme and taking the [`max_achievable`] it reports,
/// so this module never re-derives the contrast curve. If even the extreme is
/// unreachable (a mid background with no headroom in that polarity) the reason
/// propagates out.
///
/// [`max_achievable`]: Unreachable::ExceedsRange
fn anchored_contract(
    bg: &BgInput,
    anchor: TextAnchor,
    vc: &ViewingConditions,
) -> Result<Contract, Unreachable> {
    let sign = background_polarity(bg, vc);
    // `max_contrast` returns the magnitude; reapply the background's polarity so
    // the signed target points the right way (dark-on-light vs light-on-dark).
    let max = max_contrast(bg, sign, vc)?;
    let target = sign * anchor.fraction() * max;
    Ok(Contract::text(target).with_conformance(anchor.conformance()))
}

/// Build the decorative JND contract for a provisional magnitude: a degenerate
/// signed range with no readability floor, polarity chosen from the background.
fn decorative_contract(bg: &BgInput, magnitude: f64, vc: &ViewingConditions) -> Contract {
    let sign = background_polarity(bg, vc);
    let target = sign * magnitude.abs().max(DECORATIVE_FLOOR_MIN);
    // `range` already carries `Floor::None`; the degenerate band [t, t] targets t.
    Contract::range(target, target)
}

/// The maximum contrast magnitude the background can supply in `sign`'s
/// polarity, read back from the solver's own [`Unreachable::ExceedsRange`].
///
/// Probing a deliberately unreachable target makes `solve` report the true
/// forward-curve maximum, so the anchor fraction is taken against the same
/// number the solver would clip at — no duplicated contrast constants. A
/// background with genuinely zero headroom in this polarity returns its reason.
fn max_contrast(bg: &BgInput, sign: f64, vc: &ViewingConditions) -> Result<f64, Unreachable> {
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

/// The contrast polarity a background calls for: `+1` for a light background
/// (dark-on-light text) and `-1` for a dark one (light-on-dark), decided by
/// which polarity the background can actually supply more contrast in.
///
/// Reading polarity from the solver (rather than a luminance threshold) keeps a
/// single source of truth: the background hosts whichever polarity it has the
/// most headroom for, which is exactly the polarity `solve` would accept.
fn background_polarity(bg: &BgInput, vc: &ViewingConditions) -> f64 {
    let dark_on_light = max_contrast(bg, 1.0, vc).unwrap_or(0.0);
    let light_on_dark = max_contrast(bg, -1.0, vc).unwrap_or(0.0);
    if light_on_dark > dark_on_light {
        -1.0
    } else {
        1.0
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

    fn solved_lc(bg: &BgInput, role: Role, vc: &ViewingConditions) -> f64 {
        let table = RoleTable::default();
        match resolve(bg, role, &table, vc) {
            Resolved::Color(s) => s.lc(),
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        }
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
                Resolved::Color(s) => s,
                other => panic!("{}: {other:?}", role.key()),
            };
            let dark = match resolve(&black, *role, &table, &vc) {
                Resolved::Color(s) => s,
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
                        Resolved::Color(s) => s,
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
                Resolved::Color(s) => s,
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
            Resolved::Color(s) => s.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (p_default - p_custom).abs() > 10.0,
            "override should move primary: {p_default} vs {p_custom}"
        );
        // Secondary unchanged.
        let s_default = solved_lc(&bg, Role::TextSecondary, &vc);
        let s_custom = match resolve(&bg, Role::TextSecondary, &custom, &vc) {
            Resolved::Color(s) => s.lc(),
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
    fn unreachable_role_surfaces_a_reason_not_a_clip() {
        // A true mid-grey (#808080) can supply at most ~3.95:1 in either
        // polarity, so the AA *text* floor (4.5:1) is genuinely unreachable for
        // primary and secondary. The role returns `Unreachable::FloorUnreachable`
        // with the reason — never a silently clipped, sub-floor colour.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#808080").unwrap();
        let table = RoleTable::default();
        let set = resolve_set(&bg, &table, &vc);

        let primary = &set.iter().find(|(r, _)| *r == Role::TextPrimary).unwrap().1;
        assert!(
            matches!(
                primary,
                Resolved::Unreachable(Unreachable::FloorUnreachable { .. })
            ),
            "primary on #808080 must be FloorUnreachable, got {primary:?}"
        );
        // Nothing is silently clipped: every resolved colour carries real
        // contrast, and the zero token is the only legitimate zero.
        let no_silent_clip = set.iter().all(|(role, r)| match r {
            Resolved::Color(s) => s.lc().abs() >= 1.0,
            Resolved::None => *role == Role::None,
            Resolved::Unreachable(_) => true,
        });
        assert!(
            no_silent_clip,
            "no role may resolve to a zero-contrast clip"
        );
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
