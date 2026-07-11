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

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use crate::dto::ResolvedTheme;
use crate::engine::{Engine, hex_for_recheck};
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
   * `true`, если запрошенная alpha не допускает ни одного byte-тинта,
   * воспроизводящего цель в affine encoded-sRGB8 reference, и потому поднята до
   * первого проходящего `binary64`. Композит остаётся побайтно равен целевому
   * солиду; меняется только явно возвращённая `alpha`. У прямой лестницы всегда
   * `false`.
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
export type GlowDecisionProfileV1 = "stable-v1" | "legacy-platform-dependent-v1";
export type GlowLayerRecipeProfileV1 = "cam16-jprime-oklab-cusp-v1";
export type GlowDiagnosticProfileV1 = "cam16-ucs-jprime-li2017-v1";
export type GlowTargetStatusV1 =
  | "exact-noop-unreachable"
  | "legacy-reached"
  | "legacy-unreachable";
export type NumericalIndeterminacyV1 =
  | {
      readonly reason: "sound-bound-unavailable";
      readonly bounds: { readonly kind: "unavailable" };
    }
  | {
      readonly reason: "interval-overlap";
      readonly bounds: {
        readonly kind: "outward";
        readonly lower: number;
        readonly upper: number;
      };
    };
export interface GlowBitExactDecisionGuaranteeV1 {
  readonly kind: "bit-exact";
}
export interface GlowLegacyDecisionGuaranteeV1 {
  readonly kind: "legacy-platform-dependent-v1";
}
export type GlowDecisionGuaranteeV1 =
  | GlowBitExactDecisionGuaranteeV1
  | GlowLegacyDecisionGuaranteeV1;

/** Поля, общие для трёх конструктивно допустимых определённых исходов Glow. */
export interface GlowDeterminateRoleBase {
  readonly kind: "glow";
  readonly cssVar: string;
  /** Core, предназначенный потребителем для меньшего blur; геометрия не моделируется. */
  readonly coreHex: string;
  /** Halo, предназначенный потребителем для большего blur; в recipe v1 это источник. */
  readonly haloHex: string;
  /** Интенсивность screen-слоя, (0, 1]. */
  readonly alpha: number;
  /** Каноническая CSS-запись той же alpha; vars использует буквально её. */
  readonly alphaCss: string;
  /** Exact reference-домен point-композита; не renderer/display certificate. */
  readonly compositeProfile: "encoded-srgb8-screen-v1";
  readonly compositeGuarantee: "bit-exact";
  /** Версионированный алгоритм, построивший анатомию core/halo. */
  readonly layerRecipeProfile: GlowLayerRecipeProfileV1;
  /** Модель внешнего вида полного результата; обязательна, потому coreAchievedDj вычислен через CAM16. */
  readonly appearanceDiagnosticProfile: GlowDiagnosticProfileV1;
  /** Цель решается только по изолированному halo. */
  readonly constraintLayer: "halo";
  /** Запрошенный |ΔJ'| halo-композита. */
  readonly targetDj: number;
  /** Reference-композит изолированного halo на фоне резолва. */
  readonly haloCompositeHex: string;
  /** Фактический |ΔJ'| изолированного halo-композита. */
  readonly haloAchievedDj: number;
  /** Reference-композит изолированного core с той же alpha. */
  readonly coreCompositeHex: string;
  /** Фактический |ΔJ'| изолированного core-композита. */
  readonly coreAchievedDj: number;
  /** @deprecated Alias `haloAchievedDj` для совместимости. */
  readonly achievedDj: number;
  /** CSS-значение halo: `oklch(L% C H)`. */
  readonly css: string;
}

/** Точный no-op: выбор не вызывает CAM16 и не может достигнуть положительную цель. */
export interface GlowStableExactNoopRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "stable-v1";
  readonly decisionGuarantee: GlowBitExactDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: null;
  readonly targetStatus: "exact-noop-unreachable";
  /** @deprecated Выведено из targetStatus. */
  readonly degraded: true;
}

/** Legacy-выбор CAM16 достиг целевого |ΔJ'| в охарактеризованной среде исполнения. */
export interface GlowLegacyReachedRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "legacy-platform-dependent-v1";
  readonly decisionGuarantee: GlowLegacyDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: GlowDiagnosticProfileV1;
  readonly targetStatus: "legacy-reached";
  /** @deprecated Выведено из targetStatus. */
  readonly degraded: false;
}

