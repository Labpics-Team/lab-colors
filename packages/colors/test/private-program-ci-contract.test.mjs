import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
const normalizeNewlines = (value) => value.replaceAll("\r\n", "\n");

const CALLER_WORKER_SHA = "beecd257371a7a6421079b0d8207a109969aa332";
const CALLER_WORKER_REFERENCE =
  `    uses: Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@${CALLER_WORKER_SHA}`;
const RUNTIME_BUDGET_COMMAND = "        run: node scripts/check-wasm-size-budget.mjs";
const PRIVATE_BUDGET_COMMAND =
  "        run: node scripts/check-private-program-wasm-size-budget.mjs";
const PRIVATE_BROWSER_COMMAND =
  '          node scripts/test-private-program-browser.mjs "$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"';

function workflowStep(workflow, name) {
  const normalized = normalizeNewlines(workflow);
  const marker = `      - name: ${name}`;
  const start = normalized.indexOf(marker);
  assert.notEqual(start, -1, `workflow is missing step: ${name}`);
  const next = normalized.indexOf("\n      - ", start + marker.length);
  return normalized.slice(start, next === -1 ? normalized.length : next);
}

function uniqueOffset(workflow, marker) {
  const first = workflow.indexOf(marker);
  assert.notEqual(first, -1, `workflow is missing ${marker}`);
  assert.equal(
    workflow.indexOf(marker, first + marker.length),
    -1,
    `workflow duplicates ${marker}`,
  );
  return first;
}

function assertImmutableCaller(workflow) {
  const usesLines = normalizeNewlines(workflow)
    .split("\n")
    .filter((line) => line.trimStart().startsWith("uses:"));
  assert.deepEqual(usesLines, [CALLER_WORKER_REFERENCE]);
}

function assertWorkerOrderAndRoles(workflow) {
  const normalized = normalizeNewlines(workflow);
  assert.deepEqual(
    normalized.split("\n").filter((line) => line === RUNTIME_BUDGET_COMMAND),
    [RUNTIME_BUDGET_COMMAND],
    "runtime WASM must retain its one canonical budget gate",
  );
  assert.deepEqual(
    normalized.split("\n").filter((line) => line === PRIVATE_BUDGET_COMMAND),
    [PRIVATE_BUDGET_COMMAND],
    "private Program WASM must have one separate canonical budget gate",
  );

  const release = uniqueOffset(
    normalized,
    "name: verify byte-exact npm release artifact",
  );
  const privateBudget = uniqueOffset(
    normalized,
    "name: enforce measured private Program optimized WASM budget",
  );
  const identity = uniqueOffset(normalized, "name: bind verified npm tarball SHA-256");
  const upload = uniqueOffset(normalized, "name: upload verified npm tarball and manifest");
  const chrome = uniqueOffset(normalized, "name: install Chrome + dependencies");
  const privateBrowser = uniqueOffset(
    normalized,
    'name: "@labpics/colors: private Program proof from verified tarball in real browser"',
  );
  const publicBrowser = uniqueOffset(
    normalized,
    'name: "@labpics/colors: real-browser atomic output-sink proof"',
  );
  const wasmPackBrowser = uniqueOffset(normalized, "name: wasm-pack test (headless chrome)");
  assert.ok(
    release < privateBudget &&
      privateBudget < identity &&
      identity < upload &&
      upload < chrome &&
      chrome < publicBrowser &&
      publicBrowser < privateBrowser &&
      privateBrowser < wasmPackBrowser,
    "release verification, private budget, identity, upload, Chrome, and browser proofs must remain ordered",
  );
}

