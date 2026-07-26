//! `@labpics/colors` — WASM-граница контрастного движка `labcolors-core`.
//!
//! Весь крейт образует один срез Clean Architecture:
//! - `theme` — публичный словарь тем (value object) → viewing conditions ядра;
//! - `error` — сопоставимые ошибки границы (`thiserror`);
//! - `dto` — независимые от фреймворка типы результата (output boundary);
//! - `cache` — кэш контрактов;
//! - `engine` — application core: обобщённый по ролям `resolve_set`;
//! - этот модуль — единственное место с `#[wasm_bindgen]`: адаптер проецирует
//!   чистые результаты движка в JS-объекты.
//!
//! Граница возвращает данные без записи в DOM и CSS-side effects. Применение
//! CSS custom properties выполняет runtime npm-пакета, а не WASM-ядро.

mod cache;
// pub: сериализация канонического конфига через DTO (output boundary);
// wasm_bindgen экспортирует только аннотированное — JS-API от этого не растёт.
pub mod config_dto;
mod dto;
mod engine;
mod error;
mod projection;

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use crate::dto::ResolvedTheme;
use crate::engine::{Engine, hex_for_recheck};
use crate::error::{BindingError, OutputConflicts};

/// TypeScript-формы значений `resolveTheme`. `wasm-bindgen` связывает с ними
/// `LabColors.resolveTheme(...): ResolvedTheme`, поэтому потребителю не нужен
/// вручную написанный `.d.ts`.
#[wasm_bindgen(typescript_custom_section)]
const TS_RESULT_TYPES: &'static str = r##"
import type { Wcag22CriterionV1 } from "../wcag22.js";

/** Ключ темы из словаря `themes` загруженного конфига (клиентское имя). */
export type ThemeName = string;

/** Решённый цвет и фактически достигнутые им контрасты. */
export interface SolvedColor {
  readonly kind: "color";
  /** Имя CSS custom property роли, например "--lab-label-primary". */
  readonly cssVar: string;
  /** Решённый цвет как #RRGGBB; `css` и `vars` содержат oklch. */
  readonly hex: string;
  /** Готовое CSS-значение "oklch(L% C H)"; в `vars` лежит та же строка. */
  readonly css: string;
  /** Знаковая candidate-координата Ys (`lc`) замороженной SAPC-shaped кривой; не доказательство LPC или читаемости. */
  readonly lc: number;
  /** Отношение WCAG 2.1 к фону в диапазоне 1–21. */
  readonly wcagRatio: number;
  /**
   * `true`, если точная цель роли не сохранена: legal floor/иерархия сжали
   * contrast-target либо ограниченный dJ′-поиск вернул лучший из просмотренных
   * кандидатов. Глобальный оптимум не заявляется.
   */
  readonly compressed: boolean;
  /** Честный замер |ΔJ'| на отданном hex для dJ'-ролей; null у контраст-ролей (метрика — lc). */
  readonly achievedDj: number | null;
  /** Пол WCAG переопределил целевую candidate-координату Ys. */
  readonly floorOverride: boolean;
  /**
   * Минимальное отношение WCAG из контракта роли: 4.5 для AA-текста, 3.0 для
   * AA-UI или `null`, если пола нет. Solve проверяет финальную эмитированную
   * пару. Runtime-переход — только способ показа и не сертифицирует этот пол
   * на промежуточных кадрах.
   */
  readonly legalFloor: number | null;
}

/** Явный нулевой токен: цвета здесь намеренно нет; это не отказ. */
export interface NoneRole {
  readonly kind: "none";
  readonly cssVar: string;
}

/** Единственная категория failure внутри успешного snapshot: bounded search не доказал исход. */
export type FailureCategory =
  | "unresolved";

/** Типизированный локальный незавершённый поиск. Unreachable отклоняет whole resolve как OutputConflictError. */
export interface FailureRole {
  readonly kind: "failure";
  readonly cssVar: string;
  /** Семантическая категория, которой владеет Core. */
  readonly category: FailureCategory;
  /** Стабильный машинный код Core: "bounded_search_exhausted". */
  readonly code: string;
  /** Человекочитаемое объяснение. */
  readonly message: string;
}

