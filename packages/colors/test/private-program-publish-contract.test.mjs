import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
const readBytes = (...parts) => readFileSync(join(root, ...parts));
const normalizeNewlines = (value) => value.replaceAll("\r\n", "\n");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const STAGE_A_WORKER_SHA = "1461bc2ed60142aed3a8723e618b883be6418156";
const TAR_INSPECTOR_SHA256 =
  "c838aac0b1f918113d55839a77b913d1d4225dda39063c57f80efeb9fc153022";
const STAGE_A_WORKER_REFERENCE =
  `uses: Labpics-Team/lab-colors/.github/workflows/publish-worker.yml@${STAGE_A_WORKER_SHA}`;

function workflowStepLines(workflow, stepName) {
  const lines = normalizeNewlines(workflow).split("\n");
  const starts = lines
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => line.trim() === `- ${stepName}`);
  assert.equal(starts.length, 1, `expected exactly one workflow step: ${stepName}`);
  const start = starts[0].index;
  const indentation = starts[0].line.length - starts[0].line.trimStart().length;
  let end = start + 1;
  while (end < lines.length) {
    const candidate = lines[end];
    const candidateIndentation = candidate.length - candidate.trimStart().length;
    if (candidate.trim().length > 0 && candidateIndentation <= indentation) break;
    end += 1;
  }
  return lines.slice(start, end);
}

function workflowNodeScript(workflow, stepName) {
  const step = workflowStepLines(workflow, stepName);
  const run = step
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => line.trim() === "run: |");
  assert.equal(run.length, 1, `expected one run block: ${stepName}`);
  const runIndentation = run[0].line.length - run[0].line.trimStart().length;
  const body = [];
  for (let cursor = run[0].index + 1; cursor < step.length; cursor += 1) {
    const line = step[cursor];
    const indentation = line.length - line.trimStart().length;
    if (line.trim().length > 0 && indentation <= runIndentation) break;
    body.push(line.length >= runIndentation + 2 ? line.slice(runIndentation + 2) : "");
  }
  const shell = body.join("\n");
  const marker = "node <<'NODE'\n";
  const start = shell.indexOf(marker);
  assert.notEqual(start, -1, `node heredoc not found: ${stepName}`);
  const bodyStart = start + marker.length;
  const end = shell.indexOf("\nNODE", bodyStart);
  assert.notEqual(end, -1, `node heredoc terminator not found: ${stepName}`);
  return shell.slice(bodyStart, end);
}

function assertContains(source, snippet, label) {
  assert.ok(source.includes(snippet), label);
}

function assertStageACaller(caller) {
  const uses = normalizeNewlines(caller)
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("uses:"));
  assert.deepEqual(uses, [STAGE_A_WORKER_REFERENCE]);
}

