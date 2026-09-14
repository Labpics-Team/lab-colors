// Terminal C7c public entry for @labpics/colors.
//
// Canonical Program wire remains the sole authoring root. Runtime has two
// explicit projections: detached evidence snapshots and FV-01 attached
// materialization authority. Retired recipe engines and browser helper roots
// are intentionally not exported.

import initWasm, { initSync as initWasmSync } from "./pkg/labcolors.js";

let initState = "idle";
let initFlight;

export {
  AttachedMaterializationAuthority,
  AttachedProgramRuntime,
  AttachedProgramUpdate,
  compileAttachedProgramWire,
  CompiledAttachedProgram,
  compileProgramWire,
  evaluateWcag22,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
} from "./pkg/labcolors.js";

// wasm-bindgen returns every raw export from its loaders. The public facade
// deliberately erases that value: initialization is an effect, not a second
// uncurated ABI beside the typed package surface.
const COMPILE_PROGRAM_ERROR_CODES = new Set([
  "program_wire",
  "program_compile",
  "program_family_artifacts_required",
  "program_instantiate",
]);

const ATTACHED_COMPILE_ERROR_CODES = new Set([
  "attached_program_wire",
  "attached_program_compile",
  "attached_root_consumed_downstream",
  "attached_family_artifacts_required",
]);
const ATTACHED_ATTACH_ERROR_CODES = new Set([
  "attached_resource_exhausted",
  "attached_instantiate",
  "attached_invalid_bindings",
  "attached_scope_changed",
  "attached_epoch_exhausted",
  "attached_attach",
]);
const ATTACHED_AUTHORITY_ERROR_CODES = new Set([
  "attached_non_terminal_root",
  "attached_root_consumed_downstream",
  "attached_published_revision_mismatch",
  "attached_program_identity_mismatch",
  "attached_foreign_owner_generation",
  "attached_foreign_binding_epoch",
  "attached_missing_exact_point_absence_proof",
  "attached_empty_final_owned_domain",
  "attached_resource_exhausted",
  "attached_authority",
]);
const ATTACHED_UPDATE_ERROR_CODES = new Set([
  "attached_resource_exhausted",
  "attached_update",
  "attached_patch_scope_mismatch",
  "attached_stamp_mismatch",
  "attached_revision_mismatch",
  "attached_already_installed",
  "attached_host",
  "attached_sink",
  "attached_internal_invariant",
  ...ATTACHED_AUTHORITY_ERROR_CODES,
]);

export function isProgramError(error) {
  // A caught value may be a revoked Proxy or expose throwing/stateful getters.
  // Classify one observation; never replace the original failure with a probe error.
  try {
    if (!(error instanceof Error)) return false;
    const operation = error.operation;
    const code = error.code;
    if (operation === "compileProgramWire") return COMPILE_PROGRAM_ERROR_CODES.has(code);
    if (operation === "updateObserved" || operation === "updateUnknown") {
      return code === "program_update";
    }
    if (operation === "compileAttachedProgramWire") return ATTACHED_COMPILE_ERROR_CODES.has(code);
    if (operation === "attachAttachedProgram") return ATTACHED_ATTACH_ERROR_CODES.has(code);
    if (operation === "updateAttachedObserved" || operation === "updateAttachedUnknown") {
      return ATTACHED_UPDATE_ERROR_CODES.has(code);
    }
    if (operation === "validateAttachedAuthority") return ATTACHED_AUTHORITY_ERROR_CODES.has(code);
    return operation === "takeAttachedAuthority" && code === "attached_authority_unavailable";
  } catch {
    return false;
  }
}

export function init(input) {
  if (initState === "ready") return Promise.resolve();
  if (initState === "async") return initFlight;
  if (initState === "starting") {
    throw new Error("Lab Colors: initialization input admission is in progress");
  }
  if (initState === "sync") {
    throw new Error("Lab Colors: synchronous initialization is in progress");
  }

  let resolveFlight;
  let rejectFlight;
  initFlight = new Promise((resolve, reject) => {
    resolveFlight = resolve;
    rejectFlight = reject;
  });
  const flight = initFlight;
  initState = "starting";

  let pending;
  try {
    pending = initWasm(input);
  } catch (error) {
    initState = "idle";
    initFlight = undefined;
    rejectFlight(error);
    return flight;
  }
  initState = "async";
  Promise.resolve(pending).then(
    () => {
      initState = "ready";
      resolveFlight();
    },
    (error) => {
      initState = "idle";
      initFlight = undefined;
      rejectFlight(error);
    },
  );
  return flight;
}

export function initSync(input) {
  if (initState === "ready") return;
  if (initState === "async") throw new Error("Lab Colors: asynchronous initialization is in progress");
  if (initState === "starting") throw new Error("Lab Colors: initialization input admission is in progress");
  if (initState === "sync") throw new Error("Lab Colors: synchronous initialization is in progress");
  initState = "sync";
  try {
    initWasmSync(input);
    initState = "ready";
  } catch (error) {
    initState = "idle";
    throw error;
  }
}

export default init;
