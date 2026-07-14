#!/usr/bin/env node

// Whole-call #295 boundary evidence through the built package-root API.
// `--emit` prints diagnostic JSON with explicit canonical eligibility; `--record PATH`
// admits only the pinned Linux x64/Node/toolchain context and never overwrites;
// `--verify [PATH]` binds the exact WASM and reruns every structural law.
// Wall time, process maxRSS and post-call WASM pages are observations only.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

const here = dirname(fileURLToPath(import.meta.url));
const harnessPath = fileURLToPath(import.meta.url);
const canonicalPackageRoot = resolve(here, "..");
const packageRoot = process.env.LABCOLORS_BOUNDARY_PACKAGE_ROOT
  ? resolve(process.env.LABCOLORS_BOUNDARY_PACKAGE_ROOT)
  : canonicalPackageRoot;
const canonicalPackageInput = packageRoot === canonicalPackageRoot;
const repoRoot = resolve(canonicalPackageRoot, "../..");
const packageEntry = resolve(packageRoot, "index.js");
const packageManifestPath = resolve(packageRoot, "package.json");
const wasmGluePath = resolve(packageRoot, "pkg/labcolors.js");
const wasmPath = resolve(packageRoot, "pkg/labcolors_bg.wasm");
const eagerRuntimeModules = {
  adaptTheme: [resolve(packageRoot, "adapt-theme.js"), "packages/colors/adapt-theme.js"],
  applyTheme: [resolve(packageRoot, "apply-theme.js"), "packages/colors/apply-theme.js"],
  effectiveBackground: [
    resolve(packageRoot, "effective-bg.js"),
    "packages/colors/effective-bg.js",
  ],
  watchTheme: [resolve(packageRoot, "watch-theme.js"), "packages/colors/watch-theme.js"],
};
const runtimeSourceIds = [
  ...Object.keys(eagerRuntimeModules),
  "harness",
  "packageManifest",
  "packageRoot",
  "wasmGlue",
];
const coreAdmissionPath = resolve(
  repoRoot,
  "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v1.json",
);
const packOraclePath = resolve(repoRoot, "conformance/vectors/wcag22-feasibility.json");
const conformanceManifestPath = resolve(repoRoot, "conformance/vectors/manifest.json");
const wasmToolchainPath = resolve(here, "wasm-size-budget-v1.json");
const ciWorkflowPath = resolve(repoRoot, ".github/workflows/ci.yml");
const defaultMeasurementPath = resolve(here, "wcag22-feasibility-wasm-boundary-v1.json");
const pageBytes = 65_536;
const candidateCount = 256;

export const MEASUREMENT_ARTIFACT_ID = "wcag22-feasibility-wasm-whole-call-v1";
export const SCENARIO_IDS = Object.freeze([
  "minimum-evaluated",
  "maximum-canonical-applicable-relations",
  "maximum-applicable-edges",
  "maximum-opaque-utf8-bytes",
  "maximum-canonical-not-applicable-relations",
  "maximum-combined-not-applicable-envelope",
  "maximum-combined-applicable-envelope",
  "maximum-raw-duplicate-relations",
  "maximum-raw-adjacent-duplicates",
  "transport-limit-plus-one",
]);

const artifactTopKeys = [
  "artifactId",
  "bindings",
  "claimBoundary",
  "claims",
  "environment",
  "limits",
  "scenarios",
  "schemaVersion",
];
const hardGates = [
  "completion",
  "request-and-outcome-bytes",
  "sha256-binding",
  "terminal-algebra",
  "packed-shape",
  "candidate-major-lsb0-pack-oracle",
  "no-proportional-dto",
];
const memoryClaim =
  "process maxRSS is total-process high-water including V8; post-call WASM pages are linear-memory high-water observations; neither is total operation memory";
const proportionalKeys = new Set([
  "assessments",
  "cells",
  "feasibleCandidates",
  "infeasibleCandidates",
]);

