#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const V1_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v1.json");
const V2_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v2.json");
const V3_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v3.json");
const V4_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v4.json");
const V5_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v5.json");
const V6_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v6.json");
const V7_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v7.json");
const V8_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v8.json");
const V9_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v9.json");
const V10_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v10.json");
const V11_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v11.json");
const V12_PATH = resolve(REPO_ROOT, "packages/colors/bench/wasm-size-budget-v12.json");

export const DEFAULT_BUDGET = resolve(
  REPO_ROOT,
  "packages/colors/bench/wasm-size-budget-v13.json",
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
  "c34fc10404dc7057a53a28592d18342078b5cd0e5dcaa888db482abf3f5fb23c";
export const V5_FILE_SHA256 =
  "e4b53a2eb976a8c66827a559cb81232e359b734dbfb14725da215cb496ff5d59";
export const V6_FILE_SHA256 =
  "761af6050031169dac7eafdfadb2db9bbb2023b96ed5ba9d3c5dc966ffeafb32";
export const V7_FILE_SHA256 =
  "01d17c042b7dc36585e9657490048932fdf61d4715099b735aa3bf2d3dc5777e";
export const V8_FILE_SHA256 =
  "3590ffd2d158c2caf5cfbd26489e609b08d1cb640584456baa2166ccf50f5109";
export const V9_FILE_SHA256 =
  "e00fa0549d67ab027f589c053aeb4374f6437704a6277cc9784dcaa1d8015ad4";
export const V10_FILE_SHA256 =
  "6f3318c29c633860a146be5dcd29e4ce85a3a52296b9719b506aba16951a58e6";
export const V11_FILE_SHA256 =
  "fa11531ee390dd6dfdfadfadab99bbe8277f2b152b567951b17ef6093d42b1e4";
export const V12_FILE_SHA256 =
  "925452113b18b63137b9dae4786e3a8f7ba098eb47a2631a97107fbd52aa9a95";
export const V13_FILE_SHA256 =
  "3cc88303a0f43e8ca33ae70d723a3179c68b0cc2744310a791e8f43885482f34";

const V1_REPOSITORY_PATH = "packages/colors/bench/wasm-size-budget-v1.json";
const V12_REPOSITORY_PATH = "packages/colors/bench/wasm-size-budget-v12.json";
const V5_BUDGET_ID = "labcolors-wasm-roles-issue-296-c1-v5";
const V6_BUDGET_ID = "labcolors-wasm-roles-issue-296-c3-v6";
const V7_BUDGET_ID = "labcolors-wasm-roles-issue-307-c7a-v7";
const V8_BUDGET_ID = "labcolors-wasm-roles-pr-338-v8";
const V9_BUDGET_ID = "labcolors-wasm-roles-c4a-v9";
const V10_BUDGET_ID = "labcolors-wasm-roles-failure-admissibility-v10";
const V11_BUDGET_ID = "labcolors-wasm-runtime-c4cd-v11";
const V12_BUDGET_ID = "labcolors-wasm-runtime-c5-theme-keys-v12";
const V13_BUDGET_ID = "labcolors-wasm-runtime-c5-2-proxy-excision-v13";
const ROLE_ORDER = ["runtime"];
const ROLE_SPECS = {
  runtime: {
    artifact: "packages/colors/pkg/labcolors_bg.wasm",
    command:
      "CARGO_ENCODED_RUSTFLAGS=<rustPathRemap> wasm-pack build crates/labcolors-wasm --release --target web --out-dir ../../packages/colors/pkg --out-name labcolors --locked",
    recipeSha256: V1_RECIPE_SHA256,
    // Pinned Linux run 29613131229 измерил C5.2 head точно (−4691B от v12: вырез
    // muddiness-метода и legacy-прокси). Любой дальнейший рост требует НОВОЙ
    // версии снапшота, не headroom здесь.
    basis: "accepted-c5-2-proxy-excision-snapshot",
    measurementSource: "github-actions-run-29613131229",
    acceptedCeiling: 455074,
  },
};

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

function roleRecipe(v1, command) {
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
    command,
  };
}

function readImmutableJson(path, expectedSha256, label) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    fail(`cannot read immutable ${label} ${path}: ${error.message}`);
  }
  const actualSha256 = sha256(bytes);
  if (actualSha256 !== expectedSha256) {
    fail(
      `immutable ${label} file SHA-256 mismatch: ` +
        `expected=${expectedSha256} actual=${actualSha256}`,
    );
  }
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    fail(`immutable ${label} is not JSON: ${error.message}`);
  }
}

