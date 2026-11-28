import type { ProgramCleanlinessAssessmentV1, ProgramReferenceSelectionV1, ProgramSelectionComponentV1 } from "./pkg/labcolors.js";

declare const colorHandle: unique symbol;

/** Итог применённой модели; конвенция не является человеческой оценкой всей композиции. */
export type ColorCleanlinessAssessment = ProgramCleanlinessAssessmentV1;

/** Объяснение выбора тем же Core, который адаптирует цвет. Привязано к авторскому применению. */
export type ColorReferenceSelection = Readonly<Omit<ProgramReferenceSelectionV1, "targetId" | "occurrenceId"> & {
  role: string;
  use: ColorUse;
  outputs: readonly string[];
}>;

type AuthoredUse = Pick<ColorReferenceSelection, "role" | "use" | "outputs">;

/** Core-selected rule for a whole connected component, fixed before search. */
export type ColorSelectionComponent = Readonly<{
  mode: ProgramSelectionComponentV1["mode"];
  uses: readonly AuthoredUse[];
}>;

export interface ColorSystemOptions {
  /** Явная ограниченная конвенция; Core ищет соответствующий ей результат для всех reachable uses. */
  readonly cleanliness?: "nominal-srgb8-v2";
  /** Исходное окружение. Core выводит желаемый вид из source anchors и материала; resolve его не меняет. */
  readonly reference?: Readonly<Record<string, AuthoredColor>>;
}

/** Исходный encoded sRGB: HEX без alpha или три целых канала [0, 255]. */
export type AuthoredColor = string | readonly [number, number, number];

/** Один непрозрачный encoded-sRGB сегмент на всей нормированной области t ∈ [0, 1]. */
export interface ColorAffineSrgb8Field {
  readonly kind: "affine-srgb8";
  readonly from: AuthoredColor;
  readonly to: AuthoredColor;
}

/** Неизменяемая копия декларации, а не выбранный представитель поля. */
export interface ResolvedColorAffineSrgb8Field {
  readonly kind: "affine-srgb8";
  readonly from: readonly [number, number, number];
  readonly to: readonly [number, number, number];
}

/** Opaque stops with strictly increasing positions, starting at 0 and ending at 1. */
export interface ColorLinearSrgb8Field {
  readonly kind: "linear-srgb8";
  readonly stops: readonly Readonly<{at: number; color: AuthoredColor}>[];
}

export interface ResolvedColorLinearSrgb8Field {
  readonly kind: "linear-srgb8";
  readonly stops: readonly Readonly<{at: number; color: readonly [number, number, number]}>[];
}

export type ResolvedColorContext = Readonly<Record<string,
  readonly [number, number, number] | ResolvedColorAffineSrgb8Field | ResolvedColorLinearSrgb8Field>>;

export interface ColorFieldWitness {
  /** Индекс первого stop сегмента для кусочно-аффинного поля. */
  readonly segmentIndex?: number;
  /** Точный рациональный адрес внутри сегмента: [числитель, знаменатель]. */
  readonly coordinate: readonly [number, number];
  readonly nominal: readonly [number, number, number];
}

export interface ColorFieldAssessment {
  readonly input: string;
  /** First global marginal case occupied by this independent field. */
  readonly caseOffset: number;
  readonly sourceModel: "opaque-affine-encoded-srgb8-half-up-v1" | "opaque-piecewise-affine-encoded-srgb8-half-up-v1";
  readonly domain: "whole-field";
  readonly rendering: "unknown";
  /** Число точных точек и открытых интервалов, выведенных Core. */
  readonly regionCount: number;
  /** Представители в порядке Core caseIndex; не описание покрытия вместо descriptor. */
  readonly witnesses: readonly ColorFieldWitness[];
}

export type ColorFieldModelScope<Model extends "complete" | "violated" | "incomplete"> = Readonly<{
  readonly scope: "nominal-model";
  readonly coverage: Readonly<{
    readonly sourceModel: ColorFieldAssessment["sourceModel"] | "independent-opaque-srgb8-fields-v1";
    readonly model: Model;
    readonly rendering: "unknown";
  }>;
}> & (Readonly<{ readonly field: ColorFieldAssessment; readonly fields?: never }> |
  Readonly<{ readonly fields: readonly ColorFieldAssessment[]; readonly field?: never }>);

export type ColorAuthoringErrorCode =
  | "color_invalid_definition"
  | "color_foreign_reference"
  | "color_invalid_context"
  | "color_unsupported_context"
  | "color_disposed"
  | "color_busy"
  | "color_host_changed"
  | "color_core_contract";

