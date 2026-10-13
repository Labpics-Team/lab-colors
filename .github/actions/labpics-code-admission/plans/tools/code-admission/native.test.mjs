import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileDigest, prepareQualifiedExecutable } from "./identity.mjs";
import { runStructuralRule } from "./structural.mjs";
import { storageFindings } from "./storage.mjs";
import { validateStructuralRules } from "../code-admission.mjs";

const rulesPath = new URL("./rules.json", import.meta.url);
const definitions = JSON.parse(readFileSync(rulesPath, "utf8"));
const rule = validateStructuralRules(definitions).find(rule => rule.id === "lab-go-panic");
const controls = definitions.find(rule => rule.id === "lab-go-panic");
const binary = process.env.CODE_ADMISSION_AST_GREP;

function fixture(t) {
  assert.ok(binary, "CODE_ADMISSION_AST_GREP должен указывать на pinned native parser");
  const root = mkdtempSync(join(tmpdir(), "http-abort-admission-"));
  const config = join(root, "sgconfig.yml");
  writeFileSync(config, "ruleDirs: []\n");
  const runtime = prepareQualifiedExecutable(binary, fileDigest(binary), root, "native AST proof");
  t.after(() => { runtime.close(); rmSync(root, { recursive: true, force: true }); });
  const scan = args => {
    const result = runtime.run(args, {
      encoding: "utf8", maxBuffer: 8 * 1024 * 1024, timeout: 30000,
      env: { PATH: process.env.PATH, HOME: root, TMPDIR: root, NO_COLOR: "1" },
      cwd: root, stdio: ["ignore", "pipe", "pipe"],
    });
    assert.equal(result.error, undefined);
    assert.ok([0, 1].includes(result.status), result.stderr);
    return JSON.parse(result.stdout);
  };
  return { root, scan, config };
}

test("native AST допускает только прямой HTTP abort с доказанным импортом", t => {
  const f = fixture(t);
  for (const control of controls.controls) {
    const path = join(f.root, control.name + ".go");
    writeFileSync(path, control.source);
    const found = runStructuralRule(rule, [path], f.scan, f.config);
    assert.equal(found.length, control.matches, control.name);
  }
});

test("исходный blanket panic запрет воспроизводит ложный отказ на законном HTTP abort", t => {
  const f = fixture(t), path = join(f.root, "handler.go");
  writeFileSync(path, controls.negative);
  const original = { ...rule };
  delete original.allow;
  assert.equal(runStructuralRule(original, [path], f.scan, f.config).length, 1, "RED исходного дефекта");
  assert.equal(runStructuralRule(rule, [path], f.scan, f.config).length, 0, "законный соседний исход");
  writeFileSync(path, controls.positive);
  assert.equal(runStructuralRule(rule, [path], f.scan, f.config).length, 1, "обычный panic всё ещё запрещён");
});

test("подавление scan не убирает исходную panic находку", t => {
  const f = fixture(t), path = join(f.root, "suppressed.go");
  const suppression = ["ast", "grep", "ignore"].join("-");
  writeFileSync(path, 'package p\nfunc f() {\n// ' + suppression + ': lab-go-panic\npanic("boom")\n}\n');
  assert.equal(runStructuralRule(rule, [path], f.scan, f.config).length, 1);
});

test("native qualification использует те же разрешения и отвергает исходный дефект", t => {
  const f = fixture(t);
  const script = resolve("plans/tools/code-admission/qualify-ast-grep.mjs");
  const args = [script, "--ast-grep", resolve(binary), "--rules", resolve("plans/tools/code-admission/rules.json")];
  const options = { encoding: "utf8", timeout: 120000, stdio: ["ignore", "pipe", "pipe"] };
  const receipt = JSON.parse(execFileSync(process.execPath, args, options));
  const qualified = receipt.rules.find(rule => rule.ruleId === "lab-go-panic");
  assert.equal(qualified.positive, 1);
  assert.equal(qualified.negative, 0);
  assert.deepEqual(qualified.controls, controls.controls.map(({ name, matches }) => ({ name, matches })));
  const original = structuredClone(definitions);
  delete original.find(rule => rule.id === "lab-go-panic").allow;
  const mutantPath = join(f.root, "original-rules.json");
  writeFileSync(mutantPath, JSON.stringify(original));
  const failed = spawnSync(process.execPath, [...args.slice(0, -1), mutantPath], options);
  assert.equal(failed.status, 1);
  assert.match(failed.stderr, /lab-go-panic: negative contrast was incorrectly detected/);
});


