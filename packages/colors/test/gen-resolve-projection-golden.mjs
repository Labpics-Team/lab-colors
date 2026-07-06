// Генератор golden-снапшота ПОЛНОЙ JS-проекции `resolveTheme` — пара к
// `resolve-projection-parity.test.mjs`.
//
// Запускается ОДИН РАЗ на до-оптимизационной сборке `pkg/` (проекция через
// Reflect.set) и фиксирует для набора (theme, bg):
//   - `json`  — JSON.stringify всего результата: значения И порядок ключей
//     (top-level, roles, объекты ролей, vars);
//   - `f64fp` — FNV-1a по битовым паттернам каждого числового листа в порядке
//     обхода: ловит дрейф, который stringify маскирует (например, -0 → "0").
//
// Перегенерация — только осознанным решением при СМЫСЛОВОМ изменении контракта
// проекции, никогда — ради «починки» перф-оптимизации.
//
// Run: node test/gen-resolve-projection-golden.mjs   (из packages/colors)

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { initSync, LabColors } from "../pkg/labcolors.js";

const here = dirname(fileURLToPath(import.meta.url));
initSync({ module: readFileSync(resolve(here, "../pkg/labcolors_bg.wasm")) });
const CONFIG = readFileSync(
  resolve(here, "../../../crates/labcolors-wasm/tests/data/labui.config.json"),
  "utf8",
);

const engine = new LabColors();
engine.loadConfig(CONFIG);

// Бенчевый фон (#3A3A3C dark) + края и середины обеих полярностей + обе
// ic-темы: покрывает solid/translucent/glow/none-исходы контракта labui.
const CASES = [
  ["dark", "#3A3A3C"],
  ["dark", "#101012"],
  ["dark", "#2E2E30"],
  ["dark", "#7A7A7E"],
  ["light", "#FFFFFF"],
  ["light", "#F2F2F7"],
  ["light", "#7A7A7E"],
  ["light-ic", "#FFFFFF"],
  ["dark-ic", "#101012"],
];

/** FNV-1a по little-endian байтам Float64 каждого числового листа. */
function f64fp(value) {
  let h = 0x811c9dc5;
  const buf = new DataView(new ArrayBuffer(8));
  const mix = (b) => {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  };
  const walk = (v) => {
    if (typeof v === "number") {
      buf.setFloat64(0, v, true);
      for (let i = 0; i < 8; i++) mix(buf.getUint8(i));
    } else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") for (const k of Object.keys(v)) walk(v[k]);
  };
  walk(value);
  return (h >>> 0).toString(16).padStart(8, "0");
}

const golden = {
  cases: CASES.map(([theme, bg]) => {
    const r = engine.resolveTheme(bg, theme);
    return { theme, bg, json: JSON.stringify(r), f64fp: f64fp(r) };
  }),
};
writeFileSync(
  resolve(here, "resolve-projection.golden.json"),
  JSON.stringify(golden, null, 2) + "\n",
);
console.log(`resolve-projection.golden.json: ${golden.cases.length} cases`);
