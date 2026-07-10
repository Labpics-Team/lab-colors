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
    /// Двухслойный материал (kind material, #89): тинт `01` (с выведенной α) +
    /// опаковая база `02`, обе — один тон. `--lab-<role>-01` несёт
    /// `oklch(<tone> / α)`, `--lab-<role>-02` и `--lab-<role>` — `oklch(<tone>)`
    /// (солид-канон/опаковая база); композит-гарантия читаемости — в полях.
    Material(MaterialColor),
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
    /// Core-слой, предназначенный потребителем для меньшего blur, `#RRGGBB`;
    /// сам движок геометрию не моделирует.
    pub core_hex: String,
    /// Halo-слой, предназначенный потребителем для большего blur, `#RRGGBB`;
    /// в recipe v1 равен источнику.
    pub halo_hex: String,
    /// Интенсивность screen-слоя `(0, 1]`.
    pub alpha: f64,
    /// Каноническая CSS-запись той же alpha без повторного округления.
    pub alpha_css: String,
    /// Целевой |ΔJ′| изолированного halo-композита.
    pub target_dj: f64,
    /// Версия конечного reference-домена point-расчёта.
    pub reference_profile: &'static str,
    /// Слой, по которому решалась цель.
    pub constraint_layer: labcolors_core::GlowConstraintLayer,
    /// Типизированный результат target-проверки.
    pub target_status: labcolors_core::GlowTargetStatus,
    /// Reference-композит изолированного halo.
    pub halo_composite_hex: String,
    /// Фактический |ΔJ′| изолированного halo-композита.
    pub halo_achieved_dj: f64,
    /// Reference-композит изолированного core с той же alpha.
    pub core_composite_hex: String,
    /// Фактический |ΔJ′| изолированного core-композита.
    pub core_achieved_dj: f64,
}

/// Двухслойный материал (kind material): тон + выведенная α + вердикт гарантии.
///
/// Тинт `01`, база `02` и солид-канон равны тону по построению (композит `T` над
/// `T` есть `T`), поэтому один `tone_hex`. `alpha` выведена как минимальная
/// плотность, при которой композит тона над худшим фоном коридора держит `floor`.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialColor {
    /// Тон `#RRGGBB`: тинт `01`, база `02` и солид-канон одновременно.
    pub tone_hex: String,
    /// Выведенная альфа тинта `01`, `(0, 1]`.
    pub alpha: f64,
    /// Худший WCAG-контраст коммит-полюса по коридору `[чёрный, белый]`.
    pub worst_contrast: f64,
    /// WCAG-пол читаемости, который держит α (4.5 / 3.0).
    pub floor: f64,
    /// Гарантия выполнена: `worst_contrast ≥ floor`. `false` — пол недостижим
    /// даже при α = 1 (честная деградация, α = 1 как ближайшая достижимая).
    pub guaranteed: bool,
    /// Коммит-полюс поверхности белый (`true`, тёмный тон) или чёрный (`false`).
    pub pole_white: bool,
    /// Фактический |ΔJ'| тона-базы от фона резолва — различимость поверхности.
    pub achieved_dj: f64,
    /// Целевой |ΔJ'| тона был недостижим — ближайший достижимый (ADR-0002).
    pub tone_compressed: bool,
    /// Оттенок семьи выродился у края гамута (честный флаг; `false` у нейтрали).
    pub hue_vanished: bool,
    /// Солид-канон отличим от фона резолва на 8-битной сетке дисплея.
    pub distinct: bool,
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