test("native YAML distinguishes ordinary retention from a one-day failure or explicit delivery", t => {
  const f = fixture(t), path = join(f.root, 'workflow.yml');
  const cases = [
    [undefined, 1, true], ["always()", 1, true], ["failure() || true", 1, true],
    ["failure()", 1, false], ["${{ failure() && steps.test.outcome == 'failure' }}", 1, false],
    ["github.event_name == 'workflow_dispatch'", 1, false],
    ["github.ref_type == 'tag'", 1, false], ["!failure()", 1, true],
    ["github.event_name == 'workflow_dispatch' || success()", 1, true],
    ["failure()", 7, true], ["failure()", undefined, true],
  ];
  for (const [condition, retention, rejected] of cases) {
    const source = ["on: push", "jobs:", "  test:", "    runs-on: self-hosted", "    steps:",
      "      - name: retain", ...(condition ? [`        if: ${condition}`] : []),
      "        uses: actions/upload-artifact@0123456789012345678901234567890123456789",
      "        with:", "          path: result.json", ...(retention ? [`          retention-days: ${retention}`] : []), ""].join('\n');
    writeFileSync(path, source);
    assert.equal(storageFindings(f.root, [path], f.scan, f.config).length > 0, rejected, String(condition));
  }
});

test("native YAML follows flow mappings and job-owned conditions without trusting comments", t => {
  const f = fixture(t), path = join(f.root, 'action.yml');
  for (const [source, rejected] of [
    ["jobs: { x: { steps: [{ uses: actions/upload-artifact@v4, with: {path: report, retention-days: 1} }] } }", true],
    ["jobs: { x: { if: failure(), steps: [{ uses: actions/upload-artifact@v4, with: {path: report, retention-days: 1} }] } }", false],
    ["# uses: actions/upload-artifact@v4\njobs: { x: { steps: [{run: echo ok}] } }", false],
    ["runs:\n  using: composite\n  steps:\n    - uses: actions/upload-artifact@v4\n      with:\n        retention-days: 1\n", true],
  ]) {
    writeFileSync(path, source);
    assert.equal(storageFindings(f.root, [path], f.scan, f.config).length > 0, rejected, source);
  }
});


test('native YAML rejects ambiguous action identity and hidden or re-enabled storage', t => {
  const f = fixture(t), path = join(f.root, 'workflow.yml');
  for (const [step, rejected] of [
    ['uses: docker/build-push-action@v6', true],
    ["uses: 'actions/upload-artifact@v4' # quoted producer", true],
    ['uses: docker/build-push-action@v6\n        env: {DOCKER_BUILD_RECORD_UPLOAD: "false", DOCKER_BUILD_RECORD_UPLOAD: "true"}', true],
    ['uses: docker/build-push-action@v6\n        env: {DOCKER_BUILD_RECORD_UPLOAD: "false"}', false],
    ['uses: docker/build-push-action@v6\n        env: {DOCKER_BUILD_RECORD_UPLOAD: "true"}', true],
    ['uses: &producer actions/upload-artifact@v4\n        if: failure()\n        with: {retention-days: 1}', true],
    ['uses: *producer', true],
    ['uses: actions/upload-artifact@v4\n        if: failure()\n        if: always()\n        with: {retention-days: 1}', true],
    ['uses: actions/upload-artifact@v4\n        if: failure()\n        with: {retention-days: 1, retention-days: 90}', true],
  ]) {
    writeFileSync(path, 'jobs:\n  test:\n    steps:\n      - '+step+'\n');
    assert.equal(storageFindings(f.root, [path], f.scan, f.config).length > 0, rejected, step);
  }
});

test('native Docker record setting follows the nearest environment owner', t => {
  const f = fixture(t), path = join(f.root, 'workflow.yml');
  for (const [override, rejected] of [['', false], ['        env: {DOCKER_BUILD_RECORD_UPLOAD: "true"}\n', true], ['        env: ${{ fromJSON(inputs.environment) }}\n', true]]) {
    writeFileSync(path, 'env: {DOCKER_BUILD_RECORD_UPLOAD: "false"}\njobs:\n  build:\n    steps:\n      - uses: docker/build-push-action@v6\n'+override);
    assert.equal(storageFindings(f.root, [path], f.scan, f.config).length > 0, rejected);
  }
});
