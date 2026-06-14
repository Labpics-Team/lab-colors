// Effective background resolution — zero dependencies.
//
// `labcolors` resolves roles against a *solid* background. A real UI surface is
// often translucent (a panel at `rgba(…, .8)` over its parents) or has no
// background of its own (inheriting whatever is behind it). To resolve such a
// surface honestly you need the **effective** background: the opaque colour a
// viewer actually sees behind the element's own content.
//
// This module computes it by walking the ancestor chain and **alpha-compositing**
// each element's `background-color` (front-to-back) until the stack is opaque,
// over an opaque fallback (white by default). It is the bridge from the DOM's
// layered, translucent reality to the single solid hex the WASM core consumes.
//
// HONEST LIMIT: this composites solid/translucent `background-color` layers only.
// It does NOT sample `background-image`s, gradients, blurred backdrops, video, or
// content showing through — those have no single colour to read from computed
// style. For those, the caller supplies the effective background explicitly (the
// `background` option of `watchTheme`, or a sampled average). What it does cover —
// translucent panels over solid parents — is the common case and is composited
// *correctly* (true source-over alpha), not approximated.

/** @typedef {[number, number, number, number]} Rgba  r,g,b in 0..255, a in 0..1 */

/**
 * Parse a CSS colour string into `[r, g, b, a]`, or `null` if unrecognised.
 *
 * Handles the forms computed style actually yields (`rgb(r, g, b)`,
 * `rgba(r, g, b, a)`, the modern `rgb(r g b / a)`) plus `#rgb`/`#rrggbb` and the
 * `transparent` keyword. Unknown keywords return `null` (treated as "no layer").
 *
 * @param {string} css
 * @returns {Rgba | null}
 */
export function parseCssColor(css) {
  if (typeof css !== "string") return null;
  const s = css.trim().toLowerCase();
  if (s === "transparent") return [0, 0, 0, 0];

  if (s[0] === "#") {
    const h = s.slice(1);
    if (h.length === 3 || h.length === 4) {
      const r = parseInt(h[0] + h[0], 16);
      const g = parseInt(h[1] + h[1], 16);
      const b = parseInt(h[2] + h[2], 16);
      const a = h.length === 4 ? parseInt(h[3] + h[3], 16) / 255 : 1;
      return [r, g, b, a].some(Number.isNaN) ? null : [r, g, b, a];
    }
    if (h.length === 6 || h.length === 8) {
      const r = parseInt(h.slice(0, 2), 16);
      const g = parseInt(h.slice(2, 4), 16);
      const b = parseInt(h.slice(4, 6), 16);
      const a = h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1;
      return [r, g, b, a].some(Number.isNaN) ? null : [r, g, b, a];
    }
    return null;
  }

  const m = s.match(/^rgba?\(([^)]+)\)$/);
  if (!m) return null;
  // Split on commas or whitespace and an optional "/" alpha separator.
  const parts = m[1].split(/[,\s/]+/).filter((p) => p.length > 0);
  if (parts.length < 3) return null;
  const chan = (p) => (p.endsWith("%") ? (parseFloat(p) / 100) * 255 : parseFloat(p));
  const r = chan(parts[0]);
  const g = chan(parts[1]);
  const b = chan(parts[2]);
  const a = parts.length >= 4 ? (parts[3].endsWith("%") ? parseFloat(parts[3]) / 100 : parseFloat(parts[3])) : 1;
  if ([r, g, b, a].some((v) => Number.isNaN(v))) return null;
  return [clamp255(r), clamp255(g), clamp255(b), Math.min(1, Math.max(0, a))];
}

function clamp255(v) {
  return Math.min(255, Math.max(0, v));
}

/**
 * Source-over composite of `top` onto `bottom` (Porter-Duff "over").
 *
 * @param {Rgba} top
 * @param {Rgba} bottom
 * @returns {Rgba}
 */
export function compositeOver(top, bottom) {
  const at = top[3];
  const ab = bottom[3];
  const a = at + ab * (1 - at);
  if (a === 0) return [0, 0, 0, 0];
  const c = (i) => (top[i] * at + bottom[i] * ab * (1 - at)) / a;
  return [c(0), c(1), c(2), a];
}

/**
 * `[r, g, b]` (0..255) → `#RRGGBB`, channels rounded and clamped.
 *
 * @param {Rgba | [number, number, number]} rgb
 * @returns {string}
 */
export function toHex(rgb) {
  const h = (v) => Math.round(clamp255(v)).toString(16).padStart(2, "0");
  return `#${h(rgb[0])}${h(rgb[1])}${h(rgb[2])}`.toUpperCase();
}

/**
 * Compose an ordered stack of colour layers (front-to-back) over an opaque base
 * into a single opaque `#RRGGBB`. Pure — no DOM. Exposed for testing and for
 * callers that sample their own layers.
 *
 * @param {Rgba[]} layersFrontToBack  index 0 is the topmost layer
 * @param {Rgba} opaqueBase  must have alpha 1
 * @returns {string}
 */
export function compositeStackToHex(layersFrontToBack, opaqueBase) {
  let result = opaqueBase;
  // Apply from the back (closest to base) forward, so index 0 lands on top.
  for (let i = layersFrontToBack.length - 1; i >= 0; i--) {
    result = compositeOver(layersFrontToBack[i], result);
  }
  return toHex(result);
}

/**
 * The opaque effective background `#RRGGBB` visible behind `element`'s own
 * content, by walking ancestors and compositing their `background-color`s.
 *
 * Walks from `element` upward, collecting each `background-color` layer, and
 * stops at the first fully-opaque layer (which becomes the base). If the chain
 * reaches the root without an opaque layer, `fallback` (default white) is the
 * base — matching how a browser shows the page's default canvas.
 *
 * Pure and injectable: pass `getStyle` and `parentOf` to test without a DOM; in
 * the browser they default to `getComputedStyle` and `el.parentElement`.
 *
 * @param {*} element
 * @param {object} [opts]
 * @param {string} [opts.fallback="#FFFFFF"]  base when the chain is fully translucent
 * @param {(el: *) => { getPropertyValue: (p: string) => string }} [opts.getStyle]
 * @param {(el: *) => *} [opts.parentOf]
 * @param {number} [opts.maxDepth=64]  guard against detached/cyclic chains
 * @returns {string}
 */
export function effectiveBackground(element, opts = {}) {
  const fallback = opts.fallback ?? "#FFFFFF";
  const getStyle =
    opts.getStyle ?? ((el) => (typeof getComputedStyle === "function" ? getComputedStyle(el) : { getPropertyValue: () => "" }));
  const parentOf = opts.parentOf ?? ((el) => el.parentElement);
  const maxDepth = opts.maxDepth ?? 64;

  /** @type {Rgba[]} */
  const layers = [];
  let el = element;
  let depth = 0;
  let base = parseCssColor(fallback) ?? [255, 255, 255, 1];

  while (el && depth < maxDepth) {
    const css = getStyle(el).getPropertyValue("background-color");
    const c = parseCssColor(css);
    if (c && c[3] > 0) {
      if (c[3] >= 1) {
        base = c; // first opaque layer is the base; nothing behind it shows
        break;
      }
      layers.push(c);
    }
    el = parentOf(el);
    depth++;
  }

  return compositeStackToHex(layers, base);
}
