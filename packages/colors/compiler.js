// Offline compiler entry for @labpics/colors/compiler.

import {
  evaluateWcag22FeasibilityV1 as evaluateWcag22FeasibilityRawV1,
  wcag22FeasibilityEnvelopeTooLargeV1,
  wcag22FeasibilityMaxRequestBytesV1,
} from "./compiler/labcolors_compiler.js";

const typedArrayTag = Object.getOwnPropertyDescriptor(
  Object.getPrototypeOf(Uint8Array.prototype),
  Symbol.toStringTag,
).get;

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
  const requestedBytes = request.byteLength;
  if (requestedBytes > wcag22FeasibilityMaxBytes()) {
    return wcag22FeasibilityEnvelopeTooLargeV1(BigInt(requestedBytes));
  }
  return evaluateWcag22FeasibilityRawV1(request);
}
