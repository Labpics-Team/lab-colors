import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  DEFAULT_BUDGET,
  evaluateWasmBudget,
  parseBudgetDocument,
} from "../../../scripts/check-wasm-size-budget.mjs";

const script = fileURLToPath(
  new URL("../../../scripts/check-wasm-size-budget.mjs", import.meta.url),
);
const document = readFileSync(DEFAULT_BUDGET);
const emptyWasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
const canonical = (value) => Buffer.from(`${JSON.stringify(value, null, 2)}\n`);

function fixtureBudget() {
  const budget = JSON.parse(document);
  budget.measurement.rawBytes = emptyWasm.length;
  budget.policy.maxRawBytes = emptyWasm.length;
  return parseBudgetDocument(canonical(budget), join(tmpdir(), "fixture-budget.json"));
}

test("checked-in WASM budget reproduces its pinned document digest", () => {
  const budget = parseBudgetDocument(document, DEFAULT_BUDGET);
  const changed = structuredClone(budget);
  changed.policy.basis += "-changed";
  assert.throws(
    () => parseBudgetDocument(canonical(changed), DEFAULT_BUDGET),
    /current budget file SHA-256 mismatch/u,
  );
});

test("WASM budget rejects non-canonical documents and arbitrary headroom", () => {
  const budget = fixtureBudget();
  assert.throws(
    () => parseBudgetDocument(JSON.stringify(budget), "fixture-budget.json"),
    /canonical JSON/u,
  );
  budget.policy.maxRawBytes += 1;
  assert.throws(
    () => parseBudgetDocument(canonical(budget), "fixture-budget.json"),
    /zero arbitrary headroom/u,
  );
});

test("canonical WASM measurements require the exact pinned length", () => {
  const budget = fixtureBudget();
  const result = evaluateWasmBudget(budget, emptyWasm, budget.measurement.platform);
  assert.equal(result.status, "PASS");
  assert.equal(result.deltaBytes, 0);
  assert.throws(
    () => evaluateWasmBudget(
      budget, Buffer.concat([emptyWasm, Buffer.from([0])]), budget.measurement.platform,
    ),
    /exact artifact length mismatch/u,
  );
  assert.throws(
    () => evaluateWasmBudget(budget, Buffer.alloc(8), budget.measurement.platform),
    /not a WebAssembly binary/u,
  );
});

test("non-canonical platforms stay diagnostic rather than gaining authority", () => {
  const result = evaluateWasmBudget(fixtureBudget(), emptyWasm, "other-platform");
  assert.equal(result.status, "DIAGNOSTIC");
});

test("WASM budget CLI works without Python or Git and emits only its measurement", (t) => {
  const directory = mkdtempSync(join(tmpdir(), "lab-colors-size-budget-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const budgetPath = join(directory, "wasm.json");
  const runtimePath = join(directory, "runtime.wasm");
  writeFileSync(budgetPath, canonical(fixtureBudget()));
  writeFileSync(runtimePath, emptyWasm);
  // This standalone size check owns no repository inventory or subprocesses.
  // Remove both PATH spellings for Windows as well as Unix, retaining Node's absolute path.
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([key]) => key.toLowerCase() !== "path"),
  );
  env.PATH = "";
  const result = spawnSync(process.execPath, [
    script, "--budget", budgetPath, "--runtime-wasm", runtimePath,
  ], { cwd: directory, env, encoding: "utf8", timeout: 10_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, "");
  assert.match(
    result.stdout,
    /^WASM size budget (?:PASS|DIAGNOSTIC) role=runtime raw=8B ceiling=8B delta=\+0B gzip=\d+B diagnostic-only platform=\S+ artifact=[^\r\n]+ artifact-sha256=[a-f0-9]{64}\r?\n$/u,
  );
});