/** Одна обычная роль, физически недостижимая в объявленном домене. */
export interface OutputConflict {
  /** Непрозрачный client-owned ID роли. */
  readonly role: string;
  /** Стабильный машинный код Core, например "exceeds_range". */
  readonly code: string;
  /** Человекочитаемая исходная диагностика Core. */
  readonly message: string;
}

/** Whole-call ошибка: полный output snapshot не существует. */
export interface OutputConflictError extends Error {
  readonly name: "OutputConflictError";
  readonly code: "output_conflict";
  /** Непустой aggregate в порядке объявления ролей; aliases не дублируются. */
  readonly conflicts: readonly [OutputConflict, ...OutputConflict[]];
}

/** Полупрозрачная эмиссия лестницы/альфа-аналога: CSS несёт oklch(L% C H / A), а браузер композитит её. */
export interface TranslucentRole {
  readonly kind: "translucent";
  readonly cssVar: string;
  /** Тинт как #RRGGBB — цвет, который несёт oklch(… / A). */
  readonly tintHex: string;
  /** Альфа эмиссии, (0, 1]. */
  readonly alpha: number;
  /** Солид, в который тинт композитится на фоне резолва. */
  readonly compositeHex: string;
  /** Знаковая candidate-координата Ys (`lc`) композита; не доказательство LPC или читаемости. */
  readonly compositeLc: number;
  /** Отношение WCAG 2.1 композита. */
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
   * `true`, если solid-граница семейства (`border-<family>-strong`, M2)
   * затемнена вдоль его кривой до пола AA UI 3:1, потому что исходный тинт не
   * проходил на этом фоне. Это явный минимальный сдвиг по объявленной кривой.
   * Истина представления — финальные байты; сохранение воспринимаемых оттенка и
   * хромы не заявляется. Для прямой лестницы и legal family solids — `false`.
   */
  readonly floorCoerced: boolean;
  /** Готовое CSS-значение "oklch(L% C H / A)"; в `vars` лежит та же строка. */
  readonly css: string;
}

/** Свечение (kind glow): screen-слои + решённая интенсивность.
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
  /** Диагностика изолированных point-слоёв (не whole-effect); обязательна, потому coreAchievedDj вычислен через CAM16. */
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
  /** CSS-значение halo: `oklch(L% C H)`. */
  readonly css: string;
}

/** Точный no-op: выбор не вызывает CAM16 и не может достигнуть положительную цель. */
export interface GlowStableExactNoopRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "stable-v1";
  readonly decisionGuarantee: GlowBitExactDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: null;
  readonly targetStatus: "exact-noop-unreachable";
}

/** Legacy-выбор CAM16 достиг целевого |ΔJ'| в охарактеризованной среде исполнения. */
export interface GlowLegacyReachedRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "legacy-platform-dependent-v1";
  readonly decisionGuarantee: GlowLegacyDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: GlowDiagnosticProfileV1;
  readonly targetStatus: "legacy-reached";
}

/** Legacy-выбор CAM16 вернул максимум, не достигший целевого |ΔJ'|. */
export interface GlowLegacyUnreachableRole extends GlowDeterminateRoleBase {
  readonly decisionProfile: "legacy-platform-dependent-v1";
  readonly decisionGuarantee: GlowLegacyDecisionGuaranteeV1;
  readonly selectionDiagnosticProfile: GlowDiagnosticProfileV1;
  readonly targetStatus: "legacy-unreachable";
}

export type GlowDeterminateRole =
  | GlowStableExactNoopRole
  | GlowLegacyReachedRole
  | GlowLegacyUnreachableRole;

/** Терминальный stable-исход: sound target/max-решения нет, поэтому CSS-переменные отсутствуют. */
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
  /** Byte-scale affine binary64-композитор + исходная WCAG 2.1 (2018) EOTF
   *  с порогом `0.03928`, консервативной оболочкой каналов и обеими сторонами
   *  пересечённого seam. Legacy-platform-dependent: `powf` не имеет outward-bound
   *  (attestation — #258). */
  readonly numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1";
}
/** Повторно проверенные fail/pass-границы fixed-step binary partition после
 *  directed-search guard. Глобальная монотонность, первый state и минимум не
 *  заявляются. */
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

