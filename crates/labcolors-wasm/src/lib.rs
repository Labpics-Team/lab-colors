//! `@labpics/colors` — WASM bindings over the `labcolors-core` contrast engine.
//!
//! The whole crate is one Clean-Architecture slice:
//! - `theme` — the public theme vocabulary (value object) → core viewing
//!   conditions.
//! - `error` — matchable boundary errors (`thiserror`).
//! - `dto` — framework-free result types (output boundary).
//! - `cache` — the contract cache.
//! - `engine` — the application core: `resolve_set` made generic over roles.
//! - this module — the *only* place `#[wasm_bindgen]` appears: the adapter that
//!   projects the engine's pure results into JS objects.
//!
//! No DOM writes, no CSS side effects — the bindings return data. Applying it to
//! the page (CSS custom properties) is the css-injection-runtime chapter's job;
//! a vanilla helper for that lives in the npm package, not in the WASM core.

mod cache;
// pub: сериализация канонического конфига через DTO (output boundary);
// wasm_bindgen экспортирует только аннотированное — JS-API от этого не растёт.
pub mod config_dto;
mod dto;
mod engine;
mod error;
mod theme;

use wasm_bindgen::prelude::*;

use crate::dto::{ResolvedTheme, RoleOutcome};
use crate::engine::Engine;
use crate::error::BindingError;

/// TypeScript shapes for the values `resolveTheme` returns. wasm-bindgen emits
/// `LabColors.resolveTheme(...): ResolvedTheme` against these, so consumers get
/// full typing without a hand-written `.d.ts`.
#[wasm_bindgen(typescript_custom_section)]
const TS_RESULT_TYPES: &'static str = r##"
/** The stable theme contract. `-ic` variants apply increased contrast; all four spellings are fully supported. */
export type ThemeName = "light" | "dark" | "light-ic" | "dark-ic";

/** A solved colour and the contrasts it actually achieves. */
export interface SolvedColor {
  readonly kind: "color";
  /** The CSS custom-property name for this role, e.g. "--lab-label-primary". */
  readonly cssVar: string;
  /** The resolved colour as #RRGGBB (data; `css`/`vars` carry oklch). */
  readonly hex: string;
  /** Ready-to-serve CSS value: "oklch(L% C H)". `vars` carries the same string. */
  readonly css: string;
  /** Signed perceptual contrast (Lc) against the background. */
  readonly lc: number;
  /** WCAG 2.1 ratio (1–21) against the background. */
  readonly wcagRatio: number;
  /** The legal floor squeezed this role onto the smallest step below its senior. */
  readonly compressed: boolean;
  /**
   * `true` when a coloured family label (M1) lost perceptible colour on its
   * contract-solved lightness: the colour's M′ fell below the tint perceptibility
   * floor, so at the family curve's extremes (near-white / near-black) the hue is
   * physically indistinguishable. An honest, flagged outcome — not a silent
   * degradation to grey. `false` for neutral labels and coloured labels that kept
   * a distinguishable colour.
   */
  readonly hueVanished: boolean;
  /** Честный замер |ΔJ'| на отданном hex для dJ'-ролей; null у контраст-ролей (метрика — lc). */
  readonly achievedDj: number | null;
  /** The WCAG floor overrode the perceptual target. */
  readonly floorOverride: boolean;
  /**
   * The minimum WCAG ratio this role is legally clamped to (4.5 for AA text,
   * 3.0 for AA UI), or `null` for decorative / zero roles. A property of
   * the role's contract, not of this solve: a runtime easing between themes
   * uses it to hold the floor every frame of the transition.
   */
  readonly legalFloor: number | null;
}

/** The explicit zero token: no colour here, by design (not a failure). */
export interface NoneRole {
  readonly kind: "none";
  readonly cssVar: string;
}

/** No colour can satisfy this role on this background, with the reason. */
export interface UnreachableRole {
  readonly kind: "unreachable";
  readonly cssVar: string;
  /** Stable machine code, e.g. "floor_unreachable". */
  readonly code: string;
  /** Human-readable explanation. */
  readonly message: string;
}

