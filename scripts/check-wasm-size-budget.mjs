#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const SCRIPT_DIR = dirname(SCRIPT_PATH);
const REPO_ROOT = resolve(SCRIPT_DIR, "..");
const DEFAULT_WASM = resolve(REPO_ROOT, "packages/colors/pkg/labcolors_bg.wasm");
const V1_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v1.json");
const V2_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v2.json");
const V3_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v3.json");

export const DEFAULT_BUDGET = resolve(
  REPO_ROOT,
  "packages/colors/bench/wasm-size-budget-v4.json",
);
export const V1_FILE_SHA256 =
  "4f7340fc8cfd0ccb97377c385f2f8d8e7a9ef2c5ba96177f518c5d07de2825e1";
export const V1_RECIPE_SHA256 =
  "0ea74cb070e0a5facb7280f6124930a0bb673ee4dcee9c99fff110db6c9389d4";
export const V2_FILE_SHA256 =
  "713ccc314b3e6f638d87a54716d665d52f77c86f34a2b6edefe0a354a499d8b1";
export const V3_FILE_SHA256 =
  "d7937612e4c33574a8af28845bb1dd30cca86fc39fc0206cac4c377de77fec15";
export const V4_FILE_SHA256 =
  "f0c3b2190f5675791e74ebd5591b1ad3d31bc6ff877f5f46a87b62524fbdc413";

const V1_REPOSITORY_PATH = "packages/colors/bench/wasm-size-budget-v1.json";
const V4_BUDGET_ID = "labcolors-wasm-raw-issue-296-v4";
const WASM_REPOSITORY_PATH = "packages/colors/pkg/labcolors_bg.wasm";

