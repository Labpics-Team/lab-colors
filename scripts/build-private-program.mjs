#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import {
  lstat,
  mkdir,
  mkdtemp,
  open,
  readFile,
  realpath,
  readdir,
  rm,
  stat,
  cp,
} from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import {
  delimiter,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from "node:path";
import { isDeepStrictEqual } from "node:util";
import { fileURLToPath } from "node:url";

import { atomicWriteGeneratedFile } from "./atomic-write.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
export const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..");
const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");
const RUNTIME_WASM_BUDGET = resolve(PACKAGE_DIR, "bench/wasm.json");

export const PRIVATE_PROGRAM_CONSUMER_PATH = "private-program/consumer.js";
export const PRIVATE_PROGRAM_WASM_PATH =
  "private-program/labcolors_private_program.wasm";
export const PRIVATE_PROGRAM_METADATA_PATH = "private-program/build-metadata.json";
export const PRIVATE_PROGRAM_BUILD_RECEIPT_PATH =
  "private-program/.build-receipt.json";
export const PRIVATE_PROGRAM_ROLE = "private-program-consumer";
export const PRIVATE_PROGRAM_SYMBOL_PREFIX = "labcolors_private_";

const PRIVATE_PROGRAM_CRATE = "labcolors-core";
const PRIVATE_PROGRAM_FEATURE = "private-fixture";
const PRIVATE_PROGRAM_TARGET = "wasm32-unknown-unknown";
const PRIVATE_PROGRAM_PROFILE = "release";
const PRIVATE_PROGRAM_BUILD_ROOT_PREFIX = "labcolors-private-program-build-";
const PRIVATE_PROGRAM_CANONICAL_BUILD_PASSES = 2;
const PRIVATE_PROGRAM_BUILD_TIMEOUT_MS = 10 * 60 * 1_000;
const PRIVATE_PROGRAM_TOOL_PROBE_TIMEOUT_MS = 30_000;
const CARGO_ARTIFACT_PATH = "wasm32-unknown-unknown/release/labcolors_core.wasm";
const WASM_MAGIC = Buffer.from([0x00, 0x61, 0x73, 0x6d]);
const SOURCE_DIGEST_FRAMING =
  "path-u32be-length+content-u64be-length+path-utf8+content-v1";
const OPTIMIZER_TRANSPORT = "binaryen-node";
const CANONICAL_RUST_EXECUTOR = deepFreeze({
  resolution: "declared-rustup-home-exact-toolchain-directory",
  identity: "self-reported-version-and-commit",
});

const sharedWasmToolchain = JSON.parse(readFileSync(RUNTIME_WASM_BUDGET, "utf8")).toolchain;
const CANONICAL_RUSTC_COMMIT =
  "ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96";
const CANONICAL_CARGO_COMMIT =
  "30a34c6821b57de0aaec83a901aca39f88f6778c";
const CANONICAL_WASM_OPT_VERSION = "wasm-opt version 117 (version_117)";
// These payload hashes are derived from, and checked together with, the
// byte-bound Binaryen Node archive declared by packages/colors/bench/wasm.json.
const CANONICAL_OPTIMIZER_FILES = Object.freeze({
  "wasm-opt.js": Object.freeze({
    bytes: 97_741,
    sha256: "c0b4bc26f1a588dc686ae36b32c4fea3d7b99f4fb8a1778d0ba4129f326f8449",
  }),
  "wasm-opt.wasm": Object.freeze({
    bytes: 6_351_784,
    sha256: "d823328d8fcad3a59aa605c61d1620d30b9156f086d30ff3246b43c32526856b",
  }),
  "wasm-opt.worker.js": Object.freeze({
    bytes: 2_761,
    sha256: "5b7952731f6ea1d5954db968e45b13f862853e6ef14a03b3f35d036f6136b624",
  }),
});

function fail(message) {
  throw new Error(`private Program build: ${message}`);
}

function deepFreeze(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) deepFreeze(child);
  }
  return value;
}

export const PRIVATE_PROGRAM_WASM_SURFACE = deepFreeze({
  memory: {
    imported: 0,
    defined: 1,
    shared: false,
    memory64: false,
  },
  imports: [
    {
      module: "labcolors_private_fixture_host_v1",
      name: "labcolors_private_fixture_host_confirm_disposed_v1",
      kind: "function",
    },
    {
      module: "labcolors_private_fixture_host_v1",
      name: "labcolors_private_fixture_host_install_v1",
      kind: "function",
    },
  ],
  exports: [
    { name: "__data_end", kind: "global" },
    { name: "__heap_base", kind: "global" },
    { name: "labcolors_private_fixture_abort_dispose_v1", kind: "function" },
    { name: "labcolors_private_fixture_begin_dispose_v1", kind: "function" },
    { name: "labcolors_private_fixture_commit_dispose_v1", kind: "function" },
    { name: "labcolors_private_fixture_request_v1_len", kind: "function" },
    { name: "labcolors_private_fixture_request_v1_ptr", kind: "function" },
    { name: "labcolors_private_fixture_result_v1_len", kind: "function" },
    { name: "labcolors_private_fixture_result_v1_ptr", kind: "function" },
    { name: "labcolors_private_fixture_run_v1", kind: "function" },
    { name: "memory", kind: "memory" },
  ],
});

function compareSurfaceEntries(left, right) {
  const leftKey = `${left.module ?? ""}\0${left.name}\0${left.kind}`;
  const rightKey = `${right.module ?? ""}\0${right.name}\0${right.kind}`;
  return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0;
}

