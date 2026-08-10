#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import {
  PRIVATE_PROGRAM_CANONICAL_BUILD,
  PRIVATE_PROGRAM_ROLE,
  PRIVATE_PROGRAM_WASM_PATH,
} from "./build-private-program.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");

export { PRIVATE_PROGRAM_ROLE };
export const PRIVATE_PROGRAM_CANONICAL_PLATFORM = "linux-x64";
export const DEFAULT_PRIVATE_PROGRAM_BUDGET = resolve(
  REPO_ROOT,
  "packages/colors/bench/private-program-wasm.json",
);
export const DEFAULT_PRIVATE_PROGRAM_WASM = resolve(
  REPO_ROOT,
  "packages/colors/private-program/labcolors_private_program.wasm",
);

const SCHEMA_VERSION = 1;
const CANONICAL_ARTIFACT =
  "packages/colors/private-program/labcolors_private_program.wasm";
const TOOLCHAIN_KEYS = [
  "rust",
  "rustcCommit",
  "cargo",
  "cargoCommit",
  "target",
  "profile",
  "feature",
  "node",
  "binaryenRelease",
  "binaryenNodeArchiveSha256",
  "binaryenComponentSha256",
  "wasmOptFlags",
];
const BINARYEN_COMPONENT_KEYS = [
  "wasm-opt.js",
  "wasm-opt.wasm",
  "wasm-opt.worker.js",
];
const MEASUREMENT_SOURCE = /^github-actions-run-[1-9][0-9]*$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const WASM_MAGIC = Buffer.from([0x00, 0x61, 0x73, 0x6d]);

function fail(message, options) {
  throw new Error(`private Program WASM size budget: ${message}`, options);
}

function deepFreeze(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) deepFreeze(child);
  }
  return value;
}

function canonicalToolchain() {
  const build = PRIVATE_PROGRAM_CANONICAL_BUILD;
  const optimizer = build?.toolchain?.optimizer;
  const optimizerArgs = build?.recipe?.optimizer?.args;
  if (
    PRIVATE_PROGRAM_WASM_PATH !== "private-program/labcolors_private_program.wasm" ||
    optimizer === null ||
    typeof optimizer !== "object" ||
    !Array.isArray(optimizerArgs) ||
    optimizerArgs.length <= 3 ||
    optimizerArgs[0] !== "$RAW_WASM" ||
    optimizerArgs[1] !== "-o" ||
    optimizerArgs[2] !== "$OPTIMIZED_WASM"
  ) {
    fail("canonical private Program optimizer descriptor is incomplete");
  }
  const binaryenComponentSha256 = Object.fromEntries(
    BINARYEN_COMPONENT_KEYS.map((name) => {
      const digest = optimizer.files?.[name]?.sha256;
      if (typeof digest !== "string" || !SHA256.test(digest)) {
        fail(`canonical private Program optimizer is missing the ${name} digest`);
      }
      return [name, digest];
    }),
  );
  return deepFreeze({
    rust: build.toolchain.rust,
    rustcCommit: build.toolchain.rustcCommit,
    cargo: build.toolchain.cargo,
    cargoCommit: build.toolchain.cargoCommit,
    target: build.target,
    profile: build.profile,
    feature: build.feature,
    node: optimizer.node,
    binaryenRelease: optimizer.binaryenRelease,
    binaryenNodeArchiveSha256: optimizer.binaryenNodeArchiveSha256,
    binaryenComponentSha256,
    wasmOptFlags: optimizerArgs.slice(3).join(" "),
  });
}

export const PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN = canonicalToolchain();

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

function validateToolchain(toolchain) {
  exactKeys(toolchain, TOOLCHAIN_KEYS, "toolchain");
  for (const key of TOOLCHAIN_KEYS) {
    if (key === "binaryenComponentSha256") continue;
    nonEmptyLine(toolchain[key], `toolchain.${key}`);
    if (toolchain[key] !== PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN[key]) {
      fail(
        `toolchain.${key} must equal the canonical private Program build pin ` +
          `${PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN[key]}`,
      );
    }
  }
  exactKeys(
    toolchain.binaryenComponentSha256,
    BINARYEN_COMPONENT_KEYS,
    "toolchain.binaryenComponentSha256",
  );
  for (const name of BINARYEN_COMPONENT_KEYS) {
    const digest = toolchain.binaryenComponentSha256[name];
    if (!SHA256.test(digest ?? "")) {
      fail(`toolchain.binaryenComponentSha256.${name} must be lowercase SHA-256`);
    }
    if (digest !== PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN.binaryenComponentSha256[name]) {
      fail(
        `toolchain.binaryenComponentSha256.${name} must equal the canonical ` +
          "private Program build pin",
      );
    }
  }
}

