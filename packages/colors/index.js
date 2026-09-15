// Terminal C7c public entry for @labpics/colors.
//
// One runtime root: canonical Program wire -> ProgramRuntime -> ProgramSnapshot.
// Retired recipe engines and browser helper
// roots are intentionally not exported.

import initWasm, {
  initSync as initWasmSync,
  attachProgramWire as attachProgramWireWasm,
  ProgramAttachment,
} from "./pkg/labcolors.js";

let initState = "idle";
let initFlight;

export {
  compileProgramWire,
  evaluateWcag22,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
  ProgramAttachment,
  ProgramAttachedSnapshot,
  ProgramAttachedRender,
  AttachedMaterializationAuthority,
} from "./pkg/labcolors.js";

// wasm-bindgen выставляет `free()` как потребляющий метод JavaScript.
// Синхронный host callback может повторно войти в тот же объект, поэтому
// facade пакета держит небольшой JS-шлюз для каждой публичной операции
// attachment, включая освобождение. Rust-шлюз остаётся авторитетной
// отказоустойчивой границей внутри WASM.
const attachmentStates = new WeakMap();

function attachmentBusyError(operation) {
  const error = new Error("Program attachment operation is already in progress");
  error.code = "program_attachment_busy";
  error.operation = operation;
  return error;
}

function withAttachmentOperation(attachment, operation, callback) {
  const state = attachmentStates.get(attachment);
  if (state === undefined) return callback();
  if (state.busy) throw attachmentBusyError(operation);
  state.busy = true;
  try {
    return callback();
  } finally {
    state.busy = false;
  }
}

for (const [method, operation] of [
  ["updateObserved", "attachmentUpdateObserved"],
  ["updateUnknown", "attachmentUpdateUnknown"],
  ["materializationAuthority", "materializationAuthority"],
  ["materializationAuthorityFor", "materializationAuthority"],
  ["dispose", "attachmentDispose"],
]) {
  const original = ProgramAttachment.prototype[method];
  ProgramAttachment.prototype[method] = function guardedAttachmentOperation(...args) {
    return withAttachmentOperation(this, operation, () => original.apply(this, args));
  };
}

const originalAttachmentFree = ProgramAttachment.prototype.free;
ProgramAttachment.prototype.free = function guardedAttachmentFree(...args) {
  const state = attachmentStates.get(this);
  if (state === undefined) return originalAttachmentFree.apply(this, args);
  if (state.busy) throw attachmentBusyError("attachmentFree");
  state.busy = true;
  try {
    const result = originalAttachmentFree.apply(this, args);
    attachmentStates.delete(this);
    return result;
  } finally {
    state.busy = false;
  }
};

export function attachProgramWire(...args) {
  const attachment = attachProgramWireWasm(...args);
  attachmentStates.set(attachment, { busy: false });
  return attachment;
}

// wasm-bindgen returns every raw export from its loaders. The public facade
// deliberately erases that value: initialization is an effect, not a second
// uncurated ABI beside the typed package surface.
const COMPILE_PROGRAM_ERROR_CODES = new Set([
  "program_wire",
  "program_compile",
  "program_family_artifacts_required",
  "program_instantiate",
]);
const ATTACHMENT_ERROR_CODES = new Set([
  "program_attachment_binding",
  "program_attachment_instantiate",
  "program_attachment_sink_admission",
  "program_attachment_resource_exhausted",
  "program_attachment_non_terminal_target",
]);
const ATTACHMENT_UPDATE_ERROR_CODES = new Set([
  "program_attachment_update",
  "program_attachment_resource_exhausted",
  "program_attachment_internal_invariant",
  "program_attachment_already_disposed",
  "program_attachment_patch_scope_mismatch",
  "program_attachment_stamp_mismatch",
  "program_attachment_revision_mismatch",
  "program_attachment_already_installed",
  "program_attachment_host_rejected",
  "program_attachment_host_protocol",
  "program_attachment_busy",
]);
const ATTACHMENT_DISPOSE_ERROR_CODES = new Set([
  "program_attachment_already_disposed",
  "program_attachment_revoke_unconfirmed",
  "program_attachment_dispose",
  "program_attachment_busy",
]);
const MATERIALIZATION_ERROR_CODES = new Set([
  "program_materialization_not_ready",
  "program_materialization_paint_not_authority",
  "program_materialization_stale_revision",
  "program_materialization_stale_identity",
  "program_materialization_stale_sink_stamp",
  "program_materialization_foreign_binding_epoch",
  "program_materialization_terminal_binding_mismatch",
  "program_materialization_missing_point_absence_proof",
  "program_materialization_ambiguous_observation_cases",
  "program_attachment_busy",
]);
const PHYSICAL_IDENTITY_ERROR_CODES = new Set([
  "program_attachment_unsupported_physical_identity",
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
    if (operation === "attachProgramWire") return ATTACHMENT_ERROR_CODES.has(code);
    if (operation === "attachmentUpdateObserved" || operation === "attachmentUpdateUnknown") {
      return ATTACHMENT_UPDATE_ERROR_CODES.has(code);
    }
    if (operation === "attachmentDispose") return ATTACHMENT_DISPOSE_ERROR_CODES.has(code);
    if (operation === "attachmentFree") return code === "program_attachment_busy";
    if (operation === "materializationAuthority") return MATERIALIZATION_ERROR_CODES.has(code);
    if (operation === "physicalIdentity") return PHYSICAL_IDENTITY_ERROR_CODES.has(code);
    return false;
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
