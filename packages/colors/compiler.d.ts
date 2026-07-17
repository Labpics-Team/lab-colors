/// <reference lib="esnext.disposable" />

import type { Wcag22FeasibilityOutcomeV1 } from "./compiler/labcolors_compiler.js";

export {
  default,
  default as init,
  initSync,
} from "./compiler/labcolors_compiler.js";

/** Exact derived V1 request ceiling, available after compiler WASM initialization. */
export declare function wcag22FeasibilityMaxBytes(): number;

/** Evaluate one strict V1 UTF-8 JSON byte envelope; protocol failures are data. */
export declare function evaluateWcag22Feasibility(
  request: Uint8Array,
): Wcag22FeasibilityOutcomeV1;

export type {
  Wcag22FeasibilityOutcomeV1,
  Wcag22FeasibilityRequestV1,
} from "./compiler/labcolors_compiler.js";
