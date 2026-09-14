/// <reference lib="esnext.disposable" />

// Terminal C7c public surface: canonical Program wire is the sole authoring
// root. Detached snapshots remain evidence-only; FV-01 authority is reachable
// only through the attached runtime transaction.

import type { Wcag22AssessmentV1 } from "./pkg/labcolors.js";
import type { Wcag22CriterionV1 } from "./wcag22.js";

export {
  AttachedMaterializationAuthority,
  AttachedProgramRuntime,
  AttachedProgramUpdate,
  compileAttachedProgramWire,
  CompiledAttachedProgram,
  compileProgramWire,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
} from "./pkg/labcolors.js";

export type {
  AttachedPointSinkHostIntentV1,
  AttachedPointSinkHostPatchEntryV1,
  AttachedPointSinkHostV1,
  AttachedProgramBindingsV1,
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

export type AttachedProgramCompileErrorCode =
  | "attached_program_wire"
  | "attached_program_compile"
  | "attached_root_consumed_downstream"
  | "attached_family_artifacts_required";
export type AttachedProgramAttachErrorCode =
  | "attached_resource_exhausted"
  | "attached_instantiate"
  | "attached_invalid_bindings"
  | "attached_scope_changed"
  | "attached_epoch_exhausted"
  | "attached_attach";
export type AttachedAuthorityErrorCode =
  | "attached_non_terminal_root"
  | "attached_root_consumed_downstream"
  | "attached_published_revision_mismatch"
  | "attached_program_identity_mismatch"
  | "attached_foreign_owner_generation"
  | "attached_foreign_binding_epoch"
  | "attached_missing_exact_point_absence_proof"
  | "attached_empty_final_owned_domain"
  | "attached_resource_exhausted"
  | "attached_authority";
export type AttachedProgramUpdateErrorCode =
  | "attached_resource_exhausted"
  | "attached_update"
  | "attached_patch_scope_mismatch"
  | "attached_stamp_mismatch"
  | "attached_revision_mismatch"
  | "attached_already_installed"
  | "attached_host"
  | "attached_sink"
  | "attached_internal_invariant"
  | AttachedAuthorityErrorCode;
export type AttachedProgramUpdateOperation = "updateAttachedObserved" | "updateAttachedUnknown";

export type ProgramError = Error & (
  | Readonly<{ code: ProgramCompileErrorCode; operation: "compileProgramWire" }>
  | Readonly<{ code: "program_update"; operation: ProgramUpdateOperation }>
  | Readonly<{ code: AttachedProgramCompileErrorCode; operation: "compileAttachedProgramWire" }>
  | Readonly<{ code: AttachedProgramAttachErrorCode; operation: "attachAttachedProgram" }>
  | Readonly<{ code: AttachedProgramUpdateErrorCode; operation: AttachedProgramUpdateOperation; cause?: unknown }>
  | Readonly<{ code: AttachedAuthorityErrorCode; operation: "validateAttachedAuthority" }>
  | Readonly<{ code: "attached_authority_unavailable"; operation: "takeAttachedAuthority" }>
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
