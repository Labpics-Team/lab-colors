import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  MEASUREMENT_ARTIFACT_ID,
  SCENARIO_IDS,
  validateMeasurementArtifact,
} from "../bench/wcag22-feasibility-boundary.bench.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const corePath = resolve(
  root,
  "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v5.json",
);
const toolchainPath = resolve(root, "packages/colors/bench/wasm-size-budget-v1.json");
const harnessPath = resolve(
  root,
  "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs",
);
const packPath = resolve(root, "conformance/vectors/wcag22-feasibility.json");
const conformanceManifestPath = resolve(root, "conformance/vectors/manifest.json");
const packageManifestPath = resolve(root, "packages/colors/package.json");
const compilerEntryPath = resolve(root, "packages/colors/compiler.js");
const wasmGluePath = resolve(root, "packages/colors/compiler/labcolors_compiler.js");
const wasmBudgetPath = resolve(root, "packages/colors/bench/wasm-size-budget-v6.json");
const coreBytes = readFileSync(corePath);
const core = JSON.parse(coreBytes);
const toolchain = JSON.parse(readFileSync(toolchainPath));
const wasmBudgetBytes = readFileSync(wasmBudgetPath);
const wasmBudget = JSON.parse(wasmBudgetBytes);
const packBytes = readFileSync(packPath);
const pack = JSON.parse(packBytes);
const conformanceManifestBytes = readFileSync(conformanceManifestPath);
const conformanceManifest = JSON.parse(conformanceManifestBytes);
const ciSource = readFileSync(resolve(root, ".github/workflows/ci.yml"), "utf8");
const canonicalNodeVersion = `v${ciSource.match(/^\s{2}NODE_TOOLCHAIN: (\d+\.\d+\.\d+)$/mu)[1]}`;
const nativeByName = new Map(core.scenarios.map((scenario) => [scenario.name, scenario]));
const packOracleCase = pack.find((entry) => entry.caseId === "text-default-seven");
const sha256 = (value) => createHash("sha256").update(value).digest("hex");

function toolchainRecipe() {
  const measurement = toolchain.measurement;
  return {
    rustToolchain: measurement.rustToolchain,
    rustcCommit: measurement.rustcCommit,
    wasmPack: measurement.wasmPack,
    wasmBindgen: measurement.wasmBindgen,
    target: measurement.target,
    cargoProfile: measurement.cargoProfile,
    wasmOpt: measurement.wasmOpt,
    wasmOptVersion: measurement.wasmOptVersion,
    measurementPlatform: measurement.measurementPlatform,
    rustPathRemap: measurement.rustPathRemap,
    command: wasmBudget.buildRecipes.compiler.command,
  };
}

function maxRequestBytes() {
  // Independent test oracle only. Production measurement takes this value
  // exclusively from the initialized compiler-entry getter, then binds the exact
  // WASM plus limit/limit+1 witnesses; this mirror mutation-kills a fabricated
  // artifact limit without making JS a second production authority.
  const limits = core.profileLimits;
  return (
    101 +
    115 * limits.rawRelations +
    14 * limits.rawAdjacentEntries +
    6 * limits.opaqueUtf8Bytes
  );
}

function summaryFor(scenarioId, shape) {
  if (scenarioId === "transport-limit-plus-one") {
    return {
      outcome: "failure",
      source: "transport",
      code: "envelopeTooLarge",
      requestedBytes: String(maxRequestBytes() + 1),
      limitBytes: String(maxRequestBytes()),
      proportionalFieldsPresent: false,
    };
  }
  if (
    scenarioId === "maximum-opaque-utf8-bytes" ||
    scenarioId === "maximum-canonical-not-applicable-relations" ||
    scenarioId === "maximum-combined-not-applicable-envelope"
  ) {
    return {
      outcome: "success",
      terminal: "notEvaluated",
      canonicalRelations: String(shape.canonicalRelations),
      applicableRelations: "0",
      notApplicableRelations: String(shape.canonicalRelations),
      numericalEvidencePresent: false,
      proportionalFieldsPresent: false,
    };
  }
  return {
    outcome: "success",
    terminal: "feasible",
    domainCount: "256",
    canonicalRelations: String(shape.canonicalRelations),
    applicableRelations: String(shape.applicableRelations),
    notApplicableRelations: String(shape.canonicalRelations - shape.applicableRelations),
    applicableEdges: String(shape.applicableEdges),
    logicalAssessments: String(256 * shape.applicableEdges),
    failureMatrixBytes: 32 * shape.applicableEdges,
    partitionBytes: 32,
    feasibleCandidates: 1,
    lsb0PartitionMatchesMatrix: true,
    ...(scenarioId === "minimum-evaluated" ? { lsb0PackOracleMatches: true } : {}),
    proportionalFieldsPresent: false,
  };
}