function readWasmU32(bytes, offset, label) {
  let value = 0;
  for (let index = 0; index < 5; index++) {
    if (offset + index >= bytes.length) fail(`${label} is truncated`);
    const byte = bytes[offset + index];
    if (index === 4 && (byte & 0x7f) > 0x0f) {
      fail(`${label} overflows u32`);
    }
    value += (byte & 0x7f) * 2 ** (index * 7);
    if ((byte & 0x80) === 0) return { value, next: offset + index + 1 };
  }
  fail(`${label} is not a terminated u32`);
}

function parsePrivateProgramMemory(bytes, memoryImports) {
  if (memoryImports !== 0) {
    fail("private Program WASM must not import memory");
  }
  let offset = WASM_MAGIC.length + 4;
  let memorySectionCount = 0;
  let memory;
  while (offset < bytes.length) {
    const sectionId = bytes[offset++];
    const payloadLength = readWasmU32(bytes, offset, "WASM section length");
    offset = payloadLength.next;
    const payloadEnd = offset + payloadLength.value;
    if (payloadEnd > bytes.length) fail("WASM section exceeds the binary");
    if (sectionId === 5) {
      memorySectionCount += 1;
      if (memorySectionCount !== 1) fail("private Program WASM has duplicate memory sections");
      const count = readWasmU32(bytes, offset, "WASM memory count");
      offset = count.next;
      if (count.value !== 1) fail("private Program WASM must define exactly one memory");
      const limits = readWasmU32(bytes, offset, "WASM memory limits flags");
      offset = limits.next;
      if (limits.value & ~0x07) fail("private Program WASM has unknown memory limit flags");
      const minimum = readWasmU32(bytes, offset, "WASM memory minimum");
      offset = minimum.next;
      let maximum;
      if (limits.value & 0x01) {
        maximum = readWasmU32(bytes, offset, "WASM memory maximum");
        offset = maximum.next;
        if (maximum.value < minimum.value) {
          fail("private Program WASM memory maximum is below its minimum");
        }
      }
      const shared = Boolean(limits.value & 0x02);
      const memory64 = Boolean(limits.value & 0x04);
      if (shared) fail("private Program WASM memory must be non-shared");
      if (memory64) fail("private Program WASM memory must use 32-bit indexes");
      if (offset !== payloadEnd) fail("private Program WASM memory section has trailing bytes");
      memory = { imported: 0, defined: 1, shared: false, memory64: false };
    }
    offset = payloadEnd;
  }
  if (memorySectionCount !== 1 || !memory) {
    fail("private Program WASM must contain one defined memory section");
  }
  return memory;
}

export function validatePrivateProgramWasmSurface(bytes) {
  assertWasm(bytes, PRIVATE_PROGRAM_WASM_PATH);
  let module;
  try {
    module = new WebAssembly.Module(bytes);
  } catch (error) {
    fail(`cannot compile ${PRIVATE_PROGRAM_WASM_PATH}: ${error.message}`);
  }
  const actual = {
    imports: WebAssembly.Module.imports(module).sort(compareSurfaceEntries),
    exports: WebAssembly.Module.exports(module).sort(compareSurfaceEntries),
  };
  actual.memory = parsePrivateProgramMemory(
    bytes,
    actual.imports.filter(({ kind }) => kind === "memory").length,
  );
  const expected = {
    memory: PRIVATE_PROGRAM_WASM_SURFACE.memory,
    imports: [...PRIVATE_PROGRAM_WASM_SURFACE.imports].sort(compareSurfaceEntries),
    exports: [...PRIVATE_PROGRAM_WASM_SURFACE.exports].sort(compareSurfaceEntries),
  };
  if (!isDeepStrictEqual(actual, expected)) {
    fail(
      `WASM import/export surface differs from its exact private allowlist: ` +
        `expected=${JSON.stringify(expected)} actual=${JSON.stringify(actual)}`,
    );
  }
  return actual;
}

function canonicalSharedToolchain() {
  const required = [
    "rust",
    "rustcCommit",
    "target",
    "cargoProfile",
    "node",
    "binaryenRelease",
    "binaryenNodeArchiveSha256",
    "wasmOptFlags",
  ];
  for (const key of required) {
    if (typeof sharedWasmToolchain?.[key] !== "string" || !sharedWasmToolchain[key]) {
      fail(`runtime WASM toolchain is missing ${key}`);
    }
  }
  if (
    sharedWasmToolchain.target !== PRIVATE_PROGRAM_TARGET ||
    sharedWasmToolchain.cargoProfile !== PRIVATE_PROGRAM_PROFILE ||
    !CANONICAL_RUSTC_COMMIT.startsWith(sharedWasmToolchain.rustcCommit)
  ) {
    fail("private and runtime WASM toolchain pins have drifted");
  }
  return sharedWasmToolchain;
}

const canonicalShared = canonicalSharedToolchain();
const CANONICAL_OPTIMIZER = deepFreeze({
  transport: OPTIMIZER_TRANSPORT,
  node: canonicalShared.node,
  binaryenRelease: canonicalShared.binaryenRelease,
  binaryenNodeArchiveSha256: canonicalShared.binaryenNodeArchiveSha256,
  wasmOptVersion: CANONICAL_WASM_OPT_VERSION,
  files: CANONICAL_OPTIMIZER_FILES,
});
const CANONICAL_TOOLCHAIN = deepFreeze({
  rust: canonicalShared.rust,
  rustcCommit: CANONICAL_RUSTC_COMMIT,
  cargo: canonicalShared.rust,
  cargoCommit: CANONICAL_CARGO_COMMIT,
  executor: CANONICAL_RUST_EXECUTOR,
  optimizer: CANONICAL_OPTIMIZER,
});

