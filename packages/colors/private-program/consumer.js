import { acquireOutputLease } from "../output-sink.js";
import {
  decodeCertifiedReceipt,
  DISPOSE_BEGIN_BUSY_V1,
  DISPOSE_TOKEN_BASE_V1,
  DISPOSE_TOKEN_ENCODED_END_V1,
  EXPORTS_V1,
  HOST_CONFIRM_DISPOSED_V1,
  HOST_DISPOSE_CONFIRMED_V1,
  HOST_INSTALL_SUCCESS_V1,
  HOST_INSTALL_V1,
  HOST_MODULE_V1,
  OPERATION_CONFIRM_EXACT_V1,
  OPERATION_REVOKE_ALL_V1,
  OPERATION_SET_ALL_V1,
  REQUEST_REVISION_OFFSET,
  REQUEST_SINK_OUTPUT_OFFSET,
  REQUEST_STREAM_OFFSET,
  REQUEST_V2_LENGTH,
  RESULT_V2_LENGTH,
  UPDATE_REVISION_OFFSET,
  UPDATE_STREAM_OFFSET,
  UPDATE_V2_LENGTH,
} from "./abi-v2.js";

const PRIVATE_PROGRAM_WASM_URL = new URL("./labcolors_private_program.wasm", import.meta.url);
// Live dispose tokens live in [DISPOSE_TOKEN_BASE_V1, 2 * DISPOSE_TOKEN_BASE_V1 - 1],
// disjoint from every Core status code (1..=15), the Vacant sentinel 0, and the
// Busy sentinel, so a begin-dispose result can be classified without ambiguity.
const I32_MIN = -0x8000_0000;
const I32_MAX = 0x7fff_ffff;
const MAX_CANONICAL_RGBA_V1_LENGTH =
  "rgba(255,255,255,)".length + expandShortestDecimal(Number.MIN_VALUE).length;
const CANONICAL_RGBA_V1 = /^rgba\(([0-9]{1,3}),([0-9]{1,3}),([0-9]{1,3}),(0|1|0\.[0-9]+)\)$/u;

const UTF8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

class PrivateProgramConsumerError extends Error {
  constructor(code, detail, options) {
    super(`private Program consumer: ${detail}`, options);
    this.name = "PrivateProgramConsumerError";
    this.code = code;
  }
}

function protocolError(detail, options) {
  return new PrivateProgramConsumerError("PRIVATE_PROGRAM_PROTOCOL", detail, options);
}

function lifecycleError(detail) {
  return new PrivateProgramConsumerError("PRIVATE_PROGRAM_LIFECYCLE", detail);
}

function wasmStatusError(operation, status) {
  const error = new PrivateProgramConsumerError(
    "PRIVATE_PROGRAM_WASM_STATUS",
    `${operation} failed with status ${status}`,
  );
  error.status = status;
  return error;
}

function asError(cause, detail) {
  return cause instanceof Error ? cause : protocolError(detail, { cause });
}

function exactI32Carrier(value, label) {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < I32_MIN ||
    value > I32_MAX
  ) {
    throw protocolError(`${label} is not a signed i32 carrier`);
  }
  return value;
}

function exactU32(value, label) {
  exactI32Carrier(value, label);
  return value >>> 0;
}

function joinU64(low, high) {
  return BigInt(exactU32(low, "u64 low word")) |
    (BigInt(exactU32(high, "u64 high word")) << 32n);
}

function exactStatus(value, label) {
  exactI32Carrier(value, `${label} status`);
  return value >>> 0;
}

function exactLeaseSuccess(value, operation) {
  if (value !== true) {
    throw protocolError(`${operation} did not synchronously return literal true`);
  }
}

