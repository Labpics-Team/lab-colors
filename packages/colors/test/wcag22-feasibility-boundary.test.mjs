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
  "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v3.json",
);
const toolchainPath = resolve(root, "packages/colors/bench/wasm-size-budget-v1.json");
const harnessPath = resolve(
  root,
  "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs",
);
const packPath = resolve(root, "conformance/vectors/wcag22-feasibility.json");
const conformanceManifestPath = resolve(root, "conformance/vectors/manifest.json");
const packageManifestPath = resolve(root, "packages/colors/package.json");
const packageRootPath = resolve(root, "packages/colors/index.js");
const wasmGluePath = resolve(root, "packages/colors/pkg/labcolors.js");
const eagerRuntimePaths = {
  adaptTheme: "adapt-theme.js",
  applyTheme: "apply-theme.js",
  effectiveBackground: "effective-bg.js",
  watchTheme: "watch-theme.js",
};
const coreBytes = readFileSync(corePath);
const core = JSON.parse(coreBytes);
const toolchain = JSON.parse(readFileSync(toolchainPath));
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
    command: measurement.command,
  };
}

function maxRequestBytes() {
  // Independent test oracle only. Production measurement takes this value
  // exclusively from the initialized package-root getter, then binds the exact
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
        elapsedNs: String(sampleIndex + 1),
        requestBytes,
        requestSha256,
        outcomeBytes: 200 + scenarioIndex,
        outcomeSha256,
        summary: structuredClone(summary),
        processMaxRssKiBBefore: 1_000,
        processMaxRssKiBAfter: 1_001 + sampleIndex,
        wasmMemoryBytesBefore: 65_536,
        wasmMemoryBytesAfter: 65_536 * 2,
        wasmMemoryPagesBefore: 1,
        wasmMemoryPagesAfter: 2,
      })),
    };
  });
  return {
    schemaVersion: 1,
    artifactId: MEASUREMENT_ARTIFACT_ID,
    claimBoundary: "canonical-wasm-package-root-whole-call-observations-only",
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
      latency: "observation-only-no-production-threshold",
      memory:
        "process maxRSS is total-process high-water including V8; post-call WASM pages are linear-memory high-water observations; neither is total operation memory",
    },
    environment: {
      execution: "fresh-node-child-process-per-sample",
      platform: "linux-x64",
      nodeVersion: canonicalNodeVersion,
      sampleCount: core.sampleCount,
      requestConstructionMeasured: false,
      timer: "process.hrtime.bigint",
      packageRootApi: "packages/colors/index.js",
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
        path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v3.json",
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
      runtimeSources: {
        harness: {
          path: "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs",
          sha256: sha256(readFileSync(harnessPath)),
        },
        packageManifest: {
          path: "packages/colors/package.json",
          sha256: sha256(readFileSync(packageManifestPath)),
        },
        packageRoot: {
          path: "packages/colors/index.js",
          sha256: sha256(readFileSync(packageRootPath)),
        },
        wasmGlue: {
          path: "packages/colors/pkg/labcolors.js",
          sha256: sha256(readFileSync(wasmGluePath)),
        },
        ...Object.fromEntries(
          Object.entries(eagerRuntimePaths).map(([sourceId, filename]) => [
            sourceId,
            {
              path: `packages/colors/${filename}`,
              sha256: sha256(readFileSync(resolve(root, "packages/colors", filename))),
            },
          ]),
        ),
      },
      wasmToolchain: {
        path: "packages/colors/bench/wasm-size-budget-v1.json",
        schemaVersion: toolchain.schemaVersion,
        budgetId: toolchain.budgetId,
        recipeSha256: sha256(Buffer.from(JSON.stringify(toolchainRecipe()), "utf8")),
      },
      wasm: {
        path: "packages/colors/pkg/labcolors_bg.wasm",
        bytes: 500_000,
        sha256: "a".repeat(64),
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
  assert.equal(
    sha256(v1Bytes),
    "8281f372cf635174fa3cedf828a96b48a023c413f43245cfc7001d9b83ff1790",
  );
  assert.equal(
    sha256(v2Bytes),
    "dc7224f99fd243ba6bcf3759ea666414a51740ac51f9b9886956adad056dfc43",
  );
  const v1 = JSON.parse(v1Bytes);
  const v2 = JSON.parse(v2Bytes);
  assert.equal(v1.artifactId, "wcag22-feasibility-wasm-whole-call-v1");
  assert.equal(v2.artifactId, MEASUREMENT_ARTIFACT_ID);
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
      artifact.bindings.runtimeSources.harness.sha256 = "b".repeat(64);
    }],
    ["stale package root", (artifact) => {
      artifact.bindings.runtimeSources.packageRoot.sha256 = "b".repeat(64);
    }],
    ["stale generated glue", (artifact) => {
      artifact.bindings.runtimeSources.wasmGlue.sha256 = "b".repeat(64);
    }],
  ];
  assert.equal(mutations.length, 13, "anti-vacuum mutation set changed");
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

test("final CI verifies and uploads committed Linux evidence before the size gate", () => {
  const ci = ciSource;
  const harness = "node bench/wcag22-feasibility-boundary.bench.mjs";
  const verify = "\n          --verify";
  const fingerprint = "sha256sum packages/colors/pkg/labcolors_bg.wasm";
  const upload =
    "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02";
  const sizeGate = "node scripts/check-wasm-size-budget.mjs";
  const harnessIndex = ci.indexOf(harness);
  const verifyIndex = ci.indexOf(verify, harnessIndex);
  const fingerprintIndex = ci.indexOf(fingerprint, verifyIndex);
  const uploadIndex = ci.indexOf(upload, verifyIndex);
  const sizeGateIndex = ci.indexOf(sizeGate);
  assert.match(ci, /verify committed #296-B canonical whole-call WASM boundary evidence/u);
  assert.match(
    ci,
    /name: "upload exact #296-B verified whole-call evidence"/u,
  );
  assert.ok(harnessIndex >= 0, "the package-root harness must run in CI");
  assert.ok(verifyIndex > harnessIndex, "CI must rerun the committed evidence verifier");
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
    /path: \|[\s\S]*?packages\/colors\/bench\/wcag22-feasibility-wasm-boundary-v2\.json[\s\S]*?packages\/colors\/pkg\/labcolors_bg\.wasm/u,
  );
  assert.doesNotMatch(ci, /--record/u, "candidate-recording mode must not survive admission");
  assert.match(ci, /if-no-files-found: error/u);
  assert.match(
    readFileSync(resolve(root, "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs"), "utf8"),
    /--verify/u,
    "the same harness must expose the post-record hard-verify path",
  );
});
