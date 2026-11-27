#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { parseBudgetDocument } from "./check-wasm-size-budget.mjs";
import { createWasmBuildEnvironment, expectedWasmCargoProfile, readWasmBuildInputs, WASM_BUILD_RECEIPT } from "./build-wasm.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(dirname(SCRIPT_PATH), "..");
const WORKFLOW = ".github/workflows/package-worker.yml";
const BUDGET = "packages/colors/bench/wasm.json";
const RUNTIME = "packages/colors/pkg/labcolors_bg.wasm";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function createWasmMeasurement({ first, runtime, firstBuild, build, sourceSha, env, budget, workflow, tools, inputs }) {
  assert.match(sourceSha, /^[0-9a-f]{40}$/u, "invalid checked-out source SHA");
  assert.equal(sourceSha, env.GITHUB_SHA, "measurement source differs from the workflow event");
  assert.equal(tools.platform, "linux-x64", "measurement requires the canonical CI platform");
  for (const key of ["GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"]) {
    assert.match(env[key] ?? "", /^[1-9][0-9]*$/u, `missing ${key}`);
  }
  for (const key of ["GITHUB_REPOSITORY", "GITHUB_JOB", "GITHUB_EVENT_NAME", "GITHUB_REF", "GITHUB_WORKFLOW_REF", "GITHUB_WORKFLOW_SHA", "GITHUB_SERVER_URL"]) {
    assert.match(env[key] ?? "", /^[^\r\n]+$/u, `missing ${key}`);
  }
  assert.ok(WebAssembly.validate(runtime), "runtime is not a WebAssembly binary");
  assert.ok(first.equals(runtime), "runtime differs between the two canonical builds");

  const functions = [...workflow.replaceAll("\r\n", "\n").matchAll(/^          build_runtime\(\) \{\n([\s\S]*?)^          \}/gmu)];
  assert.equal(functions.length, 1, "canonical build recipe must have one owner");
  const commands = functions[0][1].replace(/[ \t]*\\\n[ \t]*/gu, " ").trim().split("\n").map((line) => line.trim());
  assert.deepEqual(commands, budget.recipe.commands, "measured build recipe differs from the declared recipe");
  assert.ok(env.GITHUB_WORKSPACE && env.CARGO_HOME, "missing canonical build paths");
  for (const key of ["rust", "rustcCommit", "wasmPack", "wasmBindgen", "node", "binaryenRelease", "binaryenNodeArchiveSha256"]) {
    assert.equal(tools[key], budget.toolchain[key], `measured ${key} differs from the declared toolchain`);
  }
  assert.equal(build?.schemaVersion, 1, "missing supported WASM build receipt");
  assert.equal(build.kind, "wasm-build", "invalid WASM build receipt kind");
  assert.deepEqual(firstBuild, build, "build receipts differ between the two canonical builds");
  assert.equal(build.sourceSha, sourceSha, "WASM build receipt belongs to different source");
  assert.deepEqual(build.artifact, { bytes: runtime.length, sha256: sha256(runtime) }, "WASM build receipt differs from runtime bytes");
  assert.deepEqual(build.recipe, budget.recipe, "WASM build receipt differs from the declared recipe");
  assert.deepEqual(build.profile, expectedWasmCargoProfile(budget), "measured Cargo profile differs from the declared recipe");
  assert.deepEqual(build.inputs, inputs, "WASM build inputs differ from the measured checkout");
  for (const key of ["platform", "rust", "rustcCommit", "wasmPack", "node", "binaryenRelease", "binaryenNodeArchiveSha256"]) {
    assert.equal(build.tools?.[key], tools[key], `built ${key} differs from the measured toolchain`);
  }

  return {
    schemaVersion: 2,
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
    build: { receiptSha256: sha256(JSON.stringify(build)), profile: { ...build.profile } },
    inputs,
    budget: { source: budget.measurement.source, maxRawBytes: budget.policy.maxRawBytes, passed: runtime.length <= budget.policy.maxRawBytes },
    // This receipt admits a measurement; the later size, package and consumer gates still decide release readiness.
    qualification: "measurement-only",
  };
}

export async function recordWasmMeasurement(firstDirectory, outputDirectory) {
  const env = process.env;
  const budget = parseBudgetDocument(readFileSync(join(ROOT, BUDGET)), join(ROOT, BUDGET));
  const buildEnvironment = createWasmBuildEnvironment(budget, ROOT, env);
  const command = (name, args) => execFileSync(name, args, { cwd: ROOT, env: buildEnvironment, encoding: "utf8" }).trim();
  assert.equal(command("git", ["status", "--porcelain=v1", "--untracked-files=normal"]), "", "measurement requires clean source");
  const sourceSha = command("git", ["rev-parse", "HEAD"]);
  buildEnvironment.RUSTC = command("rustup", ["which", "rustc"]);
  const rustc = command(buildEnvironment.RUSTC, ["-vV"]);
  const build = JSON.parse(readFileSync(join(ROOT, "packages/colors/pkg", WASM_BUILD_RECEIPT)));
  const tools = {
    platform: `${process.platform}-${process.arch}`,
    rust: /^release: (.+)$/mu.exec(rustc)?.[1],
    rustcCommit: /^commit-hash: (.+)$/mu.exec(rustc)?.[1],
    wasmPack: command("wasm-pack", ["--version"]).replace(/^wasm-pack /u, ""),
    wasmBindgen: command("wasm-bindgen", ["--version"]).replace(/^wasm-bindgen /u, ""),
    node: process.versions.node,
    binaryenRelease: build.tools.binaryenRelease,
    binaryenNodeArchiveSha256: build.tools.binaryenNodeArchiveSha256,
  };
  const inputs = await readWasmBuildInputs(ROOT);
  const runtime = readFileSync(join(ROOT, RUNTIME));
  const receipt = createWasmMeasurement({
    first: readFileSync(join(firstDirectory, "labcolors_bg.wasm")), runtime,
    firstBuild: JSON.parse(readFileSync(join(firstDirectory, WASM_BUILD_RECEIPT))), build, sourceSha, env, budget,
    workflow: readFileSync(join(ROOT, WORKFLOW), "utf8"), tools, inputs,
  });
  receipt.inputs[WORKFLOW] = sha256(readFileSync(join(ROOT, WORKFLOW)));
  receipt.inputs["scripts/record-wasm-measurement.mjs"] = sha256(readFileSync(SCRIPT_PATH));
  mkdirSync(outputDirectory);
  writeFileSync(join(outputDirectory, "labcolors_bg.wasm"), runtime, { flag: "wx" });
  writeFileSync(join(outputDirectory, "measurement.json"), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
  writeFileSync(join(outputDirectory, WASM_BUILD_RECEIPT), `${JSON.stringify(build, null, 2)}\n`, { flag: "wx" });
  appendFileSync(env.GITHUB_OUTPUT, "measurement-ready=true\n");
  console.log(`canonical WASM measurement: ${runtime.length}B sha256=${receipt.artifact.sha256} source=${sourceSha}`);
  return receipt;
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  assert.equal(process.argv.length, 4, "usage: record-wasm-measurement.mjs FIRST_BUILD_DIRECTORY OUTPUT_DIRECTORY");
  await recordWasmMeasurement(resolve(process.argv[2]), resolve(process.argv[3]));
}
