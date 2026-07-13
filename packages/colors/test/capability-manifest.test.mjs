// Capability schema is independently versioned. Before public clients exist,
// the one public projection moves atomically to proof-capable V2 rather than
// preserving two competing entrypoints.
//
// Requires the built `pkg/` (CI runs `npm test` after `wasm-pack build`).
// Skips cleanly if the wasm bundle is absent, matching the other wasm tests.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "../pkg/labcolors_bg.wasm");
const gluePath = resolve(here, "../pkg/labcolors.js");

const haveWasm = existsSync(wasmPath) && existsSync(gluePath);

test("numericalCapabilityManifest publishes the single proof-capable V2 contract", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first (CI builds before `npm test`)");
    return;
  }
  const { initSync, numericalCapabilityManifest, numericalCapabilityManifestV2 } = await import(
    pathToFileURL(gluePath).href
  );
  initSync({ module: readFileSync(wasmPath) });

  const manifest = numericalCapabilityManifest();

  // Верхний уровень: та же форма, что numericalCapabilities conformance-пака.
  assert.deepEqual(
    Object.keys(manifest).sort(),
    ["checksum", "coverage", "schemaVersion", "sites"],
    "верхний уровень несёт ровно четыре canonical-поля",
  );
  assert.equal(manifest.schemaVersion, 2, "capability schema V2");
  assert.equal(manifest.coverage, "migrated-sites-only-v1");
  assert.match(
    manifest.checksum,
    /^[0-9a-f]{8}$/,
    "checksum — FNV-1a-32 в канонической записи: ровно 8 lowercase hex",
  );

  // Rows отсортированы по UTF-8 байтам siteId (инвариант canonical preimage).
  assert.ok(Array.isArray(manifest.sites));
  const ids = manifest.sites.map((site) => site.siteId);
  assert.deepEqual(ids, [
    "glow-target-or-maximum-v1",
    "wcag22-srgb8-contrast-v1",
  ]);

  // Мигрированный glow-site: точное содержимое registry-строки. Пустые
  // массивы обязаны быть явными [] — пусто значит «нет evidence», не
  // «поле потерялось на границе».
  const glow = manifest.sites.find(
    (site) => site.siteId === "glow-target-or-maximum-v1",
  );
  assert.ok(glow, "манифест обязан покрывать glow site");
  assert.deepEqual(
    Object.keys(glow).sort(),
    [
      "artifactIds",
      "boundIds",
      "compatibilityReleases",
      "evidenceClasses",
      "proofIds",
      "runtimeAttestations",
      "siteId",
      "stableOutcomes",
    ],
    "V2 row несёт ровно восемь canonical-полей",
  );
  assert.deepEqual(glow.stableOutcomes, ["bit-exact", "indeterminate"]);
  assert.deepEqual(glow.compatibilityReleases, [
    "glow-cam16-ucs-jprime-target-or-max-v1",
  ]);
  assert.deepEqual(glow.evidenceClasses, ["bit-exact"]);
  assert.deepEqual(glow.artifactIds, []);
  assert.deepEqual(glow.boundIds, []);
  assert.deepEqual(glow.proofIds, []);
  assert.deepEqual(glow.runtimeAttestations, []);

  const wcag22 = manifest.sites[1];
  assert.deepEqual(wcag22.stableOutcomes, ["canonical-finite-bounded"]);
  assert.deepEqual(wcag22.evidenceClasses, ["canonical-finite-bounded"]);
  assert.deepEqual(wcag22.artifactIds, ["wcag22-srgb8-luminance-q55-v1"]);
  assert.deepEqual(wcag22.boundIds, ["wcag22-srgb8-outward-q55-v1"]);
  assert.deepEqual(wcag22.proofIds, ["wcag22-srgb8-full-domain-q55-v1"]);

  // Манифест — статическое свойство сборки: повторный вызов идентичен.
  assert.deepEqual(numericalCapabilityManifest(), manifest);

  // Package root реэкспортирует ту же функцию (публичная поверхность npm).
  const root = await import(pathToFileURL(resolve(here, "../index.js")).href);
  assert.equal(typeof root.numericalCapabilityManifest, "function");
  assert.deepEqual(root.numericalCapabilityManifest(), manifest);
  assert.equal(numericalCapabilityManifestV2, undefined);
  assert.equal(root.numericalCapabilityManifestV2, undefined);
});
