// Offline compiler entry for @labpics/colors/compiler.

import {
  evaluateWcag22FeasibilityV1 as evaluateWcag22FeasibilityRawV1,
  wcag22FeasibilityEnvelopeTooLargeV1,
  wcag22FeasibilityMaxRequestBytesV1,
} from "./compiler/labcolors_compiler.js";

const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const typedArrayTag = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  Symbol.toStringTag,
).get;
const typedArrayByteLength = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "byteLength",
).get;
const typedArrayBuffer = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "buffer",
).get;
const typedArrayByteOffset = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "byteOffset",
).get;
const Uint8ArrayConstructor = Uint8Array;

function hasUint8ArrayBrand(value) {
  return ArrayBuffer.isView(value) && typedArrayTag.call(value) === "Uint8Array";
}

export {
  default,
  default as init,
  initSync,
} from "./compiler/labcolors_compiler.js";

/** Exact derived V1 request ceiling, available after compiler WASM initialization. */
export function wcag22FeasibilityMaxBytes() {
  return wcag22FeasibilityMaxRequestBytesV1();
}

/**
 * Evaluate one strict V1 UTF-8 JSON envelope.
 *
 * The host rejects the wrong input type and oversized views before wasm-bindgen
 * can copy them. Rust repeats the authoritative envelope check.
 *
 * @param {Uint8Array} request
 */
export function evaluateWcag22Feasibility(request) {
  if (!hasUint8ArrayBrand(request)) {
    throw new TypeError("evaluateWcag22Feasibility request must be a Uint8Array");
  }
  const snapshotBytes = typedArrayByteLength.call(request);
  let canonicalRequest;
  try {
    canonicalRequest = new Uint8ArrayConstructor(
      typedArrayBuffer.call(request),
      typedArrayByteOffset.call(request),
      snapshotBytes,
    );
  } catch {
    throw new TypeError("evaluateWcag22Feasibility request must be a live Uint8Array");
  }
  const requestedBytes = typedArrayByteLength.call(canonicalRequest);
  if (requestedBytes > wcag22FeasibilityMaxBytes()) {
    return wcag22FeasibilityEnvelopeTooLargeV1(BigInt(requestedBytes));
  }
  return evaluateWcag22FeasibilityRawV1(canonicalRequest);
}
