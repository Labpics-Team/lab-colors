import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "../pkg/labcolors_bg.wasm");
const gluePath = resolve(here, "../pkg/labcolors.js");
const haveWasm = existsSync(wasmPath) && existsSync(gluePath);

test("evaluateWcag22 transports exact total core decisions and evidence", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first");
    return;
  }
  const { initSync, evaluateWcag22 } = await import(pathToFileURL(gluePath).href);
  initSync({ module: readFileSync(wasmPath) });
  const vectors = JSON.parse(
    readFileSync(resolve(here, "../../../conformance/vectors/wcag22.json"), "utf8"),
  );
  assert.deepEqual(
    new Set(vectors.map((vector) => vector.criterion)),
    new Set([
      "sc-1.4.3-text-default",
      "sc-1.4.3-text-large-scale",
      "sc-1.4.11-ui-component-or-state",
      "sc-1.4.11-graphical-object",
    ]),
  );
  for (const vector of vectors) {
    const got = evaluateWcag22(vector.foreground, vector.background, vector.criterion);
    assert.equal(got.kind, "evaluated");
    assert.equal(got.profileId, vector.profileId);
    assert.equal(got.criterion, vector.criterion);
    assert.equal(got.foreground, vector.foreground);
    assert.equal(got.background, vector.background);
    assert.equal(got.decision, vector.decision);
    assert.deepEqual(got.foregroundLuminanceQ55, {
      lower: vector.foregroundLowerQ55,
      upper: vector.foregroundUpperQ55,
    });
    assert.deepEqual(got.backgroundLuminanceQ55, {
      lower: vector.backgroundLowerQ55,
      upper: vector.backgroundUpperQ55,
    });
    assert.equal(got.q55Scale, vector.q55Scale);
    assert.deepEqual(got.evidence, {
      kind: vector.evidenceKind,
      artifactId: vector.artifactId,
      artifactSha256: vector.artifactSha256,
      boundId: vector.boundId,
      proofId: vector.proofId,
      proofSha256: vector.proofSha256,
      proofPayloadSha256: vector.proofPayloadSha256,
      generatorSha256: vector.generatorSha256,
      verifierSha256: vector.verifierSha256,
      profileChecksum: vector.profileChecksum,
      profileSha256: vector.profileSha256,
    });
  }

  const root = await import(pathToFileURL(resolve(here, "../index.js")).href);
  assert.equal(typeof root.evaluateWcag22, "function");
  const belowThree = vectors.find(
    (vector) => vector.criterion === "sc-1.4.11-ui-component-or-state",
  );
  assert.deepEqual(
    root.evaluateWcag22(belowThree.foreground, belowThree.background, belowThree.criterion),
    evaluateWcag22(belowThree.foreground, belowThree.background, belowThree.criterion),
  );
});

test("evaluateWcag22 rejects invalid criterion and colour without fallback", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first");
    return;
  }
  const { initSync, evaluateWcag22 } = await import(pathToFileURL(gluePath).href);
  initSync({ module: readFileSync(wasmPath) });
  assert.throws(
    () => evaluateWcag22("#000000", "#FFFFFF", "danger"),
    /unknown_wcag22_criterion/u,
  );
  for (const invalid of ["invalid", "FFFFFF", "##FFFFFF", " #FFFFFF"])
    assert.throws(
      () => evaluateWcag22(invalid, "#FFFFFF", "sc-1.4.3-text-default"),
      /invalid_color/u,
    );
});
