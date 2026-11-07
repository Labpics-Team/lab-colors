// Terminal C7c public entry for @labpics/colors.
//
// One runtime root: canonical Program wire -> ProgramRuntime -> ProgramSnapshot.
// Retired recipe engines and browser helper
// roots are intentionally not exported.

import initWasm, {
  initSync as initWasmSync,
  compileProgramWire as compileProgramWireWasm,
  finalRasterSourceOverWire as finalRasterSourceOverWireWasm,
  attachProgramWire as attachProgramWireWasm,
  decodeCertificateEnvelope as decodeCertificateEnvelopeWasm,
  issueSourceCertificateEnvelope as issueSourceCertificateEnvelopeWasm,
  ProgramAttachment,
  ProgramRuntime,
} from "./pkg/labcolors.js";
import { MAX_SECTION_ENTRIES_V1 } from "./program-wire/encoding.js";

let initState = "idle";
let initFlight;

export {
  evaluateWcag22,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
  ProgramAttachment,
  ProgramAttachedSnapshot,
  ProgramAttachedRender,
  AttachedMaterializationAuthority,
} from "./pkg/labcolors.js";

export const MAX_CERTIFICATE_ENVELOPE_BYTES = 2_097_152;

const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const intrinsicBufferGetter = Object.getOwnPropertyDescriptor(typedArrayPrototype, "buffer").get;
const intrinsicArrayBufferByteLengthGetter = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get;
const intrinsicByteLengthGetter = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "byteLength",
).get;
const intrinsicLengthGetter = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "length",
).get;
const intrinsicSet = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "set",
).value;
const intrinsicAt = Object.getOwnPropertyDescriptor(typedArrayPrototype, "at").value;
const intrinsicTypedArrayTagGetter = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  Symbol.toStringTag,
).get;
const byteStorage = { Type: Uint8Array, tag: "Uint8Array", bytesPerElement: 1 };
const scenarioStorage = { Type: Uint32Array, tag: "Uint32Array", bytesPerElement: 4 };

const CERTIFICATE_ERROR_CODES = new Set([
  "certificate_invalid_magic",
  "certificate_unsupported_schema",
  "certificate_unknown_operation",
  "certificate_unknown_authority_kind",
  "certificate_unsupported_authority_version",
  "certificate_invalid_utf8",
  "certificate_invalid_length",
  "certificate_truncated_input",
  "certificate_trailing_bytes",
  "certificate_non_canonical_revision",
  "certificate_invalid_payload_type",
  "certificate_unsupported_payload_version",
  "certificate_payload_digest_mismatch",
  "certificate_binding_digest_mismatch",
  "certificate_missing_producer_attestation",
  "certificate_producer_binding_mismatch",
  "certificate_runtime_artifact_mismatch",
  "certificate_producer_revision_mismatch",
  "certificate_content_identity_mismatch",
  "certificate_context_mismatch",
  "certificate_resource_limit_exceeded",
  "certificate_admission_capacity_exceeded",
  "certificate_unsupported_opaque",
  "certificate_binding_conflict",
  "certificate_invalid_input",
]);

function certificateIngressError(code) {
  const error = new Error("Certificate envelope input was refused");
  error.code = code;
  error.operation = "decodeCertificateEnvelope";
  return error;
}

// Generated bindings копируют массив до Rust. Проверяем реальный storage,
// не вызывая пользовательские getters, instanceof hooks или prototype traps,
// и передаём copier-у только собственный fixed-length snapshot.
function inspectTypedArrayStorage(value, storage, invalid) {
  let byteLength, length;
  try {
    if (Reflect.apply(intrinsicTypedArrayTagGetter, value, []) !== storage.tag) {
      throw invalid();
    }
    // ValidateTypedArrayBounds rejects detached and out-of-bounds views even
    // when intrinsic byteLength/length both report zero. at(0) is read-only.
    Reflect.apply(intrinsicAt, value, [0]);
    // A concurrent writer cannot participate in an atomic input snapshot.
    // ArrayBuffer's intrinsic getter rejects shared storage, including other realms.
    Reflect.apply(intrinsicArrayBufferByteLengthGetter,
      Reflect.apply(intrinsicBufferGetter, value, []), []);
    byteLength = Reflect.apply(intrinsicByteLengthGetter, value, []);
    length = Reflect.apply(intrinsicLengthGetter, value, []);
    if (
      !Number.isSafeInteger(byteLength) || byteLength < 0 ||
      length * storage.bytesPerElement !== byteLength ||
      Object.getOwnPropertyDescriptor(value, "byteLength") !== undefined ||
      Object.getOwnPropertyDescriptor(value, "length") !== undefined
    ) {
      throw invalid();
    }
  } catch {
    throw invalid();
  }
  return { byteLength, length };
}

