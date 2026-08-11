import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { lstat, mkdir, mkdtemp, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  PRIVATE_PROGRAM_MUTATION_CASES,
  applyExactMutation,
  assertAdmittedRegularFile,
  assertAdmittedTree,
  assertMutationSpecificBrowserFailure,
  assertPrivateProgramMutationAnchors,
  parseMutationTimeoutPolicy,
  validateMutationWasm,
} from "../../../scripts/test-private-program-mutations.mjs";
import { copyDeclaredCargoRegistryIndex } from "../../../scripts/build-private-program.mjs";

const EXPECTED_MUTATIONS = Object.freeze([
  [
    "compiler-graph-edge-deletion",
    "semantic",
    "rust-wasm",
    "crates/labcolors-core/src/private_fixture.rs",
    "6d43e030e5d6f4df67b319276d3d9ea1ab33d5b91a2094019ce278476cecd412",
    "PrivateProgramConsumerError: private Program consumer: run failed with status 6",
  ],
  [
    "hard-constraint-deletion",
    "semantic",
    "rust-wasm",
    "crates/labcolors-core/src/private_fixture.rs",
    "0e43717331468ae6fb6c65bb6ba441cd5260b49b37ad3c6927a9bbf02494638b",
    "PrivateProgramConsumerError: private Program consumer: run failed with status 6",
  ],
  [
    "final-recheck-call-edge-deletion",
    "semantic-source-deletion",
    "rust-wasm",
    "crates/labcolors-core/src/program_session.rs",
    "8e218fddef0b8a13f044b91b7927faeb50e501a54ecb205c05696d6510fe615e",
    "PrivateProgramConsumerError: private Program consumer: run failed with status 8",
  ],
  [
    "session-observed-update-bypass",
    "semantic",
    "rust-wasm",
    "crates/labcolors-core/src/private_fixture.rs",
    "8770e1a30407e9cd9ec3ee32feed44cd2b58a0d39be3c636cb428be8b1c472fe",
    "PrivateProgramConsumerError: private Program consumer: shipping trace permits exactly one SetAll callback",
  ],
  [
    "external-attachment-handoff-binding-bypass",
    "semantic-source-bypass",
    "rust-wasm",
    "crates/labcolors-core/src/private_fixture.rs",
    "59f5bc640fbc270db4dbbda5572e339c90aa839d61cab0ac155ef8db5e1e5d59",
    "PrivateProgramConsumerError: private Program consumer: run failed with status 7",
  ],
  [
    "javascript-publish-deletion",
    "semantic",
    "javascript",
    "packages/colors/private-program/consumer.js",
    "354125be94020475d0e34beb0f4498474cf243adb89d3bb591589ff5d485af0b",
    "Error: private Program browser fixture: computed background is the exact expected CSS literal; expected \"rgba(64, 64, 64, 0.5)\", got \"rgba(0, 0, 0, 0)\"",
  ],
]);

const CI_WORKER = readFileSync(
  new URL("../../../.github/workflows/ci-worker.yml", import.meta.url),
  "utf8",
);

const directoryLinkType = process.platform === "win32" ? "junction" : "dir";

async function assertTreeContainsNoLinks(root) {
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.pop();
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      assert.equal(entry.isSymbolicLink(), false, `copied registry index contains a link: ${path}`);
      if (entry.isDirectory()) pending.push(path);
    }
  }
}

test("private Program mutation IDs bind six exact source transformations", () => {
  assert.deepEqual(
    PRIVATE_PROGRAM_MUTATION_CASES.map(
      ({ id, proof, artifact, sourcePath, search, replacement, expectedBrowserAssertion }) => [
        id,
        proof,
        artifact,
        sourcePath,
        createHash("sha256")
          .update(JSON.stringify([search, replacement]))
          .digest("hex"),
        expectedBrowserAssertion,
      ],
    ),
    EXPECTED_MUTATIONS,
  );
});

