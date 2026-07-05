// Byte-identity parity for the JS↔WASM recheck boundary.
//
// `wasm-boundary.golden.json` was snapshotted on the PRE-optimisation engine
// (the display-path `recheck_against`). This test loads the REAL rebuilt wasm and
// asserts every `recheckContrast` result is bit-for-bit what the pre-change
// engine produced — the lock that the transcendental-eliding fast path (WCAG from
// the linear decode) changed performance and nothing else. It also checks that
// the `_recheckContrastMulti` prototype equals per-sample `recheckContrast`
// exactly, and that a handful of `resolveTheme` vars are unmoved (the
// `contrast_ratio` refactor must not shift solver output).
//
// Requires the built `pkg/` (CI runs `npm test` after `wasm-pack build`). Skips
// cleanly with a clear message if the wasm bundle is absent, so the pure-JS
// `node --test` suite still runs locally without a build.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "../pkg/labcolors_bg.wasm");
const gluePath = resolve(here, "../pkg/labcolors.js");
const goldenPath = resolve(here, "wasm-boundary.golden.json");

const haveWasm = existsSync(wasmPath) && existsSync(gluePath);

test("wasm recheck boundary is byte-identical to the pre-optimisation golden", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first (CI builds before `npm test`)");
    return;
  }
  const { initSync, LabColors } = await import(pathToFileURL(gluePath).href);
  initSync({ module: readFileSync(wasmPath) });
  const CONFIG = readFileSync(
    resolve(here, "../../../crates/labcolors-wasm/tests/data/labui.config.json"),
    "utf8",
  );
  const golden = JSON.parse(readFileSync(goldenPath, "utf8"));

  const engine = new LabColors();
  engine.loadConfig(CONFIG);

  // (1) Every recheck case: exact f64 equality (Object.is catches ±0 / NaN too).
  for (const { theme, bg, fgs, flat } of golden.recheck) {
    const got = engine.recheckContrast(bg, fgs, theme);
    assert.equal(got.length, flat.length, `${theme} ${bg}: length`);
    for (let i = 0; i < flat.length; i++) {
      assert.ok(
        Object.is(got[i], flat[i]),
        `${theme} ${bg}: index ${i} drifted ${flat[i]} → ${got[i]}`,
      );
    }
  }

  // (2) The `_recheckContrastMulti` prototype (if present — it is an unlisted,
  //     owner-decision method that may be removed) must equal per-sample recheck,
  //     byte-for-byte, over a 3-sample backdrop of the resolved role set.
  const res = engine.resolveTheme("#3A3A3C", "dark");
  const fgSet = Object.values(res.roles)
    .filter((r) => r.kind === "color")
    .map((r) => r.hex);
  const samples = ["#38383A", "#404042", "#2E2E30"];
  if (typeof engine._recheckContrastMulti === "function") {
    const multi = engine._recheckContrastMulti(samples, fgSet, "dark");
    assert.equal(multi.length, samples.length * fgSet.length * 2);
    for (let s = 0; s < samples.length; s++) {
      const per = engine.recheckContrast(samples[s], fgSet, "dark");
      for (let i = 0; i < per.length; i++) {
        const base = s * fgSet.length * 2 + i;
        assert.ok(
          Object.is(multi[base], per[i]),
          `multi sample ${s} index ${i}: ${per[i]} vs ${multi[base]}`,
        );
      }
    }
  }

  // (3) resolveTheme vars are unmoved by the contrast_ratio refactor.
  for (const { theme, bg, vars } of golden.resolveVars) {
    const got = engine.resolveTheme(bg, theme).vars;
    for (const k of Object.keys(vars)) {
      assert.equal(got[k], vars[k], `${theme} ${bg}: var ${k}`);
    }
    assert.equal(Object.keys(got).length, Object.keys(vars).length, `${theme} ${bg}: var count`);
  }
});
