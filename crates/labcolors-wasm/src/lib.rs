//! `@labpics/colors` — WASM bindings over the `labcolors-core` contrast engine.
//!
//! The whole crate is one Clean-Architecture slice:
//! - [`theme`] — the public theme vocabulary (value object) → core viewing
//!   conditions.
//! - [`error`] — matchable boundary errors (`thiserror`).
//! - [`dto`] — framework-free result types (output boundary).
//! - [`cache`] — the contract cache.
//! - [`engine`] — the application core: `resolve_set` made generic over roles.
//! - this module — the *only* place `#[wasm_bindgen]` appears: the adapter that
//!   projects the engine's pure results into JS objects.
//!
//! No DOM writes, no CSS side effects — the bindings return data. Applying it to
//! the page (CSS custom properties) is the css-injection-runtime chapter's job;
//! a vanilla helper for that lives in the npm package, not in the WASM core.

mod cache;
// pub: эмиттер паспорта (examples/emit_passport.rs) сериализует канонический
// конфиг через DTO; wasm_bindgen экспортирует только аннотированное — JS-API не растёт.
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

/** A semi-transparent ladder / alpha-analog emission: the CSS carries rgba(), the browser composites it. */
export interface RgbaRole {
  readonly kind: "rgba";
  readonly cssVar: string;
  /** The tint as #RRGGBB — the colour the rgba() carries. */
  readonly tintHex: string;
  /** The alpha of the emission, (0, 1]. */
  readonly alpha: number;
  /** The solid the tint composites to on the resolve background. */
  readonly compositeHex: string;
  /** Signed perceptual contrast (Lc) of the composite. */
  readonly compositeLc: number;
  /** WCAG 2.1 ratio of the composite. */
  readonly compositeWcag: number;
  /** Ready-to-serve CSS value: "oklch(L% C H / A)". `vars` carries the same string. */
  readonly css: string;
}

export type RoleResult = SolvedColor | RgbaRole | NoneRole | UnreachableRole;

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
  | { kind: "alpha-analog"; of: LadderSource; alpha: number }
  | { kind: "zero" };

/** Полный конфиг дизайн-системы клиента — вход loadConfig (JSON.stringify(config)). */
export interface ThemeConfig {
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
  readonly roles: ReadonlyArray<{ name: string; recipe: RoleRecipe }>;
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
   * notation, not a gamut extension); byte-exact vs the role's hex fields.
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

/// A configured contrast engine. Construct with [`LabColors::new`], then call
/// [`resolve_theme`](LabColors::resolve_theme) many times; identical calls are
/// served from the contract cache.
#[wasm_bindgen]
pub struct LabColors {
    inner: Engine,
}

#[wasm_bindgen]
impl LabColors {
    /// Create a zero-config engine on the default role table and the default
    /// per-theme viewing conditions.
    ///
    /// `v1` takes no config; the brand/anchor seam is left for a future
    /// version. Adding an optional config object later is additive — it does
    /// not change this signature.
    #[wasm_bindgen(constructor)]
    pub fn new() -> LabColors {
        LabColors {
            inner: Engine::new(),
        }
    }

    /// Resolve every role for `bgHex` under `theme` (`"light" | "dark" |
    /// "light-ic" | "dark-ic"`).
    ///
    /// Returns a [`ResolvedTheme`] object. Per-role unreachability is part of a
    /// successful result (each role carries its own `kind`); only whole-call
    /// failures (invalid hex, unknown theme) reject — as a
    /// structured `{ code, message }` error, never an unwound panic.
    #[wasm_bindgen(js_name = resolveTheme)]
    pub fn resolve_theme(&self, bg_hex: &str, theme: &str) -> Result<JsResolvedTheme, JsError> {
        let theme = crate::theme::parse_theme(theme).map_err(to_js_error)?;
        let resolved = self
            .inner
            .resolve_theme(bg_hex, theme)
            .map_err(to_js_error)?;
        Ok(project_resolved(&resolved).unchecked_into())
    }

    /// Загрузить конфиг дизайн-системы (JSON по типу `ThemeConfig` из `.d.ts`).
    ///
    /// Полный preflight движка: невалидный конфиг отклоняется структурной
    /// ошибкой `invalid_config: …` и НЕ меняет состояние. После успешной
    /// загрузки `resolveTheme` эмитит роли конфига (включая полупрозрачные
    /// `rgba`-роли лестницы). Возвращает отпечаток конфига — 16 hex-символов;
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

    /// Calculate a relative confidence score for the [`muddiness`](Self::muddiness)
    /// call on an sRGB hex colour: `0` means the call is unreliable (near the
    /// decision boundary or the grey frontier), higher means more confident.
    /// The practical ceiling is an internal calibration detail, not a public
    /// contract — do not hardcode an upper bound against this value.
    #[wasm_bindgen(js_name = confidence)]
    pub fn confidence(&self, hex: &str) -> Result<f64, JsError> {
        labcolors_core::cleanliness::confidence_from_hex(hex)
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
fn project_resolved(resolved: &ResolvedTheme) -> JsValue {
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
                set(
                    &role_obj,
                    "css",
                    &JsValue::from_str(&oklch_css(&c.hex, None)),
                );
                set(
                    &vars,
                    &css_var,
                    &JsValue::from_str(&oklch_css(&c.hex, None)),
                );
            }
            RoleOutcome::Rgba(r) => {
                set(&role_obj, "kind", &JsValue::from_str("rgba"));
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
                // Переменная несёт тинт в oklch со слэш-альфой — браузер
                // композитит на живой подложке; форма едина с солидами.
                let css = oklch_css(&r.tint_hex, Some(r.alpha));
                set(&role_obj, "css", &JsValue::from_str(&css));
                set(&vars, &css_var, &JsValue::from_str(&css));
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
    out.into()
}

/// Единая CSS-форма эмиссии: `oklch(L% C H)` / `oklch(L% C H / A)`.
/// Байт-точность реконструкции доказана round-trip тестом ядра на решётке
/// куба; hex к этому месту валиден по построению (солвер/лестница), поэтому
/// ошибка парса невозможна — ветка Err недостижима и схлопнута в сам hex
/// (честнее уронить видимым мусором, чем тихо подменить цвет).
fn oklch_css(hex: &str, alpha: Option<f64>) -> String {
    labcolors_core::oklch_css_from_hex(hex, alpha).unwrap_or_else(|_| hex.to_string())
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
    fn test_wasm_muddiness_and_confidence() {
        let colors = LabColors::new();

        // Olive is highly muddy
        let olive_mud = colors.muddiness("#6B6B2E").unwrap();
        let olive_conf = colors.confidence("#6B6B2E").unwrap();
        assert!(olive_mud > 0.80);
        assert!((olive_mud - 0.8699).abs() < 1e-3);
        assert!((olive_conf - 0.2945).abs() < 1e-3);

        // Gray is clean
        let gray_mud = colors.muddiness("#808080").unwrap();
        assert!(gray_mud < 0.05);

        // Invalid hex returns an error
        assert!(colors.muddiness("not_a_hex").is_err());
        assert!(colors.confidence("not_a_hex").is_err());
    }
}