function verifyImmutableHistory() {
  const v1 = readImmutableJson(V1_PATH, V1_FILE_SHA256, "v1");
  if (v1?.schemaVersion !== 2 || v1?.budgetId !== "labcolors-wasm-raw-issue-284-v1") {
    fail("immutable v1 build recipe identity drifted");
  }
  if (sha256(JSON.stringify(roleRecipe(v1, v1.measurement.command))) !== V1_RECIPE_SHA256) {
    fail("immutable v1 runtime recipe projection drifted");
  }

  const historical = [
    [V2_PATH, V2_FILE_SHA256, "v2", "labcolors-wasm-raw-issue-295-v2"],
    [V3_PATH, V3_FILE_SHA256, "v3", "labcolors-wasm-raw-issue-296-v3"],
    [V4_PATH, V4_FILE_SHA256, "v4", "labcolors-wasm-raw-issue-296-v4"],
  ];
  let v4;
  for (const [path, digest, label, budgetId] of historical) {
    const value = readImmutableJson(path, digest, label);
    if (value?.schemaVersion !== 3 || value?.budgetId !== budgetId) {
      fail(`immutable ${label} budget identity drifted`);
    }
    if (label === "v4") v4 = value;
  }
  const v5 = readImmutableJson(V5_PATH, V5_FILE_SHA256, "v5");
  if (v5?.schemaVersion !== 4 || v5?.budgetId !== V5_BUDGET_ID) {
    fail("immutable v5 budget identity drifted");
  }
  const v6 = readImmutableJson(V6_PATH, V6_FILE_SHA256, "v6");
  if (v6?.schemaVersion !== 5 || v6?.budgetId !== V6_BUDGET_ID) {
    fail("immutable v6 budget identity drifted");
  }
  const v7 = readImmutableJson(V7_PATH, V7_FILE_SHA256, "v7");
  if (v7?.schemaVersion !== 6 || v7?.budgetId !== V7_BUDGET_ID) {
    fail("immutable v7 budget identity drifted");
  }
  const v8 = readImmutableJson(V8_PATH, V8_FILE_SHA256, "v8");
  if (v8?.schemaVersion !== 7 || v8?.budgetId !== V8_BUDGET_ID) {
    fail("immutable v8 budget identity drifted");
  }
  const v9 = readImmutableJson(V9_PATH, V9_FILE_SHA256, "v9");
  if (v9?.schemaVersion !== 7 || v9?.budgetId !== V9_BUDGET_ID) {
    fail("immutable v9 budget identity drifted");
  }
  const v10 = readImmutableJson(V10_PATH, V10_FILE_SHA256, "v10");
  if (v10?.schemaVersion !== 7 || v10?.budgetId !== V10_BUDGET_ID) {
    fail("immutable v10 budget identity drifted");
  }
  const v11 = readImmutableJson(V11_PATH, V11_FILE_SHA256, "v11");
  if (v11?.schemaVersion !== 8 || v11?.budgetId !== V11_BUDGET_ID) {
    fail("immutable v11 budget identity drifted");
  }
  const v12 = readImmutableJson(V12_PATH, V12_FILE_SHA256, "v12");
  if (v12?.schemaVersion !== 8 || v12?.budgetId !== V12_BUDGET_ID) {
    fail("immutable v12 budget identity drifted");
  }
  return { v1, v4, v5, v6, v7, v8, v9, v10, v11, v12 };
}

function validateBudgetValue(budget) {
  exactKeys(
    budget,
    [
      "schemaVersion",
      "budgetId",
      "predecessor",
      "toolchainSource",
      "buildRecipes",
      "roles",
    ],
    "budget",
  );
  if (budget.schemaVersion !== 8) fail("supported schemaVersion is exactly 8");
  if (budget.budgetId !== V13_BUDGET_ID) fail(`budgetId must be ${V13_BUDGET_ID}`);

  exactKeys(budget.predecessor, ["path", "fileSha256"], "predecessor");
  if (
    budget.predecessor.path !== V12_REPOSITORY_PATH ||
    budget.predecessor.fileSha256 !== V12_FILE_SHA256
  ) {
    fail("predecessor must bind the immutable v12 document");
  }

  exactKeys(budget.toolchainSource, ["path", "fileSha256"], "toolchainSource");
  if (
    budget.toolchainSource.path !== V1_REPOSITORY_PATH ||
    budget.toolchainSource.fileSha256 !== V1_FILE_SHA256
  ) {
    fail("toolchainSource must bind the immutable v1 document");
  }

  exactKeys(budget.buildRecipes, ROLE_ORDER, "buildRecipes");
  exactKeys(budget.roles, ROLE_ORDER, "roles");
  const { v1 } = verifyImmutableHistory();

  for (const role of ROLE_ORDER) {
    const spec = ROLE_SPECS[role];
    const recipe = budget.buildRecipes[role];
    exactKeys(recipe, ["command", "recipeSha256"], `buildRecipes.${role}`);
    if (recipe.command !== spec.command) fail(`${role} build command drifted`);
    lowercaseDigest(recipe.recipeSha256, `buildRecipes.${role}.recipeSha256`);
    const actualRecipeSha256 = sha256(JSON.stringify(roleRecipe(v1, recipe.command)));
    if (
      recipe.recipeSha256 !== spec.recipeSha256 ||
      recipe.recipeSha256 !== actualRecipeSha256
    ) {
      fail(`${role} build recipe SHA-256 does not bind the declared command and toolchain`);
    }

    const record = budget.roles[role];
    exactKeys(record, ["artifact", "measurement", "policy"], `roles.${role}`);
    if (record.artifact !== spec.artifact) {
      fail(`roles.${role}.artifact must be ${spec.artifact}`);
    }
    exactKeys(
      record.measurement,
      ["source", "measurementPlatform", "rawBytes"],
      `roles.${role}.measurement`,
    );
    if (record.measurement.source !== spec.measurementSource) {
      fail(`roles.${role}.measurement source drifted`);
    }
    if (record.measurement.measurementPlatform !== "linux-x64") {
      fail(`roles.${role}.measurement must use canonical linux-x64`);
    }
    positiveSafeInteger(record.measurement.rawBytes, `roles.${role}.measurement.rawBytes`);

    exactKeys(record.policy, ["maxRawBytes", "basis", "gzip"], `roles.${role}.policy`);
    positiveSafeInteger(record.policy.maxRawBytes, `roles.${role}.policy.maxRawBytes`);
    if (record.policy.maxRawBytes !== record.measurement.rawBytes) {
      fail(`${role} ceiling must equal its exact measurement (zero arbitrary headroom)`);
    }
    if (
      spec.acceptedCeiling !== undefined &&
      record.policy.maxRawBytes > spec.acceptedCeiling
    ) {
      fail(`${role} exceeds its accepted snapshot ceiling`);
    }
    if (record.policy.basis !== spec.basis) {
      fail(`${role} policy basis drifted`);
    }
    if (record.policy.gzip !== "diagnostic-only") {
      fail(`${role} gzip measurement must remain diagnostic-only`);
    }
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
    if (actualFileSha256 !== V13_FILE_SHA256) {
      fail(
        `current v13 file SHA-256 mismatch: ` +
          `expected=${V13_FILE_SHA256} actual=${actualFileSha256}`,
      );
    }
  }
  return budget;
}

