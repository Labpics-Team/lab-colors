// Package-private C8c browser observation adapter.
//
// This adapter admits exactly one physical point from a deliberately narrow
// computed-CSS subset, or returns typed Unknown. It never invents an opaque page
// canvas, drops an unsupported layer/effect, or claims Raster/Field evidence.
// Point composition reuses the Core-owned encoded-sRGB8 source-over operation.

import { __over } from "./pkg/labcolors.js";
import { parseCssColor } from "./effective-bg.js";

const INVALID_RGB24 = 0xFFFFFFFF;
const DEFAULT_MAX_DEPTH = 64;

/** @typedef {[number, number, number, number]} Rgba */
/** @typedef {{kind:"point", hex:string}} PointBackgroundObservation */
/** @typedef {{kind:"unknown", reason:string}} UnknownBackgroundObservation */
/** @typedef {PointBackgroundObservation | UnknownBackgroundObservation} BackgroundObservation */

const point = (hex) => Object.freeze({ kind: "point", hex });
const unknown = (reason) => Object.freeze({ kind: "unknown", reason });

function clamp255(value) {
  return Math.min(255, Math.max(0, value));
}

function packRgb24(rgb) {
  const byte = (value) => Math.round(clamp255(Number.isFinite(value) ? value : 0));
  return ((byte(rgb[0]) << 16) | (byte(rgb[1]) << 8) | byte(rgb[2])) >>> 0;
}

function hexFromRgb24(rgb24) {
  return `#${rgb24.toString(16).padStart(6, "0").toUpperCase()}`;
}

/**
 * Every translucent layer is one physical occurrence, so Core rounds every
 * edge instead of preserving a fractional JS accumulator across the stack.
 *
 * @param {Rgba[]} layersFrontToBack
 * @param {Rgba} opaqueBase
 */
function compositePointStack(layersFrontToBack, opaqueBase) {
  let result = packRgb24(opaqueBase);
  for (let index = layersFrontToBack.length - 1; index >= 0; index--) {
    const layer = layersFrontToBack[index];
    result = __over(packRgb24(layer), layer[3], result);
    if (result === INVALID_RGB24) {
      throw new RangeError("background observation: Core rejected an admitted point layer");
    }
  }
  return hexFromRgb24(result);
}

function admittedCanvas(value) {
  if (value === undefined) return null;
  const canvas = parseCssColor(value);
  if (!canvas || canvas[3] !== 1) {
    throw new RangeError("background observation: canvas must be an opaque supported colour");
  }
  return canvas;
}

function admittedMaxDepth(value) {
  if (value === undefined) return DEFAULT_MAX_DEPTH;
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RangeError("background observation: maxDepth must be a positive safe integer");
  }
  return value;
}

function callCheckpoint(checkpoint, token) {
  if (checkpoint) checkpoint(token);
}

function styleProperty(style, property, checkpoint, token) {
  if (style === null || (typeof style !== "object" && typeof style !== "function")) {
    return null;
  }
  callCheckpoint(checkpoint, token);
  const getPropertyValue = style.getPropertyValue;
  callCheckpoint(checkpoint, token);
  if (typeof getPropertyValue !== "function") return null;
  const value = Function.prototype.call.call(getPropertyValue, style, property);
  callCheckpoint(checkpoint, token);
  return typeof value === "string" ? value.trim().toLowerCase() : null;
}

const EFFECT_PROPERTIES = Object.freeze([
  ["background-image", "none", "background-image"],
  ["background-blend-mode", "normal", "background-blend-mode"],
  ["background-clip", "border-box", "background-clip"],
  ["box-shadow", "none", "box-shadow"],
  ["mix-blend-mode", "normal", "mix-blend-mode"],
  ["filter", "none", "filter"],
  ["backdrop-filter", "none", "backdrop-filter"],
  ["-webkit-backdrop-filter", "none", "backdrop-filter"],
  ["mask-image", "none", "mask-image"],
  ["-webkit-mask-image", "none", "mask-image"],
  ["clip-path", "none", "clip-path"],
]);