/** Legacy-выбор CAM16 вернул максимум, не достигший целевого |ΔJ'|. */
export interface GlowLegacyUnreachableRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "legacy-platform-dependent-v1";
  readonly decisionGuarantee: GlowLegacyDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: GlowDiagnosticProfileV1;
  readonly targetStatus: "legacy-unreachable";
  /** @deprecated Выведено из targetStatus. */
  readonly degraded: true;
}

export type GlowDeterminateRole =
  | GlowStableExactNoopRole
  | GlowLegacyReachedRole
  | GlowLegacyUnreachableRole;

/** Stable terminal outcome: no sound target/max decision, therefore no CSS vars. */
export interface GlowIndeterminateRoleBase {
  readonly kind: "glow-indeterminate";
  readonly cssVar: string;
  readonly sourceHex: string;
  readonly targetDj: number;
  readonly constraintLayer: "halo";
  readonly decisionProfile: "stable-v1";
  readonly numericalSiteId: "glow-target-or-maximum-v1";
}

export type GlowIndeterminateRole = GlowIndeterminateRoleBase & NumericalIndeterminacyV1;

export type GlowRole = GlowDeterminateRole | GlowIndeterminateRole;

/** Численное свидетельство выбора material-alpha. Оно характеризует повторно
 * проверенный результат профиля, а не строгую межплатформенную границу
 * минимальной alpha. */
export interface MaterialAlphaGuaranteeBaseV1 {
  /** Byte-scale affine binary64 compositor + original WCAG 2.1 (2018)
   *  `0.03928` EOTF, with a conservative channel envelope and both crossed seam
   *  sides. Platform-characterized because `powf` is not outward-bounded. */
  readonly numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1";
}
/** Rechecked fail/pass endpoints from a fixed-step binary partition after the
 *  directed-search guard. No global-monotonicity, first-state or minimum claim. */
export interface MaterialBisectionBracketGuaranteeV1 extends MaterialAlphaGuaranteeBaseV1 {
  readonly kind: "bisection-bracket-characterized-v1";
  readonly iterations: number;
  readonly lowerAlpha: number;
  readonly upperAlpha: number;
}
export interface MaterialTransparentEndpointGuaranteeV1 extends MaterialAlphaGuaranteeBaseV1 {
  readonly kind: "transparent-endpoint-characterized-v1";
}
export interface MaterialOpaqueEndpointGuaranteeV1 extends MaterialAlphaGuaranteeBaseV1 {
  readonly kind: "opaque-endpoint-characterized-v1";
}
export type MaterialAlphaGuaranteeV1 =
  | MaterialBisectionBracketGuaranteeV1
  | MaterialTransparentEndpointGuaranteeV1
  | MaterialOpaqueEndpointGuaranteeV1;

/** Двухслойный материал (kind material, #89): тинт `01` (с выведенной α) над
 *  опаковой базой `02`, обе — один тон. `vars` несёт --lab-<role> (солид-канон,
 *  опаковый), --lab-<role>-01 (тинт oklch/α) и --lab-<role>-02 (база, опаковая).
 *  Композит-гарантия читаемости пересчитываема из toneHex/alpha (α-граница). */
export interface MaterialRoleBase {
  readonly kind: "material";
  readonly cssVar: string;
  /** Тон #RRGGBB: тинт 01, база 02 и солид-канон одновременно (равны). */
  readonly toneHex: string;
  /** Худший WCAG-контраст коммит-полюса по коридору [чёрный, белый]. */
  readonly worstContrast: number;
  /** Запрошенный WCAG-пол (4.5 / 3.0); держится только при `alphaStatus: "satisfied"`. */
  readonly floor: number;
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
  /** Ready-to-serve CSS value солид-канона: "oklch(L% C H)". */
  readonly css: string;
}

/** Уже прозрачная граничная точка держит запрошенный пол. */
export interface MaterialSatisfiedTransparentRole extends MaterialRoleBase {
  readonly alpha: 0;
  readonly alphaGuarantee: MaterialTransparentEndpointGuaranteeV1;
  readonly alphaStatus: "satisfied";
  readonly guaranteed: true;
}

/** Повторно проверенная верхняя граница интервала держит запрошенный пол. */
export interface MaterialSatisfiedBracketRole extends MaterialRoleBase {
  readonly alpha: number;
  readonly alphaGuarantee: MaterialBisectionBracketGuaranteeV1;
  readonly alphaStatus: "satisfied";
  readonly guaranteed: true;
}

