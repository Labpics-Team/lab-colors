export const REQUEST_V2_LENGTH = 70;
export const REQUEST_SINK_OUTPUT_OFFSET = 39;
export const REQUEST_STREAM_OFFSET = 43;
export const REQUEST_REVISION_OFFSET = 47;
export const UPDATE_V2_LENGTH = 40;
export const UPDATE_STREAM_OFFSET = 8;
export const UPDATE_REVISION_OFFSET = 12;
export const RESULT_V2_LENGTH = 72;
export const RESULT_V2_MAGIC = Object.freeze([0x4c, 0x43, 0x46, 0x52]);
export const ABI_V2 = 2;

export const RESULT_STATE_OFFSET = 8;
export const RESULT_STREAM_OFFSET = 9;
export const RESULT_REVISION_OFFSET = 13;
export const RESULT_OUTPUT_OFFSET = 21;
export const RESULT_SINK_OUTPUT_OFFSET = 25;
export const RESULT_RGB_OFFSET = 29;
export const RESULT_OPACITY_OFFSET = 32;
export const RESULT_CONTENT_IDENTITY_OFFSET = 40;
export const IDENTITY_LENGTH = 32;

export const HOST_MODULE_V1 = "labcolors_private_fixture_host_v1";
export const HOST_INSTALL_V1 = "labcolors_private_fixture_host_install_v1";
export const HOST_CONFIRM_DISPOSED_V1 = "labcolors_private_fixture_host_confirm_disposed_v1";
export const HOST_INSTALL_SUCCESS_V1 = 0x4c43_0001;
export const HOST_DISPOSE_CONFIRMED_V1 = 0x4c43_0002;

export const OPERATION_SET_ALL_V1 = 1;
export const OPERATION_REVOKE_ALL_V1 = 2;
export const OPERATION_CONFIRM_EXACT_V1 = 3;
export const STATE_WAITING_V2 = 1;
export const STATE_READY_V2 = 2;
export const STATE_STALE_V2 = 3;
export const STATE_FAILED_V2 = 4;
export const DISPOSE_BEGIN_BUSY_V1 = 0xffff_ffff;
export const DISPOSE_TOKEN_BASE_V1 = 0x1000_0000;
export const DISPOSE_TOKEN_ENCODED_END_V1 = 2 * DISPOSE_TOKEN_BASE_V1 - 1;

export const EXPORTS_V1 = Object.freeze({
  requestPointer: "labcolors_private_fixture_request_v1_ptr",
  requestLength: "labcolors_private_fixture_request_v1_len",
  resultPointer: "labcolors_private_fixture_result_v1_ptr",
  resultLength: "labcolors_private_fixture_result_v1_len",
  run: "labcolors_private_fixture_run_v1",
  updatePointer: "labcolors_private_fixture_update_v2_ptr",
  updateLength: "labcolors_private_fixture_update_v2_len",
  update: "labcolors_private_fixture_update_v2",
  beginDispose: "labcolors_private_fixture_begin_dispose_v1",
  abortDispose: "labcolors_private_fixture_abort_dispose_v1",
  commitDispose: "labcolors_private_fixture_commit_dispose_v1",
});

function lowercaseHex(bytes) {
  let value = "";
  for (const byte of bytes) value += byte.toString(16).padStart(2, "0");
  return value;
}

function allZero(bytes) {
  return bytes.every((byte) => byte === 0);
}

export function decodeCertifiedReceipt({
  bytes,
  expectedSinkOutput,
  installedOutput,
  installedSinkOutput,
  expectedStream,
  minimumRevision,
  protocolError = (detail) => new TypeError(detail),
}) {
  if (!(bytes instanceof Uint8Array) || bytes.byteLength !== RESULT_V2_LENGTH) {
    throw protocolError(`private Program result must be ${RESULT_V2_LENGTH} bytes`);
  }
  for (let index = 0; index < RESULT_V2_MAGIC.length; index += 1) {
    if (bytes[index] !== RESULT_V2_MAGIC[index]) throw protocolError("result has invalid magic");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint16(4, true) !== ABI_V2) {
    throw protocolError("result has an unsupported ABI version");
  }
  if (view.getUint16(6, true) !== RESULT_V2_LENGTH) {
    throw protocolError("result declares an invalid length");
  }

  const state = view.getUint8(RESULT_STATE_OFFSET);
  if (![STATE_WAITING_V2, STATE_READY_V2, STATE_STALE_V2, STATE_FAILED_V2].includes(state)) {
    throw protocolError("result has an invalid lifecycle state");
  }
  const stream = view.getUint32(RESULT_STREAM_OFFSET, true);
  const revision = view.getBigUint64(RESULT_REVISION_OFFSET, true);
  if (stream !== expectedStream || revision < minimumRevision) {
    throw protocolError("result observation provenance does not match the active attachment");
  }
  const output = view.getUint32(RESULT_OUTPUT_OFFSET, true);
  const sinkOutput = view.getUint32(RESULT_SINK_OUTPUT_OFFSET, true);
  const paintSource = Object.freeze([
    bytes[RESULT_RGB_OFFSET],
    bytes[RESULT_RGB_OFFSET + 1],
    bytes[RESULT_RGB_OFFSET + 2],
  ]);
  const paintOpacityBits = view.getBigUint64(RESULT_OPACITY_OFFSET, true);
  const identityBytes = bytes.subarray(
    RESULT_CONTENT_IDENTITY_OFFSET,
    RESULT_CONTENT_IDENTITY_OFFSET + IDENTITY_LENGTH,
  );
  if (state === STATE_READY_V2) {
    if (
      output === 0 ||
      sinkOutput !== expectedSinkOutput ||
      sinkOutput !== installedSinkOutput ||
      output !== installedOutput ||
      allZero(identityBytes)
    ) {
      throw protocolError("Ready result lacks an identity-matching certified output");
    }
  } else if (
    output !== 0 ||
    sinkOutput !== 0 ||
    !allZero(paintSource) ||
    paintOpacityBits !== 0n ||
    !allZero(identityBytes)
  ) {
    throw protocolError("no-output result carries forbidden certified output data");
  }
  return Object.freeze({
    output,
    sinkOutput,
    state,
    stream,
    revision,
    paintSource,
    paintOpacityBits,
    contentIdentity: lowercaseHex(identityBytes),
  });
}
