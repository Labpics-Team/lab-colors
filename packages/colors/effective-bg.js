// Effective background resolution — zero dependencies.
//
// `labcolors` resolves roles against a *solid* background. A real UI surface is
// often translucent (a panel at `rgba(…, .8)` over its parents) or has no
// background of its own (inheriting whatever is behind it). To resolve such a
// surface you need a background observation. This legacy helper produces a
// reference estimate for the subset it understands; it is not a browser pixel
// capture or a claim about what a viewer actually sees.
//
// This module computes it by walking the ancestor chain and **alpha-compositing**
// each element's `background-color` (front-to-back) until the stack is opaque,
// over an opaque fallback (white by default), yielding one solid reference hex
// the WASM core can consume.
//
// HONEST LIMIT: this composites solid/translucent `background-color` layers only.
// It does NOT sample `background-image`s, gradients, blurred backdrops, video, or
// content showing through — those have no single colour to read from computed
// style. For those, the caller supplies the effective background explicitly (the
// `background` option of `watchTheme`, or a sampled average). What it does cover —
// translucent panels over solid parents — is the common case and is composited
// *correctly* (true source-over alpha), not approximated.
//
// COLOUR FORMS: `parseCssColor` reads the forms this package actually meets —
// `#hex`, `rgb()/rgba()` (legacy comma and modern space/slash), `transparent`,
// and `oklch()` (the engine's OWN emission form since 0.4.0, and what a browser
// serialises `background-color` back to for an oklch-painted surface). Other
// modern forms — `lab()`, `lch()`, `color(srgb …)`, `color-mix()`, `hsl()`,
// named colours beyond `transparent` — are NOT parsed and currently become a
// dropped layer for compatibility. That is not safe evidence: pass the
// background explicitly. Issue #283 owns the typed `Unknown` replacement for
// unsupported syntax, missing opaque bases and incomplete traversal.

/** @typedef {[number, number, number, number]} Rgba  r,g,b in 0..255, a in 0..1 */

/**
 * Parse a CSS colour string into `[r, g, b, a]`, or `null` if unrecognised.
 *
 * Handles the forms computed style actually yields:
 *   - `rgb(r, g, b)` / `rgba(r, g, b, a)` and the modern `rgb(r g b / a)`;
 *   - `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa`;
 *   - the `transparent` keyword;
 *   - `oklch(L C H)` / `oklch(L C H / A)` — the engine's own emission form and
 *     what a browser serialises an oklch-painted `background-color` back to.
 *     `L` accepts both a `0..1` number (Chrome's computed form) and a percentage
 *     (the engine's literal form); `C` a number or a `%` (100% = 0.4 per CSS
 *     Color 4); `H` degrees (bare or `deg`-suffixed); a missing component
 *     (`none`) is 0. Conversion to sRGB bytes reuses the file's Oklab↔sRGB
 *     transform and is byte-exact to the core's round-trip proof.
 *
 * Any other form — `lab()`, `lch()`, `color(srgb …)`, `color-mix()`, `hsl()`,
 * named colours other than `transparent` — returns `null` (treated as "no
 * layer"); supply such backgrounds explicitly via the `background` option.
 *
 * @param {string} css
 * @returns {Rgba | null}
 */
