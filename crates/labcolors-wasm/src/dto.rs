//! Plain-data results of a resolve, independent of any JS representation.
//!
//! These are the binding's *output boundary*: pure Rust structs the engine
//! fills from the core's `Vec<(Role, Resolved)>`, with no knowledge of
//! wasm-bindgen or `js_sys`. The adapter layer ([`crate::lib`]) projects them
//! into a JS object. Keeping them framework-free makes the engine testable with
//! a native `cargo test` and keeps the dependency arrow pointing inward.
//!
//! Generic over the role set BY CONSTRUCTION: an entry is built per `(Role,
//! Resolved)` the core returns and keyed by `Role::key()`. Nothing here
//! enumerates the roles, so a set change carries through on a rebuild untouched:
//! the label expansion already grew it to 19 roles, and the accent ladder
//! (issue #59) added the `Translucent` outcome carried below. Any further
//! outcome lands the same way.

/// The full result of resolving one background under one theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    /// The theme key this was resolved under (`"light"`, `"dark"`, …).
    pub theme: &'static str,
    /// The normalised background hex the set was resolved against.
    pub background: String,
    /// One entry per role the core returned, in the core's deterministic order.
    /// The CSS variable name is `--lab-{key}`; the key is `entry.role_key`.
    pub roles: Vec<RoleEntry>,
}

/// One role's outcome, keyed by its stable role key.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleEntry {
    /// The stable role key — the CSS-variable stem. Built-in roles use
    /// `Role::key()`; config-defined roles carry the config's own name
    /// (the string-keyed contract), so the key is owned.
    pub role_key: String,
    /// What the role resolved to.
    pub outcome: RoleOutcome,
}

/// The four honest outcomes of resolving a role, mirroring the core's
/// `Resolved` without leaking the core type across the boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleOutcome {
    /// A solved colour with its measured contrasts and degradation flags.
    Color(SolvedColor),
    /// The explicit zero token (`Role::None`): no colour here, by design.
    None,
    /// A semi-transparent ladder / alpha-analog role: the emission is
    /// `oklch(L% C H / A)` and the browser composites it; the measured
    /// contrasts are those of the composite on the resolve background.
    Translucent(RgbaColor),
    /// Свечение (kind glow, labui ADR-0002 §5): screen-слои цвета источника +
    /// решённая интенсивность. Потребитель красит слои с
    /// `mix-blend-mode: screen`; `--lab-<role>` несёт halo, `--lab-<role>-core`
    /// — слой пересвета, `--lab-<role>-alpha` — интенсивность числом.
    Glow(GlowColor),
    /// No colour can satisfy this role on this background, with the reason.
    Unreachable {
        /// A stable machine code for the unreachability reason.
        code: &'static str,
        /// A human-readable explanation (the core's `Display`).
        message: String,
    },
}

/// Слои свечения и решённая интенсивность (kind glow).
#[derive(Debug, Clone, PartialEq)]
pub struct GlowColor {
    /// Слой пересвета (малый радиус), `#RRGGBB`.
    pub core_hex: String,
    /// Слой ореола — источник, `#RRGGBB`.
    pub halo_hex: String,
    /// Интенсивность screen-слоя `(0, 1]`.
    pub alpha: f64,
    /// Фактический |ΔJ'| композита от фона.
    pub achieved_dj: f64,
    /// Цель недостижима — ближайший достижимый шаг (ADR-0002, закон 2).
    pub degraded: bool,
}

/// A semi-transparent emission and the contrasts its composite achieves.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaColor {
    /// The tint as `#RRGGBB` — the colour the emitted `oklch(… / A)` carries.
    pub tint_hex: String,
    /// The alpha the emission carries, `(0, 1]`.
    pub alpha: f64,
    /// The solid the tint composites to on the resolve background.
    pub composite_hex: String,
    /// The signed perceptual contrast `Lc` of the composite.
    pub composite_lc: f64,
    /// The WCAG 2.1 ratio of the composite.
    pub composite_wcag: f64,
    /// `true` when the requested alpha was raised to the smallest resolvable
    /// value (`α_min`) because the requested transparency is not reproducible in
    /// gamut — an honest, flagged degradation of the role contract (mirrors
    /// `SolvedColor::compressed` / `GlowColor::degraded`). The colour never lies:
    /// the composite still equals the target solid byte-for-byte; only the
    /// alpha carried in `alpha` differs from what was asked. Always `false` for a
    /// direct ladder emission.
    pub alpha_coerced: bool,
    /// `true` when a solid family border (`border-<family>-strong`, M2 ch5c) was
    /// darkened along the family curve to meet the AA UI floor (3:1), because the
    /// raw family tint did not clear it on this background — an honest, flagged
    /// minimal legal shift (family hue/chroma preserved, only lightness moved).
    /// `false` for a direct ladder emission and for legal family solids.
    pub floor_coerced: bool,
}

/// A resolved colour and the contrasts it actually achieves.
#[derive(Debug, Clone, PartialEq)]
pub struct SolvedColor {
    /// The colour as `#RRGGBB`.
    pub hex: String,
    /// The signed perceptual contrast `Lc` against the background.
    pub lc: f64,
    /// The WCAG 2.1 ratio (1–21) against the background.
    pub wcag_ratio: f64,
    /// `true` when the legal floor squeezed this role onto the smallest step
    /// below its senior (an honest, flagged hierarchy degradation).
    pub compressed: bool,
    /// `true` when a coloured family label (M1 ch5c) lost perceptible colour on
    /// its contract-solved lightness: the colour's `M'` fell below the tint
    /// perceptibility floor, so at the family curve's extremes (near-white /
    /// near-black) the hue is physically indistinguishable. An honest, flagged
    /// outcome — not a silent degradation to grey. `false` for neutral labels and
    /// for coloured labels that kept a distinguishable colour.
    pub hue_vanished: bool,
    /// Честный замер |ΔJ'| на отданном hex для dJ'-ролей (симметрия с glow);
    /// `None` у контраст-ролей (их метрика — Lc).
    pub achieved_dj: Option<f64>,
    /// `true` when the WCAG legal floor overrode the perceptual target.
    pub floor_override: bool,
    /// The minimum WCAG ratio this role is legally clamped to (`AaText` → 4.5,
    /// `AaUi` → 3.0), or `None` for decorative / JND / zero roles. A property of
    /// the role's contract, not of this solve: a runtime easing between themes
    /// uses it to hold the floor every frame of the transition.
    pub legal_floor: Option<f64>,
}