test("the required worker runs the packed-browser mutations after the baseline proof", () => {
  const baseline =
    'node scripts/test-private-program-browser.mjs "$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"';
  const mutations =
    'node scripts/test-private-program-mutations.mjs "$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"';
  const wasmPack =
    'wasm-pack test --headless --chrome --chromedriver "$CHROMEDRIVER_PATH" crates/labcolors-wasm --locked';

  const jobs = [...CI_WORKER.matchAll(/^  ([a-zA-Z0-9_-]+):\r?$/gmu)];
  const wasmJobIndex = jobs.findIndex((match) => match[1] === "wasm");
  assert.notEqual(wasmJobIndex, -1);
  const wasmJob = CI_WORKER.slice(
    jobs[wasmJobIndex].index,
    jobs[wasmJobIndex + 1]?.index ?? CI_WORKER.length,
  );
  assert.equal(wasmJob.split(baseline).length - 1, 1);
  assert.equal(wasmJob.split(mutations).length - 1, 1);
  assert.equal(wasmJob.split(wasmPack).length - 1, 1);
  assert.ok(wasmJob.indexOf(baseline) < wasmJob.indexOf(wasmPack));
  assert.ok(wasmJob.indexOf(wasmPack) < wasmJob.indexOf(mutations));
  for (const binding of [
    'LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000"',
    'LAB_COLORS_PRIVATE_MUTATION_CHILD_TIMEOUT_MS: "180000"',
    // The overall mutation deadline derives from the single wasm-job budget
    // source: 20 minutes * 60000 ms. A literal change without touching the
    // budget must fail this contract.
    'export LAB_COLORS_PRIVATE_MUTATION_TIMEOUT_MS="$((WASM_PRIVATE_MUTATION_BUDGET_MINUTES * 60000))"',
    "VERIFIED_TARBALL: ${{ steps.verified-release.outputs.tarball }}",
    "VERIFIED_TARBALL_SHA256: ${{ steps.verified-release-identity.outputs.sha256 }}",
  ]) {
    const mutationStep = wasmJob.slice(
      wasmJob.lastIndexOf("      - name:", wasmJob.indexOf(mutations)),
      wasmJob.indexOf("      - name:", wasmJob.indexOf(mutations)),
    );
    assert.match(mutationStep, new RegExp(binding.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  }
});

test("every production mutation anchor is present exactly once", async () => {
  const checked = await assertPrivateProgramMutationAnchors();
  assert.deepEqual(checked, EXPECTED_MUTATIONS.map(([id]) => id));
});

test("exact source mutation rejects missing, repeated, and no-op anchors", () => {
  const definition = Object.freeze({ id: "example", search: "edge", replacement: "cut" });
  assert.equal(applyExactMutation("left edge right", definition), "left cut right");
  const literalReplacement = "$&-$`-$'";
  assert.equal(
    applyExactMutation("left edge right", { ...definition, replacement: literalReplacement }),
    `left ${literalReplacement} right`,
  );
  assert.throws(() => applyExactMutation("left right", definition), /found 0/u);
  assert.throws(() => applyExactMutation("edge edge", definition), /found 2/u);
  assert.throws(
    () => applyExactMutation("edge", { ...definition, replacement: 1 }),
    /replacement must be strings/u,
  );
  assert.throws(
    () => applyExactMutation("edge", { ...definition, replacement: "edge" }),
    /no-op/u,
  );
});

test("isolated offline mutation builds copy the declared Cargo registry index", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-"));
  const declaredCargoHome = join(temporary, "declared-cargo");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(declaredCargoHome, "registry", "index", "registry.example"), {
      recursive: true,
    });
    await writeFile(
      join(declaredCargoHome, "registry", "index", "registry.example", "config.json"),
      "{}\n",
    );
    await copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome });
    assert.equal(
      readFileSync(
        join(isolatedCargoHome, "registry", "index", "registry.example", "config.json"),
        "utf8",
      ),
      "{}\n",
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("isolated Cargo index materializes a symlinked index root", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-root-link-"));
  const declaredCargoHome = join(temporary, "declared-cargo");
  const physicalIndex = join(temporary, "physical-index");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(declaredCargoHome, "registry"), { recursive: true });
    await mkdir(join(physicalIndex, "registry.example"), { recursive: true });
    await writeFile(join(physicalIndex, "registry.example", "config.json"), "{}\n");
    await symlink(physicalIndex, join(declaredCargoHome, "registry", "index"), directoryLinkType);

    await copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome });

    const copiedIndex = join(isolatedCargoHome, "registry", "index");
    assert.equal((await lstat(copiedIndex)).isSymbolicLink(), false);
    await assertTreeContainsNoLinks(copiedIndex);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("isolated Cargo index accepts a symlinked declared-home ancestor", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-ancestor-link-"));
  const physicalCargoHome = join(temporary, "physical-cargo");
  const declaredCargoHome = join(temporary, "declared-cargo-link");
  const sourceIndex = join(physicalCargoHome, "registry", "index");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(sourceIndex, "registry.example", "snapshot"), { recursive: true });
    await writeFile(join(sourceIndex, "registry.example", "snapshot", "config.json"), "{}\n");
    await symlink(
      join(sourceIndex, "registry.example", "snapshot"),
      join(sourceIndex, "registry.example", "current"),
      directoryLinkType,
    );
    await symlink(physicalCargoHome, declaredCargoHome, directoryLinkType);

    await copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome });

    assert.equal(
      readFileSync(
        join(isolatedCargoHome, "registry", "index", "registry.example", "current", "config.json"),
        "utf8",
      ),
      "{}\n",
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("isolated Cargo index materializes contained source links", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-target-links-"));
  const declaredCargoHome = join(temporary, "declared-cargo");
  const sourceIndex = join(declaredCargoHome, "registry", "index");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(sourceIndex, "registry.example", "snapshot"), { recursive: true });
    await writeFile(join(sourceIndex, "registry.example", "snapshot", "config.json"), "{}\n");
    await symlink(
      join(sourceIndex, "registry.example", "snapshot"),
      join(sourceIndex, "registry.example", "current"),
      directoryLinkType,
    );

    await copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome });

    await assertTreeContainsNoLinks(join(isolatedCargoHome, "registry", "index"));
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("isolated Cargo index rejects a source link escaping the declared index", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-escape-"));
  const declaredCargoHome = join(temporary, "declared-cargo");
  const sourceIndex = join(declaredCargoHome, "registry", "index");
  const outside = join(temporary, "outside-index");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(sourceIndex, "registry.example"), { recursive: true });
    await mkdir(outside, { recursive: true });
    await writeFile(join(outside, "config.json"), "{}\n");
    await symlink(outside, join(sourceIndex, "registry.example", "escape"), directoryLinkType);

    await assert.rejects(
      copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome }),
      /symlink resolves outside the declared Cargo index/u,
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("isolated Cargo index rejects a dangling source link", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-mutation-cargo-index-dangling-"));
  const declaredCargoHome = join(temporary, "declared-cargo");
  const sourceIndex = join(declaredCargoHome, "registry", "index");
  const isolatedCargoHome = join(temporary, "isolated-cargo");
  try {
    await mkdir(join(sourceIndex, "registry.example"), { recursive: true });
    await symlink(
      join(sourceIndex, "missing"),
      join(sourceIndex, "registry.example", "dangling"),
      directoryLinkType,
    );

    await assert.rejects(
      copyDeclaredCargoRegistryIndex(isolatedCargoHome, { declaredCargoHome }),
      /registry index symlink is dangling/u,
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("a mutation kill requires its real-browser assertion and semantic marker", () => {
  const definition = PRIVATE_PROGRAM_MUTATION_CASES[0];
  const killed = Object.freeze({
    code: 1,
    signal: null,
    stdout: "",
    stderr:
      "Error: private Program browser proof: browser assertion failed: " +
      "PrivateProgramConsumerError: private Program consumer: run failed with status 6",
  });
  assert.equal(assertMutationSpecificBrowserFailure(definition, killed), true);
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stderr: "Error: ChromeDriver did not become ready",
      }),
    /expected exactly one browser assertion boundary, found 0/u,
  );
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stderr:
          killed.stderr +
          "\nAggregateError: private Program browser proof or cleanup failed",
      }),
    /encountered browser cleanup failure/u,
  );
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stderr: "private Program browser proof: browser assertion failed: wrong failure",
      }),
    /mutation-specific failure evidence: expected=.*actual=/u,
  );
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stderr:
          "private Program browser proof: browser assertion failed: wrong failure\n" +
          "private Program run failed with status 6",
      }),
    /mutation-specific failure evidence/u,
  );
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stderr: `${killed.stderr}\n${killed.stderr}`,
      }),
    /expected exactly one browser assertion boundary, found 2/u,
  );
  assert.throws(
    () =>
      assertMutationSpecificBrowserFailure(definition, {
        ...killed,
        stdout: "LAB_COLORS_PRIVATE_PROGRAM_BROWSER_PASS v1 checks=7",
      }),
    /emitted the browser PASS receipt/u,
  );
  assert.throws(
    () => assertMutationSpecificBrowserFailure(definition, { ...killed, code: 0 }),
    /normal nonzero browser assertion/u,
  );
});

