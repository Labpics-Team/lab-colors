// Reactive theme runtime — zero dependencies.
//
// `applyTheme` writes a resolved theme's `--lab-*` variables onto an element
// once. `watchTheme` closes the loop the css-injection-runtime chapter left
// open: it keeps an element's variables *in sync* with its background. Point it
// at a surface and theming follows the background — the seamless drop-in for a
// design system that re-resolves against a changing background.
//
// It serves both regimes:
//   * DISCRETE changes (theme switch, a class/style toggle, DOM reflow) are
//     caught automatically by a `MutationObserver` — call once, forget.
//   * CONTINUOUS changes (a CSS-animated or per-frame-scripted background that
//     never mutates inline style) are driven by the caller calling `refresh()`
//     inside its own `requestAnimationFrame` loop. `refresh()` is cheap: it
//     re-resolves only when the effective background actually changed, so a
//     steady background costs one string compare per frame, not a WASM solve.
//
// The effective background is computed by alpha-compositing ancestors
// (`effective-bg.js`); for surfaces over images/gradients/blur — which have no
// single readable colour — pass an explicit `background` (a hex string or a
// `() => hex` you sample yourself).

import { applyTheme } from "./apply-theme.js";
import { effectiveBackground } from "./effective-bg.js";

/**
 * @typedef {object} WatchController
 * @property {(force?: boolean) => (object | null)} refresh  Re-resolve+apply if the
 *   background (or theme) changed; `force` re-applies unconditionally. Returns the
 *   `resolveTheme` result that is now applied, or `null` if nothing was applied.
 * @property {(theme: string) => void} setTheme  Switch theme and re-apply.
 * @property {() => string} background  The effective background hex last resolved.
 * @property {() => void} stop  Disconnect observers and stop watching.
 */

/**
 * Keep `element`'s `--lab-*` variables in sync with its (effective) background.
 *
 * @param {*} element  The surface to read the background from and (by default)
 *   write the variables onto.
 * @param {object} options
 * @param {{ resolveTheme: (bgHex: string, theme: string) => object }} options.colors
 *   An initialised `LabColors` engine (already `await init()`-ed).
 * @param {string} options.theme  Theme name (`"light" | "dark" | …`).
 * @param {string | (() => string)} [options.background]  Explicit effective
 *   background, overriding the ancestor-composite (use for image/gradient/blur
 *   surfaces you sample yourself).
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

  // Apply immediately so the surface is correct on creation.
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
