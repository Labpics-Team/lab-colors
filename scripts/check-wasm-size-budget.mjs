#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  closeSync,
  constants as fsConstants,
  fstatSync,
  openSync,
  readFileSync,
  readdirSync,
} from "node:fs";
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
  "64cbdc5d53e36dca4f57197ca514f080a4605a6d69d43b9260e7ed2329e26506";

const SCHEMA_VERSION = 2;
const CANONICAL_ARTIFACT = "packages/colors/pkg/labcolors_bg.wasm";
const CANONICAL_PLATFORM = "linux-x64";
const TOOLCHAIN_KEYS = [
  "rust",
  "rustcCommit",
  "wasmPack",
  "target",
  "cargoProfile",
  "wasmBindgen",
  "wasmOpt",
];
const MANAGED_BUILDER_KEYS = [
  "version",
  "target",
  "origin",
  "archiveSha256",
  "binarySha256",
];
const WASM_OPT_KEYS = [...MANAGED_BUILDER_KEYS, "arguments"];
const NUMBERED_BUDGET = /^wasm-size-budget-v\d+\.json$/u;
const PATH_REMAP = /^[A-Z][A-Z0-9_]*=\/[^\r\n]+$/u;
const MEASUREMENT_SOURCE = /^github-actions-run-[1-9][0-9]*$/u;
const SHA256 = /^[0-9a-f]{64}$/u;

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

function lowercaseSha256(value, label) {
  if (typeof value !== "string" || !SHA256.test(value)) {
    fail(`${label} must be one lowercase SHA-256 digest`);
  }
}