const CARGO_ARGUMENTS = Object.freeze([
  "rustc",
  "-p",
  PRIVATE_PROGRAM_CRATE,
  "--lib",
  "--release",
  "--target",
  PRIVATE_PROGRAM_TARGET,
  "--target-dir",
  "$ISOLATED_CARGO_TARGET_DIR",
  "--no-default-features",
  "--features",
  PRIVATE_PROGRAM_FEATURE,
  "--crate-type=cdylib",
  "--locked",
  "--frozen",
  "--offline",
]);
const ENCODED_RUSTFLAGS = Object.freeze([
  "--cfg=labcolors_private_fixture_unshared_v1",
  "--check-cfg=cfg(labcolors_private_fixture_unshared_v1)",
  "--remap-path-prefix=$REPO_ROOT=/workspace/lab-colors",
  "--remap-path-prefix=$CARGO_HOME=/cargo-home",
  "--remap-path-prefix=$RUSTUP_HOME=/rustup-home",
  "--remap-path-prefix=$ISOLATED_CARGO_TARGET_DIR=/cargo-target",
  "--remap-path-prefix=$ISOLATED_TEMP_DIR=/build-temp",
]);
const WASM_OPT_FLAGS = Object.freeze(canonicalShared.wasmOptFlags.split(" "));

export function privateProgramBuildRecipe(optimizer) {
  const canonical = optimizer !== null;
  return {
    cargo: {
      command: canonical ? "$RUSTUP_TOOLCHAIN_CARGO" : "cargo",
      rustc: canonical ? "$RUSTUP_TOOLCHAIN_RUSTC" : null,
      args: [...CARGO_ARGUMENTS],
      encodedRustflags: [...ENCODED_RUSTFLAGS],
      environment: canonical
        ? {
            policy: "allowlist",
            cargoHome: "$ISOLATED_CARGO_HOME",
            registry: "isolated-index-copy-from-declared-cargo-home",
            temp: "$ISOLATED_TEMP_DIR",
            network: "offline",
          }
        : "ambient-contact",
      sourceArtifact: `$ISOLATED_CARGO_TARGET_DIR/${CARGO_ARTIFACT_PATH}`,
    },
    optimizer:
      optimizer === null
        ? null
        : {
            command: "$NODE_EXECUTABLE",
            script: "$BINARYEN_ROOT/wasm-opt.js",
            execution: "$ISOLATED_OPTIMIZER_ROOT/wasm-opt.js",
            args: ["$RAW_WASM", "-o", "$OPTIMIZED_WASM", ...WASM_OPT_FLAGS],
          },
    repeatability: canonical
      ? {
          passes: PRIVATE_PROGRAM_CANONICAL_BUILD_PASSES,
          isolation: ["cargo-target", "cargo-home", "temp"],
          comparison: ["raw-wasm-byte-identical", "optimized-wasm-byte-identical"],
          executor: "same-resolved-toolchain-and-node-process",
        }
      : null,
    outputArtifact: PRIVATE_PROGRAM_WASM_PATH,
  };
}

function privateProgramBuildDescriptor(toolchain) {
  return {
    crate: PRIVATE_PROGRAM_CRATE,
    feature: PRIVATE_PROGRAM_FEATURE,
    target: PRIVATE_PROGRAM_TARGET,
    profile: PRIVATE_PROGRAM_PROFILE,
    wasmSurface: PRIVATE_PROGRAM_WASM_SURFACE,
    toolchain,
    recipe: privateProgramBuildRecipe(toolchain.optimizer),
  };
}

export const PRIVATE_PROGRAM_CANONICAL_BUILD = deepFreeze(
  privateProgramBuildDescriptor(CANONICAL_TOOLCHAIN),
);

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function artifactMetadata(path, bytes) {
  if (bytes.length === 0) fail(`artifact is empty: ${path}`);
  return { path, bytes: bytes.length, sha256: sha256(bytes) };
}

function assertWasm(bytes, label) {
  if (bytes.length < 8 || !bytes.subarray(0, WASM_MAGIC.length).equals(WASM_MAGIC)) {
    fail(`${label} is not a WebAssembly binary`);
  }
}

async function sourceFiles(directory) {
  // A directory component is itself resolved and contained before its entries
  // are enumerated, so a swapped directory symlink cannot silently pull outside
  // content into the source scope. Leaf admission is re-done per file at read
  // time by admitSourceEntry, which is the authoritative containment boundary.
  const resolvedDirectory = await realpath(directory);
  if (!pathIsWithin(REPO_ROOT, resolvedDirectory)) {
    fail(`source directory does not resolve within the repository: ${directory}`);
  }
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isSymbolicLink()) {
      const target = await realpath(path);
      if (!pathIsWithin(REPO_ROOT, target)) {
        fail(`source symlink does not resolve to a repository file: ${path}`);
      }
      if (!(await stat(path)).isFile()) {
        fail(`source symlink does not resolve to a repository file: ${path}`);
      }
      files.push(path);
    } else if (entry.isDirectory()) files.push(...(await sourceFiles(path)));
    else if (entry.isFile()) files.push(path);
    else fail(`source scope contains a non-file entry: ${path}`);
  }
  return files;
}

/**
 * Admit one digest input by capturing the exact inode its path resolves to at
 * this moment, together with the canonical (symlink-free) target. Both are
 * re-verified against the opened handle by readAdmittedSource, so a same-UID
 * swap between this admission and the read either changes the opened inode
 * (rejected) or leaves the digested bytes identical to the admitted ones.
 */