export class ColorAuthoringError extends Error {
  readonly code: ColorAuthoringErrorCode;
  readonly path: string;
  constructor(code: ColorAuthoringErrorCode, path: string, message: string);
}

/** Геометрическая политика идентичности, а не категория человеческого восприятия. */
export interface ColorIdentity {
  readonly maxHueDeviation?: number;
  readonly chroma?: readonly [number, number];
  readonly lightness?: readonly [number, number];
}

export interface ColorRoleOptions {
  readonly domain?: "srgb" | "neutral";
  readonly identity?: ColorIdentity;
  /** Максимальное расстояние Oklab final visible до исходного visible при reference, иначе до anchor, в [0, 2]. */
  readonly maxChange?: number;
}

export interface ColorInputSurface {
  readonly [colorHandle]: "input";
  readonly id: string;
}

export type ColorSurface = ColorInputSurface | ColorUse;

export interface ColorTonePolicy {
  /** Внешний вход кривой. По умолчанию кривая читает видимую подложку этого применения. */
  readonly from?: ColorInputSurface;
  /** Пары [светлота входа, предпочтительная светлота] в Oklab L с возрастающими абсциссами от 0 до 1. */
  readonly curve: readonly (readonly [number, number])[];
}

export interface ColorUsePolicy {
  readonly contrast?: "wcag22-text" | "wcag22-large-text" | "wcag22-ui" | "wcag22-graphic";
  /** Opacity самого paint, а не CSS opacity всей группы с дочерними элементами. */
  readonly opacity?: number;
  readonly tone?: ColorTonePolicy;
  /** Требуемый диапазон прямого ΔE Oklab между final visible и backdrop; не JND. */
  readonly separation?: readonly [number, number];
}

export interface ColorFixedUsePolicy extends Omit<ColorUsePolicy, "tone"> {
  readonly tone?: never;
}

export type ColorRole = ColorAdaptiveRole | ColorFixedRole;

export interface ColorAdaptiveRole {
  readonly [colorHandle]: "adaptive-role";
  readonly id: string;
  readonly anchor: readonly [number, number, number];
  /** Новое локальное применение. Повторный handle сохраняет один общий paint. */
  on(surface: ColorSurface, policy?: ColorUsePolicy): ColorUse<ColorAdaptiveRole>;
}

export interface ColorFixedRole extends Omit<ColorAdaptiveRole, typeof colorHandle | "on"> {
  readonly [colorHandle]: "fixed-role";
  on(surface: ColorSurface, policy?: ColorFixedUsePolicy): ColorFixedUse;
}

export interface ColorUse<Role extends ColorRole = ColorRole> {
  readonly [colorHandle]: "use";
  readonly role: Role;
  readonly against: ColorSurface;
}

export type ColorFixedUse = ColorUse<ColorFixedRole>;

export interface ResolvedColor {
  readonly rgb: readonly [number, number, number];
  readonly opacity: number;
  readonly css: string;
}

export type ColorDiagnostic =
  | Readonly<{
      kind: "relation";
      requirement: "tonal-hierarchy";
      evidence: "candidate-witness";
      stronger: AuthoredUse;
      weaker: AuthoredUse;
      caseIndex: number;
      readonly fieldWitness?: Readonly<ColorFieldWitness & { input: string }>;
      /** CRstrong / CRweak на общем фоне; сравнение выполняется по этим значениям. */
      requiredRatio: number;
      actualRatio: number;
      /** Натуральный логарифм соответствующего отношения, только для диагностики. */
      requiredGap: number;
      actualGap: number;
      strongerPolarity: "darker" | "equal" | "lighter";
      weakerPolarity: "darker" | "equal" | "lighter";
    }>
  | Readonly<{
      kind: "relation";
      requirement: "visible-pair-separation";
      evidence: "candidate-witness";
      left: AuthoredUse;
      right: AuthoredUse;
      range: readonly [number, number];
      caseIndex: number;
      readonly fieldWitness?: Readonly<ColorFieldWitness & { input: string }>;
    }>
  | Readonly<{
      kind: "requirement";
      role: string;
      use: ColorUse;
      outputs: readonly string[];
      requirement: "contrast" | "identity" | "max-change" | "output-domain" | "separation" | "cleanliness";
      /** Проверенный нарушивший кандидат; это не минимальное конфликтное множество. */
      evidence: "candidate-witness";
      caseIndex?: number;
      readonly fieldWitness?: Readonly<ColorFieldWitness & { input: string }>;
      actual?: readonly [number, number, number];
      distanceSquared?: number;
      maximumDistance?: number;
    }>
  | Readonly<{ kind: "search-limit"; visited: bigint; budget: bigint }>
  | Readonly<{ kind: "unknown-input"; input: string }>
  | Readonly<{ kind: "stale-context"; revision: bigint }>;

