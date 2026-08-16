// Hot-path parity locks for the adapt-theme runtime.
//
// The perf pass (perf/js-runtime-hotpath) rewrites the per-frame internals —
// compiled lerp pairs and diff-writes — under one invariant:
// the APPLIED VARIABLE STATE of every frame is byte-identical to the original
// parse-per-frame implementation. This file locks that invariant two ways:
//
//   1. GOLDEN FINGERPRINTS: a deterministic mini-scenario (manual clock, stub
//      engine, fixed breach schedule) hashes the post-tick applied state of
//      every frame. The hashes below were captured on the PRE-optimisation
//      implementation (commit base of this branch); any drift in what the
//      controller paints — one byte, one frame — changes the hash.
//      Regenerate (only for a DELIBERATE behaviour change, never to "fix" a
//      perf regression): PRINT_FP=1 node --test test/hotpath-parity.test.mjs
//
//   2. PAIR/STRING PARITY: the compiled-pair fast paths must equal their
//      string-path references on randomised inputs (seeded PRNG, reproducible).
//      These assertions are skipped gracefully while the compiled helpers do
//      not exist yet (pre-optimisation), so the golden capture run is green.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import "./fake-node-brand.mjs";
import { adaptTheme } from "../adapt-theme.js";
import * as ebg from "../effective-bg.js";
import { buildMissRing, rustCacheCapacity } from "../bench/misses.mjs";
import {
  benchmarkOccurrencesFromRoles,
  materializeOccurrences,
} from "../bench/occurrences.mjs";
import { __over, initSync, LabColors } from "../pkg/labcolors.js";
import { acquireOutputLease } from "../output-sink.js";
import { outputElement } from "./output-host.mjs";

const { oklabLerp } = ebg;

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

// ── deterministic mini-harness (small, self-contained hot-path replay) ───────

const FRAME_MS = 1000 / 60;
const FRAMES = 700;
const ROLE_COUNT = 12;
const TL_COUNT = 4;

const hex2 = (n) => n.toString(16).padStart(2, "0");
const toneHex = (t) => `#${hex2(t & 0xff)}${hex2((t * 3) & 0xff)}${hex2((t * 7) & 0xff)}`.toUpperCase();
const bgTone = (bg) => parseInt(bg.slice(1, 3), 16);
const pack = (hex) => Number.parseInt(hex.slice(1), 16) >>> 0;
const unpack = (word) => `#${word.toString(16).padStart(6, "0").toUpperCase()}`;

const fnv1a = (h, str) => {
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
};

const fingerprintValues = (hash, values) => {
  const entries = [...values].sort(([left], [right]) =>
    left < right ? -1 : left > right ? 1 : 0);
  for (const [key, value] of entries) hash = fnv1a(fnv1a(hash, key), value);
  return hash >>> 0;
};

function makeElement() {
  const element = outputElement();
  element.values = element.props;
  return element;
}

function makeStubEngine() {
  const stub = {
    lastSolvedTone: -1,
    resolveTheme(bg) {
      const tone = bgTone(bg);
      stub.lastSolvedTone = tone;
      const vars = {};
      const roles = {};
      const outputBindings = [];
      for (let i = 0; i < ROLE_COUNT; i++) {
        const cssVar = `--lab-role-${i}`;
        outputBindings.push(cssVar);
        vars[cssVar] = `oklch(${(40 + ((tone + i) % 50)).toFixed(1)}% 0.1200 ${(i * 13) % 360})`;
        roles[`role${i}`] = {
          kind: "color",
          cssVar,
          lc: 60,
          hex: toneHex(tone + i * 9),
          legalFloor: i % 3 === 0 ? 3 : null,
        };
      }
      for (let i = 0; i < TL_COUNT; i++) {
        const cssVar = `--lab-tl-${i}`;
        outputBindings.push(cssVar);
        const css = `oklch(80.0% 0.0200 ${(i * 31) % 360} / 0.6)`;
        const tintHex = ebg.toHex(ebg.parseCssColor(css));
        const alpha = 0.6;
        vars[cssVar] = css;
        roles[`tl${i}`] = {
          kind: "translucent",
          cssVar,
          tintHex,
          alpha,
          compositeHex: unpack(__over(pack(tintHex), alpha, pack(bg))),
          compositeLc: 60,
        };
      }
      return { outputBindings, vars, roles };
    },
    recheckContrast(bg, fgs) {
      // Packed boundary (C8d): `bg` is a `0x00RRGGBB` word. Its R byte is the
      // same tone `bgTone` extracts from the hex spelling, so the drift — and
      // thus every applied-state fingerprint — is byte-identical to the pre-pack
      // string path.
      const drift = Math.abs(((bg >> 16) & 0xff) - stub.lastSolvedTone);
      const lc = drift >= 64 ? 40 : 60;
      const out = new Float64Array(fgs.length * 2);
      for (let i = 0; i < fgs.length; i++) out[2 * i] = lc;
      return out;
    },
  };
  return stub;
}