function fixture() {
  const limits = {
    maxRequestBytes: maxRequestBytes(),
    ...core.profileLimits,
  };
  const scenarios = SCENARIO_IDS.map((scenarioId, scenarioIndex) => {
    const sourceName =
      scenarioId === "transport-limit-plus-one"
        ? "maximum-combined-applicable-envelope"
        : scenarioId;
    const shape = structuredClone(nativeByName.get(sourceName).shape);
    const requestBytes =
      scenarioId === "maximum-combined-applicable-envelope"
        ? limits.maxRequestBytes
        : scenarioId === "transport-limit-plus-one"
          ? limits.maxRequestBytes + 1
          : 100 + scenarioIndex;
    const requestSha256 = sha256(`request:${scenarioId}`);
    const outcomeSha256 = sha256(`outcome:${scenarioId}`);
    const summary = summaryFor(scenarioId, shape);
    return {
      scenarioId,
      shape,
      samples: Array.from({ length: core.sampleCount }, (_, sampleIndex) => ({
        sampleIndex,
        initSyncElapsedNs: String(sampleIndex + 1),
        elapsedNs: String(sampleIndex + 1),
        requestBytes,
        requestSha256,
        outcomeBytes: 200 + scenarioIndex,
        outcomeSha256,
        summary: structuredClone(summary),
        processMaxRssKiBBeforeInit: 900,
        processMaxRssKiBAfterInit: 950,
        processMaxRssKiBBefore: 1_000,
        processMaxRssKiBAfter: 1_001 + sampleIndex,
        wasmMemoryBytesAfterInit: 65_536,
        wasmMemoryBytesBefore: 65_536,
        wasmMemoryBytesAfter: 65_536 * 2,
        wasmMemoryPagesAfterInit: 1,
        wasmMemoryPagesBefore: 1,
        wasmMemoryPagesAfter: 2,
      })),
    };
  });
  return {
    schemaVersion: 1,
    artifactId: MEASUREMENT_ARTIFACT_ID,
    claimBoundary: "canonical-wasm-compiler-entry-whole-call-observations-only",
    claims: {
      admission: "canonical-linux-x64-exact-wasm-only",
      hardGates: [
        "completion",
        "request-and-outcome-bytes",
        "sha256-binding",
        "terminal-algebra",
        "packed-shape",
        "candidate-major-lsb0-pack-oracle",
        "no-proportional-dto",
      ],
      timingThresholdNs: null,
      latency: "init-sync-and-warm-operation-observations-only-no-production-threshold",
      memory:
        "process maxRSS values are total-process high-water including V8 and prior warm-up/observer allocations; after-init and warm-call WASM pages are linear-memory high-water observations; neither is total operation memory",
    },
    environment: {
      execution: "fresh-node-child-process-per-sample",
      initSyncScope:
        "initSync-from-in-memory-compiler-wasm-includes-wasm-bindgen-startup-excludes-io-and-js-module-import",
      operationScope:
        "second-identical-operation-after-one-unmeasured-warm-up-whose-result-graph-is-not-retained-by-harness",
      platform: "linux-x64",
      nodeVersion: canonicalNodeVersion,
      sampleCount: core.sampleCount,
      requestConstructionMeasured: false,
      timer: "process.hrtime.bigint",
      publicEntry: "packages/colors/compiler.js",
      canonicalCandidate: true,
      rustToolchain: toolchain.measurement.rustToolchain,
      wasmPack: toolchain.measurement.wasmPack,
      wasmBindgen: toolchain.measurement.wasmBindgen,
      target: toolchain.measurement.target,
      cargoProfile: toolchain.measurement.cargoProfile,
      wasmOpt: toolchain.measurement.wasmOpt,
    },
    bindings: {
      coreAdmission: {
        path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v5.json",
        schemaVersion: core.schemaVersion,
        artifactId: core.artifactId,
        profileId: core.profileLimits.profileId,
        sha256: sha256(coreBytes),
      },
      packOracle: {
        path: "conformance/vectors/wcag22-feasibility.json",
        sha256: sha256(packBytes),
        caseId: packOracleCase.caseId,
        requestSha256: sha256(Buffer.from(packOracleCase.requestJson, "utf8")),
        outcomeSha256: sha256(Buffer.from(packOracleCase.outcomeJson, "utf8")),
        manifestPath: "conformance/vectors/manifest.json",
        manifestSha256: sha256(conformanceManifestBytes),
        packVersion: conformanceManifest.packVersion,
        packDigest: conformanceManifest.packDigest,
      },
      compilerSources: {
        harness: {
          path: "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs",
          sha256: sha256(readFileSync(harnessPath)),
        },
        packageManifest: {
          path: "packages/colors/package.json",
          sha256: sha256(readFileSync(packageManifestPath)),
        },
        compilerEntry: {
          path: "packages/colors/compiler.js",
          sha256: sha256(readFileSync(compilerEntryPath)),
        },
        wasmGlue: {
          path: "packages/colors/compiler/labcolors_compiler.js",
          sha256: sha256(readFileSync(wasmGluePath)),
        },
      },
      wasmBudget: {
        path: "packages/colors/bench/wasm-size-budget-v6.json",
        schemaVersion: wasmBudget.schemaVersion,
        budgetId: wasmBudget.budgetId,
        fileSha256: sha256(wasmBudgetBytes),
        role: "compiler",
        recipeSha256: wasmBudget.buildRecipes.compiler.recipeSha256,
      },
      wasm: {
        path: "packages/colors/compiler/labcolors_compiler_bg.wasm",
        bytes: wasmBudget.roles.compiler.measurement.rawBytes,
        sha256: wasmBudget.roles.compiler.measurement.sha256,
      },
    },
    limits,
    scenarios,
  };
}

