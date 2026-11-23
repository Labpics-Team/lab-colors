#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../", import.meta.url));
const budget = JSON.parse(await readFile(join(ROOT, "packages/colors/bench/wasm.json"), "utf8"));
const { binaryenRelease, binaryenNodeArchiveSha256, wasmOptFlags, wasmPack } = budget.toolchain;
assert.match(binaryenRelease, /^version_[0-9]+$/u);
assert.match(binaryenNodeArchiveSha256, /^[0-9a-f]{64}$/u);
const optimizerFlags = wasmOptFlags.trim().split(/\s+/u);
assert.ok(optimizerFlags.length > 0 && optimizerFlags.every((flag) => flag.startsWith("-")));

function run(command, args, capture = false, env = process.env) {
  const result = spawnSync(command, args, { cwd: ROOT, env, encoding: "utf8", stdio: capture ? "pipe" : "inherit" });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed: ${result.signal ?? result.stderr ?? result.status}`);
  return result.stdout?.trim();
}

function verifyArchive(bytes) {
  const actual = createHash("sha256").update(bytes).digest("hex");
  assert.equal(actual, binaryenNodeArchiveSha256, "Binaryen archive differs from packages/colors/bench/wasm.json");
}

async function binaryenArchive() {
  // An explicitly supplied archive still uses the same byte pin as a downloaded one.
  if (process.env.LABCOLORS_BINARYEN_ARCHIVE) {
    const archive = resolve(process.env.LABCOLORS_BINARYEN_ARCHIVE);
    verifyArchive(await readFile(archive));
    return archive;
  }
  const cache = join(process.env.XDG_CACHE_HOME ?? process.env.LOCALAPPDATA ?? join(homedir(), ".cache"),
    "labcolors", "binaryen");
  const archive = join(cache, `${binaryenNodeArchiveSha256}.tar.gz`);
  try {
    verifyArchive(await readFile(archive));
    return archive;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const url = `https://github.com/WebAssembly/binaryen/releases/download/${binaryenRelease}/binaryen-${binaryenRelease}-node.tar.gz`;
  console.log(`Downloading official Binaryen ${binaryenRelease}`);
  const response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
  assert.ok(response.ok, `Binaryen download failed: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  verifyArchive(bytes);
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

assert.equal(run("wasm-pack", ["--version"], true), `wasm-pack ${wasmPack}`, "install the wasm-pack version declared in bench/wasm.json");
const archive = await binaryenArchive();
const temporary = await mkdtemp(join(tmpdir(), "labcolors-binaryen-"));
const wasm = resolve(ROOT, budget.artifact);
const optimized = `${wasm}.${randomUUID()}.optimized.wasm`;
try {
  // Re-extract the verified archive; a modified cached executable is never reused.
  run("tar", ["-xzf", archive, "-C", temporary]);
  const binaryen = join(temporary, `binaryen-${binaryenRelease}`);
  const optimizer = join(binaryen, "wasm-opt.js");
  await Promise.all([access(optimizer), access(join(binaryen, "wasm-opt.wasm"))]);
  const crate = join(ROOT, "crates/labcolors-wasm");
  const buildEnvironment = {
    ...process.env,
    GITHUB_WORKSPACE: resolve(ROOT),
    CARGO_HOME: resolve(process.env.CARGO_HOME || join(homedir(), ".cargo")),
  };
  // Use the canonical build's path-independent Rust inputs on local checkouts too.
  buildEnvironment.CARGO_ENCODED_RUSTFLAGS = budget.recipe.rustPathRemap.map((mapping) => {
    const separator = mapping.indexOf("=");
    assert.ok(separator > 0 && separator < mapping.length - 1, "invalid Rust path mapping");
    const source = buildEnvironment[mapping.slice(0, separator)];
    assert.ok(source, `missing build path ${mapping.slice(0, separator)}`);
    return `--remap-path-prefix=${source}=${mapping.slice(separator + 1)}`;
  }).join("\x1f");
  run("wasm-pack", ["build", "--no-opt", "--release", "--target", "web",
    "--out-dir", relative(crate, dirname(wasm)), "--out-name", basename(wasm, "_bg.wasm"), crate, "--locked"], false, buildEnvironment);
  const before = (await readFile(wasm)).length;
  run(process.execPath, [optimizer, wasm, "-o", optimized, ...optimizerFlags]);
  const bytes = await readFile(optimized);
  assert.ok(WebAssembly.validate(bytes), "Binaryen produced an invalid WebAssembly module");
  await rename(optimized, wasm);
  console.log(`Binaryen ${binaryenRelease}: ${before} -> ${bytes.length} bytes (${relative(ROOT, wasm)})`);
} finally {
  await rm(optimized, { force: true });
  await rm(temporary, { recursive: true, force: true });
}