function copyTypedArrayStorage(value, storage, invalid, {
  requiredLength,
  maximumBytes,
  tooLarge = invalid,
  allocation = invalid,
} = {}, inspected = inspectTypedArrayStorage(value, storage, invalid)) {
  const { byteLength, length } = inspected;
  if (requiredLength !== undefined && length !== requiredLength) throw invalid();
  if (maximumBytes !== undefined && byteLength > maximumBytes) throw tooLarge();
  try {
    if (
      Reflect.apply(intrinsicByteLengthGetter, value, []) !== byteLength ||
      Reflect.apply(intrinsicLengthGetter, value, []) !== length
    ) {
      throw invalid();
    }
  } catch {
    throw invalid();
  }
  let normalized;
  try {
    normalized = new storage.Type(length);
  } catch {
    throw allocation();
  }
  try {
    // Даже пустой detached/OOB view обязан пройти intrinsic set и получить отказ.
    Reflect.apply(intrinsicSet, normalized, [value, 0]);
    return normalized;
  } catch {
    throw invalid();
  }
}

export function decodeCertificateEnvelope(bytes) {
  return decodeCertificateEnvelopeWasm(copyTypedArrayStorage(
    bytes, byteStorage, () => certificateIngressError("certificate_invalid_input"), {
      maximumBytes: MAX_CERTIFICATE_ENVELOPE_BYTES,
      tooLarge: () => certificateIngressError("certificate_resource_limit_exceeded"),
    },
  ));
}

function programIngressError(operation, code) {
  const error = new Error("Program input was refused");
  error.operation = operation;
  error.code = code;
  return error;
}

function copyProgramInput(value, storage, operation, code, requiredLength, inspected) {
  const resource = operation === "attachmentUpdateObserved" || operation === "materializationAuthority"
    ? "program_attachment_resource_exhausted" : "program_resource_exhausted";
  return copyTypedArrayStorage(value, storage, () => programIngressError(operation, code), {
    requiredLength,
    allocation: () => programIngressError(operation, resource),
  }, inspected);
}

// The two arrays describe one observation matrix. Admit both shapes before
// either snapshot allocation or the generated WASM copier can run.
function copyObservedInputs(scenarioIds, surfaces, surfaceCount, operation) {
  const code = operation === "updateObserved" ? "program_update" : "program_attachment_update";
  const invalid = () => programIngressError(operation, code);
  if (
    typeof surfaceCount !== "number" ||
    !Number.isInteger(surfaceCount) ||
    surfaceCount < 0 ||
    surfaceCount > 0xffff_ffff
  ) {
    throw invalid();
  }
  const scenarios = inspectTypedArrayStorage(scenarioIds, scenarioStorage, invalid);
  const pixels = inspectTypedArrayStorage(surfaces, byteStorage, invalid);
  const expectedBytes = scenarios.length * surfaceCount * 3;
  if (!Number.isSafeInteger(expectedBytes) || expectedBytes !== pixels.byteLength) {
    throw invalid();
  }
  return [
    copyProgramInput(scenarioIds, scenarioStorage, operation, code, undefined, scenarios),
    copyProgramInput(surfaces, byteStorage, operation, code, undefined, pixels),
  ];
}

export function compileProgramWire(bytes, streamId) {
  const runtime = compileProgramWireWasm(copyProgramInput(
    bytes, byteStorage, "compileProgramWire", "program_wire",
  ), streamId);
  runtimeStates.set(runtime, { busy: false });
  return runtime;
}
export function finalRasterSourceOverWire(sourcePort, destinationPort, operator, outputSlot, cleanliness) {
  return finalRasterSourceOverWireWasm(sourcePort, destinationPort, operator, outputSlot, cleanliness);
}
export const MAX_FINAL_RASTER_PIXELS = 65_536;
const runtimeStates = new WeakMap();
function withRuntimeOperation(runtime, operation, callback) {
  const state = runtimeStates.get(runtime);
  if (state?.busy) throw programIngressError(operation, "program_runtime_busy");
  if (state) state.busy = true;
  try { return callback(); } finally { if (state) state.busy = false; }
}


