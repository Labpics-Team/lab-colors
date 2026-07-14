// Public entry for @labpics/colors.
//
// Re-exports the wasm-bindgen surface (the default `init` loader, `initSync`,
// and the `LabColors` engine class) plus the vanilla DOM runtime helpers:
// `applyTheme` (one-shot apply), `watchTheme` (reactive sync), and the
// effective-background resolver. The wasm glue is the generated `pkg/` artifact
// (built by `npm run build`).

import {
  evaluateWcag22FeasibilityV1 as evaluateWcag22FeasibilityRawV1,
  wcag22FeasibilityEnvelopeTooLargeV1,
  wcag22FeasibilityMaxRequestBytesV1,
} from "./pkg/labcolors.js";

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
  LabColors,
  evaluateWcag22,
  numericalCapabilityManifest,
} from "./pkg/labcolors.js";

/** Exact derived V1 request ceiling, available after WASM initialization. */
export function wcag22FeasibilityMaxBytes() {
  return wcag22FeasibilityMaxRequestBytesV1();
}

/**
 * Evaluate one strict V1 UTF-8 JSON envelope.
 *
 * The host checks the typed array's byte length before wasm-bindgen performs
 * its avoidable input copy. Rust repeats the authoritative check. For the
 * declared Uint8Array input, envelope, resource and Core failures are returned
 * as typed outcome data. Any other JavaScript value throws a deterministic
 * TypeError before the host reads the WASM-owned ceiling or copies input.
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

export { applyTheme } from "./apply-theme.js";
export { watchTheme } from "./watch-theme.js";
export { adaptTheme } from "./adapt-theme.js";
export {
  effectiveBackground,
  parseCssColor,
  compositeOver,
  compositeStackToHex,
  toHex,
  oklabLerp,
} from "./effective-bg.js";
