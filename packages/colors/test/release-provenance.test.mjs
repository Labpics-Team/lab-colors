import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
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
    wasm: { bytes: 123, sha256: "4".repeat(64) },
  };
  const metadata = {
    schemaVersion: 1,
    package: { ...context.packageJson },
    sourceSha: context.source,
    coreVersion: context.coreVersion,
    conformance: {
      packVersion: context.conformanceEvidence.packVersion,
      packDigest: context.conformanceEvidence.packDigest,
      manifestSha256: context.conformanceEvidence.manifestSha256,
      familySetSha256: context.conformanceEvidence.familySetSha256,
    },
    wasm: { ...context.wasm },
  };
  assert.doesNotThrow(() => validateBuildMetadata(metadata, context));

  const tampered = structuredClone(metadata);
  tampered.wasm.sha256 = "5".repeat(64);
  assert.throws(
    () => validateBuildMetadata(tampered, context),
    /does not exactly bind the release inputs/,
  );
});