function assertExactTarballBinding(workflow) {
  const identity = workflowStep(workflow, "bind verified npm tarball SHA-256");
  assert.match(
    identity,
    /id: verified-release-identity\n\s+env:\n\s+VERIFIED_TARBALL: \$\{\{ steps\.verified-release\.outputs\.tarball \}\}/u,
  );
  assert.match(identity, /^\s*set -euo pipefail$/mu);
  assert.match(identity, /^\s*test -f "\$VERIFIED_TARBALL"$/mu);
  assert.match(
    identity,
    /sha256sum --binary -- "\$VERIFIED_TARBALL"/u,
    "digest must be calculated from the exact verifier output path",
  );
  assert.match(identity, /\^\[0-9a-f\]\{64\}\$/u);
  assert.match(
    identity,
    /printf 'sha256=%s\\n' "\$tarball_sha256" >> "\$GITHUB_OUTPUT"/u,
  );

  const browser = workflowStep(
    workflow,
    '"@labpics/colors: private Program proof from verified tarball in real browser"',
  );
  assert.match(
    browser,
    /VERIFIED_TARBALL: \$\{\{ steps\.verified-release\.outputs\.tarball \}\}/u,
  );
  assert.match(
    browser,
    /VERIFIED_TARBALL_SHA256: \$\{\{ steps\.verified-release-identity\.outputs\.sha256 \}\}/u,
  );
  assert.deepEqual(
    normalizeNewlines(browser).split("\n").filter((line) => line === PRIVATE_BROWSER_COMMAND),
    [PRIVATE_BROWSER_COMMAND],
    "browser proof must receive the verifier path and the digest of those exact bytes",
  );
}