test("mutation time budgets are explicit and leave browser cleanup headroom", () => {
  assert.deepEqual(
    parseMutationTimeoutPolicy({
      LAB_COLORS_PRIVATE_MUTATION_TIMEOUT_MS: "1200000",
      LAB_COLORS_PRIVATE_MUTATION_CHILD_TIMEOUT_MS: "180000",
      LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000",
    }),
    {
      overallMilliseconds: 1200000,
      childMilliseconds: 180000,
      browserMilliseconds: 60000,
    },
  );
  assert.throws(
    () =>
      parseMutationTimeoutPolicy({
        LAB_COLORS_PRIVATE_MUTATION_TIMEOUT_MS: "900000",
        LAB_COLORS_PRIVATE_MUTATION_CHILD_TIMEOUT_MS: "60000",
        LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000",
      }),
    /must exceed LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS/u,
  );
  assert.throws(
    () =>
      parseMutationTimeoutPolicy({
        LAB_COLORS_PRIVATE_MUTATION_TIMEOUT_MS: "invalid",
        LAB_COLORS_PRIVATE_MUTATION_CHILD_TIMEOUT_MS: "180000",
        LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000",
      }),
    /must be a positive integer/u,
  );
});

test("filesystem admission rejects package reparse points and escaping source links", async () => {
  const root = await mkdtemp(join(tmpdir(), "labcolors-mutation-tree-admission-"));
  try {
    const packageRoot = join(root, "package");
    const sourceRoot = join(root, "source");
    const outsideRoot = join(root, "outside");
    await Promise.all([
      mkdir(packageRoot),
      mkdir(sourceRoot),
      mkdir(outsideRoot),
    ]);
    await writeFile(join(outsideRoot, "target.txt"), "outside\n", "utf8");
    const linkType = process.platform === "win32" ? "junction" : "dir";
    await symlink(outsideRoot, join(packageRoot, "external"), linkType);
    await symlink(outsideRoot, join(sourceRoot, "external"), linkType);

    await assert.rejects(
      assertAdmittedTree(packageRoot, packageRoot, {
        allowContainedFileSymlinks: false,
        label: "package fixture",
      }),
      /contains a symlink or reparse point/u,
    );
    await assert.rejects(
      assertAdmittedTree(sourceRoot, sourceRoot, {
        allowContainedFileSymlinks: true,
        label: "source fixture",
      }),
      /escapes its admitted physical boundary/u,
    );
    await assert.rejects(
      assertAdmittedRegularFile(
        join(sourceRoot, "external", "target.txt"),
        sourceRoot,
        "JavaScript source fixture",
      ),
      /path contains a symlink or reparse point/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("WASM preflight rejects an artifact before browser execution when its surface is absent", () => {
  const emptyValidModule = Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
  assert.throws(
    () => validateMutationWasm(emptyValidModule),
    /WASM import\/export surface differs from its exact private allowlist|must contain one defined memory section/u,
  );
});
