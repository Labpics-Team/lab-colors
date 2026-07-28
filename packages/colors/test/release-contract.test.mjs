import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  validateNumericalEvidenceArtifacts,
  validateSolveFailurePair,
  validateSolveFamily,
} from "../../../scripts/verify-package-release.mjs";
import { workspacePackageTable } from "../../../scripts/cargo-workspace.mjs";
import {
  NUMERICAL_EVIDENCE_FILES,
  PACKED_NUMERICAL_EVIDENCE_PATHS,
  POINT_SUPPORT_EVIDENCE_FILES,
  WCAG22_EVIDENCE_FILES,
} from "../../../scripts/release-evidence.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");

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
  assert.match(packageJson.scripts.build, /wasm-pack build .* --locked$/);
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

  const ci = read(".github", "workflows", "ci.yml");
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
    assert.equal(readlinkSync(license), canonicalTarget);
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

  const ci = read(".github", "workflows", "ci.yml");
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
      /^\s*debian:bookworm\|ubuntu:jammy\)\n\s+ALSA_PACKAGE=libasound2\n\s+;;$/mu,
    );
    assert.match(
      active,
      /^\s*debian:trixie\|ubuntu:noble\)\n\s+ALSA_PACKAGE=libasound2t64\n\s+;;$/mu,
    );
    assert.match(
      active,
      /^\s*\*\)\n\s+echo "unsupported Chrome dependency release: \$ID:\$VERSION_CODENAME" >&2\n\s+exit 1\n\s+;;$/mu,
    );
    assert.equal(active.match(/^readonly ALSA_PACKAGE$/gmu)?.length, 1);
    const trustedSourceLines = [
      '"deb [signed-by=$DISTRO_KEYRING] https://deb.debian.org/debian $VERSION_CODENAME main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://deb.debian.org/debian $VERSION_CODENAME-updates main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://security.debian.org/debian-security $VERSION_CODENAME-security main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://archive.ubuntu.com/ubuntu $VERSION_CODENAME main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://archive.ubuntu.com/ubuntu $VERSION_CODENAME-updates main" \\',
      '"deb [signed-by=$DISTRO_KEYRING] https://security.ubuntu.com/ubuntu $VERSION_CODENAME-security main" \\',
    ];
    const activeLines = active.split("\n").map((line) => line.trim());
    assert.deepEqual(
      activeLines.filter((line) => line.startsWith('"deb [signed-by=')),
      trustedSourceLines,
      "the generated source inventory must contain only the six exact distro sources",
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
    const download = active.indexOf('apt-get "${APT_OPTIONS[@]}" download', update);
    assert.ok(optionsEnd < update && update < download);
    assert.deepEqual(
      active
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => /(?:^|[\s;&(|])apt(?:-get)?(?=\s)/u.test(line)),
      [
        'apt-get "${APT_OPTIONS[@]}" update',
        '(cd "$DEBS_DIR" && apt-get "${APT_OPTIONS[@]}" download libnspr4 libnss3 "$ALSA_PACKAGE" libgbm1 2>&1)',
      ],
      "every APT invocation must use the one immutable isolated option set",
    );
  };
  assertAptSourceIsolation(ci);
  for (const mutant of [
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
      '          (cd "$DEBS_DIR" && apt-get',
      '          unset APT_OPTIONS\n          (cd "$DEBS_DIR" && apt-get',
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
  ]) {
    assert.notEqual(mutant, ci, "hostile APT mutation must bite");
    assert.throws(() => assertAptSourceIsolation(mutant));
  }
  assert.match(ci, /Dir::State::lists=\$APT_LISTS/);
  assert.match(ci, /Dir::State::status=\/var\/lib\/dpkg\/status/);
  assert.match(ci, /Dir::Cache=\$APT_CACHE/);
  assert.match(ci, /Dir::Cache::archives=\$APT_CACHE\/archives/);
  assert.match(ci, /Debug::NoLocking=1/);
  assert.match(ci, /Acquire::Retries=3/);
  const aptUpdate = ci.indexOf('apt-get "${APT_OPTIONS[@]}" update');
  const aptDownload = ci.indexOf('apt-get "${APT_OPTIONS[@]}" download');
  assert.ok(
    aptUpdate >= 0 && aptDownload >= 0 && aptUpdate < aptDownload,
    "Chrome dependency download must use a fresh isolated APT index",
  );
  assert.match(ci, /CHROME_BIN_DIR="\$RUNNER_TEMP\/chrome-bin-\$GITHUB_JOB"/);
  assert.doesNotMatch(ci, /\$HOME|~\//, "WASM/Chrome state must not leak into shared HOME");
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
    read(".github", "workflows", "native-conformance.yml"),
    "native conformance",
  );
  assertCheckoutCredentialsAreEphemeral(
    read(".github", "workflows", "mutation.yml"),
    "scheduled mutation",
  );
  assertCheckoutCredentialsAreEphemeral(
    read(".github", "workflows", "publish.yml"),
    "publish",
  );
});