export async function admitSourceEntry(path, { repoRoot = REPO_ROOT } = {}) {
  let target;
  try {
    target = await realpath(path);
  } catch (error) {
    fail(`cannot resolve source file: ${path}: ${error.message}`);
  }
  if (!pathIsWithin(repoRoot, target)) {
    fail(`source path does not resolve to a repository file: ${path}`);
  }
  let admitted;
  try {
    admitted = await stat(path);
  } catch (error) {
    fail(`cannot stat source file: ${path}: ${error.message}`);
  }
  if (!admitted.isFile()) {
    fail(`source scope contains a non-file entry: ${path}`);
  }
  return { path, target, admitted };
}

/**
 * Read an admitted snapshot through a handle opened on the resolved target and
 * fail closed unless the opened inode is exactly the admitted one. The bytes
 * are taken from the pinned handle, so a later path swap cannot retarget what
 * the digest hashes.
 */
export async function readAdmittedSource({ path, target, admitted }) {
  let handle;
  try {
    handle = await open(target, "r");
  } catch (error) {
    fail(`cannot open admitted source file: ${path}: ${error.message}`);
  }
  try {
    const opened = await handle.stat();
    if (
      !opened.isFile() ||
      opened.dev !== admitted.dev ||
      opened.ino !== admitted.ino
    ) {
      fail(
        `source changed between admission and read: ${path} ` +
          `(admitted dev=${admitted.dev} ino=${admitted.ino}, ` +
          `opened dev=${opened.dev} ino=${opened.ino})`,
      );
    }
    return await handle.readFile();
  } finally {
    await handle.close();
  }
}

