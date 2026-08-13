import {
  decodeCertifiedReceipt,
  DISPOSE_TOKEN_BASE_V1,
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
} from "/installed/private-program/abi-v2.js";
import {
  CHANGED_UPDATE_HEX,
  exactWireBytes,
  receiptFingerprint,
  REQUEST_HEX,
  UPDATE_HEX,
} from "./vectors.mjs";

const WASM_URL = "/installed/private-program/labcolors_private_program.wasm";

function u32(value) {
  return value >>> 0;
}

function checkedView(memory, pointer, length) {
  const offset = u32(pointer);
  if (offset > memory.buffer.byteLength || length > memory.buffer.byteLength - offset) {
    throw new RangeError("private Program worker memory range is invalid");
  }
  return new Uint8Array(memory.buffer, offset, length);
}

async function runLifecycle() {
  let exports;
  let installedOutput = 0;
  let installedSinkOutput = 0;
  let expectedSinkOutput = 0;
  let sequence = 0n;
  let generation = 0;
  let disposeConfirmed = false;

  function install(
    rawGeneration,
    rawOperation,
    _revisionLow,
    _revisionHigh,
    expectedLow,
    expectedHigh,
    desiredLow,
    desiredHigh,
    rawOutput,
    rawSinkOutput,
  ) {
    generation = u32(rawGeneration);
    const operation = u32(rawOperation);
    const expected = BigInt(u32(expectedLow)) | (BigInt(u32(expectedHigh)) << 32n);
    const desired = BigInt(u32(desiredLow)) | (BigInt(u32(desiredHigh)) << 32n);
    const output = u32(rawOutput);
    const sinkOutput = u32(rawSinkOutput);
    if (operation === OPERATION_CONFIRM_EXACT_V1) {
      return expected === sequence && desired === sequence && output === installedOutput && sinkOutput === installedSinkOutput
        ? HOST_INSTALL_SUCCESS_V1
        : 0;
    }
    if (expected !== sequence || desired !== sequence + 1n) return 0;
    if (operation === OPERATION_SET_ALL_V1) {
      if (output === 0 || sinkOutput !== expectedSinkOutput) return 0;
      installedOutput = output;
      installedSinkOutput = sinkOutput;
    } else if (operation === OPERATION_REVOKE_ALL_V1) {
      if (output !== 0 || sinkOutput !== 0) return 0;
      installedOutput = 0;
      installedSinkOutput = 0;
    } else {
      return 0;
    }
    sequence = desired;
    return HOST_INSTALL_SUCCESS_V1;
  }

  function confirmDisposed(rawGeneration, rawToken) {
    disposeConfirmed = u32(rawGeneration) === generation && u32(rawToken) === generation;
    return disposeConfirmed ? HOST_DISPOSE_CONFIRMED_V1 : 0;
  }

  const source = await (await fetch(WASM_URL)).arrayBuffer();
  const instance = await WebAssembly.instantiate(source, {
    [HOST_MODULE_V1]: {
      [HOST_INSTALL_V1]: install,
      [HOST_CONFIRM_DISPOSED_V1]: confirmDisposed,
    },
  });
  exports = instance.instance.exports;
  const memory = exports.memory;

  function invoke(bytes, pointerName, length, operationName, minimumRevision) {
    checkedView(memory, exports[pointerName](), length).set(bytes);
    const status = u32(exports[operationName]());
    if (status !== 0) throw new Error(`${operationName} failed with status ${status}`);
    return decodeCertifiedReceipt({
      bytes: new Uint8Array(checkedView(memory, exports[EXPORTS_V1.resultPointer](), RESULT_V2_LENGTH)),
      expectedSinkOutput,
      installedOutput,
      installedSinkOutput,
      expectedStream: new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(
        operationName === EXPORTS_V1.run ? REQUEST_STREAM_OFFSET : UPDATE_STREAM_OFFSET,
        true,
      ),
      minimumRevision,
    });
  }

  const request = exactWireBytes(REQUEST_HEX);
  expectedSinkOutput = new DataView(request.buffer).getUint32(REQUEST_SINK_OUTPUT_OFFSET, true);
  const initial = invoke(
    request,
    EXPORTS_V1.requestPointer,
    REQUEST_V2_LENGTH,
    EXPORTS_V1.run,
    new DataView(request.buffer).getBigUint64(REQUEST_REVISION_OFFSET, true),
  );
  const update = exactWireBytes(UPDATE_HEX);
  const updated = invoke(
    update,
    EXPORTS_V1.updatePointer,
    UPDATE_V2_LENGTH,
    EXPORTS_V1.update,
    new DataView(update.buffer).getBigUint64(UPDATE_REVISION_OFFSET, true),
  );
  const changedUpdate = exactWireBytes(CHANGED_UPDATE_HEX);
  const changed = invoke(
    changedUpdate,
    EXPORTS_V1.updatePointer,
    UPDATE_V2_LENGTH,
    EXPORTS_V1.update,
    new DataView(changedUpdate.buffer).getBigUint64(UPDATE_REVISION_OFFSET, true),
  );

  const token = u32(exports[EXPORTS_V1.beginDispose]());
  if (token !== generation + DISPOSE_TOKEN_BASE_V1) throw new Error("dispose token mismatch");
  if (u32(exports[EXPORTS_V1.commitDispose](token)) !== 0 || !disposeConfirmed) {
    throw new Error("dispose confirmation mismatch");
  }
  return Object.freeze({
    initial: receiptFingerprint(initial),
    updated: receiptFingerprint(updated),
    changed: receiptFingerprint(changed),
  });
}

self.addEventListener("message", async (event) => {
  if (event.data !== "run") return;
  try {
    self.postMessage({ ok: true, value: await runLifecycle() });
  } catch (error) {
    self.postMessage({ ok: false, error: error instanceof Error ? error.message : String(error) });
  }
});
