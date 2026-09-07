export const SURROUND_AVERAGE_V1: 1;
export const SURROUND_DIM_V1: 2;
export const SURROUND_DARK_V1: 3;
export type ProgramWireSurroundV1 =
  | typeof SURROUND_AVERAGE_V1
  | typeof SURROUND_DIM_V1
  | typeof SURROUND_DARK_V1;

export const WCAG22_SC143_TEXT_DEFAULT_V1: 1;
export const WCAG22_SC143_TEXT_LARGE_SCALE_V1: 2;
export const WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1: 3;
export const WCAG22_SC1411_GRAPHICAL_OBJECT_V1: 4;
export type ProgramWireWcag22CriterionV1 =
  | typeof WCAG22_SC143_TEXT_DEFAULT_V1
  | typeof WCAG22_SC143_TEXT_LARGE_SCALE_V1
  | typeof WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1
  | typeof WCAG22_SC1411_GRAPHICAL_OBJECT_V1;

export const PROGRAM_WIRE_TOO_MANY_ENTRIES: "PROGRAM_WIRE_TOO_MANY_ENTRIES";
export const PROGRAM_WIRE_INVALID_DECLARATION: "PROGRAM_WIRE_INVALID_DECLARATION";
export type ProgramWireErrorCode =
  | typeof PROGRAM_WIRE_TOO_MANY_ENTRIES
  | typeof PROGRAM_WIRE_INVALID_DECLARATION;

export class ProgramWireError extends Error {
  readonly code: ProgramWireErrorCode;
  constructor(code: ProgramWireErrorCode, message: string);
}

export type ProgramWireRgbV1 = readonly [number, number, number];
export type ProgramWireCandidateV1 = Readonly<{
  id: number;
  rgb: ProgramWireRgbV1;
  opacity: number;
}>;

export class ProgramWireBuilderV1 {
  source(id: number, rgb: ProgramWireRgbV1): this;
  fixedTarget(id: number, source: number): this;
  finiteTarget(id: number, candidates: readonly ProgramWireCandidateV1[]): this;
  family(id: number, releaseBytes: readonly number[]): this;
  surfaceInputPort(id: number): this;
  opacityInput(id: number, value: number): this;
  solidPaint(id: number, target: number): this;
  opacityPaint(id: number, source: number, opacity: number): this;
  inputSurface(id: number, input: number): this;
  occurrenceSurface(id: number, occurrence: number): this;
  sourceOverOccurrence(
    id: number,
    subject: number,
    against: number,
    adaptingLuminance: number,
    backgroundRatio: number,
    surround: ProgramWireSurroundV1,
  ): this;
  presentationRoot(id: number, terminal: number): this;
  presentationTarget(root: number, occurrence: number): this;
  exactVisibleUnary(
    hard: boolean,
    id: number,
    occurrence: number,
    expectedRgb: ProgramWireRgbV1,
  ): this;
  wcag22VisibleUnary(
    hard: boolean,
    id: number,
    occurrence: number,
    criterion: ProgramWireWcag22CriterionV1,
  ): this;
  exactIntrinsicRelationHard(
    id: number,
    reference: number,
    candidates: readonly number[],
  ): this;
  output(slot: number, paint: number): this;
  finish(): Uint8Array;
}
