import { ProgramWireBuilderV1, type ProgramWireRgbV1 } from "./abi-v1.js";
export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

/** Включённые границы множества encoded sRGB8; gray содержит только [g, g, g]. */
export type ProgramWireDomainV3 =
  | Readonly<{ kind: "box"; lower: ProgramWireRgbV1; upper: ProgramWireRgbV1 }>
  | Readonly<{ kind: "gray"; lower: number; upper: number }>;

export interface ProgramWireColorOptionsV3 {
  readonly domain?: "srgb" | "neutral";
  readonly identity?: Readonly<{
    maxHueDeviation?: number;
    chroma?: readonly [number, number];
    lightness?: readonly [number, number];
  }>;
  readonly maxChange?: number;
  readonly appearance?: number;
  /** Anchor и identity относятся к source; visible-опору и maxChange Core выводит из reference occurrence. */
  readonly referenceAppearance?: number;
  readonly tone?: Readonly<{from: number; curve: readonly (readonly [number, number])[]}>;
}

export interface ProgramWireAssignmentOptionsV3 {
  /** Единственное неизменяемое авторское окружение: [SurfaceInputPortId, RGB]. */
  readonly reference?: readonly (readonly [number, ProgramWireRgbV1])[];
}

export interface ProgramWireComposeOptionsV3 {
  /** Неизменяемое авторское окружение: [SurfaceInputPortId, RGB]; [] задаёт окружение без входов. */
  readonly reference?: readonly (readonly [number, ProgramWireRgbV1])[];
}

export class ProgramWireBuilderV3 extends ProgramWireBuilderV1 {
  /** Характер и предпочтения видимого применения; источники вычисляет Core. */
  colorTarget(id: number, anchor: ProgramWireRgbV1, options?: ProgramWireColorOptionsV3): this;
  /** Точечное source-over применение со стандартным пакетным контекстом. */
  pointOccurrence(id: number, paint: number, surface: number): this;
  /** Включённые границы 0..2 расстояния Oklab: final visible против actual backdrop. Порог задаёт клиент. */
  visibleSeparationHard(id: number, occurrence: number, minimum: number, maximum: number): this;
  /** Включённый диапазон 0..2 расстояния Oklab между двумя разными конечными видимыми применениями. */
  visiblePairSeparationHard(id: number, left: number, right: number, minimum: number, maximum: number): this;
  /** Сильнее → слабее на одной поверхности; сохраняет reference CRstrong/CRweak, не модель salience. */
  preserveTonalHierarchy(id: number, occurrences: readonly number[]): this;
  /** Опора может лежать вне domain; конечный неотрицательный допуск имеет конечный квадрат. */
  computedTarget(id: number, domain: ProgramWireDomainV3, anchor: ProgramWireRgbV1, maxDelta: number): this;
  /** Лексикографический приоритет: каждый computed target ровно один раз; граф проверяет Core. */
  assignment(targetsInPreferenceOrder: readonly number[], options?: ProgramWireAssignmentOptionsV3): this;
  /** Каждый computed target ровно один раз. Core выбирает целые физические компоненты;
   * порядок деклараций сохраняется только для совместимости implicit-reference компонентов. */
  compose(targets: readonly number[], options?: ProgramWireComposeOptionsV3): this;
  finish(): Uint8Array;
}
