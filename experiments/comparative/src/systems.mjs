// Адаптеры сравниваемых систем. Каждый адаптер получает сид (hex) и возвращает
// лестницу light->dark: [{ hex, pos }] c pos, нормированным в [0,1].
// Операционализация лестниц предрегистрирована в docs/comparative-experiments.md.
import { readFileSync } from "node:fs";
import { argbFromHex, hexFromArgb, TonalPalette } from "@material/material-color-utilities";
import * as radix from "@radix-ui/colors";
import { initSync, LabColors } from "../../../packages/colors/pkg/labcolors.js";
import {
  circularHueDist,
  hexToOklch,
  oklchToHexClipped,
  oklchToHexGamutMapped,
} from "./color.mjs";

const repoRoot = new URL("../../../", import.meta.url);

// --- S1: lab-colors @ HEAD (WASM) ---

initSync({ module: readFileSync(new URL("packages/colors/pkg/labcolors_bg.wasm", repoRoot)) });

const BASE_CONFIG = JSON.parse(
  readFileSync(new URL("crates/labcolors-wasm/tests/data/labui.config.json", repoRoot), "utf8"),
);

/** Свип из 11 нейтральных фонов: OKLCH C=0, L равномерно 0.97 -> 0.12. */
export const BG_SWEEP = Array.from({ length: 11 }, (_, i) => {
  const L = 0.97 - i * 0.085;
  return { L, hex: oklchToHexGamutMapped(L, 0, 0), theme: L >= 0.5 ? "light" : "dark" };
});

const FLAG_RE = /coerced|degraded|compressed|unreachable|clamp/i;

/**
 * S1: сид патчит все четыре brand-якоря конфига labui; лестница — compositeHex
 * роли fill-accent (alpha=1.0, солид бренда) на каждом фоне свипа.
 * native: все солид-роли с legalFloor на каждом фоне (для M1-B).
 */
export function buildS1(seedHex) {
  const cfg = structuredClone(BASE_CONFIG);
  cfg.brand = { light: seedHex, dark: seedHex, light_ic: seedHex, dark_ic: seedHex };
  const engine = new LabColors();
  try {
    engine.loadConfig(JSON.stringify(cfg));
    const ladder = [];
    const native = [];
    for (let i = 0; i < BG_SWEEP.length; i++) {
      const bg = BG_SWEEP[i];
      const t = engine.resolveTheme(bg.hex, bg.theme);
      ladder.push({
        hex: t.roles["fill-accent"].compositeHex.toUpperCase(),
        pos: i / (BG_SWEEP.length - 1),
      });
      const roles = [];
      for (const [key, r] of Object.entries(t.roles)) {
        if (r.kind !== "color" || r.legalFloor == null) continue;
        const flags = Object.entries(r)
          .filter(([k, v]) => v === true && FLAG_RE.test(k))
          .map(([k]) => k)
          .sort();
        roles.push({ key, hex: r.hex.toUpperCase(), floor: r.legalFloor, flags });
      }
      roles.sort((a, b) => (a.key < b.key ? -1 : 1));
      native.push({ bgHex: bg.hex, roles });
    }
    return { ladder, native };
  } finally {
    engine.free?.();
  }
}

// --- S2: Material Color Utilities ---

export const MCU_TONES = [100, 90, 80, 70, 60, 50, 40, 30, 20, 10, 0];

export function buildS2(seedHex) {
  const pal = TonalPalette.fromInt(argbFromHex(seedHex));
  const ladder = MCU_TONES.map((tone, i) => ({
    hex: hexFromArgb(pal.tone(tone)).toUpperCase(),
    pos: i / (MCU_TONES.length - 1),
    tone,
  }));
  return { ladder };
}

// --- S3a / S3b: наивные OKLCH-рампы ---

const RAMP_L = BG_SWEEP.map((b) => b.L);

/** mode: "clip" (S3a, поканальный клип RGB) | "gamut" (S3b, снижение хромы). */
export function buildS3(seedHex, mode) {
  const { C, h } = hexToOklch(seedHex);
  const ladder = RAMP_L.map((L, i) => ({
    hex: mode === "clip" ? oklchToHexClipped(L, C, h) : oklchToHexGamutMapped(L, C, h),
    pos: i / (RAMP_L.length - 1),
  }));
  return { ladder };
}

// --- S4: Radix Colors ---

const RADIX_SCALES = (() => {
  const out = [];
  for (const [name, scale] of Object.entries(radix)) {
    if (!/^[a-z]+$/.test(name)) continue; // без *Dark / *A / *P3
    if (typeof scale !== "object" || scale === null) continue;
    const vals = Object.values(scale);
    if (vals.length !== 12 || !vals.every((v) => /^#[0-9a-fA-F]{6}$/.test(v))) continue;
    const step9 = hexToOklch(vals[8]);
    out.push({ name, steps: vals.map((v) => v.toUpperCase()), step9, chromatic: step9.C >= 0.03 });
  }
  out.sort((a, b) => (a.name < b.name ? -1 : 1));
  return out;
})();

/**
 * S4: сид -> хроматическая шкала Radix с циркулярно ближайшим hue шага 9;
 * сиды с C < 0.03 -> gray. Лестница = шаги 1..12 light-варианта.
 */
export function buildS4(seedHex) {
  const seed = hexToOklch(seedHex);
  let chosen;
  if (seed.C < 0.03) {
    chosen = RADIX_SCALES.find((s) => s.name === "gray");
  } else {
    let best = Infinity;
    for (const s of RADIX_SCALES) {
      if (!s.chromatic) continue;
      const d = circularHueDist(seed.h, s.step9.h);
      if (d < best) {
        best = d;
        chosen = s;
      }
    }
  }
  const ladder = chosen.steps.map((hex, i) => ({ hex, pos: i / 11, step: i + 1 }));
  return { ladder, scale: chosen.name, step9Hue: chosen.step9.h };
}
