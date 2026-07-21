#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");

export const DEFAULT_BUDGET = resolve(
  REPO_ROOT,
  "packages/colors/bench/wasm.json",
);
export const WASM_BUDGET_FILE_SHA256 =
  "1fc218f1ed02aabe298f43a484bebef3d046269e05eb479a145b86cfde8a7a30";

const SCHEMA_VERSION = 1;
const CANONICAL_ARTIFACT = "packages/colors/pkg/labcolors_bg.wasm";
const CANONICAL_PLATFORM = "linux-x64";
const TOOLCHAIN_KEYS = [
  "rust",
  "rustcCommit",
  "wasmPack",
  "wasmBindgen",
  "target",
  "cargoProfile",
  "wasmOpt",
  "wasmOptVersion",
];
const NUMBERED_BUDGET = /^wasm-size-budget-v\d+\.json$/u;
const PATH_REMAP = /^[A-Z][A-Z0-9_]*=\/[^\r\n]+$/u;
const MEASUREMENT_SOURCE = /^github-actions-run-[1-9][0-9]*$/u;

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

function nonEmptyLine(value, label) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.includes("\r") ||
    value.includes("\n")
  ) {
    fail(`${label} must be one non-empty line`);
  }
}

function positiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    fail(`${label} must be a positive safe integer`);
  }
}

function validateBudgetValue(budget) {
  exactKeys(
    budget,
    ["schemaVersion", "artifact", "toolchain", "recipe", "measurement", "policy"],
    "budget",
  );
  if (budget.schemaVersion !== SCHEMA_VERSION) {
    fail(`schemaVersion must be ${SCHEMA_VERSION}`);
  }
  if (budget.artifact !== CANONICAL_ARTIFACT) {
    fail(`artifact must be ${CANONICAL_ARTIFACT}`);
  }

  exactKeys(budget.toolchain, TOOLCHAIN_KEYS, "toolchain");
  for (const key of TOOLCHAIN_KEYS) {
    nonEmptyLine(budget.toolchain[key], `toolchain.${key}`);
  }

  exactKeys(budget.recipe, ["rustPathRemap", "command"], "recipe");
  if (
    !Array.isArray(budget.recipe.rustPathRemap) ||
    budget.recipe.rustPathRemap.length === 0 ||
    budget.recipe.rustPathRemap.some((mapping) => !PATH_REMAP.test(mapping)) ||
    new Set(budget.recipe.rustPathRemap).size !== budget.recipe.rustPathRemap.length
  ) {
    fail("recipe.rustPathRemap must contain unique canonical environment mappings");
  }
  nonEmptyLine(budget.recipe.command, "recipe.command");
  if (
    budget.recipe.command.match(/<rustPathRemap>/gu)?.length !== 1 ||
    !budget.recipe.command.startsWith("CARGO_ENCODED_RUSTFLAGS=<rustPathRemap> ")
  ) {
    fail("recipe.command must consume rustPathRemap exactly once in one command");
  }

  exactKeys(
    budget.measurement,
    ["source", "platform", "rawBytes"],
    "measurement",
  );
  if (
    typeof budget.measurement.source !== "string" ||
    !MEASUREMENT_SOURCE.test(budget.measurement.source)
  ) {
    fail("measurement.source must identify one GitHub Actions run");
  }
  if (budget.measurement.platform !== CANONICAL_PLATFORM) {
    fail(`measurement.platform must be ${CANONICAL_PLATFORM}`);
  }
  positiveSafeInteger(budget.measurement.rawBytes, "measurement.rawBytes");

  exactKeys(budget.policy, ["maxRawBytes", "basis", "gzip"], "policy");
  positiveSafeInteger(budget.policy.maxRawBytes, "policy.maxRawBytes");
  if (budget.policy.maxRawBytes !== budget.measurement.rawBytes) {
    fail("policy.maxRawBytes must equal measurement.rawBytes (zero arbitrary headroom)");
  }
  nonEmptyLine(budget.policy.basis, "policy.basis");
  if (budget.policy.gzip !== "diagnostic-only") {
    fail("policy.gzip must be diagnostic-only");
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
    if (actualFileSha256 !== WASM_BUDGET_FILE_SHA256) {
      fail(
        `current budget file SHA-256 mismatch: ` +
          `expected=${WASM_BUDGET_FILE_SHA256} actual=${actualFileSha256}`,
      );
    }
  }
  return budget;
}

function readBudget(path) {
  try {
    return parseBudgetDocument(readFileSync(path), path);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("WASM size budget:")) {
      throw error;
    }
    fail(`cannot read ${path}: ${error.message}`);
  }
}

function rejectNumberedBudgetSiblings() {
  const numbered = readdirSync(dirname(DEFAULT_BUDGET)).filter((name) =>
    NUMBERED_BUDGET.test(name)
  );
  if (numbered.length > 0) {
    fail(`numbered budget snapshots duplicate Git history: ${numbered.join(", ")}`);
  }
}

export function evaluateWasmBudget(budget, wasm, currentPlatform) {
  const bytes = Buffer.isBuffer(wasm) ? wasm : Buffer.from(wasm);
  if (
    bytes.length < 8 ||
    !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))
  ) {
    fail("runtime artifact is not a WebAssembly binary");
  }

  const rawBytes = bytes.length;
  const gzipBytes = gzipSync(bytes, { level: 9 }).length;
  const artifactSha256 = sha256(bytes);
  const isCanonicalPlatform = currentPlatform === budget.measurement.platform;
  if (isCanonicalPlatform && rawBytes !== budget.measurement.rawBytes) {
    fail(
      `runtime exact artifact length mismatch on ${currentPlatform}: ` +
        `expected=${budget.measurement.rawBytes}B actual=${rawBytes}B; ` +
        `gzip=${gzipBytes}B diagnostic-only sha256=${artifactSha256}`,
    );
  }
  return {
    status: isCanonicalPlatform ? "PASS" : "DIAGNOSTIC",
    rawBytes,
    maxRawBytes: budget.policy.maxRawBytes,
    deltaBytes: rawBytes - budget.policy.maxRawBytes,
    gzipBytes,
    currentPlatform,
    artifactSha256,
  };
}

function formatResult(result, artifact) {
  const delta = `${result.deltaBytes >= 0 ? "+" : ""}${result.deltaBytes}`;
  return (
    `WASM size budget ${result.status} role=runtime raw=${result.rawBytes}B ` +
    `ceiling=${result.maxRawBytes}B delta=${delta}B gzip=${result.gzipBytes}B ` +
    `diagnostic-only platform=${result.currentPlatform} artifact=${artifact} ` +
    `artifact-sha256=${result.artifactSha256}`
  );
}

function pathsFromArgs(args) {
  const paths = {
    budget: DEFAULT_BUDGET,
    runtime: undefined,
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
  rejectNumberedBudgetSiblings();
  const paths = pathsFromArgs(args);
  const budget = readBudget(paths.budget);
  const runtimePath = paths.runtime ?? resolve(REPO_ROOT, budget.artifact);
  let wasm;
  try {
    wasm = readFileSync(runtimePath);
  } catch (error) {
    fail(`cannot read runtime artifact ${runtimePath}: ${error.message}`);
  }
  const result = evaluateWasmBudget(
    budget,
    wasm,
    `${process.platform}-${process.arch}`,
  );
  const artifact = relative(REPO_ROOT, runtimePath).replaceAll("\\", "/");
  console.log(formatResult(result, artifact));
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  main(process.argv.slice(2));
}