export type UnresolvedColorFrame =
  | Readonly<{ state: "conflict"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>
  | Readonly<{ state: "conflict"; revision: bigint; diagnostics: readonly ColorDiagnostic[];
      context: ResolvedColorContext } & ColorFieldModelScope<"violated">>
  | Readonly<{ state: "unresolved"; reason: "search-limit"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>
  | Readonly<{ state: "unresolved"; reason: "search-limit"; revision: bigint; diagnostics: readonly ColorDiagnostic[];
      context: ResolvedColorContext } & ColorFieldModelScope<"incomplete">>
  | Readonly<{ state: "unknown"; revision: bigint; diagnostics: readonly ColorDiagnostic[]; context?: ResolvedColorContext }>
  | Readonly<{ state: "stale"; revision: bigint; diagnostics: readonly ColorDiagnostic[]; context?: ResolvedColorContext }>;

/** Пригодный результат на всей области модели; точность браузерного отображения неизвестна. */
export type ModeledColorFrame<Name extends string = string> = Readonly<{
  state: "modeled";
  revision: bigint;
  optimality: "exact" | "unfinished";
  cleanliness: ColorCleanlinessAssessment;
  reference: readonly ColorReferenceSelection[];
  selection: readonly ColorSelectionComponent[];
  context: ResolvedColorContext;
  /** Целые входы для явного preview: affine поле сериализовано с `in srgb` от 0% до 100%. */
  contextCss: Readonly<Record<string, string>>;
  values: Readonly<Record<Name, ResolvedColor>>;
} & ColorFieldModelScope<"complete">>;

export type ColorResolution<Name extends string = string> =
  | Readonly<{
      state: "ready";
      revision: bigint;
      optimality: "exact" | "unfinished";
      readonly cleanliness: ColorCleanlinessAssessment;
      /** Исходный вид и результат адаптации; пусто без reference и для fixed uses. */
      readonly reference: readonly ColorReferenceSelection[];
      readonly selection: readonly ColorSelectionComponent[];
      context: Readonly<Record<string, readonly [number, number, number]>>;
      values: Readonly<Record<Name, ResolvedColor>>;
    }>
  | ModeledColorFrame<Name>
  | UnresolvedColorFrame;

type DisplayableColorState = "ready" | "modeled";
type InstalledColorFrame<Name extends string, State extends DisplayableColorState> =
  Extract<ColorResolution<Name>, { state: State }>;

export type ColorViewUpdate<Name extends string = string, State extends DisplayableColorState = "ready"> =
  | Readonly<{ state: "committed"; frame: InstalledColorFrame<Name, State> }>
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame | ModeledColorFrame<Name> }>;

/** Несколько полей допустимы, когда каждая выбираемая компонента и каждое обязательное условие зависят максимум от одного поля. */
export type ColorContext = Readonly<Record<string, AuthoredColor | ColorAffineSrgb8Field | ColorLinearSrgb8Field | null | undefined>>;

export interface ColorAttachOptions {
  /** Доверенный nonce приложения для принадлежащего view stylesheet при CSP. */
  readonly nonce?: string;
}

export type ColorViewReplacement<Name extends string = string, State extends DisplayableColorState = "ready"> =
  | Readonly<{
      state: "committed";
      frame: InstalledColorFrame<Name, State>;
      view: ColorView<Name, State>;
    }>
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame | ModeledColorFrame<Name> }>;

export interface ColorView<Name extends string = string, State extends DisplayableColorState = "ready"> {
  readonly current: InstalledColorFrame<Name, State> | null;
  /** При отклонённом replace имена proposed.values принадлежат новой программе. */
  readonly lastUpdate: ColorViewUpdate<Name, State> | Extract<ColorViewReplacement<string>, { state: "rejected" }> | null;
  /** Предлагает полный контекст. Способ подключения определяет, может ли view отобразить modeled. */
  set(surfaces: ColorContext): ColorViewUpdate<Name, State>;
  /** Новая authored revision; успешный полный frame заменяет прежние variables в том же sink. */
  replace<NextName extends string>(
    program: ColorProgram<NextName>,
    surfaces: ColorContext,
  ): ColorViewReplacement<NextName, State>;
  dispose(): void;
}