function fail(message) {
  throw new Error(`WCAG22 feasibility WASM boundary: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value, expected, label) {
  if (!isRecord(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (!isDeepStrictEqual(actual, wanted)) {
    fail(`${label} fields ${JSON.stringify(actual)} differ from ${JSON.stringify(wanted)}`);
  }
}

function positiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) fail(`${label} must be a positive safe integer`);
}

function nonNegativeSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) {
    fail(`${label} must be a non-negative safe integer`);
  }
}

function decimal(value, label, { positive = false } = {}) {
  if (typeof value !== "string" || !/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    fail(`${label} must be canonical unsigned decimal text`);
  }
  const parsed = BigInt(value);
  if (positive && parsed === 0n) fail(`${label} must be positive`);
  return parsed;
}

function digest(value, label) {
  if (!/^[0-9a-f]{64}$/u.test(value ?? "")) fail(`${label} must be lowercase SHA-256`);
}

function readJsonWithBytes(path, label) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    fail(`cannot read ${label}: ${error.message}`);
  }
  try {
    return { bytes, value: JSON.parse(bytes.toString("utf8")) };
  } catch (error) {
    fail(`${label} is not JSON: ${error.message}`);
  }
}

function sourceContracts() {
  const core = readJsonWithBytes(coreAdmissionPath, "Core admission artifact");
  const toolchain = readJsonWithBytes(wasmToolchainPath, "WASM toolchain artifact");
  if (
    core.value?.schemaVersion !== 1 ||
    core.value?.artifactId !== "wcag22-feasibility-admission-raw-v1" ||
    core.value?.profileLimits?.profileId !== "compile-v1"
  ) {
    fail("unsupported Core admission artifact identity");
  }
  if (
    toolchain.value?.schemaVersion !== 2 ||
    toolchain.value?.budgetId !== "labcolors-wasm-raw-issue-284-v1"
  ) {
    fail("unsupported canonical WASM toolchain artifact identity");
  }
  return { core, toolchain };
}

function toolchainRecipe(toolchain) {
  const measurement = toolchain.measurement;
  const recipe = {
    rustToolchain: measurement?.rustToolchain,
    rustcCommit: measurement?.rustcCommit,
    wasmPack: measurement?.wasmPack,
    wasmBindgen: measurement?.wasmBindgen,
    target: measurement?.target,
    cargoProfile: measurement?.cargoProfile,
    wasmOpt: measurement?.wasmOpt,
    wasmOptVersion: measurement?.wasmOptVersion,
    measurementPlatform: measurement?.measurementPlatform,
    rustPathRemap: measurement?.rustPathRemap,
    command: measurement?.command,
  };
  if (
    Object.values(recipe).some((value) => value === undefined) ||
    !Array.isArray(recipe.rustPathRemap)
  ) {
    fail("canonical WASM toolchain recipe is incomplete");
  }
  return recipe;
}

function runtimeSourceBindings() {
  const binding = (path, repositoryPath, label) => {
    let bytes;
    try {
      bytes = readFileSync(path);
    } catch (error) {
      fail(`cannot read ${label}: ${error.message}`);
    }
    return { path: repositoryPath, sha256: sha256(bytes) };
  };
  const eagerModules = Object.fromEntries(
    Object.entries(eagerRuntimeModules).map(([sourceId, [path, repositoryPath]]) => [
      sourceId,
      binding(path, repositoryPath, `${sourceId} eager runtime module`),
    ]),
  );
  return {
    harness: binding(
      harnessPath,
      "packages/colors/bench/wcag22-feasibility-boundary.bench.mjs",
      "measurement harness",
    ),
    packageManifest: binding(
      packageManifestPath,
      "packages/colors/package.json",
      "package manifest",
    ),
    packageRoot: binding(packageEntry, "packages/colors/index.js", "package-root module"),
    wasmGlue: binding(
      wasmGluePath,
      "packages/colors/pkg/labcolors.js",
      "wasm-bindgen JS glue",
    ),
    ...eagerModules,
  };
}

function packOracle() {
  const pack = readJsonWithBytes(packOraclePath, "pack-5 WCAG22 feasibility oracle");
  const manifest = readJsonWithBytes(conformanceManifestPath, "pack-5 manifest");
  if (!Array.isArray(pack.value)) fail("pack-5 WCAG22 feasibility oracle must be an array");
  if (
    manifest.value?.packVersion !== "5.0.0" ||
    typeof manifest.value?.packDigest !== "string" ||
    manifest.value?.counts?.wcag22Feasibility !== pack.value.length
  ) {
    fail("pack-5 manifest does not bind the WCAG22 feasibility family");
  }
  const vector = pack.value.find((entry) => entry.caseId === "text-default-seven");
  if (!vector || typeof vector.requestJson !== "string" || typeof vector.outcomeJson !== "string") {
    fail("pack-5 WCAG22 feasibility oracle lacks text-default-seven");
  }
  let requestValue;
  let outcomeValue;
  try {
    requestValue = JSON.parse(vector.requestJson);
    outcomeValue = JSON.parse(vector.outcomeJson);
  } catch (error) {
    fail(`pack-5 text-default-seven is not nested JSON: ${error.message}`);
  }
  const relation = requestValue?.relations?.[0];
  const result = outcomeValue?.feasibility?.result;
  const matrix = result?.failureMatrix;
  const partition = result?.proof?.partition;
  if (
    requestValue?.relations?.length !== 1 ||
    relation?.kind !== "applicable" ||
    relation?.criterion !== "sc-1.4.3-text-default" ||
    !isDeepStrictEqual(relation?.adjacent, [[118, 118, 118]]) ||
    outcomeValue?.outcome !== "success" ||
    outcomeValue?.feasibility?.status !== "feasible" ||
    !Array.isArray(matrix) ||
    matrix.length !== 32 ||
    !Array.isArray(partition) ||
    partition.length !== 32
  ) {
    fail("pack-5 text-default-seven no longer defines the exact minimum LSB0 oracle");
  }
  return {
    path: "conformance/vectors/wcag22-feasibility.json",
    sha256: sha256(pack.bytes),
    caseId: vector.caseId,
    requestSha256: sha256(Buffer.from(vector.requestJson, "utf8")),
    outcomeSha256: sha256(Buffer.from(vector.outcomeJson, "utf8")),
    manifestPath: "conformance/vectors/manifest.json",
    manifestSha256: sha256(manifest.bytes),
    packVersion: manifest.value.packVersion,
    packDigest: manifest.value.packDigest,
    matrix,
    partition,
  };
}

function coreLimits(core) {
  const profile = core.profileLimits;
  for (const field of [
    "rawRelations",
    "rawAdjacentEntries",
    "opaqueUtf8Bytes",
    "canonicalRelations",
    "applicableEdges",
    "logicalAssessments",
    "packedResultBytes",
  ]) {
    positiveSafeInteger(profile?.[field], `Core profileLimits.${field}`);
  }
  return structuredClone(profile);
}

function requiredSampleCount(core) {
  positiveSafeInteger(core.sampleCount, "Core admission sampleCount");
  return core.sampleCount;
}

function canonicalNodeVersion() {
  const source = readFileSync(ciWorkflowPath, "utf8");
  const version = source.match(/^\s{2}NODE_TOOLCHAIN: (\d+\.\d+\.\d+)$/mu)?.[1];
  if (!version) fail("CI does not expose one exact NODE_TOOLCHAIN version");
  return `v${version}`;
}

function scenarioSourceName(scenarioId) {
  return scenarioId === "transport-limit-plus-one"
    ? "maximum-combined-applicable-envelope"
    : scenarioId;
}

function expectedShape(core, scenarioId) {
  const source = core.scenarios.find(
    (scenario) => scenario.name === scenarioSourceName(scenarioId),
  );
  if (!source) fail(`Core admission artifact lacks ${scenarioSourceName(scenarioId)}`);
  return structuredClone(source.shape);
}

function validateSummary(summary, shape, scenarioId, limits, label) {
  if (scenarioId === "transport-limit-plus-one") {
    exactKeys(
      summary,
      [
        "code",
        "limitBytes",
        "outcome",
        "proportionalFieldsPresent",
        "requestedBytes",
        "source",
      ],
      label,
    );
    if (
      summary.outcome !== "failure" ||
      summary.source !== "transport" ||
      summary.code !== "envelopeTooLarge" ||
      decimal(summary.requestedBytes, `${label}.requestedBytes`) !==
        BigInt(limits.maxRequestBytes + 1) ||
      decimal(summary.limitBytes, `${label}.limitBytes`) !== BigInt(limits.maxRequestBytes) ||
      summary.proportionalFieldsPresent !== false
    ) {
      fail(`${label} does not preserve the exact host-preflight failure`);
    }
    return;
  }

  const declarationOnly =
    scenarioId === "maximum-opaque-utf8-bytes" ||
    scenarioId === "maximum-canonical-not-applicable-relations" ||
    scenarioId === "maximum-combined-not-applicable-envelope";
  if (declarationOnly) {
    exactKeys(
      summary,
      [
        "applicableRelations",
        "canonicalRelations",
        "notApplicableRelations",
        "numericalEvidencePresent",
        "outcome",
        "proportionalFieldsPresent",
        "terminal",
      ],
      label,
    );
    if (
      summary.outcome !== "success" ||
      summary.terminal !== "notEvaluated" ||
      decimal(summary.canonicalRelations, `${label}.canonicalRelations`) !==
        BigInt(shape.canonicalRelations) ||
      decimal(summary.applicableRelations, `${label}.applicableRelations`) !== 0n ||
      decimal(summary.notApplicableRelations, `${label}.notApplicableRelations`) !==
        BigInt(shape.canonicalRelations) ||
      summary.numericalEvidencePresent !== false ||
      summary.proportionalFieldsPresent !== false
    ) {
      fail(`${label} fabricates or loses declaration-only evidence`);
    }
    return;
  }

  const evaluatedKeys = [
    "applicableEdges",
    "applicableRelations",
    "canonicalRelations",
    "domainCount",
    "failureMatrixBytes",
    "feasibleCandidates",
    "logicalAssessments",
    "lsb0PartitionMatchesMatrix",
    "notApplicableRelations",
    "outcome",
    "partitionBytes",
    "proportionalFieldsPresent",
    "terminal",
  ];
  if (scenarioId === "minimum-evaluated") evaluatedKeys.push("lsb0PackOracleMatches");
  exactKeys(summary, evaluatedKeys, label);
  const feasible = summary.feasibleCandidates;
  nonNegativeSafeInteger(feasible, `${label}.feasibleCandidates`);
  if (
    summary.outcome !== "success" ||
    !["feasible", "infeasible"].includes(summary.terminal) ||
    decimal(summary.domainCount, `${label}.domainCount`) !== BigInt(candidateCount) ||
    decimal(summary.canonicalRelations, `${label}.canonicalRelations`) !==
      BigInt(shape.canonicalRelations) ||
    decimal(summary.applicableRelations, `${label}.applicableRelations`) !==
      BigInt(shape.applicableRelations) ||
    decimal(summary.notApplicableRelations, `${label}.notApplicableRelations`) !==
      BigInt(shape.canonicalRelations - shape.applicableRelations) ||
    decimal(summary.applicableEdges, `${label}.applicableEdges`) !==
      BigInt(shape.applicableEdges) ||
    decimal(summary.logicalAssessments, `${label}.logicalAssessments`) !==
      BigInt(candidateCount * shape.applicableEdges) ||
    summary.failureMatrixBytes !== 32 * shape.applicableEdges ||
    summary.partitionBytes !== 32 ||
    summary.lsb0PartitionMatchesMatrix !== true ||
    (scenarioId === "minimum-evaluated" && summary.lsb0PackOracleMatches !== true) ||
    summary.proportionalFieldsPresent !== false ||
    (summary.terminal === "feasible") !== (feasible > 0)
  ) {
    fail(`${label} violates terminal, count, packed-shape or LSB0 algebra`);
  }
}

function validateSample(sample, shape, scenarioId, limits, sampleIndex, label) {
  exactKeys(
    sample,
    [
      "elapsedNs",
      "outcomeBytes",
      "outcomeSha256",
      "processMaxRssKiBAfter",
      "processMaxRssKiBBefore",
      "requestBytes",
      "requestSha256",
      "sampleIndex",
      "summary",
      "wasmMemoryBytesAfter",
      "wasmMemoryBytesBefore",
      "wasmMemoryPagesAfter",
      "wasmMemoryPagesBefore",
    ],
    label,
  );
  if (sample.sampleIndex !== sampleIndex) fail(`${label}.sampleIndex is not contiguous`);
  decimal(sample.elapsedNs, `${label}.elapsedNs`, { positive: true });
  positiveSafeInteger(sample.requestBytes, `${label}.requestBytes`);
  positiveSafeInteger(sample.outcomeBytes, `${label}.outcomeBytes`);
  digest(sample.requestSha256, `${label}.requestSha256`);
  digest(sample.outcomeSha256, `${label}.outcomeSha256`);
  for (const field of [
    "processMaxRssKiBBefore",
    "processMaxRssKiBAfter",
    "wasmMemoryBytesBefore",
    "wasmMemoryBytesAfter",
    "wasmMemoryPagesBefore",
    "wasmMemoryPagesAfter",
  ]) {
    nonNegativeSafeInteger(sample[field], `${label}.${field}`);
  }
  if (
    sample.processMaxRssKiBAfter < sample.processMaxRssKiBBefore ||
    sample.wasmMemoryBytesAfter < sample.wasmMemoryBytesBefore ||
    sample.wasmMemoryPagesAfter < sample.wasmMemoryPagesBefore ||
    sample.wasmMemoryBytesBefore !== sample.wasmMemoryPagesBefore * pageBytes ||
    sample.wasmMemoryBytesAfter !== sample.wasmMemoryPagesAfter * pageBytes
  ) {
    fail(`${label} has contradictory maxRSS or WASM linear-memory observations`);
  }
  validateSummary(sample.summary, shape, scenarioId, limits, `${label}.summary`);
}

/** Validate immutable structure and claims without applying timing/memory thresholds. */
export function validateMeasurementArtifact(
  artifact,
  { requireCanonicalArtifact = true } = {},
) {
  const { core, toolchain } = sourceContracts();
  const profileLimits = coreLimits(core.value);
  const expectedSampleCount = requiredSampleCount(core.value);
  exactKeys(artifact, artifactTopKeys, "artifact");
  if (
    artifact.schemaVersion !== 1 ||
    artifact.artifactId !== MEASUREMENT_ARTIFACT_ID ||
    artifact.claimBoundary !== "canonical-wasm-package-root-whole-call-observations-only"
  ) {
    fail("artifact identity or claim boundary drifted");
  }

  exactKeys(
    artifact.claims,
    ["admission", "hardGates", "latency", "memory", "timingThresholdNs"],
    "claims",
  );
  if (
    artifact.claims.admission !== "canonical-linux-x64-exact-wasm-only" ||
    !isDeepStrictEqual(artifact.claims.hardGates, hardGates) ||
    artifact.claims.timingThresholdNs !== null ||
    artifact.claims.latency !== "observation-only-no-production-threshold" ||
    artifact.claims.memory !== memoryClaim
  ) {
    fail("claims add a guessed threshold, inflate memory meaning or weaken hard gates");
  }

  exactKeys(
    artifact.environment,
    [
      "canonicalCandidate",
      "cargoProfile",
      "execution",
      "nodeVersion",
      "packageRootApi",
      "platform",
      "requestConstructionMeasured",
      "rustToolchain",
      "sampleCount",
      "target",
      "timer",
      "wasmBindgen",
      "wasmOpt",
      "wasmPack",
    ],
    "environment",
  );
  const measuredToolchain = toolchain.value.measurement;
  if (
    artifact.environment.execution !== "fresh-node-child-process-per-sample" ||
    artifact.environment.sampleCount !== expectedSampleCount ||
    artifact.environment.requestConstructionMeasured !== false ||
    artifact.environment.timer !== "process.hrtime.bigint" ||
    artifact.environment.packageRootApi !== "packages/colors/index.js" ||
    !/^v\d+\.\d+\.\d+$/u.test(artifact.environment.nodeVersion ?? "") ||
    (requireCanonicalArtifact &&
      artifact.environment.nodeVersion !== canonicalNodeVersion()) ||
    artifact.environment.rustToolchain !== measuredToolchain.rustToolchain ||
    artifact.environment.wasmPack !== measuredToolchain.wasmPack ||
    artifact.environment.wasmBindgen !== measuredToolchain.wasmBindgen ||
    artifact.environment.target !== measuredToolchain.target ||
    artifact.environment.cargoProfile !== measuredToolchain.cargoProfile ||
    artifact.environment.wasmOpt !== measuredToolchain.wasmOpt
  ) {
    fail("environment is not the pinned whole-call/toolchain contract");
  }
  if (
    requireCanonicalArtifact &&
    (artifact.environment.platform !== "linux-x64" ||
      artifact.environment.canonicalCandidate !== true)
  ) {
    fail("only canonical linux-x64 observations are admissible");
  }

  exactKeys(
    artifact.bindings,
    ["coreAdmission", "packOracle", "runtimeSources", "wasm", "wasmToolchain"],
    "bindings",
  );
  exactKeys(
    artifact.bindings.coreAdmission,
    ["artifactId", "path", "profileId", "schemaVersion", "sha256"],
    "bindings.coreAdmission",
  );
  if (
    artifact.bindings.coreAdmission.path !==
      "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v1.json" ||
    artifact.bindings.coreAdmission.schemaVersion !== core.value.schemaVersion ||
    artifact.bindings.coreAdmission.artifactId !== core.value.artifactId ||
    artifact.bindings.coreAdmission.profileId !== core.value.profileLimits.profileId ||
    artifact.bindings.coreAdmission.sha256 !== sha256(core.bytes)
  ) {
    fail("Core admission binding drifted");
  }
  const oracle = packOracle();
  exactKeys(
    artifact.bindings.packOracle,
    [
      "caseId",
      "manifestPath",
      "manifestSha256",
      "outcomeSha256",
      "packDigest",
      "packVersion",
      "path",
      "requestSha256",
      "sha256",
    ],
    "bindings.packOracle",
  );
  if (
    artifact.bindings.packOracle.path !== oracle.path ||
    artifact.bindings.packOracle.sha256 !== oracle.sha256 ||
    artifact.bindings.packOracle.caseId !== oracle.caseId ||
    artifact.bindings.packOracle.requestSha256 !== oracle.requestSha256 ||
    artifact.bindings.packOracle.outcomeSha256 !== oracle.outcomeSha256 ||
    artifact.bindings.packOracle.manifestPath !== oracle.manifestPath ||
    artifact.bindings.packOracle.manifestSha256 !== oracle.manifestSha256 ||
    artifact.bindings.packOracle.packVersion !== oracle.packVersion ||
    artifact.bindings.packOracle.packDigest !== oracle.packDigest
  ) {
    fail("pack-5 LSB0 oracle binding drifted");
  }
  const sources = runtimeSourceBindings();
  exactKeys(
    artifact.bindings.runtimeSources,
    runtimeSourceIds,
    "bindings.runtimeSources",
  );
  for (const sourceId of runtimeSourceIds) {
    exactKeys(
      artifact.bindings.runtimeSources[sourceId],
      ["path", "sha256"],
      `bindings.runtimeSources.${sourceId}`,
    );
    if (!isDeepStrictEqual(artifact.bindings.runtimeSources[sourceId], sources[sourceId])) {
      fail(`runtime source binding ${sourceId} drifted`);
    }
  }
  exactKeys(
    artifact.bindings.wasmToolchain,
    ["budgetId", "path", "recipeSha256", "schemaVersion"],
    "bindings.wasmToolchain",
  );
  if (
    artifact.bindings.wasmToolchain.path !==
      "packages/colors/bench/wasm-size-budget-v1.json" ||
    artifact.bindings.wasmToolchain.schemaVersion !== toolchain.value.schemaVersion ||
    artifact.bindings.wasmToolchain.budgetId !== toolchain.value.budgetId ||
    artifact.bindings.wasmToolchain.recipeSha256 !==
      sha256(Buffer.from(JSON.stringify(toolchainRecipe(toolchain.value)), "utf8"))
  ) {
    fail("canonical WASM toolchain binding drifted");
  }
  exactKeys(artifact.bindings.wasm, ["bytes", "path", "sha256"], "bindings.wasm");
  if (artifact.bindings.wasm.path !== "packages/colors/pkg/labcolors_bg.wasm") {
    fail("WASM binding path drifted");
  }
  positiveSafeInteger(artifact.bindings.wasm.bytes, "bindings.wasm.bytes");
  digest(artifact.bindings.wasm.sha256, "bindings.wasm.sha256");

  exactKeys(
    artifact.limits,
    ["maxRequestBytes", ...Object.keys(profileLimits)],
    "limits",
  );
  positiveSafeInteger(artifact.limits.maxRequestBytes, "limits.maxRequestBytes");
  const recordedProfileLimits = { ...artifact.limits };
  delete recordedProfileLimits.maxRequestBytes;
  if (!isDeepStrictEqual(recordedProfileLimits, profileLimits)) {
    fail("artifact Core limits differ from the bound admission artifact");
  }
  if (!Array.isArray(artifact.scenarios) || artifact.scenarios.length !== SCENARIO_IDS.length) {
    fail(`artifact must contain exactly ${SCENARIO_IDS.length} scenarios`);
  }
  const requestDigests = new Set();
  artifact.scenarios.forEach((scenario, scenarioIndex) => {
    const scenarioId = SCENARIO_IDS[scenarioIndex];
    exactKeys(scenario, ["samples", "scenarioId", "shape"], `scenarios[${scenarioIndex}]`);
    if (scenario.scenarioId !== scenarioId) {
      fail(`scenario order/id drift at ${scenarioIndex}: ${scenario.scenarioId}`);
    }
    const shape = expectedShape(core.value, scenarioId);
    if (!isDeepStrictEqual(scenario.shape, shape)) fail(`${scenarioId} shape drifted`);
    if (!Array.isArray(scenario.samples) || scenario.samples.length !== expectedSampleCount) {
      fail(`${scenarioId} must contain exactly ${expectedSampleCount} fresh-process samples`);
    }
    scenario.samples.forEach((sample, index) =>
      validateSample(
        sample,
        shape,
        scenarioId,
        artifact.limits,
        index,
        `${scenarioId}.samples[${index}]`,
      ));
    const first = scenario.samples[0];
    const stable = {
      requestBytes: first.requestBytes,
      requestSha256: first.requestSha256,
      outcomeBytes: first.outcomeBytes,
      outcomeSha256: first.outcomeSha256,
      summary: first.summary,
    };
    for (const sample of scenario.samples.slice(1)) {
      if (
        !isDeepStrictEqual(
          {
            requestBytes: sample.requestBytes,
            requestSha256: sample.requestSha256,
            outcomeBytes: sample.outcomeBytes,
            outcomeSha256: sample.outcomeSha256,
            summary: sample.summary,
          },
          stable,
        )
      ) {
        fail(`${scenarioId} request/outcome evidence is not byte-stable across samples`);
      }
    }
    if (
      scenarioId === "maximum-combined-applicable-envelope" &&
      first.requestBytes !== artifact.limits.maxRequestBytes
    ) {
      fail("maximum-combined-applicable-envelope does not attain the public byte ceiling");
    }
    if (
      scenarioId === "transport-limit-plus-one" &&
      first.requestBytes !== artifact.limits.maxRequestBytes + 1
    ) {
      fail("transport-limit-plus-one is not the exact host-preflight boundary");
    }
    requestDigests.add(first.requestSha256);
  });
  if (requestDigests.size !== SCENARIO_IDS.length) {
    fail("distinct boundary scenarios collapsed to the same request evidence");
  }
  return artifact;
}

function utf8Bytes(value) {
  return new TextEncoder().encode(value).byteLength;
}

function relation(relationId, occurrenceId, criterion, adjacent) {
  return { relationId, occurrenceId, kind: "applicable", criterion, adjacent };
}

function notApplicable(relationId, occurrenceId, reasonId) {
  return { relationId, occurrenceId, kind: "notApplicable", reasonId };
}

function request(relations) {
  return {
    schemaVersion: 1,
    domainId: "srgb8-neutral-axis-v1",
    resourceProfileId: "compile-v1",
    relations,
  };
}

function maximallyEscapedIdentity(index) {
  const codePoints = [
    0, 1, 2, 3, 4, 5, 6, 7, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
    25, 26, 27, 28, 29, 30, 31,
  ];
  const radix = codePoints.length;
  return String.fromCharCode(
    codePoints[index % radix],
    codePoints[Math.floor(index / radix) % radix],
    codePoints[Math.floor(index / (radix * radix)) % radix],
  );
}

function buildScenario(scenarioId, limits) {
  const max = limits.rawRelations;
  const criterion = "sc-1.4.3-text-default";
  const neutralAdjacent = [118, 118, 118];
  let value;
  switch (scenarioId) {
    case "minimum-evaluated":
      value = request([relation("r", "o", criterion, [neutralAdjacent])]);
      break;
    case "maximum-canonical-applicable-relations":
      value = request(Array.from({ length: max }, (_, index) => {
        const suffix = String(index).padStart(4, "0");
        return relation(`r${suffix}`, `o${suffix}`, criterion, [neutralAdjacent]);
      }));
      break;
    case "maximum-applicable-edges":
      value = request([relation(
        "r",
        "o".repeat(18),
        criterion,
        Array.from({ length: limits.rawAdjacentEntries }, (_, index) => [
          index & 0xff,
          (index >>> 8) & 0xff,
          255,
        ]),
      )]);
      break;
    case "maximum-opaque-utf8-bytes":
      value = request([
        notApplicable("r", "o", "é".repeat((limits.opaqueUtf8Bytes - 2) / 2)),
      ]);
      break;
    case "maximum-canonical-not-applicable-relations":
      value = request(Array.from({ length: max }, (_, index) => {
        const suffix = String(index).padStart(4, "0");
        return notApplicable(`r${suffix}`, `o${suffix}`, "declared");
      }));
      break;
    case "maximum-combined-not-applicable-envelope": {
      const baseOpaqueBytes = max * 5;
      const remaining = limits.opaqueUtf8Bytes - baseOpaqueBytes;
      value = request(Array.from({ length: max }, (_, index) => notApplicable(
        maximallyEscapedIdentity(index),
        "\0",
        "\0".repeat(index === 0 ? remaining + 1 : 1),
      )));
      break;
    }
    case "maximum-combined-applicable-envelope":
    case "transport-limit-plus-one": {
      const baseOpaqueBytes = max * 4;
      const remaining = limits.opaqueUtf8Bytes - baseOpaqueBytes;
      value = request(Array.from({ length: max }, (_, index) => relation(
        maximallyEscapedIdentity(index),
        "\0".repeat(index === 0 ? remaining + 1 : 1),
        "sc-1.4.11-ui-component-or-state",
        [[255, 255, 255]],
      )));
      break;
    }
    case "maximum-raw-duplicate-relations": {
      const duplicate = relation("duplicate", "same", criterion, [neutralAdjacent]);
      value = request(Array.from({ length: max }, () => duplicate));
      break;
    }
    case "maximum-raw-adjacent-duplicates":
      value = request([relation(
        "r",
        "o",
        criterion,
        Array.from({ length: limits.rawAdjacentEntries }, () => neutralAdjacent),
      )]);
      break;
    default:
      fail(`unknown scenario ${scenarioId}`);
  }

  const source = JSON.stringify(value);
  const raw = new TextEncoder().encode(source);
  const expected = expectedShape(sourceContracts().core.value, scenarioId);
  const rawRelations = value.relations.length;
  const rawAdjacentEntries = value.relations.reduce(
    (sum, item) => sum + (item.kind === "applicable" ? item.adjacent.length : 0),
    0,
  );
  const opaqueUtf8Bytes = value.relations.reduce(
    (sum, item) =>
      sum +
      utf8Bytes(item.relationId) +
      utf8Bytes(item.occurrenceId) +
      (item.kind === "notApplicable" ? utf8Bytes(item.reasonId) : 0),
    0,
  );
  if (
    rawRelations !== expected.rawRelations ||
    rawAdjacentEntries !== expected.rawAdjacentEntries ||
    opaqueUtf8Bytes !== expected.opaqueUtf8Bytes
  ) {
    fail(`${scenarioId} generator does not attain its Core-owned raw shape`);
  }
  if (
    (scenarioId === "maximum-combined-applicable-envelope" ||
      scenarioId === "transport-limit-plus-one") &&
    raw.byteLength !== limits.maxRequestBytes
  ) {
    fail(`${scenarioId} generator does not attain the derived compact byte ceiling`);
  }
  return {
    shape: expected,
    bytes:
      scenarioId === "transport-limit-plus-one"
        ? new Uint8Array([...raw, 0x20])
        : raw,
  };
}

function hasProportionalField(value) {
  if (Array.isArray(value)) return value.some(hasProportionalField);
  if (!isRecord(value)) return false;
  return Object.entries(value).some(
    ([key, child]) => proportionalKeys.has(key) || hasProportionalField(child),
  );
}

function packedBit(bytes, logicalIndex) {
  return (bytes[Math.floor(logicalIndex / 8)] & (1 << (logicalIndex % 8))) !== 0;
}

function summarizeOutcome(outcome, scenarioId) {
  const proportionalFieldsPresent = hasProportionalField(outcome);
  if (outcome?.outcome === "failure") {
    const protocol = outcome.error;
    if (protocol?.source !== "transport" || protocol.error?.code !== "envelopeTooLarge") {
      fail("measurement encountered an unexpected failure branch");
    }
    return {
      outcome: "failure",
      source: protocol.source,
      code: protocol.error.code,
      requestedBytes: protocol.error.requestedBytes,
      limitBytes: protocol.error.limitBytes,
      proportionalFieldsPresent,
    };
  }
  if (outcome?.outcome !== "success") fail("measurement returned an unknown outcome algebra");
  const feasibility = outcome.feasibility;
  if (feasibility?.status === "notEvaluated") {
    const relations = feasibility.result?.relations;
    if (!Array.isArray(relations) || relations.some((item) => item.kind !== "notApplicable")) {
      fail("NotEvaluated did not retain declaration-only relations");
    }
    return {
      outcome: "success",
      terminal: feasibility.status,
      canonicalRelations: String(relations.length),
      applicableRelations: "0",
      notApplicableRelations: String(relations.length),
      numericalEvidencePresent: ["domain", "failureMatrix", "partition", "proof"].some(
        (field) => field in feasibility.result,
      ),
      proportionalFieldsPresent,
    };
  }
  if (!["feasible", "infeasible"].includes(feasibility?.status)) {
    fail("measurement returned an unknown feasibility terminal");
  }
  const result = feasibility.result;
  const proof = result?.proof;
  const domain = result?.domain;
  const matrix = result?.failureMatrix;
  const partition = proof?.partition;
  if (!Array.isArray(domain) || domain.length !== candidateCount) {
    fail("evaluated result does not transport the complete 256-state domain");
  }
  domain.forEach((candidate, index) => {
    if (!isDeepStrictEqual(candidate, [index, index, index])) {
      fail(`domain candidate ${index} is not the registered neutral-axis state`);
    }
  });
  if (!Array.isArray(matrix) || !Array.isArray(partition)) {
    fail("evaluated result lacks packed matrix or partition bytes");
  }
  if (
    matrix.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255) ||
    partition.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)
  ) {
    fail("evaluated result contains a non-byte packed value");
  }
  const edges = Number(decimal(proof.applicableEdges, "proof.applicableEdges"));
  if (matrix.length !== 32 * edges || partition.length !== 32) {
    fail("evaluated result violates the exact packed-storage law");
  }
  let feasibleCandidates = 0;
  let lsb0PartitionMatchesMatrix = true;
  for (let candidate = 0; candidate < candidateCount; candidate += 1) {
    let rowFailed = false;
    for (let edge = 0; edge < edges; edge += 1) {
      rowFailed ||= packedBit(matrix, candidate * edges + edge);
    }
    const rowFeasible = !rowFailed;
    lsb0PartitionMatchesMatrix &&= packedBit(partition, candidate) === rowFeasible;
    feasibleCandidates += Number(rowFeasible);
  }
  let lsb0PackOracleMatches;
  if (scenarioId === "minimum-evaluated") {
    const oracle = packOracle();
    lsb0PackOracleMatches =
      isDeepStrictEqual(matrix, oracle.matrix) &&
      isDeepStrictEqual(partition, oracle.partition);
  }
  return {
    outcome: "success",
    terminal: feasibility.status,
    domainCount: proof.domainCount,
    canonicalRelations: proof.canonicalRelations,
    applicableRelations: proof.applicableRelations,
    notApplicableRelations: proof.notApplicableRelations,
    applicableEdges: proof.applicableEdges,
    logicalAssessments: proof.logicalAssessments,
    failureMatrixBytes: matrix.length,
    partitionBytes: partition.length,
    feasibleCandidates,
    lsb0PartitionMatchesMatrix,
    ...(scenarioId === "minimum-evaluated" ? { lsb0PackOracleMatches } : {}),
    proportionalFieldsPresent,
  };
}

function wasmMemory(initOutput) {
  const memory = initOutput?.memory;
  if (!(memory instanceof WebAssembly.Memory)) {
    fail("real wasm-bindgen InitOutput does not expose WebAssembly.Memory");
  }
  const bytes = memory.buffer.byteLength;
  if (bytes % pageBytes !== 0) fail("WASM memory byte length is not page-aligned");
  return { bytes, pages: bytes / pageBytes };
}

async function measureOneSample(scenarioId) {
  const { core } = sourceContracts();
  const wasmBytes = readFileSync(wasmPath);
  const rootApi = await import(pathToFileURL(packageEntry).href);
  const initOutput = rootApi.initSync({ module: wasmBytes });
  const publicMax = rootApi.wcag22FeasibilityMaxBytes();
  positiveSafeInteger(publicMax, "public max getter");
  const limits = { maxRequestBytes: publicMax, ...coreLimits(core.value) };
  const built = buildScenario(scenarioId, limits);
  const requestSha256 = sha256(built.bytes);

  const beforeRss = process.resourceUsage().maxRSS;
  const beforeMemory = wasmMemory(initOutput);
  const started = process.hrtime.bigint();
  const outcome = rootApi.evaluateWcag22Feasibility(built.bytes);
  const elapsedNs = process.hrtime.bigint() - started;
  const afterMemory = wasmMemory(initOutput);
  const afterRss = process.resourceUsage().maxRSS;

  const outcomeBytes = new TextEncoder().encode(JSON.stringify(outcome));
  return {
    scenarioId,
    maxRequestBytes: publicMax,
    shape: built.shape,
    sample: {
      sampleIndex: 0,
      elapsedNs: elapsedNs.toString(),
      requestBytes: built.bytes.byteLength,
      requestSha256,
      outcomeBytes: outcomeBytes.byteLength,
      outcomeSha256: sha256(outcomeBytes),
      summary: summarizeOutcome(outcome, scenarioId),
      processMaxRssKiBBefore: beforeRss,
      processMaxRssKiBAfter: afterRss,
      wasmMemoryBytesBefore: beforeMemory.bytes,
      wasmMemoryBytesAfter: afterMemory.bytes,
      wasmMemoryPagesBefore: beforeMemory.pages,
      wasmMemoryPagesAfter: afterMemory.pages,
    },
  };
}

function childSample(scenarioId) {
  const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--sample", scenarioId], {
    cwd: packageRoot,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    timeout: 5 * 60 * 1_000,
  });
  if (child.status !== 0) {
    fail(
      `fresh child failed for ${scenarioId}: ${child.stderr.trim() || child.stdout.trim()}`,
    );
  }
  try {
    return JSON.parse(child.stdout.trim());
  } catch (error) {
    fail(`fresh child emitted non-JSON for ${scenarioId}: ${error.message}`);
  }
}

function measurementArtifact() {
  const { core, toolchain } = sourceContracts();
  const profileLimits = coreLimits(core.value);
  const measuredSampleCount = requiredSampleCount(core.value);
  const oracle = packOracle();
  const sources = runtimeSourceBindings();
  const wasm = readFileSync(wasmPath);
  if (wasm.length < 8 || !wasm.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))) {
    fail("built package WASM is absent or malformed");
  }
  let publicMax;
  const scenarios = SCENARIO_IDS.map((scenarioId) => {
    const samples = Array.from({ length: measuredSampleCount }, (_, index) => {
      const measured = childSample(scenarioId);
      publicMax ??= measured.maxRequestBytes;
      if (
        measured.scenarioId !== scenarioId ||
        measured.maxRequestBytes !== publicMax ||
        !isDeepStrictEqual(measured.shape, expectedShape(core.value, scenarioId))
      ) {
        fail(`${scenarioId} child did not preserve derived identity/shape`);
      }
      return { ...measured.sample, sampleIndex: index };
    });
    return {
      scenarioId,
      shape: expectedShape(core.value, scenarioId),
      samples,
    };
  });
  positiveSafeInteger(publicMax, "fresh-child public max getter");
  const limits = { maxRequestBytes: publicMax, ...profileLimits };
  const platform = `${process.platform}-${process.arch}`;
  const measuredToolchain = toolchain.value.measurement;
  const artifact = {
    schemaVersion: 1,
    artifactId: MEASUREMENT_ARTIFACT_ID,
    claimBoundary: "canonical-wasm-package-root-whole-call-observations-only",
    claims: {
      admission: "canonical-linux-x64-exact-wasm-only",
      hardGates,
      timingThresholdNs: null,
      latency: "observation-only-no-production-threshold",
      memory: memoryClaim,
    },
    environment: {
      execution: "fresh-node-child-process-per-sample",
      platform,
      nodeVersion: process.version,
      sampleCount: measuredSampleCount,
      requestConstructionMeasured: false,
      timer: "process.hrtime.bigint",
      packageRootApi: "packages/colors/index.js",
      canonicalCandidate:
        platform === "linux-x64" &&
        canonicalPackageInput &&
        process.version === canonicalNodeVersion(),
      rustToolchain: measuredToolchain.rustToolchain,
      wasmPack: measuredToolchain.wasmPack,
      wasmBindgen: measuredToolchain.wasmBindgen,
      target: measuredToolchain.target,
      cargoProfile: measuredToolchain.cargoProfile,
      wasmOpt: measuredToolchain.wasmOpt,
    },
    bindings: {
      coreAdmission: {
        path: "crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v1.json",
        schemaVersion: core.value.schemaVersion,
        artifactId: core.value.artifactId,
        profileId: core.value.profileLimits.profileId,
        sha256: sha256(core.bytes),
      },
      packOracle: {
        path: oracle.path,
        sha256: oracle.sha256,
        caseId: oracle.caseId,
        requestSha256: oracle.requestSha256,
        outcomeSha256: oracle.outcomeSha256,
        manifestPath: oracle.manifestPath,
        manifestSha256: oracle.manifestSha256,
        packVersion: oracle.packVersion,
        packDigest: oracle.packDigest,
      },
      runtimeSources: sources,
      wasmToolchain: {
        path: "packages/colors/bench/wasm-size-budget-v1.json",
        schemaVersion: toolchain.value.schemaVersion,
        budgetId: toolchain.value.budgetId,
        recipeSha256: sha256(
          Buffer.from(JSON.stringify(toolchainRecipe(toolchain.value)), "utf8"),
        ),
      },
      wasm: {
        path: "packages/colors/pkg/labcolors_bg.wasm",
        bytes: wasm.length,
        sha256: sha256(wasm),
      },
    },
    limits,
    scenarios,
  };
  validateMeasurementArtifact(artifact, { requireCanonicalArtifact: false });
  return artifact;
}

function deterministicProjection(artifact) {
  return {
    bindings: artifact.bindings,
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
  };
}

function verifyMeasurement(path) {
  const loaded = readJsonWithBytes(path, "committed WASM boundary artifact");
  const expected = loaded.value;
  if (loaded.bytes.toString("utf8") !== `${JSON.stringify(expected)}\n`) {
    fail("committed WASM boundary artifact must be canonical compact JSON plus newline");
  }
  validateMeasurementArtifact(expected);
  const actualPlatform = `${process.platform}-${process.arch}`;
  if (
    actualPlatform !== expected.environment.platform ||
    process.version !== expected.environment.nodeVersion
  ) {
    fail(
      `verify requires ${expected.environment.platform}/${expected.environment.nodeVersion}, ` +
        `got ${actualPlatform}/${process.version}`,
    );
  }
  const wasm = readFileSync(wasmPath);
  if (
    wasm.length !== expected.bindings.wasm.bytes ||
    sha256(wasm) !== expected.bindings.wasm.sha256
  ) {
    fail("built WASM bytes differ from the exact committed canonical artifact");
  }
  const rerun = measurementArtifact();
  if (!isDeepStrictEqual(deterministicProjection(rerun), deterministicProjection(expected))) {
    fail("whole-call rerun differs in immutable request/outcome/scenario evidence");
  }
  process.stdout.write(
    `PASS ${MEASUREMENT_ARTIFACT_ID} ${expected.bindings.wasm.bytes}B ` +
      `${expected.bindings.wasm.sha256} scenarios=${SCENARIO_IDS.length}\n`,
  );
}

function usage() {
  fail(
    "usage: --emit | --record <path> | --verify [path] | --sample <scenario-id>",
  );
}

async function main(args) {
  const [mode, value, extra] = args;
  if (extra !== undefined) usage();
  if (mode === "--sample" && value) {
    process.stdout.write(JSON.stringify(await measureOneSample(value)));
    return;
  }
  if (mode === "--emit" && value === undefined) {
    process.stdout.write(`${JSON.stringify(measurementArtifact())}\n`);
    return;
  }
  if (mode === "--record" && value) {
    const artifact = measurementArtifact();
    validateMeasurementArtifact(artifact);
    writeFileSync(resolve(value), `${JSON.stringify(artifact)}\n`, { flag: "wx" });
    process.stdout.write(`${JSON.stringify(artifact)}\n`);
    return;
  }
  if (mode === "--verify" && extra === undefined) {
    verifyMeasurement(value ? resolve(value) : defaultMeasurementPath);
    return;
  }
  usage();
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
}