function expandShortestDecimal(value) {
  const shortest = String(value);
  const exponentOffset = shortest.indexOf("e");
  if (exponentOffset === -1) return shortest;
  const coefficient = shortest.slice(0, exponentOffset);
  const exponent = Number(shortest.slice(exponentOffset + 1));
  const pointOffset = coefficient.indexOf(".");
  const integerDigits = pointOffset === -1 ? coefficient.length : pointOffset;
  const digits = coefficient.replace(".", "");
  const shiftedPoint = integerDigits + exponent;
  if (shiftedPoint <= 0) return `0.${"0".repeat(-shiftedPoint)}${digits}`;
  if (shiftedPoint >= digits.length) {
    return `${digits}${"0".repeat(shiftedPoint - digits.length)}`;
  }
  return `${digits.slice(0, shiftedPoint)}.${digits.slice(shiftedPoint)}`;
}

function admitCanonicalRgba(css) {
  const match = CANONICAL_RGBA_V1.exec(css);
  if (match === null) throw protocolError("host CSS is not canonical rgba(R,G,B,A)");
  for (let index = 1; index <= 3; index++) {
    const channel = Number(match[index]);
    if (channel > 255 || String(channel) !== match[index]) {
      throw protocolError("host CSS is not canonical rgba(R,G,B,A)");
    }
  }
  const alphaText = match[4];
  const alpha = Number(alphaText);
  if (
    !Number.isFinite(alpha) ||
    alpha < 0 ||
    alpha > 1 ||
    expandShortestDecimal(alpha) !== alphaText
  ) {
    throw protocolError("host CSS is not canonical rgba(R,G,B,A)");
  }
  return css;
}

function exactRequestBytes(value) {
  if (
    !(value instanceof Uint8Array) ||
    Object.getPrototypeOf(value) !== Uint8Array.prototype ||
    value.byteLength !== REQUEST_V2_LENGTH
  ) {
    throw new TypeError(
      `private Program request must be an exact Uint8Array of ${REQUEST_V2_LENGTH} bytes`,
    );
  }
  return new Uint8Array(value);
}

function exactUpdateBytes(value) {
  if (
    !(value instanceof Uint8Array) ||
    Object.getPrototypeOf(value) !== Uint8Array.prototype ||
    value.byteLength !== UPDATE_V2_LENGTH
  ) {
    throw new TypeError(`private Program update must be an exact Uint8Array of ${UPDATE_V2_LENGTH} bytes`);
  }
  return new Uint8Array(value);
}

function checkedMemoryView(memory, rawPointer, length, label) {
  const pointer = exactU32(rawPointer, `${label} pointer`);
  const buffer = memory.buffer;
  if (
    !(buffer instanceof ArrayBuffer) ||
    pointer > buffer.byteLength ||
    length > buffer.byteLength - pointer
  ) {
    throw protocolError(`${label} range is outside the current WASM memory`);
  }
  return new Uint8Array(buffer, pointer, length);
}

function exactExport(exports, name) {
  const value = exports[name];
  if (typeof value !== "function") throw protocolError(`missing WASM export '${name}'`);
  return value;
}

function validateExports(instance) {
  const exports = instance?.exports;
  if (exports === null || typeof exports !== "object") {
    throw protocolError("WebAssembly instance has no exports object");
  }
  if (!(exports.memory instanceof WebAssembly.Memory)) {
    throw protocolError("WASM export 'memory' is not WebAssembly.Memory");
  }
  if (!(exports.memory.buffer instanceof ArrayBuffer)) {
    throw protocolError("WASM export 'memory' must use an unshared ArrayBuffer");
  }
  for (const name of Object.values(EXPORTS_V1)) exactExport(exports, name);
  const requestLength = exactStatus(exports[EXPORTS_V1.requestLength](), "request length");
  const resultLength = exactStatus(exports[EXPORTS_V1.resultLength](), "result length");
  const updateLength = exactStatus(exports[EXPORTS_V1.updateLength](), "update length");
  if (
    requestLength !== REQUEST_V2_LENGTH ||
    updateLength !== UPDATE_V2_LENGTH ||
    resultLength !== RESULT_V2_LENGTH
  ) {
    throw protocolError(
      `WASM buffer lengths are ${requestLength}/${updateLength}/${resultLength}, expected ` +
        `${REQUEST_V2_LENGTH}/${UPDATE_V2_LENGTH}/${RESULT_V2_LENGTH}`,
    );
  }
  return exports;
}

