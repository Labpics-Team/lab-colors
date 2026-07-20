// Package-internal CSS colour parsing and Oklab interpolation.
// Browser background observation lives exclusively in background-observation.js;
// this module no longer walks host state or owns point composition.

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
    if (!/^[0-9a-f]+$/u.test(h)) return null;
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
  const body = m[1].trim();
  let channels;
  let alphaToken = null;
  if (body.includes(",")) {
    if (body.includes("/")) return null;
    const parts = body.split(",").map((part) => part.trim());
    if (parts.some((part) => part.length === 0) || (parts.length !== 3 && parts.length !== 4)) {
      return null;
    }
    channels = parts.slice(0, 3);
    if (parts.length === 4) alphaToken = parts[3];
  } else {
    const slash = body.split("/").map((part) => part.trim());
    if (slash.length > 2 || slash.some((part) => part.length === 0)) return null;
    channels = slash[0].split(/\s+/u);
    if (channels.length !== 3) return null;
    if (slash.length === 2) {
      const alphaParts = slash[1].split(/\s+/u);
      if (alphaParts.length !== 1) return null;
      alphaToken = alphaParts[0];
    }
  }
  const chan = (p) => {
    const pct = p.endsWith("%");
    const value = cssNumber(pct ? p.slice(0, -1) : p);
    return value === null ? null : pct ? (value / 100) * 255 : value;
  };
  const r = chan(channels[0]);
  const g = chan(channels[1]);
  const b = chan(channels[2]);
  const a = alphaToken === null ? 1 : oklchAlpha(alphaToken);
  if ([r, g, b, a].some((value) => value === null)) return null;
  return [clamp255(r), clamp255(g), clamp255(b), Math.min(1, Math.max(0, a))];
}

function clamp255(v) {
  return Math.min(255, Math.max(0, v));
}

/**
 * Преобразует содержимое `oklch(...)` в sRGB `[r, g, b, a]`; невалидный
 * компонент даёт `null`.
 *
 * Движок эмитирует `oklch(L% C H)` / `oklch(L% C H / A)`, а computed style
 * браузера может вернуть `oklch(<L 0..1> C H [/ A])`. После oklch → Oklab
 * используются те же `oklabToLinearRgb` и `linearToSrgb`, что в локальном
 * sRGB-пути. Поканальный clamp перед округлением совпадает с законом финальной
 * sRGB8-эмиссии Core и сохраняет побайтовый round-trip; out-of-gamut каналы
 * ограничиваются независимо, как в `oklabLerp`/`toHex`.
 *
 * @param {string} inner текст между `oklch(` и `)`
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
  if (lin.some((channel) => !Number.isFinite(channel))) return null;
  const encoded = lin.map((channel) => linearToSrgb(channel) * 255);
  if (encoded.some((channel) => !Number.isFinite(channel))) return null;
  return [
    Math.round(clamp255(encoded[0])),
    Math.round(clamp255(encoded[1])),
    Math.round(clamp255(encoded[2])),
    a,
  ];
}

/** Strict CSS `<number>` (no trailing junk, unlike `parseFloat`), else `null`. */
function cssNumber(tok) {
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(tok)) return null;
  const value = Number(tok);
  return Number.isFinite(value) ? value : null;
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

// --- Oklab-coordinate interpolation ---------------------------------------
//
// The helper linearly interpolates Oklab coordinates, converts the intermediate
// value to sRGB, clamps channels and emits an opaque byte hex. This describes the
// numeric path only; it is not a guarantee about perceived timing or cleanliness.

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
 * `rgb()`/`rgba()`, `oklch()`, `transparent`) — not only `#RRGGBB`. Output is
 * always opaque: input alpha is discarded. At `t ≤ 0`/`t ≥ 1`, the selected
 * endpoint's RGB channels are normalized through `toHex`; intermediate
 * out-of-gamut channels are clamped. Unparseable input falls back to the nearer
 * parseable endpoint.
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
// `adaptTheme` interpolates the SAME from/to pair on every frame of an ease.
// The string API would re-parse both endpoints on every call. These helpers
// compile a pair once and then produce results BYTE-IDENTICAL to the string
// path:
//
//   · `lerpPairHex(pair, t)` ≡ `oklabLerp(from, to, t)`
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
 *  a working set that is a handful of strings. Запись кэша не покидает модуль:
 *  `compileLerpPair` сразу преобразует её в новый объект с собственными
 *  массивами, поэтому cache hit не требует защитной аллокации. */
function parseCssColorCached(css) {
  let hit = parseCache.get(css);
  if (hit === undefined) {
    hit = parseCssColor(css);
    if (parseCache.size >= PARSE_CACHE_CAP) parseCache.clear();
    parseCache.set(css, hit);
  }
  return hit;
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
 * @returns {{la:number[],lb:number[],aHex:string,bHex:string} | null}
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
