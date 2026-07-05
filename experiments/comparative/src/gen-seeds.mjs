// Генерация сидов (одноразовая): fixtures/seeds.json коммитится, все системы
// читают одни и те же hex. См. docs/comparative-experiments.md, раздел «Сиды».
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { oklchToHexGamutMapped } from "./color.mjs";

const repoRoot = new URL("../../../", import.meta.url);
const cfg = JSON.parse(
  readFileSync(new URL("crates/labcolors-wasm/tests/data/labui.config.json", repoRoot), "utf8"),
);

// P-набор: light-якоря палитры labui + brand.light.
const pSet = Object.entries(cfg.palette)
  .sort(([a], [b]) => Number(a) - Number(b))
  .map(([, p]) => ({ id: `p-${p.key}`, hex: p.anchors.light.toUpperCase() }));
pSet.push({ id: "p-brand", hex: cfg.brand.light.toUpperCase() });

// H-набор: oklch(0.65, 0.15, h), h = 0..330 шаг 30, гамут-безопасно.
const hSet = [];
for (let h = 0; h < 360; h += 30) {
  hSet.push({ id: `h-${String(h).padStart(3, "0")}`, hex: oklchToHexGamutMapped(0.65, 0.15, h) });
}

mkdirSync(new URL("../fixtures/", import.meta.url), { recursive: true });
writeFileSync(
  new URL("../fixtures/seeds.json", import.meta.url),
  JSON.stringify({ pSet, hSet }, null, 2) + "\n",
);
console.log(`fixtures/seeds.json: P=${pSet.length}, H=${hSet.length}`);
