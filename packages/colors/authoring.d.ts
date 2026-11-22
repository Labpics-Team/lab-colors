import type { ProgramCleanlinessAssessmentV1 } from "./pkg/labcolors.js";

declare const colorHandle: unique symbol;

/** Итог применённой модели; конвенция не является человеческой оценкой всей композиции. */
export type ColorCleanlinessAssessment = ProgramCleanlinessAssessmentV1;

export interface ColorSystemOptions {
  /** Явная ограниченная конвенция; Core ищет соответствующий ей результат для всех reachable uses. */
  readonly cleanliness?: "nominal-srgb8-v2";
}

/** Исходный encoded sRGB: HEX без alpha или три целых канала [0, 255]. */
export type AuthoredColor = string | readonly [number, number, number];

export type ColorAuthoringErrorCode =
  | "color_invalid_definition"
  | "color_foreign_reference"
  | "color_invalid_context"
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
  /** Верхняя граница прямого расстояния Oklab final visible до authored anchor, в [0, 2]. */
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

export interface ColorRole {
  readonly [colorHandle]: "role";
  readonly id: string;
  readonly anchor: readonly [number, number, number];
  /** Новый local use в порядке предпочтений декларации; повторный handle сохраняет один paint. */
  on(surface: ColorSurface, policy?: ColorUsePolicy): ColorUse;
}

export interface ColorFixedRole extends Omit<ColorRole, "on"> {
  on(surface: ColorSurface, policy?: ColorFixedUsePolicy): ColorFixedUse;
}

export interface ColorUse {
  readonly [colorHandle]: "use";
  readonly role: ColorRole;
  readonly against: ColorSurface;
}

export interface ColorFixedUse extends ColorUse {
  readonly role: ColorFixedRole;
}

export interface ResolvedColor {
  readonly rgb: readonly [number, number, number];
  readonly opacity: number;
  readonly css: string;
}

export type ColorDiagnostic =
  | Readonly<{
      kind: "requirement";
      role: string;
      use: ColorUse;
      outputs: readonly string[];
      requirement: "contrast" | "identity" | "max-change" | "output-domain" | "separation" | "cleanliness";
      /** Проверенный нарушивший кандидат; это не минимальное конфликтное множество. */
      evidence: "candidate-witness";
      caseIndex?: number;
      actual?: readonly [number, number, number];
      distanceSquared?: number;
      maximumDistance?: number;
    }>
  | Readonly<{ kind: "search-limit"; visited: bigint; budget: bigint }>
  | Readonly<{ kind: "unknown-input"; input: string }>
  | Readonly<{ kind: "stale-context"; revision: bigint }>;

export type UnresolvedColorFrame =
  | Readonly<{ state: "conflict"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>
  | Readonly<{ state: "unresolved"; reason: "search-limit"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>
  | Readonly<{ state: "unknown"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>
  | Readonly<{ state: "stale"; revision: bigint; diagnostics: readonly ColorDiagnostic[] }>;

export type ColorResolution<Name extends string = string> =
  | Readonly<{
      state: "ready";
      revision: bigint;
      optimality: "exact" | "unfinished";
      readonly cleanliness: ColorCleanlinessAssessment;
      context: Readonly<Record<string, readonly [number, number, number]>>;
      values: Readonly<Record<Name, ResolvedColor>>;
    }>
  | UnresolvedColorFrame;

export type ColorViewUpdate<Name extends string = string> =
  | Readonly<{ state: "committed"; frame: Extract<ColorResolution<Name>, { state: "ready" }> }>
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame }>;

export type ColorContext = Readonly<Record<string, AuthoredColor | null | undefined>>;

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
  | Readonly<{ state: "rejected"; proposed: UnresolvedColorFrame }>;

export interface ColorView<Name extends string = string> {
  readonly current: Extract<ColorResolution<Name>, { state: "ready" }> | null;
  readonly lastUpdate: ColorViewUpdate<Name> | null;
  /** Предлагает полный новый owned context; отказ сохраняет прежний фон и все значения. */
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
  /** Явные данные контекста; non-ready не предоставляет сохранённые outputs как свежие. */
  resolve(surfaces: ColorContext): ColorResolution<Name>;
  /** Владеет CSS custom properties входов и outputs на одном element. */
  attach(element: HTMLElement, surfaces?: ColorContext, options?: ColorAttachOptions): ColorView<Name>;
  dispose(): void;
}

export interface ColorSystem {
  surface(id: string): ColorInputSurface;
  role(id: string, anchor: AuthoredColor, options?: ColorRoleOptions): ColorRole;
  fixed(id: string, source: AuthoredColor): ColorFixedRole;
  /** Экспортирует reachable graph. Приоритет — порядок создания uses, а не порядок output keys. */
  compile<const Outputs extends Readonly<Record<string, ColorUse>>>(
    outputs: Outputs,
  ): Promise<ColorProgram<Extract<keyof Outputs, string>>>;
}

/** Краткая декларация Program; вычисления, constraints и selection выполняет Core. */
export function colors(options?: ColorSystemOptions): ColorSystem;
