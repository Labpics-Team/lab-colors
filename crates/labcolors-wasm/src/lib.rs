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
mod dto;
mod engine;
mod error;
mod theme;

use wasm_bindgen::prelude::*;

use crate::dto::{ResolvedTheme, RoleOutcome};
use crate::engine::Engine;
use crate::error::BindingError;
use crate::theme::Theme;

/// TypeScript shapes for the values `resolveTheme` returns. wasm-bindgen emits
/// `LabColors.resolveTheme(...): ResolvedTheme` against these, so consumers get
/// full typing without a hand-written `.d.ts`.
#[wasm_bindgen(typescript_custom_section)]
const TS_RESULT_TYPES: &'static str = r##"
/** The stable theme contract. `-ic` variants are reserved (not yet calibrated). */
export type ThemeName = "light" | "dark" | "light-ic" | "dark-ic";

/** A solved colour and the contrasts it actually achieves. */
export interface SolvedColor {
  readonly kind: "color";
  /** The CSS custom-property name for this role, e.g. "--lab-text-primary". */
  readonly cssVar: string;
  /** The resolved colour as #RRGGBB. */
  readonly hex: string;
  /** Signed perceptual contrast (Lc) against the background. */
  readonly lc: number;
  /** WCAG 2.1 ratio (1–21) against the background. */
  readonly wcagRatio: number;
  /** The legal floor squeezed this role onto the smallest step below its senior. */
  readonly compressed: boolean;
  /** The WCAG floor overrode the perceptual target. */
  readonly floorOverride: boolean;
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

export type RoleResult = SolvedColor | NoneRole | UnreachableRole;

/** The full result of resolving one background under one theme. */
export interface ResolvedTheme {
  readonly theme: ThemeName;
  readonly background: string;
  /** Reachable roles only: { "--lab-text-primary": "#1a1a1a", ... }. */
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
    /// `init(config)` in the task: v1 takes no config (the brand/anchor seam is
    /// reserved). Adding an optional config object later is additive — it does
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
    /// failures (invalid hex, unknown or uncalibrated theme) reject — as a
    /// structured `{ code, message }` error, never an unwound panic.
    #[wasm_bindgen(js_name = resolveTheme)]
    pub fn resolve_theme(&self, bg_hex: &str, theme: &str) -> Result<JsResolvedTheme, JsError> {
        let theme = Theme::parse(theme).map_err(to_js_error)?;
        let resolved = self
            .inner
            .resolve_theme(bg_hex, theme)
            .map_err(to_js_error)?;
        Ok(project_resolved(&resolved).unchecked_into())
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
                set(&vars, &css_var, &JsValue::from_str(&c.hex));
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
        set(&roles, entry.role_key, &role_obj);
    }
    set(&out, "vars", &vars);
    set(&out, "roles", &roles);
    out.into()
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