/** Двухслойный материал (kind material; whitepaper, «Точечные композиции»): тинт `01` (с выведенной α) над
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
  /** Целевой |ΔJ'| тона не попал в бюджет ограниченного обхода; выбран кандидат
   *  с минимальной ошибкой среди просмотренных, не оптимум всего гамута. */
  readonly toneCompressed: boolean;
  /** Солид-канон отличим от фона резолва на 8-битной сетке. */
  readonly distinct: boolean;
  /** Готовое CSS-значение солид-канона: "oklch(L% C H)". */
  readonly css: string;
}

/** Уже прозрачная граничная точка держит запрошенный пол. */
export interface MaterialSatisfiedTransparentRole extends MaterialRoleBase {
  readonly alpha: 0;
  readonly alphaGuarantee: MaterialTransparentEndpointGuaranteeV1;
  readonly alphaStatus: "satisfied";
}

/** Повторно проверенная верхняя граница интервала держит запрошенный пол. */
export interface MaterialSatisfiedBracketRole extends MaterialRoleBase {
  readonly alpha: number;
  readonly alphaGuarantee: MaterialBisectionBracketGuaranteeV1;
  readonly alphaStatus: "satisfied";
}

/** Даже непрозрачная граничная точка не держит запрошенный пол; возвращается alpha 1. */
export interface MaterialDegradedOpaqueRole extends MaterialRoleBase {
  readonly alpha: 1;
  readonly alphaGuarantee: MaterialOpaqueEndpointGuaranteeV1;
  readonly alphaStatus: "degraded";
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
  | FailureRole;

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
  | { kind: "neutral"; pick: "mid" | "edge" | "inverted" | "light" | "dark" };

/** Закрытое меню физических позиций лестницы, принимаемое компилятором конфига. */
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

/**
 * Рецепт роли из закрытого физического меню текущего resolver-а.
 * Это текущая pre-cutover грамматика, не target IR и не extension point.
 * Вариант считается мигрированным только после одностороннего lowering в общий
 * compiled graph и удаления его прежней исполняемой ветви.
 */
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
      floor?: "aa-text" | "aa-ui";
    }
  | {
      kind: "glow";
      source: LadderSource;
      step: "subtle" | "base" | "bloom";
      decision_profile: GlowDecisionProfileV1;
    }
  | { kind: "alpha-analog"; of: LadderSource; alpha: number }
  | { kind: "material"; source: LadderSource; tone_light: number; tone_dark: number; floor: "aa-text" | "aa-ui" }
  | { kind: "zero" };

/** Полный конфиг дизайн-системы клиента — вход loadConfig (JSON.stringify(config)). */
export interface ThemeConfig {
  readonly brand: ThemeAnchors;
  readonly neutral: {
    readonly anchors: { light: string; mid: string; dark: string };
    readonly tint: {
      target_mp: number;
      hue_stiffness: number;
      hue_override_deg?: number;
    };
    readonly edge?: ThemeAnchors;
    readonly inverted?: ThemeAnchors;
  };
  readonly palette: ReadonlyArray<{ key: string; anchors: ThemeAnchors }>;
  readonly themes: ReadonlyArray<{ name: string; preset: "srgb" | "dim" | "srgb-ic" | "dim-ic" }>;
  /** Словарь ролей дизайн-системы. Конфиг обязан нести собственные роли; пустой
   *  контракт (без `roles` и `aliases`) отклоняется на загрузке. */
  readonly roles: ReadonlyArray<{ name: string; recipe: RoleRecipe }>;
  readonly aliases?: ReadonlyArray<{ alias: string; target: string }>;
}