function fail(message) {
  throw new Error(`WASM size budget: ${message}`);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function exactKeys(value, expected, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object with exactly ${expected.join(", ")}`);
  }
  const actual = Object.keys(value);
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    fail(`${label} must contain exactly ${expected.join(", ")} in canonical order`);
  }
}

function lowercaseDigest(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/u.test(value)) {
    fail(`${label} must be a lowercase SHA-256 digest`);
  }
}

function positiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    fail(`${label} must be a positive safe integer`);
  }
}

function toolchainRecipe(v1) {
  const measurement = v1.measurement;
  return {
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
}

function readImmutableBudget(path, expectedSha256, version, budgetId) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    fail(`cannot read immutable ${version} budget ${path}: ${error.message}`);
  }
  const actualSha256 = sha256(bytes);
  if (actualSha256 !== expectedSha256) {
    fail(
      `immutable ${version} file SHA-256 mismatch: ` +
        `expected=${expectedSha256} actual=${actualSha256}`,
    );
  }
  let budget;
  try {
    budget = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    fail(`immutable ${version} budget is not JSON: ${error.message}`);
  }
  if (budget?.schemaVersion !== 3 || budget?.budgetId !== budgetId) {
    fail(`immutable ${version} budget identity drifted`);
  }
  return budget;
}

function verifyImmutableHistory() {
  let bytes;
  try {
    bytes = readFileSync(V1_PATH);
  } catch (error) {
    fail(`cannot read immutable build recipe ${V1_PATH}: ${error.message}`);
  }
  if (sha256(bytes) !== V1_FILE_SHA256) {
    fail(`immutable v1 file SHA-256 mismatch: expected=${V1_FILE_SHA256} actual=${sha256(bytes)}`);
  }

  let v1;
  try {
    v1 = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    fail(`immutable v1 build recipe is not JSON: ${error.message}`);
  }
  if (
    v1?.schemaVersion !== 2 ||
    v1?.budgetId !== "labcolors-wasm-raw-issue-284-v1"
  ) {
    fail("immutable v1 build recipe identity drifted");
  }
  const actualRecipeSha256 = sha256(JSON.stringify(toolchainRecipe(v1)));
  if (actualRecipeSha256 !== V1_RECIPE_SHA256) {
    fail(
      `immutable v1 toolchain recipe SHA-256 mismatch: ` +
        `expected=${V1_RECIPE_SHA256} actual=${actualRecipeSha256}`,
    );
  }

  readImmutableBudget(
    V2_PATH,
    V2_FILE_SHA256,
    "v2",
    "labcolors-wasm-raw-issue-295-v2",
  );
  return readImmutableBudget(
    V3_PATH,
    V3_FILE_SHA256,
    "v3",
    "labcolors-wasm-raw-issue-296-v3",
  );
}

function validateBudgetValue(budget) {
  exactKeys(
    budget,
    ["schemaVersion", "budgetId", "artifact", "buildRecipe", "measurement", "policy"],
    "budget",
  );
  if (budget.schemaVersion !== 3) fail("supported schemaVersion is exactly 3");
  if (budget.budgetId !== V4_BUDGET_ID) fail(`budgetId must be ${V4_BUDGET_ID}`);
  if (budget.artifact !== WASM_REPOSITORY_PATH) {
    fail(`artifact must be ${WASM_REPOSITORY_PATH}`);
  }

  exactKeys(
    budget.buildRecipe,
    ["path", "fileSha256", "recipeSha256"],
    "buildRecipe",
  );
  if (budget.buildRecipe.path !== V1_REPOSITORY_PATH) {
    fail(`buildRecipe.path must be ${V1_REPOSITORY_PATH}`);
  }
  if (budget.buildRecipe.fileSha256 !== V1_FILE_SHA256) {
    fail("buildRecipe.fileSha256 must bind the immutable v1 file");
  }
  if (budget.buildRecipe.recipeSha256 !== V1_RECIPE_SHA256) {
    fail("buildRecipe.recipeSha256 must bind the canonical v1 toolchain projection");
  }

  exactKeys(
    budget.measurement,
    ["issue", "measurementPlatform", "rawBytes", "sha256"],
    "measurement",
  );
  if (budget.measurement.issue !== 296) fail("measurement must cite Issue #296");
  if (budget.measurement.measurementPlatform !== "linux-x64") {
    fail("measurement.measurementPlatform must be canonical linux-x64");
  }
  positiveSafeInteger(budget.measurement.rawBytes, "measurement.rawBytes");
  lowercaseDigest(budget.measurement.sha256, "measurement.sha256");

  exactKeys(budget.policy, ["maxRawBytes", "derivation", "gzip"], "policy");
  positiveSafeInteger(budget.policy.maxRawBytes, "policy.maxRawBytes");
  if (budget.policy.maxRawBytes !== budget.measurement.rawBytes) {
    fail("current ceiling must equal the exact accepted measurement (zero arbitrary headroom)");
  }
  if (budget.policy.derivation !== "exact-accepted-issue-296-slice-b-measurement") {
    fail("policy.derivation must cite the exact accepted Issue #296 Slice B measurement");
  }
  if (budget.policy.gzip !== "diagnostic-only") {
    fail("gzip must remain diagnostic-only across implementations");
  }

  const previous = verifyImmutableHistory();
  positiveSafeInteger(previous.policy?.maxRawBytes, "immutable v3 policy.maxRawBytes");
  if (budget.policy.maxRawBytes > previous.policy.maxRawBytes) {
    fail("current ceiling must not exceed the immutable v3 ratchet");
  }
}

export function parseBudgetDocument(bytes, budgetPath) {
  const document = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes);
  let budget;
  try {
    budget = JSON.parse(document.toString("utf8"));
  } catch (error) {
    fail(`cannot parse ${budgetPath}: ${error.message}`);
  }
  const canonical = `${JSON.stringify(budget, null, 2)}\n`;
  if (!document.equals(Buffer.from(canonical, "utf8"))) {
    fail(`${budgetPath} must be canonical JSON with two-space indentation and one final newline`);
  }
  validateBudgetValue(budget);
  if (resolve(budgetPath) === DEFAULT_BUDGET) {
    const actualFileSha256 = sha256(document);
    if (actualFileSha256 !== V4_FILE_SHA256) {
      fail(
        `immutable v4 file SHA-256 mismatch: ` +
          `expected=${V4_FILE_SHA256} actual=${actualFileSha256}`,
      );
    }
  }
  return budget;
}

function readBudget(path) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    fail(`cannot read ${path}: ${error.message}`);
  }
  return parseBudgetDocument(bytes, path);
}

function pathsFromArgs(args) {
  let wasm = DEFAULT_WASM;
  let budget = DEFAULT_BUDGET;
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (value === undefined) fail(`${flag ?? "argument"} requires a path`);
    if (flag === "--wasm") wasm = resolve(value);
    else if (flag === "--budget") budget = resolve(value);
    else fail(`unknown argument ${flag}`);
  }
  return { wasm, budget };
}

export function evaluateWasmBudget(budget, wasm, currentPlatform) {
  const bytes = Buffer.isBuffer(wasm) ? wasm : Buffer.from(wasm);
  if (
    bytes.length < 8 ||
    !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))
  ) {
    fail("artifact is not a WebAssembly binary");
  }

  const rawBytes = bytes.length;
  const gzipBytes = gzipSync(bytes, { level: 9 }).length;
  const artifactSha256 = sha256(bytes);
  const artifactSha = artifactSha256 === budget.measurement.sha256 ? "match" : "different";
  const isCanonicalPlatform = currentPlatform === budget.measurement.measurementPlatform;
  if (isCanonicalPlatform && rawBytes !== budget.measurement.rawBytes) {
    fail(
      `exact artifact length mismatch on ${currentPlatform}: ` +
        `expected=${budget.measurement.rawBytes}B actual=${rawBytes}B; ` +
        `gzip=${gzipBytes}B diagnostic-only sha256=${artifactSha256}`,
    );
  }
  if (isCanonicalPlatform && artifactSha !== "match") {
    fail(
      `exact artifact SHA-256 mismatch on ${currentPlatform}: ` +
        `expected=${budget.measurement.sha256} actual=${artifactSha256}; ` +
        `raw=${rawBytes}B gzip=${gzipBytes}B diagnostic-only`,
    );
  }

  return {
    status: isCanonicalPlatform ? "PASS" : "DIAGNOSTIC",
    rawBytes,
    maxRawBytes: budget.policy.maxRawBytes,
    deltaBytes: rawBytes - budget.policy.maxRawBytes,
    gzipBytes,
    currentPlatform,
    artifactSha,
    artifactSha256,
  };
}

function formatResult(result, artifact) {
  const delta = `${result.deltaBytes >= 0 ? "+" : ""}${result.deltaBytes}`;
  return (
    `WASM size budget ${result.status} raw=${result.rawBytes}B ` +
    `ceiling=${result.maxRawBytes}B delta=${delta}B gzip=${result.gzipBytes}B ` +
    `diagnostic-only platform=${result.currentPlatform} artifact=${artifact} ` +
    `artifact-sha=${result.artifactSha} recipe-sha=match`
  );
}

function main(args) {
  const { wasm: wasmPath, budget: budgetPath } = pathsFromArgs(args);
  const budget = readBudget(budgetPath);
  let wasm;
  try {
    wasm = readFileSync(wasmPath);
  } catch (error) {
    fail(`cannot read ${wasmPath}: ${error.message}`);
  }
  const artifact = relative(REPO_ROOT, wasmPath).replaceAll("\\", "/");
  const result = evaluateWasmBudget(
    budget,
    wasm,
    `${process.platform}-${process.arch}`,
  );
  console.log(formatResult(result, artifact));
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  main(process.argv.slice(2));
}
