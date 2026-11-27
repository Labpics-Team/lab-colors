#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { writeWasmLicenses } from "./wasm-build-licenses.mjs";
import { DEFAULT_BUDGET, parseBudgetDocument } from "./check-wasm-size-budget.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(dirname(SCRIPT_PATH), "..");
export const WASM_BUILD_RECEIPT = "wasm-build.json";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function run(command, args, cwd, env, capture = false) {
  const result = spawnSync(command, args, { cwd, env, encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
    stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit" });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed: ${result.signal ?? result.stderr ?? result.status}`);
  return result.stdout?.trim();
}

function verifyArchive(bytes, toolchain) {
  assert.equal(sha256(bytes), toolchain.binaryenNodeArchiveSha256, "Binaryen archive differs from packages/colors/bench/wasm.json");
}

async function binaryenArchive(toolchain) {
  const { binaryenRelease, binaryenNodeArchiveSha256 } = toolchain;
  // An explicitly supplied archive still uses the same byte pin as a downloaded one.
  if (process.env.LABCOLORS_BINARYEN_ARCHIVE) {
    const archive = resolve(process.env.LABCOLORS_BINARYEN_ARCHIVE);
    verifyArchive(await readFile(archive), toolchain);
    return archive;
  }
  const cache = join(process.env.XDG_CACHE_HOME ?? process.env.LOCALAPPDATA ?? join(homedir(), ".cache"),
    "labcolors", "binaryen");
  const archive = join(cache, `${binaryenNodeArchiveSha256}.tar.gz`);
  try {
    verifyArchive(await readFile(archive), toolchain);
    return archive;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const url = `https://github.com/WebAssembly/binaryen/releases/download/${binaryenRelease}/binaryen-${binaryenRelease}-node.tar.gz`;
  console.log(`Downloading official Binaryen ${binaryenRelease}`);
  const response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
  assert.ok(response.ok, `Binaryen download failed: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  verifyArchive(bytes, toolchain);
  await mkdir(cache, { recursive: true });
  const pending = `${archive}.${randomUUID()}.partial`;
  try {
    await writeFile(pending, bytes, { flag: "wx" });
    await rename(pending, archive);
  } finally {
    await rm(pending, { force: true });
  }
  return archive;
}

export function wasmCargoProfileArgs(budget) {
  return Object.entries(budget.recipe.cargoProfile).flatMap(([key, value]) => [
    "--config", `profile.${budget.toolchain.cargoProfile}.${key}=${JSON.stringify(value)}`,
  ]);
}

export function createWasmBuildEnvironment(budget, sourceRoot, environment = process.env) {
  // --config overrides the matching profile keys. Encoded flags supersede
  // all other rustflags sources; CARGO_INCREMENTAL overrides build.incremental.
  // Keep unrelated Cargo settings, including caches, wrappers and host linkers.
  // Windows treats environment names case-insensitively. Remove all spellings
  // before assigning the settings owned by this invocation.
  const controlled = /^(?:GITHUB_WORKSPACE|CARGO_HOME|RUSTUP_TOOLCHAIN|RUSTC|CARGO_INCREMENTAL|CARGO_ENCODED_RUSTFLAGS)$/iu;
  const cargoHome = Object.entries(environment).find(([name]) => /^CARGO_HOME$/iu.test(name))?.[1];
  const buildEnvironment = {
    ...Object.fromEntries(Object.entries(environment).filter(([name]) => !controlled.test(name))),
    GITHUB_WORKSPACE: resolve(sourceRoot),
    CARGO_HOME: resolve(cargoHome || join(homedir(), ".cargo")),
    RUSTUP_TOOLCHAIN: budget.toolchain.rust,
    RUSTC: "rustc",
    CARGO_INCREMENTAL: budget.recipe.cargoProfile.incremental ? "1" : "0",
  };
  buildEnvironment.CARGO_ENCODED_RUSTFLAGS = budget.recipe.rustPathRemap.map((mapping) => {
    const separator = mapping.indexOf("=");
    assert.ok(separator > 0 && separator < mapping.length - 1, "invalid Rust path mapping");
    const source = buildEnvironment[mapping.slice(0, separator)];
    assert.ok(source, `missing build path ${mapping.slice(0, separator)}`);
    return `--remap-path-prefix=${source}=${mapping.slice(separator + 1)}`;
  }).join("\x1f");
  return buildEnvironment;
}

export async function readWasmBuildInputs(sourceRoot = ROOT) {
  const sources = ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "crates/labcolors-wasm/Cargo.toml"];
  const recipe = ["packages/colors/bench/wasm.json", "scripts/build-wasm.mjs", "scripts/check-wasm-size-budget.mjs",
    "scripts/wasm-build-licenses.mjs", "scripts/distribution-licenses.mjs", "scripts/atomic-write.mjs"];
  return Object.fromEntries(await Promise.all([
    ...sources.map((path) => [sourceRoot, path]), ...recipe.map((path) => [ROOT, path]),
  ].map(async ([root, path]) => [path, sha256(await readFile(join(root, path)))])));
}

export function expectedWasmCargoProfile(budget) {
  const profile = budget.recipe.cargoProfile;
  return { opt_level: String(profile["opt-level"]), debuginfo: profile.debug ? 2 : 0,
    debug_assertions: profile["debug-assertions"], overflow_checks: profile["overflow-checks"], test: false };
}

function normalizedCargoProfile(profile) {
  // Cargo documents null debuginfo as rustc's default level 0.
  return { ...profile, debuginfo: profile?.debuginfo === null ? 0 : profile?.debuginfo };
}

export function createWasmBuildReceipt({ messages, runtime, budget, sourceSha, inputs, tools }) {
  const artifacts = [];
  const targetDirectory = `/${budget.toolchain.target}/${budget.toolchain.cargoProfile}/`;
  const expectedProfile = expectedWasmCargoProfile(budget);
  let finished = false;
  for (const line of messages.split(/\r?\n/u)) {
    if (!line.trimStart().startsWith("{")) continue;
    const message = JSON.parse(line);
    if (message.reason === "compiler-artifact" && message.filenames?.some((path) =>
      path.replaceAll("\\", "/").includes(targetDirectory))) {
      assert.deepEqual(normalizedCargoProfile(message.profile), expectedProfile,
        `actual Cargo profile differs from the WASM recipe for ${message.target?.name}`);
    }
    if (message.reason === "compiler-artifact" && message.target?.name === "labcolors_wasm" &&
        message.target.crate_types?.includes("cdylib") &&
        message.filenames?.some((path) => /(?:^|[/\\])labcolors_wasm\.wasm$/u.test(path))) {
      assert.equal(finished, false, "WASM artifact followed build-finished");
      artifacts.push(message);
    } else if (message.reason === "build-finished") {
      assert.equal(finished, false, "duplicate Cargo build-finished");
      assert.equal(message.success, true, "WASM Cargo build failed");
      finished = true;
    }
  }
  assert.ok(finished && artifacts.length === 1, "WASM build must report one completed product artifact");
  const profile = normalizedCargoProfile(artifacts[0].profile);
  assert.deepEqual(profile, expectedProfile, "actual Cargo profile differs from the WASM recipe");
  assert.match(sourceSha, /^[0-9a-f]{40}$/u, "invalid WASM build source SHA");
  assert.ok(WebAssembly.validate(runtime), "WASM build receipt requires a valid runtime");
  return { schemaVersion: 1, kind: "wasm-build", sourceSha,
    artifact: { bytes: runtime.length, sha256: sha256(runtime) },
    profile, recipe: structuredClone(budget.recipe), tools, inputs };
}

export async function buildWasm(sourceRoot = ROOT) {
  sourceRoot = resolve(sourceRoot);
  const budget = parseBudgetDocument(await readFile(DEFAULT_BUDGET), DEFAULT_BUDGET);
  const buildEnvironment = createWasmBuildEnvironment(budget, sourceRoot);
  const { binaryenRelease, wasmOptFlags, wasmPack } = budget.toolchain;
  const optimizerFlags = wasmOptFlags.trim().split(/\s+/u);
  assert.ok(optimizerFlags.length > 0 && optimizerFlags.every((flag) => flag.startsWith("-")));
  // Resolve the compiler once so Cargo and the version probe use the same
  // pinned executable even when build.rustc is set in a Cargo config file.
  buildEnvironment.RUSTC = run("rustup", ["which", "rustc"], sourceRoot, buildEnvironment, true);
  const rustc = run(buildEnvironment.RUSTC, ["-vV"], sourceRoot, buildEnvironment, true);
  const tools = {
    platform: `${process.platform}-${process.arch}`,
    rust: /^release: (.+)$/mu.exec(rustc)?.[1],
    rustcCommit: /^commit-hash: (.+)$/mu.exec(rustc)?.[1],
    wasmPack: run("wasm-pack", ["--version"], sourceRoot, buildEnvironment, true).replace(/^wasm-pack /u, ""),
    node: process.versions.node,
    binaryenRelease,
  };
  for (const key of ["rust", "rustcCommit", "wasmPack"]) {
    assert.equal(tools[key], budget.toolchain[key], `install the ${key} version declared in bench/wasm.json`);
  }
  assert.equal(tools.wasmPack, wasmPack);
  const sourceSha = run("git", ["rev-parse", "HEAD"], sourceRoot, buildEnvironment, true);
  const inputs = await readWasmBuildInputs(sourceRoot);
  const archive = await binaryenArchive(budget.toolchain);
  tools.binaryenNodeArchiveSha256 = sha256(await readFile(archive));
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-binaryen-"));
  const wasm = resolve(sourceRoot, budget.artifact);
  const optimized = `${wasm}.${randomUUID()}.optimized.wasm`;
  try {
    // Extract the byte-verified archive on every build, including CI.
    run("tar", ["-xzf", archive, "-C", temporary], sourceRoot, buildEnvironment);
    const optimizer = join(temporary, `binaryen-${binaryenRelease}`, "wasm-opt.js");
    await Promise.all([access(optimizer), access(join(dirname(optimizer), "wasm-opt.wasm"))]);
    const crate = join(sourceRoot, "crates/labcolors-wasm");
    const generatedPackage = resolve(sourceRoot, "packages/colors/pkg");
    assert.equal(dirname(wasm), generatedPackage, "unexpected WASM output directory");
    // wasm-pack retains old hash-named snippets unless its generated tree is cleared.
    await rm(generatedPackage, { recursive: true, force: true });
    const messages = run("wasm-pack", ["build", "--no-opt", "--release", "--target", "web",
      "--out-dir", relative(crate, dirname(wasm)), "--out-name", basename(wasm, "_bg.wasm"), crate,
      "--", "--locked", "--message-format=json-render-diagnostics", ...wasmCargoProfileArgs(budget)],
    crate, buildEnvironment, true);
    const before = (await readFile(wasm)).length;
    run(process.execPath, [optimizer, wasm, "-o", optimized, ...optimizerFlags], sourceRoot, buildEnvironment);
    const bytes = await readFile(optimized);
    assert.ok(WebAssembly.validate(bytes), "Binaryen produced an invalid WebAssembly module");
    await rename(optimized, wasm);
    // wasm-pack 0.13.1 runs cargo build in the crate directory. Match its
    // metadata/config lookup, including crate-local .cargo/config.toml.
    const metadata = JSON.parse(run("cargo", ["metadata", "--locked", "--format-version", "1"], crate, buildEnvironment, true));
    await writeWasmLicenses(messages, sourceRoot, metadata);
    assert.deepEqual(await readWasmBuildInputs(sourceRoot), inputs, "WASM build inputs changed during compilation");
    const receipt = createWasmBuildReceipt({ messages, runtime: bytes, budget, sourceSha, inputs, tools });
    await writeFile(join(generatedPackage, WASM_BUILD_RECEIPT), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
    console.log(`Binaryen ${binaryenRelease}: ${before} -> ${bytes.length} bytes (${relative(sourceRoot, wasm)})`);
  } finally {
    await rm(optimized, { force: true });
    await rm(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  assert.ok(process.argv.length <= 3, "usage: build-wasm.mjs [SOURCE_ROOT]");
  await buildWasm(process.argv[2]);
}