/** Site V2 с поддержкой доказательств. Пустые массивы явно означают отсутствие допущенного evidence. */
export interface NumericalCapabilitySiteV2 {
  readonly siteId: string;
  readonly stableOutcomes: ReadonlyArray<string>;
  readonly compatibilityReleases: ReadonlyArray<string>;
  readonly evidenceClasses: ReadonlyArray<string>;
  readonly artifactIds: ReadonlyArray<string>;
  readonly boundIds: ReadonlyArray<string>;
  readonly proofIds: ReadonlyArray<string>;
  readonly runtimeAttestations: ReadonlyArray<string>;
}

/** Манифест численных возможностей с доказательствами для conformance pack 4. */
export interface NumericalCapabilityManifestV2 {
  readonly schemaVersion: 2;
  readonly coverage: string;
  readonly sites: ReadonlyArray<NumericalCapabilitySiteV2>;
  readonly checksum: string;
}

export type Wcag22DecisionV1 = "pass" | "fail";
export interface Wcag22Q55BoundsV1 {
  /** Десятичная строка u64: значения Q55 выходят за безопасный целочисленный диапазон JavaScript. */
  readonly lower: string;
  readonly upper: string;
}
export interface Wcag22AssessmentV1 {
  readonly kind: "evaluated";
  readonly profileId: "wcag22-srgb8-contrast-v1";
  readonly criterion: Wcag22CriterionV1;
  readonly foreground: string;
  readonly background: string;
  readonly foregroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly backgroundLuminanceQ55: Wcag22Q55BoundsV1;
  readonly q55Scale: string;
  readonly decision: Wcag22DecisionV1;
  readonly evidence: {
    readonly kind: "canonical-finite-bounded";
    readonly artifactId: "wcag22-srgb8-luminance-q55-v1";
    readonly artifactSha256: string;
    readonly boundId: "wcag22-srgb8-outward-q55-v1";
    readonly proofId: "wcag22-srgb8-full-domain-q55-v1";
    readonly proofSha256: string;
    readonly proofPayloadSha256: string;
    readonly generatorSha256: string;
    readonly verifierSha256: string;
    readonly profileChecksum: string;
    readonly profileSha256: string;
  };
}

/** Полный результат резолва одного фона под одной темой. */
export interface ResolvedTheme {
  readonly theme: ThemeName;
  readonly background: string;
  /**
   * Только роли с выбранным CSS-значением. Значения готовы к применению и имеют
   * одну форму: "oklch(L% C H)" для солидов и "oklch(L% C H / A)" для
   * полупрозрачных лестниц/альфа-аналогов. Solve выполняется в гамуте sRGB:
   * oklch здесь нотация, а не расширение гамута. Значения побайтно согласованы с
   * `SolvedColor.hex` и `TranslucentRole.tintHex`; `compositeHex` зависит от фона
   * и не является эмитируемым токеном.
   * Это контракт resolveTheme: applyTheme/watchTheme записывают значения без
   * изменений. Покадровый easing adaptTheme пишет конкретные интерполированные
   * цвета и этой формой эмиссии не ограничен.
   */
  readonly vars: Readonly<Record<string, string>>;
  /** Все роли по стабильному ключу без префикса --lab-. */
  readonly roles: Readonly<Record<string, RoleResult>>;
}
"##;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ResolvedTheme")]
    pub type JsResolvedTheme;

    #[wasm_bindgen(typescript_type = "NumericalCapabilityManifestV2")]
    pub type JsNumericalCapabilityManifestV2;

    #[wasm_bindgen(typescript_type = "Wcag22AssessmentV1")]
    pub type JsWcag22Assessment;

    #[wasm_bindgen(catch, js_namespace = Object, js_name = assign)]
    fn object_assign(target: &JsValue, source: &JsValue) -> Result<JsValue, JsValue>;
}

