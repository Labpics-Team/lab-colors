import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const packageRoot = fileURLToPath(new URL("../", import.meta.url));
const witness = "client preference preserves atomic opacity and report-only constraints";
const mutations = [
  {
    name: "reversed preference",
    path: "program-wire/abi-v2.js",
    before: "for (const group of groups) {",
    after: "for (const group of [...groups].reverse()) {",
  },
  {
    name: "discarded candidate opacity",
    path: "program-wire/abi-v1.js",
    before: "sink.f64Bits(candidate.opacity);",
    after: "sink.f64Bits(1);",
  },
  {
    name: "report-only becomes a veto",
    path: "program-wire/abi-v1.js",
    before: 'return this.entry(hard ? "hardConstraints" : "reportConstraints");',
    after: 'return this.entry("hardConstraints");',
  },
];

function run(mutation) {
  const consumer = mkdtempSync(join(tmpdir(), "labcolors-adaptive-mutation-"));
  try {
    const installed = join(consumer, "node_modules/@labpics/colors");
    mkdirSync(installed, { recursive: true });
    for (const path of ["package.json", "index.js", "program-wire", "pkg"]) {
      cpSync(join(packageRoot, path), join(installed, path), { recursive: true });
    }
    const contract = join(consumer, "contract.test.mjs");
    cpSync(join(packageRoot, "test/program-adaptive-contract.test.mjs"), contract);
    if (mutation) {
      const path = join(installed, mutation.path);
      const source = readFileSync(path, "utf8");
      assert.equal(source.split(mutation.before).length, 2, "mutation must have exactly one owned site");
      writeFileSync(path, source.replace(mutation.before, mutation.after));
    }
    // Новый CLI runner не наследует внутренний протокол родительского node:test.
    const env = { ...process.env };
    delete env.NODE_TEST_CONTEXT;
    const result = spawnSync(process.execPath, [
      "--test", "--test-reporter=tap", "--test-name-pattern=^" + witness + "$", contract,
    ], { cwd: consumer, env, encoding: "utf8", timeout: 30_000, maxBuffer: 1024 * 1024 });
    assert.equal(result.error, undefined);
    assert.equal(result.signal, null);
    return result;
  } finally { rmSync(consumer, { recursive: true, force: true }); }
}

test("adaptive contract detects semantic encoder mutations", async (t) => {
  const healthy = run();
  assert.equal(healthy.status, 0, healthy.stdout + healthy.stderr);
  assert.match(healthy.stdout, new RegExp("^ok [0-9]+ - " + witness + "$", "m"));
  for (const mutation of mutations) {
    await t.test(mutation.name, () => {
      const result = run(mutation);
      assert.equal(result.status, 1, result.stdout + result.stderr);
      // Ошибка импорта, синтаксиса или запуска не является обнаруженным дефектом.
      const failures = [...result.stdout.matchAll(/^not ok [0-9]+ - (.+)$/gm)].map((match) => match[1]);
      assert.deepEqual(failures, [witness], result.stdout + result.stderr);
      assert.match(result.stdout, /code: 'ERR_ASSERTION'/);
    });
  }
});
