// Раннер эксперимента: строит лестницы всех систем на общих сидах, считает
// предрегистрированные метрики, пишет results/results.json и вставляет таблицы
// в docs/comparative-experiments.md между AUTOGEN-маркерами.
// Детерминизм: RNG нет; повторный прогон обязан быть байт-идентичным.
import { execSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { hexToOklch, median } from "./color.mjs";
import * as M from "./metrics.mjs";
import { BG_SWEEP, buildS1, buildS2, buildS3, buildS4 } from "./systems.mjs";

const repoRootUrl = new URL("../../../", import.meta.url);
const SYSTEMS = ["s1", "s2", "s3a", "s3b", "s4"];
export const SYSTEM_LABELS = {
  s1: "S1 lab-colors (HEAD)",
  s2: "S2 MCU",
  s3a: "S3a OKLCH clip",
  s3b: "S3b OKLCH gamut-map",
  s4: "S4 Radix Colors",
};

function pkgVersion(name) {
  return JSON.parse(
    readFileSync(new URL(`../node_modules/${name}/package.json`, import.meta.url), "utf8"),
  ).version;
}

function seedEntry(seed) {
  const sc = hexToOklch(seed.hex);
  const zone = sc.C >= 0.02 && sc.h >= 90 && sc.h <= 140;
  const systems = {};

  let s1;
  try {
    s1 = buildS1(seed.hex);
  } catch (e) {
    s1 = { error: String(e?.message ?? e) };
  }
  systems.s1 = s1.error
    ? { error: s1.error }
    : {
        ladder: s1.ladder.map((s) => s.hex),
        m1a: M.m1aBlind(s1.ladder),
        m2: M.hueDrift(s1.ladder, sc.h),
        m3: M.chromaUtilization(s1.ladder),
        m4: M.monotonicity(s1.ladder),
        native: M.s1Native(s1.native),
      };

  const s2 = buildS2(seed.hex);
  systems.s2 = {
    ladder: s2.ladder.map((s) => s.hex),
    m1a: M.m1aBlind(s2.ladder),
    m2: M.hueDrift(s2.ladder, sc.h),
    m3: M.chromaUtilization(s2.ladder),
    m4: M.monotonicity(s2.ladder),
    native: M.s2Native(s2.ladder),
  };

  for (const [key, mode] of [
    ["s3a", "clip"],
    ["s3b", "gamut"],
  ]) {
    const s3 = buildS3(seed.hex, mode);
    systems[key] = {
      ladder: s3.ladder.map((s) => s.hex),
      m1a: M.m1aBlind(s3.ladder),
      m2: M.hueDrift(s3.ladder, sc.h),
      m3: M.chromaUtilization(s3.ladder),
      m4: M.monotonicity(s3.ladder),
      native: null,
    };
  }

  const s4 = buildS4(seed.hex);
  systems.s4 = {
    scale: s4.scale,
    ladder: s4.ladder.map((s) => s.hex),
    m1a: M.m1aBlind(s4.ladder),
    m2: M.hueDrift(s4.ladder, s4.step9Hue),
    m3: M.chromaUtilization(s4.ladder),
    m4: M.monotonicity(s4.ladder),
    native: M.s4Native(s4.ladder),
  };

  return { id: seed.id, hex: seed.hex, zone, systems };
}

function aggregate(entries, sys) {
  const ok = entries.map((e) => e.systems[sys]).filter((s) => s && !s.error);
  const m1a = { pairs: 0, ge45: 0, ge30: 0 };
  for (const s of ok) {
    m1a.pairs += s.m1a.pairs;
    m1a.ge45 += s.m1a.ge45;
    m1a.ge30 += s.m1a.ge30;
  }
  const m2means = ok.filter((s) => s.m2).map((s) => s.m2.mean);
  const m2maxes = ok.filter((s) => s.m2).map((s) => s.m2.max);
  const m3meds = ok.filter((s) => s.m3).map((s) => s.m3.median);
  const m4 = {
    ladders: ok.length,
    violations: ok.reduce((a, s) => a + s.m4.violations, 0),
    clean: ok.filter((s) => s.m4.violations === 0).length,
  };
  return {
    ladders: ok.length,
    errors: entries.filter((e) => e.systems[sys]?.error).length,
    m1a,
    m2: m2means.length
      ? { ladders: m2means.length, medianMean: median(m2means), medianMax: median(m2maxes) }
      : null,
    m3: m3meds.length ? { ladders: m3meds.length, median: median(m3meds) } : null,
    m4,
  };
}

function aggregateNative(entries) {
  const out = {};
  const s1 = { total: 0, achieved: 0, flagged: 0 };
  for (const e of entries) {
    const n = e.systems.s1?.native;
    if (!n) continue;
    s1.total += n.total;
    s1.achieved += n.achieved;
    s1.flagged += n.flagged;
  }
  out.s1 = s1;
  const s2 = { claim45: { pairs: 0, pass: 0 }, claim30: { pairs: 0, pass: 0 } };
  for (const e of entries) {
    const n = e.systems.s2.native;
    s2.claim45.pairs += n.claim45.pairs;
    s2.claim45.pass += n.claim45.pass;
    s2.claim30.pairs += n.claim30.pairs;
    s2.claim30.pass += n.claim30.pass;
  }
  out.s2 = s2;
  out.s3a = null;
  out.s3b = null;
  const s4 = { pairs: 0, pass: 0 };
  for (const e of entries) {
    s4.pairs += e.systems.s4.native.pairs;
    s4.pass += e.systems.s4.native.pass;
  }
  out.s4 = s4;
  return out;
}

export function computeResults() {
  const seedsFile = JSON.parse(
    readFileSync(new URL("../fixtures/seeds.json", import.meta.url), "utf8"),
  );
  const seeds = [...seedsFile.pSet, ...seedsFile.hSet];
  const perSeed = seeds.map(seedEntry);
  const zoneSeeds = perSeed.filter((e) => e.zone);

  const aggregates = {};
  for (const sys of SYSTEMS) {
    aggregates[sys] = {
      all: aggregate(perSeed, sys),
      zone: aggregate(zoneSeeds, sys),
    };
  }

  return {
    meta: {
      issue: 45,
      s1Commit: execSync("git rev-parse --short HEAD", { cwd: fileURLToPath(repoRootUrl) })
        .toString()
        .trim(),
      versions: {
        "@material/material-color-utilities": pkgVersion("@material/material-color-utilities"),
        "@radix-ui/colors": pkgVersion("@radix-ui/colors"),
      },
      seeds: { p: seedsFile.pSet.length, h: seedsFile.hSet.length, zone: zoneSeeds.length },
      bgSweepL: BG_SWEEP.map((b) => Number(b.L.toFixed(4))),
      zoneSeedIds: zoneSeeds.map((e) => e.id),
    },
    native: aggregateNative(perSeed),
    aggregates,
    perSeed,
  };
}

// --- рендер таблиц ---

const pct = (num, den) => (den === 0 ? "—" : `${((100 * num) / den).toFixed(1)} %`);
const deg = (x) => `${x.toFixed(1)}°`;

function renderMarkdown(r) {
  const rows = (fn) => SYSTEMS.map((sys) => fn(sys, r.aggregates[sys])).join("\n");
  const L = SYSTEM_LABELS;
  const n = r.native;
  const lines = [];
  lines.push(
    `_Прогон: lab-colors @ \`${r.meta.s1Commit}\`; MCU ${r.meta.versions["@material/material-color-utilities"]}; Radix Colors ${r.meta.versions["@radix-ui/colors"]}. ` +
      `Сидов: ${r.meta.seeds.p + r.meta.seeds.h} (P=${r.meta.seeds.p}, H=${r.meta.seeds.h}), в жёлто-зелёной зоне: ${r.meta.seeds.zone} (${r.meta.zoneSeedIds.join(", ")})._`,
    "",
    "**M1-A — слепой протокол контраста (пары шагов, расстояние ≥ 0.5)**",
    "",
    "| Система | Пар | ≥ 4.5:1 | ≥ 3.0:1 |",
    "|---|---:|---:|---:|",
    rows(
      (sys, a) =>
        `| ${L[sys]} | ${a.all.m1a.pairs} | ${pct(a.all.m1a.ge45, a.all.m1a.pairs)} | ${pct(a.all.m1a.ge30, a.all.m1a.pairs)} |`,
    ),
    "",
    "**M1-B — нативные протоколы (операционализация заявок)**",
    "",
    "| Система | Заявка | Случаев | Выполнено |",
    "|---|---|---:|---:|",
    `| ${L.s1} | роль с legalFloor достигает пола на фоне | ${n.s1.total} | ${pct(n.s1.achieved, n.s1.total)} |`,
    `| ${L.s1} | — из них с флагами честности | ${n.s1.total} | ${pct(n.s1.flagged, n.s1.total)} |`,
    `| ${L.s2} | ΔTone ≥ 50 ⇒ контраст ≥ 4.5:1 | ${n.s2.claim45.pairs} | ${pct(n.s2.claim45.pass, n.s2.claim45.pairs)} |`,
    `| ${L.s2} | ΔTone ≥ 40 ⇒ контраст ≥ 3.0:1 | ${n.s2.claim30.pairs} | ${pct(n.s2.claim30.pass, n.s2.claim30.pairs)} |`,
    `| ${L.s3a} / ${L.s3b} | нативной заявки нет | — | N/A |`,
    `| ${L.s4} | шаги 11/12 на фонах 1–3 ⇒ ≥ 4.5:1 | ${n.s4.pairs} | ${pct(n.s4.pass, n.s4.pairs)} |`,
    "",
    "**M2 — дрейф hue вдоль лестницы (медианы по сидам)**",
    "",
    "| Система | Средний дрейф | Максимальный дрейф |",
    "|---|---:|---:|",
    rows((sys, a) =>
      a.all.m2
        ? `| ${L[sys]} | ${deg(a.all.m2.medianMean)} | ${deg(a.all.m2.medianMax)} |`
        : `| ${L[sys]} | — | — |`,
    ),
    "",
    "**M3 — утилизация хромы C/Cmax при L ∈ [0.35, 0.75] (дескриптивная)**",
    "",
    "| Система | Медиана |",
    "|---|---:|",
    rows((sys, a) =>
      a.all.m3 ? `| ${L[sys]} | ${a.all.m3.median.toFixed(3)} |` : `| ${L[sys]} | — |`,
    ),
    "",
    "**M4 — монотонность (Y строго убывает вдоль light→dark)**",
    "",
    "| Система | Нарушений | Лестниц без нарушений |",
    "|---|---:|---:|",
    rows(
      (sys, a) =>
        `| ${L[sys]} | ${a.all.m4.violations} | ${a.all.m4.clean}/${a.all.m4.ladders} |`,
    ),
    "",
    "**M5 — жёлто-зелёная зона (hue сида 90–140°)**",
    "",
    "| Система | M1-A ≥ 4.5:1 | Средний дрейф hue | Нарушений монотонности |",
    "|---|---:|---:|---:|",
    rows(
      (sys, a) =>
        `| ${L[sys]} | ${pct(a.zone.m1a.ge45, a.zone.m1a.pairs)} | ${a.zone.m2 ? deg(a.zone.m2.medianMean) : "—"} | ${a.zone.m4.violations} |`,
    ),
  );
  const s1errs = SYSTEMS.map((sys) => r.aggregates[sys].all.errors).reduce((a, x) => a + x, 0);
  if (s1errs > 0) {
    lines.push("", `**Ошибки резолва:** ${s1errs} (см. results.json — публикуются честно).`);
  }
  return lines.join("\n");
}

function injectIntoDoc(md) {
  const docUrl = new URL("docs/comparative-experiments.md", repoRootUrl);
  const doc = readFileSync(docUrl, "utf8");
  const begin = "<!-- AUTOGEN:RESULTS:BEGIN -->";
  const end = "<!-- AUTOGEN:RESULTS:END -->";
  const bi = doc.indexOf(begin);
  const ei = doc.indexOf(end);
  if (bi === -1 || ei === -1) throw new Error("AUTOGEN-маркеры не найдены в докладе");
  writeFileSync(docUrl, doc.slice(0, bi + begin.length) + "\n" + md + "\n" + doc.slice(ei));
}

function main() {
  const results = computeResults();
  mkdirSync(new URL("../results/", import.meta.url), { recursive: true });
  writeFileSync(
    new URL("../results/results.json", import.meta.url),
    JSON.stringify(results, null, 2) + "\n",
  );
  injectIntoDoc(renderMarkdown(results));
  console.log(
    `Готово: сидов=${results.meta.seeds.p + results.meta.seeds.h}, ` +
      `commit=${results.meta.s1Commit}; results/results.json и таблицы в доке обновлены.`,
  );
}

if (process.argv[1] && process.argv[1].endsWith("run.mjs")) main();
