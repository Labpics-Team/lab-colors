import { ProgramWireBuilderV1, type ProgramWireRgbV1 } from "./abi-v1.js";
export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

/** Включённые границы множества encoded sRGB8; gray содержит только [g, g, g]. */
export type ProgramWireDomainV3 =
  | Readonly<{ kind: "box"; lower: ProgramWireRgbV1; upper: ProgramWireRgbV1 }>
  | Readonly<{ kind: "gray"; lower: number; upper: number }>;

export class ProgramWireBuilderV3 extends ProgramWireBuilderV1 {
  /** Опора может лежать вне domain; конечный неотрицательный допуск имеет конечный квадрат. */
  computedTarget(id: number, domain: ProgramWireDomainV3, anchor: ProgramWireRgbV1, maxDelta: number): this;
  /** Лексикографический приоритет: каждый computed target ровно один раз; граф проверяет Core. */
  assignment(targetsInPreferenceOrder: readonly number[]): this;
  finish(): Uint8Array;
}
