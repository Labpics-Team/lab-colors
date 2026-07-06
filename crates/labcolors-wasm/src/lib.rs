//! `@labpics/colors` — WASM bindings over the `labcolors-core` contrast engine.
//!
//! The whole crate is one Clean-Architecture slice:
//! - `theme` — the public theme vocabulary (value object) → core viewing
//!   conditions.
//! - `error` — matchable boundary errors (`thiserror`).
//! - `dto` — framework-free result types (output boundary).
//! - `cache` — the contract cache.
//! - `engine` — the application core: `resolve_set` made generic over roles.
//! - `project` — результат как JSON-текст: чистая, нативно тестируемая
//!   сериализация, которую адаптер отдаёт `JSON.parse` (задача #54).
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
mod project;
mod theme;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use wasm_bindgen::prelude::*;

use crate::dto::ResolvedTheme;
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
  | { kind: "pair-label"; source: LadderSource; fraction: number; floor: "aa-text" | "aa-ui" | "none" }
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
/// Запись мемо проекций: адрес Rc-аллокации записи контракт-кэша →
/// (`Weak` для проверки, что аллокация всё ещё та самая, готовый JSON-текст).
type ProjectionMemo = HashMap<usize, (Weak<ResolvedTheme>, Rc<String>)>;

#[wasm_bindgen]
pub struct LabColors {
    inner: Engine,
    /// Мемо сериализованной проекции по живым записям контракт-кэша движка —
    /// см. [`LabColors::projection_json`]. `RefCell`: wasm однопоточен, а
    /// `resolveTheme` принимает `&self`.
    projection_memo: RefCell<ProjectionMemo>,
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
            projection_memo: RefCell::new(HashMap::new()),
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
        let json = self.projection_json(&resolved).map_err(to_js_error)?;
        Ok(parse_projection(&json)?.unchecked_into())
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

    /// PROTOTYPE — NOT part of the public API (unlisted, `_`-prefixed; may change
    /// or be removed without notice). Recheck one foreground set against MANY
    /// background samples in a single call. The reactive controller's worst-case
    /// loop rechecks the same foregrounds against every sample of a varying
    /// backdrop; the dominant per-foreground CAM16 forward is background-
    /// independent, so this shares it across all samples instead of recomputing it
    /// per `recheckContrast` call.
    ///
    /// Returns a flat, background-major `Float64Array`: sample `s`, foreground `i`
    /// is at `(s * fgHexes.length + i) * 2` (`lc`) and `+1` (`wcagRatio`). The
    /// values are byte-identical to calling `recheckContrast(bgHexes[s], fgHexes,
    /// theme)` for each `s`. Wiring the runtime to it is an OWNER decision (it
    /// changes the controller↔engine call shape); it is measured, not adopted.
    #[wasm_bindgen(js_name = _recheckContrastMulti)]
    pub fn _recheck_contrast_multi(
        &self,
        bg_hexes: Vec<String>,
        fg_hexes: Vec<String>,
        theme: &str,
    ) -> Result<Vec<f64>, JsError> {
        let theme = crate::theme::parse_theme(theme).map_err(to_js_error)?;
        self.inner
            .recheck_multi(&bg_hexes, &fg_hexes, theme)
            .map_err(to_js_error)
    }
}

impl Default for LabColors {
    fn default() -> Self {
        Self::new()
    }
}

impl LabColors {
    /// JSON-текст проекции записи контракт-кэша — посчитанный не более одного
    /// раза на живую запись (перф-форма задачи #54, этап 2).
    ///
    /// [`Engine::resolve_theme`] на cache-hit возвращает `Rc` на ту же самую
    /// аллокацию [`ResolvedTheme`]; её содержимое иммутабельно, значит
    /// идентичность аллокации влечёт идентичность сериализации — и hit-путь
    /// не обязан пересобирать JSON-строку (форматирование ~30 oklch-значений)
    /// заново.
    ///
    /// Корректность (мемо никогда не отдаёт чужой/устаревший текст): хит
    /// засчитывается только если `Weak` апгрейдится И полученный `Rc`
    /// `ptr_eq` текущему. Умершая запись не апгрейдится никогда
    /// (strong == 0 необратим), поэтому переиспользование её адреса новой
    /// аллокацией (ABA) хита не даёт — будет честный пересчёт и перезапись.
    ///
    /// Ограниченность памяти зеркалит политику `ContractCache`: при
    /// достижении ёмкости сначала выметаются умершие записи; если живых всё
    /// ещё сверх ёмкости — снос целиком. Холодная пересборка — да, неверный
    /// ответ — никогда.
    fn projection_json(&self, resolved: &Rc<ResolvedTheme>) -> Result<Rc<String>, BindingError> {
        let key = Rc::as_ptr(resolved) as usize;
        if let Some((weak, json)) = self.projection_memo.borrow().get(&key)
            && weak
                .upgrade()
                .is_some_and(|live| Rc::ptr_eq(&live, resolved))
        {
            return Ok(Rc::clone(json));
        }
        let json = Rc::new(project::resolved_to_json(resolved)?);
        let mut memo = self.projection_memo.borrow_mut();
        if memo.len() >= crate::engine::CACHE_CAPACITY {
            memo.retain(|_, (weak, _)| weak.strong_count() > 0);
            if memo.len() >= crate::engine::CACHE_CAPACITY {
                memo.clear();
            }
        }
        memo.insert(key, (Rc::downgrade(resolved), Rc::clone(&json)));
        Ok(json)
    }
}

