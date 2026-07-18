import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");

function command(name, args, cwd) {
  return execFileSync(name, args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

test("prepack source guard is clean-tree and exact-SHA executable evidence", async () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-prepack-source-"));
  const scripts = join(fixture, "scripts");
  mkdirSync(scripts);
  copyFileSync(
    join(root, "scripts", "prepare-npm-package.mjs"),
    join(scripts, "prepare-npm-package.mjs"),
  );
  copyFileSync(
    join(root, "scripts", "cargo-workspace.mjs"),
    join(scripts, "cargo-workspace.mjs"),
  );
  try {
    command("git", ["init", "--quiet"], fixture);
    command("git", ["config", "user.name", "Lab Colors release test"], fixture);
    command("git", ["config", "user.email", "release-test@example.invalid"], fixture);
    writeFileSync(join(fixture, "tracked.txt"), "canonical\n");
    command("git", ["add", "."], fixture);
    command("git", ["commit", "--quiet", "-m", "fixture"], fixture);

    const previousSha = process.env.GITHUB_SHA;
    try {
      delete process.env.GITHUB_SHA;
      const module = await import(pathToFileURL(join(scripts, "prepare-npm-package.mjs")));
      const head = command("git", ["rev-parse", "HEAD"], fixture);
      assert.equal(module.verifiedSourceSha(), head);

      process.env.GITHUB_SHA = head;
      assert.equal(module.verifiedSourceSha(), head);

      process.env.GITHUB_SHA = "0".repeat(40);
      assert.throws(
        () => module.verifiedSourceSha(),
        /does not equal checked-out HEAD/,
      );

      delete process.env.GITHUB_SHA;
      writeFileSync(join(fixture, "untracked.txt"), "must make the guard red\n");
      assert.throws(() => module.verifiedSourceSha(), /source is dirty/);
    } finally {
      if (previousSha === undefined) delete process.env.GITHUB_SHA;
      else process.env.GITHUB_SHA = previousSha;
    }
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("workspace version parser is bounded to the workspace.package table", async () => {
  const { workspaceVersion } = await import(
    pathToFileURL(join(root, "scripts", "cargo-workspace.mjs"))
  );
  assert.equal(
    workspaceVersion(`
[package]
version = "9.9.9"

[workspace.package]
edition = "2024"
version = "0.2.0"

[workspace.metadata.release]
version = "8.8.8"
`),
    "0.2.0",
  );
  assert.equal(
    workspaceVersion("[workspace.package]\r\nedition = \"2024\"\r\nversion = \"0.3.0\"\r\n"),
    "0.3.0",
  );
  assert.throws(
    () => workspaceVersion(`
[workspace.package]
edition = "2024"

[workspace.metadata.release]
version = "8.8.8"
`),
    /workspace core version is absent/u,
  );
  assert.throws(
    () => workspaceVersion(`
[workspace.package]
description = """
[not.a.table]
"""
version = "0.3.0"
`),
    /multiline TOML strings are unsupported/u,
  );
  assert.throws(
    () => workspaceVersion(`
[workspace.package]
description = '''
[not.a.table]
'''
version = "0.3.0"
`),
    /multiline TOML strings are unsupported/u,
  );
  assert.equal(
    workspaceVersion(`
# documentation mentions """ but does not open a TOML string
[workspace.package]
version = "0.3.0" # the retired parser rejected a comment containing '''
description = '"""'
`),
    "0.3.0",
  );
  assert.throws(
    () => workspaceVersion(`
[workspace.package]
edition = "2024"

[[example]]
version = "7.7.7"
`),
    /workspace core version is absent/u,
  );
});

test("release command wrapper terminates a child that hangs past its bound", async () => {
  const { command, RELEASE_COMMAND_TIMEOUT_MS } = await import(
    pathToFileURL(join(root, "scripts", "verify-package-release.mjs"))
  );
  assert.equal(RELEASE_COMMAND_TIMEOUT_MS, 5 * 60 * 1_000);
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-release-timeout-"));
  try {
    const marker = join(fixture, "started");
    const hangingFixture =
      `require("node:fs").writeFileSync(${JSON.stringify(marker)}, "started\\n");` +
      "setInterval(() => {}, 60_000);";
    const startedAt = Date.now();
    assert.throws(
      // Node's test runner executes files concurrently. Give the minimum Node
      // 22 consumer enough startup headroom under a saturated CI worker; the
      // marker still proves the child ran before the bounded timeout fired.
      () => command(process.execPath, ["-e", hangingFixture], fixture, { timeoutMs: 2_000 }),
      /timed out after 2000 ms/u,
    );
    assert.ok(existsSync(marker), "anti-vacuum: the hanging fixture never started");
    assert.ok(Date.now() - startedAt < 8_000, "bounded child exceeded the test deadline");
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("build metadata exact validator rejects one-field tampering", async () => {
  const { validateBuildMetadata } = await import(
    pathToFileURL(join(root, "scripts", "verify-package-release.mjs"))
  );
  const context = {
    packageJson: { name: "@labpics/colors", version: "0.10.0" },
    source: "1".repeat(40),
    coreVersion: "0.2.0",
    conformanceEvidence: {
      packVersion: "2.0.0",
      packDigest: "64a68cbd",
      manifestSha256: "2".repeat(64),
      familySetSha256: "3".repeat(64),
    },
    wasm: {
      runtime: {
        path: "pkg/labcolors_bg.wasm",
        bytes: 123,
        sha256: "4".repeat(64),
      },
    },
  };
  const metadata = {
    schemaVersion: 2,
    package: { ...context.packageJson },
    sourceSha: context.source,
    coreVersion: context.coreVersion,
    conformance: {
      packVersion: context.conformanceEvidence.packVersion,
      packDigest: context.conformanceEvidence.packDigest,
      manifestSha256: context.conformanceEvidence.manifestSha256,
      familySetSha256: context.conformanceEvidence.familySetSha256,
    },
    wasm: [{ role: "runtime", ...context.wasm.runtime }],
  };
  assert.doesNotThrow(() => validateBuildMetadata(metadata, context));

  const tampered = structuredClone(metadata);
  tampered.wasm[0].sha256 = "6".repeat(64);
  assert.throws(
    () => validateBuildMetadata(tampered, context),
    /does not exactly bind the release inputs/,
  );
});

test("clean-consumer smoke закрепляет коррелированный Glow/Material wire", () => {
  const verifier = readFileSync(
    join(root, "scripts", "verify-package-release.mjs"),
    "utf8",
  );
  for (const literal of [
    "layerRecipeProfile",
    "appearanceDiagnosticProfile",
    "selectionDiagnosticProfile",
    "exact-noop-unreachable",
    "legacy-reached",
    "legacy-unreachable",
    "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  ]) {
    assert.match(verifier, new RegExp(literal, "u"), `missing ${literal}`);
  }
  assert.doesNotMatch(verifier, /\bdiagnosticProfile\b/u);
  assert.doesNotMatch(verifier, /targetStatus === "(?:reached|unreachable)"/u);
  assert.doesNotMatch(verifier, /outward-interval-v1/u);
});
