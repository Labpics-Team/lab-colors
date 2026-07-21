// Byte-identity parity for the JS↔WASM recheck boundary.
//
// `wasm-boundary.golden.json` was snapshotted on the PRE-optimisation engine
// (the display-path `recheck_against`). This test loads the REAL rebuilt wasm and
// asserts every `recheckContrast` result is bit-for-bit what the pre-change
// engine produced — the lock that the transcendental-eliding fast path (WCAG from
// the linear decode) changed performance and nothing else. It also checks that
// the public `recheckContrastMulti` batch call equals N per-sample
// `recheckContrast` calls exactly (the byte-identity the controller's batch path
// relies on), and that a handful of `resolveTheme` vars are unmoved (the
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
  const metaText = JSON.stringify(golden._meta);
  assert.doesNotMatch(
    metaText,
    /\?{3,}|\uFFFD/u,
    "golden provenance не должен содержать следы потери UTF-8",
  );

  const engine = new LabColors();
  engine.loadConfig(CONFIG);

  // C8d packed boundary: recheckContrast/recheckContrastMulti take packed
  // `0x00RRGGBB` words + a `Uint32Array` of foregrounds + a numeric theme handle
  // minted by `themeHandle`. The string overloads are hard-cut. `pk` mirrors the
  // controller's `packRgb24Hex`; the FROZEN golden (captured on the hex path)
  // must still hold byte-for-byte, proving the packed transport changed nothing
  // but the encoding.
  // Mirror the controller's `packRgb24Hex` EXACTLY, including #RGB shorthand
  // expansion (#fff → #FFFFFF, #123 → #112233). The frozen golden was captured
  // on the string boundary, which normalised shorthand before measuring; the
  // packed boundary does pure shifts with no expansion, so the test must expand
  // here to keep byte-identity against the golden's shorthand fixtures.
  const pk = (hex) => {
    const body = hex.charCodeAt(0) === 35 /* '#' */ ? hex.slice(1) : hex;
    const six =
      body.length === 3
        ? body[0] + body[0] + body[1] + body[1] + body[2] + body[2]
        : body;
    return Number.parseInt(six, 16) >>> 0;
  };
  const words = (fgs) => Uint32Array.from(fgs, pk);
  assert.equal(
    typeof engine.themeHandle,
    "function",
    "engine must expose the numeric themeHandle mint",
  );

  // (C1) Every recheck case, packed input: exact f64 equality to the golden the
  //      string boundary produced (Object.is catches ±0 / NaN too).
  for (const { theme, bg, fgs, flat } of golden.recheck) {
    const handle = engine.themeHandle(theme);
    const got = engine.recheckContrast(pk(bg), words(fgs), handle);
    assert.equal(got.length, flat.length, `${theme} ${bg}: length`);
    for (let i = 0; i < flat.length; i++) {
      assert.ok(
        Object.is(got[i], flat[i]),
        `${theme} ${bg}: index ${i} drifted ${flat[i]} → ${got[i]}`,
      );
    }
  }

  // (C2) The public `recheckContrastMulti` batch call must equal per-sample
  //      `recheckContrast`, byte-for-byte, over a 3-sample backdrop of the resolved
  //      role set — the background-major layout the controller's batch path depends
  //      on. Hard assertion (not an `if present` skip): the method is public surface.
  const darkHandle = engine.themeHandle("dark");
  const res = engine.resolveTheme("#3A3A3C", "dark");
  const fgSet = Object.values(res.roles)
    .filter((r) => r.kind === "color")
    .map((r) => r.hex);
  const packedFgs = words(fgSet);
  const samples = ["#38383A", "#404042", "#2E2E30"];
  assert.equal(
    typeof engine.recheckContrastMulti,
    "function",
    "engine must expose the public recheckContrastMulti batch method",
  );
  const multi = engine.recheckContrastMulti(words(samples), packedFgs, darkHandle);
  assert.equal(multi.length, samples.length * fgSet.length * 2);
  for (let s = 0; s < samples.length; s++) {
    const per = engine.recheckContrast(pk(samples[s]), packedFgs, darkHandle);
    for (let i = 0; i < per.length; i++) {
      const base = s * fgSet.length * 2 + i;
      assert.ok(
        Object.is(multi[base], per[i]),
        `multi sample ${s} index ${i}: ${per[i]} vs ${multi[base]}`,
      );
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