export async function privateProgramCoreSourceDigest() {
  const files = [
    resolve(REPO_ROOT, "Cargo.toml"),
    resolve(REPO_ROOT, "Cargo.lock"),
    ...(await sourceFiles(resolve(REPO_ROOT, "crates/labcolors-core"))),
  ];
  for (const configPath of [".cargo/config", ".cargo/config.toml"]) {
    try {
      await stat(resolve(REPO_ROOT, configPath));
      files.push(resolve(REPO_ROOT, configPath));
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  const orderedFiles = files
    .map((path) => ({
      path,
      relativePath: relative(REPO_ROOT, path).split(sep).join("/"),
    }))
    .sort(({ relativePath: left }, { relativePath: right }) =>
      left < right ? -1 : left > right ? 1 : 0
    );

  const hash = createHash("sha256");
  for (const { path, relativePath } of orderedFiles) {
    const pathBytes = Buffer.from(relativePath, "utf8");
    // Every digested byte is read through the admitted snapshot handle; a path
    // swap between admission and read fails closed instead of silently binding
    // different bytes to the receipt.
    const content = await readAdmittedSource(await admitSourceEntry(path));
    const frame = Buffer.alloc(12);
    frame.writeUInt32BE(pathBytes.length, 0);
    frame.writeBigUInt64BE(BigInt(content.length), 4);
    hash.update(frame);
    hash.update(pathBytes);
    hash.update(content);
  }
  return {
    algorithm: "sha256",
    framing: SOURCE_DIGEST_FRAMING,
    symlinks: "resolve-file-content-within-repository",
    scope: [
      "Cargo.toml",
      "Cargo.lock",
      ".cargo/config?",
      ".cargo/config.toml?",
      "crates/labcolors-core/**",
    ],
    files: orderedFiles.length,
    sha256: hash.digest("hex"),
  };
}

function commandText(command, args, { env = process.env } = {}) {
  return execFileSync(command, args, {
    cwd: REPO_ROOT,
    env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: PRIVATE_PROGRAM_TOOL_PROBE_TIMEOUT_MS,
    windowsHide: true,
  }).trim();
}

function versionFromVerbose(output, label) {
  const release = output.match(/^release:\s*(\S+)$/mu)?.[1];
  const commit = output.match(/^commit-hash:\s*([0-9a-f]+)$/mu)?.[1];
  const host = output.match(/^host:\s*(\S+)$/mu)?.[1];
  if (!release || !commit || !host) fail(`cannot parse ${label} --version --verbose`);
  return { release, commit, host };
}

function environmentEntry(environment, name) {
  const matches = Object.entries(environment).filter(
    ([key]) => key.toUpperCase() === name,
  );
  if (matches.length > 1) {
    fail(`canonical environment contains duplicate case variants for ${name}`);
  }
  return matches[0];
}

function environmentValue(environment, name) {
  return environmentEntry(environment, name)?.[1];
}

const CANONICAL_AMBIENT_RUST_ENV_ALLOWLIST = new Set([
  "CARGO_HOME",
  "CARGO_TERM_COLOR",
  "RUSTUP_HOME",
  "RUST_TOOLCHAIN",
]);

export function validateCanonicalBuildEnvironment(environment = process.env) {
  const forbidden = [];
  for (const key of Object.keys(environment)) {
    const normalized = key.toUpperCase();
    if (CANONICAL_AMBIENT_RUST_ENV_ALLOWLIST.has(normalized)) continue;
    if (
      normalized === "CARGO" ||
      normalized.startsWith("CARGO_") ||
      normalized === "RUSTC" ||
      normalized.startsWith("RUST") ||
      normalized === "NODE_OPTIONS" ||
      normalized === "NODE_PATH"
    ) {
      forbidden.push(key);
    }
  }
  if (forbidden.length !== 0) {
    fail(
      `canonical environment contains forbidden executor or build overrides: ` +
        forbidden.sort().join(", "),
    );
  }
  for (const name of ["CARGO_HOME", "RUSTUP_HOME"]) {
    const entry = environmentEntry(environment, name);
    if (entry && !String(entry[1]).trim()) {
      fail(`canonical environment declares an empty ${name}`);
    }
  }
  const declaredToolchain = environmentValue(environment, "RUST_TOOLCHAIN")?.trim();
  if (declaredToolchain && declaredToolchain !== CANONICAL_TOOLCHAIN.rust) {
    fail(
      `RUST_TOOLCHAIN differs from the canonical pin: ` +
        `expected=${CANONICAL_TOOLCHAIN.rust} actual=${declaredToolchain}`,
    );
  }
  return true;
}

function inheritedOperatingSystemEnvironment(environment = process.env) {
  const inherited = {};
  for (const name of ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"]) {
    const value = environmentValue(environment, name);
    if (value !== undefined) inherited[name] = value;
  }
  return inherited;
}

function controlledProbeEnvironment({ rustupHome, path }) {
  return {
    ...inheritedOperatingSystemEnvironment(),
    PATH: path,
    RUSTUP_HOME: rustupHome,
    LANG: "C",
    LC_ALL: "C",
  };
}

function probeAmbientRustToolchain() {
  const rustc = versionFromVerbose(
    commandText(process.env.RUSTC?.trim() || "rustc", ["--version", "--verbose"]),
    "rustc",
  );
  const cargo = versionFromVerbose(
    commandText(process.env.CARGO?.trim() || "cargo", ["--version", "--verbose"]),
    "cargo",
  );
  return {
    rust: rustc.release,
    rustcCommit: rustc.commit,
    cargo: cargo.release,
    cargoCommit: cargo.commit,
    executor: {
      resolution: "ambient-contact",
      identity: "self-reported-version-and-commit",
    },
  };
}

function pathIsWithin(parent, child) {
  const childRelative = relative(parent, child);
  return (
    childRelative === "" ||
    (childRelative !== ".." &&
      !childRelative.startsWith(`..${sep}`) &&
      !isAbsolute(childRelative))
  );
}

async function executableWithin(root, path, label) {
  let canonical;
  try {
    canonical = await realpath(path);
  } catch (error) {
    fail(`cannot resolve ${label} at ${path}: ${error.message}`);
  }
  if (!pathIsWithin(root, canonical)) {
    fail(`${label} resolves outside the pinned rustup toolchain directory`);
  }
  const metadata = await stat(canonical);
  if (!metadata.isFile() || (process.platform !== "win32" && (metadata.mode & 0o111) === 0)) {
    fail(`${label} is not an executable regular file: ${canonical}`);
  }
  return canonical;
}

async function resolveCanonicalRustToolchain(environment = process.env) {
  const rustupHome = await realpath(
    resolve(environmentValue(environment, "RUSTUP_HOME")?.trim() || join(homedir(), ".rustup")),
  );
  const toolchainsDirectory = resolve(rustupHome, "toolchains");
  const candidates = (await readdir(toolchainsDirectory, { withFileTypes: true })).filter(
    (entry) =>
      entry.isDirectory() && entry.name.startsWith(`${CANONICAL_TOOLCHAIN.rust}-`),
  );
  if (candidates.length !== 1) {
    fail(
      `expected exactly one installed ${CANONICAL_TOOLCHAIN.rust} rustup toolchain; ` +
        `found=${JSON.stringify(candidates.map(({ name }) => name).sort())}`,
    );
  }
  const toolchainDirectory = await realpath(
    resolve(toolchainsDirectory, candidates[0].name),
  );
  if (!pathIsWithin(rustupHome, toolchainDirectory)) {
    fail("pinned rustup toolchain directory resolves outside RUSTUP_HOME");
  }
  const executableSuffix = process.platform === "win32" ? ".exe" : "";
  const cargo = await executableWithin(
    toolchainDirectory,
    resolve(toolchainDirectory, `bin/cargo${executableSuffix}`),
    "canonical cargo",
  );
  const rustc = await executableWithin(
    toolchainDirectory,
    resolve(toolchainDirectory, `bin/rustc${executableSuffix}`),
    "canonical rustc",
  );
  const probeEnvironment = controlledProbeEnvironment({
    rustupHome,
    path: [...new Set([dirname(cargo), dirname(rustc)])].join(delimiter),
  });
  const rustcVersion = versionFromVerbose(
    commandText(rustc, ["--version", "--verbose"], { env: probeEnvironment }),
    "rustc",
  );
  const cargoVersion = versionFromVerbose(
    commandText(cargo, ["--version", "--verbose"], { env: probeEnvironment }),
    "cargo",
  );
  if (
    rustcVersion.host !== cargoVersion.host ||
    candidates[0].name !== `${CANONICAL_TOOLCHAIN.rust}-${rustcVersion.host}`
  ) {
    fail("cargo, rustc, and the pinned rustup toolchain directory disagree on host");
  }
  return {
    cargo,
    rustc,
    rustupHome,
    receipt: {
      rust: rustcVersion.release,
      rustcCommit: rustcVersion.commit,
      cargo: cargoVersion.release,
      cargoCommit: cargoVersion.commit,
      executor: CANONICAL_RUST_EXECUTOR,
    },
  };
}

function assertCanonicalRustToolchain(actual) {
  const expected = { ...CANONICAL_TOOLCHAIN, optimizer: undefined };
  const comparable = { ...actual, optimizer: undefined };
  if (!isDeepStrictEqual(comparable, expected)) {
    fail(
      `canonical optimizer requires the pinned Rust toolchain: ` +
        `expected=${JSON.stringify(expected)} actual=${JSON.stringify(comparable)}`,
    );
  }
}

function optimizerConfigurationCount(environment = process.env) {
  return ["BINARYEN_ROOT", "BINARYEN_RELEASE", "BINARYEN_NODE_SHA256"].filter(
    (name) => environmentValue(environment, name)?.trim(),
  ).length;
}

function controlledOptimizerEnvironment(temporaryDirectory = tmpdir()) {
  return {
    ...inheritedOperatingSystemEnvironment(),
    PATH: dirname(process.execPath),
    LANG: "C",
    LC_ALL: "C",
    TMPDIR: temporaryDirectory,
    TEMP: temporaryDirectory,
    TMP: temporaryDirectory,
  };
}

async function configuredOptimizer(environment = process.env) {
  const root = environmentValue(environment, "BINARYEN_ROOT")?.trim() || "";
  const release = environmentValue(environment, "BINARYEN_RELEASE")?.trim() || "";
  const archiveSha256 =
    environmentValue(environment, "BINARYEN_NODE_SHA256")?.trim().toLowerCase() || "";
  const configured = [root, release, archiveSha256].filter(Boolean).length;
  if (configured === 0) return null;
  if (configured !== 3) {
    fail(
      "BINARYEN_ROOT, BINARYEN_RELEASE, and BINARYEN_NODE_SHA256 must be set together",
    );
  }
  if (
    release !== CANONICAL_OPTIMIZER.binaryenRelease ||
    archiveSha256 !== CANONICAL_OPTIMIZER.binaryenNodeArchiveSha256
  ) {
    fail("configured Binaryen release or archive SHA-256 differs from the canonical pin");
  }
  if (process.versions.node !== CANONICAL_OPTIMIZER.node) {
    fail(
      `Binaryen Node transport requires Node ${CANONICAL_OPTIMIZER.node}; ` +
        `actual=${process.versions.node}`,
    );
  }

  const files = [];
  for (const [name, expected] of Object.entries(CANONICAL_OPTIMIZER.files)) {
    const bytes = await readFile(resolve(root, name));
    const actual = artifactMetadata(name, bytes);
    if (actual.bytes !== expected.bytes || actual.sha256 !== expected.sha256) {
      fail(
        `${name} differs from the byte-bound Binaryen archive: ` +
          `expected=${JSON.stringify(expected)} actual=${JSON.stringify(actual)}`,
      );
    }
    files.push({ name, bytes });
  }
  return { files: Object.freeze(files), receipt: CANONICAL_OPTIMIZER };
}

function cargoArguments(targetDirectory) {
  return CARGO_ARGUMENTS.map((value) =>
    value === "$ISOLATED_CARGO_TARGET_DIR" ? targetDirectory : value
  );
}

function encodedRustflags({ targetDirectory, cargoHome, rustupHome, temporaryDirectory }) {
  const replacements = new Map([
    ["$REPO_ROOT", REPO_ROOT],
    ["$CARGO_HOME", cargoHome],
    ["$RUSTUP_HOME", rustupHome],
    ["$ISOLATED_CARGO_TARGET_DIR", targetDirectory],
    ["$ISOLATED_TEMP_DIR", temporaryDirectory],
  ]);
  return ENCODED_RUSTFLAGS.map((flag) => {
    for (const [placeholder, path] of replacements) {
      if (flag.includes(placeholder)) return flag.replace(placeholder, path);
    }
    return flag;
  }).join("\x1f");
}

function contactCargoEnvironment(targetDirectory, temporaryDirectory) {
  const cargoHome = resolve(process.env.CARGO_HOME?.trim() || join(homedir(), ".cargo"));
  const rustupHome = resolve(process.env.RUSTUP_HOME?.trim() || join(homedir(), ".rustup"));
  return {
    ...process.env,
    CARGO_ENCODED_RUSTFLAGS: encodedRustflags({
      targetDirectory,
      cargoHome,
      rustupHome,
      temporaryDirectory,
    }),
  };
}

function canonicalCargoEnvironment({
  cargo,
  rustc,
  rustupHome,
  targetDirectory,
  cargoHome,
  temporaryDirectory,
}) {
  return {
    ...inheritedOperatingSystemEnvironment(),
    PATH: [...new Set([dirname(cargo), dirname(rustc)])].join(delimiter),
    CARGO_HOME: cargoHome,
    RUSTUP_HOME: rustupHome,
    RUSTC: rustc,
    CARGO_ENCODED_RUSTFLAGS: encodedRustflags({
      targetDirectory,
      cargoHome,
      rustupHome,
      temporaryDirectory,
    }),
    CARGO_INCREMENTAL: "0",
    CARGO_NET_OFFLINE: "true",
    CARGO_TERM_COLOR: "never",
    LANG: "C",
    LC_ALL: "C",
    TMPDIR: temporaryDirectory,
    TEMP: temporaryDirectory,
    TMP: temporaryDirectory,
  };
}

export function canonicalCargoConfigurationPaths({
  repoRoot = REPO_ROOT,
  cargoHome = resolve(
    environmentValue(process.env, "CARGO_HOME")?.trim() || join(homedir(), ".cargo"),
  ),
} = {}) {
  const paths = [];
  let directory = resolve(repoRoot);
  for (;;) {
    paths.push(resolve(directory, ".cargo/config"));
    paths.push(resolve(directory, ".cargo/config.toml"));
    const parent = dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  paths.push(resolve(cargoHome, "config"));
  paths.push(resolve(cargoHome, "config.toml"));
  const seen = new Set();
  return paths.filter((path) => {
    const key = process.platform === "win32" ? path.toLowerCase() : path;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export async function assertCanonicalCargoConfigurationAbsent(options = {}) {
  const found = [];
  for (const path of canonicalCargoConfigurationPaths(options)) {
    try {
      await lstat(path);
      found.push(path);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  if (found.length !== 0) {
    fail(`canonical build rejects Cargo config files: ${found.join(", ")}`);
  }
  return true;
}

async function assertRegistryIndexLinksContained(root) {
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.pop();
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      fail(`cannot inspect declared Cargo registry index ${directory}: ${error.message}`);
    }
    for (const entry of entries) {
      const path = resolve(directory, entry.name);
      if (entry.isSymbolicLink()) {
        let target;
        try {
          target = await realpath(path);
        } catch (error) {
          fail(`registry index symlink is dangling: ${path}: ${error.message}`);
        }
        if (!pathIsWithin(root, target)) {
          fail(`registry index symlink resolves outside the declared Cargo index: ${path}`);
        }
      } else if (entry.isDirectory()) {
        pending.push(path);
      } else if (!entry.isFile()) {
        fail(`registry index contains a non-regular entry: ${path}`);
      }
    }
  }
}

async function createBuildSandbox({ canonical }) {
  const root = await mkdtemp(join(tmpdir(), PRIVATE_PROGRAM_BUILD_ROOT_PREFIX));
  const targetDirectory = resolve(root, "target");
  const cargoHome = resolve(root, "cargo-home");
  const temporaryDirectory = resolve(root, "temp");
  await mkdir(targetDirectory, { recursive: true });
  await mkdir(temporaryDirectory, { recursive: true, mode: 0o700 });
  if (canonical) {
    await mkdir(cargoHome, { recursive: true, mode: 0o700 });
    const declaredCargoHome = resolve(
      environmentValue(process.env, "CARGO_HOME")?.trim() || join(homedir(), ".cargo"),
    );
    const sourceIndex = resolve(declaredCargoHome, "registry", "index");
    const targetIndex = resolve(cargoHome, "registry", "index");
    try {
      await assertRegistryIndexLinksContained(sourceIndex);
      await cp(sourceIndex, targetIndex, { recursive: true });
    } catch (error) {
      fail(`canonical build requires the declared Cargo registry index: ${error.message}`);
    }
  }
  return { root, targetDirectory, cargoHome, temporaryDirectory };
}

async function materializeOptimizer({ optimizer, sandboxRoot, temporaryDirectory }) {
  const root = resolve(sandboxRoot, "binaryen");
  await mkdir(root, { recursive: true, mode: 0o700 });
  for (const { name, bytes } of optimizer.files) {
    const destination = resolve(root, name);
    await atomicWriteGeneratedFile(destination, bytes);
    const copied = await readFile(destination);
    if (!copied.equals(bytes)) {
      fail(`verified Binaryen snapshot changed while being materialized: ${name}`);
    }
  }
  const script = resolve(root, "wasm-opt.js");
  const version = commandText(process.execPath, [script, "--version"], {
    env: controlledOptimizerEnvironment(temporaryDirectory),
  });
  if (version !== CANONICAL_OPTIMIZER.wasmOptVersion) {
    fail(`unexpected wasm-opt version: ${version}`);
  }
  return { root, script };
}

async function runBuildPass({ rustExecution, optimizer }) {
  const canonical = optimizer !== null;
  const sandbox = await createBuildSandbox({ canonical });
  try {
    const cargo = canonical
      ? rustExecution.cargo
      : process.env.CARGO?.trim() || "cargo";
    const environment = canonical
      ? canonicalCargoEnvironment({
          cargo: rustExecution.cargo,
          rustc: rustExecution.rustc,
          rustupHome: rustExecution.rustupHome,
          targetDirectory: sandbox.targetDirectory,
          cargoHome: sandbox.cargoHome,
          temporaryDirectory: sandbox.temporaryDirectory,
        })
      : contactCargoEnvironment(sandbox.targetDirectory, sandbox.temporaryDirectory);
    execFileSync(cargo, cargoArguments(sandbox.targetDirectory), {
      cwd: REPO_ROOT,
      env: environment,
      stdio: "inherit",
      timeout: PRIVATE_PROGRAM_BUILD_TIMEOUT_MS,
      windowsHide: true,
    });
    const rawPath = resolve(
      sandbox.targetDirectory,
      ...CARGO_ARTIFACT_PATH.split("/"),
    );
    const raw = await readFile(rawPath);
    assertWasm(raw, CARGO_ARTIFACT_PATH);

    let wasm = raw;
    if (optimizer !== null) {
      const isolatedOptimizer = await materializeOptimizer({
        optimizer,
        sandboxRoot: sandbox.root,
        temporaryDirectory: sandbox.temporaryDirectory,
      });
      const optimizedPath = resolve(sandbox.root, "labcolors_core.optimized.wasm");
      execFileSync(
        process.execPath,
        [isolatedOptimizer.script, rawPath, "-o", optimizedPath, ...WASM_OPT_FLAGS],
        {
          cwd: isolatedOptimizer.root,
          env: controlledOptimizerEnvironment(sandbox.temporaryDirectory),
          stdio: "inherit",
          timeout: PRIVATE_PROGRAM_BUILD_TIMEOUT_MS,
          windowsHide: true,
        },
      );
      wasm = await readFile(optimizedPath);
      assertWasm(wasm, "optimized private Program artifact");
    }
    validatePrivateProgramWasmSurface(wasm);
    return { raw, wasm };
  } finally {
    await rm(sandbox.root, { recursive: true, force: true });
  }
}

async function assertSourceUnchanged(expected) {
  const actual = await privateProgramCoreSourceDigest();
  if (!isDeepStrictEqual(actual, expected)) {
    fail("Core source changed while the private Program artifact was being built");
  }
}

function assertRepeatedBuildsEqual(first, second) {
  for (const name of ["raw", "wasm"]) {
    if (!first[name].equals(second[name])) {
      fail(
        `repeated isolated ${name} artifacts differ: ` +
          `first=${sha256(first[name])} second=${sha256(second[name])}`,
      );
    }
  }
}

function receiptFor({ source, build, wasm }) {
  return {
    schemaVersion: 1,
    source,
    build,
    artifact: artifactMetadata(PRIVATE_PROGRAM_WASM_PATH, wasm),
  };
}

export function validatePrivateProgramBuildReceipt(
  receipt,
  { source, wasm, requireOptimizer = true },
) {
  assertWasm(wasm, PRIVATE_PROGRAM_WASM_PATH);
  const toolchain = requireOptimizer ? CANONICAL_TOOLCHAIN : receipt?.build?.toolchain;
  if (!toolchain || (requireOptimizer && toolchain.optimizer === null)) {
    fail("canonical release requires the optimized private Program artifact");
  }
  const expected = receiptFor({
    source,
    build: privateProgramBuildDescriptor(toolchain),
    wasm,
  });
  if (!isDeepStrictEqual(receipt, expected)) {
    fail(
      `build receipt differs from its exact source, toolchain, recipe, or artifact: ` +
        `expected=${JSON.stringify(expected)} actual=${JSON.stringify(receipt)}`,
    );
  }
  validatePrivateProgramWasmSurface(wasm);
  return receipt;
}

export async function readPrivateProgramBuildReceipt({ requireOptimizer = true } = {}) {
  const receiptPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_BUILD_RECEIPT_PATH);
  const [receiptBytes, wasm, source] = await Promise.all([
    readFile(receiptPath),
    readFile(resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH)),
    privateProgramCoreSourceDigest(),
  ]);
  let receipt;
  try {
    receipt = JSON.parse(receiptBytes.toString("utf8"));
  } catch (error) {
    fail(`cannot parse ${PRIVATE_PROGRAM_BUILD_RECEIPT_PATH}: ${error.message}`);
  }
  const canonical = Buffer.from(`${JSON.stringify(receipt, null, 2)}\n`, "utf8");
  if (!receiptBytes.equals(canonical)) {
    fail(`${PRIVATE_PROGRAM_BUILD_RECEIPT_PATH} is not canonical JSON`);
  }
  return validatePrivateProgramBuildReceipt(receipt, { source, wasm, requireOptimizer });
}

// Local contact builds may emit an explicit raw receipt; release verification
// opts into the optimizer requirement before Cargo can create an artifact.
export async function buildPrivateProgram({ requireOptimizer = false } = {}) {
  const optimizerConfiguration = optimizerConfigurationCount();
  const canonicalRequested = requireOptimizer || optimizerConfiguration !== 0;
  if (canonicalRequested) validateCanonicalBuildEnvironment();
  if (optimizerConfiguration !== 0 && optimizerConfiguration !== 3) {
    fail(
      "BINARYEN_ROOT, BINARYEN_RELEASE, and BINARYEN_NODE_SHA256 must be set together",
    );
  }
  if (requireOptimizer && optimizerConfiguration === 0) {
    fail("canonical release requires the configured Binaryen optimizer");
  }
  let rustExecution = null;
  let optimizer = null;
  let rustToolchain;
  if (canonicalRequested) {
    const cargoHome = resolve(
      environmentValue(process.env, "CARGO_HOME")?.trim() || join(homedir(), ".cargo"),
    );
    await assertCanonicalCargoConfigurationAbsent({ cargoHome });
    rustExecution = await resolveCanonicalRustToolchain();
    assertCanonicalRustToolchain(rustExecution.receipt);
    optimizer = await configuredOptimizer();
    rustToolchain = rustExecution.receipt;
  } else {
    rustToolchain = probeAmbientRustToolchain();
  }
  const toolchain = {
    ...rustToolchain,
    optimizer: optimizer?.receipt ?? null,
  };
  const build = privateProgramBuildDescriptor(toolchain);
  const output = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH);
  const receiptPath = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_BUILD_RECEIPT_PATH);
  const sourceBefore = await privateProgramCoreSourceDigest();
  const first = await runBuildPass({ rustExecution, optimizer });
  await assertSourceUnchanged(sourceBefore);
  let result = first;
  if (optimizer !== null) {
    const second = await runBuildPass({ rustExecution, optimizer });
    await assertSourceUnchanged(sourceBefore);
    assertRepeatedBuildsEqual(first, second);
    result = second;
  }
  const receipt = receiptFor({ source: sourceBefore, build, wasm: result.wasm });
  await mkdir(dirname(output), { recursive: true });
  await atomicWriteGeneratedFile(output, result.wasm);
  await atomicWriteGeneratedFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  process.stdout.write(
    `private Program WASM built: ${PRIVATE_PROGRAM_WASM_PATH} ` +
      `(${result.wasm.length} bytes, ` +
      `${optimizer === null ? "raw contact" : "optimized repeated build"})\n`,
  );
  return { output, receipt: receiptPath };
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH;

if (invokedDirectly) {
  buildPrivateProgram().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  });
}
