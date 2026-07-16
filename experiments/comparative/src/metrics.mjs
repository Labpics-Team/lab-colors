// Предрегистрированные метрики M1-M4 (M5 = те же метрики на подмножестве зоны,
// применяется в run.mjs). Определения: docs/comparative-experiments.md.
import {
  circularHueDist,
  hexToOklch,
  maxChroma,
  median,
  relativeLuminance,
  wcagContrast,
} from "./color.mjs";

/** M1-A: слепой протокол — все пары шагов с |Δpos| >= 0.5. */
export function m1aBlind(ladder) {
  let pairs = 0;
  let ge45 = 0;
  let ge30 = 0;
  for (let i = 0; i < ladder.length; i++) {
    for (let j = i + 1; j < ladder.length; j++) {
      // eps компенсирует двоичную погрешность позиций (0.6 - 0.1 < 0.5 в IEEE 754)
      if (Math.abs(ladder[j].pos - ladder[i].pos) < 0.5 - 1e-9) continue;
      pairs++;
      const c = wcagContrast(ladder[i].hex, ladder[j].hex);
      if (c >= 4.5) ge45++;
      if (c >= 3.0) ge30++;
    }
  }
  return { pairs, ge45, ge30 };
}

/** M2: циркулярный |Δh| к референсному hue; шаги с C < 0.02 исключаются. */
export function hueDrift(ladder, refHue) {
  const ds = [];
  for (const step of ladder) {
    const { C, h } = hexToOklch(step.hex);
    if (C < 0.02) continue;
    ds.push(circularHueDist(h, refHue));
  }
  if (ds.length === 0) return null;
  return {
    counted: ds.length,
    mean: ds.reduce((a, x) => a + x, 0) / ds.length,
    max: Math.max(...ds),
  };
}

/** M3: медиана C / C_max(L, h) по шагам с L в [0.35, 0.75]. Дескриптивная. */
export function chromaUtilization(ladder) {
  const us = [];
  for (const step of ladder) {
    const { L, C, h } = hexToOklch(step.hex);
    if (L < 0.35 || L > 0.75) continue;
    const cm = maxChroma(L, h);
    if (cm < 1e-4) continue;
    us.push(Math.min(C / cm, 1));
  }
  if (us.length === 0) return null;
  return { counted: us.length, median: median(us) };
}

/** M4: Y (WCAG) обязана строго убывать вдоль light->dark; нарушение: Y(i+1) >= Y(i) - eps. */
export function monotonicity(ladder, eps = 1e-6) {
  const ys = ladder.map((s) => relativeLuminance(s.hex));
  let violations = 0;
  for (let i = 0; i + 1 < ys.length; i++) {
    if (ys[i + 1] >= ys[i] - eps) violations++;
  }
  return { violations };
}

/** M1-B для S1: доля (роль x фон), достигших своего floorRatio (по арбитру), + флаги честности. */
export function s1Native(native) {
  let total = 0;
  let achieved = 0;
  let flagged = 0;
  for (const { bgHex, roles } of native) {
    for (const r of roles) {
      total++;
      if (wcagContrast(r.hex, bgHex) >= r.floor) achieved++;
      if (r.flags.length > 0) flagged++;
    }
  }
  return { total, achieved, flagged };
}

/** M1-B для S2: ΔTone >= 50 -> заявка >= 4.5:1; ΔTone >= 40 -> заявка >= 3.0:1. */
export function s2Native(ladder) {
  const out = { claim45: { pairs: 0, pass: 0 }, claim30: { pairs: 0, pass: 0 } };
  for (let i = 0; i < ladder.length; i++) {
    for (let j = i + 1; j < ladder.length; j++) {
      const dT = Math.abs(ladder[i].tone - ladder[j].tone);
      if (dT < 40) continue;
      const c = wcagContrast(ladder[i].hex, ladder[j].hex);
      if (dT >= 50) {
        out.claim45.pairs++;
        if (c >= 4.5) out.claim45.pass++;
      }
      out.claim30.pairs++;
      if (c >= 3.0) out.claim30.pass++;
    }
  }
  return out;
}

const S4_PAIRS = [
  [11, 1],
  [11, 2],
  [12, 1],
  [12, 2],
  [12, 3],
];

/** M1-B для S4: текстовые шаги 11/12 на фонах 1-3, порог 4.5:1. */
export function s4Native(ladder) {
  const by = new Map(ladder.map((s) => [s.step, s.hex]));
  let pairs = 0;
  let pass = 0;
  for (const [fg, bg] of S4_PAIRS) {
    pairs++;
    if (wcagContrast(by.get(fg), by.get(bg)) >= 4.5) pass++;
  }
  return { pairs, pass };
}