function assertPrivateMutationDeadline(workflow) {
  const normalized = normalizeNewlines(workflow);
  const wasmStart = normalized.indexOf("\n  wasm:\n");
  assert.notEqual(wasmStart, -1, "worker must retain the wasm job");
  const nextJobMatch = /\n  [A-Za-z0-9_-]+:\n/gu.exec(
    normalized.slice(wasmStart + "\n  wasm:\n".length),
  );
  const nextJob = nextJobMatch
    ? wasmStart + "\n  wasm:\n".length + nextJobMatch.index
    : -1;
  const wasm = normalized.slice(wasmStart, nextJob === -1 ? normalized.length : nextJob);
  assert.match(wasm, /timeout-minutes: 70\n/u, "wasm job must include mutation headroom");
  for (const declaration of [
    'WASM_JOB_TIMEOUT_MINUTES: "70"',
    'WASM_PRE_MUTATION_BUDGET_MINUTES: "40"',
    'WASM_PRIVATE_MUTATION_BUDGET_MINUTES: "20"',
    'WASM_JOB_HEADROOM_MINUTES: "5"',
  ]) {
    assert.match(wasm, new RegExp(declaration.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"));
  }
  const start = wasm.indexOf("name: start wasm job deadline ledger");
  const guard = wasm.indexOf("name: assert private mutation deadline headroom");
  const mutation = wasm.indexOf('name: "@labpics/colors: private Program packed-browser mutation proof"');
  const wasmPack = wasm.indexOf("name: wasm-pack test (headless chrome)");
  assert.ok(
    start >= 0 &&
      start < wasmPack &&
      wasmPack < guard &&
      guard < mutation,
    "deadline ledger, wasm parity, guard, and mutation must be ordered",
  );
  const guardText = wasm.slice(guard, mutation);
  assert.match(guardText, /WASM_JOB_STARTED_EPOCH/u);
  assert.match(guardText, /WASM_PRE_MUTATION_BUDGET_MINUTES/u);
  assert.match(guardText, /WASM_PRIVATE_MUTATION_BUDGET_MINUTES/u);
  assert.match(guardText, /WASM_JOB_HEADROOM_MINUTES/u);
  assert.match(guardText, /remaining/u);
  assert.match(guardText, /remaining="\$\(\(WASM_JOB_TIMEOUT_MINUTES \* 60 - elapsed\)\)"/u);
  assert.match(
    guardText,
    /required="\$\(\(\(WASM_PRIVATE_MUTATION_BUDGET_MINUTES \+ WASM_JOB_HEADROOM_MINUTES\) \* 60\)\)"/u,
  );
  assert.match(guardText, /exit 1/u);
  assert.ok(70 > 40 + 20 + 5, "outer timeout must exceed declared budgets and teardown headroom");
}

test("Stage B activates the public caller at the merged Stage A worker commit", () => {
  const caller = read(".github", "workflows", "ci.yml");
  assertImmutableCaller(caller);

  for (const mutation of [
    caller.replace(CALLER_WORKER_SHA, "0".repeat(40)),
    caller.replace(CALLER_WORKER_SHA, "main"),
    caller.replace(CALLER_WORKER_SHA, CALLER_WORKER_SHA.slice(0, 12)),
    caller.replace(CALLER_WORKER_REFERENCE, ""),
    caller.replace(CALLER_WORKER_REFERENCE, `${CALLER_WORKER_REFERENCE}\n${CALLER_WORKER_REFERENCE}`),
    caller.replace(
      CALLER_WORKER_REFERENCE,
      "    uses: ./.github/workflows/ci-worker.yml",
    ),
  ]) {
    assert.notEqual(mutation, caller, "caller mutation must alter the live contract");
    assert.throws(() => assertImmutableCaller(mutation));
  }
});

test("worker keeps runtime and private size budgets separate and ordered fail-closed", () => {
  const worker = read(".github", "workflows", "ci-worker.yml");
  assertWorkerOrderAndRoles(worker);

  for (const mutation of [
    worker.replace(PRIVATE_BUDGET_COMMAND, RUNTIME_BUDGET_COMMAND),
    worker.replace(
      "name: enforce measured private Program optimized WASM budget",
      "name: private Program size diagnostic only",
    ),
  ]) {
    assert.notEqual(mutation, worker, "size-role mutation must alter the live contract");
    assert.throws(() => assertWorkerOrderAndRoles(mutation));
  }

  const privateChecker = read("scripts", "check-private-program-wasm-size-budget.mjs");
  assert.match(privateChecker, /role=\$\{PRIVATE_PROGRAM_ROLE\}/u);
  assert.match(privateChecker, /packages\/colors\/bench\/private-program-wasm\.json/u);
  assert.match(
    privateChecker,
    /packages\/colors\/private-program\/labcolors_private_program\.wasm/u,
  );
  assert.doesNotMatch(privateChecker, /check-wasm-size-budget\.mjs/u);
});

test("worker binds the browser proof to the exact verified tarball bytes", () => {
  const worker = read(".github", "workflows", "ci-worker.yml");
  assertExactTarballBinding(worker);
  assertWorkerOrderAndRoles(worker);

  for (const mutation of [
    worker.replace(
      "VERIFIED_TARBALL: ${{ steps.verified-release.outputs.tarball }}",
      "VERIFIED_TARBALL: ${{ steps.verified-release.outputs.manifest }}",
    ),
    worker.replace(
      "sha256sum --binary -- \"$VERIFIED_TARBALL\"",
      "sha256sum --binary -- packages/colors/.release/*.tgz",
    ),
    worker.replace(
      PRIVATE_BROWSER_COMMAND,
      "          node scripts/test-private-program-browser.mjs packages/colors/.release/*.tgz deadbeef",
    ),
  ]) {
    assert.notEqual(mutation, worker, "tarball-identity mutation must alter the live contract");
    assert.throws(() => assertExactTarballBinding(mutation));
  }

  const chrome = workflowStep(
    worker,
    "install Chrome + dependencies (Chrome for Testing + apt-get download)",
  );
  const browser = workflowStep(
    worker,
    '"@labpics/colors: private Program proof from verified tarball in real browser"',
  );
  const withoutBrowser = worker.replace(`${browser}\n`, "");
  const reordered = withoutBrowser.replace(chrome, `${browser}\n${chrome}`);
  assert.notEqual(reordered, worker, "browser-order mutation must alter the live workflow");
  assert.throws(() => assertWorkerOrderAndRoles(reordered));
});

test("private mutation keeps its own deadline reachable inside the wasm job", () => {
  const worker = read(".github", "workflows", "ci-worker.yml");
  assertPrivateMutationDeadline(worker);
  for (const mutation of [
    worker.replace("timeout-minutes: 70", "timeout-minutes: 40"),
    worker.replace('WASM_JOB_TIMEOUT_MINUTES: "70"', 'WASM_JOB_TIMEOUT_MINUTES: "40"'),
    worker.replace("name: assert private mutation deadline headroom", "name: mutation deadline diagnostic only"),
    worker.replace("WASM_JOB_HEADROOM_MINUTES: \"5\"", "WASM_JOB_HEADROOM_MINUTES: \"0\""),
  ]) {
    assert.notEqual(mutation, worker, "deadline mutation must alter the live workflow");
    assert.throws(() => assertPrivateMutationDeadline(mutation));
  }
});

test("missing or pending private size budget reports the artifact then fails closed", () => {
  const temporary = mkdtempSync(join(tmpdir(), "labcolors-private-program-size-budget-"));
  try {
    const wasmPath = join(temporary, "private-program.wasm");
    const missingBudget = join(temporary, "missing-budget.json");
    const pendingBudget = join(temporary, "pending-budget.json");
    const wasm = Buffer.alloc(16);
    wasm.set([0x00, 0x61, 0x73, 0x6d]);
    writeFileSync(wasmPath, wasm);
    writeFileSync(
      pendingBudget,
      `${JSON.stringify({ status: "pending" }, null, 2)}\n`,
    );
    const expectedSha256 = createHash("sha256").update(wasm).digest("hex");
    const checker = join(root, "scripts", "check-private-program-wasm-size-budget.mjs");
    const run = (budgetPath) => spawnSync(
      process.execPath,
      [
        checker,
        "--budget",
        budgetPath,
        "--private-program-wasm",
        wasmPath,
      ],
      { cwd: root, encoding: "utf8" },
    );

    for (const [label, budgetPath, failure] of [
      ["missing", missingBudget, /budget file is missing/u],
      ["pending", pendingBudget, /budget is pending/u],
    ]) {
      const result = run(budgetPath);
      assert.notEqual(result.status, 0, `${label} budget must fail closed`);
      assert.match(
        result.stdout,
        new RegExp(
          `role=private-program-consumer raw=16B .*artifact-sha256=${expectedSha256}`,
          "u",
        ),
        `${label} budget failure must retain the observed bytes and hash`,
      );
      assert.match(result.stderr, failure);
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("private budget contract carries only its relevant optimized-build pins", async () => {
  const checker = await import(
    new URL("../../../scripts/check-private-program-wasm-size-budget.mjs", import.meta.url)
  );
  assert.equal(checker.PRIVATE_PROGRAM_CANONICAL_PLATFORM, "linux-x64");
  assert.equal(checker.PRIVATE_PROGRAM_ROLE, "private-program-consumer");
  assert.deepEqual(Object.keys(checker.PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN), [
    "rust",
    "rustcCommit",
    "cargo",
    "cargoCommit",
    "target",
    "profile",
    "feature",
    "node",
    "binaryenRelease",
    "binaryenNodeArchiveSha256",
    "binaryenComponentSha256",
    "wasmOptFlags",
  ]);
  assert.doesNotMatch(
    JSON.stringify(checker.PRIVATE_PROGRAM_EXPECTED_TOOLCHAIN),
    /wasmPack|wasmBindgen/iu,
  );

  const wasm = Buffer.alloc(16);
  wasm.set([0x00, 0x61, 0x73, 0x6d]);
  const observation = checker.observePrivateProgramWasm(wasm, "linux-x64");
  const exactBudget = {
    measurement: { platform: "linux-x64", rawBytes: wasm.length },
    policy: { maxRawBytes: wasm.length },
  };
  assert.equal(
    checker.evaluatePrivateProgramWasmBudget(exactBudget, observation).status,
    "PASS",
  );
  assert.throws(
    () => checker.evaluatePrivateProgramWasmBudget(
      {
        measurement: { platform: "linux-x64", rawBytes: wasm.length },
        policy: { maxRawBytes: wasm.length + 1 },
      },
      observation,
    ),
    /zero arbitrary headroom/u,
  );
  assert.equal(
    checker.evaluatePrivateProgramWasmBudget(exactBudget, {
      ...observation,
      currentPlatform: "darwin-arm64",
    }).status,
    "DIAGNOSTIC",
    "non-canonical hosts must not acquire a second baseline",
  );
});

const PRIVATE_FIXTURE_STEP_NAME = "cargo test private-fixture feature";
const PRIVATE_FIXTURE_COMMAND =
  "cargo test -p labcolors-core --lib --features private-fixture --locked";
const PRIVATE_FIXTURE_RUN = `        run: ${PRIVATE_FIXTURE_COMMAND}`;
const DEFAULT_WORKSPACE_RUN = "        run: cargo test --workspace --locked";

function testJobBody(workflow) {
  const normalized = normalizeNewlines(workflow);
  const start = normalized.indexOf("\n  test:\n");
  assert.notEqual(start, -1, "worker must retain the mandatory core test job");
  const nextJobMatch = /\n  [A-Za-z0-9_-]+:\n/gu.exec(
    normalized.slice(start + "\n  test:\n".length),
  );
  const nextJob = nextJobMatch
    ? start + "\n  test:\n".length + nextJobMatch.index
    : -1;
  return normalized.slice(start, nextJob === -1 ? normalized.length : nextJob);
}

function assertPrivateFixtureTestGate(workflow) {
  const testJob = testJobBody(workflow);
  // The default workspace run must stay; the private-fixture gate is an
  // additional mandatory step inside the same core test job. The lifecycle and
  // dispose tests in private_fixture.rs compile only with `--features
  // private-fixture`, so the default `cargo test --workspace --locked` step
  // cannot stand in for it (CI ran only default features until this gate).
  assert.ok(
    testJob.includes(DEFAULT_WORKSPACE_RUN),
    "the mandatory core test job must retain the default workspace cargo test",
  );
  assert.deepEqual(
    testJob.split("\n").filter((line) => line === PRIVATE_FIXTURE_RUN),
    [PRIVATE_FIXTURE_RUN],
    "the mandatory core test job must execute the private-fixture gate once",
  );
  assert.ok(
    testJob.indexOf(DEFAULT_WORKSPACE_RUN) < testJob.indexOf(PRIVATE_FIXTURE_RUN),
    "the private-fixture gate must not replace the default workspace cargo test",
  );
  const step = workflowStep(testJob, PRIVATE_FIXTURE_STEP_NAME);
  assert.doesNotMatch(
    step,
    /^\s*(?:if|continue-on-error):/mu,
    "the private-fixture gate cannot be conditional or made non-blocking",
  );
}

test("the feature-gated private fixture lifecycle tests stay one required core test gate", () => {
  const worker = read(".github", "workflows", "ci-worker.yml");
  assertPrivateFixtureTestGate(worker);
  assertPrivateFixtureTestGate(worker.replaceAll("\n", "\r\n"));

  for (const [label, mutant] of [
    ["removed", worker.replace(`${PRIVATE_FIXTURE_RUN}\n`, "")],
    [
      "weakened",
      worker.replace(
        PRIVATE_FIXTURE_COMMAND,
        "cargo test -p labcolors-core --lib --locked",
      ),
    ],
    [
      "non-blocking",
      worker.replace(
        `      - name: ${PRIVATE_FIXTURE_STEP_NAME}`,
        `      - name: ${PRIVATE_FIXTURE_STEP_NAME}\n        continue-on-error: true`,
      ),
    ],
    [
      "conditional",
      worker.replace(
        `      - name: ${PRIVATE_FIXTURE_STEP_NAME}`,
        `      - name: ${PRIVATE_FIXTURE_STEP_NAME}\n        if: false`,
      ),
    ],
  ]) {
    assert.notEqual(mutant, worker, `${label} mutation must alter the live workflow`);
    assert.throws(
      () => assertPrivateFixtureTestGate(mutant),
      undefined,
      `${label} mutation must be rejected`,
    );
  }

  // Moving the gate out of the mandatory core test job into another job must
  // fail even though the exact command still exists somewhere in the file.
  const step = workflowStep(worker, PRIVATE_FIXTURE_STEP_NAME);
  const withoutGate = worker.replace(`${step}\n`, "");
  const auditMarker = "  audit:\n";
  const auditIndex = withoutGate.indexOf(auditMarker);
  assert.notEqual(auditIndex, -1, "worker must retain the audit job");
  const reordered = `${withoutGate.slice(0, auditIndex + auditMarker.length)}${step}\n${withoutGate.slice(auditIndex + auditMarker.length)}`;
  assert.notEqual(reordered, worker, "relocation mutation must alter the live workflow");
  assert.throws(
    () => assertPrivateFixtureTestGate(reordered),
    undefined,
    "relocation into a non-core job must be rejected",
  );
});
