// Deterministic hot-path benchmark for the @labpics/colors runtime.
//
// Measures the per-frame cost of `adaptTheme` (the rAF-driven controller) and
// its supporting primitives (`oklabLerp`, `parseCssColor`,
// `effectiveBackground`) on a manual clock with a stub engine, so numbers are
// reproducible and independent of WASM/solver cost — this isolates exactly the
// JS overhead a weak device pays every frame.
//
// Every scenario is fully deterministic: same schedule, same colours, same
// breach timing. Besides timing, each scenario reports a BEHAVIOUR FINGERPRINT
// (FNV-1a over the post-tick applied variable state of every frame) plus
// solve/recheck/style-op counters. An optimisation of the hot path must keep
// `fingerprint`, `solves` and `rechecks` IDENTICAL (byte-identical applied
// state); `styleSets`/`styleRemoves` may go DOWN (fewer redundant DOM writes)
// but never change the fingerprint.
//
// Run: node bench/hotpath.bench.mjs

import { performance } from "node:perf_hooks";
import { adaptTheme } from "../adapt-theme.js";
import { oklabLerp, parseCssColor, effectiveBackground } from "../effective-bg.js";

const FRAME_MS = 1000 / 60;
const WARMUP_FRAMES = 300;
const MEASURE_FRAMES = 3000;
const ROLE_COUNT = 24;
const TRANSLUCENT_COUNT = 8;

// ── helpers ─────────────────────────────────────────────────────────────────

const hex2 = (n) => n.toString(16).padStart(2, "0");
const toneHex = (t) => `#${hex2(t & 0xff)}${hex2((t * 3) & 0xff)}${hex2((t * 7) & 0xff)}`.toUpperCase();
const bgTone = (bg) => parseInt(bg.slice(1, 3), 16);

