import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createWasmMeasurement } from "../../../scripts/record-wasm-measurement.mjs";

const budget = JSON.parse(readFileSync(new URL("../bench/wasm.json", import.meta.url)));
const workflow = readFileSync(new URL("../../../.github/workflows/package-worker.yml", import.meta.url), "utf8");
const sourceSha = "1".repeat(40);

function fixture() {
  const runtime = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  return {
    first: Buffer.from(runtime), runtime, sourceSha, budget: structuredClone(budget), workflow,
    tools: { platform: "linux-x64", ...budget.toolchain },
    inputs: { "Cargo.lock": "2".repeat(64) },
    env: {
      GITHUB_SHA: sourceSha, GITHUB_RUN_ID: "123", GITHUB_RUN_ATTEMPT: "2", GITHUB_JOB: "wasm",
      GITHUB_REPOSITORY: "Labpics-Team/lab-colors", GITHUB_EVENT_NAME: "pull_request",
      GITHUB_REF: "refs/pull/1/merge", GITHUB_WORKFLOW_REF: "Labpics-Team/lab-colors/.github/workflows/pr.yml@refs/pull/1/merge",
      GITHUB_WORKFLOW_SHA: sourceSha, GITHUB_SERVER_URL: "https://github.com",
      GITHUB_WORKSPACE: "/checkout", CARGO_HOME: "/cargo",
      CARGO_ENCODED_RUSTFLAGS: "--remap-path-prefix=/checkout=/workspace/lab-colors\x1f--remap-path-prefix=/cargo=/cargo-home",
    },
  };
}

test("measurement preserves reproduced bytes and source/run identity without admitting a release", () => {
  const input = fixture();
  const receipt = createWasmMeasurement(input);
  assert.equal(receipt.sourceSha, sourceSha);
  assert.equal(receipt.run.attempt, "2");
  assert.equal(receipt.run.url, "https://github.com/Labpics-Team/lab-colors/actions/runs/123/attempts/2");
  assert.equal(receipt.artifact.bytes, input.runtime.length);
  assert.equal(receipt.artifact.sha256, createHash("sha256").update(input.runtime).digest("hex"));
  assert.equal(receipt.qualification, "measurement-only");
  assert.deepEqual(receipt.reproducibility, { builds: 2, byteIdentical: true, scope: "runtime-wasm" });
});

test("an over-budget canonical build remains downloadable as a failed-budget measurement", () => {
  const input = fixture();
  input.budget.measurement.rawBytes = 1;
  input.budget.policy.maxRawBytes = 1;
  const receipt = createWasmMeasurement(input);
  assert.equal(receipt.budget.passed, false);
  assert.equal(receipt.artifact.bytes, 8);
  assert.equal(receipt.budget.maxRawBytes, 1);
});

test("different builds, source, recipe, tools and non-CI contexts cannot produce an admitted measurement", () => {
  const changes = [
    [(input) => { input.first = Buffer.concat([input.first, Buffer.from([0])]); }, /differs between/u],
    [(input) => { input.runtime = Buffer.alloc(8); }, /not a WebAssembly/u],
    [(input) => { input.env.GITHUB_SHA = "2".repeat(40); }, /source differs/u],
    [(input) => { delete input.env.GITHUB_RUN_ATTEMPT; }, /GITHUB_RUN_ATTEMPT/u],
    [(input) => { input.tools.platform = "darwin-arm64"; }, /canonical CI platform/u],
    [(input) => { input.tools.rustcCommit = "0".repeat(40); }, /rustcCommit/u],
    [(input) => { input.tools.binaryenNodeArchiveSha256 = "0".repeat(64); }, /binaryenNodeArchiveSha256/u],
    [(input) => { input.env.CARGO_ENCODED_RUSTFLAGS += "\x1f-Ctarget-cpu=native"; }, /Rust flags/u],
    [(input) => { input.workflow = input.workflow.replace("--flatten --rereloop -Oz", "-Oz"); }, /build recipe/u],
  ];
  for (const [edit, reason] of changes) {
    const input = fixture();
    edit(input);
    assert.throws(() => createWasmMeasurement(input), reason);
  }
});