/// Единственный публичный манифест численных возможностей: V2 с доказательствами.
///
/// Свободная функция, а не метод движка: манифест — статическое свойство
/// сборки (core registry SSOT), он не зависит ни от загруженного конфига, ни
/// от состояния кэша. До появления клиентов ошибочная промежуточная V1
/// projection удалена, чтобы не закреплять две конкурирующие поверхности.
#[wasm_bindgen(js_name = numericalCapabilityManifest)]
pub fn numerical_capability_manifest() -> Result<JsNumericalCapabilityManifestV2, JsError> {
    // Та же «широкая» схема границы, что у resolveTheme: одна UTF-8 строка +
    // нативный JSON.parse вместо пообъектной сборки Reflect::set.
    let json = crate::projection::capability_manifest_json();
    let parsed = js_sys::JSON::parse(&json).map_err(|_| {
        to_js_error(BindingError::Internal {
            reason: "capability manifest не распарсился как JSON".to_string(),
        })
    })?;
    Ok(parsed.unchecked_into())
}

/// Точная оценка WCAG 2.2 одной финальной пары sRGB8.
#[wasm_bindgen(js_name = evaluateWcag22)]
pub fn evaluate_wcag22(
    foreground_hex: &str,
    background_hex: &str,
    criterion: &str,
) -> Result<JsWcag22Assessment, JsError> {
    use labcolors_core::wcag22::Wcag22CriterionV1 as C;
    let criterion = C::parse(criterion).ok_or_else(|| {
        to_js_error(BindingError::UnknownWcag22Criterion {
            requested: criterion.to_string(),
        })
    })?;
    let assessment =
        labcolors_core::wcag22::evaluate_wcag22_hex(foreground_hex, background_hex, criterion)
            .map_err(|error| {
                use labcolors_core::wcag22::Wcag22EvaluationErrorV1 as E;
                to_js_error(match error {
                    E::InvalidSrgb8 { field, reason } => BindingError::InvalidColor {
                        reason: format!("{field}: {reason}"),
                    },
                    other => BindingError::Internal {
                        reason: other.to_string(),
                    },
                })
            })?;
    let json = crate::projection::wcag22_json(&assessment).map_err(to_js_error)?;
    let parsed = js_sys::JSON::parse(&json).map_err(|_| {
        to_js_error(BindingError::Internal {
            reason: "WCAG22 projection не распарсился как JSON".to_string(),
        })
    })?;
    Ok(parsed.unchecked_into())
}

const INVALID_RGB24: u32 = u32::MAX;

fn unpack_rgb24(value: u32) -> [u8; 3] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]
}

