#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { parseBudgetDocument } from "./check-wasm-size-budget.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(dirname(SCRIPT_PATH), "..");
const WORKFLOW = ".github/workflows/package-worker.yml";
const BUDGET = "packages/colors/bench/wasm.json";
const RUNTIME = "packages/colors/pkg/labcolors_bg.wasm";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function createWasmMeasurement({ first, runtime, sourceSha, env, budget, workflow, tools, inputs }) {
  assert.match(sourceSha, /^[0-9a-f]{40}$/u, "invalid checked-out source SHA");
  assert.equal(sourceSha, env.GITHUB_SHA, "measurement source differs from the workflow event");
  assert.equal(tools.platform, "linux-x64", "measurement requires the canonical CI platform");
  for (const key of ["GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"]) {
    assert.match(env[key] ?? "", /^[1-9][0-9]*$/u, `missing ${key}`);
  }
  for (const key of ["GITHUB_REPOSITORY", "GITHUB_JOB", "GITHUB_EVENT_NAME", "GITHUB_REF", "GITHUB_WORKFLOW_REF", "GITHUB_WORKFLOW_SHA", "GITHUB_SERVER_URL"]) {
    assert.match(env[key] ?? "", /^[^\r\n]+$/u, `missing ${key}`);
  }
  assert.ok(runtime.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])), "runtime is not a WebAssembly binary");
  assert.ok(first.equals(runtime), "runtime differs between the two canonical builds");

  const functions = [...workflow.matchAll(/^          build_runtime\(\) \{\n([\s\S]*?)^          \}/gmu)];
  assert.equal(functions.length, 1, "canonical build recipe must have one owner");
  const commands = functions[0][1].replace(/[ \t]*\\\n[ \t]*/gu, " ").trim().split("\n").map((line) => line.trim());
  assert.deepEqual(commands, budget.recipe.commands, "measured build recipe differs from the declared recipe");
  const flags = budget.recipe.rustPathRemap.map((mapping) => {
    const [name, destination] = mapping.split("=");
    assert.ok(env[name], `missing path-remap source ${name}`);
    return `--remap-path-prefix=${env[name]}=${destination}`;
  }).join("\x1f");
  assert.equal(env.CARGO_ENCODED_RUSTFLAGS, flags, "measured Rust flags differ from the declared recipe");
  for (const key of ["rust", "rustcCommit", "wasmPack", "wasmBindgen", "node", "binaryenRelease", "binaryenNodeArchiveSha256"]) {
    assert.equal(tools[key], budget.toolchain[key], `measured ${key} differs from the declared toolchain`);
  }

  return {
    schemaVersion: 1,
    kind: "canonical-wasm-measurement",
    sourceSha,
    run: {
      repository: env.GITHUB_REPOSITORY,
      id: env.GITHUB_RUN_ID,
      attempt: env.GITHUB_RUN_ATTEMPT,
      job: env.GITHUB_JOB,
      event: env.GITHUB_EVENT_NAME,
      ref: env.GITHUB_REF,
      workflowRef: env.GITHUB_WORKFLOW_REF,
      workflowSha: env.GITHUB_WORKFLOW_SHA,
      url: `${env.GITHUB_SERVER_URL}/${env.GITHUB_REPOSITORY}/actions/runs/${env.GITHUB_RUN_ID}/attempts/${env.GITHUB_RUN_ATTEMPT}`,
    },
    platform: tools.platform,
    artifact: { path: "labcolors_bg.wasm", bytes: runtime.length, sha256: sha256(runtime) },
    gzip: { bytes: gzipSync(runtime, { level: 9 }).length, role: "diagnostic-only" },
    reproducibility: { builds: 2, byteIdentical: true, scope: "runtime-wasm" },
    toolchain: { ...budget.toolchain },
    recipe: { ...budget.recipe },
    inputs,
    budget: { source: budget.measurement.source, maxRawBytes: budget.policy.maxRawBytes, passed: runtime.length <= budget.policy.maxRawBytes },
    // This receipt admits a measurement; the later size, package and consumer gates still decide release readiness.
    qualification: "measurement-only",
  };
}

export function recordWasmMeasurement(firstDirectory, outputDirectory) {
  const env = process.env;
  const command = (name, args) => execFileSync(name, args, { cwd: ROOT, encoding: "utf8" }).trim();
  assert.equal(command("git", ["status", "--porcelain=v1", "--untracked-files=normal"]), "", "measurement requires clean source");
  const sourceSha = command("git", ["rev-parse", "HEAD"]);
  const budget = parseBudgetDocument(readFileSync(join(ROOT, BUDGET)), join(ROOT, BUDGET));
  const rustc = command("rustc", ["-vV"]);
  const binaryenArchive = join(env.RUNNER_TEMP, `binaryen-${env.GITHUB_JOB}.tar.gz`);
  const tools = {
    platform: `${process.platform}-${process.arch}`,
    rust: /^release: (.+)$/mu.exec(rustc)?.[1],
    rustcCommit: /^commit-hash: (.+)$/mu.exec(rustc)?.[1],
    wasmPack: command("wasm-pack", ["--version"]).replace(/^wasm-pack /u, ""),
    wasmBindgen: command("wasm-bindgen", ["--version"]).replace(/^wasm-bindgen /u, ""),
    node: process.versions.node,
    binaryenRelease: env.BINARYEN_RELEASE,
    binaryenNodeArchiveSha256: sha256(readFileSync(binaryenArchive)),
  };
  const inputs = Object.fromEntries([
    WORKFLOW, BUDGET, "Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "crates/labcolors-wasm/Cargo.toml",
  ].map((path) => [path, sha256(readFileSync(join(ROOT, path)))]));
  const runtime = readFileSync(join(ROOT, RUNTIME));
  const receipt = createWasmMeasurement({
    first: readFileSync(join(firstDirectory, "labcolors_bg.wasm")), runtime, sourceSha, env, budget,
    workflow: readFileSync(join(ROOT, WORKFLOW), "utf8"), tools, inputs,
  });
  mkdirSync(outputDirectory);
  writeFileSync(join(outputDirectory, "labcolors_bg.wasm"), runtime, { flag: "wx" });
  writeFileSync(join(outputDirectory, "measurement.json"), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
  appendFileSync(env.GITHUB_OUTPUT, "measurement-ready=true\n");
  console.log(`canonical WASM measurement: ${runtime.length}B sha256=${receipt.artifact.sha256} source=${sourceSha}`);
  return receipt;
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  assert.equal(process.argv.length, 4, "usage: record-wasm-measurement.mjs FIRST_BUILD_DIRECTORY OUTPUT_DIRECTORY");
  recordWasmMeasurement(resolve(process.argv[2]), resolve(process.argv[3]));
}
