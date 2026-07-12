// Additive-поверхность #292: `numericalCapabilityManifest()` — canonical
// numerical capability manifest сборки, спроецированный из core registry SSOT
// (не рукописная копия). Тест пинит ФОРМУ (camelCase-поля проекции
// conformance-пака), покрытие мигрированного glow-site и формат checksum
// (FNV-1a-32, 8 lowercase hex), но НЕ значение checksum: значение принадлежит
// registry и меняется вместе с ним законно — дрейф формы был бы дефектом
// границы, дрейф значения — свойством ядра.
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

test("numericalCapabilityManifest projects the core capability SSOT", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first (CI builds before `npm test`)");
    return;
  }
  const { initSync, numericalCapabilityManifest } = await import(
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
  assert.equal(manifest.schemaVersion, 1, "capability schema V1");
  assert.equal(manifest.coverage, "migrated-sites-only-v1");
  assert.match(
    manifest.checksum,
    /^[0-9a-f]{8}$/,
    "checksum — FNV-1a-32 в канонической записи: ровно 8 lowercase hex",
  );

  // Rows отсортированы по UTF-8 байтам siteId (инвариант canonical preimage).
  assert.ok(Array.isArray(manifest.sites) && manifest.sites.length > 0);
  const ids = manifest.sites.map((site) => site.siteId);
  assert.deepEqual(ids, [...ids].sort(), "sites отсортированы по siteId");

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
      "runtimeAttestations",
      "siteId",
      "stableOutcomes",
    ],
    "row несёт ровно семь canonical-полей",
  );
  assert.deepEqual(glow.stableOutcomes, ["bit-exact", "indeterminate"]);
  assert.deepEqual(glow.compatibilityReleases, [
    "glow-cam16-ucs-jprime-target-or-max-v1",
  ]);
  assert.deepEqual(glow.evidenceClasses, ["bit-exact"]);
  assert.deepEqual(glow.artifactIds, []);
  assert.deepEqual(glow.boundIds, []);
  assert.deepEqual(glow.runtimeAttestations, []);

  // Манифест — статическое свойство сборки: повторный вызов идентичен.
  assert.deepEqual(numericalCapabilityManifest(), manifest);

  // Package root реэкспортирует ту же функцию (публичная поверхность npm).
  const root = await import(pathToFileURL(resolve(here, "../index.js")).href);
  assert.equal(typeof root.numericalCapabilityManifest, "function");
  assert.deepEqual(root.numericalCapabilityManifest(), manifest);
});
