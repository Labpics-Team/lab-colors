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

/** Core-selected rule for a whole connected component, fixed before search. */
export type ColorSelectionComponent = Readonly<{
  mode: ProgramSelectionComponentV1["mode"];
  uses: readonly Readonly<Pick<ColorReferenceSelection, "role" | "use" | "outputs">>[];
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

export type ResolvedColorContext = Readonly<Record<string,
  readonly [number, number, number] | ResolvedColorAffineSrgb8Field>>;

export interface ColorFieldWitness {
  /** Точный рациональный адрес внутри нормированной области: [числитель, знаменатель]. */
  readonly coordinate: readonly [number, number];
  readonly nominal: readonly [number, number, number];
}

export interface ColorFieldAssessment {
  readonly input: string;
  readonly sourceModel: "opaque-affine-encoded-srgb8-half-up-v1";
  readonly domain: "whole-field";
  readonly rendering: "unknown";
  /** Число точных точек и открытых интервалов, выведенных Core. */
  readonly regionCount: number;
  /** Представители в порядке Core caseIndex; не описание покрытия вместо descriptor. */
  readonly witnesses: readonly ColorFieldWitness[];
}

export interface ColorFieldModelScope<Model extends "complete" | "violated" | "incomplete"> {
  readonly scope: "nominal-model";
  readonly field: ColorFieldAssessment;
  readonly coverage: Readonly<{
    readonly sourceModel: ColorFieldAssessment["sourceModel"];
    readonly model: Model;
    readonly rendering: "unknown";
  }>;
}

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
  readonly from: ColorInputSurface;
  /** [Oklab L входа, предпочтительный Oklab L]; абсциссы возрастают от 0 до 1. */
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
  /** Новый local use в порядке предпочтений декларации; повторный handle сохраняет один paint. */
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
      stronger: Readonly<{ role: string; use: ColorUse; outputs: readonly string[] }>;
      weaker: Readonly<{ role: string; use: ColorUse; outputs: readonly string[] }>;
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

export type ColorViewUpdate<Name extends string = string> =
  | Readonly<{ state: "committed"; frame: Extract<ColorResolution<Name>, { state: "ready" }> }>
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame | ModeledColorFrame<Name> }>;

/** Допускается одно affine поле; прочие входы постоянны. Несколько полей пока не поддержаны. */
export type ColorContext = Readonly<Record<string, AuthoredColor | ColorAffineSrgb8Field | null | undefined>>;

export interface ColorAttachOptions {
  /** Доверенный nonce приложения для принадлежащего view stylesheet при CSP. */
  readonly nonce?: string;
}

export type ColorViewReplacement<Name extends string = string> =
  | Readonly<{
      state: "committed";
      frame: Extract<ColorResolution<Name>, { state: "ready" }>;
      view: ColorView<Name>;
    }>
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame | ModeledColorFrame<Name> }>;

export interface ColorView<Name extends string = string> {
  readonly current: Extract<ColorResolution<Name>, { state: "ready" }> | null;
  /** При отклонённом replace имена proposed.values принадлежат новой программе. */
  readonly lastUpdate: ColorViewUpdate<Name> | Extract<ColorViewReplacement<string>, { state: "rejected" }> | null;
  /** Предлагает полный owned context; modeled отклоняется вместе с фоном и сохраняет прежние значения. */
  set(surfaces: ColorContext): ColorViewUpdate<Name>;
  /** Новая authored revision; успешный полный frame заменяет прежние variables в том же sink. */
  replace<NextName extends string>(
    program: ColorProgram<NextName>,
    surfaces: ColorContext,
  ): ColorViewReplacement<NextName>;
  dispose(): void;
}

export interface ColorProgram<Name extends string = string> {
  readonly inputs: readonly string[];
  readonly outputs: readonly Name[];
  /** Явные данные контекста; поле возвращает modeled, недоступный результат не выдаёт старые outputs за свежие. */
  resolve(surfaces: ColorContext): ColorResolution<Name>;
  /** Владеет CSS custom properties; modeled остаётся lastUpdate.proposed, автоматически не применяется. */
  attach(element: HTMLElement, surfaces?: ColorContext, options?: ColorAttachOptions): ColorView<Name>;
  dispose(): void;
}

export interface ColorSystem {
  surface(id: string): ColorInputSurface;
  role(id: string, anchor: AuthoredColor, options?: ColorRoleOptions): ColorAdaptiveRole;
  fixed(id: string, source: AuthoredColor): ColorFixedRole;
  /** Сохраняет исходные отношения силы контраста сильнее → слабее на общей поверхности. Требует reference. */
  preserveTonalHierarchy(members: readonly ColorUse[]): void;
  /** Экспортирует reachable graph. Core совместно выбирает Point-применения без приоритета имён и порядка создания.
   * Компоненты с неявным reference сохраняют последовательный режим совместимости; frame.selection раскрывает режим.
   */
  compile<const Outputs extends Readonly<Record<string, ColorUse>>>(
    outputs: Outputs,
  ): Promise<ColorProgram<Extract<keyof Outputs, string>>>;
}

/** Краткая декларация Program; вычисления, constraints и selection выполняет Core. */
export function colors(options?: ColorSystemOptions): ColorSystem;
