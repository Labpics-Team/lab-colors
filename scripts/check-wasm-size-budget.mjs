#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SCRIPT_DIR, "..");
const DEFAULT_WASM = resolve(REPO_ROOT, "packages/colors/pkg/labcolors_bg.wasm");
const DEFAULT_BUDGET = resolve(
  REPO_ROOT,
  "packages/colors/bench/wasm-size-budget-v1.json",
);

function fail(message) {
  throw new Error(`WASM size budget: ${message}`);
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

function readBudget(path) {
  let budget;
  try {
    budget = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`cannot read ${path}: ${error.message}`);
  }
  const measurement = budget?.measurement;
  const policy = budget?.policy;
  if (budget?.schemaVersion !== 1) fail("supported schemaVersion is exactly 1");
  if (budget?.budgetId !== "labcolors-wasm-raw-issue-284-v1") {
    fail("unexpected budgetId");
  }
  if (measurement?.issue !== 284) fail("measurement must cite Issue #284");
  if (!Number.isSafeInteger(measurement?.rawBytes) || measurement.rawBytes <= 0) {
    fail("measurement.rawBytes must be a positive safe integer");
  }
  if (!/^[0-9a-f]{64}$/u.test(measurement?.sha256 ?? "")) {
    fail("measurement.sha256 must identify the exact measured artifact");
  }
  if (
    !Number.isSafeInteger(measurement?.gzip9BytesDiagnostic) ||
    measurement.gzip9BytesDiagnostic <= 0
  ) {
    fail("measurement.gzip9BytesDiagnostic must be a positive measured integer");
  }
  for (const field of [
    "rustToolchain",
    "rustcCommit",
    "gzipImplementation",
    "wasmPack",
    "wasmBindgen",
    "target",
    "cargoProfile",
    "wasmOpt",
    "wasmOptVersion",
    "measurementPlatform",
    "command",
  ]) {
    if (typeof measurement[field] !== "string" || measurement[field].length === 0) {
      fail(`measurement.${field} must be non-empty provenance`);
    }
  }
  if (!Number.isSafeInteger(policy?.maxRawBytes) || policy.maxRawBytes <= 0) {
    fail("policy.maxRawBytes must be a positive safe integer");
  }
  if (policy.maxRawBytes !== measurement.rawBytes) {
    fail("V1 ceiling must equal the exact accepted measurement (no arbitrary headroom)");
  }
  if (policy.derivation !== "exact-accepted-issue-284-measurement") {
    fail("unexpected raw-byte ceiling derivation");
  }
  if (policy.gzip !== "diagnostic-only") {
    fail("gzip must remain diagnostic-only across implementations");
  }
  return budget;
}

const { wasm: wasmPath, budget: budgetPath } = pathsFromArgs(process.argv.slice(2));
const budget = readBudget(budgetPath);
let wasm;
try {
  wasm = readFileSync(wasmPath);
} catch (error) {
  fail(`cannot read ${wasmPath}: ${error.message}`);
}
if (wasm.length < 8 || !wasm.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))) {
  fail(`${wasmPath} is not a WebAssembly binary`);
}

const rawBytes = wasm.length;
const maxRawBytes = budget.policy.maxRawBytes;
const gzipBytes = gzipSync(wasm, { level: 9 }).length;
const sha256 = createHash("sha256").update(wasm).digest("hex");
const baselineSha = sha256 === budget.measurement.sha256 ? "match" : "different";
const artifact = relative(REPO_ROOT, wasmPath).replaceAll("\\", "/");

if (rawBytes > maxRawBytes) {
  fail(
    `FAIL ${artifact} raw=${rawBytes}B exceeds ceiling=${maxRawBytes}B ` +
      `by=${rawBytes - maxRawBytes}B; gzip=${gzipBytes}B diagnostic-only; sha256=${sha256}`,
  );
}

console.log(
  `WASM size budget PASS raw=${rawBytes}B ceiling=${maxRawBytes}B ` +
    `remaining=${maxRawBytes - rawBytes}B gzip=${gzipBytes}B ` +
    `diagnostic-only baseline-sha=${baselineSha}`,
);
