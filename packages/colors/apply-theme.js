// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: write a resolved theme's
// reachable colours onto an element as `--lab-*` custom properties.

/**
 * Apply a resolved theme's CSS variables to an element.
 *
 * Writes every reachable role from `result.vars` onto `element.style` via
 * `setProperty`. Unreachable and zero-token (`none`) roles carry no colour, so
 * they are absent from `vars` and are simply not written — the caller's CSS
 * fallbacks stay in effect for those, which is the honest behaviour.
 *
 * @param {HTMLElement} element - The target element (e.g. `document.documentElement`).
 * @param {{ vars: Record<string, string> }} result - A `resolveTheme(...)` result.
 * @returns {void}
 */
export function applyTheme(element, result) {
  if (!element || typeof element.style?.setProperty !== "function") {
    throw new TypeError("applyTheme: first argument must be an element with a style");
  }
  if (!result || typeof result.vars !== "object" || result.vars === null) {
    throw new TypeError("applyTheme: second argument must be a resolveTheme result");
  }
  for (const [name, value] of Object.entries(result.vars)) {
    element.style.setProperty(name, value);
  }
}