const runtimeUpdateObserved = ProgramRuntime.prototype.updateObserved;
ProgramRuntime.prototype.updateObserved = function updateObserved(revision, scenarioIds, surfaces, surfaceCount) {
  const [ownedIds, ownedSurfaces] = copyObservedInputs(scenarioIds, surfaces, surfaceCount, "updateObserved");
  return withRuntimeOperation(this, "updateObserved", () => runtimeUpdateObserved.call(this, revision, ownedIds, ownedSurfaces, surfaceCount));
};

// Идентичность и тело принадлежат Core; аргументы вызывающего кода не передаются.
export function issueSourceCertificateEnvelope() {
  return issueSourceCertificateEnvelopeWasm();
}

export function isCertificateError(error) {
  try {
    if (!(error instanceof Error)) return false;
    const operation = error.operation;
    const code = error.code;
    if (operation === "decodeCertificateEnvelope") return CERTIFICATE_ERROR_CODES.has(code);
    if (operation === "issueSourceCertificateEnvelope") {
      return code === "certificate_producer_identity_unavailable" ||
        (code !== "certificate_invalid_input" && CERTIFICATE_ERROR_CODES.has(code));
    }
    return false;
  } catch {
    return false;
  }
}

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

function attachmentFreeBeforeDisposeError() {
  const error = new Error("Program attachment must be disposed after external revoke before free");
  error.code = "program_attachment_revoke_unconfirmed";
  error.operation = "attachmentFree";
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
  ["materializationAuthorityForBinding", "materializationAuthority"],
  ["dispose", "attachmentDispose"],
]) {
  const original = ProgramAttachment.prototype[method];
  ProgramAttachment.prototype[method] = function guardedAttachmentOperation(...args) {
    const result = withAttachmentOperation(this, operation, () => {
      if (method === "updateObserved") {
        [args[1], args[2]] = copyObservedInputs(args[1], args[2], args[3], operation);
      } else if (method === "materializationAuthorityFor") {
        args[1] = copyProgramInput(args[1], byteStorage, operation, "program_materialization_stale_identity", 32);
      }
      return original.apply(this, args);
    });
    if (method === "dispose") {
      const state = attachmentStates.get(this);
      if (state !== undefined) state.disposed = true;
    }
    return result;
  };
}

const originalAttachmentFree = ProgramAttachment.prototype.free;
ProgramAttachment.prototype.free = function guardedAttachmentFree(...args) {
  const state = attachmentStates.get(this);
  if (state === undefined) return originalAttachmentFree.apply(this, args);
  if (state.busy) throw attachmentBusyError("attachmentFree");
  if (!state.disposed) throw attachmentFreeBeforeDisposeError();
  state.busy = true;
  try {
    const result = originalAttachmentFree.apply(this, args);
    attachmentStates.delete(this);
    return result;
  } finally {
    state.busy = false;
  }
};

// Generated alias захватывает исходный free до установки шлюза. Оба способа
// освобождения обязаны проверять busy и подтверждённый внешний revoke.
if (Symbol.dispose) {
  ProgramAttachment.prototype[Symbol.dispose] = ProgramAttachment.prototype.free;
}

function copyAttachmentBindings(bindings) {
  const invalid = () => programIngressError("attachProgramWire", "program_attachment_binding");
  const resource = () => programIngressError("attachProgramWire", "program_attachment_resource_exhausted");
  const property = (object, key) => {
    try {
      const descriptor = Object.getOwnPropertyDescriptor(object, key);
      if (!descriptor || !Object.hasOwn(descriptor, "value")) throw invalid();
      return descriptor.value;
    } catch {
      throw invalid();
    }
  };
  const tables = [property(bindings, "emissions"), property(bindings, "presentations")];
  const fields = [["slot", "sinkOutput"], ["slot", "root", "occurrence"]];
  const lengths = tables.map((table) => {
    let isArray;
    try {
      isArray = Array.isArray(table);
    } catch {
      throw invalid();
    }
    if (!isArray) throw invalid();
    const length = property(table, "length");
    if (!Number.isSafeInteger(length) || length < 0) throw invalid();
    // Это существующий budget записи LCPW; новая таблица не меняет grammar или solver.
    if (length > MAX_SECTION_ENTRIES_V1) throw resource();
    return length;
  });
  const result = [];
  for (let tableIndex = 0; tableIndex < tables.length; tableIndex++) {
    let flattened;
    try {
      flattened = new scenarioStorage.Type(lengths[tableIndex] * fields[tableIndex].length);
    } catch {
      throw resource();
    }
    for (let row = 0; row < lengths[tableIndex]; row++) {
      const record = property(tables[tableIndex], String(row));
      for (let column = 0; column < fields[tableIndex].length; column++) {
        const value = property(record, fields[tableIndex][column]);
        if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) throw invalid();
        flattened[row * fields[tableIndex].length + column] = value;
      }
    }
    result.push(flattened);
  }
  return result;
}

