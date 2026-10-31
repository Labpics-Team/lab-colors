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
    "point-support-retained-reference-surplus-v1",
    "wcag22-srgb8-contrast-v1",
  ]);

  // Удалённый рецепт не рекламируется как возможность рабочего пакета.
  assert.equal(ids.includes("glow-target-or-maximum-v1"), false);
  for (const site of manifest.sites) {
    assert.deepEqual(Object.keys(site).sort(), ["artifactIds", "boundIds",
      "compatibilityReleases", "evidenceClasses", "proofIds", "runtimeAttestations",
      "siteId", "stableOutcomes"]);
    assert.deepEqual(site.compatibilityReleases, []);
  }

  const pointSupport = manifest.sites.find(
    (site) => site.siteId === "point-support-retained-reference-surplus-v1",
  );
  assert.deepEqual(pointSupport, {
    siteId: "point-support-retained-reference-surplus-v1",
    stableOutcomes: ["canonical-finite-bounded"],
    compatibilityReleases: [],
    evidenceClasses: ["canonical-finite-bounded"],
    artifactIds: ["wcag22-srgb8-luminance-q55-v1"],
    boundIds: ["point-support-reference-surplus-q55-bps-v1"],
    proofIds: ["point-support-reference-surplus-integer-v1"],
    runtimeAttestations: [],
  });

  const wcag22 = manifest.sites.find(
    (site) => site.siteId === "wcag22-srgb8-contrast-v1",
  );
  assert.deepEqual(wcag22, {
    siteId: "wcag22-srgb8-contrast-v1",
    stableOutcomes: ["canonical-finite-bounded"],
    compatibilityReleases: [],
    evidenceClasses: ["canonical-finite-bounded"],
    artifactIds: ["wcag22-srgb8-luminance-q55-v1"],
    boundIds: ["wcag22-srgb8-outward-q55-v1"],
    proofIds: ["wcag22-srgb8-full-domain-q55-v1"],
    runtimeAttestations: [],
  });

  // Манифест — статическое свойство сборки: повторный вызов идентичен.
  assert.deepEqual(numericalCapabilityManifest(), manifest);

  // Package root реэкспортирует ту же функцию (публичная поверхность npm).
  const root = await import(pathToFileURL(resolve(here, "../index.js")).href);
  assert.equal(typeof root.numericalCapabilityManifest, "function");
  assert.deepEqual(root.numericalCapabilityManifest(), manifest);
  assert.equal(numericalCapabilityManifestV2, undefined);
  assert.equal(root.numericalCapabilityManifestV2, undefined);

  await t.test("terminal projections preserve data ownership and property semantics", () => {
    const evaluate = () =>
      root.evaluateWcag22("#00000a", "#FFFFFF", "sc-1.4.3-text-default");
    // Independent snapshots must not alias a future cached projection.
    const expectedManifest = structuredClone(numericalCapabilityManifest());
    const expectedAssessment = structuredClone(evaluate());
    const poisoned = [
      [Object.prototype, "schemaVersion"],
      [Object.prototype, "siteId"],
      [Object.prototype, "kind"],
      [Object.prototype, "lower"],
      [Object.prototype, "get"],
      [Object.prototype, "set"],
      [Array.prototype, "0"],
    ];
    const previous = poisoned.map(([object, key]) => {
      const descriptor = Object.getOwnPropertyDescriptor(object, key);
      if (descriptor !== undefined) Object.setPrototypeOf(descriptor, null);
      return descriptor;
    });
    let setterCalls = 0;
    const poison = {
      __proto__: null,
      configurable: true,
      set() {
        setterCalls += 1;
        throw new Error("inherited projection setter");
      },
    };
    let projectedManifest;
    let assessment;
    try {
      for (const [object, key] of poisoned) {
        Object.defineProperty(object, key, poison);
      }
      projectedManifest = numericalCapabilityManifest();
      assessment = evaluate();
    } finally {
      for (let index = 0; index < poisoned.length; index += 1) {
        const [object, key] = poisoned[index];
        if (previous[index] === undefined) {
          delete object[key];
        } else {
          Object.defineProperty(object, key, previous[index]);
        }
      }
    }

    assert.equal(setterCalls, 0);
    assert.deepEqual(projectedManifest, expectedManifest);
    assert.deepEqual(assessment, expectedAssessment);
    assert.equal(assessment.foreground, "#00000A");
    assert.equal(assessment.q55Scale, (1n << 55n).toString(), "exact Q55 scale");

    assert.deepEqual(Object.keys(projectedManifest), [
      "schemaVersion", "coverage", "sites", "checksum",
    ]);
    for (const site of projectedManifest.sites) {
      assert.deepEqual(Object.keys(site), [
        "siteId", "stableOutcomes", "compatibilityReleases", "evidenceClasses",
        "artifactIds", "boundIds", "proofIds", "runtimeAttestations",
      ]);
    }
    assert.deepEqual(Object.keys(assessment), [
      "kind", "profileId", "criterion", "foreground", "background",
      "foregroundLuminanceQ55", "backgroundLuminanceQ55", "q55Scale",
      "decision", "evidence",
    ]);
    assert.deepEqual(Object.keys(assessment.foregroundLuminanceQ55), ["lower", "upper"]);
    assert.deepEqual(Object.keys(assessment.backgroundLuminanceQ55), ["lower", "upper"]);
    assert.deepEqual(Object.keys(assessment.evidence), [
      "kind", "artifactId", "artifactSha256", "boundId", "proofId",
      "proofSha256", "proofPayloadSha256", "generatorSha256", "verifierSha256",
      "profileChecksum", "profileSha256",
    ]);

    const assertDataProperties = (value) => {
      if (value === null || typeof value !== "object") return;
      assert.equal(
        Object.getPrototypeOf(value),
        Array.isArray(value) ? Array.prototype : Object.prototype,
      );
      assert.deepEqual(Object.getOwnPropertySymbols(value), []);
      for (const key of Object.keys(value)) {
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        assert.equal(descriptor.get, undefined);
        assert.equal(descriptor.set, undefined);
        assert.equal(descriptor.writable, true);
        assert.equal(descriptor.enumerable, true);
        assert.equal(descriptor.configurable, true);
        assertDataProperties(descriptor.value);
      }
    };
    assertDataProperties(projectedManifest);
    assertDataProperties(assessment);

    projectedManifest.sites[0].siteId = "caller-owned";
    projectedManifest.sites[0].stableOutcomes[0] = "caller-owned";
    assessment.foregroundLuminanceQ55.lower = "caller-owned";
    assessment.evidence.artifactId = "caller-owned";
    assert.deepEqual(numericalCapabilityManifest(), expectedManifest);
    assert.deepEqual(evaluate(), expectedAssessment);
  });
});
