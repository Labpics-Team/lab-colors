import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  linkSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, delimiter, dirname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  materializeVerifiedTarballSnapshot,
  npmInvocation,
  packInto,
  runtimeSmokeSource,
  smokePackedPackage,
  typeSmokeSource,
  validateNumericalEvidenceArtifacts,
  validatePrivateProgramMetadata,
  validateRuntimeWasmIsolation,
  validateSolveFailurePair,
  validateSolveFamily,
} from "../../../scripts/verify-package-release.mjs";
import {
  PRIVATE_PROGRAM_CANONICAL_BUILD,
  PRIVATE_PROGRAM_WASM_SURFACE,
  assertCanonicalCargoConfigurationAbsent,
  canonicalCargoConfigurationPaths,
  privateProgramCoreSourceDigest,
  validateCanonicalBuildEnvironment,
  validatePrivateProgramBuildReceipt,
  validatePrivateProgramWasmSurface,
} from "../../../scripts/build-private-program.mjs";
import { atomicWriteGeneratedFile } from "../../../scripts/atomic-write.mjs";
import { workspacePackageTable } from "../../../scripts/cargo-workspace.mjs";
import {
  NUMERICAL_EVIDENCE_FILES,
  PACKED_NUMERICAL_EVIDENCE_PATHS,
  POINT_SUPPORT_EVIDENCE_FILES,
  WCAG22_EVIDENCE_FILES,
} from "../../../scripts/release-evidence.mjs";
import { chromeArguments } from "./javascript-source-contract.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
// This subprocess performs one local ESM import; ten seconds bounds a deadlock
// while leaving two orders of magnitude over the measured sub-second path.
const DOM_FREE_PROBE_TIMEOUT_MS = 10_000;
// Each fixture is a sub-megabyte local archive; ten seconds distinguishes a
// parser deadlock from normal startup even on the supported Windows launcher.
const TAR_INSPECTOR_TEST_TIMEOUT_MS = 10_000;

test("npm invocation is argv-safe and directly executable on every supported host", () => {
  assert.deepEqual(
    npmInvocation({
      platform: "win32",
      node: "C:\\runtime\\node.exe",
      lifecycleEntrypoint: undefined,
      pathExists: () => true,
    }),
    {
      commandName: "C:\\runtime\\node.exe",
      argsPrefix: ["C:\\runtime\\node_modules\\npm\\bin\\npm-cli.js"],
    },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "linux",
      node: "/opt/node/bin/node",
      lifecycleEntrypoint: undefined,
      pathExists: () => false,
    }),
    { commandName: "npm", argsPrefix: [] },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "linux",
      node: "/opt/node/bin/node",
      lifecycleEntrypoint: "/opt/npm/lib/npm-cli.js",
      pathExists: () => true,
    }),
    {
      commandName: "/opt/node/bin/node",
      argsPrefix: ["/opt/npm/lib/npm-cli.js"],
    },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "linux",
      node: "/opt/node/bin/node",
      lifecycleEntrypoint: "/missing/npm/lib/npm-cli.js",
      pathExists: () => false,
    }),
    { commandName: "npm", argsPrefix: [] },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "win32",
      node: "C:\\runtime\\node.exe",
      lifecycleEntrypoint: "C:\\toolchain\\npm-cli.js",
      pathExists: () => true,
    }),
    {
      commandName: "C:\\runtime\\node.exe",
      argsPrefix: ["C:\\toolchain\\npm-cli.js"],
    },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "win32",
      node: "C:\\runtime\\node.exe",
      lifecycleEntrypoint: "C:\\missing\\npm-cli.js",
      pathExists: (path) => path === "C:\\runtime\\node_modules\\npm\\bin\\npm-cli.js",
    }),
    {
      commandName: "C:\\runtime\\node.exe",
      argsPrefix: ["C:\\runtime\\node_modules\\npm\\bin\\npm-cli.js"],
    },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "linux",
      node: "/opt/node/bin/node",
      lifecycleEntrypoint: "/opt/pnpm/bin/pnpm.cjs",
      pathExists: () => true,
    }),
    { commandName: "npm", argsPrefix: [] },
  );
  assert.deepEqual(
    npmInvocation({
      platform: "darwin",
      node: "/opt/node/bin/node",
      lifecycleEntrypoint: "/opt/pnpm/bin/pnpm.cjs",
      pathExists: () => false,
    }),
    { commandName: "npm", argsPrefix: [] },
  );
  assert.throws(
    () => npmInvocation({
      platform: "win32",
      node: "C:\\runtime\\node.exe",
      lifecycleEntrypoint: "C:\\toolchain\\yarn.js",
      pathExists: () => false,
    }),
    /npm CLI entrypoint is unavailable/u,
  );
});

function workflowNodeScript(workflow, stepName) {
  const runScript = workflowRunScript(workflow, stepName);
  const marker = "node <<'NODE'\n";
  const start = runScript.indexOf(marker);
  assert.ok(start >= 0, `node heredoc not found after: ${stepName}`);
  const bodyStart = start + marker.length;
  const end = runScript.indexOf("\nNODE", bodyStart);
  assert.ok(end >= 0, `node heredoc terminator not found after: ${stepName}`);
  return runScript.slice(bodyStart, end);
}

function workflowStepLines(workflow, stepName) {
  const lines = workflow.replaceAll("\r\n", "\n").split("\n");
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

function workflowRunScript(workflow, stepName) {
  const step = workflowStepLines(workflow, stepName);
  const runLines = step
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => line.trim() === "run: |");
  assert.equal(runLines.length, 1, `expected one run block in workflow step: ${stepName}`);
  const run = runLines[0];
  const runIndentation = run.line.length - run.line.trimStart().length;
  const body = [];
  for (let cursor = run.index + 1; cursor < step.length; cursor += 1) {
    const line = step[cursor];
    const indentation = line.length - line.trimStart().length;
    if (line.trim().length > 0 && indentation <= runIndentation) break;
    body.push(line.length >= runIndentation + 2 ? line.slice(runIndentation + 2) : "");
  }
  assert.ok(body.some((line) => line.length > 0), `empty run block: ${stepName}`);
  return body.join("\n");
}

function bashExecutable() {
  if (process.platform !== "win32") return "/bin/bash";
  const gitExecPath = execFileSync("git", ["--exec-path"], {
    encoding: "utf8",
  }).trim();
  const candidate = resolve(gitExecPath, "../../..", "bin", "bash.exe");
  assert.ok(existsSync(candidate), `Git Bash is unavailable: ${candidate}`);
  return candidate;
}

// Windows keeps the original case of inherited environment keys (usually
// `Path`); Node passes every key through, so a second `PATH` key would not
// reliably shadow the inherited one. Delete the path key case-insensitively
// before setting the controlled PATH so a fakeBin substitution is guaranteed.
function withControlledPath(environment, pathValue) {
  const controlled = { ...environment };
  for (const key of Object.keys(controlled)) {
    if (key.toLowerCase() === "path") delete controlled[key];
  }
  controlled.PATH = pathValue;
  return controlled;
}

// Windows without Developer Mode or SeCreateSymbolicLinkPrivilege rejects
// symlink creation with EPERM before any inspected behavior runs. Probe real
// symlink support once; symlink-specific scenarios skip only when the platform
// cannot create symlinks — the symlink is never replaced by a regular file.
let symlinkSupport = null;
function symlinksSupported() {
  if (symlinkSupport !== null) return symlinkSupport;
  const probe = mkdtempSync(join(tmpdir(), "labcolors-symlink-probe-"));
  try {
    const target = join(probe, "target");
    writeFileSync(target, "probe");
    symlinkSync(target, join(probe, "link"), "file");
    symlinkSupport = true;
  } catch (error) {
    if (error?.code === "EPERM" || error?.code === "EACCES" || error?.code === "ENOSYS") {
      symlinkSupport = false;
    } else {
      throw error;
    }
  } finally {
    rmSync(probe, { recursive: true, force: true });
  }
  return symlinkSupport;
}

function testPythonInvocation() {
  return process.platform === "win32"
    ? { commandName: "py", argsPrefix: ["-3"] }
    : { commandName: "python3", argsPrefix: [] };
}

function assertCheckoutCredentialsAreEphemeral(workflow, name) {
  const lines = workflow.split("\n");
  const checkouts = lines
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => line.trimStart().startsWith("- uses: actions/checkout@"));
  assert.ok(checkouts.length > 0, `${name} has no checkout steps`);

  for (const { line, index } of checkouts) {
    const indentation = line.length - line.trimStart().length;
    const step = [];
    for (let cursor = index + 1; cursor < lines.length; cursor++) {
      const candidate = lines[cursor];
      const candidateIndentation = candidate.length - candidate.trimStart().length;
      if (candidate.trim().length > 0 && candidateIndentation <= indentation) break;
      step.push(candidate);
    }
    assert.ok(
      step.some((candidate) => candidate.trim() === "persist-credentials: false"),
      `${name} checkout at line ${index + 1} persists the workflow token`,
    );
  }
}

function tomlString(table, key) {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const matches = [
    ...table.matchAll(
      new RegExp(`^[ \\t]*${escaped}[ \\t]*=[ \\t]*\"([^\"\\r\\n]+)\"[ \\t]*(?:#.*)?$`, "gmu"),
    ),
  ];
  assert.equal(matches.length, 1, `expected exactly one ${key} in [workspace.package]`);
  return matches[0][1];
}

