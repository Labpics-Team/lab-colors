/// <reference lib="esnext.disposable" />

import type {
  Wcag22ExplicitSelectionOutcomeV1,
  Wcag22FeasibilityOutcomeV1,
} from "./compiler/labcolors_compiler.js";

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

export type {
  Wcag22ExplicitCandidateV1,
  Wcag22ExplicitDecisionV1,
  Wcag22ExplicitEvaluatedV1,
  Wcag22ExplicitEvaluationProofV1,
  Wcag22ExplicitFinalVerificationV1,
  Wcag22ExplicitInvalidSelectionRequestV1,
  Wcag22ExplicitNoSelectionV1,
  Wcag22ExplicitNotEvaluatedV1,
  Wcag22ExplicitPolicyBindingV1,
  Wcag22ExplicitSelectedV1,
  Wcag22ExplicitSelectionErrorV1,
  Wcag22ExplicitSelectionIntegrityViolationV1,
  Wcag22ExplicitSelectionOperationErrorV1,
  Wcag22ExplicitSelectionOutcomeV1,
  Wcag22ExplicitSelectionPolicyV1,
  Wcag22ExplicitSelectionRequestV1,
  Wcag22ExplicitSelectionResultV1,
  Wcag22ExplicitSelectionTransportErrorV1,
} from "./compiler/labcolors_compiler.js";

/** Exact derived atomic-operation ceiling, available after WASM init. */
export declare function wcag22ExplicitSelectionMaxBytes(): number;

/**
 * Evaluate one strict atomic `wcag22-explicit-selection-v1` UTF-8 envelope
 * with the same hostile-input preflight as the feasibility entry.
 */
export declare function evaluateWcag22ExplicitSelection(
  request: Uint8Array,
): Wcag22ExplicitSelectionOutcomeV1;
