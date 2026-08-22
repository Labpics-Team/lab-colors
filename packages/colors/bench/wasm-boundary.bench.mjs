// JS↔WASM boundary benchmark for the @labpics/colors REAL engine.
//
// This benchmark drives the REAL wasm-bindgen boundary — `recheckContrast` (the
// per-frame primitive)
// and `resolveTheme` (the on-breach re-solve) — so we can see what a call across
// the JS↔wasm line actually costs, and prove an optimisation keeps the results
// byte-identical.
//
// Fixture: the frozen canonical labui passport, resolved on a representative
// background. The shipping measurement uses every solid and translucent
// occurrence: alpha sources are composited again on each current backdrop
// before the packed contrast call, exactly like `adaptTheme`. The opaque-only
// batch remains a lower-level capability measurement, not a controller claim.
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
import { buildMissRing, rustCacheCapacity } from "./misses.mjs";
import {
  benchmarkOccurrencesFromRoles,
  materializeOccurrences,
} from "./occurrences.mjs";
import { __over, initSync, LabColors } from "../pkg/labcolors.js";

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

// Three worst-case samples of a varying backdrop (what strict/gradient mode feeds).
const SAMPLES = ["#38383A", "#404042", "#2E2E30"];

// C8d packed boundary: recheckContrast/recheckContrastMulti take packed
// `0x00RRGGBB` words + a `Uint32Array` of foregrounds + a numeric theme handle.
// Mirror the controller's `packRgb24Hex` (incl. #RGB shorthand expansion) and
// mint the theme handle ONCE, so the bench drives the real packed ABI — not
// hex strings silently coerced to 0. `resolveTheme` keeps the string theme (a
// cold authoring edge, unchanged by the hard-cut).
const pk = (hex) => {
  const b = hex.charCodeAt(0) === 35 /* '#' */ ? hex.slice(1) : hex;
  const six = b.length === 3 ? b[0] + b[0] + b[1] + b[1] + b[2] + b[2] : b;
  return Number.parseInt(six, 16) >>> 0;
};
const THEME_HANDLE = engine.themeHandle(THEME);
const OCCURRENCES = benchmarkOccurrencesFromRoles(resolved.roles, pk);
const OPAQUE_FGSW = Uint32Array.from(
  OCCURRENCES.filter(({ opacity }) => opacity === 1),
  ({ sourceRgb24 }) => sourceRgb24,
);
const SAMPLESW = Uint32Array.from(SAMPLES, pk);

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

// Shipping frame with one current backdrop: allocate one row, materialize every
// occurrence on that backdrop, then cross the packed recheck boundary once.
const recheckMixed1 = () => {
  const row = new Uint32Array(OCCURRENCES.length);
  materializeOccurrences(OCCURRENCES, SAMPLESW[0], row, __over);
  return engine.recheckContrast(SAMPLESW[0], row, THEME_HANDLE);
};
// Shipping frame with a finite three-sample support. The row allocation is
// shared across samples, while alpha occurrences are rematerialized per sample.
const recheckMixed3 = () => {
  const row = new Uint32Array(OCCURRENCES.length);
  let last;
  for (let s = 0; s < SAMPLESW.length; s++) {
    materializeOccurrences(OCCURRENCES, SAMPLESW[s], row, __over);
    last = engine.recheckContrast(SAMPLESW[s], row, THEME_HANDLE);
  }
  return last;
};
// Capability microbench: valid only for an all-opaque occurrence set, where one
// foreground row is physically shared by every backdrop.
const recheckOpaqueMulti3 = OPAQUE_FGSW.length > 0
  ? () => engine.recheckContrastMulti(SAMPLESW, OPAQUE_FGSW, THEME_HANDLE)
  : null;
// Re-solve, cache HIT (same bg repeatedly): pays only the JS-object projection.
const resolveHit = () => engine.resolveTheme(SOLVE_BG, THEME);
// Re-solve, cache MISS (distinct bg each call): full solve + projection. A ring
// one element larger than the Rust cache cannot reach a still-resident key.
// Nearby encoded colours avoid mixing legitimate contract conflicts into a
// successful-resolve latency sample; the test suite exhaustively admits them.
const cacheCapacity = rustCacheCapacity(
  readFileSync(resolve(here, "../../../crates/labcolors-wasm/src/engine.rs"), "utf8"),
);
const cacheMissBackgrounds = buildMissRing(pk(SOLVE_BG), cacheCapacity);
let missIndex = 0;
const resolveMiss = () => {
  const background = cacheMissBackgrounds[missIndex++ % cacheMissBackgrounds.length];
  return engine.resolveTheme(background, THEME);
};

// ── run ─────────────────────────────────────────────────────────────────────

const gcOn = typeof globalThis.gc === "function";
console.log(
  `node ${process.version} | occurrences=${OCCURRENCES.length} ` +
    `(opaque=${OPAQUE_FGSW.length}, alpha=${OCCURRENCES.length - OPAQUE_FGSW.length}) | ` +
    `theme=${THEME} | alloc-proxy=${gcOn}`,
);
console.log("");
console.log("call                              median ns   min ns   ns/role   alloc B/call");

// [label, fn, batches, inner, allocN, perRoleDivisor]
const plan = [
  ["mixed occurrence ×1", recheckMixed1, 40, 10000, 20000, OCCURRENCES.length],
  ["mixed occurrence ×3", recheckMixed3, 40, 3000, 7000, OCCURRENCES.length * 3],
  ...(recheckOpaqueMulti3
    ? [["opaque batch capability ×3", recheckOpaqueMulti3, 40, 7000, 15000, OPAQUE_FGSW.length * 3]]
    : []),
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
const fingerprintRow = new Uint32Array(OCCURRENCES.length);
for (const sample of SAMPLESW) {
  materializeOccurrences(OCCURRENCES, sample, fingerprintRow, __over);
  fp = fnv1aF64(fp, engine.recheckContrast(sample, fingerprintRow, THEME_HANDLE));
}
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