fn pack_rgb24([red, green, blue]: [u8; 3]) -> u32 {
    (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}

/// Package-private scalar bridge for the canonical point compositor.
///
/// `0x00RRGGBB` keeps the hot successful boundary allocation-free. The
/// unreachable RGB24 value `0xFFFFFFFF` is the opacity-rejection sentinel;
/// `effective-bg.js` turns it into a loud internal failure instead of a colour.
/// RGB24 words are package-constructed, not a second public parser boundary.
/// The package root deliberately hides both this seam and raw init exports.
#[wasm_bindgen(js_name = __over)]
pub fn point_source_over_encoded_srgb8_v1(
    source_rgb24: u32,
    opacity: f64,
    backdrop_rgb24: u32,
) -> u32 {
    let source = unpack_rgb24(source_rgb24);
    let backdrop = unpack_rgb24(backdrop_rgb24);
    labcolors_core::alpha::composite_over_srgb8(source, opacity, backdrop)
        .ok()
        .map(pack_rgb24)
        .unwrap_or(INVALID_RGB24)
}

/// Контрастный движок над дизайн-системой клиента. Создайте его через
/// [`LabColors::new`], загрузите конфиг методом
/// [`loadConfig`](LabColors::load_config), затем многократно вызывайте
/// [`resolve_theme`](LabColors::resolve_theme); одинаковые вызовы обслуживаются
/// из кэша контрактов.
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
    /// Создаёт движок без загруженной дизайн-системы.
    ///
    /// Движок агностичен (ADR-0001) и не содержит встроенной таблицы ролей,
    /// поэтому [`resolveTheme`](LabColors::resolve_theme) возвращает
    /// `config_required`, пока [`loadConfig`](LabColors::load_config) не загрузит
    /// дизайн-систему.
    #[wasm_bindgen(constructor)]
    pub fn new() -> LabColors {
        LabColors {
            inner: Engine::new(),
            proj_memo: RefCell::new(None),
        }
    }

    /// Решает каждую роль для `bgHex` в `theme` — КЛИЕНТСКОМ ключе из словаря
    /// `themes` загруженного конфига (канонический путь: ключ → `VcPreset` →
    /// viewing conditions). Ключ вне словаря — `unknown_theme`.
    ///
    /// Возвращает полный `ResolvedTheme`. Локальный `unresolved` остаётся
    /// типизированным исходом роли; ordinary `unreachable` отклоняет весь вызов
    /// как `OutputConflictError`. Rejected/internal также
    /// отклоняются атомарно: частичной темы или CSS не бывает.
    /// Ошибки границы — структурная форма `"<code>: <message>"`, Rust-паника
    /// в JavaScript не разматывается.
    #[wasm_bindgen(js_name = resolveTheme)]
    pub fn resolve_theme(&self, bg_hex: &str, theme: &str) -> Result<JsResolvedTheme, JsError> {
        let resolved = self
            .inner
            .resolve_theme(bg_hex, theme)
            .map_err(to_js_error)?;
        // Одна «широкая» FFI-строка + нативный JSON.parse вместо ~тысячи
        // Reflect::set (почему — в док-комменте модуля `projection`); мемо
        // снимает с кэш-хита ещё и пересборку строки. Каждый вызов отдаёт свежий
        // объект, поэтому внешняя runtime-мутация не меняет кэш или следующий
        // resolve; публичный тип при этом объявляет снимок неизменяемым.
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
    /// конфига — 16 hex-символов. Это детерминированный вероятностный
    /// идентификатор; корректность reload держится на полном очищении кэша, а
    /// не на предположении об отсутствии 64-битных коллизий.
    #[wasm_bindgen(js_name = loadConfig)]
    pub fn load_config(&mut self, json: &str) -> Result<String, JsError> {
        let fp = self.inner.load_config(json).map_err(to_js_error)?;
        // Успешный atomic reload чистит и projection-memo: следующая проекция
        // пересобирается для нового контракта. Неудачный reload возвращается
        // выше ДО этой строки — прежние state/cache/memo не тронуты.
        *self.proj_memo.borrow_mut() = None;
        Ok(format!("{fp:016x}"))
    }

    /// Минтит numeric handle темы `theme` — слот клиентского ключа в словаре
    /// `themes` загруженного конфига. Это холодное string→number понижение: рантайм
    /// разрешает имя темы в handle ОДИН раз на границе solve, затем адресует его
    /// численно в покадровом recheck-цикле, без пере-сканирования словаря строкой.
    /// Recheck без загруженного конфига невозможен; неизвестный ключ — обычный JS
    /// `Error` со стабильным префиксом `"<code>: <message>"`.
    #[wasm_bindgen(js_name = themeHandle)]
    pub fn theme_handle(&self, theme: &str) -> Result<u32, JsError> {
        self.inner.theme_handle(theme).map_err(to_js_error)
    }

    /// Повторно проверяет контрасты packed foreground-слов `fgs` к packed фону
    /// `bg` (оба `0x00RRGGBB`, старший байт зарезервирован и обязан быть нулём) в
    /// теме, адресованной numeric `theme` handle из [`themeHandle`](Self::theme_handle).
    /// Реактивный runtime вызывает этот дешёвый примитив покадрово и запускает
    /// новый solve лишь после устойчивого провала уже решённых цветов. Полного
    /// solve нет: одна оценка замороженной кривой для фона и по одной для foreground.
    ///
    /// Вход — один смежный typed-array copy в линейную память: ноль hex-парсинга,
    /// ноль `String`/`Cow` на foreground; зарезервированный старший байт каждого
    /// слова валидируется один раз без аллокации. Возвращает `Float64Array`
    /// чередующихся пар `[lc, wcagRatio]` в порядке `fgs` — тот же выходной layout,
    /// что у прежней строковой границы, побайтно: `2*i` — знаковая
    /// candidate-координата Ys foreground `i` замороженной SAPC-shaped кривой, а не
    /// вердикт LPC/читаемости; `2*i+1` — отношение WCAG. Слово с ненулевым старшим
    /// байтом или неизвестный handle дают обычный JS `Error` со стабильным
    /// префиксом `"<code>: <message>"`.
    #[wasm_bindgen(js_name = recheckContrast)]
    pub fn recheck_contrast(
        &self,
        bg: u32,
        fgs: Vec<u32>,
        theme: u32,
    ) -> Result<Vec<f64>, JsError> {
        self.inner.recheck_u32(bg, &fgs, theme).map_err(to_js_error)
    }

    /// Точная runtime-перепроверка stable Glow. Возвращает, является ли точечный
    /// screen-слой побайтным no-op для любой альфы в encoded sRGB8. `adaptTheme`
    /// использует этот дешёвый предикат Core для обнаружения единственного
    /// перехода stable determinate/indeterminate без запуска CAM16 и нового solve
    /// на каждом кадре анимированного фона.
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

    /// Одним вызовом проверяет набор packed foreground-слов `fgs` против многих
    /// packed образцов фона `bgs` (все `0x00RRGGBB`, старший байт required-zero) в
    /// теме, адресованной numeric `theme` handle. Контроллер использует это для
    /// меняющегося backdrop: gradient, image, bg-blur или glass. Каждый foreground
    /// декодируется один раз; его display-relative luminance переиспользуется для
    /// всех образцов.
    ///
    /// Возвращает плоский background-major `Float64Array`: для образца `s` и
    /// foreground `i` индекс `(s * fgs.length + i) * 2` содержит `lc`, а
    /// следующий — `wcagRatio`. Значения побайтно совпадают с отдельным вызовом
    /// `recheckContrast(bgs[s], fgs, theme)` для каждого `s`.
    #[wasm_bindgen(js_name = recheckContrastMulti)]
    pub fn recheck_contrast_multi(
        &self,
        bgs: Vec<u32>,
        fgs: Vec<u32>,
        theme: u32,
    ) -> Result<Vec<f64>, JsError> {
        self.inner
            .recheck_multi_u32(&bgs, &fgs, theme)
            .map_err(to_js_error)
    }
}

impl Default for LabColors {
    fn default() -> Self {
        Self::new()
    }
}

/// Преобразует ошибку границы в JS `Error` со стабильным машинным кодом и
/// человекочитаемой причиной, чтобы JS мог ветвиться без отдельного класса
/// ошибки. Формат: `"<code>: <message>"`.
///
/// Это свободная функция, а не `From`: `thiserror` уже даёт `BindingError`
/// blanket-реализацию `From<E: Error> for JsError` через wasm-bindgen, и тот путь
/// потерял бы стабильный код. Здесь код сохраняется в сообщении.
fn to_js_error(err: BindingError) -> JsError {
    let js_error = JsError::new(&format!("{}: {}", err.code(), err));
    let BindingError::OutputConflict { conflicts } = &err else {
        return js_error;
    };
    if decorate_output_conflict_error(&js_error, conflicts).is_ok() {
        return js_error;
    }
    // JSON.parse/Object.assign target a fresh built-in Error; отказ означал бы
    // adapter/platform drift. Не бросаем частично оформленную conflict-ошибку
    // и не паникуем — сужаем её до честного internal_error.
    JsError::new("internal_error: output conflict error projection failed")
}

/// Добавить к обычному JS `Error` типизированный conflict payload одним wide-JSON
/// переходом — тем же способом, которым проецируется успешный результат.
fn decorate_output_conflict_error(error: &JsError, conflicts: &OutputConflicts) -> Result<(), ()> {
    let error_value: JsValue = error.clone().into();
    let payload =
        js_sys::JSON::parse(&crate::projection::output_conflict_json(conflicts)).map_err(|_| ())?;
    object_assign(&error_value, &payload)
        .map(|_| ())
        .map_err(|_| ())
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

    fn custom_types() -> &'static str {
        include_str!("lib.rs")
            .split_once("const TS_RESULT_TYPES: &'static str = r##\"")
            .and_then(|(_, tail)| tail.split_once("\"##;").map(|(types, _)| types))
            .expect("custom TypeScript section is extractable")
    }

    fn shared_wcag22_types() -> &'static str {
        include_str!("../../../packages/colors/wcag22.d.ts")
    }

    fn string_union<'a>(types: &'a str, name: &str) -> Vec<&'a str> {
        let declaration = format!("export type {name} =");
        types
            .split_once(&declaration)
            .and_then(|(_, tail)| tail.split_once(';').map(|(block, _)| block))
            .unwrap_or_else(|| panic!("{name} union exists"))
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("| \"")
                    .and_then(|value| value.strip_suffix('"'))
            })
            .collect()
    }

    #[test]
    fn generated_config_types_cover_the_closed_ladder_menu_and_dto_fields() {
        let types = custom_types();
        let declared = string_union(types, "LadderPositionV1");
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
        assert!(types.contains("floor?: \"aa-text\" | \"aa-ui\";"));
        assert!(types.contains("floor: \"aa-text\" | \"aa-ui\" | \"none\""));
        assert!(!types.contains("ratio: number"));
        assert!(types.contains("readonly roles: ReadonlyArray"));
    }

    #[test]
    fn generated_success_failure_category_excludes_whole_call_conflict() {
        let declared = string_union(custom_types(), "FailureCategory");
        let declared_set: std::collections::HashSet<&str> = declared.iter().copied().collect();
        let expected = [labcolors_core::RoleFailureCategory::Unresolved.as_str()];
        let expected_set: std::collections::HashSet<&str> = expected.into_iter().collect();
        assert_eq!(
            declared.len(),
            declared_set.len(),
            "duplicate TS failure literal"
        );
        assert_eq!(
            declared_set, expected_set,
            "successful TS role failure menu содержит только Unresolved"
        );
        let types = custom_types();
        assert!(types.contains("readonly name: \"OutputConflictError\";"));
        assert!(types.contains("readonly code: \"output_conflict\";"));
        assert!(
            types.contains("readonly conflicts: readonly [OutputConflict, ...OutputConflict[]];")
        );
    }

    #[test]
    fn generated_wcag22_criterion_type_equals_the_core_wire_menu() {
        let declared = string_union(shared_wcag22_types(), "Wcag22CriterionV1");
        let declared_set: std::collections::HashSet<&str> = declared.iter().copied().collect();
        let core_set: std::collections::HashSet<&str> =
            labcolors_core::wcag22::Wcag22CriterionV1::ALL
                .iter()
                .map(|criterion| criterion.key())
                .collect();
        assert_eq!(
            declared.len(),
            declared_set.len(),
            "duplicate TS WCAG22 criterion literal"
        );
        assert_eq!(
            declared_set, core_set,
            "TS WCAG22 criterion menu must equal core ALL"
        );
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

    #[test]
    fn packed_point_boundary_matches_the_independent_rational_oracle() {
        for source in u8::MIN..=u8::MAX {
            for backdrop in u8::MIN..=u8::MAX {
                let source_rgb24 = u32::from(source) << 16;
                let backdrop_rgb24 = u32::from(backdrop) << 16;
                let actual =
                    point_source_over_encoded_srgb8_v1(source_rgb24, 0.122, backdrop_rgb24);
                let numerator = 122_u32 * u32::from(source) + 878_u32 * u32::from(backdrop);
                let expected = ((numerator + 500) / 1_000) << 16;
                assert_eq!(actual, expected, "source={source}, backdrop={backdrop}");
            }
        }
    }

    #[test]
    fn packed_point_boundary_rejects_every_invalid_opacity() {
        for opacity in [f64::NAN, f64::NEG_INFINITY, -0.1, 1.1, f64::INFINITY] {
            assert_eq!(
                point_source_over_encoded_srgb8_v1(0, opacity, 0),
                INVALID_RGB24
            );
        }
        assert_eq!(
            point_source_over_encoded_srgb8_v1(0, -0.0, 0x12_34_56),
            0x12_34_56
        );
    }
}
