// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: write a resolved theme's
// reachable colours onto an element as `--lab-*` custom properties.

import { admitSnapshot, writeVars } from "./snapshot.js";

/**
 * Apply a resolved theme's CSS variables to an element.
 *
 * The full result is admitted before the first CSSOM call. An ordinary
 * Unreachable rejects atomically as `OutputConflictError`; explicit None,
 * Unresolved, and numerical indeterminacy remain value-less metadata. After
 * admission, the writer clears prior inline `--lab-*` values and writes the
 * selected values from `result.vars`.
 *
 * @param {HTMLElement} element - The target element (e.g. `document.documentElement`).
 * @param {{ vars: Record<string, string>, roles: Record<string, object> }} result
 *   A complete `resolveTheme(...)` result.
 * @returns {void}
 */
export function applyTheme(element, result) {
  if (!element || typeof element.style?.setProperty !== "function") {
    throw new TypeError("applyTheme: first argument must be an element with a style");
  }
  const snapshot = admitSnapshot(result, "applyTheme");
  writeVars(element, snapshot.vars, "applyTheme");
}