function fnv1a(hash, str) {
  let h = hash >>> 0;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** Minimal CSSOM-like inline style: ordered names + value map + op counters. */
function makeElement() {
  const names = [];
  const values = new Map();
  const counts = { set: 0, remove: 0 };
  return {
    counts,
    values,
    style: {
      setProperty(name, value) {
        counts.set++;
        if (!values.has(name)) names.push(name);
        values.set(name, value);
      },
      removeProperty(name) {
        counts.remove++;
        if (values.delete(name)) {
          const i = names.indexOf(name);
          if (i >= 0) names.splice(i, 1);
        }
      },
      item(i) {
        return names[i] ?? "";
      },
      get length() {
        return names.length;
      },
    },
  };
}

/** Deterministic stand-in for the WASM engine. `resolveTheme` derives role
 *  colours from the background tone; `recheckContrast` reports a breach (Lc 40
 *  vs target 60) iff the tone drifted ≥ 64 away from the last solved tone. */
function makeStubEngine() {
  const stub = {
    solves: 0,
    rechecks: 0,
    lastSolvedTone: -1,
    resolveTheme(bg) {
      stub.solves++;
      const tone = bgTone(bg);
      stub.lastSolvedTone = tone;
      const vars = {};
      const roles = {};
      for (let i = 0; i < ROLE_COUNT; i++) {
        const cssVar = `--lab-role-${i}`;
        vars[cssVar] = `oklch(${(40 + ((tone + i) % 50)).toFixed(1)}% 0.1200 ${(i * 13) % 360})`;
        roles[`role${i}`] = {
          kind: "color",
          cssVar,
          lc: 60,
          hex: toneHex(tone + i * 9),
          legalFloor: i % 3 === 0 ? 3 : null,
        };
      }
      for (let i = 0; i < TRANSLUCENT_COUNT; i++) {
        const cssVar = `--lab-tl-${i}`;
        vars[cssVar] = `oklch(80.0% 0.0200 ${(i * 31) % 360} / 0.6)`;
        roles[`tl${i}`] = { kind: "translucent", cssVar };
      }
      return { vars, roles };
    },
    recheckContrast(bg, fgs) {
      stub.rechecks++;
      const drift = Math.abs(bgTone(bg) - stub.lastSolvedTone);
      const lc = drift >= 64 ? 40 : 60;
      const out = new Float64Array(fgs.length * 2);
      for (let i = 0; i < fgs.length; i++) out[2 * i] = lc;
      return out;
    },
  };
  return stub;
}

// ── scenario driver ─────────────────────────────────────────────────────────

/**
 * @param {string} name
 * @param {(frame:number)=>string|string[]} bgAt  deterministic background schedule
 * @param {{fingerprint?: boolean}} [mode]
 */
function runScenario(name, bgAt, mode = {}) {
  const el = makeElement();
  const stub = makeStubEngine();
  let now = 0;
  let frame = 0;
  const ctrl = adaptTheme(el, {
    colors: stub,
    theme: "light",
    background: () => bgAt(frame),
    now: () => now,
    win: undefined,
  });

  let fp = 0x811c9dc5;
  const snapshot = () => {
    for (const [k, v] of el.values) fp = fnv1a(fnv1a(fp, k), v);
  };

  for (frame = 1; frame <= WARMUP_FRAMES; frame++) {
    now = frame * FRAME_MS;
    ctrl.tick(now);
  }
  el.counts.set = 0;
  el.counts.remove = 0;
  const rechecks0 = stub.rechecks;
  const solves0 = stub.solves;

  const t0 = performance.now();
  for (; frame <= WARMUP_FRAMES + MEASURE_FRAMES; frame++) {
    now = frame * FRAME_MS;
    ctrl.tick(now);
    if (mode.fingerprint) snapshot();
  }
  const t1 = performance.now();
  ctrl.stop();

  return {
    name,
    totalMs: t1 - t0,
    usPerFrame: ((t1 - t0) / MEASURE_FRAMES) * 1000,
    styleSets: el.counts.set,
    styleRemoves: el.counts.remove,
    solves: stub.solves - solves0,
    rechecks: stub.rechecks - rechecks0,
    fingerprint: mode.fingerprint ? fp.toString(16).padStart(8, "0") : "-",
  };
}

// Schedules. Tones are integers; toneHex() maps them onto #RRGGBB.
const SOLVED0 = 0x80;
const steadyBg = () => toneHex(SOLVED0);
// ±32 sine drift around the solved tone: key changes every frame, never breaches.
const driftBg = (f) => toneHex(SOLVED0 + Math.round(32 * Math.sin((2 * Math.PI * f) / 240)));
// Every 90 frames jump 96 tones away and hold 45 frames: sustained breach →
// re-solve → 280ms ease, then back near the new tone (no breach) — a steady
// mix of recheck / solve / ease frames.
const breachBg = (f) => toneHex(SOLVED0 + (Math.floor(f / 90) % 2 === 1 ? 96 : 0) + (f % 3));
// Three-sample varying backdrop with the same breach schedule.
const breachBg3 = (f) => {
  const base = breachBg(f);
  const t = bgTone(base);
  return [base, toneHex(t + 8), toneHex(t + 16)];
};

// ── micro benches ───────────────────────────────────────────────────────────

function micro(name, iters, fn) {
  // warmup
  for (let i = 0; i < Math.min(iters, 2e4); i++) fn(i);
  const t0 = performance.now();
  let sink = 0;
  for (let i = 0; i < iters; i++) sink ^= fn(i).length ?? 0;
  const t1 = performance.now();
  return { name, iters, nsPerOp: ((t1 - t0) / iters) * 1e6, sink };
}

const PARSE_FORMS = [
  "#1a2b3c",
  "#f0e1d2cc",
  "rgb(18, 52, 86)",
  "rgba(240, 225, 210, 0.8)",
  "rgb(18 52 86 / 0.5)",
  "oklch(62.8% 0.2577 29.2)",
  "oklch(0.628 0.2577 29.2 / 0.9)",
  "transparent",
];

function fakeChain(depth) {
  // depth translucent rgba layers over an opaque root — the worst honest case
  // for the ancestor walk.
  const nodes = [];
  let parent = null;
  for (let i = 0; i < depth; i++) {
    const css =
      i === depth - 1 ? "rgb(240, 240, 240)" : `rgba(${20 + i * 7}, ${30 + i * 5}, ${40 + i * 3}, 0.35)`;
    const node = { css, parent };
    nodes.unshift(node);
    parent = null;
  }
  for (let i = 0; i < nodes.length - 1; i++) nodes[i].parent = nodes[i + 1];
  return nodes[0];
}

// ── run ─────────────────────────────────────────────────────────────────────

const args = new Set(process.argv.slice(2));
const fingerprint = args.has("--fingerprint");

console.log(`node ${process.version} | frames=${MEASURE_FRAMES} roles=${ROLE_COUNT}+${TRANSLUCENT_COUNT}tl | fingerprint=${fingerprint}`);
console.log("");
console.log("scenario            µs/frame   styleSet  styleRem  solves  rechecks  fingerprint");

const scenarios = [
  runScenario("steady", steadyBg, { fingerprint }),
  runScenario("drift-nobreach", driftBg, { fingerprint }),
  runScenario("ease-default", breachBg, { fingerprint }),
  runScenario("ease-3bg", breachBg3, { fingerprint }),
];
for (const s of scenarios) {
  console.log(
    `${s.name.padEnd(18)} ${s.usPerFrame.toFixed(2).padStart(9)} ${String(s.styleSets).padStart(10)} ${String(s.styleRemoves).padStart(9)} ${String(s.solves).padStart(7)} ${String(s.rechecks).padStart(9)}  ${s.fingerprint}`,
  );
}

console.log("");
console.log("micro                     ns/op");
const chain = fakeChain(8);
const micros = [
  micro("oklabLerp hex→hex", 2e5, (i) => oklabLerp("#1A2B3C", "#F0E1D2", (i % 100) / 100)),
  micro("oklabLerp oklch→hex", 1e5, (i) => oklabLerp("oklch(62.8% 0.2577 29.2)", "#F0E1D2", (i % 100) / 100)),
  micro("parseCssColor mixed", 2e5, (i) => parseCssColor(PARSE_FORMS[i & 7]) ?? ""),
  micro("effectiveBackground d8", 5e4, () =>
    effectiveBackground(chain, {
      getStyle: (el) => ({ getPropertyValue: () => el.css }),
      parentOf: (el) => el.parent,
    }),
  ),
];
for (const m of micros) {
  console.log(`${m.name.padEnd(24)} ${m.nsPerOp.toFixed(0).padStart(7)}`);
}
