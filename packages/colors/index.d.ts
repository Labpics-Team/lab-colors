/// <reference lib="esnext.disposable" />

// Terminal C7c public surface: canonical Program wire is the sole authoring
// and runtime root. Recipe DTOs and legacy theme helpers are not re-exported.

import type { Wcag22AssessmentV1 } from "./pkg/labcolors.js";
import type { Wcag22CriterionV1 } from "./wcag22.js";

export {
  compileProgramWire,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
} from "./pkg/labcolors.js";

export type {
  NumericalCapabilitySiteV2,
  NumericalCapabilityManifestV2,
  Wcag22DecisionV1,
  Wcag22Q55BoundsV1,
  Wcag22AssessmentV1,
} from "./pkg/labcolors.js";
export type { Wcag22CriterionV1 } from "./wcag22.js";

export type ProgramCompileErrorCode =
  | "program_wire"
  | "program_compile"
  | "program_family_artifacts_required"
  | "program_instantiate";
export type ProgramUpdateOperation = "updateObserved" | "updateUnknown";
export type ProgramError = Error & (
  | Readonly<{ code: ProgramCompileErrorCode; operation: "compileProgramWire" }>
  | Readonly<{ code: "program_update"; operation: ProgramUpdateOperation }>
);
export type ProgramErrorCode = ProgramError["code"];
export type ProgramOperation = ProgramError["operation"];
export declare function isProgramError(error: unknown): error is ProgramError;

type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;
type SyncInitInput = BufferSource | WebAssembly.Module;

export declare function init(
  input?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>,
): Promise<void>;
export declare function initSync(input: { module: SyncInitInput } | SyncInitInput): void;
export default init;

export declare function evaluateWcag22(
  foreground: string,
  background: string,
  criterion: Wcag22CriterionV1,
): Wcag22AssessmentV1;
