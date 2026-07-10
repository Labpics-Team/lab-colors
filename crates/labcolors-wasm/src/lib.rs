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
mod projection;
mod theme;

use wasm_bindgen::prelude::*;

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
  /** Готовое CSS-значение. При powerless chroma вместо выдуманного H стоит `none`; `vars` несёт ту же строку. */
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

/** Полупрозрачная эмиссия: CSS несёт oklch(L% C (H или none) / A), а браузер композитит её. */
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
  /** Готовое CSS-значение; powerless hue сериализуется как `none`. `vars` несёт ту же строку. */
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
  /** Готовое CSS-значение halo; у ахромата H отсутствует и записывается как `none`. */
  readonly css: string;
}

/** Двухслойный материал (kind material, #89): тинт `01` (с выведенной α) над
 *  опаковой базой `02`, обе — один тон. `vars` несёт --lab-<role> (солид-канон,
 *  опаковый), --lab-<role>-01 (тинт oklch/α) и --lab-<role>-02 (база, опаковая).
 *  Композит-гарантия читаемости пересчитываема из toneHex/alpha (α-граница). */
export interface MaterialRole {
  readonly kind: "material";
  readonly cssVar: string;
  /** Тон #RRGGBB: тинт 01, база 02 и солид-канон одновременно (равны). */
  readonly toneHex: string;
  /** Выведенная альфа тинта 01, (0, 1]. */
  readonly alpha: number;
  /** Худший WCAG-контраст коммит-полюса по коридору [чёрный, белый]. */
  readonly worstContrast: number;
  /** WCAG-пол читаемости, который держит α (4.5 / 3.0). */
  readonly floor: number;
  /** Гарантия выполнена: worstContrast ≥ floor. `false` — пол недостижим даже
   *  при α = 1 (честная деградация, α = 1 как ближайшая достижимая). */
  readonly guaranteed: boolean;
  /** Коммит-полюс поверхности белый (true, тёмный тон) или чёрный (false). */
  readonly poleWhite: boolean;
  /** Фактический |ΔJ'| тона-базы от фона — различимость поверхности. */
  readonly achievedDj: number;
  /** Целевой |ΔJ'| тона был недостижим — ближайший достижимый (ADR-0002). */
  readonly toneCompressed: boolean;
  /** Оттенок семьи выродился у края гамута (честный флаг; false у нейтрали). */
  readonly hueVanished: boolean;
  /** Солид-канон отличим от фона резолва на 8-битной сетке. */
  readonly distinct: boolean;
  /** Готовое CSS-значение солид-канона; powerless hue записывается как `none`. */
  readonly css: string;
}

export type RoleResult =
  | SolvedColor
  | TranslucentRole
  | GlowRole
  | MaterialRole
  | NoneRole
  | UnreachableRole;

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
  | { kind: "material"; source: LadderSource; tone_light: number; tone_dark: number; floor: "aa-text" | "aa-ui" }
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
    readonly geometry?: "anchor-distance-v2";
    readonly categories: ReadonlyArray<{
      name: string;
      family: string;
    }>;
  };
  readonly themes: ReadonlyArray<{ name: string; preset: "srgb" | "dim" | "srgb-ic" | "dim-ic" }>;
  /** Словарь ролей дизайн-системы. Конфиг обязан нести собственные роли; пустой
   *  контракт (без `roles` и `aliases`) отклоняется на загрузке. */
  readonly roles?: ReadonlyArray<{ name: string; recipe: RoleRecipe }>;
  readonly aliases?: ReadonlyArray<{ alias: string; target: string }>;
}

/** Полный результат резолва одного фона в одной теме. */
export interface ResolvedTheme {
  readonly theme: ThemeName;
  readonly background: string;
  /**
   * Только достижимые роли. Значение — готовый CSS Oklch; у powerless chroma
   * компонент H записан как `none`, у полупрозрачной роли добавлена `/ A`.
   * Цвет решён в sRGB: Oklch здесь форма записи, а не расширение gamut.
   * Строка байт-точна относительно `SolvedColor.hex` или tintHex; compositeHex
   * описывает уже композит на фоне и не является эмитированным токеном.
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

/// Движок контраста над дизайн-системой потребителя. После загрузки конфига
/// одинаковые вызовы получают готовый снимок из тематического слота кэша.
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
        // DTO и строка имеют один кэш-ключ и живут в одном снимке. Поэтому
        // чередование тем не превращает попадание решателя в промах проекции.
        // `JSON.parse` всё равно создаёт свежий объект: мутация результата в JS
        // не меняет ни кэш, ни следующий ответ.
        // По построению строка — валидный JSON; невозможный отказ парсера —
        // честная внутренняя ошибка, не паника.
        let parsed = js_sys::JSON::parse(resolved.json()).map_err(|_| {
            to_js_error(BindingError::Internal {
                reason: "проекция не распарсилась как JSON".to_string(),
            })
        })?;
        Ok(parsed.unchecked_into())
    }

    /// Загрузить конфиг дизайн-системы (JSON по типу `ThemeConfig` из `.d.ts`).
    ///
    /// Полный preflight движка: невалидный конфиг отклоняется структурной
    /// ошибкой `invalid_config: …` и НЕ меняет состояние. После успешной
    /// загрузки `resolveTheme` эмитит роли конфига (включая полупрозрачные
    /// роли лестницы — эмиссия `oklch(L% C (H|none) / α)`). Возвращает отпечаток
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

    /// Вычисляет замороженную историческую характеристику Закона Грязи V1.
    ///
    /// Исходное имя сохраняет совместимость API, но результат не является
    /// вероятностью для популяции и не подменяет контекстную модель V2.
    #[wasm_bindgen(js_name = muddiness)]
    pub fn muddiness(&self, hex: &str) -> Result<f64, JsError> {
        labcolors_core::cleanliness::muddiness_from_hex(hex)
            .map_err(|reason| to_js_error(BindingError::InvalidBackground { reason }))
    }

    /// Recheck one foreground set against MANY background samples in a single
    /// call. The reactive controller's worst-case loop rechecks the same
    /// foregrounds against every sample of a varying backdrop (gradient / image /
    /// bg-blur / glass); the dominant per-foreground CAM16 forward is background-
    /// independent, so this shares it across all samples instead of recomputing it
    /// per `recheckContrast` call — a measured ~2.6x on the multi-sample recheck.
    ///
    /// Returns a flat, background-major `Float64Array`: sample `s`, foreground `i`
    /// is at `(s * fgHexes.length + i) * 2` (`lc`) and `+1` (`wcagRatio`). The
    /// values are byte-identical to calling `recheckContrast(bgHexes[s], fgHexes,
    /// theme)` for each `s`.
    #[wasm_bindgen(js_name = recheckContrastMulti)]
    pub fn recheck_contrast_multi(
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

        // Исторический порядок V1 закреплён ради совместимости, а не как научный oracle.
        let olive_mud = colors.muddiness("#6B6B2E").unwrap();
        assert!(olive_mud > 0.80);
        assert!((olive_mud - 0.8699).abs() < 1e-3);

        // Серый вектор также проверяет именно воспроизводимость V1.
        let gray_mud = colors.muddiness("#808080").unwrap();
        assert!(gray_mud < 0.05);

        // Invalid hex returns an error
        assert!(colors.muddiness("not_a_hex").is_err());
    }
}