function validateBudgetValue(budget) {
  if (
    budget !== null &&
    typeof budget === "object" &&
    !Array.isArray(budget) &&
    budget.status === "pending"
  ) {
    fail("budget is pending; only a measured Linux budget can pass");
  }
  exactKeys(
    budget,
    ["schemaVersion", "role", "artifact", "toolchain", "measurement", "policy"],
    "budget",
  );
  if (budget.schemaVersion !== SCHEMA_VERSION) {
    fail(`schemaVersion must be ${SCHEMA_VERSION}`);
  }
  if (budget.role !== PRIVATE_PROGRAM_ROLE) {
    fail(`role must be ${PRIVATE_PROGRAM_ROLE}`);
  }
  if (budget.artifact !== CANONICAL_ARTIFACT) {
    fail(`artifact must be ${CANONICAL_ARTIFACT}`);
  }
  validateToolchain(budget.toolchain);

  exactKeys(
    budget.measurement,
    ["source", "platform", "rawBytes"],
    "measurement",
  );
  if (
    typeof budget.measurement.source !== "string" ||
    !MEASUREMENT_SOURCE.test(budget.measurement.source)
  ) {
    fail("measurement.source must identify one real GitHub Actions run");
  }
  if (budget.measurement.platform !== PRIVATE_PROGRAM_CANONICAL_PLATFORM) {
    fail(`measurement.platform must be ${PRIVATE_PROGRAM_CANONICAL_PLATFORM}`);
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

export function parsePrivateProgramBudgetDocument(bytes, budgetPath) {
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
  return budget;
}

function readBudget(path) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    if (error?.code === "ENOENT") {
      fail(
        `budget file is missing at ${path}; record this observed artifact in a real ` +
          "GitHub Actions Linux run before adding the zero-headroom budget",
      );
    }
    fail(`cannot read budget ${path}: ${error.message}`, { cause: error });
  }
  return parsePrivateProgramBudgetDocument(bytes, path);
}

export function observePrivateProgramWasm(wasm, currentPlatform) {
  const bytes = Buffer.isBuffer(wasm) ? wasm : Buffer.from(wasm);
  if (bytes.length < 8 || !bytes.subarray(0, WASM_MAGIC.length).equals(WASM_MAGIC)) {
    fail("private Program artifact is not a WebAssembly binary");
  }
  return {
    rawBytes: bytes.length,
    gzipBytes: gzipSync(bytes, { level: 9 }).length,
    currentPlatform,
    artifactSha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

export function evaluatePrivateProgramWasmBudget(budget, observation) {
  positiveSafeInteger(budget?.measurement?.rawBytes, "measurement.rawBytes");
  positiveSafeInteger(budget?.policy?.maxRawBytes, "policy.maxRawBytes");
  if (budget.policy.maxRawBytes !== budget.measurement.rawBytes) {
    fail("policy.maxRawBytes must equal measurement.rawBytes (zero arbitrary headroom)");
  }
  const isCanonicalPlatform =
    observation.currentPlatform === budget.measurement.platform;
  if (isCanonicalPlatform && observation.rawBytes !== budget.measurement.rawBytes) {
    fail(
      `private Program exact artifact length mismatch on ${observation.currentPlatform}: ` +
        `expected=${budget.measurement.rawBytes}B actual=${observation.rawBytes}B; ` +
        `gzip=${observation.gzipBytes}B diagnostic-only ` +
        `sha256=${observation.artifactSha256}`,
    );
  }
  return {
    ...observation,
    status: isCanonicalPlatform ? "PASS" : "DIAGNOSTIC",
    maxRawBytes: budget.policy.maxRawBytes,
    deltaBytes: observation.rawBytes - budget.policy.maxRawBytes,
  };
}

function displayPath(path) {
  return relative(REPO_ROOT, path).replaceAll("\\", "/");
}

function formatObservation(observation, artifact) {
  return (
    `private Program WASM observed role=${PRIVATE_PROGRAM_ROLE} ` +
    `raw=${observation.rawBytes}B gzip=${observation.gzipBytes}B diagnostic-only ` +
    `platform=${observation.currentPlatform} artifact=${artifact} ` +
    `artifact-sha256=${observation.artifactSha256}`
  );
}

function formatResult(result, artifact) {
  const delta = `${result.deltaBytes >= 0 ? "+" : ""}${result.deltaBytes}`;
  return (
    `private Program WASM size budget ${result.status} role=${PRIVATE_PROGRAM_ROLE} ` +
    `raw=${result.rawBytes}B ceiling=${result.maxRawBytes}B delta=${delta}B ` +
    `gzip=${result.gzipBytes}B diagnostic-only platform=${result.currentPlatform} ` +
    `artifact=${artifact} artifact-sha256=${result.artifactSha256}`
  );
}

function pathsFromArgs(args) {
  const paths = {
    budget: DEFAULT_PRIVATE_PROGRAM_BUDGET,
    privateProgramWasm: DEFAULT_PRIVATE_PROGRAM_WASM,
  };
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (value === undefined) fail(`${flag ?? "argument"} requires a path`);
    if (flag === "--budget") paths.budget = resolve(value);
    else if (flag === "--private-program-wasm") {
      paths.privateProgramWasm = resolve(value);
    } else fail(`unknown argument ${flag}`);
  }
  return paths;
}

function main(args) {
  const paths = pathsFromArgs(args);
  let wasm;
  try {
    wasm = readFileSync(paths.privateProgramWasm);
  } catch (error) {
    fail(
      `cannot read private Program artifact ${paths.privateProgramWasm}: ${error.message}`,
      { cause: error },
    );
  }
  const artifact = displayPath(paths.privateProgramWasm);
  const observation = observePrivateProgramWasm(
    wasm,
    `${process.platform}-${process.arch}`,
  );
  process.stdout.write(`${formatObservation(observation, artifact)}\n`);
  const budget = readBudget(paths.budget);
  const result = evaluatePrivateProgramWasmBudget(budget, observation);
  process.stdout.write(`${formatResult(result, artifact)}\n`);
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  }
}