/** Даже непрозрачная граничная точка не держит запрошенный пол; возвращается alpha 1. */
export interface MaterialDegradedOpaqueRole extends MaterialRoleBase {
  readonly alpha: 1;
  readonly alphaGuarantee: MaterialOpaqueEndpointGuaranteeV1;
  readonly alphaStatus: "degraded";
  readonly guaranteed: false;
}

export type MaterialRole =
  | MaterialSatisfiedTransparentRole
  | MaterialSatisfiedBracketRole
  | MaterialDegradedOpaqueRole;

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

/** Closed physical ladder menu accepted by the config compiler. */
export type LadderPositionV1 =
  | "label-primary"
  | "label-secondary"
  | "label-tertiary"
  | "label-quaternary"
  | "fill-primary"
  | "fill-secondary"
  | "fill-tertiary"
  | "fill-quaternary"
  | "border-base"
  | "border-soft"
  | "border-strong"
  | "focus-ring"
  | "glow"
  | "skeleton-base"
  | "skeleton-highlight"
  | "neutral-fill-primary"
  | "neutral-fill-secondary"
  | "neutral-fill-tertiary"
  | "neutral-fill-quaternary"
  | "neutral-border-base"
  | "neutral-border-soft"
  | "shadow-minor"
  | "shadow-ambient"
  | "shadow-penumbra"
  | "shadow-major";