function runFingerprint(bgAt) {
  const el = makeElement();
  let now = 0;
  let frame = 0;
  const ctrl = adaptTheme(el, {
    colors: makeStubEngine(),
    theme: "light",
    background: () => bgAt(frame),
    now: () => now,
    win: undefined,
  });
  let fp = 0x811c9dc5;
  for (frame = 1; frame <= FRAMES; frame++) {
    now = frame * FRAME_MS;
    ctrl.tick(now);
    fp = fingerprintValues(fp, el.values);
  }
  ctrl.stop();
  return fp.toString(16).padStart(8, "0");
}

const SOLVED0 = 0x80;
const steadyBg = () => toneHex(SOLVED0);
const driftBg = (f) => toneHex(SOLVED0 + Math.round(32 * Math.sin((2 * Math.PI * f) / 240)));
const breachBg = (f) => toneHex(SOLVED0 + (Math.floor(f / 90) % 2 === 1 ? 96 : 0) + (f % 3));
// Re-derived with lexicographic state iteration on both the sequential writer
// at 0ff950db and the atomic sink. Their fingerprints are byte-identical; only
// the old Map-insertion-order oracle changed. (steady === drift is expected:
// while rechecks pass, the applied state never changes.)
const GOLDEN = {
  steady: "e3cfef6d",
  drift: "e3cfef6d",
  ease: "a385f743",
};

const CASES = [
  ["steady", steadyBg],
  ["drift", driftBg],
  ["ease", breachBg],
];

test("hotpath state fingerprint is independent of output lease acquisition order", () => {
  const first = outputElement();
  const firstA = acquireOutputLease(first, ["--lab-a"], "hotpath/order/a");
  const firstB = acquireOutputLease(first, ["--lab-b"], "hotpath/order/b");
  firstA.publish({ "--lab-a": "#111111" });
  firstB.publish({ "--lab-b": "#222222" });

  const second = outputElement();
  const secondB = acquireOutputLease(second, ["--lab-b"], "hotpath/order/b");
  const secondA = acquireOutputLease(second, ["--lab-a"], "hotpath/order/a");
  secondB.publish({ "--lab-b": "#222222" });
  secondA.publish({ "--lab-a": "#111111" });

  assert.equal(
    fingerprintValues(0x811c9dc5, first.props),
    fingerprintValues(0x811c9dc5, second.props),
  );
});

if (process.env.PRINT_FP) {
  test("print golden fingerprints (capture mode)", () => {
    for (const [name, bg] of CASES) {
      console.log(`GOLDEN ${name}: "${runFingerprint(bg)}"`);
    }
  });
} else {
  for (const [name, bg] of CASES) {
    test(`golden fingerprint: ${name}`, () => {
      assert.equal(runFingerprint(bg), GOLDEN[name]);
    });
  }
}

// ── compiled-pair parity vs the string paths (post-optimisation only) ────────

const hasCompiled =
  typeof ebg.compileLerpPair === "function" &&
  typeof ebg.lerpPairHex === "function";

// Mulberry32 — tiny seeded PRNG, reproducible across runs.
function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function randColor(rnd) {
  const forms = [
    () => `#${hex2((rnd() * 256) | 0)}${hex2((rnd() * 256) | 0)}${hex2((rnd() * 256) | 0)}`,
    () => `rgb(${(rnd() * 256) | 0}, ${(rnd() * 256) | 0}, ${(rnd() * 256) | 0})`,
    () => `rgba(${(rnd() * 256) | 0} ${(rnd() * 256) | 0} ${(rnd() * 256) | 0} / ${rnd().toFixed(2)})`,
    () => `oklch(${(rnd() * 100).toFixed(1)}% ${(rnd() * 0.3).toFixed(4)} ${(rnd() * 360).toFixed(1)})`,
  ];
  return forms[(rnd() * forms.length) | 0]();
}

const T_EDGES = [-0.5, 0, 1e-9, 0.25, 0.5, 0.75, 1 - 1e-9, 1, 1.5];