export function attachProgramWire(...args) {
  const bytes = copyProgramInput(args[0], byteStorage, "attachProgramWire", "program_wire");
  let bindings, host;
  if (args.length === 4) {
    bindings = args[2];
    host = args[3];
  } else if (args.length === 7) {
    bindings = {
      emissions: [{ slot: args[2], sinkOutput: args[3] }],
      presentations: [{ slot: args[2], root: args[4], occurrence: args[5] }],
    };
    host = args[6];
  } else {
    throw programIngressError("attachProgramWire", "program_attachment_binding");
  }
  if (typeof host !== "function") throw programIngressError("attachProgramWire", "program_attachment_binding");
  const [emissions, presentations] = copyAttachmentBindings(bindings);
  const attachment = attachProgramWireWasm(bytes, args[1], emissions, presentations, host);
  attachmentStates.set(attachment, { busy: false, disposed: false });
  return attachment;
}

// wasm-bindgen returns every raw export from its loaders. The public facade
// deliberately erases that value: initialization is an effect, not a second
// uncurated ABI beside the typed package surface.
const PROGRAM_INFRASTRUCTURE_ERROR_CODES = [
  "program_resource_exhausted",
  "program_internal_invariant",
];
const COMPILE_PROGRAM_ERROR_CODES = new Set([
  ...PROGRAM_INFRASTRUCTURE_ERROR_CODES,
  "program_wire",
  "program_compile",
  "program_unsupported",
  "program_family_artifacts_required",
  "program_instantiate",
]);
const UPDATE_PROGRAM_ERROR_CODES = new Set([
  "program_update",
  "program_unsupported",
  ...PROGRAM_INFRASTRUCTURE_ERROR_CODES,
]);
const ATTACHMENT_ERROR_CODES = new Set([
  ...COMPILE_PROGRAM_ERROR_CODES,
  "program_attachment_binding",
  "program_attachment_instantiate",
  "program_attachment_sink_admission",
  "program_attachment_resource_exhausted",
  "program_attachment_internal_invariant",
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
const ATTACHMENT_FREE_ERROR_CODES = new Set([
  "program_attachment_revoke_unconfirmed",
  "program_attachment_busy",
]);
const MATERIALIZATION_ERROR_CODES = new Set([
  "program_attachment_resource_exhausted",
  "program_materialization_not_ready",
  "program_materialization_paint_not_authority",
  "program_materialization_stale_revision",
  "program_materialization_stale_identity",
  "program_materialization_stale_sink_stamp",
  "program_materialization_foreign_binding_epoch",
  "program_materialization_terminal_binding_mismatch",
  "program_materialization_missing_point_absence_proof",
  "program_materialization_ambiguous_observation_cases",
  "program_materialization_ambiguous_bindings",
  "program_attachment_busy",
]);
const PHYSICAL_IDENTITY_ERROR_CODES = new Set([
  "program_physical_identity",
]);

const RUNTIME_BUSY_OPERATIONS = new Set([
  "updateObserved", "updateUnknown", "updateFinalRaster", "consumeFinalRaster",
  "contentIdentity", "finalRasterRefusal", "free",
]);

export function isProgramError(error) {
  // A caught value may be a revoked Proxy or expose throwing/stateful getters.
  // Classify one observation; never replace the original failure with a probe error.
  try {
    if (!(error instanceof Error)) return false;
    const operation = error.operation;
    const code = error.code;
    if (code === "program_runtime_busy") return RUNTIME_BUSY_OPERATIONS.has(operation);
    if (operation === "finalRasterSourceOverWire" || operation === "updateFinalRaster" || operation === "consumeFinalRaster") {
      return FINAL_RASTER_ERROR_CODES.has(code);
    }
    if (operation === "compileProgramWire") return COMPILE_PROGRAM_ERROR_CODES.has(code);
    if (operation === "updateObserved" || operation === "updateUnknown") {
      return UPDATE_PROGRAM_ERROR_CODES.has(code);
    }
    if (operation === "attachProgramWire") return ATTACHMENT_ERROR_CODES.has(code);
    if (operation === "attachmentUpdateObserved" || operation === "attachmentUpdateUnknown") {
      return ATTACHMENT_UPDATE_ERROR_CODES.has(code);
    }
    if (operation === "attachmentDispose") return ATTACHMENT_DISPOSE_ERROR_CODES.has(code);
    if (operation === "attachmentFree") return ATTACHMENT_FREE_ERROR_CODES.has(code);
    if (operation === "materializationAuthority") return MATERIALIZATION_ERROR_CODES.has(code);
    if (operation === "physicalIdentity") return PHYSICAL_IDENTITY_ERROR_CODES.has(code);
    if (operation === "assessmentReport") return PROGRAM_INFRASTRUCTURE_ERROR_CODES.includes(code);
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


/** Читает закрытую пару операции и причины без преобразований и разбора message. */
export function isBindingError(error) {
  try {
    if (!(error instanceof Error)) return false;
    const operation = error.operation;
    const code = error.code;
    switch (operation) {
      case "evaluateWcag22":
        return code === "invalid_color" || code === "unknown_wcag22_criterion" || code === "internal_error";
      case "numericalCapabilityManifest":
        return code === "internal_error";
      case "readProgramOutput":
        return code === "invalid_output_index";
      case "readPresentationComposite":
        return code === "invalid_composite_id" || code === "presentation_composite_unavailable";
      default:
        return false;
    }
  } catch {
    return false;
  }
}

// Полный field payload проверяется до JS snapshot и generated WASM copies.
const FINAL_RASTER_ERROR_CODES = new Set([
  "program_resource_exhausted",
  "program_field_extent", "program_field_dpr", "program_field_length",
  "program_field_premultiplied", "program_field_ports", "program_field_update",
  "program_field_unsupported", "program_field_transparent", "program_field_convention",
  "program_field_tq", "program_field_cc", "program_field_stale", "program_field_program",
  "program_field_output", "program_field_context", "program_field_internal",
  "program_field_consumer_rejected", "program_field_consumer_failed",
]);
const runtimeFinalUpdate = ProgramRuntime.prototype.updateFinalRaster;
ProgramRuntime.prototype.updateFinalRaster = function (
  revision, width, height, dpr, context, sourcePort, source, destinationPort, destination,
) {
  return withRuntimeOperation(this, "updateFinalRaster", () => {
    const invalid = () => programIngressError("updateFinalRaster", "program_field_length");
    if (!Number.isInteger(width) || !Number.isInteger(height) ||
        width <= 0 || height <= 0 || width > 0xffff_ffff || height > 0xffff_ffff)
      throw programIngressError("updateFinalRaster", "program_field_extent");
    const count = width * height;
    if (!Number.isSafeInteger(count) || count > MAX_FINAL_RASTER_PIXELS)
      throw programIngressError("updateFinalRaster", "program_resource_exhausted");
    if (!Number.isInteger(dpr) || dpr < 1 || dpr > 4)
      throw programIngressError("updateFinalRaster", "program_field_dpr");
    const c = inspectTypedArrayStorage(context, byteStorage, invalid);
    const s = inspectTypedArrayStorage(source, byteStorage, invalid);
    const d = inspectTypedArrayStorage(destination, byteStorage, invalid);
    if (c.length !== 32 || s.length !== count * 4 || d.length !== count * 4) throw invalid();
    const own = (value, inspected) => copyProgramInput(value, byteStorage,
      "updateFinalRaster", "program_field_length", undefined, inspected);
    return runtimeFinalUpdate.call(this, revision, width, height, dpr,
      own(context, c), sourcePort, own(source, s), destinationPort, own(destination, d));
  });
};
const runtimeConsumeFinal = ProgramRuntime.prototype.consumeFinalRaster;
ProgramRuntime.prototype.consumeFinalRaster = function (revision, content, output, context, consumer) {
  return withRuntimeOperation(this, "consumeFinalRaster", () => {
    const identity = copyProgramInput(content, byteStorage, "consumeFinalRaster", "program_field_program", 32);
    const boundContext = copyProgramInput(context, byteStorage, "consumeFinalRaster", "program_field_context", 32);
    return runtimeConsumeFinal.call(this, revision, identity, output, boundContext, consumer);
  });
};
for (const method of ["updateUnknown", "contentIdentity", "finalRasterRefusal", "free"]) {
  const original = ProgramRuntime.prototype[method];
  ProgramRuntime.prototype[method] = function (...args) {
    return withRuntimeOperation(this, method, () => original.apply(this, args));
  };
}
if (Symbol.dispose) ProgramRuntime.prototype[Symbol.dispose] = ProgramRuntime.prototype.free;