/** A semi-transparent ladder / alpha-analog emission: the CSS carries oklch(L% C H / A), the browser composites it. */
export interface TranslucentRole {
  readonly kind: "translucent";
  readonly cssVar: string;
  /** The tint as #RRGGBB (data) — the colour the oklch(… / A) carries. */
  readonly tintHex: string;
  /** The alpha of the emission, (0, 1]. */
  readonly alpha: number;
  /** The solid the tint composites to on the resolve background. */
  readonly compositeHex: string;
  /** Signed perceptual contrast (Lc) of the composite. */
  readonly compositeLc: number;
  /** WCAG 2.1 ratio of the composite. */
  readonly compositeWcag: number;
  /**
   * `true` when the requested alpha was raised to the smallest resolvable value
   * (α_min) because the requested transparency is not reproducible in gamut — an
   * honest, flagged contract degradation (mirrors `SolvedColor.compressed` /
   * `GlowRole.degraded`). The colour never lies: the composite still equals the
   * target solid byte-for-byte; only `alpha` differs from what was asked. Always
   * `false` for a direct ladder emission.
   */
  readonly alphaCoerced: boolean;
  /**
   * `true` when a solid family border (`border-<family>-strong`, M2) was darkened
   * along the family curve to meet the AA UI floor (3:1), because the raw family
   * tint did not clear it on this background — an honest, flagged minimal legal
   * shift (family hue/chroma preserved, only lightness moved). `false` for a
   * direct ladder emission and for legal family solids.
   */
  readonly floorCoerced: boolean;
  /** Ready-to-serve CSS value: "oklch(L% C H / A)". `vars` carries the same string. */
  readonly css: string;
}

/** Свечение (kind glow, labui ADR-0002 §5): screen-слои + решённая интенсивность.
 *  Потребитель красит слои с mix-blend-mode: screen; `vars` несёт
 *  --lab-<role> (halo, oklch), --lab-<role>-core и --lab-<role>-alpha. */
export interface GlowRole {
  readonly kind: "glow";
  readonly cssVar: string;
  /** Слой пересвета (малый радиус), #RRGGBB. */
  readonly coreHex: string;
  /** Слой ореола — источник, #RRGGBB. */
  readonly haloHex: string;
  /** Интенсивность screen-слоя, (0, 1]. */
  readonly alpha: number;
  /** Фактический |ΔJ'| композита от фона. */
  readonly achievedDj: number;
  /** Цель недостижима — ближайший достижимый шаг (ADR-0002, закон 2). */
  readonly degraded: boolean;
  /** Ready-to-serve CSS value халo: "oklch(L% C H)". */
  readonly css: string;
}

export type RoleResult = SolvedColor | TranslucentRole | GlowRole | NoneRole | UnreachableRole;

/** Пер-темная четвёрка якорных hex (light / dark / light-ic / dark-ic). */
export interface ThemeAnchors {
  readonly light: string;
  readonly dark: string;
  readonly light_ic: string;
  readonly dark_ic: string;
}

/** Источник тинта лестницы/альфа-аналога. */
export type LadderSource =
  | { kind: "brand" }
  | { kind: "family"; key: string }
  | { kind: "sentiment"; name: string }
  | { kind: "neutral"; pick: "mid" | "edge" | "inverted" | "light" | "dark" };

/** Рецепт роли из физического меню движка. */
export type RoleRecipe =
  | { kind: "text-anchor"; fraction: number; floor: "aa-text" | "aa-ui" | "none" }
  | { kind: "dj-anchor"; light: number; dark: number }
  | { kind: "decorative-lc"; magnitude: number }
  | { kind: "ladder"; source: LadderSource; position: string }
  | { kind: "glow"; source: LadderSource; step: "subtle" | "base" | "bloom" }
  | { kind: "pair-fill"; source: LadderSource }
  | { kind: "alpha-analog"; of: LadderSource; alpha: number }
  | { kind: "zero" };

/** Именованный пресет ролей. Тонкий конфиг несёт `preset` вместо простыни `roles`. */
export type RolePreset = "labui";

/** Полный конфиг дизайн-системы клиента — вход loadConfig (JSON.stringify(config)). */
export interface ThemeConfig {
  /**
   * Пресет ролей: наполняет словарь дизайн-системы целиком, чтобы клиент вносил
   * ТОЛЬКО значения (якоря, ручки), не семантику. Тонкий конфиг задаёт `preset` и
   * ОПУСКАЕТ `roles`/`aliases`. Задать `preset` вместе с непустыми `roles` —
   * ошибка `invalid_config` (оверрайд отдельных ролей — не этот слой).
   */
  readonly preset?: RolePreset;
  readonly brand: ThemeAnchors;
  readonly neutral: {
    readonly anchors: { light: string; mid: string; dark: string };
    readonly tint: {
      ratio: number;
      target_mp: number;
      hue_stiffness: number;
      hue_override_deg?: number;
    };
    readonly edge?: ThemeAnchors;
    readonly inverted?: ThemeAnchors;
  };
  readonly palette: ReadonlyArray<{ key: string; anchors: ThemeAnchors }>;
  readonly sentiments: {
    readonly categories: ReadonlyArray<{
      name: string;
      family: string;
      hue_floor_deg?: number;
      preferred_side?: -1 | 1;
    }>;
    readonly hardness: number;
    readonly chroma_fraction: number;
  };
  readonly themes: ReadonlyArray<{ name: string; preset: "srgb" | "dim" | "srgb-ic" | "dim-ic" }>;
  /** Опускается в тонком конфиге (задан `preset`); иначе — полный словарь ролей. */
  readonly roles?: ReadonlyArray<{ name: string; recipe: RoleRecipe }>;
  readonly aliases?: ReadonlyArray<{ alias: string; target: string }>;
}

