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
  validateSolveFailurePair,
  validateSolveFamily,
  validateWcag22EvidenceArtifacts,
} from "../../../scripts/verify-package-release.mjs";
import { workspacePackageTable } from "../../../scripts/cargo-workspace.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");

function workflowNodeScript(workflow, stepName) {
  const step = workflow.indexOf(stepName);
  assert.ok(step >= 0, `workflow step not found: ${stepName}`);
  const marker = "node <<'NODE'\n";
  const start = workflow.indexOf(marker, step);
  assert.ok(start >= 0, `node heredoc not found after: ${stepName}`);
  const bodyStart = start + marker.length;
  const end = workflow.indexOf("\n          NODE", bodyStart);
  assert.ok(end >= 0, `node heredoc terminator not found after: ${stepName}`);
  return workflow
    .slice(bodyStart, end)
    .split("\n")
    .map((line) => line.startsWith("          ") ? line.slice(10) : line)
    .join("\n");
}

function workflowRunScript(workflow, stepName) {
  const step = workflow.indexOf(stepName);
  assert.ok(step >= 0, `workflow step not found: ${stepName}`);
  const marker = "\n        run: |\n";
  const start = workflow.indexOf(marker, step);
  assert.ok(start >= 0, `run block not found after: ${stepName}`);
  const bodyStart = start + marker.length;
  const end = workflow.indexOf("\n      - ", bodyStart);
  assert.ok(end >= 0, `next workflow step not found after: ${stepName}`);
  return workflow
    .slice(bodyStart, end)
    .split("\n")
    .map((line) => line.startsWith("          ") ? line.slice(10) : line)
    .join("\n");
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
    assert.match(manifest, /^license\.workspace = true$/mu);
    assert.doesNotMatch(manifest, /^license-file\s*=/mu);
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
  assert.match(ci, /test -L crates\/labcolors-core\/LICENSE/);
  assert.match(ci, /cmp LICENSE crates\/labcolors-core\/LICENSE/);
  assert.match(ci, /tar -xzf .*labcolors-core-\$\{crate_version\}\.crate/);
  assert.match(ci, /test ! -L "\$crate_dir\/LICENSE"/);
  assert.match(ci, /cmp LICENSE "\$crate_dir\/LICENSE"/);
  assert.match(
    ci,
    /cargo test --doc --manifest-path "\$crate_dir\/Cargo\.toml" --locked/,
  );
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
    writeFileSync(
      join(payload, "package.json"),
      `${JSON.stringify({ name: "@labpics/colors", version: "0.10.0" })}\n`,
    );
    const runtimeWasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
    mkdirSync(join(payload, "pkg"));
    writeFileSync(join(payload, "pkg", "labcolors_bg.wasm"), runtimeWasm);

    const expectedSha = "a".repeat(40);
    const conformance = {
      packVersion: "9.0.0",
      packDigest: "12345678",
      manifestSha256: "c".repeat(64),
      familySetSha256: "d".repeat(64),
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
      package: { name: "@labpics/colors", version: "0.10.0" },
      sourceSha: expectedSha,
      coreVersion: "0.2.0",
      conformance,
      wasm: wasmEvidence,
    };
    const metadataPath = join(payload, "build-metadata.json");
    const metadataBytes = Buffer.from(`${JSON.stringify(buildMetadata)}\n`);
    writeFileSync(metadataPath, metadataBytes);

    const tarball = join(artifact, "labpics-colors-0.10.0.tgz");
    execFileSync("tar", ["-czf", tarball, "-C", join(temporary, "payload"), "package"]);
    const bytes = readFileSync(tarball);

    const manifest = {
      schemaVersion: 3,
      npm: "0.10.0",
      core: "0.2.0",
      conformance,
      sourceSha: expectedSha,
      artifacts: {
        tarball: {
          path: ".release/labpics-colors-0.10.0.tgz",
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
        EXPECTED_TAG: "colors-v0.10.0",
        EXPECTED_NODE: process.versions.node,
        EXPECTED_NPM: "11.9.0",
        GITHUB_OUTPUT: output,
        PATH: `${fakeBin}:${process.env.PATH ?? ""}`,
      },
      encoding: "utf8",
      stdio: "pipe",
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

test("conformance pack 10 removes only the muddiness family", () => {
  const immutableFamilies = new Map([
    ["contrasts.json", "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145"],
    ["ladders.json", "496f562e55ad8110aeb8a07042b1964ec9ff4d0f1e8c09e362d1b2d14c513036"],
    ["alpha.json", "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1"],
    ["wcag22.json", "6e234fa3a0d4e2b21f515b8f4e6be76f223768821e0308e774c31a5ce7a1d826"],
  ]);
  assert.equal(immutableFamilies.size, 4, "anti-vacuum: unchanged family set changed");
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
  for (const [name, expected] of immutableFamilies) {
    const bytes = readFileSync(join(root, "conformance", "vectors", name));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected, name);
  }
  assert.equal(
    createHash("sha256")
      .update(readFileSync(join(root, "conformance", "vectors", "solve.json")))
      .digest("hex"),
    "db04e50698cc3b10223f4005f74dd35cc5ae0a29988825e44db5c985aa9207af",
    "pack-7 solve family bytes drifted",
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
    "pack 7 must not preserve the superseded failure kind",
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
    ["unsupported", "gamut_unsupported"],
    ["rejected", "invalid_input"],
  ];
  assert.equal(boundaryRows.length, 6, "public core failure dictionary changed");
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

test("WASM role size budgets are exact, append-only, and acyclic", async () => {
  const bench = join(root, "packages", "colors", "bench");
  const paths = Object.fromEntries(
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14].map((version) => [
      `v${version}`,
      join(bench, `wasm-size-budget-v${version}.json`),
    ]),
  );
  const checkerPath = join(root, "scripts", "check-wasm-size-budget.mjs");
  const sha256 = (value) => createHash("sha256").update(value).digest("hex");
  const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
  const expectedHashes = {
    v1: "4f7340fc8cfd0ccb97377c385f2f8d8e7a9ef2c5ba96177f518c5d07de2825e1",
    v2: "713ccc314b3e6f638d87a54716d665d52f77c86f34a2b6edefe0a354a499d8b1",
    v3: "d7937612e4c33574a8af28845bb1dd30cca86fc39fc0206cac4c377de77fec15",
    v4: "c34fc10404dc7057a53a28592d18342078b5cd0e5dcaa888db482abf3f5fb23c",
    v5: "e4b53a2eb976a8c66827a559cb81232e359b734dbfb14725da215cb496ff5d59",
    v6: "761af6050031169dac7eafdfadb2db9bbb2023b96ed5ba9d3c5dc966ffeafb32",
    v7: "01d17c042b7dc36585e9657490048932fdf61d4715099b735aa3bf2d3dc5777e",
    v8: "3590ffd2d158c2caf5cfbd26489e609b08d1cb640584456baa2166ccf50f5109",
    v9: "e00fa0549d67ab027f589c053aeb4374f6437704a6277cc9784dcaa1d8015ad4",
    v10: "6f3318c29c633860a146be5dcd29e4ce85a3a52296b9719b506aba16951a58e6",
    v11: "fa11531ee390dd6dfdfadfadab99bbe8277f2b152b567951b17ef6093d42b1e4",
    v12: "925452113b18b63137b9dae4786e3a8f7ba098eb47a2631a97107fbd52aa9a95",
    v13: "3cc88303a0f43e8ca33ae70d723a3179c68b0cc2744310a791e8f43885482f34",
    v14: "20c5886e3edaa6eaf3e37b915d81982a3a13e30064fc7fa8eb702eda38a20fb6",
  };
  const documents = {};
  for (const version of Object.keys(paths)) {
    const bytes = readFileSync(paths[version]);
    const value = JSON.parse(bytes);
    documents[version] = value;
    assert.equal(sha256(bytes), expectedHashes[version], `${version} byte identity drifted`);
    if (version !== "v1") assert.equal(bytes.toString("utf8"), canonicalJson(value));
  }

  const { v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14 } = documents;
  assert.equal(v1.budgetId, "labcolors-wasm-raw-issue-284-v1");
  assert.equal(v2.budgetId, "labcolors-wasm-raw-issue-295-v2");
  assert.equal(v3.budgetId, "labcolors-wasm-raw-issue-296-v3");
  assert.equal(v4.budgetId, "labcolors-wasm-raw-issue-296-v4");
  assert.deepEqual(Object.keys(v5), [
    "schemaVersion",
    "budgetId",
    "predecessor",
    "toolchainSource",
    "buildRecipes",
    "roles",
  ]);
  assert.equal(v5.schemaVersion, 4);
  assert.equal(v5.budgetId, "labcolors-wasm-roles-issue-296-c1-v5");
  assert.deepEqual(v5.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v4.json",
    fileSha256: expectedHashes.v4,
  });
  assert.deepEqual(v5.toolchainSource, {
    path: "packages/colors/bench/wasm-size-budget-v1.json",
    fileSha256: expectedHashes.v1,
  });
  assert.deepEqual(Object.keys(v5.buildRecipes), ["runtime", "compiler"]);
  assert.deepEqual(Object.keys(v5.roles), ["runtime", "compiler"]);
  assert.equal(
    v5.buildRecipes.runtime.recipeSha256,
    "0ea74cb070e0a5facb7280f6124930a0bb673ee4dcee9c99fff110db6c9389d4",
  );
  assert.equal(
    v5.buildRecipes.compiler.recipeSha256,
    "ce53cea5f579c512a6d2f0c3348f250ac0a5e03206de55e7979c8eae1403be8f",
  );
  assert.match(v5.buildRecipes.runtime.command, /crates\/labcolors-wasm/u);
  assert.match(v5.buildRecipes.compiler.command, /crates\/labcolors-compiler/u);
  assert.deepEqual(v5.roles.runtime.measurement, {
    issue: 296,
    slice: "C1",
    measurementPlatform: "linux-x64",
    rawBytes: 454385,
    sha256: "8cd65f001d4bb4b8ddead9084e705a64bee14cd796c7bc6ebeb2f2687aa5fdba",
  });
  assert.deepEqual(v5.roles.compiler.measurement, {
    issue: 296,
    slice: "C1",
    measurementPlatform: "linux-x64",
    rawBytes: 175212,
    sha256: "3a552ce43ada7d0b10e90a23b4a7e50a4ecad77a446374b98ca8ee6b5c6a2a45",
  });
  assert.equal(v5.roles.runtime.policy.maxRawBytes, v5.roles.runtime.measurement.rawBytes);
  assert.equal(v5.roles.compiler.policy.maxRawBytes, v5.roles.compiler.measurement.rawBytes);
  assert.ok(v5.roles.runtime.policy.maxRawBytes <= v1.policy.maxRawBytes);
  assert.ok(v5.roles.runtime.policy.maxRawBytes <= v4.policy.maxRawBytes);

  // V6 (#296-C3): compiler-роль публикует атомарную операцию — рост размера
  // зафиксирован новым точным измерением; runtime-измерение байт-идентично C1.
  assert.equal(v6.schemaVersion, 5);
  assert.equal(v6.budgetId, "labcolors-wasm-roles-issue-296-c3-v6");
  assert.deepEqual(v6.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v5.json",
    fileSha256: expectedHashes.v5,
  });
  assert.deepEqual(v6.toolchainSource, v5.toolchainSource);
  assert.deepEqual(v6.buildRecipes, v5.buildRecipes);
  assert.deepEqual(v6.roles.runtime, v5.roles.runtime);
  assert.deepEqual(v6.roles.compiler.measurement, {
    issue: 296,
    slice: "C3",
    measurementPlatform: "linux-x64",
    rawBytes: 229658,
    sha256: "34e2a561862ee06d52d1104f8ba60ccf9967e2e4fd09803d4e75e1966074bc8d",
  });
  assert.equal(
    v6.roles.compiler.policy.derivation,
    "exact-accepted-issue-296-slice-c3-compiler-measurement",
  );
  assert.equal(v6.roles.runtime.policy.maxRawBytes, v6.roles.runtime.measurement.rawBytes);
  assert.equal(v6.roles.compiler.policy.maxRawBytes, v6.roles.compiler.measurement.rawBytes);
  assert.ok(v6.roles.runtime.policy.maxRawBytes <= v1.policy.maxRawBytes);
  assert.ok(v6.roles.runtime.policy.maxRawBytes <= v4.policy.maxRawBytes);

  // V7 (#307-C7a): size policy хранит только измеряемую величину. SHA каждого
  // конкретного source-коммита принадлежит release provenance, не size budget.
  assert.equal(v7.schemaVersion, 6);
  assert.equal(v7.budgetId, "labcolors-wasm-roles-issue-307-c7a-v7");
  assert.deepEqual(v7.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v6.json",
    fileSha256: expectedHashes.v6,
  });
  assert.deepEqual(v7.toolchainSource, v6.toolchainSource);
  assert.deepEqual(v7.buildRecipes, v6.buildRecipes);
  assert.deepEqual(v7.roles.runtime.measurement, {
    issue: 307,
    slice: "C7a",
    measurementPlatform: "linux-x64",
    rawBytes: 454334,
  });
  assert.equal(
    v7.roles.runtime.policy.derivation,
    "exact-accepted-issue-307-slice-c7a-runtime-measurement",
  );
  assert.deepEqual(v7.roles.compiler.measurement, {
    issue: 296,
    slice: "C3",
    measurementPlatform: "linux-x64",
    rawBytes: 229658,
  });
  assert.deepEqual(v7.roles.compiler.policy, v6.roles.compiler.policy);
  for (const role of ["runtime", "compiler"]) {
    assert.equal(v7.roles[role].policy.maxRawBytes, v7.roles[role].measurement.rawBytes);
    assert.ok(v7.roles[role].policy.maxRawBytes <= v6.roles[role].policy.maxRawBytes);
  }

  // V8 accepts the exact measured PR #338 runtime snapshot; it does not infer
  // how many bytes belong to one capability. The unchanged role stays ratcheted.
  assert.equal(v8.schemaVersion, 7);
  assert.equal(v8.budgetId, "labcolors-wasm-roles-pr-338-v8");
  assert.deepEqual(v8.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v7.json",
    fileSha256: expectedHashes.v7,
  });
  assert.deepEqual(v8.toolchainSource, v7.toolchainSource);
  assert.deepEqual(v8.buildRecipes, v7.buildRecipes);
  assert.deepEqual(v8.roles.runtime.measurement, {
    source: "github-actions-run-29548782379",
    measurementPlatform: "linux-x64",
    rawBytes: 456696,
  });
  assert.equal(
    v8.roles.runtime.policy.basis,
    "accepted-pr-338-runtime-snapshot",
  );
  assert.equal(v8.roles.runtime.policy.maxRawBytes, v8.roles.runtime.measurement.rawBytes);
  assert.ok(v8.roles.runtime.policy.maxRawBytes > v7.roles.runtime.policy.maxRawBytes);
  assert.equal(v8.roles.compiler.artifact, v7.roles.compiler.artifact);
  assert.deepEqual(v8.roles.compiler.measurement, {
    source: "github-actions-run-29548782379",
    measurementPlatform: "linux-x64",
    rawBytes: 229658,
  });
  assert.deepEqual(v8.roles.compiler.policy, {
    maxRawBytes: v7.roles.compiler.policy.maxRawBytes,
    basis: "unchanged-v7-compiler-ceiling",
    gzip: "diagnostic-only",
  });

  // V9 (roadmap C4a): the atomic explicit-selection operation is excised, and
  // the canonical compiler returns BYTE-IDENTICAL to its pre-atomic artifact
  // (175212B — the C1-era compiler), which pins the excision as exact. The
  // untouched runtime role keeps its accepted PR-338 snapshot.
  assert.equal(v9.schemaVersion, 7);
  assert.equal(v9.budgetId, "labcolors-wasm-roles-c4a-v9");
  assert.deepEqual(v9.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v8.json",
    fileSha256: expectedHashes.v8,
  });
  assert.deepEqual(v9.toolchainSource, v8.toolchainSource);
  assert.deepEqual(v9.buildRecipes, v8.buildRecipes);
  assert.deepEqual(v9.roles.runtime, v8.roles.runtime);
  assert.deepEqual(v9.roles.compiler.measurement, {
    source: "github-actions-run-29571640106",
    measurementPlatform: "linux-x64",
    rawBytes: 175212,
  });
  assert.deepEqual(v9.roles.compiler.policy, {
    maxRawBytes: 175212,
    basis: "accepted-c4a-excision-snapshot",
    gzip: "diagnostic-only",
  });
  assert.ok(
    v9.roles.compiler.policy.maxRawBytes < v8.roles.compiler.policy.maxRawBytes,
    "C4a must shrink the compiler role, never grow it",
  );

  // V10 (failure admissibility): wire-строки ролевых отказов + жёсткий страж
  // реентерабельности кэша стоят +2545B runtime над принятым PR-338 снапшотом;
  // compiler не тронут и держит C4a pre-atomic ратчет.
  assert.equal(v10.schemaVersion, 7);
  assert.equal(v10.budgetId, "labcolors-wasm-roles-failure-admissibility-v10");
  assert.deepEqual(v10.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v9.json",
    fileSha256: expectedHashes.v9,
  });
  assert.deepEqual(v10.toolchainSource, v9.toolchainSource);
  assert.deepEqual(v10.buildRecipes, v9.buildRecipes);
  assert.deepEqual(v10.roles.runtime.measurement, {
    source: "github-actions-run-29578036842",
    measurementPlatform: "linux-x64",
    rawBytes: 459241,
  });
  assert.equal(
    v10.roles.runtime.policy.basis,
    "accepted-failure-admissibility-runtime-snapshot",
  );
  assert.equal(v10.roles.runtime.policy.maxRawBytes, 459241);
  assert.deepEqual(v10.roles.compiler, v9.roles.compiler);

  // V11 (C4d): the offline compiler line is excised, so the budget collapses to
  // the single runtime ratchet. The runtime crate is untouched by the excision:
  // the carried measurement stays byte-identical to the immutable v10 snapshot.
  assert.equal(v11.schemaVersion, 8);
  assert.equal(v11.budgetId, "labcolors-wasm-runtime-c4cd-v11");
  assert.deepEqual(v11.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v10.json",
    fileSha256: expectedHashes.v10,
  });
  assert.deepEqual(v11.toolchainSource, v10.toolchainSource);
  assert.deepEqual(Object.keys(v11.buildRecipes), ["runtime"]);
  assert.deepEqual(Object.keys(v11.roles), ["runtime"]);
  assert.deepEqual(v11.buildRecipes.runtime, v10.buildRecipes.runtime);
  assert.deepEqual(v11.roles.runtime, v10.roles.runtime);

  // V12 (C5.1): словарь клиентских theme-ключей вместо fixed enum + отказ
  // EmptyThemes на загрузке — принятый рост runtime +524B, зафиксирован новым
  // точным снапшотом (run 29609974767).
  assert.equal(v12.schemaVersion, 8);
  assert.equal(v12.budgetId, "labcolors-wasm-runtime-c5-theme-keys-v12");
  assert.deepEqual(v12.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v11.json",
    fileSha256: expectedHashes.v11,
  });
  assert.deepEqual(v12.toolchainSource, v11.toolchainSource);
  assert.deepEqual(v12.buildRecipes, v11.buildRecipes);
  assert.deepEqual(v12.roles.runtime.measurement, {
    source: "github-actions-run-29609974767",
    measurementPlatform: "linux-x64",
    rawBytes: 459765,
  });
  assert.deepEqual(v12.roles.runtime.policy, {
    maxRawBytes: 459765,
    basis: "accepted-c5-theme-dictionary-snapshot",
    gzip: "diagnostic-only",
  });
  assert.equal(
    v12.roles.runtime.policy.maxRawBytes - v11.roles.runtime.policy.maxRawBytes,
    524,
    "C5.1 growth is the exact accepted dictionary-lookup delta",
  );

  // V13 (C5.2): вырез legacy cleanliness-прокси (muddiness-метод и его кодовые
  // пути) — принятое СНИЖЕНИЕ runtime −4691B (run 29613131229).
  assert.equal(v13.schemaVersion, 8);
  assert.equal(v13.budgetId, "labcolors-wasm-runtime-c5-2-proxy-excision-v13");
  assert.deepEqual(v13.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v12.json",
    fileSha256: expectedHashes.v12,
  });
  assert.deepEqual(v13.toolchainSource, v12.toolchainSource);
  assert.deepEqual(v13.buildRecipes, v12.buildRecipes);
  assert.deepEqual(v13.roles.runtime.measurement, {
    source: "github-actions-run-29613131229",
    measurementPlatform: "linux-x64",
    rawBytes: 455074,
  });
  assert.deepEqual(v13.roles.runtime.policy, {
    maxRawBytes: 455074,
    basis: "accepted-c5-2-proxy-excision-snapshot",
    gzip: "diagnostic-only",
  });
  assert.equal(
    v12.roles.runtime.policy.maxRawBytes - v13.roles.runtime.policy.maxRawBytes,
    4691,
    "C5.2 must shrink the runtime by the exact excision delta",
  );

  // V14 (C6): legacy sentiment/recipe API вырезан из Core и WASM — канонический
  // Linux runtime уменьшился ещё на 26534B (run 29647236232).
  assert.equal(v14.schemaVersion, 8);
  assert.equal(v14.budgetId, "labcolors-wasm-runtime-c6-legacy-excision-v14");
  assert.deepEqual(v14.predecessor, {
    path: "packages/colors/bench/wasm-size-budget-v13.json",
    fileSha256: expectedHashes.v13,
  });
  assert.deepEqual(v14.toolchainSource, v13.toolchainSource);
  assert.deepEqual(v14.buildRecipes, v13.buildRecipes);
  assert.deepEqual(v14.roles.runtime.measurement, {
    source: "github-actions-run-29647236232",
    measurementPlatform: "linux-x64",
    rawBytes: 428540,
  });
  assert.deepEqual(v14.roles.runtime.policy, {
    maxRawBytes: 428540,
    basis: "accepted-c6-legacy-excision-snapshot",
    gzip: "diagnostic-only",
  });
  assert.equal(
    v13.roles.runtime.policy.maxRawBytes - v14.roles.runtime.policy.maxRawBytes,
    26534,
    "C6 must shrink the runtime by the exact legacy-excision delta",
  );

  const checker = await import(
    new URL("../../../scripts/check-wasm-size-budget.mjs", import.meta.url)
  );
  assert.equal(checker.DEFAULT_BUDGET, paths.v14);
  for (const version of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]) {
    assert.equal(checker[`V${version}_FILE_SHA256`], expectedHashes[`v${version}`]);
  }
  assert.equal(checker.V1_RECIPE_SHA256, v5.buildRecipes.runtime.recipeSha256);
  const ci = read(".github", "workflows", "ci.yml");
  assert.match(ci, /name: enforce measured WASM runtime budget/u);
  assert.match(ci, /run: node scripts\/check-wasm-size-budget\.mjs/u);
  const wasmJob = ci.match(
    /\n  wasm:\n(?<body>[\s\S]*?)(?=\n  [a-z][a-z0-9_-]*:\n|\s*$)/u,
  )?.groups?.body;
  assert.ok(wasmJob, "CI must contain a bounded wasm job");
  assert.match(wasmJob, /runs-on: \[self-hosted, Linux, X64\]/u);
  assert.match(wasmJob, /GITHUB_WORKSPACE=\/workspace\/lab-colors/u);
  assert.match(wasmJob, /CARGO_HOME=\/cargo-home/u);
  const repetition = workflowRunScript(
    ci,
    "name: repeat runtime WASM build in one toolchain-pinned CI job",
  );
  const recipePrefix = "CARGO_ENCODED_RUSTFLAGS=<rustPathRemap> ";
  const expectedRemapExport = `export CARGO_ENCODED_RUSTFLAGS=${v1.measurement.rustPathRemap
    .map((mapping) => {
      const separator = mapping.indexOf("=");
      assert.ok(separator > 0, "path remap must name one environment source");
      return `"--remap-path-prefix=\$${mapping.slice(0, separator)}=${mapping.slice(separator + 1)}"`;
    })
    .join("$'\\x1f'")}`;
  const runtimeCommand = v14.buildRecipes.runtime.command;
  assert.ok(runtimeCommand.startsWith(recipePrefix));
  const expectedBuild = runtimeCommand.slice(recipePrefix.length);
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
      "the live path-remap command must equal the versioned budget declaration",
    );
    const functionBody = script.match(
      /(?:^|\n)build_runtime\(\) \{\n(?<body>(?:  [^\n]+\n)+)\}/u,
    )?.groups?.body;
    assert.ok(functionBody, "build_runtime must be one bounded shell function");
    assert.deepEqual(
      functionBody.split("\n").map((line) => line.trim()).filter(Boolean),
      [expectedBuild],
      "the CI build command must equal the versioned budget recipe command",
    );
    assert.equal(
      script.match(/^build_runtime$/gmu)?.length,
      2,
      "the same recipe must run exactly twice",
    );
    assert.match(script, /^cargo clean$/mu);
    assert.match(script, /^cp -a packages\/colors\/pkg "\$first\/pkg"$/mu);
    assert.deepEqual(
      [...script.matchAll(/^if ! diff[^\n]+\n  echo [^\n]+\n  exit 1\nfi$/gmu)]
        .map((match) => match[0]),
      [expectedDiffBlock],
      "the output directory must be compared by one fail-closed exact command",
    );
    assert.equal(
      script.match(
        /^for root in "\$GITHUB_WORKSPACE" "\$CARGO_HOME" "\$RUSTUP_HOME"; do\n  if LC_ALL=C grep -a -F -q -- "\$root\/" "\$wasm"; then\n    echo "unmapped build path \$root in \$wasm" >&2\n    exit 1\n  fi\ndone$/mu,
      )?.[0],
      expectedPathGuard,
      "host-path rejection must remain one fail-closed exact block",
    );
  };
  assertRepeatabilityContract(repetition);

  const recipeBypass = repetition.replace(
    expectedBuild,
    `${expectedBuild} --features unreviewed`,
  );
  assert.notEqual(recipeBypass, repetition, "recipe mutation must bite a real command");
  assert.throws(() => assertRepeatabilityContract(recipeBypass));

  const diffBypass = repetition.replace(
    expectedDiffBlock,
    expectedDiffBlock.replace("  exit 1", "  :"),
  );
  assert.notEqual(diffBypass, repetition, "diff mutation must bite a real command");
  assert.throws(() => assertRepeatabilityContract(diffBypass));

  const pathBypass = repetition.replace(
    expectedPathGuard,
    expectedPathGuard.replace("    exit 1", "    :"),
  );
  assert.notEqual(pathBypass, repetition, "path mutation must bite the live guard");
  assert.throws(() => assertRepeatabilityContract(pathBypass));

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-wasm-runtime-budget-v14-"));
  try {
    const runtimePath = join(temporary, "runtime.wasm");
    const fixtureBudgetPath = join(temporary, "budget.json");
    const runtimeBytes = Buffer.alloc(16);
    runtimeBytes.set([0x00, 0x61, 0x73, 0x6d]);
    const fixture = structuredClone(v14);
    fixture.roles.runtime.measurement.rawBytes = runtimeBytes.length;
    fixture.roles.runtime.policy.maxRawBytes = runtimeBytes.length;
    writeFileSync(runtimePath, runtimeBytes);
    writeFileSync(fixtureBudgetPath, canonicalJson(fixture));
    assert.doesNotThrow(() =>
      checker.parseBudgetDocument(readFileSync(fixtureBudgetPath), fixtureBudgetPath)
    );

    const run = () => execFileSync(
      process.execPath,
      [
        checkerPath,
        "--budget",
        fixtureBudgetPath,
        "--runtime-wasm",
        runtimePath,
      ],
      { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    const output = run();
    assert.match(
      output,
      /role=runtime raw=16B .*artifact-sha256=[0-9a-f]{64}/u,
    );
    assert.doesNotMatch(output, /role=compiler/u, "the compiler role must stay collapsed");
    assert.doesNotMatch(
      output,
      /(?:recipe|artifact)-sha(?:256)?=match/u,
      "the size checker cannot infer artifact provenance from arbitrary input bytes",
    );

    const schemaMutations = [
      ["schema rollback", (value) => { value.schemaVersion = 7; }],
      ["identity drift", (value) => { value.budgetId = "other"; }],
      ["predecessor path", (value) => { value.predecessor.path = "other.json"; }],
      ["predecessor hash", (value) => { value.predecessor.fileSha256 = "0".repeat(64); }],
      ["toolchain path", (value) => { value.toolchainSource.path = "other.json"; }],
      ["toolchain hash", (value) => { value.toolchainSource.fileSha256 = "0".repeat(64); }],
      ["missing runtime recipe", (value) => { delete value.buildRecipes.runtime; }],
      ["extra recipe", (value) => { value.buildRecipes.compiler = value.buildRecipes.runtime; }],
      ["runtime command drift", (value) => {
        value.buildRecipes.runtime.command += " --features unreviewed";
      }],
      ["runtime recipe digest", (value) => {
        value.buildRecipes.runtime.recipeSha256 = "0".repeat(64);
      }],
      ["missing runtime role", (value) => { delete value.roles.runtime; }],
      ["extra role", (value) => { value.roles.compiler = value.roles.runtime; }],
      ["artifact drift", (value) => {
        value.roles.runtime.artifact = "packages/colors/compiler/labcolors_compiler_bg.wasm";
      }],
      ["runtime measurement source", (value) => {
        value.roles.runtime.measurement.source = "github-actions-run-other";
      }],
      ["measurement platform", (value) => {
        value.roles.runtime.measurement.measurementPlatform = "darwin-arm64";
      }],
      ["zero bytes", (value) => { value.roles.runtime.measurement.rawBytes = 0; }],
      ["fractional bytes", (value) => { value.roles.runtime.measurement.rawBytes = 1.5; }],
      ["artifact digest conflation", (value) => {
        value.roles.runtime.measurement.sha256 = "0".repeat(64);
      }],
      ["ceiling mismatch", (value) => { value.roles.runtime.policy.maxRawBytes += 1; }],
      ["basis drift", (value) => { value.roles.runtime.policy.basis = "guessed"; }],
      ["gzip gate", (value) => { value.roles.runtime.policy.gzip = "gate"; }],
      ["unscoped runtime growth", (value) => {
        // Согласованный рост: одновременно поднимаем measurement и ceiling —
        // именно так выглядел бы «честный» новый снапшот без принятого
        // acceptedCeiling-закона. Чекер обязан отклонить его всё равно:
        // рост сверх принятого снапшота требует НОВОЙ версии бюджета,
        // а не правки текущей.
        const ceiling = v14.roles.runtime.policy.maxRawBytes;
        value.roles.runtime.measurement.rawBytes = ceiling + 1;
        value.roles.runtime.policy.maxRawBytes = ceiling + 1;
      }],
      ["whole-call cycle", (value) => { value.wholeCallArtifact = "forbidden"; }],
      ["top-level key reorder", (value) => ({
        budgetId: value.budgetId,
        schemaVersion: value.schemaVersion,
        predecessor: value.predecessor,
        toolchainSource: value.toolchainSource,
        buildRecipes: value.buildRecipes,
        roles: value.roles,
      })],
    ];
    assert.equal(schemaMutations.length, 24, "v14 schema mutation set changed");
    for (const [name, mutate] of schemaMutations) {
      const invalid = structuredClone(fixture);
      const result = mutate(invalid) ?? invalid;
      writeFileSync(fixtureBudgetPath, canonicalJson(result));
      assert.throws(run, undefined, `${name} must fail the checker`);
    }

    writeFileSync(fixtureBudgetPath, `${JSON.stringify(fixture)}\n`);
    assert.throws(run, /canonical JSON/u, "non-canonical JSON must fail");
    writeFileSync(
      fixtureBudgetPath,
      canonicalJson(fixture).replace(
        '  "schemaVersion": 8,\n',
        '  "schemaVersion": 8,\n  "schemaVersion": 8,\n',
      ),
    );
    assert.throws(run, /canonical JSON/u, "duplicate JSON fields must fail");

    const record = fixture.roles.runtime;
    const canonical = checker.evaluateWasmBudget("runtime", record, runtimeBytes, "linux-x64");
    assert.equal(canonical.status, "PASS");
    assert.equal(canonical.artifactSha256, sha256(runtimeBytes));
    const sameSizeMutation = Buffer.from(runtimeBytes);
    sameSizeMutation[sameSizeMutation.length - 1] = 1;
    const sameSize = checker.evaluateWasmBudget(
      "runtime",
      record,
      sameSizeMutation,
      "linux-x64",
    );
    assert.equal(sameSize.status, "PASS");
    assert.notEqual(sameSize.artifactSha256, canonical.artifactSha256);
    assert.throws(
      () => checker.evaluateWasmBudget(
        "runtime",
        record,
        Buffer.concat([runtimeBytes, Buffer.from([0])]),
        "linux-x64",
      ),
      /length mismatch/u,
    );
    assert.throws(
      () => checker.evaluateWasmBudget("runtime", record, runtimeBytes.subarray(0, -1), "linux-x64"),
      /length mismatch/u,
    );
    assert.equal(
      checker.evaluateWasmBudget("runtime", record, sameSizeMutation, "darwin-arm64").status,
      "DIAGNOSTIC",
    );
    assert.throws(
      () => checker.evaluateWasmBudget("compiler", record, runtimeBytes, "linux-x64"),
      /unknown execution role/u,
      "the collapsed compiler role must stay rejected",
    );

    const coordinatedMutation = structuredClone(fixture);
    coordinatedMutation.roles.runtime.measurement.rawBytes -= 1;
    coordinatedMutation.roles.runtime.policy.maxRawBytes -= 1;
    assert.throws(
      () => checker.parseBudgetDocument(Buffer.from(canonicalJson(coordinatedMutation)), paths.v14),
      /current v14 file SHA-256 mismatch/u,
      "coordinated artifact and document drift must still fail the default identity",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("runtime WASM does not duplicate separately shipped WCAG22 evidence documents", () => {
  const wasm = readFileSync(
    join(root, "packages", "colors", "pkg", "labcolors_bg.wasm"),
  );
  for (const name of [
    "wcag22-srgb8-v1.json",
    "wcag22-srgb8-q55-proof-v1.json",
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

test("npm release carries and re-verifies the exact WCAG22 finite evidence", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const evidenceFiles = [
    "evidence/wcag22-srgb8-v1.json",
    "evidence/wcag22-srgb8-q55-v1.bin",
    "evidence/wcag22-srgb8-q55-proof-v1.json",
  ];
  for (const path of evidenceFiles) {
    assert.ok(packageJson.files.includes(path), `npm files omits ${path}`);
  }

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
  for (const name of evidenceFiles.map((path) => path.split("/").at(-1))) {
    assert.match(prepare, new RegExp(name.replaceAll(".", "\\."), "u"));
  }

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /verify_wcag22_q55\.py/);
  assert.match(verifier, /WCAG22_EVIDENCE_FILES/);
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
});

test("packed and clean-installed WCAG22 evidence stays byte-exact", async () => {
  const names = [
    "wcag22-srgb8-v1.json",
    "wcag22-srgb8-q55-v1.bin",
    "wcag22-srgb8-q55-proof-v1.json",
  ];
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
      validateWcag22EvidenceArtifacts(temporary, expected, "fixture"),
    );

    const corrupted = Buffer.from(contents[0]);
    corrupted[0] ^= 1;
    writeFileSync(join(evidenceDir, names[0]), corrupted);
    await assert.rejects(
      validateWcag22EvidenceArtifacts(temporary, expected, "fixture"),
      /fixture WCAG22 evidence bytes differ/u,
      "same-length evidence corruption must fail",
    );

    writeFileSync(join(evidenceDir, names[0]), contents[0]);
    const wrongDigest = structuredClone(expected);
    wrongDigest[0].sha256 = "0".repeat(64);
    await assert.rejects(
      validateWcag22EvidenceArtifacts(temporary, wrongDigest, "fixture"),
      /fixture WCAG22 evidence metadata differs/u,
      "expected digest drift must fail independently of the byte comparison",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(
    verifier,
    /validatePackedWcag22Evidence\(canonicalPack\.path, wcag22Evidence\.artifacts\)/u,
  );
  assert.match(
    verifier,
    /verifyCleanConsumer\([\s\S]*?wcag22Evidence\.artifacts[\s\S]*?\);/u,
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
  for (const subpath of ["apply-theme", "watch-theme", "adapt-theme", "effective-bg"]) {
    assert.match(
      verifier,
      new RegExp(`@labpics/colors/${subpath}`, "u"),
      `clean-consumer type smoke must compile the ${subpath} public subpath`,
    );
  }

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
    "effective-bg.d.ts",
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