export function parseCssColor(css) {
  if (typeof css !== "string") return null;
  const s = css.trim().toLowerCase();
  if (s === "transparent") return [0, 0, 0, 0];

  if (s.startsWith("oklch(") && s.endsWith(")")) return parseOklch(s.slice(6, -1));

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
 * Parse the inside of an `oklch(...)` (the part between the parens) into sRGB
 * `[r, g, b, a]`, or `null` on any malformed component.
 *
 * The engine emits `oklch(L% C H)` / `oklch(L% C H / A)`; a browser's computed
 * form is `oklch(<L 0..1> C H [/ A])`. Components are whitespace-separated with
 * an optional `/`-separated alpha, exactly like `rgb()`'s modern syntax.
 * oklch → Oklab is `a = C·cos(H)`, `b = C·sin(H)`; then the file's own
 * `oklabToLinearRgb` + `linearToSrgb` land it in sRGB — the SAME transform (and
 * the SAME clamp-then-round the core uses in `hex_from_srgb`) that carries the
 * emitter's byte-exact round-trip, so an emitted string decodes to its source
 * bytes. Out-of-gamut channels clamp per channel, matching `oklabLerp`/`toHex`.
 *
 * @param {string} inner  the text between `oklch(` and `)`
 * @returns {Rgba | null}
 */
function parseOklch(inner) {
  const slash = inner.indexOf("/");
  const lch = (slash >= 0 ? inner.slice(0, slash) : inner).trim();
  const alphaTok = slash >= 0 ? inner.slice(slash + 1).trim() : null;
  const comps = lch.split(/\s+/).filter((p) => p.length > 0);
  if (comps.length !== 3) return null;

  const L = oklchLightness(comps[0]);
  const C = oklchChroma(comps[1]);
  const H = oklchHue(comps[2]);
  const a = alphaTok === null ? 1 : oklchAlpha(alphaTok);
  if (L === null || C === null || H === null || a === null) return null;

  const hRad = (H * Math.PI) / 180;
  const lin = oklabToLinearRgb(L, C * Math.cos(hRad), C * Math.sin(hRad));
  const byte = (i) => Math.round(clamp255(linearToSrgb(lin[i]) * 255));
  return [byte(0), byte(1), byte(2), a];
}

/** Strict CSS `<number>` (no trailing junk, unlike `parseFloat`), else `null`. */
function cssNumber(tok) {
  return /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(tok) ? parseFloat(tok) : null;
}

/** L: a percentage → `/100` into `0..1`; a bare number is already `0..1`; `none`
 * → 0. Lightness clamps to `[0, 1]` (CSS Color 4) — a byte-level clamp alone
 * masks out-of-range L only at low chroma, not high, so clamp L explicitly. */
function oklchLightness(tok) {
  if (tok === "none") return 0;
  const pct = tok.endsWith("%");
  const n = cssNumber(pct ? tok.slice(0, -1) : tok);
  if (n === null) return null;
  return Math.min(1, Math.max(0, pct ? n / 100 : n));
}

/** C: a bare number is absolute chroma; a percentage is a fraction of 0.4
 * (CSS Color 4: 100% = 0.4); `none` → 0. Negative chroma clamps to 0. */
function oklchChroma(tok) {
  if (tok === "none") return 0;
  const pct = tok.endsWith("%");
  const n = cssNumber(pct ? tok.slice(0, -1) : tok);
  if (n === null) return null;
  return Math.max(0, pct ? (n / 100) * 0.4 : n);
}

/** H: degrees, bare or `deg`-suffixed; `none` → 0. (grad/rad/turn are out of
 * scope — the engine and browsers emit bare degrees.) */
function oklchHue(tok) {
  if (tok === "none") return 0;
  return cssNumber(tok.endsWith("deg") ? tok.slice(0, -3) : tok);
}

/** Alpha: a number `0..1` or a percentage; `none` → 0; clamped to `[0, 1]`. */
function oklchAlpha(tok) {
  if (tok === "none") return 0;
  const pct = tok.endsWith("%");
  const n = cssNumber(pct ? tok.slice(0, -1) : tok);
  if (n === null) return null;
  return Math.min(1, Math.max(0, pct ? n / 100 : n));
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
  // Affine-форма математически равна expanded source-over и фиксирует
  // объявленный byte-scale binary64 operation order. Это numerical profile,
  // не утверждение глобальной монотонности: округление может менять локальный
  // порядок соседних значений, а legacy WCAG EOTF дополнительно имеет seam.
  const a = ab + at * (1 - ab);
  if (a === 0) return [0, 0, 0, 0];
  const c = (i) => {
    const bottomPremultiplied = bottom[i] * ab;
    return (bottomPremultiplied + at * (top[i] - bottomPremultiplied)) / a;
  };
  return [c(0), c(1), c(2), a];
}

/**
 * `[r, g, b]` (0..255) → `#RRGGBB`, channels rounded and clamped.
 *
 * @param {Rgba | [number, number, number]} rgb
 * @returns {string}
 */
export function toHex(rgb) {
  // Coerce non-finite channels (NaN/±Infinity, e.g. from a malformed `Rgba`
  // passed by a caller) to 0, so a bad input yields a valid `#RRGGBB` rather
  // than an invalid CSS string like `"#NAN0000"`.
  const h = (v) => {
    const n = Math.round(clamp255(Number.isFinite(v) ? v : 0));
    return n.toString(16).padStart(2, "0");
  };
  return `#${h(rgb[0])}${h(rgb[1])}${h(rgb[2])}`.toUpperCase();
}

// --- Perceptual interpolation (Oklab) -------------------------------------
//
// A crossfade should feel even: equal progress should be equal *perceived*
// change. A straight sRGB-channel blend is not that — it lingers in the brighter
// half (black→white at t=0.5 is #808080, but the perceived midpoint grey is
// ~#606060). Interpolating in Oklab — a perceptually-uniform space — fixes the
// timing and, for chromatic endpoints, avoids the muddy desaturated midpoint a
// raw RGB blend produces. Björn Ottosson's sRGB↔Oklab transform, self-contained.

/** sRGB gamma transfer (IEC 61966-2-1): encoded channel 0..1 → linear 0..1. */
function srgbToLinear(c) {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** Inverse sRGB transfer: linear 0..1 → encoded 0..1. */
function linearToSrgb(c) {
  return c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055;
}

/** Linear-light sRGB `[r,g,b]` (0..1) → Oklab `[L, a, b]`. */
function linearRgbToOklab(r, g, b) {
  const l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
  const m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
  const s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
  const l_ = Math.cbrt(l);
  const m_ = Math.cbrt(m);
  const s_ = Math.cbrt(s);
  return [
    0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_,
    1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_,
    0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_,
  ];
}

/** Oklab `[L, a, b]` → linear-light sRGB `[r,g,b]` (0..1, may be out of gamut). */
function oklabToLinearRgb(L, A, B) {
  const l_ = L + 0.3963377774 * A + 0.2158037573 * B;
  const m_ = L - 0.1055613458 * A - 0.0638541728 * B;
  const s_ = L - 0.0894841775 * A - 1.291485548 * B;
  const l = l_ * l_ * l_;
  const m = m_ * m_ * m_;
  const s = s_ * s_ * s_;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/**
 * Interpolate two colours in Oklab at `t ∈ [0,1]`, returning `#RRGGBB`.
 *
 * `from`/`to` may be ANY string `parseCssColor` accepts (`#rgb`/`#rrggbb`,
 * `rgb()`/`rgba()`, `oklch()`, `transparent`) — not only `#RRGGBB`. Perceptually
 * uniform: equal steps in `t` are equal steps in perceived lightness (and a
 * straight, non-muddy path in hue/chroma), so a crossfade feels even rather than
 * lingering bright. Endpoints are returned exactly (`t ≤ 0` → `from`, `t ≥ 1` →
 * `to`), always normalised to `#RRGGBB` through `toHex`; out-of-gamut
 * intermediates are clamped per channel. Unparseable input falls back to the
 * nearer parseable endpoint.
 *
 * @param {string} from  any colour string `parseCssColor` accepts
 * @param {string} to    any colour string `parseCssColor` accepts
 * @param {number} t
 * @returns {string} a `#RRGGBB` string
 */
export function oklabLerp(from, to, t) {
  const a = parseCssColor(from);
  const b = parseCssColor(to);
  if (!a || !b) return (b && t >= 0.5) || !a ? (b ? toHex(b) : "#000000") : toHex(a);
  if (t <= 0) return toHex(a);
  if (t >= 1) return toHex(b);
  const la = linearRgbToOklab(srgbToLinear(a[0] / 255), srgbToLinear(a[1] / 255), srgbToLinear(a[2] / 255));
  const lb = linearRgbToOklab(srgbToLinear(b[0] / 255), srgbToLinear(b[1] / 255), srgbToLinear(b[2] / 255));
  const lin = oklabToLinearRgb(
    la[0] + (lb[0] - la[0]) * t,
    la[1] + (lb[1] - la[1]) * t,
    la[2] + (lb[2] - la[2]) * t,
  );
  return toHex([linearToSrgb(lin[0]) * 255, linearToSrgb(lin[1]) * 255, linearToSrgb(lin[2]) * 255]);
}

// --- Compiled hot-path forms (package-internal) -----------------------------
//
// `adaptTheme` interpolates the SAME from/to pair on every frame of an ease,
// and strict mode re-derives WCAG luminance from the interpolated colour up to
// 14× per floored role per frame (`floorBlend`'s bisection). Doing that
// through the string API means re-parsing both endpoints and round-tripping
// through `#RRGGBB` text on every call — measured at ~85-90% of the ease-frame
// budget (bench/hotpath.bench.mjs). These helpers compile a pair once and then
// produce results BYTE-IDENTICAL to their string-path equivalents:
//
//   · `lerpPairHex(pair, t)`       ≡ `oklabLerp(from, to, t)`
//   · `lerpPairLuminance(pair, t)` ≡ WCAG luminance of `oklabLerp(from, to, t)`
//   · `wcagLuminanceCached(css)`   ≡ luminance of `parseCssColor(css) ?? black`
//
// (locked by test/hotpath-parity.test.mjs on randomised inputs). They are
// consumed by `adapt-theme.js` and are NOT part of the public package surface
// (`index.js` does not re-export them).

const PARSE_CACHE_CAP = 256;
const parseCache = new Map();

/** `parseCssColor` behind a small bounded memo, for per-frame callers feeding
 *  it recurring strings (computed-style values, backdrop samples, ease
 *  endpoints). The cap is a blunt bound, not an LRU: a full cache is simply
 *  cleared and refills within a frame — cheaper than eviction bookkeeping for
 *  a working set that is a handful of strings. The cached arrays are SHARED —
 *  package-internal callers must treat them as immutable. (The public
 *  `parseCssColor` stays unmemoized and returns a fresh array per call.) */
export function parseCssColorCached(css) {
  let hit = parseCache.get(css);
  if (hit === undefined) {
    hit = parseCssColor(css);
    if (parseCache.size >= PARSE_CACHE_CAP) parseCache.clear();
    parseCache.set(css, hit);
  }
  return hit;
}

/** Relative luminance of r,g,b channels (0..255) in the frozen original WCAG
 *  2.1 (2018) profile: 0.03928 / 12.92 / 2.4, matching `adapt-theme`. */
function wcagLumChannels(r, g, b) {
  const lin = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

const LUM_CACHE_CAP = 256;
const lumCache = new Map();

/** WCAG relative luminance of any colour string, memoised. Byte-equal to the
 *  string path `adapt-theme` used per sample per frame: luminance of
 *  `parseCssColor(css) ?? [0,0,0,1]` — i.e. unparseable input yields the
 *  luminance of black, preserving the historical fallback. */
export function wcagLuminanceCached(css) {
  let lum = lumCache.get(css);
  if (lum === undefined) {
    const c = parseCssColorCached(css) ?? [0, 0, 0, 1];
    lum = wcagLumChannels(c[0], c[1], c[2]);
    if (lumCache.size >= LUM_CACHE_CAP) lumCache.clear();
    lumCache.set(css, lum);
  }
  return lum;
}

/** Round a channel to the exact byte `toHex` would emit (non-finite → 0,
 *  clamp, round) — keeps the numeric luminance path quantisation-identical to
 *  the `#RRGGBB` round-trip it replaces. */
function hexByte(v) {
  return Math.round(clamp255(Number.isFinite(v) ? v : 0));
}

/**
 * Compile a from/to colour pair for repeated interpolation. Returns `null`
 * when either endpoint fails to parse — callers fall back to `oklabLerp`,
 * which owns the unparseable-endpoint fallback semantics. The pair carries
 * both endpoints' Oklab coordinates plus their exact `toHex` forms, so the
 * per-frame work is one Oklab lerp + gamut map — no string parsing at all.
 *
 * @param {string} from  any colour string `parseCssColor` accepts
 * @param {string} to    any colour string `parseCssColor` accepts
 * @returns {{la:number[],lb:number[],aHex:string,bHex:string,aBytes:number[],bBytes:number[]} | null}
 */
export function compileLerpPair(from, to) {
  const a = parseCssColorCached(from);
  const b = parseCssColorCached(to);
  if (!a || !b) return null;
  return {
    la: linearRgbToOklab(srgbToLinear(a[0] / 255), srgbToLinear(a[1] / 255), srgbToLinear(a[2] / 255)),
    lb: linearRgbToOklab(srgbToLinear(b[0] / 255), srgbToLinear(b[1] / 255), srgbToLinear(b[2] / 255)),
    aHex: toHex(a),
    bHex: toHex(b),
    aBytes: [hexByte(a[0]), hexByte(a[1]), hexByte(a[2])],
    bBytes: [hexByte(b[0]), hexByte(b[1]), hexByte(b[2])],
  };
}

/** Byte-identical to `oklabLerp(from, to, t)` for the pair's endpoints — the
 *  same double-precision Oklab lerp on the same parsed channels, minus the
 *  per-call re-parse. */
export function lerpPairHex(pair, t) {
  if (t <= 0) return pair.aHex;
  if (t >= 1) return pair.bHex;
  const la = pair.la;
  const lb = pair.lb;
  const lin = oklabToLinearRgb(
    la[0] + (lb[0] - la[0]) * t,
    la[1] + (lb[1] - la[1]) * t,
    la[2] + (lb[2] - la[2]) * t,
  );
  return toHex([linearToSrgb(lin[0]) * 255, linearToSrgb(lin[1]) * 255, linearToSrgb(lin[2]) * 255]);
}

/** Relative luminance of `lerpPairHex(pair, t)` in the frozen legacy WCAG 2.1
 *  (2018) profile, WITHOUT the `#RRGGBB` round-trip: channels are quantised to
 *  the exact bytes `toHex` would emit, then fed to the same profile formula.
 *  This preserves value parity; it does not prove bisection monotonicity. */
export function lerpPairLuminance(pair, t) {
  if (t <= 0) return wcagLumChannels(pair.aBytes[0], pair.aBytes[1], pair.aBytes[2]);
  if (t >= 1) return wcagLumChannels(pair.bBytes[0], pair.bBytes[1], pair.bBytes[2]);
  const la = pair.la;
  const lb = pair.lb;
  const lin = oklabToLinearRgb(
    la[0] + (lb[0] - la[0]) * t,
    la[1] + (lb[1] - la[1]) * t,
    la[2] + (lb[2] - la[2]) * t,
  );
  return wcagLumChannels(
    hexByte(linearToSrgb(lin[0]) * 255),
    hexByte(linearToSrgb(lin[1]) * 255),
    hexByte(linearToSrgb(lin[2]) * 255),
  );
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
 * Legacy opaque reference-background estimate for the supported
 * `background-color` subset. This is not a browser pixel observation.
 *
 * Walks from `element` upward, collecting each `background-color` layer, and
 * stops at the first fully-opaque layer (which becomes the base). If the chain
 * reaches the root without an opaque layer, `fallback` (default white) is used
 * as an explicit compatibility assumption; it is not evidence of the canvas.
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