function unsupportedEffect(style, checkpoint, token) {
  for (const [property, initial, reason] of EFFECT_PROPERTIES) {
    const value = styleProperty(style, property, checkpoint, token);
    if (value === null) return "unreadable-computed-style";
    // Empty means this property is unavailable on the current engine. An
    // unavailable property cannot contribute that effect on that engine.
    if (value !== "" && value !== initial) return reason;
  }

  const opacity = styleProperty(style, "opacity", checkpoint, token);
  if (opacity === null) return "unreadable-computed-style";
  if (opacity !== "" && (!Number.isFinite(Number(opacity)) || Number(opacity) !== 1)) {
    return "element-opacity";
  }

  const display = styleProperty(style, "display", checkpoint, token);
  if (display === null) return "unreadable-computed-style";
  if (display === "none") return "display-none";
  if (display === "contents") return "display-contents";

  const visibility = styleProperty(style, "visibility", checkpoint, token);
  if (visibility === null) return "unreadable-computed-style";
  if (visibility === "hidden" || visibility === "collapse") return "visibility";

  const contentVisibility = styleProperty(style, "content-visibility", checkpoint, token);
  if (contentVisibility === null) return "unreadable-computed-style";
  if (contentVisibility !== "" && contentVisibility !== "visible") {
    return "content-visibility";
  }

  return null;
}

/**
 * Observe one supported computed-CSS background point.
 *
 * Supported physics is intentionally narrow: uniform `background-color` layers
 * with normal blending, no image/filter/backdrop/mask/clip effect, visible
 * elements, and group opacity 1. A fully translucent root is Unknown unless the
 * caller declares an opaque `canvas`.
 *
 * Finding an opaque colour stops only colour collection. The adapter still walks
 * all remaining ancestors because group opacity/filter/blend on a higher ancestor
 * changes the final rendered point even when backgrounds behind the opaque layer
 * cannot show through.
 *
 * @param {*} element
 * @param {object} [options]
 * @param {string} [options.canvas] caller-declared opaque page canvas
 * @param {(element:*)=>*} [options.getStyle]
 * @param {(element:*)=>*} [options.parentOf]
 * @param {number} [options.maxDepth=64]
 * @param {(token:*)=>void} [options.checkpoint]
 * @param {*} [options.checkpointToken]
 * @returns {BackgroundObservation}
 */
export function observePointBackground(element, options = {}) {
  const canvas = admittedCanvas(options.canvas);
  const maxDepth = admittedMaxDepth(options.maxDepth);
  const getStyle =
    options.getStyle ??
    ((node) =>
      typeof getComputedStyle === "function"
        ? getComputedStyle(node)
        : { getPropertyValue: () => "" });
  const parentOf = options.parentOf ?? ((node) => node.parentElement);
  const checkpoint = options.checkpoint;
  if (typeof getStyle !== "function" || typeof parentOf !== "function") {
    throw new TypeError("background observation: getStyle and parentOf must be functions");
  }
  if (checkpoint !== undefined && typeof checkpoint !== "function") {
    throw new TypeError("background observation: checkpoint must be a function");
  }
  if (element === null || element === undefined) return unknown("missing-element");

  const token = options.checkpointToken;
  /** @type {Rgba[]} */
  const layers = [];
  const seen = new Set();
  let node = element;
  let depth = 0;
  let opaqueBase = null;

  while (node !== null && node !== undefined) {
    if (depth >= maxDepth) return unknown("depth-exhausted");
    if (seen.has(node)) return unknown("ancestor-cycle");
    seen.add(node);

    callCheckpoint(checkpoint, token);
    const style = getStyle(node);
    callCheckpoint(checkpoint, token);
    const effect = unsupportedEffect(style, checkpoint, token);
    if (effect !== null) return unknown(effect);

    if (opaqueBase === null) {
      const css = styleProperty(style, "background-color", checkpoint, token);
      if (css === null) return unknown("unreadable-computed-style");
      const colour = parseCssColor(css);
      if (!colour) return unknown("unsupported-background-color");
      if (colour[3] > 0) {
        if (colour[3] >= 1) opaqueBase = colour;
        else layers.push(colour);
      }
    }

    callCheckpoint(checkpoint, token);
    node = parentOf(node);
    callCheckpoint(checkpoint, token);
    depth++;
  }

  if (opaqueBase === null) {
    if (canvas === null) return unknown("transparent-root");
    opaqueBase = canvas;
  }
  return point(compositePointStack(layers, opaqueBase));
}