/** Рецепт роли из физического меню движка. */
export type RoleRecipe =
  | {
      kind: "text-anchor";
      fraction: number;
      floor: "aa-text" | "aa-ui" | "none";
      hue?: LadderSource;
    }
  | { kind: "dj-anchor"; light: number; dark: number }
  | { kind: "decorative-lc"; magnitude: number }
  | {
      kind: "ladder";
      source: LadderSource;
      position: LadderPositionV1;
      floor?: "aa-text" | "aa-ui" | "none";
    }
  | {
      kind: "glow";
      source: LadderSource;
      step: "subtle" | "base" | "bloom";
      decision_profile: GlowDecisionProfileV1;
    }
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
  /** Словарь ролей дизайн-системы. Конфиг обязан нести собственные роли; пустой
   *  контракт (без `roles` и `aliases`) отклоняется на загрузке. */
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
    /// Одноячеечный мемо сериализации последнего резолва: кэш-хит движка
    /// возвращает тот же `Rc`, значит его JSON можно не пересобирать —
    /// hit-путь `resolveTheme` платит только `JSON.parse`. Ячейка держит
    /// сильный `Rc`, поэтому `Rc::ptr_eq` не может ложно совпасть с новой
    /// аллокацией (наша жива — адрес занят); смена конфига даёт новые `Rc`,
    /// и мемо промахивается честно.
    proj_memo: RefCell<Option<(Rc<ResolvedTheme>, Rc<str>)>>,
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
            proj_memo: RefCell::new(None),
        }
    }

    /// Resolve every role for `bgHex` under `theme` (`"light" | "dark" |
    /// "light-ic" | "dark-ic"`).
    ///
    /// Returns a `ResolvedTheme` object. Per-role unreachability is part of a
    /// successful result (each role carries its own `kind`); only whole-call
    /// failures reject (no config loaded yet as `config_required`, invalid hex,
    /// unknown theme, a core invariant failure, and the by-construction-
    /// unreachable oklch serialisation failure as `internal_error`) — as a
    /// structured `"<code>: <message>"` error, never an unwound panic.
    #[wasm_bindgen(js_name = resolveTheme)]
    pub fn resolve_theme(&self, bg_hex: &str, theme: &str) -> Result<JsResolvedTheme, JsError> {
        let theme = crate::theme::parse_theme(theme).map_err(to_js_error)?;
        let resolved = self
            .inner
            .resolve_theme(bg_hex, theme)
            .map_err(to_js_error)?;
        // Одна «широкая» FFI-строка + нативный JSON.parse вместо ~тысячи
        // Reflect::set (почему — в док-комменте модуля `projection`); мемо
        // снимает с кэш-хита ещё и пересборку строки. Каждый вызов по-прежнему
        // отдаёт СВЕЖИЙ объект — семантика для мутирующего потребителя не
        // меняется.
        let json: Rc<str> = {
            let mut memo = self.proj_memo.borrow_mut();
            match memo.as_ref() {
                Some((rc, s)) if Rc::ptr_eq(rc, &resolved) => Rc::clone(s),
                _ => {
                    let s: Rc<str> = crate::projection::resolved_json(&resolved)
                        .map_err(to_js_error)?
                        .into();
                    *memo = Some((Rc::clone(&resolved), Rc::clone(&s)));
                    s
                }
            }
        };
        // По построению строка — валидный JSON; невозможный отказ парсера —
        // честная внутренняя ошибка, не паника.
        let parsed = js_sys::JSON::parse(&json).map_err(|_| {
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

    /// Exact stable-Glow runtime recheck. Returns whether the point screen
    /// layer is a byte-for-byte no-op for every alpha under encoded sRGB8.
    /// `adaptTheme` uses this cheap core-owned predicate to detect the only
    /// stable determinate/indeterminate class transition without running CAM16
    /// or re-solving on every animated-background frame.
    #[wasm_bindgen(js_name = isStableGlowPointNoop)]
    pub fn is_stable_glow_point_noop(&self, tint_hex: &str, bg_hex: &str) -> Result<bool, JsError> {
        let tint = hex_for_recheck(tint_hex).map_err(|error| match error {
            BindingError::InvalidBackground { reason } => {
                to_js_error(BindingError::InvalidColor { reason })
            }
            other => to_js_error(other),
        })?;
        let background = hex_for_recheck(bg_hex).map_err(to_js_error)?;
        labcolors_core::screen_point_is_exact_noop(tint.as_ref(), background.as_ref())
            .map_err(|reason| to_js_error(stable_glow_recheck_core_error(reason)))
    }

    /// Return the frozen legacy `muddiness` coordinate for an sRGB hex colour.
    ///
    /// This is an experimental compatibility proxy: it reproduces the historic
    /// numeric API, but is not an observer-validated human clean/dirty verdict
    /// or a production decision. The legacy identifier is retained only for
    /// compatibility.
    #[wasm_bindgen(js_name = muddiness)]
    pub fn muddiness(&self, hex: &str) -> Result<f64, JsError> {
        labcolors_core::cleanliness::muddiness_from_hex(hex)
            .map_err(|reason| to_js_error(BindingError::InvalidColor { reason }))
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

fn stable_glow_recheck_core_error(reason: String) -> BindingError {
    BindingError::Internal {
        reason: format!(
            "stable Glow point recheck rejected core-generated validated hex values: {reason}"
        ),
    }
}

#[cfg(test)]
mod native_contract_tests {
    use super::*;

    #[test]
    fn generated_config_types_cover_the_closed_ladder_menu_and_dto_fields() {
        let source = include_str!("lib.rs");
        let types = source
            .split_once("const TS_RESULT_TYPES: &'static str = r##\"")
            .and_then(|(_, tail)| tail.split_once("\"##;").map(|(types, _)| types))
            .expect("custom TypeScript section is extractable");
        let block = types
            .split_once("export type LadderPositionV1 =")
            .and_then(|(_, tail)| tail.split_once(';').map(|(block, _)| block))
            .expect("LadderPositionV1 union exists");
        let declared: Vec<&str> = block
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("| \"")
                    .and_then(|value| value.strip_suffix('"'))
            })
            .collect();
        let declared_set: std::collections::HashSet<&str> = declared.iter().copied().collect();
        let core_set: std::collections::HashSet<&str> = labcolors_core::LadderPosition::ALL
            .iter()
            .map(|position| position.key())
            .collect();
        assert_eq!(
            declared.len(),
            declared_set.len(),
            "duplicate TS ladder literal"
        );
        assert_eq!(declared_set, core_set, "TS ladder menu must equal core ALL");
        assert!(types.contains("hue?: LadderSource"));
        assert!(types.contains("floor?: \"aa-text\" | \"aa-ui\" | \"none\""));
        assert!(types.contains("readonly roles: ReadonlyArray"));
    }

    #[test]
    fn stable_glow_noop_boundary_normalises_the_resolve_hex_vocabulary() {
        let colors = LabColors::new();
        assert_eq!(
            colors.is_stable_glow_point_noop("#001", "fff").unwrap(),
            colors
                .is_stable_glow_point_noop("#000011", "#FFFFFF")
                .unwrap()
        );
        assert!(
            colors
                .is_stable_glow_point_noop("#010000", "#FE0000")
                .unwrap()
        );
        assert!(
            !colors
                .is_stable_glow_point_noop("#800000", "#FE0000")
                .unwrap()
        );
    }

    #[test]
    fn stable_glow_core_failure_after_boundary_validation_is_internal() {
        let error = stable_glow_recheck_core_error("fixture drift".into());
        assert!(matches!(error, BindingError::Internal { .. }));
        assert_eq!(error.code(), "internal_error");
    }
}
