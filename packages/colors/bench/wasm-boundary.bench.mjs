// JS↔WASM boundary benchmark for the @labpics/colors REAL engine.
//
// The sibling `hotpath.bench.mjs` measures the pure-JS controller (`adaptTheme`)
// against a STUB engine, isolating JS overhead. This one is its pair: it drives
// the REAL wasm-bindgen boundary — `recheckContrast` (the per-frame primitive)
// and `resolveTheme` (the on-breach re-solve) — so we can see what a call across
// the JS↔wasm line actually costs, and prove an optimisation keeps the results
// byte-identical.
//
// Fixture: the frozen canonical labui passport, resolved on a representative
// background, giving the true ~28-role foreground set the runtime rechecks each
// frame (the controller passes `colorRoles.map(r => r.hex)` to recheckContrast).
//
// Reports median ns/call after warmup (medians resist GC/JIT jitter better than
// means). With `--expose-gc` it also reports a heap-allocation proxy: bytes of JS
// heap retained per call, forced-GC-bracketed — a lower bound on per-call garbage.
//
// Run:  node bench/wasm-boundary.bench.mjs
//       node --expose-gc bench/wasm-boundary.bench.mjs   (adds alloc proxy)

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { initSync, LabColors } from "../pkg/labcolors.js";

const here = dirname(fileURLToPath(import.meta.url));
initSync({ module: readFileSync(resolve(here, "../pkg/labcolors_bg.wasm")) });

const CONFIG = readFileSync(
  resolve(here, "../../../crates/labcolors-wasm/tests/data/labui.config.json"),
  "utf8",
);

// ── fixture ───────────────────────────────────────────────────────────────
// Resolve on a representative mid background, then recheck the resulting set
// against a *nearby, shifted* background — exactly the runtime's shape (the
// current colours re-checked against a drifted backdrop, not their own bg).

const engine = new LabColors();
engine.loadConfig(CONFIG);

const SOLVE_BG = "#3A3A3C"; // a representative surface (mid dark)
const THEME = "dark";
const resolved = engine.resolveTheme(SOLVE_BG, THEME);
const FGS = Object.values(resolved.roles)
  .filter((r) => r.kind === "color")
  .map((r) => r.hex);

// Three worst-case samples of a varying backdrop (what strict/gradient mode feeds).
const SAMPLES = ["#38383A", "#404042", "#2E2E30"];

// ── timing core ─────────────────────────────────────────────────────────────

/** Median ns/call: run `fn` in `batches` batches of `inner` calls, take the
 *  median batch throughput. `inner` amortises timer resolution; the median over
 *  batches rejects GC/JIT outliers. */
function medianNs(label, batches, inner, fn) {
  // Warm the JIT and the wasm paths hard before measuring.
  for (let i = 0; i < Math.min(inner * 4, 40000); i++) fn(i);
  const perBatch = [];
  for (let b = 0; b < batches; b++) {
    const t0 = performance.now();
    for (let i = 0; i < inner; i++) fn(i);
    const t1 = performance.now();
    perBatch.push(((t1 - t0) / inner) * 1e6);
  }
  perBatch.sort((a, b) => a - b);
  const median = perBatch[perBatch.length >> 1];
  const min = perBatch[0];
  return { label, medianNs: median, minNs: min };
}

/** Per-call JS-heap allocation proxy (needs --expose-gc). Force GC, sample
 *  heapUsed, run N calls holding no references, force GC, resample. The delta /
 *  N is a floor on retained garbage per call (transient garbage GC'd mid-run is
 *  not counted, so this UNDER-reports — a lower bound, honest as such). */
function allocPerCall(fn, n) {
  if (typeof globalThis.gc !== "function") return null;
  globalThis.gc();
  globalThis.gc();
  const before = process.memoryUsage().heapUsed;
  let sink = 0;
  for (let i = 0; i < n; i++) {
    const r = fn(i);
    sink ^= r.length | 0; // touch result, retain nothing
  }
  globalThis.gc();
  const after = process.memoryUsage().heapUsed;
  void sink;
  return (after - before) / n;
}

// ── the calls under test ────────────────────────────────────────────────────