test("compiled pair ≡ string path, 500 random pairs", { skip: !hasCompiled }, () => {
  const rnd = mulberry32(0xc0ffee);
  for (let i = 0; i < 500; i++) {
    const from = randColor(rnd);
    const to = randColor(rnd);
    const pair = ebg.compileLerpPair(from, to);
    assert.ok(pair, `pair must compile for parseable endpoints: ${from} → ${to}`);
    for (const t of T_EDGES.concat(rnd(), rnd())) {
      const viaString = oklabLerp(from, to, t);
      assert.equal(ebg.lerpPairHex(pair, t), viaString, `lerp mismatch @t=${t}: ${from} → ${to}`);
    }
  }
});

test("compileLerpPair falls back (null) on unparseable endpoints", { skip: !hasCompiled }, () => {
  assert.equal(ebg.compileLerpPair("blah", "#112233"), null);
  assert.equal(ebg.compileLerpPair("#112233", "hsl(1,2%,3%)"), null);
});

test("the boundary benchmark materializes opaque and alpha occurrences behaviorally", () => {
  const occurrences = benchmarkOccurrencesFromRoles(
    {
      label: { kind: "color", hex: "#010203" },
      veil: { kind: "translucent", tintHex: "#C0B2FA", alpha: 0.122 },
      unresolved: { kind: "failure" },
    },
    pack,
  );
  const black = materializeOccurrences(
    occurrences,
    pack("#000000"),
    new Uint32Array(occurrences.length),
    __over,
  );
  const white = materializeOccurrences(
    occurrences,
    pack("#FFFFFF"),
    new Uint32Array(occurrences.length),
    __over,
  );

  assert.deepEqual([...black], [pack("#010203"), pack("#17161F")]);
  assert.deepEqual([...white], [pack("#010203"), pack("#F7F6FE")]);
  assert.equal(black[0], white[0], "opaque occurrence must be backdrop-independent");
  assert.notEqual(black[1], white[1], "alpha occurrence must be rematerialized per backdrop");
  assert.throws(
    () => materializeOccurrences(
      [occurrences[1]],
      pack("#000000"),
      new Uint32Array(1),
      () => 0xFFFFFFFF,
    ),
    { name: "RangeError", message: /Core rejected admitted opacity/u },
  );
});

test("rustCacheCapacity accepts formatting-preserving Rust literal variants", () => {
  for (const source of [
    "const CACHE_CAPACITY: usize = 4096;",
    "  pub const  CACHE_CAPACITY : usize = 4_096 ; // policy",
    "pub(crate)\tconst CACHE_CAPACITY:\tusize=4_096usize; /* policy */",
  ]) {
    assert.equal(rustCacheCapacity(source), 4096, source);
  }
  assert.throws(
    () => rustCacheCapacity("const CACHE_CAPACITY: usize = 1 << 12;"),
    /capacity is absent or outside/u,
  );
});

test("the cache-miss benchmark corpus is admissible and never masks conflict", () => {
  const engineSource = readFileSync(
    new URL("../../../crates/labcolors-wasm/src/engine.rs", import.meta.url),
    "utf8",
  );
  const capacity = rustCacheCapacity(engineSource);
  const solveBackground = "#3A3A3C";
  const ring = buildMissRing(pack(solveBackground), capacity);

  assert.equal(ring.length, capacity + 1);
  assert.equal(new Set(ring).size, ring.length);
  assert.equal(ring.includes(solveBackground), false);

  const colors = new LabColors();
  colors.loadConfig(
    readFileSync(
      new URL("../../../crates/labcolors-wasm/tests/data/labui.config.json", import.meta.url),
      "utf8",
    ),
  );
  for (const background of ring) {
    assert.doesNotThrow(
      () => colors.resolveTheme(background, "dark"),
      `cache-miss background ${background} must admit a successful latency sample`,
    );
  }

  let conflict;
  try {
    colors.resolveTheme("#1099FF", "dark");
  } catch (error) {
    conflict = error;
  }
  assert.ok(conflict instanceof Error, "the historical arbitrary sweep witness must conflict");
  assert.equal(conflict.code, "output_conflict");
  assert.deepEqual(
    conflict.conflicts.map(({ role, code }) => ({ role, code })),
    [
      { role: "border-warning-strong", code: "unsatisfiable_criterion" },
      { role: "border-success-strong", code: "unsatisfiable_criterion" },
    ],
  );
});
