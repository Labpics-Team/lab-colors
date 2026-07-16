// Reactive theme runtime — zero dependencies.
//
// `applyTheme` writes a resolved theme's `--lab-*` variables once. `watchTheme`
// repeats that operation when an explicitly supplied background input or the
// helper's supported `background-color` reference estimate changes. It does not
// observe rendered pixels or infer a whole backdrop field.
//
// It serves both regimes:
//   * DISCRETE `style`/`class` attribute changes in the observed subtree schedule
//     a refresh through `MutationObserver`; layout and pixel changes do not.
//   * CONTINUOUS changes (a CSS-animated or per-frame-scripted background that
//     never mutates inline style) are driven by the caller calling `refresh()`
//     inside its own `requestAnimationFrame` loop. `refresh()` re-resolves only
//     when the supplied/reference background string changes.
//
// The fallback estimate alpha-composites the supported ancestor
// `background-color` chain (`effective-bg.js`). For images/gradients/blur, pass
// an explicit reference hex; one sample does not represent the whole field.

import { applyTheme } from "./apply-theme.js";
import { effectiveBackground } from "./effective-bg.js";

/**
 * @typedef {object} WatchController
 * @property {(force?: boolean) => (object | null)} refresh  Re-resolve+apply if the
 *   background (or theme) changed; `force` re-applies unconditionally. Returns the
 *   `resolveTheme` result that is now applied, or `null` if nothing was applied.
 * @property {(theme: string) => void} setTheme  Switch theme and re-apply.
 * @property {() => string} background  The background/reference hex last resolved.
 * @property {() => void} stop  Disconnect observers and stop watching.
 */

/**
 * Keep `element`'s `--lab-*` variables aligned with a supplied background or
 * the supported ancestor-chain reference estimate.
 *
 * @param {*} element  The surface to read the background from and (by default)
 *   write the variables onto.
 * @param {object} options
 * @param {{ resolveTheme: (bgHex: string, theme: string) => object }} options.colors
 *   An initialised `LabColors` engine (already `await init()`-ed).
 * @param {string} options.theme  Theme name (`"light" | "dark" | …`).
 * @param {string | (() => string)} [options.background]  Explicit reference
 *   background, overriding the ancestor estimate. A sample from an
 *   image/gradient/blur remains one declared point, not whole-field evidence.
 * @param {*} [options.target=element]  Element to write the variables onto.
 * @param {string} [options.fallback="#FFFFFF"]  Base for a fully-translucent chain.
 * @param {boolean} [options.observe=true]  Auto-refresh on DOM attribute mutations.
 * @param {*} [options.root]  Mutation-observer root (default: the document element).
 * @param {*} [options.win=globalThis]  Window-like host (for MutationObserver).
 * @param {(el:*)=>*} [options.getStyle]  Injection seam for `effectiveBackground`.
 * @param {(el:*)=>*} [options.parentOf]  Injection seam for `effectiveBackground`.
 * @returns {WatchController}
 */
export function watchTheme(element, options) {
  if (!options || typeof options.colors?.resolveTheme !== "function") {
    throw new TypeError("watchTheme: options.colors must be an initialised LabColors engine");
  }
  if (typeof options.theme !== "string") {
    throw new TypeError("watchTheme: options.theme must be a theme name string");
  }

  const target = options.target ?? element;
  const fallback = options.fallback ?? "#FFFFFF";
  const win = options.win ?? (typeof globalThis !== "undefined" ? globalThis : undefined);
  let theme = options.theme;
  let lastBg = null;
  let lastTheme = null;
  let lastResult = null;

  const readBackground = () => {
    const b = options.background;
    if (typeof b === "function") return b();
    if (typeof b === "string") return b;
    return effectiveBackground(element, {
      fallback,
      getStyle: options.getStyle,
      parentOf: options.parentOf,
    });
  };

  const refresh = (force = false) => {
    const bg = readBackground();
    if (!force && bg === lastBg && theme === lastTheme) return lastResult;
    const result = options.colors.resolveTheme(bg, theme);
    applyTheme(target, result);
    lastBg = bg;
    lastTheme = theme;
    lastResult = result;
    return result;
  };

  // Coalesce a burst of mutations into a single refresh on the next microtask.
  let scheduled = false;
  let stopped = false;
  const schedule = () => {
    if (scheduled || stopped) return;
    scheduled = true;
    Promise.resolve().then(() => {
      scheduled = false;
      // A `stop()` between scheduling and this microtask must cancel the refresh
      // — the watcher is done, no late writes.
      if (!stopped) refresh();
    });
  };

  let observer = null;
  if (options.observe !== false && win && typeof win.MutationObserver === "function") {
    const root =
      options.root ??
      (typeof win.document !== "undefined" ? win.document.documentElement : null);
    if (root) {
      observer = new win.MutationObserver(schedule);
      // A background can change on the element OR any ancestor, via inline style
      // or a class swap — so watch attribute changes across the subtree.
      observer.observe(root, {
        subtree: true,
        attributes: true,
        attributeFilter: ["style", "class"],
      });
    }
  }

  // Resolve and apply immediately against the first supplied/reference input.
  refresh(true);

  return {
    refresh,
    setTheme(next) {
      theme = next;
      refresh();
    },
    background() {
      return lastBg;
    },
    stop() {
      stopped = true;
      if (observer) observer.disconnect();
      observer = null;
    },
  };
}