function readRequestSinkOutput(bytes) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(
    REQUEST_SINK_OUTPUT_OFFSET,
    true,
  );
}

function frozenPublication(outputBinding, css) {
  const publication = Object.create(null);
  Object.defineProperty(publication, outputBinding, {
    configurable: false,
    enumerable: true,
    value: css,
    writable: false,
  });
  return Object.freeze(publication);
}

async function instantiatePrivateProgram(imports) {
  const response = await fetch(PRIVATE_PROGRAM_WASM_URL);
  if (response?.ok !== true || typeof response.arrayBuffer !== "function") {
    throw new PrivateProgramConsumerError(
      "PRIVATE_PROGRAM_FETCH",
      `failed to fetch sibling WASM '${PRIVATE_PROGRAM_WASM_URL.href}'`,
    );
  }
  const source = await response.arrayBuffer();
  const instantiated = await WebAssembly.instantiate(source, imports);
  const instance = instantiated?.instance ?? instantiated;
  if (!(instance instanceof WebAssembly.Instance) && typeof instance?.exports !== "object") {
    throw protocolError("WebAssembly.instantiate returned no instance");
  }
  return instance;
}

/**
 * Creates the private, single-output Program consumer backed by its sibling WASM artifact.
 */
export async function createPrivateProgramConsumer(options) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("private Program consumer options must be an object");
  }
  const { target, outputBinding } = options;
  let exports = null;
  let phase = "vacant";
  let lease = null;
  let requestSinkOutput = null;
  let observationStream = null;
  let observationRevision = null;
  let generation = null;
  let installedOutput = null;
  let installedSinkOutput = null;
  let hostInstallCommitted = false;
  let hostInstallInFlight = false;
  let hostFailure = null;
  let disposeToken = null;
  let tombstoneGeneration = null;
  let confirmSeen = false;

  function resetVacant() {
    phase = "vacant";
    lease = null;
    requestSinkOutput = null;
    observationStream = null;
    observationRevision = null;
    generation = null;
    installedOutput = null;
    installedSinkOutput = null;
    hostInstallCommitted = false;
    hostInstallInFlight = false;
    hostFailure = null;
    disposeToken = null;
    tombstoneGeneration = null;
    confirmSeen = false;
  }

  function hostInstallChecked(
    rawGeneration,
    rawOperation,
    revisionLow,
    revisionHigh,
    expectedLow,
    expectedHigh,
    desiredLow,
    desiredHigh,
    rawOutput,
    rawSinkOutput,
    rawCssPointer,
    rawCssLength,
  ) {
    if ((phase !== "running" && phase !== "updating") || lease === null) {
      throw lifecycleError("host install arrived outside run");
    }
    if (phase === "running" && hostInstallCommitted) {
      throw protocolError("shipping run permits exactly one SetAll callback");
    }
    const nextGeneration = exactU32(rawGeneration, "host generation");
    if (nextGeneration === 0) throw protocolError("host generation must be non-zero");
    if (generation === null) generation = nextGeneration;
    else if (generation !== nextGeneration) throw protocolError("host generation changed during run");

    const operation = exactU32(rawOperation, "host operation");
    exactU32(revisionLow, "host revision low word");
    exactU32(revisionHigh, "host revision high word");
    const expected = joinU64(expectedLow, expectedHigh);
    const desired = joinU64(desiredLow, desiredHigh);
    const output = exactU32(rawOutput, "host output");
    const sinkOutput = exactU32(rawSinkOutput, "host sink output");
    const cssLength = exactU32(rawCssLength, "host CSS length");

    if (operation === OPERATION_CONFIRM_EXACT_V1) {
      if (!hostInstallCommitted || expected !== desired || output !== installedOutput || sinkOutput !== installedSinkOutput) {
        throw protocolError("ConfirmExact does not match the committed snapshot");
      }
      return HOST_INSTALL_SUCCESS_V1;
    }
    const expectedSequence = hostInstallCommitted ? desired - 1n : 0n;
    if (expected !== expectedSequence || desired !== expected + 1n) {
      throw protocolError("terminal sequence must advance exactly once");
    }
    if (operation === OPERATION_REVOKE_ALL_V1) {
      if (output !== 0 || sinkOutput !== 0 || cssLength !== 0) {
        throw protocolError("RevokeAll must carry no output payload");
      }
      exactLeaseSuccess(lease.publish(Object.freeze(Object.create(null))), "output lease revoke");
      installedOutput = 0;
      installedSinkOutput = 0;
      hostInstallCommitted = true;
      return HOST_INSTALL_SUCCESS_V1;
    }
    if (operation !== OPERATION_SET_ALL_V1) {
      throw protocolError("host operation is not part of the terminal algebra");
    }
    if (sinkOutput !== requestSinkOutput || cssLength === 0) {
      throw protocolError("SetAll does not match the authored sink identity");
    }
    if (cssLength > MAX_CANONICAL_RGBA_V1_LENGTH) {
      throw protocolError("host CSS length exceeds the canonical rgba bound");
    }
    const cssBytes = checkedMemoryView(exports.memory, rawCssPointer, cssLength, "host CSS");
    if (cssBytes.some((byte) => byte > 0x7f)) {
      throw protocolError("host CSS is not ASCII");
    }
    const css = admitCanonicalRgba(UTF8.decode(cssBytes));
    exactLeaseSuccess(lease.publish(frozenPublication(outputBinding, css)), "output lease publish");
    installedOutput = output;
    installedSinkOutput = sinkOutput;
    hostInstallCommitted = true;
    return HOST_INSTALL_SUCCESS_V1;
  }

  function hostInstall(...arguments_) {
    if (hostFailure !== null) return 0;
    if (hostInstallInFlight) {
      hostFailure = protocolError("reentrant host install is forbidden");
      return 0;
    }
    hostInstallInFlight = true;
    try {
      return hostInstallChecked(...arguments_);
    } catch (cause) {
      hostFailure ??= asError(cause, "host install threw a non-Error value");
      return 0;
    } finally {
      hostInstallInFlight = false;
    }
  }

  function hostConfirmDisposed(rawGeneration, rawToken) {
    try {
      const confirmedGeneration = exactU32(rawGeneration, "dispose generation");
      const confirmedToken = exactU32(rawToken, "dispose token");
      if (
        phase !== "commit-pending" ||
        tombstoneGeneration === null ||
        disposeToken === null ||
        confirmedGeneration !== tombstoneGeneration ||
        confirmedToken !== disposeToken
      ) {
        return 0;
      }
      confirmSeen = true;
      return HOST_DISPOSE_CONFIRMED_V1;
    } catch {
      return 0;
    }
  }

  const imports = Object.freeze({
    [HOST_MODULE_V1]: Object.freeze({
      [HOST_INSTALL_V1]: hostInstall,
      [HOST_CONFIRM_DISPOSED_V1]: hostConfirmDisposed,
    }),
  });
  const instance = await instantiatePrivateProgram(imports);
  exports = validateExports(instance);

  function directCleanup() {
    phase = "cleanup-running";
    try {
      exactLeaseSuccess(lease.dispose(), "output lease dispose");
      resetVacant();
      return true;
    } catch (cause) {
      phase = "cleanup-required";
      throw cause;
    }
  }

  function cleanupPoisonedHostLease() {
    phase = "poison-cleanup-running";
    try {
      exactLeaseSuccess(lease.dispose(), "output lease dispose");
    } catch (cause) {
      phase = "poisoned-cleanup-required";
      throw cause;
    }
    resetVacant();
    phase = "poisoned";
    return true;
  }

  function poisonUnknownCore(failure, lifecycleFailure, hostLeaseAlreadyDisposed = false) {
    if (hostLeaseAlreadyDisposed) {
      resetVacant();
      phase = "poisoned";
      throw new AggregateError(
        [failure, lifecycleFailure],
        "private Program host lease is clean but Core lifecycle is poisoned",
      );
    }
    phase = "poisoned-cleanup-required";
    try {
      cleanupPoisonedHostLease();
    } catch (cleanupCause) {
      throw new AggregateError(
        [
          failure,
          lifecycleFailure,
          asError(cleanupCause, "poisoned cleanup threw a non-Error value"),
        ],
        "private Program operation failed, Core lifecycle is unknown, and host lease cleanup failed",
      );
    }
    throw new AggregateError(
      [failure, lifecycleFailure],
      "private Program host lease was cleaned but Core lifecycle is poisoned",
    );
  }

  function copyRequest(request) {
    const pointer = exports[EXPORTS_V1.requestPointer]();
    checkedMemoryView(exports.memory, pointer, REQUEST_V2_LENGTH, "request buffer").set(request);
  }

  function copyUpdate(update) {
    const pointer = exports[EXPORTS_V1.updatePointer]();
    checkedMemoryView(exports.memory, pointer, UPDATE_V2_LENGTH, "update buffer").set(update);
  }

  function copyResult() {
    const pointer = exports[EXPORTS_V1.resultPointer]();
    return new Uint8Array(
      checkedMemoryView(exports.memory, pointer, RESULT_V2_LENGTH, "result buffer"),
    );
  }

  function abortCoreDispose(failure, retainTombstone) {
    let abortFailure = null;
    try {
      const status = exactStatus(
        exports[EXPORTS_V1.abortDispose](disposeToken + DISPOSE_TOKEN_BASE_V1),
        "abort dispose",
      );
      if (status !== 0) abortFailure = wasmStatusError("abort dispose", status);
    } catch (cause) {
      abortFailure = asError(cause, "abort dispose threw a non-Error value");
    }
    if (abortFailure !== null) {
      return poisonUnknownCore(failure, abortFailure, tombstoneGeneration !== null);
    }
    phase = "active";
    disposeToken = null;
    if (!retainTombstone) tombstoneGeneration = null;
    confirmSeen = false;
    throw failure;
  }

  function commitDisposedAttachment() {
    phase = "commit-pending";
    confirmSeen = false;
    let failure = null;
    try {
      const status = exactStatus(
        exports[EXPORTS_V1.commitDispose](disposeToken + DISPOSE_TOKEN_BASE_V1),
        "commit dispose",
      );
      if (status !== 0) failure = wasmStatusError("commit dispose", status);
      else if (!confirmSeen) {
        failure = protocolError("commit dispose did not confirm the same-generation tombstone");
      }
    } catch (cause) {
      failure = asError(cause, "commit dispose threw a non-Error value");
    }
    if (failure !== null) return abortCoreDispose(failure, true);
    resetVacant();
    return true;
  }

  function closeBeganDispose() {
    if (generation === null || lease === null || disposeToken === null) {
      return abortCoreDispose(
        protocolError("Core disposal has no matching host generation"),
        tombstoneGeneration !== null,
      );
    }
    if (tombstoneGeneration !== null && tombstoneGeneration !== generation) {
      return abortCoreDispose(protocolError("host tombstone generation changed"), true);
    }
    if (tombstoneGeneration === null) {
      try {
        exactLeaseSuccess(lease.dispose(), "output lease dispose");
      } catch (cause) {
        return abortCoreDispose(
          asError(cause, "output lease dispose threw a non-Error value"),
          false,
        );
      }
      tombstoneGeneration = generation;
    }
    return commitDisposedAttachment();
  }

  function beginActiveDispose() {
    const hostLeaseAlreadyDisposed = tombstoneGeneration !== null;
    phase = "disposing";
    let token;
    try {
      token = exactStatus(exports[EXPORTS_V1.beginDispose](), "begin dispose");
    } catch (cause) {
      return poisonUnknownCore(
        asError(cause, "begin dispose threw a non-Error value"),
        lifecycleError("Core lifecycle is unknown after begin dispose"),
        hostLeaseAlreadyDisposed,
      );
    }
    if (token === 0) {
      return poisonUnknownCore(
        protocolError("begin dispose returned zero for an active consumer"),
        lifecycleError("Core and consumer lifecycles diverged before host cleanup"),
        hostLeaseAlreadyDisposed,
      );
    }
    if (token === DISPOSE_BEGIN_BUSY_V1) {
      return poisonUnknownCore(
        protocolError("begin dispose returned Busy outside consumer reentry"),
        lifecycleError("Core lifecycle is unknown after begin dispose"),
        hostLeaseAlreadyDisposed,
      );
    }
    // Core encodes every live dispose token above every error status: a value
    // outside the live range is a typed Core failure and must never be read
    // as a lease token.
    if (token < DISPOSE_TOKEN_BASE_V1 || token > DISPOSE_TOKEN_ENCODED_END_V1) {
      return poisonUnknownCore(
        wasmStatusError("begin dispose", token),
        lifecycleError("Core lifecycle is unknown after begin dispose"),
        hostLeaseAlreadyDisposed,
      );
    }
    const rawToken = token - DISPOSE_TOKEN_BASE_V1;
    if (generation === null) generation = rawToken;
    if (rawToken !== generation) {
      disposeToken = rawToken;
      return abortCoreDispose(
        protocolError("begin dispose returned a stale generation"),
        tombstoneGeneration !== null,
      );
    }
    disposeToken = rawToken;
    return closeBeganDispose();
  }

  function probeAndCleanupFailedRun(failure) {
    phase = "probing";
    let token;
    try {
      token = exactStatus(exports[EXPORTS_V1.beginDispose](), "probe active attachment");
    } catch (cause) {
      return poisonUnknownCore(
        failure,
        asError(cause, "active attachment probe threw a non-Error value"),
      );
    }
    if (token === DISPOSE_BEGIN_BUSY_V1) {
      return poisonUnknownCore(
        failure,
        protocolError("active attachment probe returned Busy"),
      );
    }
    if (token === 0) {
      phase = "cleanup-required";
      try {
        directCleanup();
      } catch (cleanupCause) {
        throw new AggregateError(
          [failure, asError(cleanupCause, "pre-attach cleanup threw a non-Error value")],
          "private Program run and provisional lease cleanup both failed",
        );
      }
      throw failure;
    }
    // A value outside the encoded live-token range is a typed Core failure
    // and must never be read as a lease token.
    if (token < DISPOSE_TOKEN_BASE_V1 || token > DISPOSE_TOKEN_ENCODED_END_V1) {
      return poisonUnknownCore(
        failure,
        wasmStatusError("active attachment probe", token),
      );
    }
    const rawToken = token - DISPOSE_TOKEN_BASE_V1;

    let generationFailure = null;
    if (generation === null) generation = rawToken;
    else if (generation !== rawToken) {
      generationFailure = protocolError("host callback generation disagrees with Core lifecycle");
      generation = rawToken;
    }
    phase = "disposing";
    disposeToken = rawToken;
    try {
      closeBeganDispose();
    } catch (cleanupCause) {
      throw new AggregateError(
        [
          failure,
          ...(generationFailure === null ? [] : [generationFailure]),
          asError(cleanupCause, "active cleanup threw a non-Error value"),
        ],
        "private Program run and active attachment cleanup both failed",
      );
    }
    if (generationFailure !== null) {
      throw new AggregateError(
        [failure, generationFailure],
        "private Program run exposed a mismatched host generation",
      );
    }
    throw failure;
  }

  function run(requestBytes) {
    if (phase !== "vacant") throw lifecycleError(`run is unavailable while ${phase}`);
    const request = exactRequestBytes(requestBytes);
    copyRequest(request);

    const authoredSinkOutput = readRequestSinkOutput(request);
    phase = "preparing";
    let acquired;
    try {
      acquired = acquireOutputLease(target, [outputBinding], "private Program consumer");
    } catch (cause) {
      phase = "vacant";
      throw cause;
    }
    lease = acquired;
    phase = "running";
    requestSinkOutput = authoredSinkOutput;
    observationStream = new DataView(request.buffer, request.byteOffset, request.byteLength).getUint32(
      REQUEST_STREAM_OFFSET,
      true,
    );
    observationRevision = new DataView(request.buffer, request.byteOffset, request.byteLength).getBigUint64(
      REQUEST_REVISION_OFFSET,
      true,
    );
    generation = null;
    installedOutput = null;
    installedSinkOutput = null;
    hostInstallCommitted = false;
    hostInstallInFlight = false;
    hostFailure = null;

    let status;
    let runCause = null;
    try {
      status = exactStatus(exports[EXPORTS_V1.run](), "run");
    } catch (cause) {
      runCause = asError(cause, "WASM run threw a non-Error value");
    }

    if (hostFailure !== null || runCause !== null || status !== 0) {
      const failure = hostFailure ?? runCause ?? wasmStatusError("run", status);
      return probeAndCleanupFailedRun(failure);
    }
    if (
      !hostInstallCommitted ||
      generation === null ||
      installedOutput === null ||
      installedSinkOutput === null
    ) {
      return probeAndCleanupFailedRun(
        protocolError("successful run did not install one certified output"),
      );
    }
    let receipt;
    try {
      receipt = decodeCertifiedReceipt({
        bytes: copyResult(),
        expectedSinkOutput: authoredSinkOutput,
        installedOutput,
        installedSinkOutput,
        expectedStream: observationStream,
        minimumRevision: observationRevision,
        protocolError,
      });
    } catch (cause) {
      return probeAndCleanupFailedRun(asError(cause, "result decoding threw a non-Error value"));
    }
    phase = "active";
    return receipt;
  }

  function update(updateBytes) {
    if (phase !== "active") throw lifecycleError(`update is unavailable while ${phase}`);
    const update = exactUpdateBytes(updateBytes);
    const incomingStream = new DataView(update.buffer, update.byteOffset, update.byteLength).getUint32(
      UPDATE_STREAM_OFFSET,
      true,
    );
    const incomingRevision = new DataView(update.buffer, update.byteOffset, update.byteLength).getBigUint64(
      UPDATE_REVISION_OFFSET,
      true,
    );
    if (incomingStream !== observationStream) {
      throw protocolError("observation update stream does not match the active attachment");
    }
    if (observationRevision === null || incomingRevision < observationRevision) {
      throw protocolError("observation update revision rolled back");
    }
    copyUpdate(update);
    phase = "updating";
    hostFailure = null;
    hostInstallInFlight = false;
    let status;
    try {
      status = exactStatus(exports[EXPORTS_V1.update](), "update");
    } catch (cause) {
      return probeAndCleanupFailedRun(asError(cause, "WASM update threw a non-Error value"));
    }
    if (hostFailure !== null) return probeAndCleanupFailedRun(hostFailure);
    if (status !== 0) {
      phase = "active";
      throw wasmStatusError("update", status);
    }
    let receipt;
    try {
      receipt = decodeCertifiedReceipt({
        bytes: copyResult(),
        expectedSinkOutput: requestSinkOutput,
        installedOutput,
        installedSinkOutput,
        expectedStream: observationStream,
        minimumRevision: incomingRevision,
        protocolError,
      });
    } catch (cause) {
      return probeAndCleanupFailedRun(
        asError(cause, "update result decoding threw a non-Error value"),
      );
    }
    observationRevision = receipt.revision;
    phase = "active";
    return receipt;
  }

  function dispose() {
    if (phase === "vacant") return true;
    if (phase === "poisoned") return true;
    if (phase === "poisoned-cleanup-required") return cleanupPoisonedHostLease();
    if (phase === "cleanup-required") return directCleanup();
    if (phase !== "active") throw lifecycleError(`dispose is unavailable while ${phase}`);
    if (generation === null || lease === null) {
      throw protocolError("active attachment has no matching host generation");
    }
    return beginActiveDispose();
  }

  return Object.freeze({ run, update, dispose });
}