/** The full result of resolving one background under one theme. */
export interface ResolvedTheme {
  readonly theme: ThemeName;
  readonly background: string;
  /**
   * Reachable roles only. Values are ready-to-serve CSS in ONE form:
   * "oklch(L% C H)" for solid roles, "oklch(L% C H / A)" for semi-transparent
   * ladder/alpha-analog roles. Solved in the sRGB gamut (oklch is the
   * notation, not a gamut extension); byte-exact vs `SolvedColor.hex` and
   * `TranslucentRole.tintHex` (`compositeHex` is the background-specific
   * composite, not the emitted token).
   * Scope: this is resolveTheme's contract (applyTheme/watchTheme inject it
   * verbatim); adaptTheme's per-frame easing writes concrete interpolated
   * colours and is not bound by the emission form.
   */
  readonly vars: Record<string, string>;
  /** Every role, keyed by its stable role key (without the --lab- prefix). */
  readonly roles: Record<string, RoleResult>;
}
"##;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ResolvedTheme")]
    pub type JsResolvedTheme;
}

/// A contrast engine over a consumer-supplied design system. Construct with
/// [`LabColors::new`], load a config with [`loadConfig`](LabColors::load_config),
/// then call [`resolve_theme`](LabColors::resolve_theme) many times; identical
/// calls are served from the contract cache.
#[wasm_bindgen]
pub struct LabColors {
    inner: Engine,
}

#[wasm_bindgen]
impl LabColors {
    /// Create an engine with no design system loaded.
    ///
    /// The engine is agnostic (ADR-0001): it carries no built-in role table, so
    /// [`resolveTheme`](LabColors::resolve_theme) rejects with `config_required`
    /// until [`loadConfig`](LabColors::load_config) supplies a design system.
    #[wasm_bindgen(constructor)]
    pub fn new() -> LabColors {
        LabColors {
            inner: Engine::new(),
        }
    }

    /// Resolve every role for `bgHex` under `theme` (`"light" | "dark" |
    /// "light-ic" | "dark-ic"`).
    ///
    /// Returns a `ResolvedTheme` object. Per-role unreachability is part of a
    /// successful result (each role carries its own `kind`); only whole-call
    /// failures reject (no config loaded yet as `config_required`, invalid hex,
    /// unknown theme, and the by-construction-unreachable oklch serialisation
    /// failure as `internal_error`) — as a structured `"<code>: <message>"`
    /// error, never an unwound panic.
    #[wasm_bindgen(js_name = resolveTheme)]
    pub fn resolve_theme(&self, bg_hex: &str, theme: &str) -> Result<JsResolvedTheme, JsError> {
        let theme = crate::theme::parse_theme(theme).map_err(to_js_error)?;
        let resolved = self
            .inner
            .resolve_theme(bg_hex, theme)
            .map_err(to_js_error)?;
        Ok(project_resolved(&resolved)?.unchecked_into())
    }

    /// Загрузить конфиг дизайн-системы (JSON по типу `ThemeConfig` из `.d.ts`).
    ///
    /// Полный preflight движка: невалидный конфиг отклоняется структурной
    /// ошибкой `invalid_config: …` и НЕ меняет состояние. После успешной
    /// загрузки `resolveTheme` эмитит роли конфига (включая полупрозрачные
    /// роли лестницы — эмиссия `oklch(L% C H / α)`). Возвращает отпечаток
    /// конфига — 16 hex-символов;
    /// разные конфиги дают разные отпечатки (и разные кэш-пространства).
    #[wasm_bindgen(js_name = loadConfig)]
    pub fn load_config(&mut self, json: &str) -> Result<String, JsError> {
        let fp = self.inner.load_config(json).map_err(to_js_error)?;
        Ok(format!("{fp:016x}"))
    }