test("publish accepts only canonical exact-SHA workflow runs and their immutable CI artifact", () => {
  const publish = read(".github", "workflows", "publish.yml");
  assert.match(publish, /^\s*NODE_TOOLCHAIN: "24\.14\.0"$/m);
  assert.match(publish, /^\s*NPM_TOOLCHAIN: "11\.9\.0"$/m);
  assert.match(
    publish,
    /^concurrency:\n  group: npm-publish\n  cancel-in-progress: false$/m,
  );
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
    /- name: npm publish verified CI tarball \(granular token, no rebuild\/repack\)[\s\S]*?env:\s*\n\s*NODE_AUTH_TOKEN: \$\{\{ secrets\.NPM_TOKEN \}\}[\s\S]*?run: npm publish --ignore-scripts/,
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
    "swift conformance (self-hosted Linux, swift container)",
  ];
  for (const check of requiredChecks) {
    assert.ok(publish.includes(JSON.stringify(check)), `publish gate lost required check ${check}`);
  }
  assert.match(publish, /file: "ci\.yml"[\s\S]*path: "\.github\/workflows\/ci\.yml"/);
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
    /TARBALL_PATH: \$\{\{ steps\.verified-artifact\.outputs\.tarball \}\}[\s\S]*run: npm publish --ignore-scripts "\$TARBALL_PATH"/,
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
  const publish = read(".github", "workflows", "publish.yml");
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
    const runShellGuard = (sha) =>
      execFileSync("/bin/bash", ["-euo", "pipefail", "-c", fullGuard], {
        cwd: shellFixture,
        env: {
          ...process.env,
          GITHUB_SHA: sha,
          PATH: `${fakeBin}:${process.env.PATH}`,
        },
        encoding: "utf8",
        stdio: "pipe",
      });
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
  const publish = read(".github", "workflows", "publish.yml");
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
      },
    ],
    ciJobs: [
      ...requiredCiJobs.map((name) => successfulJob(name, 101)),
      successfulJob(requiredCiJobs[0], 999),
    ],
    nativeJobs: [
      successfulJob("swift conformance (self-hosted Linux, swift container)", 202),
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

    const wrongRunJobs = structuredClone(fixtures);
    wrongRunJobs.ciJobs = wrongRunJobs.ciJobs.filter((job) =>
      !(job.name === requiredCiJobs[0] && job.run_id === 101));
    assert.throws(() => execute(wrongRunJobs), /Command failed/u);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("publish artifact validator executes and rejects identity or byte drift", () => {
  const publish = read(".github", "workflows", "publish.yml");
  const validator = workflowNodeScript(
    publish,
    "name: validate manifest identity and byte-exact tarball",
  );
  assert.ok(validator.length > 1_000, "anti-vacuum: extracted validator is unexpectedly small");

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-publish-contract-"));
  try {
    const artifact = join(temporary, "artifact");
    const payload = join(temporary, "payload", "package");
    mkdirSync(artifact, { recursive: true });
    mkdirSync(payload, { recursive: true });
    const packageVersion = "0.11.0";
    const coreVersion = "0.3.0";
    writeFileSync(
      join(payload, "package.json"),
      `${JSON.stringify({ name: "@labpics/colors", version: packageVersion })}\n`,
    );
    const runtimeWasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
    mkdirSync(join(payload, "pkg"));
    writeFileSync(join(payload, "pkg", "labcolors_bg.wasm"), runtimeWasm);

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

    const tarball = join(artifact, `labpics-colors-${packageVersion}.tgz`);
    execFileSync("tar", ["-czf", tarball, "-C", join(temporary, "payload"), "package"]);
    const bytes = readFileSync(tarball);

    const manifest = {
      schemaVersion: 4,
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
        method: "two-independent-npm-pack-passes",
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
        wasm: structuredClone(wasmEvidence),
        buildMetadata: {
          path: "build-metadata.json",
          bytes: metadataBytes.length,
          sha256: createHash("sha256").update(metadataBytes).digest("hex"),
        },
      },
    };
    const manifestPath = join(artifact, "release-manifest.json");
    const output = join(temporary, "github-output");
    const fakeBin = join(temporary, "bin");
    mkdirSync(fakeBin);
    writeFileSync(join(fakeBin, "npm"), "#!/bin/sh\nprintf '11.9.0\\n'\n");
    chmodSync(join(fakeBin, "npm"), 0o755);

    const execute = () => execFileSync(process.execPath, ["-e", validator], {
      env: {
        ...process.env,
        ARTIFACT_DIR: artifact,
        EXPECTED_SHA: expectedSha,
        EXPECTED_TAG: `colors-v${packageVersion}`,
        EXPECTED_NODE: process.versions.node,
        EXPECTED_NPM: "11.9.0",
        GITHUB_WORKSPACE: root,
        GITHUB_OUTPUT: output,
        PATH: `${fakeBin}:${process.env.PATH ?? ""}`,
      },
      encoding: "utf8",
      stdio: "pipe",
      cwd: root,
    });

    writeFileSync(manifestPath, `${JSON.stringify(manifest)}\n`);
    execute();
    assert.equal(readFileSync(output, "utf8"), `tarball=${tarball}\n`);

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
    execFileSync("tar", ["-czf", tarball, "-C", join(temporary, "payload"), "package"]);
    const tamperedTarball = readFileSync(tarball);
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

test("release verifier performs an independent byte-for-byte reproduction pass", () => {
  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /reproducibility/);
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

  const ci = read(".github", "workflows", "ci.yml");
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
  assert.match(wasmJob, /runs-on: \[self-hosted, Linux, X64\]/u);
  assert.ok(
    ci.includes(`  RUST_TOOLCHAIN: ${budget.toolchain.rust}`),
    "the live Rust toolchain must equal the budget declaration",
  );
  assert.ok(
    wasmJob.includes(
      `cargo install wasm-pack --version ${budget.toolchain.wasmPack} --locked`,
    ),
    "the live wasm-pack toolchain must equal the budget declaration",
  );
  assert.ok(
    wasmJob.includes(`targets: ${budget.toolchain.target}`),
    "the live WASM target must equal the budget declaration",
  );

  const repetition = workflowRunScript(
    ci,
    "name: repeat runtime WASM build in one toolchain-pinned CI job",
  );
  const recipePrefix = "CARGO_ENCODED_RUSTFLAGS=<rustPathRemap> ";
  const expectedRemapExport = `export CARGO_ENCODED_RUSTFLAGS=${budget.recipe.rustPathRemap
    .map((mapping) => {
      const separator = mapping.indexOf("=");
      assert.ok(separator > 0, "path remap must name one environment source");
      return `"--remap-path-prefix=\$${mapping.slice(0, separator)}=${mapping.slice(separator + 1)}"`;
    })
    .join("$'\\x1f'")}`;
  assert.ok(budget.recipe.command.startsWith(recipePrefix));
  const expectedBuild = budget.recipe.command.slice(recipePrefix.length);
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
      functionBody.split("\n").map((line) => line.trim()).filter(Boolean),
      [expectedBuild],
      "the live build must equal the budget recipe",
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
      repetition.replace(expectedBuild, `${expectedBuild} --features unreviewed`),
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
      ["recipe command", (value) => { value.recipe.command = "wasm-pack build"; }],
      ["recipe command line", (value) => { value.recipe.command += "\nother"; }],
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
    assert.equal(schemaMutations.length, 30, "schema mutation matrix changed");
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
      /schemaVersion must be 1/u,
      "schema must fail before a missing artifact is read",
    );

    writeFileSync(fixtureBudgetPath, `${JSON.stringify(fixture)}\n`);
    assert.throws(run, /canonical JSON/u, "non-canonical JSON must fail");
    writeFileSync(
      fixtureBudgetPath,
      canonicalJson(fixture).replace(
        '  "schemaVersion": 1,\n',
        '  "schemaVersion": 1,\n  "schemaVersion": 1,\n',
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
  const workflow = read(".github", "workflows", "ci.yml");
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
    /validatePackedNumericalEvidence\(canonicalPack\.path, numericalEvidenceArtifacts\)/u,
  );
  assert.match(
    verifier,
    /verifyCleanConsumer\([\s\S]*?numericalEvidenceArtifacts[\s\S]*?\);/u,
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
  assert.match(prepare, /const BUILD_METADATA = resolve\(PACKAGE_DIR, "build-metadata\.json"\)/);
  assert.match(prepare, /sourceSha/);
  assert.ok(
    prepare.indexOf("const sourceSha = verifiedSourceSha()") <
      prepare.indexOf("atomicWrite(PACKED_LICENSE"),
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