test("whole-call evidence history is exact and deterministic", () => {
  const v1Bytes = readFileSync(resolve(
    root,
    "packages/colors/bench/wcag22-feasibility-wasm-boundary-v1.json",
  ));
  const v2Bytes = readFileSync(resolve(
    root,
    "packages/colors/bench/wcag22-feasibility-wasm-boundary-v2.json",
  ));
  const v3Bytes = readFileSync(resolve(
    root,
    "packages/colors/bench/wcag22-feasibility-wasm-boundary-v3.json",
  ));
  const v4Bytes = readFileSync(resolve(
    root,
    "packages/colors/bench/wcag22-feasibility-wasm-boundary-v4.json",
  ));
  assert.equal(
    sha256(v1Bytes),
    "8281f372cf635174fa3cedf828a96b48a023c413f43245cfc7001d9b83ff1790",
  );
  assert.equal(
    sha256(v2Bytes),
    "3b4ec73fc09eeee03a96fa785fe7c4c6af419965b74b9e454f1378cf3170d888",
  );
  assert.equal(
    sha256(v3Bytes),
    "60e0b0f621fb4e0fcc5c57c527a8f1bf11487ee34581c98239b1ac6c31e6de86",
  );
  assert.equal(
    sha256(v4Bytes),
    "34fcc24d74c1c0b877c04457799d5d67b8947915199a14202f737353bfff4257",
  );
  const v1 = JSON.parse(v1Bytes);
  const v2 = JSON.parse(v2Bytes);
  const v3 = JSON.parse(v3Bytes);
  const v4 = JSON.parse(v4Bytes);
  assert.equal(v1.artifactId, "wcag22-feasibility-wasm-whole-call-v1");
  assert.equal(v2.artifactId, "wcag22-feasibility-wasm-whole-call-v2");
  assert.equal(v3.artifactId, "wcag22-feasibility-wasm-whole-call-v3");
  assert.equal(v4.artifactId, "wcag22-feasibility-wasm-whole-call-v4");
  // V4 перезаписан после pack-6/admission-V5: связывает точную новую истину,
  // компилерный WASM байт-в-байт равен ратчету C1.
  assert.deepEqual(v4.bindings.coreAdmission, {
    path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v5.json",
    schemaVersion: 1,
    artifactId: "wcag22-feasibility-admission-raw-v5",
    profileId: "compile-v1",
    sha256: "6079612797bb28a9fc97c1451efd830e2323ed06e15fbffd70527dddc3fa84c5",
  });
  assert.deepEqual(v4.bindings.wasm, {
    path: "packages/colors/compiler/labcolors_compiler_bg.wasm",
    bytes: 175212,
    sha256: "3a552ce43ada7d0b10e90a23b4a7e50a4ecad77a446374b98ca8ee6b5c6a2a45",
  });
  assert.equal(v4.bindings.packOracle.packVersion, "6.0.0");
  assert.deepEqual(v2.bindings.coreAdmission, {
    path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v3.json",
    schemaVersion: 1,
    artifactId: "wcag22-feasibility-admission-raw-v3",
    profileId: "compile-v1",
    sha256: "46ec939523a9aff4f253c4c74e997dfd95812a694b2507fae885ff60244ade3a",
  });
  assert.deepEqual(v2.bindings.wasm, {
    path: "packages/colors/pkg/labcolors_bg.wasm",
    bytes: 520920,
    sha256: "c179f42cd90c24699167ee78b4080c80fb38247c54953e7dc020483f6fcf94ed",
  });
  assert.deepEqual(v3.bindings.coreAdmission, {
    path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v4.json",
    schemaVersion: 1,
    artifactId: "wcag22-feasibility-admission-raw-v4",
    profileId: "compile-v1",
    sha256: "3c257c336bc403eee933990fd7188a3b0a6e89d0cbc983aff18846ef76206275",
  });
  assert.deepEqual(v3.bindings.wasm, {
    path: "packages/colors/compiler/labcolors_compiler_bg.wasm",
    bytes: 175212,
    sha256: "3a552ce43ada7d0b10e90a23b4a7e50a4ecad77a446374b98ca8ee6b5c6a2a45",
  });

  const deterministicProjection = (artifact) => ({
    limits: artifact.limits,
    scenarios: artifact.scenarios.map((scenario) => ({
      scenarioId: scenario.scenarioId,
      shape: scenario.shape,
      samples: scenario.samples.map((sample) => ({
        sampleIndex: sample.sampleIndex,
        requestBytes: sample.requestBytes,
        requestSha256: sample.requestSha256,
        outcomeBytes: sample.outcomeBytes,
        outcomeSha256: sample.outcomeSha256,
        summary: sample.summary,
      })),
    })),
  });
  assert.deepEqual(deterministicProjection(v2), deterministicProjection(v1));
  assert.deepEqual(deterministicProjection(v3), deterministicProjection(v2));
  assert.deepEqual(deterministicProjection(v4), deterministicProjection(v3));
});