/** Result for one complete, opaque sRGB8 reference bitmap in the same renderer context. */
export type ColorRasterResult<Name extends string = string> =
  | Readonly<{ state: "ready"; scope: "renderer-bitmap"; image: ImageData;
      fieldRelation: "not-dirtier" | "incomparable" | "unavailable";
      frame: Extract<ColorResolution<Name>, { state: "ready" | "modeled" }> } & (
        | { relation: "preserved" | "restored"; basis: "exact-bitmap-identity" }
        | { relation: "unavailable"; reason: "incomparable-bitmap" | "no-reference-paints"
            | "different-bitmap" | "search-limit" | "proposal-limit" | "no-ready-proposal" }
      )>
  | Readonly<{ state: "unavailable"; fieldRelation: "unavailable";
      proposed: Exclude<ColorResolution<Name>, { state: "ready" | "modeled" }> }>;

export interface ColorRasterReference<Name extends string = string> {
  /** Declare the same viewing conditions as capture to assess the field returned in this update. */
  resolve(surfaces: ColorContext, options?: { readonly sameViewingContext?: boolean }): ColorRasterResult<Name>;
  dispose(): void;
}

export interface ColorProgram<Name extends string = string> {
  /** Capture the authored reference using the application's complete opaque bitmap renderer.
   * Reuse only in the same viewing context. Each returned frame has passed its own V3 graph.
   */
  captureRaster(draw: (frame: Extract<ColorResolution<Name>, { state: "ready" | "modeled" }>) => ImageData): ColorRasterReference<Name>;
  readonly inputs: readonly string[];
  readonly outputs: readonly Name[];
  /** Явные данные контекста; поле возвращает modeled, недоступный результат не выдаёт старые outputs за свежие. */
  resolve(surfaces: ColorContext): ColorResolution<Name>;
  /** Владеет CSS custom properties; modeled остаётся lastUpdate.proposed, автоматически не применяется. */
  attach(element: HTMLElement, surfaces?: ColorContext, options?: ColorAttachOptions): ColorView<Name>;
  /** Показывает также пригодное в модели поле; точность его растеризации остаётся неизвестной. */
  preview(element: HTMLElement, surfaces?: ColorContext, options?: ColorAttachOptions): ColorView<Name, DisplayableColorState>;
  dispose(): void;
}

/** A Program V3 declaration and the names needed to use its ports and output slots. */
export interface EncodedColorSystem<Name extends string = string> {
  /** Owned by this invocation. Mutating these bytes cannot change other programs. */
  readonly bytes: Uint8Array;
  readonly inputs: readonly Readonly<{ name: string; port: number }>[];
  readonly outputs: readonly Readonly<{ name: Name; slot: number }>[];
}

export interface ColorSystem {
  surface(id: string): ColorInputSurface;
  role(id: string, anchor: AuthoredColor, options?: ColorRoleOptions): ColorAdaptiveRole;
  fixed(id: string, source: AuthoredColor): ColorFixedRole;
  /** Обе границы расстояния Oklab между конечными цветами включены. Каждое применение имеет свою подложку. */
  separate(left: ColorUse, right: ColorUse, range: readonly [number, number]): void;
  /** Сохраняет исходные отношения силы контраста сильнее → слабее на общей поверхности. Требует reference. */
  preserveTonalHierarchy(members: readonly ColorUse[]): void;
  /** Encode once at build time for distribution to compatible Core runtimes. */
  encode<const Outputs extends Readonly<Record<string, ColorUse>>>(
    outputs: Outputs,
  ): EncodedColorSystem<Extract<keyof Outputs, string>>;
  /** Экспортирует reachable graph. Core совместно выбирает Point-применения без приоритета имён и порядка создания.
   * Связанные reference-применения, включая вложенные адаптивные подложки, подбираются совместно.
   * Режим каждой группы доступен в frame.selection.
   */
  compile<const Outputs extends Readonly<Record<string, ColorUse>>>(
    outputs: Outputs,
  ): Promise<ColorProgram<Extract<keyof Outputs, string>>>;
}

/** Краткая декларация Program; вычисления, constraints и selection выполняет Core. */
export function colors(options?: ColorSystemOptions): ColorSystem;
