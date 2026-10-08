import { ProgramWireBuilderV1 } from "./abi-v1.js";
export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

export type ProgramSelectionChoiceV2 = Readonly<{ target: number; candidate: number }>;
export type ProgramSelectionStateV2 = Readonly<{
  id: number;
  choices: readonly ProgramSelectionChoiceV2[];
}>;

export class ProgramWireBuilderV2 extends ProgramWireBuilderV1 {
  /** Rank groups in preference order; Core validates and canonically orders ties. */
  selectionRelease(revision: bigint, rankGroups: readonly (readonly ProgramSelectionStateV2[])[]): this;
  finish(): Uint8Array;
}