// One frame with a solid background = ONE recheck of the whole fg set.
const recheck1 = () => engine.recheckContrast(SAMPLES[0], FGS, THEME);
// One frame with a 3-sample varying backdrop = THREE rechecks (worst-case loop).
const recheck3 = () => {
  let last;
  for (let s = 0; s < 3; s++) last = engine.recheckContrast(SAMPLES[s], FGS, THEME);
  return last;
};
// The same 3-sample frame as `recheck3`, but batched into ONE call so each
// foreground's CAM16 forward is computed once and shared across samples.
// Byte-identical to `recheck3`; the public batch API the controller now uses.
const recheckMulti3 = () => engine.recheckContrastMulti(SAMPLES, FGS, THEME);
// Re-solve, cache HIT (same bg repeatedly): pays only the JS-object projection.
const resolveHit = () => engine.resolveTheme(SOLVE_BG, THEME);
// Re-solve, cache MISS (distinct bg each call): full solve + projection. Sweep
// 4096 distinct backgrounds so we never repeat within the cache window.
let missTone = 0;
const resolveMiss = () => {
  const t = missTone++ & 0xfff;
  const hex = `#${(0x100000 + t * 17).toString(16).slice(-6).toUpperCase()}`;
  return engine.resolveTheme(hex, THEME);
};

// ── run ─────────────────────────────────────────────────────────────────────

const gcOn = typeof globalThis.gc === "function";
console.log(
  `node ${process.version} | roles=${FGS.length} color fgs | theme=${THEME} | alloc-proxy=${gcOn}`,
);
console.log("");
console.log("call                              median ns   min ns   ns/role   alloc B/call");

// [label, fn, batches, inner, allocN, perRoleDivisor]
const plan = [
  ["recheckContrast ×1 (28 roles)", recheck1, 40, 20000, 40000, FGS.length],
  ["recheckContrast ×3 (28 roles)", recheck3, 40, 7000, 15000, FGS.length * 3],
  ["recheckContrastMulti 3bg", recheckMulti3, 40, 7000, 15000, FGS.length * 3],
  ["resolveTheme cache-hit", resolveHit, 25, 4000, 8000, 0],
  ["resolveTheme cache-miss", resolveMiss, 20, 1500, 4000, 0],
];

for (const [label, fn, batches, inner, allocN, perRoleDiv] of plan) {
  const r = medianNs(label, batches, inner, fn);
  const perRole = perRoleDiv ? (r.medianNs / perRoleDiv).toFixed(1) : "-";
  const a = gcOn ? allocPerCall(fn, allocN) : null;
  const alloc = a == null ? "-" : a.toFixed(0);
  console.log(
    `${label.padEnd(32)} ${r.medianNs.toFixed(0).padStart(8)} ${r.minNs.toFixed(0).padStart(8)} ${String(perRole).padStart(9)} ${String(alloc).padStart(14)}`,
  );
}

// A stable fingerprint of the recheck output over the sample set — so a
// before/after run proves byte-identical numeric results across an optimisation.
function fnv1aF64(hash, arr) {
  let h = hash >>> 0;
  const b = new Uint8Array(arr.buffer, arr.byteOffset, arr.byteLength);
  for (let i = 0; i < b.length; i++) {
    h ^= b[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}
let fp = 0x811c9dc5;
for (const s of SAMPLES) fp = fnv1aF64(fp, engine.recheckContrast(s, FGS, THEME));
// Include a resolveTheme vars fingerprint too (the projection is under test).
const vfp = (() => {
  let h = 0x811c9dc5;
  const v = engine.resolveTheme(SOLVE_BG, THEME).vars;
  for (const k of Object.keys(v).sort()) {
    for (let i = 0; i < k.length; i++) (h ^= k.charCodeAt(i)), (h = Math.imul(h, 0x01000193) >>> 0);
    const s = v[k];
    for (let i = 0; i < s.length; i++) (h ^= s.charCodeAt(i)), (h = Math.imul(h, 0x01000193) >>> 0);
  }
  return h >>> 0;
})();
console.log("");
console.log(`recheck fingerprint: ${fp.toString(16).padStart(8, "0")}`);
console.log(`resolve vars fingerprint: ${vfp.toString(16).padStart(8, "0")}`);