test("canonical whole-call artifact schema accepts all immutable scenarios", () => {
  const artifact = fixture();
  assert.doesNotThrow(() => validateMeasurementArtifact(artifact));
  assert.equal(artifact.claims.timingThresholdNs, null);
  assert.equal(artifact.scenarios.length, 10);
});

test("whole-call checker mutation-kills missing evidence and inflated claims", () => {
  const mutations = [
    ["missing scenario", (artifact) => { artifact.scenarios.pop(); }],
    ["guessed timing gate", (artifact) => { artifact.claims.timingThresholdNs = 1; }],
    ["total-memory overclaim", (artifact) => {
      artifact.claims.memory = "total operation memory";
    }],
    ["unstable outcome digest", (artifact) => {
      artifact.scenarios[0].samples[1].outcomeSha256 = "b".repeat(64);
    }],
    ["LSB0 unchecked", (artifact) => {
      artifact.scenarios[0].samples[0].summary.lsb0PartitionMatchesMatrix = false;
    }],
    ["pack-5 LSB0 oracle unchecked", (artifact) => {
      artifact.scenarios[0].samples[0].summary.lsb0PackOracleMatches = false;
    }],
    ["matrix law drift", (artifact) => {
      artifact.scenarios[1].samples[0].summary.failureMatrixBytes -= 1;
    }],
    ["non-canonical host", (artifact) => {
      artifact.environment.platform = "darwin-arm64";
      artifact.environment.canonicalCandidate = false;
    }],
    ["fabricated public ceiling", (artifact) => {
      artifact.limits.maxRequestBytes += 1;
    }],
    ["missing fresh-process sample", (artifact) => {
      artifact.scenarios[0].samples.pop();
    }],
    ["stale measurement harness", (artifact) => {
      artifact.bindings.compilerSources.harness.sha256 = "b".repeat(64);
    }],
    ["stale package root", (artifact) => {
      artifact.bindings.compilerSources.compilerEntry.sha256 = "b".repeat(64);
    }],
    ["stale generated glue", (artifact) => {
      artifact.bindings.compilerSources.wasmGlue.sha256 = "b".repeat(64);
    }],
    ["runtime source inserted", (artifact) => {
      artifact.bindings.compilerSources.packageRoot = {
        path: "packages/colors/index.js",
        sha256: "b".repeat(64),
      };
    }],
    ["stale role budget", (artifact) => {
      artifact.bindings.wasmBudget.fileSha256 = "b".repeat(64);
    }],
    ["wrong execution role", (artifact) => {
      artifact.bindings.wasmBudget.role = "runtime";
    }],
    ["wrong compiler recipe", (artifact) => {
      artifact.bindings.wasmBudget.recipeSha256 = "b".repeat(64);
    }],
  ];
  assert.doesNotThrow(() => validateMeasurementArtifact(fixture()));
  assert.equal(mutations.length, 17, "anti-vacuum mutation set changed");
  for (const [name, mutate] of mutations) {
    const artifact = fixture();
    mutate(artifact);
    assert.throws(
      () => validateMeasurementArtifact(artifact),
      undefined,
      `${name} must fail the checker`,
    );
  }
});