function validateManagedBuilder(builder, expectedKeys, label, expectedOrigin) {
  exactKeys(builder, expectedKeys, label);
  nonEmptyLine(builder.version, `${label}.version`);
  nonEmptyLine(builder.target, `${label}.target`);
  nonEmptyLine(builder.origin, `${label}.origin`);
  if (builder.origin !== expectedOrigin(builder)) {
    fail(`${label}.origin does not match its version and target`);
  }
  lowercaseSha256(builder.archiveSha256, `${label}.archiveSha256`);
  lowercaseSha256(builder.binarySha256, `${label}.binarySha256`);
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
  for (const key of ["rust", "rustcCommit", "wasmPack", "target", "cargoProfile"]) {
    nonEmptyLine(budget.toolchain[key], `toolchain.${key}`);
  }
  validateManagedBuilder(
    budget.toolchain.wasmBindgen,
    MANAGED_BUILDER_KEYS,
    "toolchain.wasmBindgen",
    ({ version, target }) =>
      `https://github.com/rustwasm/wasm-bindgen/releases/download/${version}/` +
      `wasm-bindgen-${version}-${target}.tar.gz`,
  );
  validateManagedBuilder(
    budget.toolchain.wasmOpt,
    WASM_OPT_KEYS,
    "toolchain.wasmOpt",
    ({ version, target }) =>
      `https://github.com/WebAssembly/binaryen/releases/download/version_${version}/` +
      `binaryen-version_${version}-${target}.tar.gz`,
  );
  if (
    !Array.isArray(budget.toolchain.wasmOpt.arguments) ||
    budget.toolchain.wasmOpt.arguments.length === 0 ||
    budget.toolchain.wasmOpt.arguments.some(
      (argument) =>
        typeof argument !== "string" ||
        argument.length === 0 ||
        argument.includes("\r") ||
        argument.includes("\n"),
    ) ||
    new Set(budget.toolchain.wasmOpt.arguments).size !==
      budget.toolchain.wasmOpt.arguments.length
  ) {
    fail("toolchain.wasmOpt.arguments must contain unique non-empty arguments");
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
    ["baselineSource", "platform", "rawBytes"],
    "measurement",
  );
  if (
    typeof budget.measurement.baselineSource !== "string" ||
    !MEASUREMENT_SOURCE.test(budget.measurement.baselineSource)
  ) {
    fail("measurement.baselineSource must identify one GitHub Actions run");
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

function exactOwnKeySet(value, expected, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  const actual = Reflect.ownKeys(value);
  if (
    actual.some((key) => typeof key !== "string") ||
    actual.length !== expected.length ||
    expected.some((key) => !Object.hasOwn(value, key))
  ) {
    fail(`${label} must contain exactly ${expected.join(" and ")}`);
  }
}

export function evaluateWasmBudget(budget, wasm, context) {
  if (context === null || typeof context !== "object" || Array.isArray(context)) {
    fail("evaluation context must be an object");
  }
  nonEmptyLine(context.platform, "evaluation context platform");
  if (context.role !== "diagnostic" && context.role !== "release-equivalent") {
    fail("evaluation context role must be diagnostic or release-equivalent");
  }
  if (context.role === "diagnostic") {
    exactOwnKeySet(context, ["platform", "role"], "diagnostic evaluation context");
  } else {
    exactOwnKeySet(
      context,
      ["platform", "role", "builders"],
      "release-equivalent evaluation context",
    );
    exactOwnKeySet(
      context.builders,
      ["wasmBindgenSha256", "wasmOptSha256"],
      "release-equivalent builders",
    );
    lowercaseSha256(
      context.builders.wasmBindgenSha256,
      "release-equivalent builders.wasmBindgenSha256",
    );
    lowercaseSha256(
      context.builders.wasmOptSha256,
      "release-equivalent builders.wasmOptSha256",
    );
    if (
      context.builders.wasmBindgenSha256 !==
      budget.toolchain.wasmBindgen.binarySha256
    ) {
      fail("release-equivalent wasm-bindgen CLI digest mismatch");
    }
    if (context.builders.wasmOptSha256 !== budget.toolchain.wasmOpt.binarySha256) {
      fail("release-equivalent wasm-opt digest mismatch");
    }
    if (context.platform !== budget.measurement.platform) {
      fail(
        `release-equivalent role requires ${budget.measurement.platform}, ` +
          `got ${context.platform}`,
      );
    }
  }

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
  const enforceMeasurement = context.role === "release-equivalent";
  if (enforceMeasurement && rawBytes !== budget.measurement.rawBytes) {
    fail(
      `runtime exact artifact length mismatch on ${context.platform}: ` +
        `expected=${budget.measurement.rawBytes}B actual=${rawBytes}B; ` +
        `gzip=${gzipBytes}B diagnostic-only sha256=${artifactSha256}`,
    );
  }
  return {
    status: enforceMeasurement ? "PASS" : "DIAGNOSTIC",
    rawBytes,
    maxRawBytes: budget.policy.maxRawBytes,
    deltaBytes: rawBytes - budget.policy.maxRawBytes,
    gzipBytes,
    currentPlatform: context.platform,
    evaluationRole: context.role,
    builderSha256: context.role === "release-equivalent" ? context.builders : undefined,
    artifactSha256,
  };
}

function formatResult(result, artifact) {
  const delta = `${result.deltaBytes >= 0 ? "+" : ""}${result.deltaBytes}`;
  const builders = result.builderSha256 === undefined
    ? ""
    : ` wasm-bindgen-sha256=${result.builderSha256.wasmBindgenSha256}` +
      ` wasm-opt-sha256=${result.builderSha256.wasmOptSha256}`;
  return (
    `WASM size budget ${result.status} role=runtime raw=${result.rawBytes}B ` +
    `ceiling=${result.maxRawBytes}B delta=${delta}B ` +
    `gzip-diagnostic=${result.gzipBytes}B platform=${result.currentPlatform} ` +
    `evaluation=${result.evaluationRole} artifact=${artifact} ` +
    `artifact-sha256=${result.artifactSha256}${builders}`
  );
}

function pathsFromArgs(args) {
  const paths = {
    budget: DEFAULT_BUDGET,
    runtime: undefined,
    wasmBindgenArchive: undefined,
    wasmOptArchive: undefined,
    wasmBindgenCli: undefined,
    wasmOpt: undefined,
    operation: "diagnostic",
  };
  const pathFlags = new Map([
    ["--budget", "budget"],
    ["--runtime-wasm", "runtime"],
    ["--wasm-bindgen-archive", "wasmBindgenArchive"],
    ["--wasm-opt-archive", "wasmOptArchive"],
    ["--wasm-bindgen-cli", "wasmBindgenCli"],
    ["--wasm-opt", "wasmOpt"],
  ]);
  const operationFlags = new Map([
    ["--builder-plan", "builder-plan"],
    ["--admit-builders", "admit-builders"],
    ["--release-equivalent", "release-equivalent"],
  ]);
  const seenPathFlags = new Set();
  for (let index = 0; index < args.length;) {
    const flag = args[index];
    const operation = operationFlags.get(flag);
    if (operation !== undefined) {
      if (paths.operation === operation) {
        fail(`${flag} may appear only once`);
      }
      if (paths.operation !== "diagnostic") {
        fail(`${flag} conflicts with ${paths.operation}`);
      }
      paths.operation = operation;
      index += 1;
      continue;
    }
    const key = pathFlags.get(flag);
    if (key === undefined) fail(`unknown argument ${flag}`);
    if (seenPathFlags.has(flag)) fail(`${flag} may appear only once`);
    const value = args[index + 1];
    if (value === undefined) fail(`${flag ?? "argument"} requires a path`);
    paths[key] = resolve(value);
    seenPathFlags.add(flag);
    index += 2;
  }
  const builderPathCount = [
    paths.wasmBindgenArchive,
    paths.wasmOptArchive,
    paths.wasmBindgenCli,
    paths.wasmOpt,
  ]
    .filter((path) => path !== undefined).length;
  if (paths.operation === "builder-plan" && builderPathCount !== 0) {
    fail("--builder-plan does not accept builder paths");
  }
  if (paths.operation === "admit-builders" && builderPathCount !== 4) {
    fail(
      "--admit-builders requires both archives and both executable paths",
    );
  }
  if (
    paths.operation === "release-equivalent" &&
    (builderPathCount !== 2 ||
      paths.wasmBindgenArchive !== undefined ||
      paths.wasmOptArchive !== undefined)
  ) {
    fail("--release-equivalent requires exactly --wasm-bindgen-cli and --wasm-opt");
  }
  if (paths.operation === "diagnostic" && builderPathCount !== 0) {
    fail("builder paths require --admit-builders or --release-equivalent");
  }
  if (
    paths.operation !== "diagnostic" &&
    paths.operation !== "release-equivalent" &&
    paths.runtime !== undefined
  ) {
    fail(`${paths.operation} does not accept --runtime-wasm`);
  }
  return paths;
}

function currentIdentityCanExecute(stat) {
  if (
    typeof process.geteuid !== "function" ||
    typeof process.getegid !== "function" ||
    typeof process.getgroups !== "function"
  ) {
    return (stat.mode & 0o111) !== 0;
  }
  const effectiveUser = process.geteuid();
  if (effectiveUser === 0) return (stat.mode & 0o111) !== 0;
  if (stat.uid === effectiveUser) return (stat.mode & 0o100) !== 0;
  const groups = new Set([process.getegid(), ...process.getgroups()]);
  if (groups.has(stat.gid)) return (stat.mode & 0o010) !== 0;
  return (stat.mode & 0o001) !== 0;
}

function exactFileSha256(path, label, executable) {
  const kind = executable ? "executable regular" : "regular";
  let descriptor;
  try {
    descriptor = openSync(
      path,
      fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW | fsConstants.O_NONBLOCK,
    );
  } catch (error) {
    fail(
      `${label} must be one ${kind} non-symlink file: ${path} (${error.message})`,
    );
  }
  try {
    const stat = fstatSync(descriptor);
    if (!stat.isFile() || (executable && !currentIdentityCanExecute(stat))) {
      fail(`${label} must be one ${kind} non-symlink file: ${path}`);
    }
    return sha256(readFileSync(descriptor));
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("WASM size budget:")) {
      throw error;
    }
    fail(`cannot read ${label} ${path}: ${error.message}`);
  } finally {
    closeSync(descriptor);
  }
}

function exactBuilderSha256(path, label) {
  return exactFileSha256(path, label, true);
}

function exactArchiveSha256(path, label) {
  return exactFileSha256(path, label, false);
}

function builderDigests(paths) {
  return {
    wasmBindgenSha256: exactBuilderSha256(
      paths.wasmBindgenCli,
      "wasm-bindgen CLI",
    ),
    wasmOptSha256: exactBuilderSha256(paths.wasmOpt, "wasm-opt"),
  };
}

function admitBuilderFiles(budget, paths) {
  const archives = {
    wasmBindgenSha256: exactArchiveSha256(
      paths.wasmBindgenArchive,
      "wasm-bindgen archive",
    ),
    wasmOptSha256: exactArchiveSha256(paths.wasmOptArchive, "wasm-opt archive"),
  };
  if (archives.wasmBindgenSha256 !== budget.toolchain.wasmBindgen.archiveSha256) {
    fail("wasm-bindgen archive digest mismatch");
  }
  if (archives.wasmOptSha256 !== budget.toolchain.wasmOpt.archiveSha256) {
    fail("wasm-opt archive digest mismatch");
  }
  const builders = builderDigests(paths);
  if (builders.wasmBindgenSha256 !== budget.toolchain.wasmBindgen.binarySha256) {
    fail("wasm-bindgen CLI digest mismatch");
  }
  if (builders.wasmOptSha256 !== budget.toolchain.wasmOpt.binarySha256) {
    fail("wasm-opt digest mismatch");
  }
  return { archives, builders };
}

function main(args) {
  rejectNumberedBudgetSiblings();
  const paths = pathsFromArgs(args);
  const budget = readBudget(paths.budget);
  if (paths.operation === "builder-plan") {
    console.log(
      [
        budget.toolchain.wasmBindgen.origin,
        budget.toolchain.wasmBindgen.archiveSha256,
        budget.toolchain.wasmOpt.origin,
        budget.toolchain.wasmOpt.archiveSha256,
      ].join("\n"),
    );
    return;
  }
  if (paths.operation === "admit-builders") {
    const admitted = admitBuilderFiles(budget, paths);
    console.log(
      `WASM builders PASS ` +
        `wasm-bindgen-archive-sha256=${admitted.archives.wasmBindgenSha256} ` +
        `wasm-bindgen-sha256=${admitted.builders.wasmBindgenSha256} ` +
        `wasm-opt-archive-sha256=${admitted.archives.wasmOptSha256} ` +
        `wasm-opt-sha256=${admitted.builders.wasmOptSha256}`,
    );
    return;
  }
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
    paths.operation === "diagnostic"
      ? {
          platform: `${process.platform}-${process.arch}`,
          role: paths.operation,
        }
      : {
          platform: `${process.platform}-${process.arch}`,
          role: paths.operation,
          builders: builderDigests(paths),
        },
  );
  const artifact = relative(REPO_ROOT, runtimePath).replaceAll("\\", "/");
  console.log(formatResult(result, artifact));
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  main(process.argv.slice(2));
}
