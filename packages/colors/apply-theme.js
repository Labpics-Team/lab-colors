// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: write a resolved theme's
// reachable colours onto an element as `--lab-*` custom properties.

const LAB_VAR_PREFIX = "--lab-";

/**
 * Apply a resolved theme's CSS variables to an element.
 *
 * First clears every inline `--lab-*` custom property a previous call set on
 * this element, then writes every reachable role from `result.vars` via
 * `setProperty`. Unreachable and zero-token (`none`) roles carry no colour, so
 * they are absent from `vars` and are not written — the caller's CSS fallbacks
 * stay in effect for those, including across theme re-application (a role
 * reachable in the previous theme but not in the new one does not linger).
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
  // Inline style is a live list; collect names first, then remove, so the
  // iteration is not invalidated mid-walk.
  const stale = [];
  for (let i = 0; i < element.style.length; i++) {
    const name = element.style.item(i);
    if (name.startsWith(LAB_VAR_PREFIX)) stale.push(name);
  }
  for (const name of stale) {
    element.style.removeProperty(name);
  }
  for (const [name, value] of Object.entries(result.vars)) {
    element.style.setProperty(name, value);
  }
}
