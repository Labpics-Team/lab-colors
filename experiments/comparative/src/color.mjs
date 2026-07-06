// Арбитр: независимая судейская математика сравнительных экспериментов (#45).
// sRGB <-> OKLab/OKLCh по Björn Ottosson (https://bottosson.github.io/posts/oklab/),
// относительная яркость и контраст по WCAG 2.1.
// Намеренно НЕ использует ни @labpics/colors, ни @material/material-color-utilities,
// ни @radix-ui/colors — см. docs/comparative-experiments.md, раздел «Арбитр».

const HEX_RE = /^#([0-9a-fA-F]{6})$/;

export function hexToRgb(hex) {
  const m = HEX_RE.exec(hex);
  if (!m) throw new Error(`не 6-значный hex-цвет: ${hex}`);
  const n = parseInt(m[1], 16);
  return { r: ((n >> 16) & 255) / 255, g: ((n >> 8) & 255) / 255, b: (n & 255) / 255 };
}

const clamp01 = (c) => Math.min(1, Math.max(0, c));

export function rgbToHex({ r, g, b }) {
  const to = (c) => Math.round(clamp01(c) * 255).toString(16).padStart(2, "0");
  return `#${to(r)}${to(g)}${to(b)}`.toUpperCase();
}

const srgbToLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const linearToSrgb = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

// --- WCAG 2.1 ---

export function relativeLuminance(hex) {
  const { r, g, b } = hexToRgb(hex);
  return 0.2126 * srgbToLinear(r) + 0.7152 * srgbToLinear(g) + 0.0722 * srgbToLinear(b);
}

export function wcagContrast(hexA, hexB) {
  const ya = relativeLuminance(hexA);
  const yb = relativeLuminance(hexB);
  const [lo, hi] = ya < yb ? [ya, yb] : [yb, ya];
  return (hi + 0.05) / (lo + 0.05);
}

// --- OKLab / OKLCh (Ottosson) ---

function linearRgbToOklab({ r, g, b }) {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return {
    L: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    a: 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    b: 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  };
}

function oklabToLinearRgb({ L, a, b }) {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return {
    r: 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    g: -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    b: -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  };
}

export function hexToOklab(hex) {
  const { r, g, b } = hexToRgb(hex);
  return linearRgbToOklab({ r: srgbToLinear(r), g: srgbToLinear(g), b: srgbToLinear(b) });
}

export function hexToOklch(hex) {
  const { L, a, b } = hexToOklab(hex);
  const C = Math.hypot(a, b);
  let h = (Math.atan2(b, a) * 180) / Math.PI;
  if (h < 0) h += 360;
  return { L, C, h };
}

function oklchToLinearRgb(L, C, h) {
  const rad = (h * Math.PI) / 180;
  return oklabToLinearRgb({ L, a: C * Math.cos(rad), b: C * Math.sin(rad) });
}

// --- sRGB-гамут ---

export function inGamut(L, C, h, eps = 1e-6) {
  const { r, g, b } = oklchToLinearRgb(L, C, h);
  return r >= -eps && r <= 1 + eps && g >= -eps && g <= 1 + eps && b >= -eps && b <= 1 + eps;
}

/** Максимальная хрома на границе sRGB-гамута при фиксированных (L, h); бинарный поиск. */
export function maxChroma(L, h, precision = 1e-4) {
  if (L <= 0 || L >= 1) return 0;
  if (!inGamut(L, 0, h)) return 0;
  let lo = 0;
  let hi = 0.5;
  while (hi - lo > precision) {
    const mid = (lo + hi) / 2;
    if (inGamut(L, mid, h)) lo = mid;
    else hi = mid;
  }
  return lo;
}

export function gamutMapChroma(L, C, h, precision = 1e-4) {
  return inGamut(L, C, h) ? C : Math.min(C, maxChroma(L, h, precision));
}

/** OKLCh -> hex с поканальным клипом RGB (стратегия наивной рампы S3a). */
export function oklchToHexClipped(L, C, h) {
  const { r, g, b } = oklchToLinearRgb(L, C, h);
  return rgbToHex({
    r: linearToSrgb(clamp01(r)),
    g: linearToSrgb(clamp01(g)),
    b: linearToSrgb(clamp01(b)),
  });
}

/** OKLCh -> hex со снижением хромы до гамута (стратегия S3b и генерации сидов). */
export function oklchToHexGamutMapped(L, C, h) {
  return oklchToHexClipped(L, gamutMapChroma(L, C, h), h);
}

// --- утилиты ---

export function circularHueDist(a, b) {
  const d = Math.abs((((a - b) % 360) + 360) % 360);
  return Math.min(d, 360 - d);
}

export function median(xs) {
  if (xs.length === 0) return null;
  const s = [...xs].sort((x, y) => x - y);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}