/// Развернуть JSON-текст проекции в свежий JS-граф одним нативным
/// `JSON.parse` — два пересечения границы на вызов вместо сотен
/// `Reflect::set` (≈30 ролей × ≈10 свойств; перф-форма задачи #54, этап 1).
/// Порядок ключей и значения идентичны прежней по-полевой проекции
/// (лок: `resolve-projection-parity.test.mjs`); каждый вызов по-прежнему
/// возвращает свежий граф.
fn parse_projection(json: &str) -> Result<JsValue, JsError> {
    js_sys::JSON::parse(json).map_err(|_| {
        // По построению недостижимо: сериализатор эмитит валидный JSON (лок
        // нативным тестом). При нарушении — честная структурная ошибка,
        // не unwound-паника.
        to_js_error(BindingError::Internal {
            reason: "JSON.parse отверг сериализованную проекцию результата".into(),
        })
    })
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod projection_memo_tests {
    use super::*;
    use crate::theme::Theme;

    const LABUI: &str = include_str!("../tests/data/labui.config.json");

    #[test]
    fn hit_reuses_the_serialised_string_for_the_same_cache_entry() {
        let mut colors = LabColors::new();
        colors.inner.load_config(LABUI).unwrap();
        let first = colors.inner.resolve_theme("#3A3A3C", Theme::Dark).unwrap();
        let hit = colors.inner.resolve_theme("#3A3A3C", Theme::Dark).unwrap();
        assert!(
            Rc::ptr_eq(&first, &hit),
            "прекондиция: контракт-кэш движка отдаёт ту же аллокацию"
        );
        let j1 = colors.projection_json(&first).unwrap();
        let j2 = colors.projection_json(&hit).unwrap();
        assert!(
            Rc::ptr_eq(&j1, &j2),
            "hit обязан переиспользовать сериализацию, не пересобирать её"
        );
    }

    #[test]
    fn dead_entry_never_hits_fresh_serialisation_is_identical() {
        let mut colors = LabColors::new();
        colors.inner.load_config(LABUI).unwrap();
        let a = colors.inner.resolve_theme("#3A3A3C", Theme::Dark).unwrap();
        let ja = colors.projection_json(&a).unwrap();
        drop(a);
        // Перезагрузка конфига сносит контракт-кэш движка — прежняя запись
        // мертва; тот же конфиг ⇒ новая аллокация с тем же содержимым.
        colors.inner.load_config(LABUI).unwrap();
        let b = colors.inner.resolve_theme("#3A3A3C", Theme::Dark).unwrap();
        let jb = colors.projection_json(&b).unwrap();
        assert!(
            !Rc::ptr_eq(&ja, &jb),
            "умершая запись не должна давать хит (Weak не апгрейдится)"
        );
        assert_eq!(
            ja.as_str(),
            jb.as_str(),
            "независимые сериализации одного содержимого идентичны"
        );
    }

    #[test]
    fn memo_sweeps_dead_entries_and_stays_bounded() {
        let mut colors = LabColors::new();
        colors.inner.load_config(LABUI).unwrap();
        {
            let mut memo = colors.projection_memo.borrow_mut();
            for i in 0..crate::engine::CACHE_CAPACITY {
                let dead = Rc::new(ResolvedTheme {
                    theme: "dark",
                    background: String::new(),
                    roles: Vec::new(),
                });
                let weak = Rc::downgrade(&dead);
                drop(dead);
                memo.insert(usize::MAX - i, (weak, Rc::new(String::new())));
            }
        }
        let live = colors.inner.resolve_theme("#3A3A3C", Theme::Dark).unwrap();
        let _ = colors.projection_json(&live).unwrap();
        assert_eq!(
            colors.projection_memo.borrow().len(),
            1,
            "переполнение выметает умершие записи; остаётся только живая"
        );
    }
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