function readBudget(path) {
  try {
    return parseBudgetDocument(readFileSync(path), path);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("WASM size budget:")) throw error;
    fail(`cannot read ${path}: ${error.message}`);
  }
}

export function evaluateWasmBudget(role, record, wasm, currentPlatform) {
  if (!ROLE_ORDER.includes(role)) fail(`unknown execution role ${role}`);
  const bytes = Buffer.isBuffer(wasm) ? wasm : Buffer.from(wasm);
  if (
    bytes.length < 8 ||
    !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))
  ) {
    fail(`${role} artifact is not a WebAssembly binary`);
  }

  const rawBytes = bytes.length;
  const gzipBytes = gzipSync(bytes, { level: 9 }).length;
  const artifactSha256 = sha256(bytes);
  const isCanonicalPlatform = currentPlatform === record.measurement.measurementPlatform;
  if (isCanonicalPlatform && rawBytes !== record.measurement.rawBytes) {
    fail(
      `${role} exact artifact length mismatch on ${currentPlatform}: ` +
        `expected=${record.measurement.rawBytes}B actual=${rawBytes}B; ` +
        `gzip=${gzipBytes}B diagnostic-only sha256=${artifactSha256}`,
    );
  }
  return {
    role,
    status: isCanonicalPlatform ? "PASS" : "DIAGNOSTIC",
    rawBytes,
    maxRawBytes: record.policy.maxRawBytes,
    deltaBytes: rawBytes - record.policy.maxRawBytes,
    gzipBytes,
    currentPlatform,
    artifactSha256,
  };
}

function formatResult(result, artifact) {
  const delta = `${result.deltaBytes >= 0 ? "+" : ""}${result.deltaBytes}`;
  return (
    `WASM size budget ${result.status} role=${result.role} raw=${result.rawBytes}B ` +
    `ceiling=${result.maxRawBytes}B delta=${delta}B gzip=${result.gzipBytes}B ` +
    `diagnostic-only platform=${result.currentPlatform} artifact=${artifact} ` +
    `artifact-sha256=${result.artifactSha256}`
  );
}

function pathsFromArgs(args) {
  const paths = {
    budget: DEFAULT_BUDGET,
    runtime: resolve(REPO_ROOT, ROLE_SPECS.runtime.artifact),
  };
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (value === undefined) fail(`${flag ?? "argument"} requires a path`);
    if (flag === "--budget") paths.budget = resolve(value);
    else if (flag === "--runtime-wasm") paths.runtime = resolve(value);
    else fail(`unknown argument ${flag}`);
  }
  return paths;
}

function main(args) {
  const paths = pathsFromArgs(args);
  const budget = readBudget(paths.budget);
  for (const role of ROLE_ORDER) {
    let wasm;
    try {
      wasm = readFileSync(paths[role]);
    } catch (error) {
      fail(`cannot read ${role} artifact ${paths[role]}: ${error.message}`);
    }
    const result = evaluateWasmBudget(
      role,
      budget.roles[role],
      wasm,
      `${process.platform}-${process.arch}`,
    );
    const artifact = relative(REPO_ROOT, paths[role]).replaceAll("\\", "/");
    console.log(formatResult(result, artifact));
  }
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  main(process.argv.slice(2));
}