test("CI verifies committed whole-call evidence before the size gate", () => {
  const ci = ciSource;
  const harness = "node bench/wcag22-feasibility-boundary.bench.mjs";
  const evidence = "bench/wcag22-feasibility-wasm-boundary-v4.json";
  const verify = `--verify ${evidence}`;
  const fingerprint = "name: independently fingerprint both execution-role WASM artifacts";
  const upload =
    "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02";
  const sizeGate = "node scripts/check-wasm-size-budget.mjs";
  const harnessIndex = ci.indexOf(harness);
  const verifyIndex = ci.indexOf(verify, harnessIndex);
  const fingerprintIndex = ci.indexOf(fingerprint, verifyIndex);
  const uploadIndex = ci.indexOf(upload, verifyIndex);
  const sizeGateIndex = ci.indexOf(sizeGate);
  assert.match(ci, /verify committed #296-C2 canonical whole-call compiler evidence/u);
  assert.match(
    ci,
    /name: upload exact #296-C2 whole-call evidence/u,
  );
  assert.ok(harnessIndex >= 0, "the compiler-entry harness must run in CI");
  assert.ok(verifyIndex > harnessIndex, "CI must verify the committed evidence");
  assert.ok(
    fingerprintIndex > verifyIndex,
    "an independent system tool must fingerprint the verified WASM",
  );
  assert.ok(uploadIndex > fingerprintIndex, "upload must follow the independent fingerprint");
  assert.ok(
    sizeGateIndex > uploadIndex,
    "the retrievable evidence must precede the immutable size gate",
  );
  assert.match(
    ci,
    /path: \|[\s\S]*?packages\/colors\/bench\/wcag22-feasibility-wasm-boundary-v4\.json[\s\S]*?packages\/colors\/pkg\/labcolors_bg\.wasm[\s\S]*?packages\/colors\/compiler\/labcolors_compiler_bg\.wasm/u,
  );
  assert.doesNotMatch(
    ci,
    /wcag22-feasibility-boundary\.bench\.mjs --record/u,
    "CI must never mint a new whole-call truth from the revision under test",
  );
  assert.match(ci, /if-no-files-found: error/u);
  assert.match(
    readFileSync(resolve(root, "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs"), "utf8"),
    /--verify/u,
    "the same harness must expose the post-record hard-verify path",
  );
});