    /// Recheck the contrasts `fgHexes` achieve against `bgHex` under `theme` —
    /// the cheap per-frame primitive a reactive runtime uses to decide whether
    /// already-resolved colours still pass against a changed background (re-solve
    /// only when they stably do not). No full solve: one perceptual-model forward
    /// for the background plus one per foreground.
    ///
    /// Returns a `Float64Array` of `[lc, wcagRatio]` pairs, interleaved and in the
    /// order of `fgHexes`: index `2*i` is foreground `i`'s signed `Lc`, `2*i+1`
    /// its WCAG ratio. Rejects (structured `{code, message}`) on an invalid hex or
    /// an unknown theme.
    #[wasm_bindgen(js_name = recheckContrast)]
    pub fn recheck_contrast(
        &self,
        bg_hex: &str,
        fg_hexes: Vec<String>,
        theme: &str,
    ) -> Result<Vec<f64>, JsError> {
        let theme = crate::theme::parse_theme(theme).map_err(to_js_error)?;
        self.inner
            .recheck(bg_hex, &fg_hexes, theme)
            .map_err(to_js_error)
    }

    /// Calculate the muddiness score (0 to 1) of an sRGB hex colour.
    #[wasm_bindgen(js_name = muddiness)]
    pub fn muddiness(&self, hex: &str) -> Result<f64, JsError> {
        labcolors_core::cleanliness::muddiness_from_hex(hex)
            .map_err(|reason| to_js_error(BindingError::InvalidBackground { reason }))
    }
}

impl Default for LabColors {
    fn default() -> Self {
        Self::new()
    }
}

/// Project a pure [`ResolvedTheme`] into the JS object the `.d.ts` describes.
///
/// Built generically from the role vector — no role is named here, so the set
/// can grow without touching this function.
fn project_resolved(resolved: &ResolvedTheme) -> Result<JsValue, JsError> {
    let out = js_sys::Object::new();
    set(&out, "theme", &JsValue::from_str(resolved.theme));
    set(&out, "background", &JsValue::from_str(&resolved.background));

    let vars = js_sys::Object::new();
    let roles = js_sys::Object::new();
    for entry in &resolved.roles {
        let css_var = format!("--lab-{}", entry.role_key);
        let role_obj = js_sys::Object::new();
        set(&role_obj, "cssVar", &JsValue::from_str(&css_var));
        match &entry.outcome {
            RoleOutcome::Color(c) => {
                set(&role_obj, "kind", &JsValue::from_str("color"));
                set(&role_obj, "hex", &JsValue::from_str(&c.hex));
                set(&role_obj, "lc", &JsValue::from_f64(c.lc));
                set(&role_obj, "wcagRatio", &JsValue::from_f64(c.wcag_ratio));
                set(&role_obj, "compressed", &JsValue::from_bool(c.compressed));
                set(
                    &role_obj,
                    "hueVanished",
                    &JsValue::from_bool(c.hue_vanished),
                );
                set(
                    &role_obj,
                    "achievedDj",
                    &c.achieved_dj.map_or(JsValue::NULL, JsValue::from_f64),
                );
                set(
                    &role_obj,
                    "floorOverride",
                    &JsValue::from_bool(c.floor_override),
                );
                set(
                    &role_obj,
                    "legalFloor",
                    &c.legal_floor.map_or(JsValue::NULL, JsValue::from_f64),
                );
                // Единая форма эмиссии: oklch и для солида (hex остаётся
                // данными роли; синтаксис переменной один на все исходы).
                let css = oklch_css(&c.hex, None)?;
                set(&role_obj, "css", &JsValue::from_str(&css));
                set(&vars, &css_var, &JsValue::from_str(&css));
            }
            RoleOutcome::Translucent(r) => {
                set(&role_obj, "kind", &JsValue::from_str("translucent"));
                set(&role_obj, "tintHex", &JsValue::from_str(&r.tint_hex));
                set(&role_obj, "alpha", &JsValue::from_f64(r.alpha));
                set(
                    &role_obj,
                    "compositeHex",
                    &JsValue::from_str(&r.composite_hex),
                );
                set(&role_obj, "compositeLc", &JsValue::from_f64(r.composite_lc));
                set(
                    &role_obj,
                    "compositeWcag",
                    &JsValue::from_f64(r.composite_wcag),
                );
                set(
                    &role_obj,
                    "alphaCoerced",
                    &JsValue::from_bool(r.alpha_coerced),
                );
                set(
                    &role_obj,
                    "floorCoerced",
                    &JsValue::from_bool(r.floor_coerced),
                );
                // Переменная несёт тинт в oklch со слэш-альфой — браузер
                // композитит на живой подложке; форма едина с солидами.
                let css = oklch_css(&r.tint_hex, Some(r.alpha))?;
                set(&role_obj, "css", &JsValue::from_str(&css));
                set(&vars, &css_var, &JsValue::from_str(&css));
            }
            RoleOutcome::Glow(g) => {
                // Свечение: слои для screen-наложения потребителем.
                // --lab-<role> несёт halo (единая oklch-форма), сателлиты
                // --lab-<role>-core / --lab-<role>-alpha — анатомия и
                // решённая интенсивность (число, не цвет).
                set(&role_obj, "kind", &JsValue::from_str("glow"));
                set(&role_obj, "coreHex", &JsValue::from_str(&g.core_hex));
                set(&role_obj, "haloHex", &JsValue::from_str(&g.halo_hex));
                set(&role_obj, "alpha", &JsValue::from_f64(g.alpha));
                set(&role_obj, "achievedDj", &JsValue::from_f64(g.achieved_dj));
                set(&role_obj, "degraded", &JsValue::from_bool(g.degraded));
                let halo_css = oklch_css(&g.halo_hex, None)?;
                let core_css = oklch_css(&g.core_hex, None)?;
                set(&role_obj, "css", &JsValue::from_str(&halo_css));
                set(&vars, &css_var, &JsValue::from_str(&halo_css));
                set(
                    &vars,
                    &format!("{css_var}-core"),
                    &JsValue::from_str(&core_css),
                );
                set(
                    &vars,
                    &format!("{css_var}-alpha"),
                    &JsValue::from_str(&format!("{:.4}", g.alpha)),
                );
            }
            RoleOutcome::None => {
                set(&role_obj, "kind", &JsValue::from_str("none"));
            }
            RoleOutcome::Unreachable { code, message } => {
                set(&role_obj, "kind", &JsValue::from_str("unreachable"));
                set(&role_obj, "code", &JsValue::from_str(code));
                set(&role_obj, "message", &JsValue::from_str(message));
            }
        }
        set(&roles, &entry.role_key, &role_obj);
    }
    set(&out, "vars", &vars);
    set(&out, "roles", &roles);
    Ok(out.into())
}