function packageTable(source) {
  const lines = source.split(/\r?\n/u);
  const packageHeaders = lines
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => /^[ \t]*\[package\][ \t]*(?:#.*)?$/u.test(line));
  assert.equal(packageHeaders.length, 1, "expected exactly one [package] table");
  const start = packageHeaders[0].index + 1;
  const relativeEnd = lines.slice(start).findIndex((line) =>
    /^[ \t]*(?:\[[^\[\]\r\n]+\]|\[\[[^\[\]\r\n]+\]\])[ \t]*(?:#.*)?$/u.test(line)
  );
  const end = relativeEnd < 0 ? lines.length : start + relativeEnd;
  return lines.slice(start, end);
}

function assertWorkspaceReleaseMetadata(source) {
  const workspacePackage = workspacePackageTable(source);
  assert.equal(tomlString(workspacePackage, "version"), "0.3.0");
  assert.equal(tomlString(workspacePackage, "rust-version"), "1.85");
  assert.equal(
    tomlString(workspacePackage, "repository"),
    "https://github.com/Labpics-Team/lab-colors",
  );
}

test("breaking release metadata is one explicit 0.3.0/0.11.0 contract", () => {
  const workspace = read("Cargo.toml");
  assertWorkspaceReleaseMetadata(workspace);

  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const packageLock = JSON.parse(read("packages", "colors", "package-lock.json"));
  assert.equal(packageJson.version, "0.11.0");
  assert.equal(packageJson.packageManager, "npm@11.9.0");
  assert.equal(packageLock.version, "0.11.0");
  assert.equal(packageLock.packages[""].version, "0.11.0");
  assert.equal(packageJson.engines.node, ">=22.11.0");
  assert.equal(packageLock.packages[""].engines.node, ">=22.11.0");
  assert.equal(
    packageJson.scripts.prepack,
    "npm run build && node ../../scripts/prepare-npm-package.mjs",
  );
  assert.match(
    packageJson.scripts.build,
    /^wasm-pack build .* --locked && node \.\.\/\.\.\/scripts\/build-private-program\.mjs$/u,
  );
  assert.equal(
    packageJson.scripts.test,
    "node ../../scripts/ensure-private-program-artifact.mjs && node --test",
    "package tests must establish their ignored private Program artifact prerequisite explicitly",
  );
});

test("the private Program artifact is packed without becoming a public subpath", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  assert.deepEqual(
    packageJson.files.filter((path) => path.startsWith("private-program/")),
    [
      "private-program/consumer.js",
      "private-program/labcolors_private_program.wasm",
      "private-program/build-metadata.json",
    ],
  );
  assert.deepEqual(packageJson.exports, {
    ".": { types: "./index.d.ts", default: "./index.js" },
    "./apply-theme": { types: "./apply-theme.d.ts", default: "./apply-theme.js" },
    "./watch-theme": { types: "./watch-theme.d.ts", default: "./watch-theme.js" },
    "./adapt-theme": { types: "./adapt-theme.d.ts", default: "./adapt-theme.js" },
    "./pkg/labcolors_bg.wasm": "./pkg/labcolors_bg.wasm",
    "./build-metadata.json": "./build-metadata.json",
    "./package.json": "./package.json",
  });

  const build = read("scripts", "build-private-program.mjs");
  assert.match(
    packageJson.scripts.build,
    /wasm-pack build .* --locked\s*&&\s*node \.\.\/\.\.\/scripts\/build-private-program\.mjs$/u,
  );
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.args, [
    "rustc",
    "-p",
    "labcolors-core",
    "--lib",
    "--release",
    "--target",
    "wasm32-unknown-unknown",
    "--target-dir",
    "$ISOLATED_CARGO_TARGET_DIR",
    "--no-default-features",
    "--features",
    "private-fixture",
    "--crate-type=cdylib",
    "--locked",
    "--frozen",
    "--offline",
  ]);
  assert.equal(PRIVATE_PROGRAM_CANONICAL_BUILD.feature, "private-fixture");
  assert.equal(PRIVATE_PROGRAM_CANONICAL_BUILD.target, "wasm32-unknown-unknown");
  assert.equal(Object.hasOwn(PRIVATE_PROGRAM_CANONICAL_BUILD, "abi"), false);
  assert.deepEqual(
    PRIVATE_PROGRAM_CANONICAL_BUILD.wasmSurface,
    PRIVATE_PROGRAM_WASM_SURFACE,
  );
  assert.match(build, /mkdtemp/u, "each private build must own an isolated target directory");
  assert.equal(
    PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.command,
    "$RUSTUP_TOOLCHAIN_CARGO",
  );
  assert.equal(
    PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.rustc,
    "$RUSTUP_TOOLCHAIN_RUSTC",
  );
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.toolchain.executor, {
    resolution: "declared-rustup-home-exact-toolchain-directory",
    identity: "self-reported-version-and-commit",
  });
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.environment, {
    policy: "allowlist",
    cargoHome: "$ISOLATED_CARGO_HOME",
    registry: "isolated-index-copy-from-declared-cargo-home",
    temp: "$ISOLATED_TEMP_DIR",
    network: "offline",
  });
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.cargo.encodedRustflags, [
    "--cfg=labcolors_private_fixture_unshared_v1",
    "--check-cfg=cfg(labcolors_private_fixture_unshared_v1)",
    "--remap-path-prefix=$REPO_ROOT=/workspace/lab-colors",
    "--remap-path-prefix=$CARGO_HOME=/cargo-home",
    "--remap-path-prefix=$RUSTUP_HOME=/rustup-home",
    "--remap-path-prefix=$ISOLATED_CARGO_TARGET_DIR=/cargo-target",
    "--remap-path-prefix=$ISOLATED_TEMP_DIR=/build-temp",
  ]);
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.repeatability, {
    passes: 2,
    isolation: ["cargo-target", "cargo-home", "temp"],
    comparison: ["raw-wasm-byte-identical", "optimized-wasm-byte-identical"],
    executor: "same-resolved-toolchain-and-node-process",
  });
  assert.doesNotMatch(
    JSON.stringify(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.repeatability),
    /independent/iu,
  );
  assert.match(build, /BINARYEN_ROOT[\s\S]*BINARYEN_RELEASE[\s\S]*BINARYEN_NODE_SHA256/u);
  assert.equal(
    PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.optimizer.command,
    "$NODE_EXECUTABLE",
  );
  assert.equal(
    PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.optimizer.script,
    "$BINARYEN_ROOT/wasm-opt.js",
  );
  assert.equal(
    PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.optimizer.execution,
    "$ISOLATED_OPTIMIZER_ROOT/wasm-opt.js",
  );
  assert.deepEqual(PRIVATE_PROGRAM_CANONICAL_BUILD.recipe.optimizer.args, [
    "$RAW_WASM",
    "-o",
    "$OPTIMIZED_WASM",
    "-Oz",
    "--enable-bulk-memory",
    "--enable-nontrapping-float-to-int",
  ]);
  assert.match(build, /private-program\/\.build-receipt\.json/u);
  assert.match(build, /assertRepeatedBuildsEqual\(first, second\)/u);
  assert.match(
    build,
    /materializeOptimizer\([\s\S]*atomicWriteGeneratedFile\(destination, bytes\)/u,
    "Binaryen must execute only from a byte snapshot materialized in the build sandbox",
  );
  assert.match(
    build,
    /const version = commandText\(process\.execPath, \[script, "--version"\]/u,
    "the optimizer version must be probed from the materialized snapshot",
  );

  const prepare = read("scripts", "prepare-npm-package.mjs");
  const verifier = read("scripts", "verify-package-release.mjs");
  const ensure = read("scripts", "ensure-private-program-artifact.mjs");
  const canonicalBuild = read("scripts", "run-canonical-private-program-build.mjs");
  assert.match(
    ensure,
    /validatePrivateProgramBuildReceipt\(receipt, \{[^}]*source[^}]*wasm[^}]*requireOptimizer: true/u,
    "the test prerequisite must reuse only a canonically verified artifact",
  );
  assert.match(
    ensure,
    /runCanonicalPrivateProgramBuild\(\)/u,
    "the test prerequisite must delegate to the shared hermetic build primitive",
  );
  assert.match(
    canonicalBuild,
    /spawnSync\(/u,
    "the shared primitive must build in a hermetic child process",
  );
  assert.match(
    canonicalBuild,
    /--require-optimizer/u,
    "the child must request the same canonical optimized build the release gate produces",
  );
  assert.match(
    canonicalBuild,
    /isCanonicalBuildEnvOverride/u,
    "the child environment must drop exactly the canonical forbidden ambient overrides",
  );
  assert.match(
    canonicalBuild,
    /PRIVATE_PROGRAM_BUILD_TIMEOUT_MS/u,
    "the shared primitive must reuse the declared per-pass build budget",
  );
  assert.doesNotMatch(
    canonicalBuild,
    /CARGO_INCREMENTAL/u,
    "the hermetic boundary must not hardcode one ambient override variable",
  );
  assert.doesNotMatch(
    ensure,
    /spawnSync|hermeticBuildEnvironment|isCanonicalBuildEnvOverride/u,
    "the test prerequisite must not duplicate the child boundary implementation",
  );
  assert.match(
    ensure,
    /process\.exitCode = 1/u,
    "a failed prerequisite build must fail the test command, not skip the fixture",
  );
  assert.match(
    prepare,
    /PRIVATE_PROGRAM_METADATA_PATH[\s\S]*readPrivateProgramBuildReceipt/u,
  );
  assert.match(prepare, /requireOptimizer: true/u);
  assert.match(
    verifier,
    /const expectedSource = verifiedSourceSha\(\);[\s\S]*runCanonicalPrivateProgramBuild\(\);[\s\S]*await prepareNpmPackage\(\)[\s\S]*source !== expectedSource/u,
    "the release gate must bind one clean HEAD across the hermetic build and receipt",
  );
  assert.ok(
    verifier.includes('packageJson.types.replace(/^\\.\\//u, "")'),
    "the declared npm types spelling must be normalised like an export target",
  );
  assert.match(verifier, /ERR_PACKAGE_PATH_NOT_EXPORTED/u);
  const packageSmoke = smokePackedPackage.toString();
  assert.match(packageSmoke, /"--offline"[\s\S]*"--ignore-scripts"/u);
  assert.doesNotMatch(
    packageSmoke,
    /buildPrivateProgram|prepareNpmPackage|BINARYEN|cargo|rustc/u,
    "consumer-floor smoke must not rebuild or acquire a private build toolchain",
  );
  assert.match(
    verifier,
    /packageSmokeIndex\s*>=\s*0[\s\S]*smokePackedPackage\(tarball\)[\s\S]*:\s*verifyPackageRelease\(\)/u,
  );
  for (const path of [
    "private-program/consumer.js",
    "private-program/labcolors_private_program.wasm",
    "private-program/build-metadata.json",
  ]) {
    assert.match(verifier, new RegExp(path.replaceAll(".", "\\."), "u"));
  }
  assert.match(verifier, /schemaVersion: 5/u);
  assert.match(
    verifier,
    /wasm:\s*\[[\s\S]*role: "runtime"[\s\S]*role: PRIVATE_PROGRAM_ROLE[\s\S]*privateProgramArtifacts\.wasm/u,
  );
  assert.match(
    verifier,
    /privateProgramConsumer:\s*\{[\s\S]*role: PRIVATE_PROGRAM_ROLE[\s\S]*buildMetadata: privateProgramBuildMetadata[\s\S]*consumer: privateProgramArtifacts\.consumer/u,
  );
});

test("npm tarball inspection is independent of npm JSON and rejects ambiguous archives", async () => {
  const temporary = mkdtempSync(join(tmpdir(), "labcolors-tar-inspector-"));
  const inspector = join(root, "scripts", "inspect-npm-tarball.py");
  const { commandName, argsPrefix } = testPythonInvocation();
  const fixtureSource = String.raw`
import gzip
import io
import json
import sys
import tarfile

destination = sys.argv[1]
spec = json.loads(sys.argv[2])
if "rawOversizedSize" in spec:
    member = tarfile.TarInfo("package/oversized.bin")
    member.size = spec["rawOversizedSize"]
    payload = member.tobuf(format=tarfile.USTAR_FORMAT) + bytes(1024)
    with open(destination, "wb") as output:
        with gzip.GzipFile(fileobj=output, mode="wb", mtime=0) as archive:
            archive.write(payload)
else:
    archive_format = getattr(tarfile, spec.get("format", "USTAR_FORMAT"))
    with tarfile.open(destination, "w:gz", format=archive_format) as archive:
        for record in spec["members"]:
            member = tarfile.TarInfo(record["path"])
            kind = record.get("kind", "file")
            if kind == "file":
                content = record.get("content", "").encode("utf-8")
                member.size = len(content)
                archive.addfile(member, io.BytesIO(content))
            elif kind == "symlink":
                member.type = tarfile.SYMTYPE
                member.linkname = record.get("target", "target")
                archive.addfile(member)
            elif kind == "fifo":
                member.type = tarfile.FIFOTYPE
                archive.addfile(member)
            elif kind == "directory":
                member.type = tarfile.DIRTYPE
                archive.addfile(member)
            else:
                raise ValueError(f"unknown fixture kind: {kind}")
    with gzip.open(destination, "rb") as source:
        payload = source.read()
    while payload.endswith(bytes(512)):
        payload = payload[:-512]
    payload += bytes(1024)
    with open(destination, "wb") as output:
        with gzip.GzipFile(fileobj=output, mode="wb", mtime=0) as archive:
            archive.write(payload)
`;

  const writeFixture = (name, files, spec) => {
    const tarball = join(temporary, `${name}.tgz`);
    const inventory = join(temporary, `${name}.json`);
    writeFileSync(inventory, `${JSON.stringify({ schemaVersion: 1, files })}\n`);
    execFileSync(commandName, [...argsPrefix, "-c", fixtureSource, tarball, JSON.stringify(spec)], {
      encoding: "utf8",
      stdio: "pipe",
      timeout: TAR_INSPECTOR_TEST_TIMEOUT_MS,
    });
    return { tarball, inventory };
  };
  const inspect = ({ tarball, inventory }, inspectorPath = inspector) =>
    spawnSync(
      commandName,
      [
        ...argsPrefix,
        inspectorPath,
        "--tarball",
        tarball,
        "--declared-inventory-json",
        inventory,
      ],
      {
        encoding: "utf8",
        stdio: "pipe",
        timeout: TAR_INSPECTOR_TEST_TIMEOUT_MS,
      },
    );

  try {
    const valid = writeFixture("valid", ["a.txt", "nested/b.txt"], {
      members: [
        { path: "package/nested/b.txt", content: "beta" },
        { path: "package/a.txt", content: "alpha" },
      ],
    });
    const accepted = inspect(valid);
    assert.equal(accepted.status, 0, accepted.stderr);
    const receipt = JSON.parse(accepted.stdout);
    assert.deepEqual(Object.keys(receipt), [
      "schemaVersion",
      "verdict",
      "tarball",
      "limits",
      "members",
      "inventory",
    ]);
    assert.equal(receipt.schemaVersion, 1);
    assert.equal(receipt.verdict, "canonical");
    assert.deepEqual(
      receipt.members.map(({ index, rawPath, normalizedPath, type, size }) => ({
        index,
        rawPath,
        normalizedPath,
        type,
        size,
      })),
      [
        {
          index: 0,
          rawPath: "package/nested/b.txt",
          normalizedPath: "nested/b.txt",
          type: "file",
          size: 4,
        },
        {
          index: 1,
          rawPath: "package/a.txt",
          normalizedPath: "a.txt",
          type: "file",
          size: 5,
        },
      ],
    );
    assert.deepEqual(receipt.inventory, {
      files: ["a.txt", "nested/b.txt"],
      totalFileBytes: 9,
    });
    assert.equal(receipt.limits.maxMembers, 2);
    assert.equal(receipt.tarball.bytes, readFileSync(valid.tarball).length);
    assert.equal(
      receipt.tarball.sha256,
      createHash("sha256").update(readFileSync(valid.tarball)).digest("hex"),
    );
    assert.ok(receipt.members.every((member) => /^[0-9a-f]{64}$/u.test(member.sha256)));

    for (const [index, character] of [...`<>"|?*`].entries()) {
      const path = `forbidden${character}name.txt`;
      const result = inspect(
        writeFixture(`windows-forbidden-${index}`, [path], {
          members: [{ path: `package/${path}`, content: "forbidden" }],
        }),
      );
      assert.notEqual(result.status, 0, `Windows-forbidden ${JSON.stringify(character)} passed`);
      assert.match(result.stderr, /forbidden Windows filename character/u);
    }

    for (const [index, path] of [
      "CON",
      "PRN",
      "AUX",
      "NUL",
      "CONIN$",
      "conout$.json",
      "COM1.txt",
      "lPt9.bin",
      "COM¹",
      "com².txt",
      "CoM³.json",
      "LPT¹",
      "lpt².txt",
      "LpT³.json",
    ].entries()) {
      const result = inspect(
        writeFixture(`windows-device-${index}`, [path], {
          members: [{ path: `package/${path}`, content: "device" }],
        }),
      );
      assert.notEqual(result.status, 0, `reserved Windows device ${path} passed`);
      assert.match(result.stderr, /reserved Windows device name/u);
    }

    const alternateStreamFixture = writeFixture(
      "windows-alternate-stream",
      ["file:stream"],
      { members: [{ path: "package/file:stream", content: "stream" }] },
    );
    const alternateStream = inspect(alternateStreamFixture);
    assert.notEqual(alternateStream.status, 0, "Windows alternate-stream path passed");
    assert.match(alternateStream.stderr, /forbidden Windows filename character/u);

    const inspectorSource = readFileSync(inspector, "utf8");
    const colonMutant = inspectorSource.replace(
      /(_WINDOWS_FORBIDDEN_FILENAME_CHARACTERS = frozenset\('[^']*):([^']*'\))/u,
      "$1$2",
    );
    assert.notEqual(colonMutant, inspectorSource, "colon-removal mutation did not bite");
    const colonMutantPath = join(temporary, "inspect-npm-tarball-no-colon.py");
    writeFileSync(colonMutantPath, colonMutant);
    const colonMutantResult = inspect(alternateStreamFixture, colonMutantPath);
    assert.equal(
      colonMutantResult.status,
      0,
      `colon-removal mutant did not expose the ADS defect: ${colonMutantResult.stderr}`,
    );

    const nonDevices = ["COM0", "COM10", "LPT0", "LPT10"];
    const admittedDeviceBoundaries = inspect(
      writeFixture("windows-device-boundaries", nonDevices, {
        members: nonDevices.map((path) => ({ path: `package/${path}`, content: path })),
      }),
    );
    assert.equal(admittedDeviceBoundaries.status, 0, admittedDeviceBoundaries.stderr);
    assert.deepEqual(JSON.parse(admittedDeviceBoundaries.stdout).inventory.files, nonDevices);

    if (symlinksSupported()) {
      const linkedTarball = join(temporary, "linked-valid.tgz");
      symlinkSync(valid.tarball, linkedTarball, "file");
      const linkedResult = inspect({ tarball: linkedTarball, inventory: valid.inventory });
      assert.notEqual(linkedResult.status, 0, "tarball path symlink unexpectedly passed");
      assert.match(linkedResult.stderr, /symbolic link or reparse point/u);
    }

    const rejected = [
      {
        name: "duplicate",
        files: ["a.txt", "b.txt"],
        spec: {
          members: [
            { path: "package/a.txt", content: "first" },
            { path: "package/a.txt", content: "second" },
          ],
        },
        message: /duplicate raw member path/u,
      },
      {
        name: "traversal",
        files: ["escape.txt"],
        spec: { members: [{ path: "package/../escape.txt", content: "escape" }] },
        message: /traversal segment/u,
      },
      {
        name: "backslash",
        files: ["escape.txt"],
        spec: { members: [{ path: "package\\escape.txt", content: "escape" }] },
        message: /single package\/ namespace/u,
      },
      {
        name: "symlink",
        files: ["link"],
        spec: { members: [{ path: "package/link", kind: "symlink" }] },
        message: /only regular files are allowed/u,
      },
      {
        name: "fifo",
        files: ["pipe"],
        spec: { members: [{ path: "package/pipe", kind: "fifo" }] },
        message: /only regular files are allowed/u,
      },
      {
        name: "directory",
        files: ["directory"],
        spec: { members: [{ path: "package/directory", kind: "directory" }] },
        message: /only regular files are allowed/u,
      },
      {
        name: "case-folding",
        files: ["A.txt", "a.txt"],
        spec: {
          members: [
            { path: "package/A.txt", content: "upper" },
            { path: "package/a.txt", content: "lower" },
          ],
        },
        message: /portable case-folding path collisions/u,
      },
      {
        name: "pax",
        files: [`${"a".repeat(101)}.txt`],
        spec: {
          format: "PAX_FORMAT",
          members: [{ path: `package/${"a".repeat(101)}.txt`, content: "pax" }],
        },
        message: /only regular files are allowed/u,
      },
      {
        name: "oversized",
        files: ["oversized.bin"],
        spec: { rawOversizedSize: 64 * 1024 * 1024 + 1 },
        message: /regular-file bytes exceed/u,
      },
      {
        name: "undeclared",
        files: ["a.txt"],
        spec: { members: [{ path: "package/b.txt", content: "other" }] },
        message: /inventory differs from declaration/u,
      },
    ];
    assert.equal(rejected.length, 10, "anti-vacuum: hostile tar matrix shrank");
    for (const fixture of rejected) {
      const result = inspect(writeFixture(fixture.name, fixture.files, fixture.spec));
      assert.notEqual(result.status, 0, `${fixture.name} unexpectedly passed`);
      assert.match(result.stderr, fixture.message, fixture.name);
    }

    const trailing = writeFixture("trailing", ["a.txt"], {
      members: [{ path: "package/a.txt", content: "valid" }],
    });
    writeFileSync(
      trailing.tarball,
      Buffer.concat([readFileSync(trailing.tarball), Buffer.from("trailing")]),
    );
    const trailingResult = inspect(trailing);
    assert.notEqual(trailingResult.status, 0);
    assert.match(trailingResult.stderr, /concatenated member or trailing bytes/u);

    const packSource = packInto.toString();
    const liveInspectorCall =
      /const inspected = await inspectNpmTarball\(path, expected, packResult\);/u;
    assert.match(
      packSource,
      /validatePackedFiles\(packageJson, packResult\)[\s\S]*const inspected = await inspectNpmTarball\(path, expected, packResult\);/u,
      "npm JSON and the live raw-tar call must both gate packInto",
    );
    assert.match(packSource, liveInspectorCall);
    const inspectionBypass = packSource.replace(
      liveInspectorCall,
      "const inspected = { bytes: Buffer.alloc(0) };",
    );
    assert.notEqual(inspectionBypass, packSource, "inspector-call mutation did not bite");
    assert.doesNotMatch(
      inspectionBypass,
      liveInspectorCall,
      "removing the actual packInto inspection call must fail this contract",
    );

    const originalBytes = readFileSync(valid.tarball);
    const packResult = {
      size: originalBytes.length,
      unpackedSize: 9,
      shasum: createHash("sha1").update(originalBytes).digest("hex"),
      integrity: `sha512-${createHash("sha512").update(originalBytes).digest("base64")}`,
      files: [
        { path: "a.txt", size: 5 },
        { path: "nested/b.txt", size: 4 },
      ],
    };
    const snapshot = await materializeVerifiedTarballSnapshot({
      tarballName: basename(valid.tarball),
      expected: ["a.txt", "nested/b.txt"],
      packResult,
      bytes: originalBytes,
      sha256: createHash("sha256").update(originalBytes).digest("hex"),
    });
    try {
      assert.notEqual(resolve(snapshot.path), resolve(valid.tarball));
      assert.equal(basename(snapshot.path), `${snapshot.sha256}.tgz`);
      assert.equal(lstatSync(snapshot.path).isFile(), true);
      assert.equal(lstatSync(snapshot.path).isSymbolicLink(), false);
      assert.equal(lstatSync(snapshot.path).nlink, 1);
      if (process.platform !== "win32") {
        assert.equal(lstatSync(snapshot.path).mode & 0o777, 0o600);
      }
      writeFileSync(valid.tarball, "original path changed after validation");
      assert.ok(readFileSync(snapshot.path).equals(originalBytes));
      assert.equal(
        snapshot.sha256,
        createHash("sha256").update(readFileSync(snapshot.path)).digest("hex"),
      );
    } finally {
      rmSync(dirname(snapshot.path), { recursive: true, force: true });
    }

    const swappedPath = join(temporary, "path-swap.tgz");
    const replacementPath = join(temporary, "path-swap-replacement.tgz");
    writeFileSync(swappedPath, originalBytes);
    writeFileSync(replacementPath, originalBytes);
    const pathSwapProbe = String.raw`
import importlib.util
import os
import pathlib
import sys

module_path, original, replacement = sys.argv[1:]
spec = importlib.util.spec_from_file_location("labcolors_tar_inspector", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
real_open = module.os.open

def swap_before_open(candidate, flags, *args, **kwargs):
    if os.path.abspath(os.fspath(candidate)) == os.path.abspath(original):
        os.replace(replacement, original)
    return real_open(candidate, flags, *args, **kwargs)

module.os.open = swap_before_open
try:
    module._read_snapshot(pathlib.Path(original))
except module.InspectionError as error:
    if "changed between lstat and open" not in str(error):
        raise
else:
    raise AssertionError("tarball path swap unexpectedly passed")
`;
    execFileSync(
      commandName,
      [...argsPrefix, "-c", pathSwapProbe, inspector, swappedPath, replacementPath],
      {
        encoding: "utf8",
        stdio: "pipe",
        timeout: TAR_INSPECTOR_TEST_TIMEOUT_MS,
      },
    );

    // Declared-inventory snapshot symmetry: the JSON declaration must be read
    // with the same O_NOFOLLOW/lstat/fstat/samestat discipline as the tarball,
    // so a hostile inventory path cannot be swapped or redirected between
    // validation and use, and cannot exhaust memory before parsing.
    const inventoryValidTarball = writeFixture("inventory-valid", ["a.txt"], {
      members: [{ path: "package/a.txt", content: "alpha" }],
    });
    const inventoryBase = join(temporary, "inventory-hostile.json");
    writeFileSync(
      inventoryBase,
      `${JSON.stringify({ schemaVersion: 1, files: ["a.txt"] })}\n`,
    );

    if (symlinksSupported()) {
      const linkedInventory = join(temporary, "inventory-linked.json");
      symlinkSync(inventoryBase, linkedInventory, "file");
      const linkedInventoryResult = inspect({
        tarball: inventoryValidTarball.tarball,
        inventory: linkedInventory,
      });
      assert.notEqual(
        linkedInventoryResult.status,
        0,
        "declared inventory symlink unexpectedly passed",
      );
      assert.match(linkedInventoryResult.stderr, /symbolic link or reparse point/u);
    }

    const directoryInventory = join(temporary, "inventory-directory");
    mkdirSync(directoryInventory);
    const directoryResult = inspect({
      tarball: inventoryValidTarball.tarball,
      inventory: directoryInventory,
    });
    assert.notEqual(
      directoryResult.status,
      0,
      "directory declared inventory unexpectedly passed",
    );
    assert.match(directoryResult.stderr, /not a regular file/u);

    const inventorySwapReplacement = join(temporary, "inventory-swap-replacement.json");
    writeFileSync(
      inventorySwapReplacement,
      `${JSON.stringify({ schemaVersion: 1, files: ["a.txt"] })}\n`,
    );
    const inventorySwapProbe = String.raw`
import importlib.util
import os
import pathlib
import sys

module_path, original, replacement = sys.argv[1:]
spec = importlib.util.spec_from_file_location("labcolors_tar_inspector", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
real_open = module.os.open

def swap_before_open(candidate, flags, *args, **kwargs):
    if os.path.abspath(os.fspath(candidate)) == os.path.abspath(original):
        os.replace(replacement, original)
    return real_open(candidate, flags, *args, **kwargs)

module.os.open = swap_before_open
try:
    module._load_declared_inventory(pathlib.Path(original))
except module.InspectionError as error:
    if "changed between lstat and open" not in str(error):
        raise
else:
    raise AssertionError("declared inventory path swap unexpectedly passed")
`;
    execFileSync(
      commandName,
      [
        ...argsPrefix,
        "-c",
        inventorySwapProbe,
        inspector,
        inventoryBase,
        inventorySwapReplacement,
      ],
      {
        encoding: "utf8",
        stdio: "pipe",
        timeout: TAR_INSPECTOR_TEST_TIMEOUT_MS,
      },
    );

    const inventoryLimitProbe = String.raw`
import importlib.util
import sys

module_path = sys.argv[1]
spec = importlib.util.spec_from_file_location("labcolors_tar_inspector", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
print(module.MAX_DECLARED_INVENTORY_BYTES)
`;
    const inventoryLimit = Number(
      execFileSync(
        commandName,
        [...argsPrefix, "-c", inventoryLimitProbe, inspector],
        { encoding: "utf8", stdio: "pipe", timeout: TAR_INSPECTOR_TEST_TIMEOUT_MS },
      ).trim(),
    );
    assert.ok(
      Number.isSafeInteger(inventoryLimit) && inventoryLimit > 0,
      "the inspector must expose a positive declared-inventory byte ceiling",
    );

    const oversizeInventory = join(temporary, "inventory-oversize.json");
    writeFileSync(oversizeInventory, Buffer.alloc(inventoryLimit + 1, 0x61));
    const oversizeResult = inspect({
      tarball: inventoryValidTarball.tarball,
      inventory: oversizeInventory,
    });
    assert.notEqual(
      oversizeResult.status,
      0,
      "oversized declared inventory unexpectedly passed",
    );
    assert.match(oversizeResult.stderr, /declared inventory has .* bytes; limit is/u);

    const verifier = read("scripts", "verify-package-release.mjs");
    assert.match(
      verifier,
      /const verifiedTarball = await materializeVerifiedTarballSnapshot\(canonicalPack\)[\s\S]*path: `\.release\/\$\{basename\(verifiedTarball\.path\)\}`[\s\S]*return \{ manifest: RELEASE_MANIFEST, tarball: verifiedTarball\.path \};/u,
    );
    assert.doesNotMatch(
      verifier,
      /return \{ manifest: RELEASE_MANIFEST, tarball: canonicalPack\.path \};/u,
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("the canonical private Program build rejects ambient executor and Cargo config input", async () => {
  const allowed = {
    CARGO_HOME: "/declared/cargo-home",
    CARGO_TERM_COLOR: "always",
    RUSTUP_HOME: "/declared/rustup-home",
    RUST_TOOLCHAIN: "1.96.0",
  };
  assert.doesNotThrow(() => validateCanonicalBuildEnvironment(allowed));
  for (const name of [
    "CARGO",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_PROFILE_RELEASE_LTO",
    "CARGO_SOURCE_CRATES_IO_REPLACE_WITH",
    "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER",
    "NODE_OPTIONS",
    "NODE_PATH",
    "RUSTC",
    "RUSTC_BOOTSTRAP",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_WRAPPER",
    "RUSTFLAGS",
    "RUSTUP_TOOLCHAIN",
  ]) {
    assert.throws(
      () => validateCanonicalBuildEnvironment({ ...allowed, [name]: "hostile" }),
      /forbidden executor or build overrides/u,
      `${name} must not influence a canonical receipt`,
    );
  }
  assert.throws(
    () => validateCanonicalBuildEnvironment({ ...allowed, RUST_TOOLCHAIN: "stable" }),
    /RUST_TOOLCHAIN differs from the canonical pin/u,
  );

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-cargo-config-test-"));
  try {
    const repoRoot = join(temporary, "workspace", "repo");
    const cargoHome = join(temporary, "cargo-home");
    mkdirSync(repoRoot, { recursive: true });
    mkdirSync(cargoHome, { recursive: true });
    assert.ok(
      canonicalCargoConfigurationPaths({ repoRoot, cargoHome }).includes(
        join(temporary, "workspace", ".cargo", "config.toml"),
      ),
    );
    const ancestorCargo = join(temporary, "workspace", ".cargo");
    mkdirSync(ancestorCargo, { recursive: true });
    writeFileSync(join(ancestorCargo, "config.toml"), "[build]\nrustc-wrapper='hostile'\n");
    await assert.rejects(
      assertCanonicalCargoConfigurationAbsent({ repoRoot, cargoHome }),
      /canonical build rejects Cargo config files/u,
    );
    rmSync(ancestorCargo, { recursive: true, force: true });
    writeFileSync(join(cargoHome, "config"), "[build]\nrustflags=['hostile']\n");
    await assert.rejects(
      assertCanonicalCargoConfigurationAbsent({ repoRoot, cargoHome }),
      /canonical build rejects Cargo config files/u,
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("generated release writes cannot follow pre-planted temporary or destination symlinks", async () => {
  const atomicWriteSource = read("scripts", "atomic-write.mjs");
  const privateBuildSource = read("scripts", "build-private-program.mjs");
  const prepareSource = read("scripts", "prepare-npm-package.mjs");
  const verifySource = read("scripts", "verify-package-release.mjs");
  assert.match(
    atomicWriteSource,
    /mkdtemp\([\s\S]*open\(temporary, "wx"[\s\S]*handle\.sync\(\)[\s\S]*rename\(temporary, path\)/u,
  );
  assert.doesNotMatch(atomicWriteSource, /process\.pid/u);
  for (const source of [privateBuildSource, prepareSource, verifySource]) {
    assert.match(source, /atomicWriteGeneratedFile/u);
    assert.doesNotMatch(source, /\.tmp-\$\{process\.pid\}/u);
  }

  // The whole fixture is a symlink scenario: on platforms that cannot create
  // symlinks (Windows without Developer Mode) skip it instead of failing with
  // EPERM before the inspected atomic-write behavior runs.
  if (!symlinksSupported()) return;

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-atomic-write-test-"));
  try {
    const destination = join(temporary, "artifact.json");
    const victim = join(temporary, "victim.txt");
    const predictableTemporary = `${destination}.tmp-${process.pid}`;
    writeFileSync(victim, "victim");
    symlinkSync(victim, destination, "file");
    symlinkSync(victim, predictableTemporary, "file");

    await atomicWriteGeneratedFile(destination, "replacement");

    assert.equal(readFileSync(destination, "utf8"), "replacement");
    assert.equal(readFileSync(victim, "utf8"), "victim");
    assert.equal(lstatSync(destination).isSymbolicLink(), false);
    assert.equal(lstatSync(predictableTemporary).isSymbolicLink(), true);
    assert.deepEqual(
      readdirSync(temporary).filter((name) =>
        name.startsWith(`.${basename(destination)}.tmp-`),
      ),
      [],
      "exclusive temporary directories must be removed after the rename",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("private Program metadata binds the exact source, recipe, and packed bytes", () => {
  const context = {
    packageJson: { name: "@labpics/colors", version: "0.11.0" },
    source: "1".repeat(40),
    coreVersion: "0.3.0",
    privateProgramBuild: {
      source: {
        algorithm: "sha256",
        framing: "test-framing-v1",
        scope: ["crates/labcolors-core/src/**"],
        files: 1,
        sha256: "5".repeat(64),
      },
      build: PRIVATE_PROGRAM_CANONICAL_BUILD,
    },
    artifacts: {
      consumer: {
        path: "private-program/consumer.js",
        bytes: 13,
        sha256: "2".repeat(64),
      },
      wasm: {
        path: "private-program/labcolors_private_program.wasm",
        bytes: 17,
        sha256: "3".repeat(64),
      },
    },
  };
  const metadata = {
    schemaVersion: 1,
    role: "private-program-consumer",
    package: { ...context.packageJson },
    source: {
      gitSha: context.source,
      core: {
        crate: "labcolors-core",
        version: context.coreVersion,
        digest: structuredClone(context.privateProgramBuild.source),
      },
    },
    build: PRIVATE_PROGRAM_CANONICAL_BUILD,
    artifacts: structuredClone(context.artifacts),
  };

  assert.doesNotThrow(() => validatePrivateProgramMetadata(metadata, context));
  for (const [label, mutate] of [
    [
      "Core source",
      (value) => {
        value.source.core.digest.sha256 = "4".repeat(64);
      },
    ],
    [
      "build recipe",
      (value) => {
        value.build.feature = "stale-feature";
      },
    ],
    [
      "consumer",
      (value) => {
        value.artifacts.consumer.sha256 = "4".repeat(64);
      },
    ],
    [
      "WASM",
      (value) => {
        value.artifacts.wasm.sha256 = "4".repeat(64);
      },
    ],
    [
      "shape",
      (value) => {
        value.unbound = true;
      },
    ],
  ]) {
    const stale = structuredClone(metadata);
    mutate(stale);
    assert.throws(
      () => validatePrivateProgramMetadata(stale, context),
      /private Program metadata does not exactly bind its release inputs/u,
      `${label} drift must be rejected`,
    );
  }
});

test("the canonical private Program receipt rejects an unoptimized or stale artifact", () => {
  const wasm = Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
  const source = {
    algorithm: "sha256",
    framing: "test-framing-v1",
    scope: ["crates/labcolors-core/src/**"],
    files: 1,
    sha256: "6".repeat(64),
  };
  const receipt = {
    schemaVersion: 1,
    source,
    build: PRIVATE_PROGRAM_CANONICAL_BUILD,
    artifact: {
      path: "private-program/labcolors_private_program.wasm",
      bytes: wasm.length,
      sha256: createHash("sha256").update(wasm).digest("hex"),
    },
  };

  const rawReceipt = structuredClone(receipt);
  rawReceipt.build.toolchain.optimizer = null;
  rawReceipt.build.recipe.optimizer = null;
  assert.throws(
    () =>
      validatePrivateProgramBuildReceipt(rawReceipt, {
        source,
        wasm,
        requireOptimizer: true,
      }),
    /build receipt differs from its exact source, toolchain, recipe, or artifact/u,
  );

  assert.throws(
    () =>
      validatePrivateProgramBuildReceipt(receipt, {
        source: { ...source, sha256: "7".repeat(64) },
        wasm,
        requireOptimizer: true,
      }),
    /build receipt differs from its exact source, toolchain, recipe, or artifact/u,
  );
});

test("the public runtime WASM rejects every private Program symbol prefix", () => {
  assert.doesNotThrow(() => validateRuntimeWasmIsolation(Buffer.from("labcolors_runtime_v1")));
  assert.throws(
    () =>
      validateRuntimeWasmIsolation(
        Buffer.from("prefix:labcolors_private_fixture_run_v1:suffix"),
      ),
    /public runtime WASM contains private Program symbol prefix/u,
  );
});

test("the private Program WASM has one exact package-private import/export surface", () => {
  assert.deepEqual(PRIVATE_PROGRAM_WASM_SURFACE.imports, [
    {
      module: "labcolors_private_fixture_host_v1",
      name: "labcolors_private_fixture_host_confirm_disposed_v1",
      kind: "function",
    },
    {
      module: "labcolors_private_fixture_host_v1",
      name: "labcolors_private_fixture_host_install_v1",
      kind: "function",
    },
  ]);
  assert.deepEqual(
    PRIVATE_PROGRAM_WASM_SURFACE.exports.map(({ name, kind }) => `${name}:${kind}`),
    [
      "__data_end:global",
      "__heap_base:global",
      "labcolors_private_fixture_abort_dispose_v1:function",
      "labcolors_private_fixture_begin_dispose_v1:function",
      "labcolors_private_fixture_commit_dispose_v1:function",
      "labcolors_private_fixture_request_v1_len:function",
      "labcolors_private_fixture_request_v1_ptr:function",
      "labcolors_private_fixture_result_v1_len:function",
      "labcolors_private_fixture_result_v1_ptr:function",
      "labcolors_private_fixture_run_v1:function",
      "labcolors_private_fixture_update_v2:function",
      "labcolors_private_fixture_update_v2_len:function",
      "labcolors_private_fixture_update_v2_ptr:function",
      "memory:memory",
    ],
  );
  assert.throws(
    () =>
      validatePrivateProgramWasmSurface(
        Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]),
      ),
    /WASM import\/export surface differs from its exact private allowlist|must contain one defined memory/u,
  );
});

// WASM sections are located by parsing the header and each section's LEB128
// size, never by scanning for the memory-section byte pattern: the bytes
// `05 03 01 00` can legitimately occur inside data, code, or custom section
// payloads, and a raw scan would then mutate invalid WASM and fail for an
// unrelated reason.
function findMemorySectionOffset(wasm) {
  let offset = 8;
  while (offset < wasm.length) {
    const sectionStart = offset;
    const sectionId = wasm[offset++];
    let size = 0;
    let shift = 0;
    let byte;
    do {
      if (offset >= wasm.length) return -1;
      byte = wasm[offset++];
      size |= (byte & 0x7f) << shift;
      shift += 7;
    } while (byte & 0x80);
    if (sectionId === 5) return sectionStart;
    offset += size;
  }
  return -1;
}

test("the private Program WASM memory section is found by parsing, not by byte pattern", () => {
  // A custom section payload containing the decoy sequence `05 03 01 00`
  // precedes the real memory section; the raw byte scan would stop at the
  // decoy and mutate invalid WASM, while section parsing lands on id 5.
  const wasm = Buffer.from([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    0x00, 0x08, 0x03, 0x64, 0x65, 0x63, 0x05, 0x03, 0x01, 0x00, // custom "dec" + decoy
    0x05, 0x03, 0x01, 0x00, 0x01, // real memory section (count 1, flags 0, min 1)
  ]);
  assert.equal(findMemorySectionOffset(wasm), 18);
  assert.equal(findMemorySectionOffset(wasm.subarray(0, 18)), -1);
});

test("the private Program WASM rejects a shared-memory section mutant", () => {
  const wasm = readFileSync(
    join(root, "packages", "colors", "private-program", "labcolors_private_program.wasm"),
  );
  const sectionOffset = findMemorySectionOffset(wasm);
  assert.notEqual(sectionOffset, -1, "canonical private WASM must expose a compact memory section");
  const mutant = Buffer.concat([
    wasm.subarray(0, sectionOffset + 1),
    Buffer.from([4, 1, 3, wasm[sectionOffset + 4], wasm[sectionOffset + 4]]),
    wasm.subarray(sectionOffset + 5),
  ]);
  assert.throws(
    () => validatePrivateProgramWasmSurface(mutant),
    /memory must be non-shared/u,
    "shared memory must be rejected by the binary admission contract",
  );
});

test("workspace release metadata cannot be rescued by a later TOML table", () => {
  const expected = {
    version: "0.3.0",
    "rust-version": "1.85",
    repository: "https://github.com/Labpics-Team/lab-colors",
  };
  for (const poisoned of Object.keys(expected)) {
    const actual = { ...expected, [poisoned]: "wrong" };
    assert.throws(() => assertWorkspaceReleaseMetadata(`
[workspace.package]
version = "${actual.version}"
rust-version = "${actual["rust-version"]}"
repository = "${actual.repository}"

[workspace.metadata.release]
version = "0.3.0"
rust-version = "1.85"
repository = "https://github.com/Labpics-Team/lab-colors"
`), `later table rescued poisoned ${poisoned}`);
  }
});

test("every workspace package inherits the declared MSRV", () => {
  const manifests = readdirSync(join(root, "crates"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join("crates", entry.name, "Cargo.toml"));
  assert.ok(manifests.length > 1, "anti-vacuum: workspace package list is non-trivial");
  for (const manifest of manifests) {
    assert.match(
      read(manifest),
      /^rust-version\.workspace = true$/m,
      `${manifest} не публикует/не наследует workspace MSRV`,
    );
  }
});

test("consumers resolve the base Core graph without deleted capabilities", () => {
  const isolatedCoreEdge =
    /labcolors-core = \{ path = "\.\.\/labcolors-core", default-features = false \}/u;
  const wasmManifest = read("crates", "labcolors-wasm", "Cargo.toml");
  const ffiManifest = read("crates", "labcolors-ffi", "Cargo.toml");
  const conformanceManifest = read("crates", "labcolors-conformance", "Cargo.toml");

  // C4c: offline protocol/compiler линия вырезана целиком — самих крейтов нет.
  for (const erased of ["labcolors-protocol", "labcolors-compiler"]) {
    assert.ok(
      !existsSync(join(root, "crates", erased)),
      `${erased} must stay deleted, not resurrected`,
    );
  }
  for (const [name, manifest] of [
    ["labcolors-wasm", wasmManifest],
    ["labcolors-ffi", ffiManifest],
    ["labcolors-conformance", conformanceManifest],
  ]) {
    assert.match(manifest, isolatedCoreEdge, `${name} must keep the isolated Core edge`);
    assert.doesNotMatch(manifest, /labcolors-protocol/u, name);
    assert.doesNotMatch(manifest, /wcag22-feasibility|wcag22-explicit/u, name);
  }
  const coreManifest = read("crates", "labcolors-core", "Cargo.toml");
  assert.match(coreManifest, /^default = \[\]$/mu, "Core default capability set must stay empty");
  assert.doesNotMatch(coreManifest, /wcag22-feasibility|wcag22-explicit/u);

  const ci = read(".github", "workflows", "ci-worker.yml");
  const projection = workflowRunScript(
    ci,
    "name: prove core capability projection boundary",
  );
  const declaredDirectCore = projection.match(
    /direct_core_consumers = \(\n(?<items>(?:    "[^"]+",\n)+)\)/u,
  )?.groups?.items;
  assert.ok(declaredDirectCore, "CI must declare one direct-Core consumer SSOT");
  assert.deepEqual(
    [...declaredDirectCore.matchAll(/"([^"]+)"/gu)].map((match) => match[1]),
    ["labcolors-wasm", "labcolors-ffi", "labcolors-conformance"],
  );
  assert.equal(
    projection.match(/for consumer in direct_core_consumers:/gu)?.length,
    1,
  );
  assert.match(
    projection,
    /core\["features"\]\.get\("default"\) != \[\]:/u,
    "projection must pin the empty Core default capability set",
  );
  assert.match(
    projection,
    /for erased in \("labcolors-protocol", "labcolors-compiler"\):/u,
    "projection must reject resurrected offline crates",
  );
  assert.match(projection, /core_dependency\["features"\]/u);
  assert.match(
    projection,
    /"cargo", "tree", "-p", "labcolors-wasm",[\s\S]*?"--target", "wasm32-unknown-unknown"/u,
  );
  for (const forbidden of ["wcag22-feasibility", "wcag22-explicit"]) {
    assert.ok(
      projection.includes(`"${forbidden}"`),
      `projection must forbid deleted capability ${forbidden}`,
    );
  }
  assert.doesNotMatch(
    projection,
    /wcag22-explicit-selection|protocol_consumers/u,
    "deleted projection laws must not reappear",
  );
  assert.doesNotMatch(projection, /for consumer in labcolors-/u);
});

test("MSRV and packaged Rust crate gates are executable CI contracts", () => {
  assert.ok(existsSync(join(root, "LICENSE")), "root LICENSE отсутствует");
  const cargoMetadata = JSON.parse(execFileSync(process.env.CARGO ?? "cargo", [
    "metadata",
    "--no-deps",
    "--format-version",
    "1",
    "--locked",
  ], { cwd: root, encoding: "utf8" }));
  const workspaceMembers = new Set(cargoMetadata.workspace_members);
  const publishableCargoRoots = cargoMetadata.packages
    .filter((crate) => workspaceMembers.has(crate.id))
    .filter((crate) => crate.publish === null || crate.publish.length > 0)
    .map((crate) => dirname(crate.manifest_path));
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const wasmPackInvocationCount = packageJson.scripts.build.match(/\bwasm-pack\s+build\b/gu)?.length ?? 0;
  const wasmPackCommands = packageJson.scripts.build
    .split(/\s*&&\s*/u)
    .filter((command) => /\bwasm-pack\s+build\b/u.test(command));
  assert.equal(
    wasmPackCommands.length,
    wasmPackInvocationCount,
    "every wasm-pack build invocation must be one independently parsed command",
  );
  const wasmPackRoots = wasmPackCommands.map((command) => {
    const match = command.match(
      /\bwasm-pack\s+build\s+(?<root>"[^"]+"|'[^']+'|[^\s]+)/u,
    );
    assert.ok(match, `cannot parse wasm-pack crate root from: ${command}`);
    const token = match.groups.root;
    const cratePath = token.startsWith('"') || token.startsWith("'")
      ? token.slice(1, -1)
      : token;
    assert.ok(!cratePath.startsWith("-"), `wasm-pack crate root must precede flags: ${command}`);
    const crateRoot = resolve(root, "packages", "colors", cratePath);
    assert.ok(
      existsSync(join(crateRoot, "Cargo.toml")),
      `wasm-pack crate root has no Cargo.toml: ${crateRoot}`,
    );
    return crateRoot;
  });
  assert.ok(publishableCargoRoots.length > 0, "anti-vacuum: no publishable Cargo roots");
  assert.ok(wasmPackRoots.length > 0, "anti-vacuum: no wasm-pack build roots");

  const distributableRoots = [...new Set([
    ...publishableCargoRoots,
    ...wasmPackRoots,
  ])].sort();
  const coreRoot = resolve(root, "crates", "labcolors-core");
  const coreReceipt = JSON.parse(read(
    "crates",
    "labcolors-core",
    "contracts",
    "clean-set-srgb8-v1",
    "receipt-v1.json",
  ));
  const coreSpdx = coreReceipt.license_scope?.core_package_spdx;
  assert.equal(typeof coreSpdx, "string", "clean-set receipt must own Core SPDX");
  const workspaceSpdx = tomlString(
    workspacePackageTable(read("Cargo.toml")),
    "license",
  );
  const packageMetadataByRoot = new Map(
    cargoMetadata.packages.map((crate) => [dirname(crate.manifest_path), crate]),
  );
  assert.ok(distributableRoots.includes(coreRoot), "anti-vacuum: Core is distributable");
  assert.deepEqual(
    distributableRoots
      .filter((crateRoot) => !existsSync(join(crateRoot, "LICENSE")))
      .map((crateRoot) => relative(root, crateRoot)),
    [],
    "every distributable crate root must expose the canonical LICENSE",
  );
  for (const crateRoot of distributableRoots) {
    const license = join(crateRoot, "LICENSE");
    assert.ok(lstatSync(license).isSymbolicLink(), `${license} must preserve the root SSOT`);
    const canonicalTarget = relative(crateRoot, join(root, "LICENSE")).replaceAll("\\", "/");
    assert.equal(readlinkSync(license).replaceAll("\\", "/"), canonicalTarget);
    assert.equal(readFileSync(license, "utf8"), read("LICENSE"));
    const manifest = readFileSync(join(crateRoot, "Cargo.toml"), "utf8");
    const licenseDeclarations = packageTable(manifest)
      .filter((line) => /^license(?:\.workspace)?\s*=/u.test(line));
    const packageMetadata = packageMetadataByRoot.get(crateRoot);
    assert.ok(packageMetadata, `cargo metadata omitted ${crateRoot}`);
    assert.equal(packageMetadata.license_file, null, `${crateRoot} must use SPDX only`);
    if (crateRoot === coreRoot) {
      assert.deepEqual(licenseDeclarations, [`license = "${coreSpdx}"`]);
      assert.equal(packageMetadata.license, coreSpdx);
    } else {
      assert.deepEqual(licenseDeclarations, ["license.workspace = true"]);
      assert.equal(packageMetadata.license, workspaceSpdx);
    }
    assert.doesNotMatch(manifest, /^[ \t]*license-file\s*=/mu);
  }
  const coreManifest = read("crates", "labcolors-core", "Cargo.toml");
  const coreLib = read("crates", "labcolors-core", "src", "lib.rs");
  assert.match(coreManifest, /^description = "[^"]+"$/m);
  assert.match(coreManifest, /^readme = "README\.md"$/m);
  assert.ok(
    existsSync(join(root, "crates", "labcolors-core", "README.md")),
    "published core crate needs a package-local README",
  );
  assert.match(coreLib, /include_str!\("\.\.\/README\.md"\)/);
  assert.doesNotMatch(coreLib, /include_str!\("\.\.\/\.\.\/\.\.\/README\.md"\)/);

  const ci = read(".github", "workflows", "ci-worker.yml");
  assert.match(ci, /^\s*MSRV_TOOLCHAIN: 1\.85\.0$/m);
  assert.match(ci, /^\s*NODE_TOOLCHAIN: 24\.14\.0$/m);
  assert.match(ci, /^\s*NODE_CONSUMER_FLOOR: 22\.11\.0$/m);
  assert.match(
    ci,
    /^\s*node-consumer-floor:[\s\S]*needs: wasm[\s\S]*node-version: \$\{\{ env\.NODE_CONSUMER_FLOOR \}\}[\s\S]*actions\/download-artifact@[0-9a-f]{40}[\s\S]*--package-smoke/m,
  );
  assert.match(ci, /^\s*CHROME_FOR_TESTING_VERSION: 150\.0\.7871\.115$/m);
  assert.match(
    ci,
    /^\s*CHROME_FOR_TESTING_SHA256: 1be2db033133c5e2dd1a4e8664bf67b19a61bcf6ed28d2b00f433b3f0b4f9585$/m,
  );
  assert.match(
    ci,
    /^\s*CHROMEDRIVER_FOR_TESTING_SHA256: 6ac3919edd107ca13d08cccc118dc83821877e504014233f171bbd94cb01a80e$/m,
  );
  assert.match(
    ci,
    /actions\/setup-node@[0-9a-f]{40}[\s\S]*node-version: \$\{\{ env\.NODE_TOOLCHAIN \}\}/,
  );
  assert.match(ci, /^\s*msrv:$/m);
  assert.match(ci, /cargo check --workspace --all-targets --locked/);
  assert.match(ci, /cargo package -p labcolors-core --locked/);
  const corePackageStepName =
    "name: package labcolors-core and run extracted package doctests";
  const assertCorePackageGate = (workflow) => {
    const step = workflowStepLines(workflow, corePackageStepName);
    assert.deepEqual(
      step.filter((line) => /^(?:if|continue-on-error):/u.test(line.trim())),
      [],
      "Core package verification step cannot be disabled or made non-blocking",
    );
    const lines = workflowRunScript(workflow, corePackageStepName).split(/\r?\n/u);
    assert.equal(
      lines[0],
      "set -euo pipefail",
      "Core package verification must start in fail-closed shell mode",
    );
    assert.deepEqual(
      lines.filter((line) => /^\s*set(?:\s|$)/u.test(line)),
      ["set -euo pipefail"],
      "Core package verification cannot disable fail-fast after its prologue",
    );
    assert.ok(lines.includes("test -L crates/labcolors-core/LICENSE"));
    assert.ok(lines.includes("cmp LICENSE crates/labcolors-core/LICENSE"));
    const extract = lines.indexOf(
      'tar -xzf "target/package/labcolors-core-${crate_version}.crate" -C "$package_root"',
    );
    const shellContinuation = "\\";
    const verifierCommand = [
      `python3 scripts/verify_clean_set_receipt.py core-package ${shellContinuation}`,
      `  --source-root "$GITHUB_WORKSPACE" ${shellContinuation}`,
      '  --package-root "$crate_dir"',
    ];
    const verify = lines.indexOf(verifierCommand[0]);
    assert.deepEqual(lines.slice(verify, verify + 3), verifierCommand);
    const doctest = lines.indexOf(
      'cargo test --doc --manifest-path "$crate_dir/Cargo.toml" --locked',
    );
    assert.ok(
      extract >= 0 && extract < verify && verify < doctest,
      "extracted Core package must be verified before its doctests",
    );
  };
  assertCorePackageGate(ci);
  assertCorePackageGate(ci.replaceAll("\n", "\r\n"));
  const stepLine = `      - ${corePackageStepName}`;
  for (const bypass of ["if: false", "continue-on-error: true"]) {
    const mutated = ci.replace(stepLine, `${stepLine}\n        ${bypass}`);
    assert.notEqual(mutated, ci, `workflow mutation must insert ${bypass}`);
    assert.throws(() => assertCorePackageGate(mutated));
  }
  const failOpenShell = ci.replace("          set -euo pipefail", "          set +e");
  assert.notEqual(failOpenShell, ci, "workflow mutation must disable shell fail-fast");
  assert.throws(() => assertCorePackageGate(failOpenShell));
  const verifierLine =
    "          python3 scripts/verify_clean_set_receipt.py core-package \\";
  const commentedVerifier = ci.replace(verifierLine, `          # ${verifierLine.trim()}`);
  assert.notEqual(commentedVerifier, ci, "workflow mutation must comment the verifier");
  assert.throws(() => assertCorePackageGate(commentedVerifier));
  assert.match(ci, /id: verified-release[\s\S]*npm run release:verify/);
  assert.match(ci, /actions\/upload-artifact@[0-9a-f]{40}[\s\S]*steps\.verified-release\.outputs\.tarball/);
  assert.match(ci, /steps\.verified-release\.outputs\.manifest/);
  assert.match(
    ci,
    /name: colors-release-\$\{\{ github\.sha \}\}-attempt-\$\{\{ github\.run_attempt \}\}/,
  );
  assert.match(ci, /^\s*include-hidden-files: true$/m);

  const verified = ci.indexOf("id: verified-release");
  const uploaded = ci.indexOf("name: upload verified npm tarball and manifest");
  const browserInstall = ci.indexOf("name: install Chrome + dependencies");
  const browserGate = ci.indexOf("name: wasm-pack test (headless chrome)");
  assert.ok(
    verified < uploaded && uploaded < browserInstall && browserInstall < browserGate,
    "verified artifact must be uploaded before the pinned Chrome dependency executes",
  );
  assert.match(ci, /WASM_PACK_CACHE=\$RUNNER_TEMP\/wasm-pack-\$GITHUB_JOB/);
  assert.match(
    ci,
    /mkdir -p "\$RUNNER_TEMP\/tmp-\$GITHUB_JOB" "\$RUNNER_TEMP\/wasm-pack-\$GITHUB_JOB"/,
  );
  assert.doesNotMatch(ci, /chromedriver-bb6facf4ea9511f6|Pre-seeded wasm-pack/);
  assert.match(ci, /CHROME_ROOT="\$RUNNER_TEMP\/chrome-\$GITHUB_JOB"/);
  assert.match(ci, /DEPS_DIR="\$RUNNER_TEMP\/chrome-deps-\$GITHUB_JOB"/);
  assert.match(ci, /APT_LISTS="\$DEPS_DIR\/apt-lists"/);
  assert.match(ci, /APT_CACHE="\$DEPS_DIR\/apt-cache"/);
  const chromeInstallStep = "name: install Chrome + dependencies (Chrome for Testing + apt-get download)";
  const assertAptSourceIsolation = (workflow) => {
    const active = workflowRunScript(workflow, chromeInstallStep)
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("#"))
      .join("\n");
    const activeLines = active.split("\n").map((line) => line.trim());
    assert.equal(
      activeLines[0],
      "set -euo pipefail",
      "Chrome dependency install must start in fail-closed shell mode",
    );
    assert.deepEqual(
      activeLines.filter((line) => /\bset\b/u.test(line)),
      ["set -euo pipefail"],
      "Chrome dependency install cannot weaken fail-closed shell mode",
    );
    assert.equal(
      active.match(/^readonly APT_SOURCES="\$DEPS_DIR\/apt-sources"$/gmu)?.length,
      1,
      "Chrome APT source root must have one active job-local authority",
    );
    assert.equal(
      active.match(/\bAPT_SOURCES(?:\[[^\]\n]*\])?\s*\+?=/gu)?.length,
      1,
      "Chrome APT source authority must be assigned exactly once",
    );
    assert.doesNotMatch(active, /\bunset\b[^\n;]*\bAPT_SOURCES\b/gu);
    const optionArrays = [...active.matchAll(
      /^[ \t]*APT_OPTIONS=\(\n(?<body>(?:[ \t]+[^\n]*\n)+)^[ \t]*\)$/gmu,
    )];
    assert.equal(optionArrays.length, 1, "Chrome step must have one active APT_OPTIONS array");
    assert.equal(
      active.match(/\bAPT_OPTIONS(?:\[[^\]\n]*\])?\s*\+?=/gu)?.length,
      1,
      "Chrome APT options must be assigned exactly once",
    );
    assert.equal(active.match(/^readonly APT_OPTIONS$/gmu)?.length, 1);
    assert.doesNotMatch(active, /\bunset\b[^\n;]*\bAPT_OPTIONS\b/gu);
    const optionBody = optionArrays[0].groups?.body ?? "";
    for (const option of [
      '-o "Dir::Etc::sourcelist=$APT_SOURCES/sources.list"',
      '-o "Dir::Etc::sourceparts=$APT_SOURCES/sources.list.d"',
    ]) {
      assert.equal(
        optionBody.split("\n").filter((line) => line.trim() === option).length,
        1,
        `missing active isolated-source option: ${option}`,
      );
    }
    assert.match(active, /^: "\$\{ID:\?missing distro ID\}"$/mu);
    assert.match(active, /^: "\$\{VERSION_CODENAME:\?missing distro codename\}"$/mu);
    assert.match(active, /^case "\$ID:\$VERSION_CODENAME" in$/mu);
    assert.match(
      active,
      /^\s*debian:bookworm\|ubuntu:jammy\)\n\s+ALSA_PACKAGE=libasound2\n\s+ATK_PACKAGE=libatk1\.0-0\n\s+ATK_BRIDGE_PACKAGE=libatk-bridge2\.0-0\n\s+ATSPI_PACKAGE=libatspi2\.0-0\n\s+CUPS_PACKAGE=libcups2\n\s+GLIB_PACKAGE=libglib2\.0-0\n\s+;;$/mu,
    );
    assert.match(
      active,
      /^\s*debian:trixie\|ubuntu:noble\)\n\s+ALSA_PACKAGE=libasound2t64\n\s+ATK_PACKAGE=libatk1\.0-0t64\n\s+ATK_BRIDGE_PACKAGE=libatk-bridge2\.0-0t64\n\s+ATSPI_PACKAGE=libatspi2\.0-0t64\n\s+CUPS_PACKAGE=libcups2t64\n\s+GLIB_PACKAGE=libglib2\.0-0t64\n\s+;;$/mu,
    );
    assert.match(
      active,
      /^\s*\*\)\n\s+echo "unsupported Chrome dependency release: \$ID:\$VERSION_CODENAME" >&2\n\s+exit 1\n\s+;;$/mu,
    );
    assert.ok(activeLines.includes(
      "readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE",
    ));
    assert.deepEqual(
      activeLines.filter((line) => /^(?:ALSA|ATK|ATK_BRIDGE|ATSPI|CUPS|GLIB)_PACKAGE=/u.test(line)),
      [
        "ALSA_PACKAGE=libasound2",
        "ATK_PACKAGE=libatk1.0-0",
        "ATK_BRIDGE_PACKAGE=libatk-bridge2.0-0",
        "ATSPI_PACKAGE=libatspi2.0-0",
        "CUPS_PACKAGE=libcups2",
        "GLIB_PACKAGE=libglib2.0-0",
        "ALSA_PACKAGE=libasound2t64",
        "ATK_PACKAGE=libatk1.0-0t64",
        "ATK_BRIDGE_PACKAGE=libatk-bridge2.0-0t64",
        "ATSPI_PACKAGE=libatspi2.0-0t64",
        "CUPS_PACKAGE=libcups2t64",
        "GLIB_PACKAGE=libglib2.0-0t64",
      ],
      "the supported release matrix must be the sole ABI package authority",
    );
    assert.doesNotMatch(
      active,
      /\b(?:ALSA|ATK|ATK_BRIDGE|ATSPI|CUPS|GLIB)_PACKAGE\s*\+=/u,
    );
    const packageArrays = [...active.matchAll(
      /^[ \t]*CHROME_RUNTIME_PACKAGES=\(\n(?<body>(?:[ \t]+[^\n]*\n)+)^[ \t]*\)$/gmu,
    )];
    assert.equal(packageArrays.length, 1, "Chrome must have one direct ELF package-root array");
    assert.deepEqual(
      (packageArrays[0].groups?.body ?? "")
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean),
      [
        '"$ALSA_PACKAGE"',
        '"$ATK_BRIDGE_PACKAGE"',
        '"$ATK_PACKAGE"',
        '"$ATSPI_PACKAGE"',
        '"$CUPS_PACKAGE"',
        '"$GLIB_PACKAGE"',
        "libcairo2",
        "libdbus-1-3",
        "libexpat1",
        "libgbm1",
        "libnspr4",
        "libnss3",
        "libpango-1.0-0",
        "libudev1",
        "libx11-6",
        "libxcb1",
        "libxcomposite1",
        "libxdamage1",
        "libxext6",
        "libxfixes3",
        "libxkbcommon0",
        "libxrandr2",
      ],
      "package roots must cover every direct DT_NEEDED owner of pinned Chrome and ChromeDriver",
    );
    assert.equal(active.match(/^readonly CHROME_RUNTIME_PACKAGES$/gmu)?.length, 1);
    const trustedSourceLines = [
      '"deb [signed-by=$DISTRO_KEYRING] https://deb.debian.org/debian $VERSION_CODENAME main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://deb.debian.org/debian $VERSION_CODENAME-updates main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://security.debian.org/debian-security $VERSION_CODENAME-security main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://archive.ubuntu.com/ubuntu $VERSION_CODENAME main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://archive.ubuntu.com/ubuntu $VERSION_CODENAME-updates main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://security.ubuntu.com/ubuntu $VERSION_CODENAME-security main" \\',
    ];
    assert.deepEqual(
      activeLines.filter((line) => /(?:^|["'])deb\s/u.test(line)),
      trustedSourceLines,
      "the generated source inventory must contain only the six exact distro sources",
    );
    assert.deepEqual(
      activeLines.filter((line) => /\$\{?APT_SOURCES\}?\/sources\.list/u.test(line)),
      [
        '"$APT_SOURCES/sources.list.d"',
        '> "$APT_SOURCES/sources.list"',
        '> "$APT_SOURCES/sources.list"',
        '-o "Dir::Etc::sourcelist=$APT_SOURCES/sources.list"',
        '-o "Dir::Etc::sourceparts=$APT_SOURCES/sources.list.d"',
      ],
      "the isolated source inventory must have one closed set of readers and writers",
    );
    assert.deepEqual(
      activeLines.filter((line) => line.startsWith("DISTRO_KEYRING=")),
      [
        "DISTRO_KEYRING=/usr/share/keyrings/debian-archive-keyring.gpg",
        "DISTRO_KEYRING=/usr/share/keyrings/ubuntu-archive-keyring.gpg",
      ],
      "each distro branch must bind its official archive keyring exactly once",
    );
    assert.equal(active.match(/^readonly DISTRO_KEYRING$/gmu)?.length, 1);
    assert.equal(active.match(/^\s*test -r "\$DISTRO_KEYRING"$/gmu)?.length, 2);
    assert.match(
      active,
      /^\s*\*\)\n\s+echo "unsupported Chrome dependency distro: \$ID" >&2\n\s+exit 1\n\s+;;$/mu,
    );

    const optionsEnd = optionArrays[0].index + optionArrays[0][0].length;
    const afterOptions = active.slice(optionsEnd);
    assert.match(afterOptions, /^\nreadonly APT_OPTIONS\napt-get /u);
    const update = active.indexOf('apt-get "${APT_OPTIONS[@]}" update', optionsEnd);
    const download = active.indexOf('apt-get "${APT_OPTIONS[@]}" install \\', update);
    assert.ok(optionsEnd < update && update < download);
    assert.deepEqual(
      active
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => /(?:^|[\s;&(|])apt(?:-get)?(?=\s)/u.test(line)),
      [
        'apt-get "${APT_OPTIONS[@]}" update',
        'apt-get "${APT_OPTIONS[@]}" install \\',
      ],
      "every APT invocation must use the one immutable isolated option set",
    );
    for (const option of [
      "--download-only \\",
      "--reinstall \\",
      "--no-install-recommends \\",
      "--yes \\",
      '"${CHROME_RUNTIME_PACKAGES[@]}"',
    ]) {
      assert.ok(activeLines.includes(option), `missing package-closure resolver option: ${option}`);
    }
    assert.match(active, /^assert_elf_closure\(\) \{$/mu);
    assert.match(active, /^\s*LD_LIBRARY_PATH="\$\{DEPS_LIB\}:\$\{LD_LIBRARY_PATH:-\}" ldd "\$executable"/mu);
    assert.match(active, /^assert_elf_closure "\$CHROME_BIN"$/mu);
    assert.match(active, /^assert_elf_closure "\$CHROMEDRIVER_BIN"$/mu);
    assert.match(active, /^LD_LIBRARY_PATH="\$\{DEPS_LIB\}:\$\{LD_LIBRARY_PATH:-\}" "\$CHROME_BIN" --version$/mu);
    assert.match(active, /^LD_LIBRARY_PATH="\$\{DEPS_LIB\}:\$\{LD_LIBRARY_PATH:-\}" "\$CHROMEDRIVER_BIN" --version$/mu);
    assert.doesNotMatch(active, /(?:CHROME_BIN|CHROMEDRIVER_BIN).*--version\s*\|\|\s*true/u);
  };
  assertAptSourceIsolation(ci);
  for (const [mutationIndex, mutant] of [
    ci.replace(
      '            -o "Dir::Etc::sourcelist=$APT_SOURCES/sources.list"',
      '            # -o "Dir::Etc::sourcelist=$APT_SOURCES/sources.list"',
    ),
    ci.replace(
      '          apt-get "${APT_OPTIONS[@]}" update',
      '          APT_OPTIONS=()\n          apt-get "${APT_OPTIONS[@]}" update',
    ),
    ci.replace(
      '          apt-get "${APT_OPTIONS[@]}" update',
      '          APT_SOURCES=/etc/apt\n          apt-get "${APT_OPTIONS[@]}" update',
    ),
    ci.replace(
      '          apt-get "${APT_OPTIONS[@]}" update',
      '          :; APT_OPTIONS=()\n          apt-get "${APT_OPTIONS[@]}" update',
    ),
    ci.replace(
      '          apt-get "${APT_OPTIONS[@]}" install \\',
      '          unset APT_OPTIONS\n          apt-get "${APT_OPTIONS[@]}" install \\',
    ),
    ci.replace(
      '          apt-get "${APT_OPTIONS[@]}" update',
      '          apt-get download libnss3\n          apt-get "${APT_OPTIONS[@]}" update',
    ),
    ci.replace(
      "debian:bookworm|ubuntu:jammy)",
      "debian:bookworm|ubuntu:noble)",
    ),
    ci.replace(
      "https://deb.debian.org/debian $VERSION_CODENAME main",
      "https://example.invalid/debian $VERSION_CODENAME main",
    ),
    ci.replace(
      "[signed-by=$DISTRO_KEYRING] https://archive.ubuntu.com/ubuntu",
      "[signed-by=/tmp/forged.gpg] https://archive.ubuntu.com/ubuntu",
    ),
    ci.replace(
      "DISTRO_KEYRING=/usr/share/keyrings/debian-archive-keyring.gpg",
      "DISTRO_KEYRING=/tmp/forged.gpg",
    ),
    ci.replace(
      "https://security.ubuntu.com/ubuntu $VERSION_CODENAME-security main",
      "https://security.ubuntu.com/ubuntu stable-security main",
    ),
    ci.replace(
      '          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n          CHROME_RUNTIME_PACKAGES=(',
      '          ALSA_PACKAGE=libasound2t64\n' +
        '          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n          CHROME_RUNTIME_PACKAGES=(',
    ),
    ci.replace(
      "          readonly DISTRO_KEYRING\n",
      "          readonly DISTRO_KEYRING\n" +
        '          echo "deb [trusted=yes] https://deb.debian.org/debian sid main" >> "$APT_SOURCES/sources.list"\n',
    ),
    ci.replace(
      "        run: |\n          set -euo pipefail\n          # -- CfT chrome + chromedriver --",
      "        run: |\n          set +e\n          # -- CfT chrome + chromedriver --",
    ),
    ci.replace(
      '          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n          CHROME_RUNTIME_PACKAGES=(',
      '          ALSA_PACKAGE+=t64\n          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n          CHROME_RUNTIME_PACKAGES=(',
    ),
    ci.replace(
      "          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n",
      "          readonly ALSA_PACKAGE ATK_PACKAGE ATK_BRIDGE_PACKAGE ATSPI_PACKAGE CUPS_PACKAGE GLIB_PACKAGE\n          :; set +e\n",
    ),
    ci.replace(
      "          readonly DISTRO_KEYRING\n",
      "          readonly DISTRO_KEYRING\n" +
        '          echo "deb https://example.invalid/debian sid main" >> $APT_SOURCES/sources.list\n',
    ),
  ].entries()) {
    assert.notEqual(mutant, ci, `hostile APT mutation ${mutationIndex} must bite`);
    assert.throws(
      () => assertAptSourceIsolation(mutant),
      undefined,
      `hostile APT mutation ${mutationIndex} must be rejected`,
    );
  }
  assert.match(ci, /Dir::State::lists=\$APT_LISTS/);
  assert.match(ci, /Dir::State::status=\/var\/lib\/dpkg\/status/);
  assert.match(ci, /Dir::Cache=\$APT_CACHE/);
  assert.match(ci, /Dir::Cache::archives=\$APT_CACHE\/archives/);
  assert.match(ci, /Debug::NoLocking=1/);
  assert.match(ci, /Acquire::Retries=3/);
  const aptUpdate = ci.indexOf('apt-get "${APT_OPTIONS[@]}" update');
  const aptDownload = ci.indexOf('apt-get "${APT_OPTIONS[@]}" install \\');
  assert.ok(
    aptUpdate >= 0 && aptDownload >= 0 && aptUpdate < aptDownload,
    "Chrome dependency download must use a fresh isolated APT index",
  );
  assert.match(ci, /CHROME_BIN_DIR="\$RUNNER_TEMP\/chrome-bin-\$GITHUB_JOB"/);
  const executableCi = ci
    .split("\n")
    .filter((line) => !line.trimStart().startsWith("#"))
    .join("\n");
  assert.doesNotMatch(
    executableCi,
    /\$HOME|~\//,
    "WASM/Chrome state must not leak into shared HOME",
  );
  assert.match(
    ci,
    /printf '%s  %s\\n' "\$CHROME_FOR_TESTING_SHA256"[\s\S]*sha256sum --check --strict/,
  );
  assert.match(
    ci,
    /printf '%s  %s\\n' "\$CHROMEDRIVER_FOR_TESTING_SHA256"[\s\S]*sha256sum --check --strict/,
  );
  assert.ok(
    ci.lastIndexOf("sha256sum --check --strict") < ci.indexOf("unzip -q -o"),
    "CfT archives must be authenticated before extraction",
  );
  assert.doesNotMatch(ci, /last-known-good-versions/);
  assert.match(
    ci,
    /wasm-pack test --headless --chrome --chromedriver "\$CHROMEDRIVER_PATH" crates\/labcolors-wasm --locked/,
  );
  assertCheckoutCredentialsAreEphemeral(ci, "CI");
  assertCheckoutCredentialsAreEphemeral(
    read(".github", "workflows", "native-conformance-worker.yml"),
    "native conformance",
  );
  assertCheckoutCredentialsAreEphemeral(
    read(".github", "workflows", "mutation-worker.yml"),
    "scheduled mutation",
  );
  assertCheckoutCredentialsAreEphemeral(
    read(".github", "workflows", "publish-worker.yml"),
    "publish",
  );
});

test("the atomic output sink has one bounded pinned-Chrome browser gate", () => {
  const ci = read(".github", "workflows", "ci-worker.yml");
  const proof = read("scripts", "test-browser-output-sink.mjs");
  const stepName = 'name: "@labpics/colors: real-browser atomic output-sink proof"';

  const assertGate = (workflow) => {
    const step = workflowStepLines(workflow, stepName);
    const activeStep = step
      .filter((line) => !line.trimStart().startsWith("#"))
      .join("\n");
    assert.doesNotMatch(activeStep, /^\s*(?:if:|continue-on-error:)/mu);
    assert.equal(
      step.filter((line) =>
        line.trim() === 'LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000"'
      ).length,
      1,
      "browser proof must have one explicit whole-run deadline",
    );
    assert.equal(
      workflowRunScript(workflow, stepName),
      "set -euo pipefail\nnode scripts/test-browser-output-sink.mjs",
      "browser proof must be one fail-closed dependency-free command",
    );
    assert.equal(
      [...workflow.matchAll(/node scripts\/test-browser-output-sink\.mjs/gmu)].length,
      1,
      "workflow must have one browser output-sink authority",
    );
    const install = workflow.indexOf("name: install Chrome + dependencies");
    const gate = workflow.indexOf(stepName);
    const wasm = workflow.indexOf("name: wasm-pack test (headless chrome)");
    assert.ok(
      install >= 0 && install < gate && gate < wasm,
      "real-DOM output proof must reuse pinned Chrome before the WASM browser parity gate",
    );
  };

  const assertProofContract = (source) => {
    const staticImports = source
      .split(/\r?\n/u)
      .filter((line) => line.trimStart().startsWith("import "));
    assert.deepEqual(
      staticImports,
      [
        'import { spawn } from "node:child_process";',
        'import { access, readFile, stat } from "node:fs/promises";',
        'import { createServer } from "node:http";',
        'import { isAbsolute, resolve } from "node:path";',
        'import { fileURLToPath } from "node:url";',
      ],
      "browser proof runtime imports must be the reviewed Node builtin set",
    );
    assert.equal(
      [...source.matchAll(/\bimport\s*\(/gmu)].length,
      1,
      "the served output sink must be the only dynamic import",
    );
    assert.match(source, /^  const module = await import\(moduleUrl\);$/mu);
    assert.equal(
      [...source.matchAll(/LAB_COLORS_BROWSER_OUTPUT_SINK_PASS v2 checks=11/gmu)].length,
      1,
      "browser proof must expose one exact success receipt",
    );
    assert.match(source, /const chromePath = await executableFromEnv\("CHROME_PATH"\);/u);
    assert.match(source, /const chromeDriverPath = await executableFromEnv\("CHROMEDRIVER_PATH"\);/u);
    assert.match(source, /startChromeDriver\(\s*chromeDriverPath,/u);
    assert.match(source, /binary: chromePath,/u);
    assert.ok(
      chromeArguments(source).includes("--no-sandbox"),
      "the userspace CfT proof must opt out of an unavailable host sandbox",
    );
    const flagOutsideChromeArguments = source.replace(
      '                  "--no-sandbox",',
      '                  "--window-size=800,600",\n                  // "--no-sandbox"',
    );
    assert.notEqual(flagOutsideChromeArguments, source);
    assert.equal(chromeArguments(flagOutsideChromeArguments).includes("--no-sandbox"), false);
    assert.match(source, /spawn\(executable, \["--port=0"\]/u);
    assert.match(
      source,
      /if \(overall\.controller\.signal\.aborted\) \{\s*onOverallAbort\(\);\s*\} else if \(startup\.controller\.signal\.aborted\)/u,
    );
    assert.match(source, /LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS must be a positive integer/u);
    assert.match(
      source,
      /const overall = timeoutSignal\(proofTimeoutMilliseconds, "browser output-sink proof"\);/u,
    );
    assert.match(source, /overall\.throwIfExpired\(\);/u);
    assert.match(source, /AbortSignal\.any\(\[\s*deadline\.controller\.signal,\s*overall\.controller\.signal,/u);
    assert.match(source, /server\.listen\(\{ port: 0, host: LOOPBACK_HOST, signal: listenSignal \}\);/u);
    assert.match(source, /listen\(server, commandTimeoutMilliseconds, overall\)/u);
    assert.match(source, /closeServer\(server, PROCESS_STOP_TIMEOUT_MS\)/u);
    assert.match(
      source,
      /if \(driverPort && sessionId\) \{\s*const teardown = timeoutSignal\(PROCESS_STOP_TIMEOUT_MS, "WebDriver session teardown"\);/u,
    );
    assert.match(
      source,
      /await driverRequest\(\s*driverPort,\s*`\/session\/\$\{sessionId\}`,\s*\{ method: "DELETE" \},\s*PROCESS_STOP_TIMEOUT_MS,\s*teardown,\s*\)/u,
    );
    assert.match(
      source,
      /try \{\s*await driverRequest\([\s\S]*?`\/session\/\$\{sessionId\}`[\s\S]*?\)\.catch\(\(error\) => cleanupErrors\.push\(error\)\);\s*\} finally \{\s*teardown\.clear\(\);\s*\}/u,
    );
    assert.match(source, /"packages\/colors\/output-sink\.js"/u);
    assert.match(source, /"packages\/colors\/output-bindings\.js"/u);
    assert.match(source, /"packages\/colors\/sequence-identity-matches\.js"/u);
    assert.match(
      source,
      /const \[moduleSource, bindingModuleSource, alignmentModuleSource\] = await Promise\.all\(\[/u,
    );
    assert.match(source, /response\.end\(moduleSource\);/u);
    assert.match(source, /response\.end\(bindingModuleSource\);/u);
    assert.match(source, /response\.end\(alignmentModuleSource\);/u);
    assert.match(source, /args: \[`\$\{origin\}\/output-sink\.js\?proof=v2`\]/u);
    assert.match(source, /\/session\/\$\{sessionId\}\/execute\/async/u);
    assert.match(source, /\["replace", "insertRule", "deleteRule", "addRule", "removeRule"\]/u);
    assert.match(source, /\["set", "append", "delete", "clear"\]/u);
    assert.match(source, /const inlineMutationObserver = new MutationObserver\(recordInlineMutations\);/u);
    assert.match(source, /attributeFilter: \["style"\],/u);
    assert.match(source, /"style MutationObserver is sensitive to namespace writes"/u);
    assert.match(source, /"legacy live CSSOM instrumentation is mutation-sensitive"/u);
    assert.match(source, /"live Typed OM instrumentation is mutation-sensitive"/u);
    assert.match(source, /"binding admission performs no live replacement before commit"/u);
    assert.equal(
      [...source.matchAll(/^\s*action\(markActionInvoked\);$/gmu)].length,
      1,
      "native admission helper must execute its supplied action exactly once",
    );
    assert.equal(
      [...source.matchAll(/^\s*markInvoked\(\);$/gmu)].length,
      3,
      "each native-brand counterexample must witness entry into its own action",
    );
    assert.match(
      source,
      /equal\(actionInvocations, 1, `\$\{message\}: hostile admission action executes exactly once`\)/u,
    );
    assert.match(source, /mark\("native-target-brand-matrix"\)/u);
    assert.match(source, /"Proxy around a real open-shadow host fails native target admission"/u);
    assert.match(source, /"plain structural fake fails native target admission"/u);
    assert.match(
      source,
      /"genuine Element with shadowed identity fields fails native target admission"/u,
    );
    assert.match(source, /"exact-target-identity"/u);
    assert.match(source, /"document root uses the exact :root selector"/u);
    assert.match(source, /"open ShadowRoot uses the exact :host selector"/u);
    assert.match(
      source,
      /"arbitrary connected light-DOM child is outside the output identity boundary"/u,
    );
    assert.match(
      source,
      /"arbitrary ShadowRoot descendant cannot impersonate its host identity"/u,
    );
    assert.match(source, /"closed ShadowRoot is outside the explicit output identity boundary"/u);
    assert.match(source, /"cloneNode\(true\) cannot receive the shadow host owned property"/u);
    assert.doesNotMatch(source, /markerName|data-lab-colors-output-sink/u);
    assert.match(source, /"post-replace-drift-recovery"/u);
    assert.match(source, /let injectPostReplaceDrift = true;/u);
    assert.match(source, /"OUTPUT_ATOMICITY_VIOLATION"/u);
    assert.match(source, /"post-replace drift restores prior live bytes exactly"/u);
    assert.match(source, /"post-replace drift leaves the logical stamp unchanged"/u);
    assert.match(source, /"post-replace drift leaves the prior computed value effective"/u);
    assert.match(source, /"temporary replaceSync fault wrapper is restored exactly"/u);
    assert.doesNotMatch(source, /(?:playwright|puppeteer|selenium|npm install|npx )/iu);
  };

  assertGate(ci);
  assertGate(ci.replaceAll("\n", "\r\n"));
  assertProofContract(proof);

  const stepLine = `      - ${stepName}`;
  for (const [name, mutant] of [
    ["conditional bypass", ci.replace(stepLine, `${stepLine}\n        if: false`)],
    ["nonblocking bypass", ci.replace(stepLine, `${stepLine}\n        continue-on-error: true`)],
    ["unbounded declaration", ci.replace('LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "60000"',
      'LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS: "0"')],
    ["fail-open shell", ci.replace(
      "          set -euo pipefail\n          node scripts/test-browser-output-sink.mjs",
      "          set +e\n          node scripts/test-browser-output-sink.mjs",
    )],
    ["commented proof", ci.replace(
      "          node scripts/test-browser-output-sink.mjs",
      "          # node scripts/test-browser-output-sink.mjs",
    )],
  ]) {
    assert.notEqual(mutant, ci, `${name} mutation must alter the workflow`);
    assert.throws(() => assertGate(mutant), undefined, `${name} mutation must be rejected`);
  }

  const receiptMutant = proof.replace(
    "LAB_COLORS_BROWSER_OUTPUT_SINK_PASS v2 checks=11",
    "LAB_COLORS_BROWSER_OUTPUT_SINK_PASS v2 checks=10",
  );
  assert.notEqual(receiptMutant, proof, "receipt mutation must alter the proof script");
  assert.throws(
    () => assertProofContract(receiptMutant),
    undefined,
    "incomplete proof receipt must be rejected",
  );

  for (const [name, mutant] of [
    ["unbound Chrome binary", proof.replace("binary: chromePath,", "binary: chromeDriverPath,")],
    ["unbound ChromeDriver", proof.replace(
      "startChromeDriver(\n      chromeDriverPath,",
      "startChromeDriver(\n      chromePath,",
    )],
    ["substituted browser module", proof.replace(
      "const module = await import(moduleUrl);",
      'const module = await import("./substitute.js");',
    )],
    ["unbounded proof server", proof.replace(
      "listen(server, commandTimeoutMilliseconds, overall)",
      "listen(server)",
    )],
    ["nonabortable proof server", proof.replace(
      "server.listen({ port: 0, host: LOOPBACK_HOST, signal: listenSignal });",
      "server.listen(0, LOOPBACK_HOST);",
    )],
    ["lost startup abort", proof.replace(
      "if (overall.controller.signal.aborted) {\n        onOverallAbort();\n      } else if (startup.controller.signal.aborted) {\n        onStartupAbort();\n      }",
      "",
    )],
    ["unobserved inline namespace writer", proof.replace(
      'attributeFilter: ["style"],',
      'attributeFilter: ["class"],',
    )],
    ["unobserved legacy CSSOM writer", proof.replace(
      '["replace", "insertRule", "deleteRule", "addRule", "removeRule"]',
      '["replace", "insertRule", "deleteRule"]',
    )],
    ["unobserved Typed OM writer", proof.replace(
      '["set", "append", "delete", "clear"]',
      '["set"]',
    )],
    ["unobserved binding precommit", proof.replace(
      '"binding admission performs no live replacement before commit"',
      '"binding admission checked only final state"',
    )],
    ["synthetic native target rejection", proof.replace(
      "action(markActionInvoked);",
      'throw Object.assign(new Error("synthetic capability"), { code: "OUTPUT_TARGET_CAPABILITY" });',
    )],
    ["lost exact root selector oracle", proof.replace(
      '"document root uses the exact :root selector"',
      '"document root has a selector"',
    )],
    ["lost clone identity counterexample", proof.replace(
      '"cloneNode(true) cannot receive the shadow host owned property"',
      '"clone was appended"',
    )],
    ["lost native target brand receipt", proof.replace(
      'mark("native-target-brand-matrix");',
      'mark("exact-target-identity");',
    )],
    ["lost shadowed native identity counterexample", proof.replace(
      '"genuine Element with shadowed identity fields fails native target admission"',
      '"structural target was checked"',
    )],
    ["lost post-replace byte rollback oracle", proof.replace(
      '"post-replace drift restores prior live bytes exactly"',
      '"post-replace drift was observed"',
    )],
    ["skipped expired-session teardown", proof.replace(
      "if (driverPort && sessionId) {",
      "if (driverPort && sessionId && !overall.controller.signal.aborted) {",
    )],
    ["session teardown coupled to overall deadline", proof.replace(
      "          teardown,\n        ).catch((error) => cleanupErrors.push(error));",
      "          overall,\n        ).catch((error) => cleanupErrors.push(error));",
    )],
    ["uncleared session teardown timer", proof.replace(
      "        teardown.clear();",
      "",
    )],
    ["unserved output-binding authority", proof.replace(
      "      response.end(bindingModuleSource);",
      "      response.writeHead(404).end();",
    )],
  ]) {
    assert.notEqual(mutant, proof, `${name} mutation must alter the proof script`);
    assert.throws(
      () => assertProofContract(mutant),
      undefined,
      `${name} mutation must be rejected`,
    );
  }
});

test("publish accepts only canonical exact-SHA workflow runs and their immutable CI artifact", () => {
  const caller = read(".github", "workflows", "publish.yml");
  const publish = read(".github", "workflows", "publish-worker.yml");
  assert.match(
    caller,
    /^\s*uses: Labpics-Team\/lab-colors\/\.github\/workflows\/publish-worker\.yml@[0-9a-f]{40}$/m,
  );
  assert.equal(
    [...caller.matchAll(/publish-worker\.yml@/gu)].length,
    1,
    "publish caller must bind exactly one immutable worker",
  );
  assert.doesNotMatch(caller, /^\s+steps:$/m);
  assert.doesNotMatch(caller, /^\s+secrets:\s*inherit$/m);
  assert.match(publish, /^\s*NODE_TOOLCHAIN: "24\.14\.0"$/m);
  assert.match(publish, /^\s*NPM_TOOLCHAIN: "11\.9\.0"$/m);
  assert.match(
    caller,
    /^concurrency:\n  group: npm-publish\n  cancel-in-progress: false$/m,
  );
  assert.match(caller, /permissions:\n  contents: read\n  actions: read\n/);
  assert.match(publish, /permissions:\n  contents: read\n  actions: read\n/);
  assert.doesNotMatch(publish, /^\s*checks:/m);
  assert.doesNotMatch(publish, /id-token:/);
  assert.doesNotMatch(publish, /Trusted Publishing|OIDC/);
  assert.equal(
    [...publish.matchAll(/secrets\.NPM_TOKEN/g)].length,
    1,
    "granular npm publish token must be scoped to one step",
  );
  assert.match(
    publish,
    /- name: npm publish verified CI tarball \(granular token, no rebuild\/repack\)[\s\S]*?env:\s*\n\s*NODE_AUTH_TOKEN: \$\{\{ secrets\.NPM_TOKEN \}\}[\s\S]*?run:\s*\|[\s\S]*?npm publish --ignore-scripts/,
  );
  assert.match(publish, /TMPDIR=\$RUNNER_TEMP\/tmp-\$GITHUB_JOB/);
  assert.doesNotMatch(publish, /RUSTUP_HOME|CARGO_HOME|RUST_TOOLCHAIN/);
  assert.doesNotMatch(publish, /Swatinem\/rust-cache/);
  assert.doesNotMatch(publish, /^\s*cache: npm$/m);

  const requiredChecks = [
    "Node 22 consumer floor",
    "MSRV workspace check",
    "clippy + rustfmt",
    "cargo doc (intra-doc links)",
    "test",
    "cargo audit (rustsec)",
    "wasm build + headless test + size",
    "swift conformance (self-hosted Linux, pinned toolchain)",
  ];
  for (const check of requiredChecks) {
    assert.ok(publish.includes(JSON.stringify(check)), `publish gate lost required check ${check}`);
  }
  assert.match(publish, /file: "ci\.yml"[\s\S]*path: "\.github\/workflows\/ci\.yml"/);
  assert.match(
    publish,
    /ci-worker\.yml@1461bc2ed60142aed3a8723e618b883be6418156"[\s\S]*sha: "1461bc2ed60142aed3a8723e618b883be6418156"/,
  );
  assert.match(
    publish,
    /file: "native-conformance\.yml"[\s\S]*path: "\.github\/workflows\/native-conformance\.yml"/,
  );
  assert.match(publish, /head_sha: expectedSha/);
  assert.match(publish, /branch: "main"/);
  assert.match(publish, /event: "push"/);
  assert.match(publish, /run\.path === spec\.path/);
  assert.match(publish, /run\.head_sha === expectedSha/);
  assert.match(publish, /Number\(right\.id\) - Number\(left\.id\)/);
  assert.match(publish, /run\.status === "completed" && run\.conclusion === "success"/);
  assert.match(publish, /\/actions\/runs\/\$\{run\.id\}\/jobs\?filter=latest/);
  assert.match(publish, /Number\(job\.run_id\) === Number\(run\.id\)/);
  assert.doesNotMatch(publish, /check-runs|\/commits\/.*\/checks/);

  assert.match(publish, /node-version: \$\{\{ env\.NODE_TOOLCHAIN \}\}/);
  assert.match(
    publish,
    /actions\/download-artifact@[0-9a-f]{40}[\s\S]*name: colors-release-\$\{\{ github\.sha \}\}-attempt-\$\{\{ steps\.canonical-runs\.outputs\.ci_run_attempt \}\}[\s\S]*run-id: \$\{\{ steps\.canonical-runs\.outputs\.ci_run_id \}\}/,
  );
  assert.match(publish, /outputs\.push\(`ci_run_attempt=\$\{run\.run_attempt\}`\)/);
  assert.match(publish, /manifest\.sourceSha !== expectedSha/);
  assert.match(publish, /manifest\.npm !== expectedVersion/);
  assert.match(publish, /process\.versions\.node !== expectedNode/);
  assert.match(publish, /execFileSync\("npm", \["--version"\]/);
  assert.match(publish, /npmVersion !== expectedNpm/);
  assert.match(publish, /manifest\.artifacts\?\.tarball/);
  assert.match(publish, /evidence\.bytes !== bytes\.length/);
  assert.match(publish, /createHash\("sha256"\)\.update\(bytes\)\.digest\("hex"\)/);
  assert.match(publish, /packedPackage\.name !== "@labpics\/colors"/);
  assert.match(
    publish,
    /TARBALL_PATH: \$\{\{ steps\.verified-artifact\.outputs\.tarball \}\}[\s\S]*TARBALL_SHA256: \$\{\{ steps\.verified-artifact\.outputs\.sha256 \}\}[\s\S]*run:\s*\|[\s\S]*npm publish --ignore-scripts --@labpics:registry=https:\/\/registry\.npmjs\.org "\$TARBALL_PATH"/,
  );
  assert.doesNotMatch(publish, /wasm-pack|npm ci|release:verify|actions\/upload-artifact|npm pack/);

  const downloaded = publish.indexOf("name: download immutable exact-SHA release artifact");
  const exactNode = publish.indexOf("actions/setup-node@");
  const validated = publish.indexOf("id: verified-artifact");
  const token = publish.indexOf("NODE_AUTH_TOKEN:");
  const canonicalGuard = publish.indexOf("id: canonical-runs");
  assert.ok(
    canonicalGuard >= 0 &&
      canonicalGuard < downloaded &&
      downloaded < exactNode &&
      exactNode < validated &&
      validated < token,
    "network toolchain setup must happen only after canonical-run and artifact gates",
  );
});

test("tag ancestry guard works after credential-free checkout and rejects non-ancestors", () => {
  const publish = read(".github", "workflows", "publish-worker.yml");
  const fullGuard = workflowRunScript(
    publish,
    "name: guard — exact tag SHA is in origin/main",
  );
  assert.doesNotMatch(
    fullGuard,
    /\bgit\s+fetch\b/u,
    "the credential-free ancestry step must not perform any private-repo git fetch",
  );
  assert.match(
    publish,
    /      - name: guard — exact tag SHA is in origin\/main\n        env:\n          GH_READ_TOKEN: \$\{\{ github\.token \}\}\n        run: \|/,
    "the ancestry step must receive the job-scoped read token directly",
  );
  assert.match(
    publish,
    /checked_out="\$\(git rev-parse HEAD\)"[\s\S]*?"\$checked_out" != "\$GITHUB_SHA"/,
    "the API ancestry proof must not replace exact checkout identity",
  );
  assert.match(
    publish,
    /fetch-depth: 1[\s\S]*?persist-credentials: false/,
    "publish checkout should fetch only the tagged commit and persist no credential",
  );

  const guard = workflowNodeScript(
    publish,
    "name: guard — exact tag SHA is in origin/main",
  );
  const expectedSha = "a".repeat(40);
  const fetchHarness = `
    const fixture = JSON.parse(process.env.ANCESTRY_FIXTURE);
    global.fetch = async (input, init) => {
      const url = new URL(String(input));
      const expectedPath =
        "/repos/Labpics-Team/lab-colors/compare/${expectedSha}...main";
      if (url.pathname !== expectedPath) {
        return new Response("unexpected API path", { status: 404 });
      }
      if (init?.headers?.Authorization !== "Bearer test-token") {
        return new Response("missing job-scoped token", { status: 401 });
      }
      if (
        init.headers.Accept !== "application/vnd.github+json" ||
        init.headers["X-GitHub-Api-Version"] !== "2022-11-28"
      ) {
        return new Response("missing pinned GitHub API contract", { status: 400 });
      }
      return new Response(JSON.stringify(fixture.body), {
        status: fixture.httpStatus,
        headers: { "content-type": "application/json" },
      });
    };
  `;
  const execute = (body, { httpStatus = 200, unset = [], overrides = {} } = {}) => {
    const env = {
      ...process.env,
      ANCESTRY_FIXTURE: JSON.stringify({ body, httpStatus }),
      GH_READ_TOKEN: "test-token",
      GITHUB_API_URL: "https://api.github.test",
      GITHUB_REPOSITORY: "Labpics-Team/lab-colors",
      GITHUB_SHA: expectedSha,
      ...overrides,
    };
    for (const key of unset) delete env[key];
    return execFileSync(process.execPath, ["-e", `${fetchHarness}\n${guard}`], {
      env,
      encoding: "utf8",
      stdio: "pipe",
    });
  };

  const shellFixture = mkdtempSync(join(tmpdir(), "labcolors-tag-head-"));
  try {
    execFileSync("git", ["init", "--quiet"], { cwd: shellFixture });
    writeFileSync(join(shellFixture, "fixture"), "exact tag checkout\n");
    execFileSync("git", ["add", "fixture"], { cwd: shellFixture });
    execFileSync(
      "git",
      [
        "-c",
        "user.name=Release Guard Test",
        "-c",
        "user.email=release-guard@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "fixture",
      ],
      { cwd: shellFixture },
    );
    const checkedOutSha = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: shellFixture,
      encoding: "utf8",
    }).trim();
    const fakeBin = join(shellFixture, "bin");
    mkdirSync(fakeBin);
    const fakeNode = join(fakeBin, "node");
    writeFileSync(fakeNode, "#!/bin/sh\nexit 0\n");
    chmodSync(fakeNode, 0o755);
    const runShellGuard = (sha) => {
      const env = withControlledPath(
        process.env,
        `${fakeBin}${delimiter}${process.env.PATH ?? ""}`,
      );
      env.GITHUB_SHA = sha;
      return execFileSync(bashExecutable(), ["-euo", "pipefail", "-c", fullGuard], {
        cwd: shellFixture,
        env,
        encoding: "utf8",
        stdio: "pipe",
      });
    };
    assert.doesNotThrow(() => runShellGuard(checkedOutSha));
    assert.throws(() => runShellGuard("b".repeat(40)), /Command failed/u);
  } finally {
    rmSync(shellFixture, { recursive: true, force: true });
  }

  const acceptedStatuses = ["identical", "ahead"];
  assert.equal(acceptedStatuses.length, 2, "anti-vacuum: both legal compare states are covered");
  for (const status of acceptedStatuses) {
    assert.match(
      execute({
        status,
        base_commit: { sha: expectedSha },
        merge_base_commit: { sha: expectedSha },
      }),
      /verified tag commit is in main history/u,
    );
  }

  const rejected = [
    {
      status: "behind",
      base_commit: { sha: expectedSha },
      merge_base_commit: { sha: expectedSha },
    },
    {
      status: "diverged",
      base_commit: { sha: expectedSha },
      merge_base_commit: { sha: "b".repeat(40) },
    },
    {
      status: "ahead",
      base_commit: { sha: expectedSha },
      merge_base_commit: { sha: "b".repeat(40) },
    },
    {
      status: "ahead",
      base_commit: { sha: "b".repeat(40) },
      merge_base_commit: { sha: expectedSha },
    },
  ];
  assert.equal(rejected.length, 4, "anti-vacuum: ancestry rejection matrix was reduced");
  for (const fixture of rejected) {
    assert.throws(() => execute(fixture), /Command failed/u);
  }
  assert.throws(
    () => execute({ message: "forbidden" }, { httpStatus: 403 }),
    /Command failed/u,
  );
  const requiredEnvironment = [
    "GH_READ_TOKEN",
    "GITHUB_API_URL",
    "GITHUB_REPOSITORY",
    "GITHUB_SHA",
  ];
  assert.equal(requiredEnvironment.length, 4, "anti-vacuum: required env matrix shrank");
  for (const key of requiredEnvironment) {
    assert.throws(
      () => execute({}, { unset: [key] }),
      /Command failed/u,
      `missing ${key} must fail closed`,
    );
  }
  assert.throws(
    () => execute({}, { overrides: { GITHUB_SHA: "not-a-sha" } }),
    /Command failed/u,
    "malformed tag SHA must fail before the API call",
  );
});

test("canonical-run guard executes against workflow-scoped runs and jobs", () => {
  const publish = read(".github", "workflows", "publish-worker.yml");
  const selector = workflowNodeScript(
    publish,
    "name: guard — canonical exact-SHA workflow runs and their own jobs",
  );
  const requiredCiJobs = [
    "Node 22 consumer floor",
    "MSRV workspace check",
    "clippy + rustfmt",
    "cargo doc (intra-doc links)",
    "test",
    "cargo audit (rustsec)",
    "wasm build + headless test + size",
  ];
  assert.ok(requiredCiJobs.length > 5, "anti-vacuum: CI gate list is unexpectedly small");

  const expectedSha = "a".repeat(40);
  const successfulJob = (name, runId) => ({
    name,
    run_id: runId,
    status: "completed",
    conclusion: "success",
  });
  const fixtures = {
    ciRuns: [
      {
        id: 999,
        path: ".github/workflows/not-ci.yml",
        head_sha: expectedSha,
        head_branch: "main",
        event: "push",
        status: "completed",
        conclusion: "success",
      },
      {
        id: 101,
        run_attempt: 3,
        path: ".github/workflows/ci.yml",
        head_sha: expectedSha,
        head_branch: "main",
        event: "push",
        status: "completed",
        conclusion: "success",
        referenced_workflows: [
          {
            path: "Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@1461bc2ed60142aed3a8723e618b883be6418156",
            sha: "1461bc2ed60142aed3a8723e618b883be6418156",
          },
        ],
      },
    ],
    nativeRuns: [
      {
        id: 202,
        run_attempt: 1,
        path: ".github/workflows/native-conformance.yml",
        head_sha: expectedSha,
        head_branch: "main",
        event: "push",
        status: "completed",
        conclusion: "success",
        referenced_workflows: [
          {
            path: "Labpics-Team/lab-colors/.github/workflows/native-conformance-worker.yml@1461bc2ed60142aed3a8723e618b883be6418156",
            sha: "1461bc2ed60142aed3a8723e618b883be6418156",
          },
        ],
      },
    ],
    ciJobs: [
      ...requiredCiJobs.map((name) => successfulJob(name, 101)),
      successfulJob(requiredCiJobs[0], 999),
    ],
    nativeJobs: [
      successfulJob("swift conformance (self-hosted Linux, pinned toolchain)", 202),
    ],
  };
  const fetchHarness = `
    const fixtures = JSON.parse(process.env.FETCH_FIXTURES);
    global.fetch = async (input) => {
      const path = new URL(String(input)).pathname;
      let payload;
      if (path.endsWith("/actions/workflows/ci.yml/runs")) {
        payload = { workflow_runs: fixtures.ciRuns };
      } else if (path.endsWith("/actions/workflows/native-conformance.yml/runs")) {
        payload = { workflow_runs: fixtures.nativeRuns };
      } else if (path.endsWith("/actions/runs/101/jobs")) {
        payload = { jobs: fixtures.ciJobs };
      } else if (path.endsWith("/actions/runs/202/jobs")) {
        payload = { jobs: fixtures.nativeJobs };
      } else {
        return new Response("unexpected API path", { status: 404 });
      }
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    };
  `;

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-run-gate-"));
  try {
    const output = join(temporary, "github-output");
    const execute = (value) => {
      writeFileSync(output, "");
      return execFileSync(process.execPath, ["-e", `${fetchHarness}\n${selector}`], {
        env: {
          ...process.env,
          FETCH_FIXTURES: JSON.stringify(value),
          GH_READ_TOKEN: "test-token",
          GITHUB_API_URL: "https://api.github.test",
          GITHUB_OUTPUT: output,
          GITHUB_REPOSITORY: "Labpics-Team/lab-colors",
          GITHUB_SHA: expectedSha,
        },
        encoding: "utf8",
        stdio: "pipe",
      });
    };

    execute(fixtures);
    assert.equal(
      readFileSync(output, "utf8"),
      "ci_run_id=101\nci_run_attempt=3\nnative_run_id=202\n",
    );

    const missingWorkerIdentity = structuredClone(fixtures);
    delete missingWorkerIdentity.ciRuns[1].referenced_workflows;
    assert.throws(() => execute(missingWorkerIdentity), /Command failed/u);

    const wrongWorkerIdentity = structuredClone(fixtures);
    wrongWorkerIdentity.ciRuns[1].referenced_workflows[0].sha = "b".repeat(40);
    assert.throws(() => execute(wrongWorkerIdentity), /Command failed/u);

    const wrongRunJobs = structuredClone(fixtures);
    wrongRunJobs.ciJobs = wrongRunJobs.ciJobs.filter((job) =>
      !(job.name === requiredCiJobs[0] && job.run_id === 101));
    assert.throws(() => execute(wrongRunJobs), /Command failed/u);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("publish artifact validator executes and rejects identity or byte drift", async () => {
  const publish = read(".github", "workflows", "publish-worker.yml");
  const validator = workflowNodeScript(
    publish,
    "name: validate manifest identity and byte-exact tarball",
  );
  assert.ok(validator.length > 1_000, "anti-vacuum: extracted validator is unexpectedly small");

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-publish-contract-"));
  try {
    const validatorPath = join(temporary, "publish-validator.cjs");
    writeFileSync(validatorPath, `${validator}\n`);
    const artifact = join(temporary, "artifact");
    const payload = join(temporary, "payload", "package");
    mkdirSync(artifact, { recursive: true });
    mkdirSync(payload, { recursive: true });
    const packageVersion = "0.11.0";
    const coreVersion = "0.3.0";
    const packageManifest = JSON.parse(read("packages", "colors", "package.json"));
    assert.equal(packageManifest.version, packageVersion);
    writeFileSync(
      join(payload, "package.json"),
      `${JSON.stringify(packageManifest, null, 2)}\n`,
    );
    copyFileSync(join(root, "packages", "colors", "README.md"), join(payload, "README.md"));
    copyFileSync(join(root, "LICENSE"), join(payload, "LICENSE"));

    const generatedPaths = new Set([
      "LICENSE",
      "build-metadata.json",
      "pkg/labcolors_bg.wasm",
      ...NUMERICAL_EVIDENCE_FILES.map((name) => `evidence/${name}`),
      "private-program/consumer.js",
      "private-program/labcolors_private_program.wasm",
      "private-program/build-metadata.json",
    ]);
    for (const path of packageManifest.files) {
      if (generatedPaths.has(path)) continue;
      const source = join(root, "packages", "colors", ...path.split("/"));
      const destination = join(payload, ...path.split("/"));
      assert.ok(existsSync(source), `fixture source is unavailable: ${path}`);
      mkdirSync(dirname(destination), { recursive: true });
      copyFileSync(source, destination);
    }
    const runtimeWasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
    mkdirSync(join(payload, "pkg"), { recursive: true });
    writeFileSync(join(payload, "pkg", "labcolors_bg.wasm"), runtimeWasm);

    const privateDirectory = join(payload, "private-program");
    mkdirSync(privateDirectory, { recursive: true });
    const privateConsumer = readFileSync(
      join(root, "packages", "colors", "private-program", "consumer.js"),
    );
    const privateWasm = readFileSync(
      join(
        root,
        "packages",
        "colors",
        "private-program",
        "labcolors_private_program.wasm",
      ),
    );
    writeFileSync(join(privateDirectory, "consumer.js"), privateConsumer);
    writeFileSync(join(privateDirectory, "labcolors_private_program.wasm"), privateWasm);

    const evidenceDir = join(payload, "evidence");
    mkdirSync(evidenceDir);
    const contracts = join(root, "crates", "labcolors-core", "contracts");
    const evidenceBytes = new Map();
    for (const name of NUMERICAL_EVIDENCE_FILES) {
      const contents = readFileSync(join(contracts, name));
      evidenceBytes.set(name, contents);
      writeFileSync(join(evidenceDir, name), contents);
    }
    const evidenceArtifact = (name) => {
      const contents = evidenceBytes.get(name);
      assert.ok(contents, `missing fixture evidence ${name}`);
      return {
        path: `evidence/${name}`,
        bytes: contents.length,
        sha256: createHash("sha256").update(contents).digest("hex"),
      };
    };
    const wcagProfileBytes = evidenceBytes.get(WCAG22_EVIDENCE_FILES[0]);
    const wcagProofBytes = evidenceBytes.get(WCAG22_EVIDENCE_FILES[2]);
    const pointProofBytes = evidenceBytes.get(POINT_SUPPORT_EVIDENCE_FILES[0]);
    assert.ok(wcagProfileBytes && wcagProofBytes && pointProofBytes);
    const wcagProfile = JSON.parse(wcagProfileBytes.toString("utf8"));
    const wcagProof = JSON.parse(wcagProofBytes.toString("utf8"));
    const pointProof = JSON.parse(pointProofBytes.toString("utf8"));

    const conformanceManifestBytes = readFileSync(
      join(root, "conformance", "vectors", "manifest.json"),
    );
    const conformanceManifest = JSON.parse(conformanceManifestBytes.toString("utf8"));
    const familyNames = [
      "contrasts.json",
      "ladders.json",
      "alpha.json",
      "solve.json",
      "wcag22.json",
    ];
    const familyBytes = familyNames.map((name) =>
      readFileSync(join(root, "conformance", "vectors", name))
    );

    const expectedSha = "a".repeat(40);
    const conformance = {
      packVersion: conformanceManifest.packVersion,
      packDigest: conformanceManifest.packDigest,
      counts: conformanceManifest.counts,
      manifestSha256: createHash("sha256").update(conformanceManifestBytes).digest("hex"),
      familySetSha256: createHash("sha256").update(Buffer.concat(familyBytes)).digest("hex"),
      families: familyNames.map((name, index) => ({
        path: `conformance/vectors/${name}`,
        bytes: familyBytes[index].length,
        sha256: createHash("sha256").update(familyBytes[index]).digest("hex"),
      })),
    };
    const wasmEvidence = [
      {
        role: "runtime",
        path: "pkg/labcolors_bg.wasm",
        bytes: runtimeWasm.length,
        sha256: createHash("sha256").update(runtimeWasm).digest("hex"),
      },
    ];
    const buildMetadata = {
      schemaVersion: 2,
      package: { name: "@labpics/colors", version: packageVersion },
      sourceSha: expectedSha,
      coreVersion,
      conformance: {
        packVersion: conformance.packVersion,
        packDigest: conformance.packDigest,
        manifestSha256: conformance.manifestSha256,
        familySetSha256: conformance.familySetSha256,
      },
      wasm: wasmEvidence,
    };
    const metadataPath = join(payload, "build-metadata.json");
    const metadataBytes = Buffer.from(`${JSON.stringify(buildMetadata)}\n`);
    writeFileSync(metadataPath, metadataBytes);

    const privateWasmEvidence = {
      role: "private-program-consumer",
      path: "private-program/labcolors_private_program.wasm",
      bytes: privateWasm.length,
      sha256: createHash("sha256").update(privateWasm).digest("hex"),
    };
    const privateConsumerEvidence = {
      path: "private-program/consumer.js",
      bytes: privateConsumer.length,
      sha256: createHash("sha256").update(privateConsumer).digest("hex"),
    };
    const privateMetadata = {
      schemaVersion: 1,
      role: "private-program-consumer",
      package: { name: "@labpics/colors", version: packageVersion },
      source: {
        gitSha: expectedSha,
        core: {
          crate: "labcolors-core",
          version: coreVersion,
          digest: await privateProgramCoreSourceDigest(),
        },
      },
      build: PRIVATE_PROGRAM_CANONICAL_BUILD,
      artifacts: {
        consumer: privateConsumerEvidence,
        wasm: {
          path: privateWasmEvidence.path,
          bytes: privateWasmEvidence.bytes,
          sha256: privateWasmEvidence.sha256,
        },
      },
    };
    const privateMetadataBytes = Buffer.from(`${JSON.stringify(privateMetadata)}\n`);
    const privateMetadataPath = join(privateDirectory, "build-metadata.json");
    writeFileSync(privateMetadataPath, privateMetadataBytes);
    const privateMetadataEvidence = {
      path: "private-program/build-metadata.json",
      bytes: privateMetadataBytes.length,
      sha256: createHash("sha256").update(privateMetadataBytes).digest("hex"),
    };

    const packFixture = () => {
      const invocation = npmInvocation();
      const output = execFileSync(
        invocation.commandName,
        [
          ...invocation.argsPrefix,
          "pack",
          "--ignore-scripts",
          "--json",
          `--pack-destination=${artifact}`,
          payload,
        ],
        { cwd: temporary, encoding: "utf8", stdio: "pipe" },
      );
      const packed = JSON.parse(output);
      assert.equal(packed.length, 1);
      const tarball = join(artifact, packed[0].filename);
      return { tarball, bytes: readFileSync(tarball) };
    };
    const packedFixture = packFixture();
    const { tarball, bytes } = packedFixture;

    const manifest = {
      schemaVersion: 5,
      npm: packageVersion,
      core: coreVersion,
      wire: {
        identity: `resolved-theme@${packageVersion}`,
        embeddedInPayload: false,
        trackingIssue: 258,
      },
      conformance,
      normativeEvidence: {
        wcag22: {
          profileId: wcagProfile.profileId,
          profileChecksum: wcagProof.profile_checksum,
          artifactId: wcagProof.artifact_id,
          boundId: wcagProof.bound_id,
          proofId: wcagProof.proof_id,
          kernelId: wcagProof.kernel_id,
          terminalEvidenceId: wcagProof.terminal_evidence_id,
          parserId: wcagProof.parser_id,
          facadeId: wcagProof.facade_id,
          artifacts: WCAG22_EVIDENCE_FILES.map(evidenceArtifact),
        },
      },
      numericalEvidence: {
        pointSupportReferenceSurplus: {
          siteId: pointProof.site_id,
          profileId: pointProof.profile_id,
          artifactId: pointProof.artifact_id,
          boundId: pointProof.bound_id,
          proofId: pointProof.proof_id,
          proofSha256: createHash("sha256").update(pointProofBytes).digest("hex"),
          proofPayloadSha256: pointProof.proof_payload_sha256,
          declaredOperationLaw: pointProof.declared_operation_law,
          certifiedClaim: pointProof.certified_claim,
          excludedClaim: pointProof.excluded_claim,
          sourceBinding: {
            schemaVersion: pointProof.source_binding_schema_version,
            law: pointProof.source_binding_law,
            scope: pointProof.source_binding_scope,
            exclusions: pointProof.source_binding_exclusions,
            closureSha256: pointProof.source_closure_sha256,
          },
          q55Dependency: {
            artifactId: pointProof.q55_dependency.artifact_id,
            artifactSha256: pointProof.q55_dependency.artifact_sha256,
            proofId: pointProof.q55_dependency.proof_id,
            proofSha256: pointProof.q55_dependency.proof_sha256,
            proofPayloadSha256: pointProof.q55_dependency.proof_payload_sha256,
          },
          artifacts: POINT_SUPPORT_EVIDENCE_FILES.map(evidenceArtifact),
        },
      },
      sourceSha: expectedSha,
      reproducibility: {
        method: "same-executor-two-pass-npm-pack",
        passes: 2,
        byteIdentical: true,
      },
      requirements: {
        consumerRuntime: {
          node: ">=22.11.0",
          verifiedFloor: "22.11.0",
          canonicalGate: "Node 22 consumer floor",
        },
        buildToolchain: { node: process.versions.node, npm: "11.9.0" },
        typescript: {
          compiler: "5.9.3",
          minimumConsumerCompiler: "5.2.2",
          target: "ES2022",
          libraries: ["ES2022", "DOM"],
          skipLibCheck: false,
        },
      },
      supported: [
        "exact-alpha-srgb8-v1",
        "exact-screen-composite-srgb8-v1",
        "typed-glow-indeterminate-v1",
        "wcag22-srgb8-contrast-v1",
      ],
      numericalCapabilities: structuredClone(conformanceManifest.numericalCapabilities),
      unsupported: [
        "embedded-wire-schema-version",
        "stable-cam16-glow-target-or-maximum-selection",
        "renderer-or-output-pipeline-equivalence",
        "spatial-glow-field",
      ],
      artifacts: {
        tarball: {
          path: `.release/labpics-colors-${packageVersion}.tgz`,
          bytes: bytes.length,
          sha256: createHash("sha256").update(bytes).digest("hex"),
        },
        wasm: [...structuredClone(wasmEvidence), structuredClone(privateWasmEvidence)],
        buildMetadata: {
          path: "build-metadata.json",
          bytes: metadataBytes.length,
          sha256: createHash("sha256").update(metadataBytes).digest("hex"),
        },
        privateProgramConsumer: {
          role: "private-program-consumer",
          buildMetadata: privateMetadataEvidence,
          consumer: privateConsumerEvidence,
        },
      },
    };
    const manifestPath = join(artifact, "release-manifest.json");
    const output = join(temporary, "github-output");
    const fakeBin = join(temporary, "bin");
    mkdirSync(fakeBin);
    let expectedNpm = "11.9.0";
    let pythonBin = "";
    if (process.platform === "win32") {
      const linkOrCopy = (source, destination) => {
        try {
          linkSync(source, destination);
        } catch {
          copyFileSync(source, destination);
        }
      };
      linkOrCopy(process.execPath, join(fakeBin, "npm.exe"));
      expectedNpm = `v${process.versions.node}`;
      const pythonExecutable = execFileSync(
        "py",
        ["-3", "-c", "import sys; print(sys.executable)"],
        { encoding: "utf8", stdio: "pipe" },
      ).trim();
      pythonBin = dirname(pythonExecutable);
      // The validator resolves `python3` through PATH; a python.org Windows
      // install ships python.exe without python3.exe, so provide the alias in
      // fakeBin (first on PATH) instead of asserting a system python3.exe.
      linkOrCopy(pythonExecutable, join(fakeBin, "python3.exe"));
    } else {
      writeFileSync(join(fakeBin, "npm"), "#!/bin/sh\nprintf '11.9.0\\n'\n");
      chmodSync(join(fakeBin, "npm"), 0o755);
    }

    const execute = () => {
      const env = withControlledPath(
        process.env,
        [fakeBin, pythonBin, process.env.PATH ?? ""].filter(Boolean).join(delimiter),
      );
      Object.assign(env, {
        ARTIFACT_DIR: artifact,
        EXPECTED_SHA: expectedSha,
        EXPECTED_TAG: `colors-v${packageVersion}`,
        EXPECTED_NODE: process.versions.node,
        EXPECTED_NPM: expectedNpm,
        GITHUB_WORKSPACE: root,
        GITHUB_OUTPUT: output,
        RUNNER_TEMP: temporary,
      });
      return execFileSync(process.execPath, [validatorPath], {
        env,
        encoding: "utf8",
        stdio: "pipe",
        cwd: root,
      });
    };

    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    execute();
    const verifiedOutputs = new Map(
      readFileSync(output, "utf8")
        .trim()
        .split(/\r?\n/u)
        .map((line) => {
          // GITHUB_OUTPUT allows `=` inside values (e.g. Windows paths), so
          // split only at the first separator and keep the whole remainder.
          const separator = line.indexOf("=");
          assert.ok(separator > 0, `malformed GITHUB_OUTPUT line: ${line}`);
          return [line.slice(0, separator), line.slice(separator + 1)];
        }),
    );
    const verifiedTarball = verifiedOutputs.get("tarball");
    assert.ok(verifiedTarball && verifiedTarball !== tarball);
    assert.ok(readFileSync(verifiedTarball).equals(bytes));
    assert.equal(
      verifiedOutputs.get("sha256"),
      createHash("sha256").update(bytes).digest("hex"),
    );

    manifest.sourceSha = "b".repeat(40);
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);

    manifest.sourceSha = expectedSha;
    manifest.artifacts.tarball.sha256 = "0".repeat(64);
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);

    manifest.artifacts.tarball.sha256 = createHash("sha256").update(bytes).digest("hex");
    manifest.artifacts.wasm[0].bytes += 1;
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);

    manifest.artifacts.wasm[0].bytes -= 1;
    manifest.artifacts.wasm[0].role = "compiler";
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);

    manifest.artifacts.wasm[0].role = "runtime";
    manifest.artifacts.buildMetadata.sha256 = "0".repeat(64);
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);

    const tamperedMetadataBytes = Buffer.from(
      `${JSON.stringify({ ...buildMetadata, sourceSha: "b".repeat(40) })}\n`,
    );
    writeFileSync(metadataPath, tamperedMetadataBytes);
    const tamperedPack = packFixture();
    assert.equal(tamperedPack.tarball, tarball);
    const tamperedTarball = tamperedPack.bytes;
    manifest.artifacts.tarball.bytes = tamperedTarball.length;
    manifest.artifacts.tarball.sha256 = createHash("sha256")
      .update(tamperedTarball)
      .digest("hex");
    manifest.artifacts.buildMetadata.bytes = tamperedMetadataBytes.length;
    manifest.artifacts.buildMetadata.sha256 = createHash("sha256")
      .update(tamperedMetadataBytes)
      .digest("hex");
    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    assert.throws(execute, /Command failed/u);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("release verifier performs a same-executor byte-for-byte reproduction pass", () => {
  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /reproducibility/);
  assert.match(verifier, /method: "same-executor-two-pass-npm-pack"/u);
  assert.doesNotMatch(verifier, /two-independent-npm-pack-passes/u);
  assert.match(verifier, /byteIdentical: true/);
  assert.match(verifier, /const npmVersion = lockedNpmVersion\(packageJson\)/);
  assert.match(verifier, /consumerRuntime: \{/);
  assert.match(verifier, /verifiedFloor: consumerNodeFloor/);
  assert.match(verifier, /canonicalGate: "Node 22 consumer floor"/);
  assert.match(verifier, /buildToolchain: \{/);
  assert.match(verifier, /node: process\.versions\.node/);
  assert.match(verifier, /GITHUB_OUTPUT/);
  assert.match(verifier, /familySetSha256: sha256\(Buffer\.concat\(familyBuffers\)\)/);
  assert.match(verifier, /sha256: sha256\(familyBuffers\[index\]\)/);
  assert.match(verifier, /numericalCapabilities: conformance\.numericalCapabilities/);
  assert.match(
    verifier,
    /CAPABILITY_CHECKSUM_DOMAIN_V2 = "labcolors\.numerical-capability\.v2"/,
  );
  assert.match(verifier, /capabilities\.schemaVersion !== 2/);
  assert.doesNotMatch(
    verifier,
    /numericalCapabilities:\s*\{\s*"/,
    "release manifest must copy the generated capability manifest, not duplicate it",
  );
});

test("conformance pack 10 has the exact canonical family inventory", () => {
  const canonicalFamilies = new Map([
    ["contrasts.json", "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145"],
    ["ladders.json", "496f562e55ad8110aeb8a07042b1964ec9ff4d0f1e8c09e362d1b2d14c513036"],
    ["alpha.json", "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1"],
    ["wcag22.json", "8b2e44feba985a6f0017d4192c1c03fcc5c22da1d7d86df91dcb5bb214de7ab1"],
  ]);
  assert.equal(canonicalFamilies.size, 4, "anti-vacuum: canonical family set changed");
  for (const removed of [
    "wcag22-explicit-selection.json",
    "wcag22-feasibility.json",
    "muddiness.json",
  ]) {
    assert.ok(
      !existsSync(join(root, "conformance", "vectors", removed)),
      `${removed} must be gone, not regenerated`,
    );
  }
  for (const [name, expected] of canonicalFamilies) {
    const bytes = readFileSync(join(root, "conformance", "vectors", name));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected, name);
  }
  assert.equal(
    createHash("sha256")
      .update(readFileSync(join(root, "conformance", "vectors", "solve.json")))
      .digest("hex"),
    "db04e50698cc3b10223f4005f74dd35cc5ae0a29988825e44db5c985aa9207af",
    "canonical solve family bytes drifted",
  );

  const manifest = JSON.parse(read("conformance", "vectors", "manifest.json"));
  assert.equal(manifest.packVersion, "10.0.0");
  const solve = JSON.parse(read("conformance", "vectors", "solve.json"));
  const supersededKind = ["un", "reachable"].join("");
  const failures = solve.filter(({ outcome }) => outcome.kind === "failure");
  assert.ok(failures.length > 0, "anti-vacuum: solve family has no typed failure");
  const failurePairs = new Set();
  for (const { outcome } of failures) {
    assert.deepEqual(
      Object.keys(outcome).sort(),
      ["category", "code", "kind"],
      "failure wire must be exactly {kind,category,code}",
    );
    assert.equal(outcome.category, "unreachable");
    failurePairs.add(`${outcome.category}/${outcome.code}`);
  }
  assert.deepEqual(
    [...failurePairs].sort(),
    [
      "unreachable/below_contrast_floor",
      "unreachable/exceeds_range",
      "unreachable/floor_unreachable",
    ],
  );
  assert.equal(
    solve.some(({ outcome }) => outcome.kind === supersededKind),
    false,
    "the current pack must not preserve the superseded failure kind",
  );
  assert.equal(
    manifest.counts.total,
    Object.entries(manifest.counts)
      .filter(([key]) => key !== "total")
      .reduce((sum, [, value]) => sum + value, 0),
    "manifest total must equal the sum of every family count",
  );
});

test("release checker rejects solve failure wire drift", () => {
  const canonical = JSON.parse(read("conformance", "vectors", "solve.json"));
  assert.doesNotThrow(() => validateSolveFamily(canonical));

  const mutations = [
    ["missing category", (outcome) => delete outcome.category],
    ["wrong category", (outcome) => { outcome.category = "unresolved"; }],
    ["old kind", (outcome) => { outcome.kind = ["un", "reachable"].join(""); }],
    ["extra field", (outcome) => { outcome.reason = "plausible fallback"; }],
    ["internal category", (outcome) => { outcome.category = "internal"; }],
    ["unknown code", (outcome) => { outcome.code = "future_guess"; }],
  ];
  assert.equal(mutations.length, 6, "anti-vacuum mutation corpus changed");
  for (const [name, mutate] of mutations) {
    const family = structuredClone(canonical);
    const failure = family.find(({ outcome }) => outcome.kind === "failure")?.outcome;
    assert.ok(failure, "anti-vacuum: solve family has no failure fixture");
    mutate(failure);
    assert.throws(() => validateSolveFamily(family), undefined, name);
  }

  const successesOnly = canonical.filter(({ outcome }) => outcome.kind === "solved");
  assert.throws(
    () => validateSolveFamily(successesOnly),
    /exercise both outcomes/u,
    "removing the failure branch must fail closed",
  );

  const boundaryRows = [
    ["unreachable", "below_contrast_floor"],
    ["unreachable", "exceeds_range"],
    ["unresolved", "bounded_search_exhausted"],
    ["unreachable", "floor_unreachable"],
    ["rejected", "invalid_input"],
  ];
  assert.equal(boundaryRows.length, 5, "public core failure dictionary changed");
  for (const [category, code] of boundaryRows) {
    assert.doesNotThrow(() => validateSolveFailurePair(category, code));
    const wrongCategory = category === "unreachable" ? "rejected" : "unreachable";
    assert.throws(
      () => validateSolveFailurePair(wrongCategory, code),
      /differs from/u,
      `${code} category mutation must bite`,
    );
  }
});

test("release checker rejects solved payload drift", () => {
  const canonical = JSON.parse(read("conformance", "vectors", "solve.json"));
  assert.doesNotThrow(() => validateSolveFamily(canonical));

  const solvedFields = ["floorOverride", "hex", "kind", "lc", "wcagRatio"];
  const set = (field, value) => (outcome) => { outcome[field] = value; };
  const drop = (field) => (outcome) => { delete outcome[field]; };
  const fieldsError = (actual) => ({
    message: `solve[0].outcome fields ${JSON.stringify(actual)} differ from ${JSON.stringify(solvedFields)}`,
  });
  const hexError = { message: "solve[0].outcome.hex must be canonical #RRGGBB" };
  const lcError = { message: "solve[0].outcome.lc must be finite" };
  const ratioError = {
    message: "solve[0].outcome.wcagRatio must be finite and within [1, 21]",
  };
  const mutations = [
    ["missing hex", drop("hex"), fieldsError(solvedFields.filter((key) => key !== "hex"))],
    ["extra solved field", set("note", "plausible fallback"), fieldsError([...solvedFields, "note"].sort())],
    ["unknown solved kind", set("kind", "success"), { message: "solve[0].outcome has unsupported kind success" }],
    ["hex type", set("hex", 0x767676), hexError],
    ["hex prefix", set("hex", "C4C4C4"), hexError],
    ["hex length", set("hex", "#C4C4C"), hexError],
    ["hex uppercase", set("hex", "#c4c4c4"), hexError],
    ["hex alphabet", set("hex", "#GGGGGG"), hexError],
    ["lc type", set("lc", "68.2"), lcError],
    ["non-finite lc", set("lc", Number.NaN), lcError],
    ["infinite lc", set("lc", Number.POSITIVE_INFINITY), lcError],
    ["ratio type", set("wcagRatio", "4.5"), ratioError],
    ["non-finite ratio", set("wcagRatio", Number.NaN), ratioError],
    ["infinite ratio", set("wcagRatio", Number.POSITIVE_INFINITY), ratioError],
    ["ratio below physical range", set("wcagRatio", 0.99), ratioError],
    ["ratio above physical range", set("wcagRatio", 21.01), ratioError],
    ["floor override type", set("floorOverride", null), { message: "solve[0].outcome.floorOverride must be boolean" }],
  ];
  assert.equal(mutations.length, 17, "solved anti-vacuum mutation corpus changed");
  for (const [name, mutate, expected] of mutations) {
    const family = structuredClone(canonical);
    const solved = family.find(({ outcome }) => outcome.kind === "solved")?.outcome;
    assert.ok(solved, "anti-vacuum: solve family has no solved fixture");
    // In-memory mutation intentionally preserves NaN; a JSON round-trip would coerce it to null.
    mutate(solved);
    assert.throws(() => validateSolveFamily(family), expected, name);
  }

  for (const ratio of [1, 21]) {
    const family = structuredClone(canonical);
    family.find(({ outcome }) => outcome.kind === "solved").outcome.wcagRatio = ratio;
    assert.doesNotThrow(
      () => validateSolveFamily(family),
      `inclusive WCAG ratio boundary ${ratio} must remain valid`,
    );
  }

  const failuresOnly = canonical.filter(({ outcome }) => outcome.kind === "failure");
  assert.throws(
    () => validateSolveFamily(failuresOnly),
    /got solved=0 failure=5/u,
    "removing the solved branch must fail closed",
  );
});

test("release evidence carries no trace of the excised offline line", () => {
  const prepare = read("scripts", "prepare-npm-package.mjs");
  const verifier = read("scripts", "verify-package-release.mjs");

  assert.doesNotMatch(prepare, /feasibility|labcolors-compiler|wcag22-explicit|muddiness/iu);
  assert.doesNotMatch(
    verifier,
    /feasibility|labcolors-compiler|wcag22-explicit|verifyPackedRoleIsolation|muddiness/iu,
  );
  assert.doesNotMatch(verifier, /from "@labpics\/colors\/compiler"/u);
  assert.match(verifier, /conformance\.packVersion !== "10\.0\.0"/u);
  assert.match(verifier, /validateSolveFamily\(families\[3\]\)/u);
  assert.match(
    verifier,
    /countKeys = \["contrasts", "ladders", "alpha", "solve", "wcag22"\]/u,
    "release count projection must cover exactly the five surviving families",
  );
  assert.match(
    verifier,
    /"wcag22-srgb8-contrast-v1",\n    \],/u,
    "supported list must end at the exact runtime evaluator capability",
  );
});

test("WASM runtime budget is one canonical self-contained exact contract", async () => {
  const bench = join(root, "packages", "colors", "bench");
  const budgetPath = join(bench, "wasm.json");
  const checkerPath = join(root, "scripts", "check-wasm-size-budget.mjs");
  const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
  const sha256 = (value) => createHash("sha256").update(value).digest("hex");
  assert.deepEqual(
    readdirSync(bench).filter((name) => /^wasm-size-budget-v\d+\.json$/u.test(name)),
    [],
    "numbered WASM budget snapshots duplicate Git history",
  );

  const budgetBytes = readFileSync(budgetPath);
  const budget = JSON.parse(budgetBytes);
  assert.equal(budgetBytes.toString("utf8"), canonicalJson(budget));
  assert.doesNotMatch(
    budgetBytes.toString("utf8"),
    /predecessor|toolchainSource|wasm-size-budget-v/u,
    "the current contract must be self-contained instead of linking Git history",
  );

  const checker = await import(
    new URL("../../../scripts/check-wasm-size-budget.mjs", import.meta.url)
  );
  assert.equal(checker.DEFAULT_BUDGET, budgetPath);
  assert.equal(sha256(budgetBytes), checker.WASM_BUDGET_FILE_SHA256);
  assert.deepEqual(
    checker.parseBudgetDocument(budgetBytes, budgetPath),
    budget,
    "the pinned canonical document must parse",
  );

  const ci = read(".github", "workflows", "ci-worker.yml");
  assert.match(ci, /name: enforce measured WASM runtime budget/u);
  const exactBudgetCommand = "        run: node scripts/check-wasm-size-budget.mjs";
  const assertExactBudgetCommand = (workflow) => {
    assert.deepEqual(
      workflow
        .split("\n")
        .filter((line) => line.includes("run: node scripts/check-wasm-size-budget.mjs")),
      [exactBudgetCommand],
      "CI must execute the canonical budget and built artifact without overrides",
    );
  };
  assertExactBudgetCommand(ci);
  for (const bypass of [
    `${exactBudgetCommand} --budget attacker.json`,
    `${exactBudgetCommand} --runtime-wasm attacker.wasm`,
  ]) {
    const mutated = ci.replace(exactBudgetCommand, bypass);
    assert.notEqual(mutated, ci, "budget CLI mutation must bite the live workflow");
    assert.throws(() => assertExactBudgetCommand(mutated));
  }

  const wasmJob = ci.match(
    /\n  wasm:\n(?<body>[\s\S]*?)(?=\n  [a-z][a-z0-9_-]*:\n|\s*$)/u,
  )?.groups?.body;
  assert.ok(wasmJob, "CI must contain a bounded wasm job");
  assert.match(
    wasmJob,
    /runs-on: ubuntu-latest/u,
    "the wasm job must run on an ephemeral GitHub-hosted runner",
  );
  assert.doesNotMatch(wasmJob, /runs-on: \[self-hosted, Linux, X64\]/u);
  assert.doesNotMatch(wasmJob, /labpics-ci-sandbox/u);
  assert.ok(
    ci.includes(`  RUST_TOOLCHAIN: ${budget.toolchain.rust}`),
    "the live Rust toolchain must equal the budget declaration",
  );
  assert.ok(
    ci.includes(`  NODE_TOOLCHAIN: ${budget.toolchain.node}`),
    "the live Node toolchain must equal the budget declaration",
  );
  assert.ok(
    wasmJob.includes(
      `cargo install wasm-pack --version ${budget.toolchain.wasmPack} --locked`,
    ),
    "the live wasm-pack toolchain must equal the budget declaration",
  );
  const bindgenInstall = workflowRunScript(ci, "name: install locked wasm-bindgen CLI");
  assert.match(bindgenInstall, /^set -euo pipefail$/mu);
  const bindgenCommand =
    /cargo install wasm-bindgen-cli --version (?<version>\S+) --locked \\\n  --root "\$WASM_PACK_CACHE\/\.wasm-bindgen-cargo-install-(?<rootVersion>[^"/]+)"/u
      .exec(bindgenInstall)?.groups;
  assert.ok(
    bindgenCommand?.version === budget.toolchain.wasmBindgen &&
      bindgenCommand.rootVersion === budget.toolchain.wasmBindgen,
    "wasm-pack must consume a lockfile-resolved wasm-bindgen CLI",
  );
  assert.ok(
    ci.indexOf("name: install locked wasm-bindgen CLI") <
      ci.indexOf("name: repeat runtime WASM build in one toolchain-pinned CI job"),
    "the locked wasm-bindgen CLI must exist before wasm-pack builds the runtime",
  );
  assert.ok(
    wasmJob.includes(`targets: ${budget.toolchain.target}`),
    "the live WASM target must equal the budget declaration",
  );
  assert.ok(
    wasmJob.includes(`BINARYEN_RELEASE: ${budget.toolchain.binaryenRelease}`),
    "the live Binaryen release must equal the budget declaration",
  );
  assert.ok(
    wasmJob.includes(
      `BINARYEN_NODE_SHA256: "${budget.toolchain.binaryenNodeArchiveSha256}"`,
    ),
    "the live Binaryen archive must equal the budget declaration",
  );

  const repetition = workflowRunScript(
    ci,
    "name: repeat runtime WASM build in one toolchain-pinned CI job",
  );
  const expectedRemapExport = `export CARGO_ENCODED_RUSTFLAGS=${budget.recipe.rustPathRemap
    .map((mapping) => {
      const separator = mapping.indexOf("=");
      assert.ok(separator > 0, "path remap must name one environment source");
      return `"--remap-path-prefix=\$${mapping.slice(0, separator)}=${mapping.slice(separator + 1)}"`;
    })
    .join("$'\\x1f'")}`;
  const logicalShellCommands = (body) => {
    const commands = [];
    let current = "";
    for (const line of body.split("\n")) {
      const fragment = line.trim();
      if (fragment.length === 0) continue;
      const continued = fragment.endsWith("\\");
      const withoutContinuation = continued ? fragment.slice(0, -1).trimEnd() : fragment;
      current += `${current.length === 0 ? "" : " "}${withoutContinuation}`;
      if (!continued) {
        commands.push(current);
        current = "";
      }
    }
    assert.equal(current, "", "recipe must not end with an unterminated shell continuation");
    return commands;
  };
  const expectedDiffBlock = [
    'if ! diff --no-dereference --recursive "$first/pkg" packages/colors/pkg; then',
    '  echo "runtime WASM output changed between builds" >&2',
    "  exit 1",
    "fi",
  ].join("\n");
  const expectedPathGuard = [
    'for root in "$GITHUB_WORKSPACE" "$CARGO_HOME" "$RUSTUP_HOME"; do',
    '  if LC_ALL=C grep -a -F -q -- "$root/" "$wasm"; then',
    '    echo "unmapped build path $root in $wasm" >&2',
    "    exit 1",
    "  fi",
    "done",
  ].join("\n");
  const assertRepeatabilityContract = (script) => {
    assert.match(script, /^set -euo pipefail$/mu);
    assert.deepEqual(
      script.split("\n").filter((line) => line.startsWith("export CARGO_ENCODED_RUSTFLAGS=")),
      [expectedRemapExport],
      "the live path remap must equal the budget declaration",
    );
    const functionBody = script.match(
      /(?:^|\n)build_runtime\(\) \{\n(?<body>(?:  [^\n]+\n)+)\}/u,
    )?.groups?.body;
    assert.ok(functionBody, "build_runtime must be one bounded shell function");
    assert.deepEqual(
      logicalShellCommands(functionBody),
      budget.recipe.commands,
      "the live build must equal the budget recipe",
    );
    assert.equal(
      budget.recipe.commands.filter((command) =>
        command.includes(budget.toolchain.wasmOptFlags)
      ).length,
      1,
      "the declared Binaryen flags must occur in exactly one recipe command",
    );
    assert.equal(
      script.match(/^build_runtime$/gmu)?.length,
      2,
      "the exact recipe must run twice",
    );
    assert.match(script, /^cargo clean$/mu);
    assert.match(script, /^cp -a packages\/colors\/pkg "\$first\/pkg"$/mu);
    assert.deepEqual(
      [...script.matchAll(/^if ! diff[^\n]+\n  echo [^\n]+\n  exit 1\nfi$/gmu)]
        .map((match) => match[0]),
      [expectedDiffBlock],
      "both generated directories must be compared fail-closed",
    );
    assert.equal(
      script.match(
        /^for root in "\$GITHUB_WORKSPACE" "\$CARGO_HOME" "\$RUSTUP_HOME"; do\n  if LC_ALL=C grep -a -F -q -- "\$root\/" "\$wasm"; then\n    echo "unmapped build path \$root in \$wasm" >&2\n    exit 1\n  fi\ndone$/mu,
      )?.[0],
      expectedPathGuard,
      "host paths must remain rejected fail-closed",
    );
  };
  assertRepeatabilityContract(repetition);

  for (const [name, mutated] of [
    [
      "recipe",
      repetition.replace(
        "    crates/labcolors-wasm --locked",
        "    crates/labcolors-wasm --locked --features unreviewed",
      ),
    ],
    [
      "rebuild comparison",
      repetition.replace(expectedDiffBlock, expectedDiffBlock.replace("  exit 1", "  :")),
    ],
    [
      "path guard",
      repetition.replace(expectedPathGuard, expectedPathGuard.replace("    exit 1", "    :")),
    ],
  ]) {
    assert.notEqual(mutated, repetition, `${name} mutation must bite live CI`);
    assert.throws(() => assertRepeatabilityContract(mutated));
  }

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-wasm-runtime-budget-"));
  try {
    const runtimePath = join(temporary, "runtime.wasm");
    const fixtureBudgetPath = join(temporary, "budget.json");
    const runtimeBytes = Buffer.alloc(16);
    runtimeBytes.set([0x00, 0x61, 0x73, 0x6d]);
    const fixture = structuredClone(budget);
    fixture.measurement.rawBytes = runtimeBytes.length;
    fixture.policy.maxRawBytes = runtimeBytes.length;
    writeFileSync(runtimePath, runtimeBytes);
    writeFileSync(fixtureBudgetPath, canonicalJson(fixture));
    assert.doesNotThrow(() =>
      checker.parseBudgetDocument(readFileSync(fixtureBudgetPath), fixtureBudgetPath)
    );

    const runWith = (fixturePath, wasmPath) => execFileSync(
      process.execPath,
      [
        checkerPath,
        "--budget",
        fixturePath,
        "--runtime-wasm",
        wasmPath,
      ],
      { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    const run = () => runWith(fixtureBudgetPath, runtimePath);
    assert.match(
      run(),
      /role=runtime raw=16B .*artifact-sha256=[0-9a-f]{64}/u,
    );

    const schemaMutations = [
      ["schema", (value) => { value.schemaVersion = 0; }],
      ["artifact", (value) => { value.artifact = "packages/colors/pkg/other.wasm"; }],
      ...Object.keys(budget.toolchain).map((key) => [
        `toolchain.${key}`,
        (value) => { value.toolchain[key] = ""; },
      ]),
      ["toolchain line", (value) => { value.toolchain.rust += "\nother"; }],
      ["toolchain extra", (value) => { value.toolchain.extra = "forbidden"; }],
      ["path remap syntax", (value) => { value.recipe.rustPathRemap[0] = "OTHER"; }],
      ["path remap duplicate", (value) => {
        value.recipe.rustPathRemap[1] = value.recipe.rustPathRemap[0];
      }],
      ["path remap empty", (value) => { value.recipe.rustPathRemap = []; }],
      ["recipe commands type", (value) => { value.recipe.commands = "wasm-pack build"; }],
      ["recipe commands empty", (value) => { value.recipe.commands = []; }],
      ["recipe commands duplicate", (value) => {
        value.recipe.commands[1] = value.recipe.commands[0];
      }],
      ["recipe command line", (value) => { value.recipe.commands[0] += "\nother"; }],
      ["recipe extra", (value) => { value.recipe.digest = "0".repeat(64); }],
      ["measurement source", (value) => { value.measurement.source = "other"; }],
      ["measurement platform", (value) => { value.measurement.platform = "darwin-arm64"; }],
      ["zero bytes", (value) => { value.measurement.rawBytes = 0; }],
      ["fractional bytes", (value) => { value.measurement.rawBytes = 1.5; }],
      ["artifact digest conflation", (value) => {
        value.measurement.sha256 = "0".repeat(64);
      }],
      ["headroom", (value) => { value.policy.maxRawBytes += 1; }],
      ["basis", (value) => { value.policy.basis = ""; }],
      ["basis line", (value) => { value.policy.basis += "\nother"; }],
      ["gzip gate", (value) => { value.policy.gzip = "gate"; }],
      ["missing policy", (value) => { delete value.policy; }],
      ["history link", (value) => { value.predecessor = "forbidden"; }],
      ["top-level reorder", (value) => ({
        artifact: value.artifact,
        schemaVersion: value.schemaVersion,
        toolchain: value.toolchain,
        recipe: value.recipe,
        measurement: value.measurement,
        policy: value.policy,
      })],
    ];
    assert.equal(schemaMutations.length, 34, "schema mutation matrix changed");
    for (const [name, mutate] of schemaMutations) {
      const invalid = structuredClone(fixture);
      const result = mutate(invalid) ?? invalid;
      assert.throws(
        () => checker.parseBudgetDocument(
          Buffer.from(canonicalJson(result)),
          fixtureBudgetPath,
        ),
        /WASM size budget:/u,
        `${name} must fail before artifact evaluation`,
      );
    }

    const schemaFirst = structuredClone(fixture);
    schemaFirst.schemaVersion = 0;
    writeFileSync(fixtureBudgetPath, canonicalJson(schemaFirst));
    assert.throws(
      () => runWith(fixtureBudgetPath, join(temporary, "missing-runtime.wasm")),
      /schemaVersion must be 2/u,
      "schema must fail before a missing artifact is read",
    );

    writeFileSync(fixtureBudgetPath, `${JSON.stringify(fixture)}\n`);
    assert.throws(run, /canonical JSON/u, "non-canonical JSON must fail");
    writeFileSync(
      fixtureBudgetPath,
      canonicalJson(fixture).replace(
        '  "schemaVersion": 2,\n',
        '  "schemaVersion": 2,\n  "schemaVersion": 2,\n',
      ),
    );
    assert.throws(run, /canonical JSON/u, "duplicate JSON fields must fail");

    const canonical = checker.evaluateWasmBudget(fixture, runtimeBytes, "linux-x64");
    assert.equal(canonical.status, "PASS");
    assert.equal(canonical.artifactSha256, sha256(runtimeBytes));
    const sameSizeMutation = Buffer.from(runtimeBytes);
    sameSizeMutation[sameSizeMutation.length - 1] = 1;
    const sameSize = checker.evaluateWasmBudget(
      fixture,
      sameSizeMutation,
      "linux-x64",
    );
    assert.equal(sameSize.status, "PASS");
    assert.notEqual(sameSize.artifactSha256, canonical.artifactSha256);
    for (const differentSize of [
      Buffer.concat([runtimeBytes, Buffer.from([0])]),
      runtimeBytes.subarray(0, -1),
    ]) {
      assert.throws(
        () => checker.evaluateWasmBudget(fixture, differentSize, "linux-x64"),
        /length mismatch/u,
      );
    }
    assert.equal(
      checker.evaluateWasmBudget(fixture, sameSizeMutation, "darwin-arm64").status,
      "DIAGNOSTIC",
    );
    assert.throws(
      () => checker.evaluateWasmBudget(fixture, Buffer.alloc(16), "linux-x64"),
      /not a WebAssembly binary/u,
    );

    const coordinatedMutation = structuredClone(fixture);
    coordinatedMutation.measurement.rawBytes -= 1;
    coordinatedMutation.policy.maxRawBytes -= 1;
    assert.throws(
      () => checker.parseBudgetDocument(
        Buffer.from(canonicalJson(coordinatedMutation)),
        budgetPath,
      ),
      /current budget file SHA-256 mismatch/u,
      "coordinated contract drift must still fail the canonical file identity",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("runtime WASM does not duplicate separately shipped numerical evidence documents", () => {
  const wasm = readFileSync(
    join(root, "packages", "colors", "pkg", "labcolors_bg.wasm"),
  );
  for (const name of [
    "wcag22-srgb8-v1.json",
    "wcag22-srgb8-q55-proof-v1.json",
    "point-support-reference-surplus-q55-bps-proof-v1.json",
  ]) {
    const evidence = readFileSync(
      join(root, "crates", "labcolors-core", "contracts", name),
    );
    assert.equal(
      wasm.indexOf(evidence),
      -1,
      `${name} belongs in npm evidence/, not the runtime WASM`,
    );
  }
});

test("npm release carries and re-verifies the exact numerical evidence inventory", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const evidenceFiles = [
    "evidence/point-support-reference-surplus-q55-bps-proof-v1.json",
    "evidence/wcag22-srgb8-q55-proof-v1.json",
    "evidence/wcag22-srgb8-q55-v1.bin",
    "evidence/wcag22-srgb8-v1.json",
  ].sort();
  assert.deepEqual([...PACKED_NUMERICAL_EVIDENCE_PATHS].sort(), evidenceFiles);
  assert.deepEqual(
    packageJson.files.filter((path) => path.startsWith("evidence/")).sort(),
    evidenceFiles,
  );
  assert.deepEqual(
    [...NUMERICAL_EVIDENCE_FILES].sort(),
    evidenceFiles.map((path) => path.slice("evidence/".length)),
  );
  assert.deepEqual([...WCAG22_EVIDENCE_FILES].sort(), [
    "wcag22-srgb8-q55-proof-v1.json",
    "wcag22-srgb8-q55-v1.bin",
    "wcag22-srgb8-v1.json",
  ]);
  assert.deepEqual([...POINT_SUPPORT_EVIDENCE_FILES], [
    "point-support-reference-surplus-q55-bps-proof-v1.json",
  ]);

  const artifact = join(
    root,
    "crates",
    "labcolors-core",
    "contracts",
    "wcag22-srgb8-q55-v1.bin",
  );
  assert.ok(existsSync(artifact), "canonical Q55 binary artifact is absent");
  assert.equal(lstatSync(artifact).size, 768 * 2 * 8, "artifact must be 1536 little-endian u64s");

  const prepare = read("scripts", "prepare-npm-package.mjs");
  assert.match(prepare, /from "\.\/release-evidence\.mjs"/u);
  assert.match(prepare, /for \(const file of NUMERICAL_EVIDENCE_FILES\)/u);
  assert.match(prepare, /assertPackageEvidenceInventory\(packageJson\.files\)/u);

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /verify_wcag22_q55\.py/);
  assert.match(verifier, /verify_point_support_surplus\.py/);
  assert.match(verifier, /NUMERICAL_EVIDENCE_FILES/);
  const numericalVerifier = read("scripts", "verify_wcag22_q55.py");
  assert.match(numericalVerifier, /NORMATIVE_PROFILE_V1/);
  assert.ok(
    numericalVerifier.includes(String.raw`r'\1"<self-digest>"'`),
    "facade normalization must preserve the literal regex backreference",
  );
  assert.ok(
    !numericalVerifier.includes(String.raw`rf'\1"<self-digest>"'`),
    "a replacement without interpolation must not use an f-string",
  );
  const conformanceReadme = read("conformance", "README.md");
  assert.match(conformanceReadme, /manifest\.packVersion`, сейчас `10\.0\.0`/u);
  assert.match(
    conformanceReadme,
    /crates\/labcolors-conformance\/tests\/pack_v10_contract\.rs/u,
  );
  assert.doesNotMatch(
    conformanceReadme,
    /Предыдущий bump|→ 10\.0\.0|→ 9\.0\.0/u,
  );
  assert.match(conformanceReadme, /`wcag22\.json`/u);
  assert.doesNotMatch(conformanceReadme, /`wcag22-explicit-selection\.json`|`wcag22-feasibility\.json`|`muddiness\.json`/u);
  assert.match(
    conformanceReadme,
    /contrasts, ladders, alpha, solve, wcag22/u,
  );
  assert.doesNotMatch(conformanceReadme, /сейчас `[3-9]\.0\.0`/u);
  const workflow = read(".github", "workflows", "ci-worker.yml");
  assert.match(workflow, /python3 scripts\/verify_wcag22_q55\.py/);
  assert.match(workflow, /python3 scripts\/verify_point_support_surplus\.py/);
});

test("packed and clean-installed numerical evidence stays byte-exact", async () => {
  const names = [...NUMERICAL_EVIDENCE_FILES];
  const contents = names.map((name) =>
    readFileSync(join(root, "crates", "labcolors-core", "contracts", name))
  );
  const expected = names.map((name, index) => ({
    path: `evidence/${name}`,
    bytes: contents[index].length,
    sha256: createHash("sha256").update(contents[index]).digest("hex"),
  }));
  const temporary = mkdtempSync(join(tmpdir(), "labcolors-evidence-boundary-"));
  try {
    const evidenceDir = join(temporary, "evidence");
    mkdirSync(evidenceDir);
    for (const [index, name] of names.entries()) {
      writeFileSync(join(evidenceDir, name), contents[index]);
    }
    await assert.doesNotReject(
      validateNumericalEvidenceArtifacts(temporary, expected, "fixture"),
    );

    for (const index of [0, names.length - 1]) {
      const corrupted = Buffer.from(contents[index]);
      corrupted[0] ^= 1;
      writeFileSync(join(evidenceDir, names[index]), corrupted);
      await assert.rejects(
        validateNumericalEvidenceArtifacts(temporary, expected, "fixture"),
        /fixture numerical evidence bytes differ/u,
        `same-length evidence corruption must fail for ${names[index]}`,
      );

      writeFileSync(join(evidenceDir, names[index]), contents[index]);
      const wrongDigest = structuredClone(expected);
      wrongDigest[index].sha256 = "0".repeat(64);
      await assert.rejects(
        validateNumericalEvidenceArtifacts(temporary, wrongDigest, "fixture"),
        /fixture numerical evidence metadata differs/u,
        `expected digest drift must fail independently for ${names[index]}`,
      );
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(
    verifier,
    /validatePackedNumericalEvidence\(canonicalPack\.bytes, numericalEvidenceArtifacts\)/u,
  );
  assert.match(
    verifier,
    /verifyCleanConsumer\(\s*canonicalPack\.bytes,[\s\S]*?numericalEvidenceArtifacts[\s\S]*?\);/u,
  );
});

test("Swift capability mirror transports proof IDs in the canonical checksum order", () => {
  const swift = read(
    "bindings",
    "swift",
    "Tests",
    "LabColorsConformanceTests",
    "ConformanceTests.swift",
  );
  assert.match(swift, /let proofIds: \[String\]/);
  assert.match(
    swift,
    /pushSortedKeyList\(site\.boundIds\)\s+pushSortedKeyList\(site\.proofIds\)\s+pushSortedKeyList\(site\.runtimeAttestations\)/u,
  );
});

test("published build metadata binds source, conformance, and WASM inputs", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  assert.equal(
    packageJson.exports["./build-metadata.json"],
    "./build-metadata.json",
    "build metadata must be a consumer-visible subpath export",
  );
  assert.ok(packageJson.files.includes("build-metadata.json"));

  const prepare = read("scripts", "prepare-npm-package.mjs");
  assert.match(prepare, /import \{ workspaceVersion \} from "\.\/cargo-workspace\.mjs";/);
  // The metadata builder needs the local sha256 digest helper at runtime; its
  // removal must fail here, not later as a ReferenceError inside the release
  // gate.
  assert.match(
    prepare,
    /const sha256 = \(bytes\) => createHash\("sha256"\)\.update\(bytes\)\.digest\("hex"\);/u,
  );
  assert.match(prepare, /import \{ createHash \} from "node:crypto";/u);
  assert.match(prepare, /const BUILD_METADATA = resolve\(PACKAGE_DIR, "build-metadata\.json"\)/);
  assert.match(prepare, /sourceSha/);
  assert.ok(
    prepare.indexOf("const sourceSha = verifiedSourceSha()") <
      prepare.indexOf("atomicWriteGeneratedFile(PACKED_LICENSE"),
    "source guard must run before generated packing inputs are written",
  );
  assert.match(prepare, /--porcelain=v1/);
  assert.match(prepare, /--untracked-files=normal/);
  assert.match(prepare, /GITHUB_SHA .* does not equal checked-out HEAD/);
  assert.match(prepare, /coreVersion/);
  assert.match(prepare, /packVersion: conformance\.packVersion/);
  assert.match(prepare, /packDigest: conformance\.packDigest/);
  assert.match(prepare, /manifestSha256: sha256\(Buffer\.from\(conformanceSource\)\)/);
  assert.match(prepare, /familySetSha256: sha256\(Buffer\.concat\(familyBytes\)\)/);
  assert.match(prepare, /schemaVersion: 2/u);
  assert.match(prepare, /role: "runtime"[\s\S]*?path: "pkg\/labcolors_bg\.wasm"/u);
  assert.doesNotMatch(prepare, /role: "compiler"|compilerWasm/u);
  assert.match(prepare, /bytes: runtimeWasm\.length/u);
  assert.match(prepare, /sha256: sha256\(runtimeWasm\)/u);
  assert.match(
    prepare,
    /assertWasm\(runtimeWasm, "pkg\/labcolors_bg\.wasm"\)/u,
    "prepack must reject a malformed public runtime WASM before writing metadata",
  );
  assert.match(
    prepare,
    /assertWasm\(privateProgramWasm, PRIVATE_PROGRAM_WASM_PATH\)/u,
  );

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /import \{ workspaceVersion \} from "\.\/cargo-workspace\.mjs";/);
  assert.match(verifier, /function validateBuildMetadata/);
  assert.match(verifier, /isDeepStrictEqual\(metadata, expected\)/);
  assert.match(verifier, /require\.resolve\("@labpics\/colors\/build-metadata\.json"\)/);
  assert.doesNotMatch(verifier, /@labpics\/colors\/compiler\/wasm/u);
  assert.match(verifier, /installedBuildMetadata/);
  assert.match(verifier, /isDeepStrictEqual\(installedBuildMetadata, expectedBuildMetadata\)/);
  assert.match(verifier, /"--offline"/);
  assert.match(verifier, /packageDirectory: "typescript"/u);
  assert.match(verifier, /packageDirectory: "typescript-floor"/u);
  assert.match(verifier, /compiler\.packageDirectory,[\s\S]*?"package\.json"/u);
  assert.doesNotMatch(verifier, /`typescript@\$\{typescriptVersion\}`/);
  assert.match(verifier, /"--lib",\s+"ES2022,DOM"/u);
  assert.doesNotMatch(verifier, /ES2022,DOM,ESNext\.Disposable/u);
  assert.match(verifier, /libraries: \["ES2022", "DOM"\]/u);
  assert.match(verifier, /role: "runtime", \.\.\.wasm\.runtime/u);
  assert.doesNotMatch(verifier, /role: "compiler"/u);
  assert.match(verifier, /buildMetadata,/u);
});

test("clean consumer smoke executes root and public-subpath output lifecycles", () => {
  const runtime = runtimeSmokeSource();
  const types = typeSmokeSource();

  const assertStructuralRuntime = (source) => {
    assert.match(
      source,
      /const colorsApi = await import\("@labpics\/colors"\);[\s\S]*?default: init,[\s\S]*?\bapplyTheme,/u,
      "runtime smoke must destructure the root API from one package-root import",
    );
    assert.match(
      source,
      /const applyThemeApi = await import\("@labpics\/colors\/apply-theme"\);/u,
    );
    assert.match(
      source,
      /const watchThemeApi = await import\("@labpics\/colors\/watch-theme"\);/u,
    );
    assert.match(
      source,
      /const adaptThemeApi = await import\("@labpics\/colors\/adapt-theme"\);/u,
    );
    assert.match(source, /assert\.equal\(applyThemeFromSubpath, applyTheme,/u);
    assert.match(source, /assert\.equal\(watchThemeFromSubpath, watchTheme,/u);
    assert.match(source, /assert\.equal\(adaptThemeFromSubpath, adaptTheme,/u);
    assert.match(source, /"createOutputSink" in namespace, false/u);
    assert.match(source, /const runtimeElements = new WeakSet\(\);/u);
    assert.match(source, /const runtimeDocuments = new WeakSet\(\);/u);
    assert.match(source, /const runtimeStates = new WeakMap\(\);/u);
    assert.match(source, /class RuntimeNode/u);
    assert.match(source, /class RuntimeDocument extends RuntimeNode/u);
    assert.match(source, /class RuntimeElement extends RuntimeNode/u);
    assert.match(source, /runtimeDocuments\.add\(document\);/u);
    assert.match(source, /runtimeElements\.add\(documentElement\);/u);
    assert.ok(
      source.indexOf('Object.defineProperty(globalThis, "document"') <
        source.indexOf('const colorsApi = await import("@labpics/colors");'),
      "the ambient DOM oracle must exist before the package is evaluated",
    );
    assert.match(source, /"createOutputSink"/u);
    assert.match(source, /import\("@labpics\/colors\/output-sink"\)/u);
    assert.match(source, /import\("@labpics\/colors\/output-bindings"\)/u);
    assert.match(source, /import\("@labpics\/colors\/sequence-identity-matches"\)/u);
    assert.match(source, /const runtimeDocument = \(\) =>/u);
    assert.match(source, /class FakeCSSStyleSheet/u);
    assert.ok(
      source.includes('const match = /^(:root|:host) \\{(?: (.*))?\\}$/u.exec(text);'),
      "the clean fake must parse only the two identity-native selectors",
    );
    assert.match(source, /selectorMatches = \(selector\) => selector === ":root"/u);
    assert.doesNotMatch(
      source,
      /\b(?:querySelectorAll|getAttribute|hasAttribute|setAttribute|removeAttribute|attributes)\b/u,
      "the clean fake must not preserve a selector-marker seam",
    );
    assert.match(source, /documentElement/u);
    assert.match(source, /const assertPublished =/u);
    assert.match(source, /const assertDisposed =/u);
    assert.equal(
      [...source.matchAll(/applyTheme\(appliedTarget, resolved\)/gu)].length,
      2,
      "identical apply must exercise the sink no-op path",
    );
    assert.equal(
      [...source.matchAll(/applyThemeFromSubpath\(subpathAppliedTarget, resolved\)/gu)].length,
      2,
      "the public apply subpath must exercise the same idempotent sink",
    );
    assert.match(source, /watchTheme subpath repeated dispose/u);
    assert.match(source, /adaptTheme subpath repeated dispose/u);
    assert.match(source, /host\.liveReplaceCount, 1/u);
    assert.match(source, /host\.scratchReplaceCount > 0/u);
    assert.match(source, /host\.document\.adoptedStyleSheets\.length, 0/u);
    assert.match(source, /watcher\.refresh\(\)/u);
    assert.match(source, /assertPublished\(watchedHost, "identical watchTheme refresh"\)/u);
    assert.match(source, /watcher\.dispose\(\)/u);
    assert.match(source, /adaptive\.tick\(0\)/u);
    assert.match(source, /adaptive\.dispose\(\)/u);
    assert.match(source, /applyThemeFromSubpath\(subpathAppliedTarget, resolved\)/u);
    assert.match(source, /watchThemeFromSubpath\(subpathWatchedTarget, \{/u);
    assert.match(source, /adaptThemeFromSubpath\(subpathAdaptedTarget, \{/u);
    assert.doesNotMatch(source, /runtimeTarget|output-host|\.\.?\/[^"']*output-sink/u);
  };
  assertStructuralRuntime(runtime);

  for (const [name, mutation] of [
    ["style-only target", (source) => source.replace("const runtimeDocument = () =>", "const runtimeTarget = () =>")],
    ["missing constructed sheet", (source) => source.replace("class FakeCSSStyleSheet", "class RemovedCSSStyleSheet")],
    [
      "missing identical apply no-op",
      (source) => source.replace(
        "assert.equal(applyTheme(appliedTarget, resolved), applied);",
        "assert.equal(applied, applied);",
      ),
    ],
    ["missing universal watch disposal", (source) => source.replaceAll("watcher.dispose()", "watcher.stop()")],
    ["missing universal adapt disposal", (source) => source.replaceAll("adaptive.dispose()", "adaptive.stop()")],
    ["missing applyTheme runtime subpath", (source) => source.replace(
      "applyThemeFromSubpath(subpathAppliedTarget, resolved)",
      "applyTheme(subpathAppliedTarget, resolved)",
    )],
    ["missing watchTheme runtime subpath", (source) => source.replace(
      "watchThemeFromSubpath(subpathWatchedTarget, {",
      "watchTheme(subpathWatchedTarget, {",
    )],
    ["missing adaptTheme runtime subpath", (source) => source.replace(
      "adaptThemeFromSubpath(subpathAdaptedTarget, {",
      "adaptTheme(subpathAdaptedTarget, {",
    )],
    ["unbranded clean target", (source) => source.replace(
      "  runtimeElements.add(documentElement);\n",
      "",
    )],
    ["stale attribute-selector parser", (source) => source.replace(
      'const match = /^(:root|:host) \\{(?: (.*))?\\}$/u.exec(text);',
      'const match = /^(:root|\\[[a-z0-9-]+\\]) \\{(?: (.*))?\\}$/u.exec(text);',
    )],
    ["stale root query seam", (source) => source.replace(
      "    documentElement: null,\n  };",
      "    documentElement: null,\n    querySelectorAll() { return []; },\n  };",
    )],
    ["stale marker attribute seam", (source) => source.replace(
      "    getRootNode: () => document,\n    style: inlineStyle,",
      "    getRootNode: () => document,\n    setAttribute() {},\n    style: inlineStyle,",
    )],
  ]) {
    assert.throws(
      () => assertStructuralRuntime(mutation(runtime)),
      undefined,
      `contract mutation must bite: ${name}`,
    );
  }

  assert.match(types, /type OutputBindingSet/u);
  assert.match(types, /type ApplyThemeAttachment as RootApplyThemeAttachment/u);
  assert.match(types, /type ApplyThemeAttachment as SubpathApplyThemeAttachment/u);
  assert.match(types, /type WatchController as RootWatchController/u);
  assert.match(types, /type AdaptController as RootAdaptController/u);
  assert.match(types, /const outputBindings: OutputBindingSet = resolved\.outputBindings/u);
  assert.match(types, /@ts-expect-error OutputBindingSet is immutable/u);
  assert.match(types, /outputBindings\.push\(/u);
  assert.match(types, /\.dispose\(\)/u);
  assert.match(types, /\[Symbol\.dispose\]\?\.\(\)/u);
});

test("runtime docs disclose output rollback and same-realm trust boundaries", () => {
  const applyThemeSource = read("packages", "colors", "apply-theme.js");
  assert.match(
    applyThemeSource,
    /outputBindings[\s\S]{0,160}проверяются на конфликт[\s\S]{0,100}несвязанные inline-декларации остаются нетронутыми/u,
  );
  assert.doesNotMatch(
    applyThemeSource,
    /inline declarations не сканируются и не изменяются/u,
  );

  for (const document of [
    read("packages", "colors", "README.md"),
    read("docs", "whitepaper.md"),
  ]) {
    assert.match(document, /rollback[\s\S]{0,80}может сохранить[\s\S]{0,40}кандидатные bytes/u);
    assert.match(document, /Recovery journal[\s\S]{0,80}ожидаемые предыдущие[\s\S]{0,20}bytes/u);
    assert.match(document, /следующая операция[\s\S]{0,80}соглас/u);
    assert.match(document, /Web-IDL accessors[\s\S]{0,160}shadowable/u);
    assert.match(document, /ambient `document`[\s\S]{0,160}ECMAScript primordials/u);
    assert.match(document, /pre-import[\s\S]{0,40}(?:compromise|подмен)/u);
    assert.match(document, /Target authority в `Symbol\.for`/u);
    assert.match(document, /не\s+является\s+(?:границей|механизмом)\s+авторизац/u);
    assert.match(document, /hostile same-realm/u);
    assert.doesNotMatch(
      document,
      /sink восстанавливает и проверяет предыдущие bytes до сообщения об отказе/u,
    );
  }
});

test("the shipped output sink exposes no injectable target-brand authority", () => {
  const manifest = JSON.parse(read("packages", "colors", "package.json"));
  const rootEntry = read("packages", "colors", "index.js");
  const sink = read("packages", "colors", "output-sink.js");

  assert.equal(manifest.exports["./output-sink"], undefined);
  assert.equal(manifest.exports["./output-bindings"], undefined);
  assert.equal(manifest.exports["./sequence-identity-matches"], undefined);
  assert.ok(
    manifest.files.includes("output-bindings.js"),
    "the private binding authority must ship for internal runtime imports",
  );
  assert.doesNotMatch(rootEntry, /createOutputSink/u);
  assert.doesNotMatch(sink, /export\s+(?:function|const|let|var|class)\s+createOutputSink/u);
  assert.match(sink, /function captureDomOracle\(globalObject\)/u);
  assert.match(sink, /const document = globalObject\.document;/u);
  assert.match(sink, /const nodeType = accessor\(document, "nodeType"\);/u);
  assert.match(sink, /const documentAdopted = accessorPair\(document, "adoptedStyleSheets"\);/u);
  assert.match(sink, /const shadowAdopted = accessorPair\(sentinelRoot, "adoptedStyleSheets"\);/u);
  assert.match(sink, /APPLY\(DOM_ORACLE\.ownerDocument, target, \[\]\)/u);
  assert.match(sink, /APPLY\(DOM_ORACLE\.elementStyle, target, \[\]\)/u);
  assert.match(
    sink,
    /function acquireOutputLeaseUnchecked[\s\S]{0,220}preflightAcquisition[\s\S]{0,120}authorityDescriptor/u,
  );
  assert.doesNotMatch(sink, /ACQUISITION_STATE|admissionAuthority|admission\.run/u);
  assert.doesNotMatch(sink, /globalObject\.Node|globalThis\.Node|\.ownerDocument;|\.getRootNode\(\)/u);
  assert.doesNotMatch(
    sink,
    /allowStructuralTarget|process\.env|globalThis\[[^\]]*brand|Symbol\.for\([^)]*brand/iu,
  );
});

test("DOM-free acquisition preserves the original oracle-capture failure", () => {
  const sinkUrl = pathToFileURL(join(root, "packages", "colors", "output-sink.js")).href;
  const probe = `
    const { acquireOutputLease } = await import(${JSON.stringify(sinkUrl)});
    try {
      acquireOutputLease({}, ["--lab-probe"], "release/dom-free");
      throw new Error("DOM-free acquisition unexpectedly succeeded");
    } catch (error) {
      if (error?.code !== "OUTPUT_TARGET_CAPABILITY") throw error;
      if (!(error.cause instanceof TypeError)) {
        throw new Error("DOM oracle failure did not retain its TypeError cause");
      }
      if (!error.cause.message.includes("globalThis.document")) {
        throw new Error(\`unexpected DOM oracle cause: \${error.cause.message}\`);
      }
    }
  `;
  execFileSync(process.execPath, ["--input-type=module", "-e", probe], {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: DOM_FREE_PROBE_TIMEOUT_MS,
  });
});

test("generated clean-consumer type smoke compiles at both supported TypeScript gates", () => {
  const packageDirectory = join(root, "packages", "colors");
  const temporary = mkdtempSync(join(tmpdir(), "labcolors-consumer-type-smoke-"));
  const smoke = join(temporary, "smoke.ts");
  try {
    writeFileSync(
      join(temporary, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );
    const packageScope = join(temporary, "node_modules", "@labpics");
    mkdirSync(packageScope, { recursive: true });
    symlinkSync(
      packageDirectory,
      join(packageScope, "colors"),
      process.platform === "win32" ? "junction" : "dir",
    );
    writeFileSync(smoke, typeSmokeSource());
    for (const compiler of ["typescript-floor", "typescript"]) {
      execFileSync(process.execPath, [
        join(packageDirectory, "node_modules", compiler, "lib", "tsc.js"),
        "--noEmit",
        "--strict",
        "--skipLibCheck",
        "false",
        "--target",
        "ES2022",
        "--lib",
        "ES2022,DOM",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        smoke,
      ], {
        cwd: temporary,
        stdio: ["ignore", "pipe", "pipe"],
      });
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("runtime declarations expose one curated type surface", () => {
  const wasmSource = read("crates", "labcolors-wasm", "src", "lib.rs");
  const customSection = wasmSource.match(
    /const TS_RESULT_TYPES: &'static str = r##"([\s\S]*?)"##;/u,
  )?.[1];
  assert.ok(customSection, "custom TypeScript section not found");
  const generatedNames = [
    ...customSection.matchAll(/^export\s+(?:type|interface)\s+([A-Za-z][A-Za-z0-9_]*)/gmu),
  ].map((match) => match[1]);
  assert.ok(generatedNames.length > 10, "anti-vacuum: custom type surface is non-trivial");
  assert.equal(new Set(generatedNames).size, generatedNames.length, "duplicate custom type name");
  assert.doesNotMatch(customSection, /Feasibility|feasibility/u);

  const rootDeclarations = read("packages", "colors", "index.d.ts");
  assert.match(
    rootDeclarations,
    /^\/\/\/ <reference lib="esnext\.disposable" \/>/u,
    "package root must make wasm-bindgen disposal types self-contained for consumers",
  );
  const typecheck = JSON.parse(read("packages", "colors", "tsconfig.json"));
  assert.deepEqual(typecheck.compilerOptions.lib, ["ES2022", "DOM"]);
  assert.equal(typecheck.compilerOptions.skipLibCheck, false);

  for (const subpath of ["apply-theme", "watch-theme", "adapt-theme"]) {
    const declarations = read("packages", "colors", `${subpath}.d.ts`);
    assert.match(
      declarations,
      /from "\.\/index\.js";/u,
      `${subpath} declarations must reuse the curated root type owner`,
    );
    assert.doesNotMatch(
      declarations,
      /\.\/pkg\/labcolors\.js/u,
      `${subpath} declarations must not bypass the curated root type owner`,
    );
  }

  const verifier = read("scripts", "verify-package-release.mjs");
  for (const subpath of ["apply-theme", "watch-theme", "adapt-theme"]) {
    assert.match(
      verifier,
      new RegExp(`@labpics/colors/${subpath}`, "u"),
      `clean-consumer type smoke must compile the ${subpath} public subpath`,
    );
  }
  assert.doesNotMatch(verifier, /from "@labpics\/colors\/effective-bg"/u);
  assert.match(verifier, /ERR_PACKAGE_PATH_NOT_EXPORTED/u);

  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  assert.equal(
    packageJson.exports["./effective-bg"],
    undefined,
    "low-level effective-background math must not be a package subpath",
  );

  const rootTypes = rootDeclarations.match(
    /export type \{([\s\S]*?)\} from "\.\/pkg\/labcolors\.js";/u,
  )?.[1];
  assert.ok(rootTypes, "curated root type export block not found");
  const exportedNames = [...rootTypes.matchAll(/^\s{2}([A-Za-z][A-Za-z0-9_]*),$/gmu)].map(
    (match) => match[1],
  );
  assert.deepEqual(
    [...exportedNames].sort(),
    [...generatedNames].sort(),
    "root types must equal the runtime generated surface exactly",
  );
  assert.doesNotMatch(
    rootDeclarations,
    /^export (?:declare )?(?:type|interface|class|enum|namespace)\s+[A-Za-z]/mu,
    "root declarations must not add local named types beside the curated re-export blocks",
  );
  assert.doesNotMatch(rootDeclarations, /Feasibility|feasibility/u);
  assert.match(rootDeclarations, /export type \{ Wcag22CriterionV1 \} from "\.\/wcag22\.js"/u);

  assert.doesNotMatch(rootTypes, /InitOutput|__wbg_/u, "raw wasm ABI must stay private");
  assert.ok(
    !existsSync(join(root, "packages", "colors", "compiler.d.ts")) &&
      !existsSync(join(root, "packages", "colors", "compiler.js")),
    "the excised compiler entry must stay deleted",
  );
});

test("public declarations compile at the documented minimum TypeScript version", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const packageLock = JSON.parse(read("packages", "colors", "package-lock.json"));
  assert.equal(packageJson.devDependencies["typescript-floor"], "npm:typescript@5.2.2");
  assert.equal(
    packageLock.packages["node_modules/typescript-floor"]?.version,
    "5.2.2",
    "the consumer floor must be an exact offline lock, not a floating install",
  );

  const readme = read("packages", "colors", "README.md");
  assert.match(readme, /TypeScript `>= 5\.2\.2`/u);
  assert.match(
    readme,
    /typescriptlang\.org\/docs\/handbook\/release-notes\/typescript-5-2\.html/u,
  );

  execFileSync(process.execPath, [
    join(root, "packages", "colors", "node_modules", "typescript-floor", "lib", "tsc.js"),
    "--noEmit",
    "--strict",
    "--skipLibCheck",
    "false",
    "--target",
    "ES2022",
    "--lib",
    "ES2022,DOM",
    "--module",
    "NodeNext",
    "--moduleResolution",
    "NodeNext",
    "index.d.ts",
    "apply-theme.d.ts",
    "watch-theme.d.ts",
    "adapt-theme.d.ts",
  ], {
    cwd: join(root, "packages", "colors"),
    stdio: ["ignore", "pipe", "pipe"],
  });

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /minimumConsumerCompiler/u);
  assert.match(verifier, /node_modules\/typescript-floor/u);
});

test("conformance docs define every neutral-axis count as an oracle output", () => {
  const readme = read("conformance", "README.md");
  for (const range of [
    "#000000…#040404",
    "#FEFEFE…#FFFFFF",
    "#757575…#767676",
    "#000000…#2D2D2D",
    "#D2D2D2…#FFFFFF",
    "#5A5A5A…#949494",
  ]) {
    assert.ok(readme.includes(range), `neutral-axis count docs omit exact range ${range}`);
  }
  assert.match(readme, /256/u);
  assert.match(readme, /scripts\/verify_wcag22_neutral_axis\.py/u);
  assert.match(readme, /wcag22-neutral-axis-oracle-v1\.json/u);
  assert.match(readme, /wcag22_neutral_axis_replay\.rs/u);
  assert.match(readme, /не параметры/u);
});