function assertPrivateProgramPublishContract(workflow) {
  assertContains(
    workflow,
    "runs-on: ubuntu-latest",
    "directory durability and tar admission must execute on the pinned Linux runner family",
  );
  const validator = workflowNodeScript(
    workflow,
    "name: validate manifest identity and byte-exact tarball",
  );
  assert.doesNotThrow(() => new Function(validator), "publish validator must parse as JavaScript");
  assert.equal(
    sha256(readBytes("scripts", "inspect-npm-tarball.py")),
    TAR_INSPECTOR_SHA256,
    "the canonical tar inspector bytes changed without a contract revision",
  );

  const required = [
    ["manifest.schemaVersion !== 5", "publish must accept manifest schema 5 only"],
    [
      '["tarball", "wasm", "buildMetadata", "privateProgramConsumer"]',
      "manifest artifacts must have the exact V5 key order",
    ],
    [
      'manifest.reproducibility.method !== "same-executor-two-pass-npm-pack"',
      "publish must bind the V5 reproducibility method",
    ],
    [
      'const PRIVATE_PROGRAM_ROLE = "private-program-consumer";',
      "publish must pin the private Program role",
    ],
    [
      'const PRIVATE_PROGRAM_METADATA_PATH = "private-program/build-metadata.json";',
      "publish must pin the private metadata path",
    ],
    [
      'const PRIVATE_PROGRAM_CONSUMER_PATH = "private-program/consumer.js";',
      "publish must pin the private consumer path",
    ],
    [
      '"private-program/labcolors_private_program.wasm";',
      "publish must pin the private WASM path",
    ],
    [
      'record.sha256 !== actualSha256',
      "packed private records must be checked against independently hashed bytes",
    ],
    [
      `"${TAR_INSPECTOR_SHA256}";`,
      "publish must pin the exact canonical tar inspector bytes",
    ],
    ["EXPECTED_PACKAGE_FILES.length !== 28", "publish must pin all declared package files"],
    ["EXPECTED_TAR_INVENTORY.length !== 30", "publish must pin the exact npm tar inventory"],
    [
      "if (sha256(inspectorBytes) !== TAR_INSPECTOR_SHA256)",
      "publish must verify the tar inspector before invoking it",
    ],
    [
      'const output = execFileSync(\n    "python3",',
      "publish must invoke the canonical tar inspector",
    ],
    [
      '["schemaVersion", "verdict", "tarball", "limits", "members", "inventory"]',
      "publish must require the exact tar inspection result shape",
    ],
    [
      "JSON.stringify(inspection.inventory.files) !==\n      JSON.stringify(EXPECTED_TAR_INVENTORY)",
      "publish must bind helper output to its worker-owned 30-file allowlist",
    ],
    [
      'member.rawPath !== `package/${member.normalizedPath}`',
      "publish must reject a non-canonical packed path",
    ],
    [
      'member.rawPath !== `package/${member.normalizedPath}` ||\n      member.type !== "file"',
      "publish must reject non-regular packed members",
    ],
    [
      "sha256(packed) !== member.sha256",
      "each extracted member must match the inspector byte hash",
    ],
    [
      'const handle = openSync(path, "wx", 0o600);',
      "validated bytes must be materialized through an exclusive private file",
    ],
    [
      "const tarball = materializeVerifiedTarball(bytes, digest, runnerTemp);",
      "all extraction and publish output must use the verified byte snapshot",
    ],
    ["fsyncSync(handle);", "verified snapshot bytes must be flushed before handoff"],
    [
      'exactOrderedKeys(privateProgram, ["role", "buildMetadata", "consumer"]',
      "private Program provenance must have the exact normalized shape",
    ],
    [
      'privateWasmMatches.length !== 1',
      "the private role must join exactly one WASM record",
    ],
    [
      'privateMetadata.schemaVersion !== 1',
      "packed private metadata must use its exact schema",
    ],
    [
      'privateMetadata.role !== privateProgram.role',
      "packed private metadata must bind the manifest role",
    ],
    [
      'privateMetadata.package.name !== "@labpics/colors"',
      "packed private metadata must bind the package",
    ],
    [
      'privateMetadata.source.gitSha !== expectedSha',
      "packed private metadata must bind the source SHA",
    ],
    [
      'privateMetadata.source.core.version !== manifest.core',
      "packed private metadata must bind the Core version",
    ],
    [
      'JSON.stringify(privateMetadata.source.core.digest) !== JSON.stringify(expectedCoreDigest)',
      "packed private metadata must bind the independently recomputed Core digest",
    ],
    [
      'JSON.stringify(privateMetadata.build) !== JSON.stringify(PRIVATE_PROGRAM_CANONICAL_BUILD)',
      "packed private metadata must bind the canonical private build descriptor",
    ],
    [
      'JSON.stringify(privateMetadata.artifacts.consumer) !==\n    JSON.stringify(privateProgram.consumer)',
      "nested consumer evidence must match the manifest record",
    ],
    [
      'JSON.stringify(privateMetadata.artifacts.wasm) !==\n    JSON.stringify(privateWasmArtifact)',
      "nested WASM evidence must match the role-joined manifest record",
    ],
    [
      'key === "./private-program" || key.startsWith("./private-program/")',
      "private Program files must remain non-exported",
    ],
    [
      "JSON.stringify(packedPackage.files) !== JSON.stringify(EXPECTED_PACKAGE_FILES)",
      "packed package files must match the worker-owned allowlist",
    ],
    [
      "JSON.stringify(packedPackage.exports) !== JSON.stringify(EXPECTED_PACKAGE_EXPORTS)",
      "packed exports must match the admitted public surface",
    ],
  ];
  for (const [snippet, label] of required) assertContains(validator, snippet, label);
  assert.equal(
    (validator.match(/execFileSync\(\s*"python3"/gu) ?? []).length,
    1,
    "publish must invoke exactly one worker-pinned tar inspector",
  );
  assert.ok(
    !validator.includes('["-tzf", tarball]'),
    "publish must not retain a second tar inventory parser",
  );
  const snapshotIndex = validator.indexOf(
    "const tarball = materializeVerifiedTarball(bytes, digest, runnerTemp);",
  );
  const inspectionIndex = validator.indexOf("const tarInspection = inspectCanonicalTarball(");
  const extractionIndex = validator.indexOf("const wcagProfileBytes = packedEvidence(");
  assert.ok(
    snapshotIndex >= 0 && snapshotIndex < inspectionIndex && inspectionIndex < extractionIndex,
    "the pinned inspector must admit the private snapshot before any extraction",
  );
  const iifeIndex = validator.indexOf("(async () => {");
  assert.ok(iifeIndex >= 0 && iifeIndex < snapshotIndex, "validator IIFE is missing");
  const beforeInspection = validator.slice(iifeIndex, inspectionIndex);
  for (const forbidden of [
    'execFileSync("tar"',
    "packedMember(",
    "packedEvidence(",
    "packedArtifact(",
  ]) {
    assert.ok(
      !beforeInspection.includes(forbidden),
      `archive access before canonical inspection is forbidden: ${forbidden}`,
    );
  }
  assert.equal(
    (validator.match(/execFileSync\(\s*"tar"/gu) ?? []).length,
    1,
    "all tar extraction must pass through the one inspected-member boundary",
  );
  assertContains(
    workflow,
    "TARBALL_SHA256: ${{ steps.verified-artifact.outputs.sha256 }}",
    "publish step must receive the validated tarball digest",
  );
  assertContains(
    workflow,
    'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
    "publish step must rehash the tarball after validation",
  );
  assertContains(
    workflow,
    '"$actual_sha256" != "$TARBALL_SHA256"',
    "publish step must fail on validation-to-publish byte drift",
  );
  assertContains(
    workflow,
    "The snapshot is tamper-evident, not immutable against same-UID code.",
    "publish must state its trusted-source and same-job mutation premise honestly",
  );
  const workflowLines = normalizeNewlines(workflow).split("\n");
  const validationStep = workflowLines.findIndex(
    (line) => line.trim() === "- name: validate manifest identity and byte-exact tarball",
  );
  const publishStep = workflowLines.findIndex(
    (line) =>
      line.trim() ===
      "- name: npm publish verified CI tarball (granular token, no rebuild/repack)",
  );
  assert.ok(validationStep >= 0 && validationStep < publishStep, "publish step order drifted");
  assert.ok(
    !workflowLines.slice(validationStep + 1, publishStep).some((line) => /^      - /u.test(line)),
    "no step may run between snapshot validation and the token-bearing publish step",
  );

  assert.deepEqual(
    [...workflow.matchAll(/(?:ci-worker|native-conformance-worker)\.yml@([0-9a-f]{40})/gu)]
      .map((match) => match[1]),
    [STAGE_A_WORKER_SHA, STAGE_A_WORKER_SHA],
    "Stage A must not drift the admitted CI/native worker pins",
  );
}

function replaceOnce(source, before, after) {
  const first = source.indexOf(before);
  assert.notEqual(first, -1, `mutation target is missing: ${before}`);
  assert.equal(source.indexOf(before, first + before.length), -1, `mutation target is not unique: ${before}`);
  return source.slice(0, first) + after + source.slice(first + before.length);
}

test("Stage A keeps publish.yml pinned to the pre-Stage-B worker", () => {
  const caller = read(".github", "workflows", "publish.yml");
  assertStageACaller(caller);

  for (const mutant of [
    caller.replace(STAGE_A_WORKER_SHA, "0".repeat(40)),
    caller.replace(STAGE_A_WORKER_REFERENCE, `uses: ./.github/workflows/publish-worker.yml`),
    caller.replace(STAGE_A_WORKER_REFERENCE, `${STAGE_A_WORKER_REFERENCE}\n    ${STAGE_A_WORKER_REFERENCE}`),
  ]) {
    assert.notEqual(mutant, caller, "caller mutation must alter the contract");
    assert.throws(() => assertStageACaller(mutant));
  }
});

test("publish worker binds every private Program byte through manifest V5", () => {
  const worker = read(".github", "workflows", "publish-worker.yml");
  assertPrivateProgramPublishContract(worker);

  const mutations = [
    ['["tarball", "wasm", "buildMetadata", "privateProgramConsumer"]', '["tarball", "wasm", "buildMetadata"]'],
    ['const PRIVATE_PROGRAM_ROLE = "private-program-consumer";', 'const PRIVATE_PROGRAM_ROLE = "runtime";'],
    ['const PRIVATE_PROGRAM_METADATA_PATH = "private-program/build-metadata.json";', 'const PRIVATE_PROGRAM_METADATA_PATH = "build-metadata.json";'],
    ['const PRIVATE_PROGRAM_CONSUMER_PATH = "private-program/consumer.js";', 'const PRIVATE_PROGRAM_CONSUMER_PATH = "index.js";'],
    ['"private-program/labcolors_private_program.wasm";', '"pkg/labcolors_bg.wasm";'],
    ["record.sha256 !== actualSha256", "record.sha256 === actualSha256"],
    ["manifest.schemaVersion !== 5", "manifest.schemaVersion !== 4"],
    [
      TAR_INSPECTOR_SHA256,
      "0".repeat(64),
    ],
    [
      "if (sha256(inspectorBytes) !== TAR_INSPECTOR_SHA256)",
      "if (sha256(inspectorBytes) === TAR_INSPECTOR_SHA256)",
    ],
    [
      "JSON.stringify(inspection.inventory.files) !==\n                JSON.stringify(EXPECTED_TAR_INVENTORY)",
      "JSON.stringify(inspection.inventory.files) ===\n                JSON.stringify(EXPECTED_TAR_INVENTORY)",
    ],
    [
      'member.rawPath !== `package/${member.normalizedPath}` ||\n                member.type !== "file"',
      'member.rawPath !== `package/${member.normalizedPath}` ||\n                member.type === "file"',
    ],
    ["sha256(packed) !== member.sha256", "sha256(packed) === member.sha256"],
    [
      "const tarball = materializeVerifiedTarball(bytes, digest, runnerTemp);",
      "const tarball = downloadedTarball;",
    ],
    [
      "const tarball = materializeVerifiedTarball(bytes, digest, runnerTemp);",
      'const tarball = materializeVerifiedTarball(bytes, digest, runnerTemp);\n          execFileSync("tar", ["-tzf", tarball]);',
    ],
    ["privateWasmMatches.length !== 1", "privateWasmMatches.length < 1"],
    ["privateMetadata.schemaVersion !== 1", "privateMetadata.schemaVersion !== 2"],
    ["privateMetadata.role !== privateProgram.role", "privateMetadata.role === privateProgram.role"],
    ["privateMetadata.source.gitSha !== expectedSha", "privateMetadata.source.gitSha === expectedSha"],
    ["privateMetadata.source.core.version !== manifest.core", "privateMetadata.source.core.version === manifest.core"],
    [
      "JSON.stringify(privateMetadata.artifacts.consumer) !==\n              JSON.stringify(privateProgram.consumer)",
      "JSON.stringify(privateMetadata.artifacts.consumer) ===\n              JSON.stringify(privateProgram.consumer)",
    ],
    [
      "JSON.stringify(privateMetadata.artifacts.wasm) !==\n              JSON.stringify(privateWasmArtifact)",
      "JSON.stringify(privateMetadata.artifacts.wasm) ===\n              JSON.stringify(privateWasmArtifact)",
    ],
    [
      'key === "./private-program" || key.startsWith("./private-program/")',
      'key === "./private-program"',
    ],
  ];
  assert.equal(mutations.length, 22, "private publish mutation matrix changed");
  for (const [before, after] of mutations) {
    const mutant = replaceOnce(worker, before, after);
    assert.throws(
      () => assertPrivateProgramPublishContract(mutant),
      undefined,
      `mutation must be rejected: ${before}`,
    );
  }
});