/// Единая CSS-форма эмиссии: `oklch(L% C H)` / `oklch(L% C H / A)`.
/// Байт-точность реконструкции доказана round-trip тестом ядра на решётке
/// куба. Hex к этому месту валиден по построению (солвер/лестница), но при
/// невозможном парсе — честная структурная ошибка, НЕ тихая подмена формы:
/// потребитель ждёт oklch, а полупрозрачная роль при подмене ещё и потеряла бы альфу.
fn oklch_css(hex: &str, alpha: Option<f64>) -> Result<String, JsError> {
    labcolors_core::oklch_css_from_hex(hex, alpha).map_err(|reason| {
        to_js_error(BindingError::Internal {
            reason: format!("резолвнутый цвет не сериализуется в oklch: {reason}"),
        })
    })
}

/// Set a property on a JS object. `Reflect::set` on a freshly created `Object`
/// cannot fail (the target is always a real object and the key a string), so
/// the result is intentionally ignored — there is no recoverable error here and
/// nothing to surface to the caller.
fn set(target: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(target, &JsValue::from_str(key), value);
}

/// Turn a boundary error into a JS `Error` carrying both the stable machine
/// code and the human reason, so JS can branch on the cause without a custom
/// error class. Format: `"<code>: <message>"`.
///
/// A free function rather than a `From` impl: `thiserror` already gives
/// `BindingError` a blanket `From<E: Error> for JsError` via wasm-bindgen, and
/// that path would drop the stable code. This keeps the code in the message.
fn to_js_error(err: BindingError) -> JsError {
    JsError::new(&format!("{}: {}", err.code(), err))
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_muddiness() {
        let colors = LabColors::new();

        // Olive is highly muddy
        let olive_mud = colors.muddiness("#6B6B2E").unwrap();
        assert!(olive_mud > 0.80);
        assert!((olive_mud - 0.8699).abs() < 1e-3);

        // Gray is clean
        let gray_mud = colors.muddiness("#808080").unwrap();
        assert!(gray_mud < 0.05);

        // Invalid hex returns an error
        assert!(colors.muddiness("not_a_hex").is_err());
    }
}
