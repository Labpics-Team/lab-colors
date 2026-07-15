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
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  validateWcag22EvidenceArtifacts,
  validateWcag22FeasibilityFamily,
} from "../../../scripts/verify-package-release.mjs";

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

test("breaking release metadata is one explicit 0.2.0/0.10.0 contract", () => {
  const workspace = read("Cargo.toml");
  assert.match(workspace, /\[workspace\.package\][\s\S]*\nversion = "0\.2\.0"/);
  assert.match(workspace, /\nrust-version = "1\.85"/);
  assert.match(
    workspace,
    /repository = "https:\/\/github\.com\/Labpics-Team\/lab-colors"/,
  );

  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const packageLock = JSON.parse(read("packages", "colors", "package-lock.json"));
  assert.equal(packageJson.version, "0.10.0");
  assert.equal(packageJson.packageManager, "npm@11.9.0");
  assert.equal(packageLock.version, "0.10.0");
  assert.equal(packageLock.packages[""].version, "0.10.0");
  assert.equal(packageJson.engines.node, ">=22.11.0");
  assert.equal(packageLock.packages[""].engines.node, ">=22.11.0");
  assert.equal(
    packageJson.scripts.prepack,
    "npm run build && node ../../scripts/prepare-npm-package.mjs",
  );
  assert.match(packageJson.scripts.build, /wasm-pack build .* --locked$/);
});

test("every workspace package inherits the declared MSRV", () => {
  const manifests = readdirSync(join(root, "crates"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join("crates", entry.name, "Cargo.toml"));
  manifests.push(join("experiments", "psychophysics", "Cargo.toml"));
  assert.ok(manifests.length > 1, "anti-vacuum: workspace package list is non-trivial");
  for (const manifest of manifests) {
    assert.match(
      read(manifest),
      /^rust-version\.workspace = true$/m,
      `${manifest} не публикует/не наследует workspace MSRV`,
    );
  }
});

test("WCAG22 feasibility projects only the registered-domain capability through transports", () => {
  const isolatedCoreEdge =
    /labcolors-core = \{ path = "\.\.\/labcolors-core", default-features = false \}/u;
  const protocolEdge = /labcolors-protocol = \{ path = "\.\.\/labcolors-protocol" \}/u;
  const protocolManifest = read("crates", "labcolors-protocol", "Cargo.toml");
  const wasmManifest = read("crates", "labcolors-wasm", "Cargo.toml");
  const ffiManifest = read("crates", "labcolors-ffi", "Cargo.toml");
  const conformanceManifest = read("crates", "labcolors-conformance", "Cargo.toml");

  assert.match(
    protocolManifest,
    /labcolors-core = \{ path = "\.\.\/labcolors-core", default-features = false, features = \["wcag22-feasibility"\] \}/u,
  );
  for (const manifest of [wasmManifest, ffiManifest, conformanceManifest]) {
    assert.match(manifest, isolatedCoreEdge);
    assert.match(manifest, protocolEdge);
    assert.doesNotMatch(manifest, /features = \["wcag22-feasibility"\]/u);
  }

  const ci = read(".github", "workflows", "ci.yml");
  const projection = workflowRunScript(
    ci,
    "name: prove core capability projection boundary",
  );
  const declaredConsumers = projection.match(
    /consumers = \(\n(?<items>(?:    "[^"]+",\n)+)\)/u,
  )?.groups?.items;
  assert.ok(declaredConsumers, "CI must declare one consumer SSOT");
  assert.deepEqual(
    [...declaredConsumers.matchAll(/"([^"]+)"/gu)].map((match) => match[1]),
    ["labcolors-wasm", "labcolors-ffi", "labcolors-conformance"],
  );
  assert.equal(
    projection.match(/for consumer in consumers:/gu)?.length,
    1,
    "the dependency and feature-tree checks must share one consumer loop",
  );
  assert.match(
    projection,
    /core\["features"\]\.get\("default"\) != \[\n\s+"wcag22-feasibility",\n\s+"wcag22-explicit-feasibility",\n\s*\]:/u,
  );
  assert.match(projection, /protocol_core\["features"\] != \["wcag22-feasibility"\]/u);
  assert.match(projection, /core_dependency\["features"\]/u);
  assert.match(projection, /dependency\["name"\] == "labcolors-protocol"/u);
  assert.match(
    projection,
    /\["cargo", "tree", "-p", consumer, "--edges", "normal", "-e", "features"\]/u,
  );
  assert.match(
    projection,
    /'labcolors-core feature "wcag22-explicit-feasibility"' in feature_tree/u,
  );
  assert.doesNotMatch(projection, /for consumer in labcolors-/u);
});

test("MSRV and packaged Rust crate gates are executable CI contracts", () => {
  assert.ok(existsSync(join(root, "LICENSE")), "root LICENSE отсутствует");
  const coreLicense = join(root, "crates", "labcolors-core", "LICENSE");
  const wasmLicense = join(root, "crates", "labcolors-wasm", "LICENSE");
  for (const license of [coreLicense, wasmLicense]) {
    assert.ok(lstatSync(license).isSymbolicLink(), `${license} must preserve the root SSOT`);
    assert.equal(readlinkSync(license), "../../LICENSE");
    assert.equal(readFileSync(license, "utf8"), read("LICENSE"));
  }
  const coreManifest = read("crates", "labcolors-core", "Cargo.toml");
  const coreLib = read("crates", "labcolors-core", "src", "lib.rs");
  assert.match(coreManifest, /^description = "[^"]+"$/m);
  assert.match(coreManifest, /^readme = "README\.md"$/m);
  assert.match(coreManifest, /^license\.workspace = true$/m);
  assert.doesNotMatch(coreManifest, /^license-file\s*=/m);
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
    /^\s*node-consumer-floor:[\s\S]*needs: wasm[\s\S]*node-version: \$\{\{ env\.NODE_CONSUMER_FLOOR \}\}[\s\S]*actions\/download-artifact@[0-9a-f]{40}[\s\S]*--runtime-smoke/m,
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
    "docs-drift (нейминг-канон)",
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
    "docs-drift (нейминг-канон)",
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

    const tarball = join(artifact, "labpics-colors-0.10.0.tgz");
    execFileSync("tar", ["-czf", tarball, "-C", join(temporary, "payload"), "package"]);
    const bytes = readFileSync(tarball);
    const expectedSha = "a".repeat(40);
    const manifest = {
      // Release-manifest schema v2: numericalCapabilities вместо numericalSites
      // (см. verify-package-release.mjs); validator publish-workflow пиняет 2.
      schemaVersion: 2,
      npm: "0.10.0",
      sourceSha: expectedSha,
      artifacts: {
        tarball: {
          path: ".release/labpics-colors-0.10.0.tgz",
          bytes: bytes.length,
          sha256: createHash("sha256").update(bytes).digest("hex"),
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

test("conformance pack 5 adds only the feasibility family", () => {
  const immutableFamilies = new Map([
    ["contrasts.json", "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145"],
    ["ladders.json", "496f562e55ad8110aeb8a07042b1964ec9ff4d0f1e8c09e362d1b2d14c513036"],
    ["alpha.json", "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1"],
    ["solve.json", "64acfc4a8c613a4b11e4e83c52a33ecf308320abc6ab18fde20853a7f2399f06"],
    ["muddiness.json", "3c5497b251f04c089d33452b9bf0bfba7f4ef9a72dc496180ff42aad08377aa3"],
    ["wcag22.json", "6e234fa3a0d4e2b21f515b8f4e6be76f223768821e0308e774c31a5ce7a1d826"],
  ]);
  assert.equal(immutableFamilies.size, 6, "anti-vacuum: prior family set changed");
  for (const [name, expected] of immutableFamilies) {
    const bytes = readFileSync(join(root, "conformance", "vectors", name));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected, name);
  }

  const manifest = JSON.parse(read("conformance", "vectors", "manifest.json"));
  assert.equal(manifest.packVersion, "5.0.0");
  assert.ok(
    existsSync(join(root, "conformance", "vectors", "wcag22-feasibility.json")),
    "pack 5 must add the single feasibility family",
  );
  assert.equal(manifest.counts.wcag22Feasibility > 0, true);
});

test("release checker independently validates and mutation-proves feasibility pack semantics", () => {
  const canonical = JSON.parse(
    read("conformance", "vectors", "wcag22-feasibility.json"),
  );
  const atomicProofSha256 = createHash("sha256")
    .update(readFileSync(join(
      root,
      "crates",
      "labcolors-core",
      "contracts",
      "wcag22-srgb8-q55-proof-v1.json",
    )))
    .digest("hex");
  assert.doesNotThrow(() => validateWcag22FeasibilityFamily(canonical, atomicProofSha256));

  const mutateOutcome = (family, caseId, mutate) => {
    const vector = family.find((entry) => entry.caseId === caseId);
    assert.ok(vector, `mutation fixture missing ${caseId}`);
    const outcome = JSON.parse(vector.outcomeJson);
    mutate(outcome);
    vector.outcomeJson = JSON.stringify(outcome);
  };
  const mutations = [
    ["vector schema expansion", (family) => { family[0].extra = true; }],
    ["non-compact request", (family) => { family[0].requestJson += " "; }],
    ["proportional cells", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.cells = []; },
    )],
    ["domain truncation", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.domain.pop(); },
    )],
    ["matrix truncation", (family) => mutateOutcome(
      family,
      "text-default-two",
      (outcome) => { outcome.feasibility.result.failureMatrix.pop(); },
    )],
    ["partition LSB0 flip", (family) => mutateOutcome(
      family,
      "ui-component-fifty-nine",
      (outcome) => { outcome.feasibility.result.proof.partition[0] ^= 1; },
    )],
    ["domain digest flip", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.proof.domainDigest[0] ^= 1; },
    )],
    ["relation-set digest flip", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.proof.relationSetDigest[0] ^= 1; },
    )],
    ["evaluation ID flip", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.proof.evaluationId[0] ^= 1; },
    )],
    ["atomic proof digest flip", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.proof.proofSha256[0] ^= 1; },
    )],
    ["not-evaluated domain digest flip", (family) => mutateOutcome(
      family,
      "all-not-applicable",
      (outcome) => { outcome.feasibility.result.domainDigest[0] ^= 1; },
    )],
    ["not-evaluated relation-set digest flip", (family) => mutateOutcome(
      family,
      "all-not-applicable",
      (outcome) => { outcome.feasibility.result.relationSetDigest[0] ^= 1; },
    )],
    ["numeric u64", (family) => mutateOutcome(
      family,
      "text-default-seven",
      (outcome) => { outcome.feasibility.result.proof.domainCount = 256; },
    )],
    ["conflict collapsed", (family) => {
      const source = family.find((entry) => entry.caseId === "all-not-applicable");
      const target = family.find((entry) => entry.caseId === "conflicting-relation-id");
      target.outcomeJson = source.outcomeJson;
    }],
    ["resource failure collapsed", (family) => {
      const source = family.find((entry) => entry.caseId === "all-not-applicable");
      const target = family.find((entry) => entry.caseId === "raw-adjacent-resource-rejection");
      target.outcomeJson = source.outcomeJson;
    }],
    ["opaque identity collapsed", (family) => {
      const first = family.find((entry) => entry.caseId === "opaque-identity-a");
      const second = family.find((entry) => entry.caseId === "opaque-identity-b");
      const firstOutcome = JSON.parse(first.outcomeJson);
      const secondOutcome = JSON.parse(second.outcomeJson);
      secondOutcome.feasibility.result.proof.relationSetDigest =
        firstOutcome.feasibility.result.proof.relationSetDigest;
      second.outcomeJson = JSON.stringify(secondOutcome);
    }],
  ];
  assert.equal(mutations.length, 16, "anti-vacuum mutation corpus changed");
  for (const [name, mutate] of mutations) {
    const family = structuredClone(canonical);
    mutate(family);
    assert.throws(
      () => validateWcag22FeasibilityFamily(family, atomicProofSha256),
      undefined,
      `${name} must fail the release checker`,
    );
  }
});

test("release evidence carries the versioned WCAG22 feasibility operation", () => {
  const prepare = read("scripts", "prepare-npm-package.mjs");
  const verifier = read("scripts", "verify-package-release.mjs");

  assert.match(prepare, /"wcag22-feasibility\.json"/u);
  assert.match(verifier, /"wcag22-feasibility\.json"/u);
  assert.match(verifier, /conformance\.packVersion !== "5\.0\.0"/u);
  assert.match(
    verifier,
    /"wcag22Feasibility"/u,
    "release count projection must include the new family",
  );
  assert.match(
    verifier,
    /"wcag22-feasibility-v1"/u,
    "release supported list must advertise the compiler operation",
  );
  assert.match(
    verifier,
    /validateWcag22FeasibilityFamily\(families\[6\], sha256\(proofBytes\)\)/u,
    "pack validation must bind feasibility proof fields to the canonical atomic proof bytes",
  );
  assert.match(verifier, /evaluateWcag22Feasibility/u);
  assert.match(verifier, /wcag22FeasibilityMaxBytes/u);
  assert.match(verifier, /type Wcag22FeasibilityRequestV1/u);
  assert.match(verifier, /type Wcag22FeasibilityOutcomeV1/u);
  assert.match(verifier, /get\("text-default-seven"\)\?\.vector/u);
  assert.match(
    verifier,
    /JSON\.stringify\(evaluateWcag22Feasibility\(feasibilityRequest\)\)[\s\S]*?feasibilityFixture\.outcomeJson/u,
  );
  assert.equal(
    verifier.match(/writeFile\(runtimePath, runtimeSmokeSource\(feasibilityFixture\)\)/gu)?.length,
    2,
    "clean-install and Node-floor smokes must execute the same canonical fixture",
  );
  assert.ok(
    verifier.indexOf("await init({ module_or_path:") <
      verifier.indexOf("wcag22FeasibilityMaxBytes()"),
    "clean smoke must import safely and call the getter only after WASM init",
  );
  assert.match(verifier, /@ts-expect-error byte API rejects strings/u);
  assert.match(verifier, /case "notEvaluated"/u);
  assert.match(verifier, /case "incompatibleCoreContract"/u);
});

test("WCAG22 WASM budget history is exact, append-only, and acyclic", async () => {
  const v1Path = join(root, "packages", "colors", "bench", "wasm-size-budget-v1.json");
  const v2Path = join(root, "packages", "colors", "bench", "wasm-size-budget-v2.json");
  const v3Path = join(root, "packages", "colors", "bench", "wasm-size-budget-v3.json");
  const v4Path = join(root, "packages", "colors", "bench", "wasm-size-budget-v4.json");
  const checkerPath = join(root, "scripts", "check-wasm-size-budget.mjs");
  const sha256 = (value) => createHash("sha256").update(value).digest("hex");
  const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;

  const v1Bytes = readFileSync(v1Path);
  const v1 = JSON.parse(v1Bytes);
  assert.equal(
    sha256(v1Bytes),
    "4f7340fc8cfd0ccb97377c385f2f8d8e7a9ef2c5ba96177f518c5d07de2825e1",
    "the immutable #284 evidence and build recipe must remain byte-identical",
  );
  const recipe = {
    rustToolchain: v1.measurement.rustToolchain,
    rustcCommit: v1.measurement.rustcCommit,
    wasmPack: v1.measurement.wasmPack,
    wasmBindgen: v1.measurement.wasmBindgen,
    target: v1.measurement.target,
    cargoProfile: v1.measurement.cargoProfile,
    wasmOpt: v1.measurement.wasmOpt,
    wasmOptVersion: v1.measurement.wasmOptVersion,
    measurementPlatform: v1.measurement.measurementPlatform,
    rustPathRemap: v1.measurement.rustPathRemap,
    command: v1.measurement.command,
  };
  assert.equal(
    sha256(JSON.stringify(recipe)),
    "0ea74cb070e0a5facb7280f6124930a0bb673ee4dcee9c99fff110db6c9389d4",
  );
  assert.deepEqual(v1.measurement.rustPathRemap, [
    "GITHUB_WORKSPACE=/workspace/lab-colors",
    "CARGO_HOME=/cargo-home",
  ]);

  const v2Bytes = readFileSync(v2Path);
  const v2 = JSON.parse(v2Bytes);
  assert.equal(v2Bytes.toString("utf8"), canonicalJson(v2));
  assert.equal(
    sha256(v2Bytes),
    "713ccc314b3e6f638d87a54716d665d52f77c86f34a2b6edefe0a354a499d8b1",
    "the admitted v2 document must be byte-immutable",
  );
  assert.deepEqual(Object.keys(v2), [
    "schemaVersion",
    "budgetId",
    "artifact",
    "buildRecipe",
    "measurement",
    "policy",
  ]);
  assert.deepEqual(Object.keys(v2.buildRecipe), ["path", "fileSha256", "recipeSha256"]);
  assert.deepEqual(Object.keys(v2.measurement), [
    "issue",
    "measurementPlatform",
    "rawBytes",
    "sha256",
  ]);
  assert.deepEqual(Object.keys(v2.policy), ["maxRawBytes", "derivation", "gzip"]);
  assert.equal(v2.schemaVersion, 3);
  assert.equal(v2.budgetId, "labcolors-wasm-raw-issue-295-v2");
  assert.equal(v2.artifact, "packages/colors/pkg/labcolors_bg.wasm");
  assert.deepEqual(v2.buildRecipe, {
    path: "packages/colors/bench/wasm-size-budget-v1.json",
    fileSha256: "4f7340fc8cfd0ccb97377c385f2f8d8e7a9ef2c5ba96177f518c5d07de2825e1",
    recipeSha256: "0ea74cb070e0a5facb7280f6124930a0bb673ee4dcee9c99fff110db6c9389d4",
  });
  assert.deepEqual(v2.measurement, {
    issue: 295,
    measurementPlatform: "linux-x64",
    rawBytes: 521240,
    sha256: "d37841bfb2615d05c8366b08dcc7e5aed1bbd3cf27c3db67896108c5ec9c9ca0",
  });
  assert.deepEqual(v2.policy, {
    maxRawBytes: 521240,
    derivation: "exact-accepted-issue-295-slice-b-measurement",
    gzip: "diagnostic-only",
  });

  const v3Bytes = readFileSync(v3Path);
  const v3 = JSON.parse(v3Bytes);
  assert.equal(v3Bytes.toString("utf8"), canonicalJson(v3));
  assert.equal(
    sha256(v3Bytes),
    "d7937612e4c33574a8af28845bb1dd30cca86fc39fc0206cac4c377de77fec15",
    "the admitted v3 document must be byte-immutable",
  );
  assert.deepEqual(Object.keys(v3), Object.keys(v2));
  assert.deepEqual(Object.keys(v3.buildRecipe), Object.keys(v2.buildRecipe));
  assert.deepEqual(Object.keys(v3.measurement), Object.keys(v2.measurement));
  assert.deepEqual(Object.keys(v3.policy), Object.keys(v2.policy));
  assert.equal(v3.schemaVersion, 3);
  assert.equal(v3.budgetId, "labcolors-wasm-raw-issue-296-v3");
  assert.equal(v3.artifact, "packages/colors/pkg/labcolors_bg.wasm");
  assert.deepEqual(v3.buildRecipe, v2.buildRecipe);
  assert.deepEqual(v3.measurement, {
    issue: 296,
    measurementPlatform: "linux-x64",
    rawBytes: 521231,
    sha256: "779379e914909ff1ddbb5afdd6554d026b586f3c71ef6b2cfeba3468bf93e029",
  });
  assert.deepEqual(v3.policy, {
    maxRawBytes: 521231,
    derivation: "exact-accepted-issue-296-slice-a-measurement",
    gzip: "diagnostic-only",
  });

  const v4Bytes = readFileSync(v4Path);
  const v4 = JSON.parse(v4Bytes);
  assert.equal(v4Bytes.toString("utf8"), canonicalJson(v4));
  assert.equal(
    sha256(v4Bytes),
    "c34fc10404dc7057a53a28592d18342078b5cd0e5dcaa888db482abf3f5fb23c",
    "the admitted v4 document must be byte-immutable",
  );
  assert.deepEqual(Object.keys(v4), Object.keys(v3));
  assert.deepEqual(Object.keys(v4.buildRecipe), Object.keys(v3.buildRecipe));
  assert.deepEqual(Object.keys(v4.measurement), Object.keys(v3.measurement));
  assert.deepEqual(Object.keys(v4.policy), Object.keys(v3.policy));
  assert.equal(v4.schemaVersion, 3);
  assert.equal(v4.budgetId, "labcolors-wasm-raw-issue-296-v4");
  assert.equal(v4.artifact, "packages/colors/pkg/labcolors_bg.wasm");
  assert.deepEqual(v4.buildRecipe, v3.buildRecipe);
  assert.deepEqual(v4.measurement, {
    issue: 296,
    measurementPlatform: "linux-x64",
    rawBytes: 520920,
    sha256: "c179f42cd90c24699167ee78b4080c80fb38247c54953e7dc020483f6fcf94ed",
  });
  assert.deepEqual(v4.policy, {
    maxRawBytes: 520920,
    derivation: "exact-accepted-issue-296-slice-b-measurement",
    gzip: "diagnostic-only",
  });
  assert.ok(v4.policy.maxRawBytes <= v3.policy.maxRawBytes, "the v4 ratchet may only tighten");

  const checker = await import(
    new URL("../../../scripts/check-wasm-size-budget.mjs", import.meta.url)
  );
  assert.equal(checker.DEFAULT_BUDGET, v4Path);
  assert.equal(checker.V1_FILE_SHA256, v4.buildRecipe.fileSha256);
  assert.equal(checker.V1_RECIPE_SHA256, v4.buildRecipe.recipeSha256);
  assert.equal(checker.V2_FILE_SHA256, sha256(v2Bytes));
  assert.equal(checker.V3_FILE_SHA256, sha256(v3Bytes));
  assert.equal(checker.V4_FILE_SHA256, sha256(v4Bytes));

  const wholeCallSource = read(
    "packages",
    "colors",
    "bench",
    "wcag22-feasibility-boundary.bench.mjs",
  );
  assert.match(wholeCallSource, /wasmToolchainPath = resolve\(here, "wasm-size-budget-v1\.json"\)/u);
  assert.doesNotMatch(wholeCallSource, /wasm-size-budget-v[234]\.json/u);
  assert.doesNotMatch(
    read("scripts", "check-wasm-size-budget.mjs"),
    /wcag22-feasibility-wasm-boundary-v1\.json/u,
    "the sibling whole-call artifact must not become a size-budget dependency",
  );

  const ci = read(".github", "workflows", "ci.yml");
  assert.match(ci, /name: enforce measured WASM raw-byte budget/u);
  assert.match(ci, /run: node scripts\/check-wasm-size-budget\.mjs/u);
  assert.doesNotMatch(ci, /Not a hard gate yet/u);
  const wasmJob = ci.match(/\n  wasm:\n(?<body>[\s\S]*?)(?=\n  [a-z][a-z0-9_-]*:\n)/u)?.groups?.body;
  assert.ok(wasmJob, "CI must contain a bounded wasm job");
  assert.match(wasmJob, /runs-on: \[self-hosted, Linux, X64\]/u);
  assert.match(wasmJob, /CARGO_ENCODED_RUSTFLAGS/u);
  assert.match(wasmJob, /GITHUB_WORKSPACE=\/workspace\/lab-colors/u);
  assert.match(wasmJob, /CARGO_HOME=\/cargo-home/u);

  const builtWasm = readFileSync(
    join(root, "packages", "colors", "pkg", "labcolors_bg.wasm"),
  ).toString("latin1");
  assert.match(builtWasm, /\/cargo-home\/registry\/src\//u);
  assert.doesNotMatch(builtWasm, /\/(?:Users|home)\/[^\0]*?\/\.cargo\/registry\/src\//u);
  assert.doesNotMatch(
    builtWasm,
    /\/opt\/actions-runner\/[^\0]*?\/cargo-wasm\/registry\/src\//u,
  );

  const temporary = mkdtempSync(join(tmpdir(), "labcolors-wasm-budget-v4-"));
  try {
    const wasmPath = join(temporary, "fixture.wasm");
    const fixtureBudgetPath = join(temporary, "budget.json");
    const bytes = Buffer.alloc(16);
    bytes.set([0x00, 0x61, 0x73, 0x6d]);
    const fixture = structuredClone(v4);
    fixture.measurement.rawBytes = bytes.length;
    fixture.measurement.sha256 = sha256(bytes);
    fixture.policy.maxRawBytes = bytes.length;
    writeFileSync(wasmPath, bytes);
    writeFileSync(fixtureBudgetPath, canonicalJson(fixture));

    const run = () => execFileSync(
      process.execPath,
      [checkerPath, "--wasm", wasmPath, "--budget", fixtureBudgetPath],
      { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    assert.match(
      run(),
      /WASM size budget (?:PASS|DIAGNOSTIC) raw=16B .*artifact-sha=match/u,
    );

    const schemaMutations = [
      ["schema rollback", (value) => { value.schemaVersion = 2; }],
      ["identity drift", (value) => { value.budgetId = "other"; }],
      ["artifact path drift", (value) => { value.artifact = "other.wasm"; }],
      ["missing field", (value) => { delete value.artifact; }],
      ["unknown field", (value) => { value.unknown = true; }],
      ["recipe path drift", (value) => { value.buildRecipe.path = "other.json"; }],
      ["v1 file drift", (value) => { value.buildRecipe.fileSha256 = "0".repeat(64); }],
      ["recipe drift", (value) => { value.buildRecipe.recipeSha256 = "0".repeat(64); }],
      ["missing recipe field", (value) => { delete value.buildRecipe.recipeSha256; }],
      ["measurement issue drift", (value) => { value.measurement.issue = 295; }],
      ["measurement platform drift", (value) => {
        value.measurement.measurementPlatform = "darwin-arm64";
      }],
      ["unknown measurement field", (value) => { value.measurement.unknown = true; }],
      ["zero bytes", (value) => { value.measurement.rawBytes = 0; }],
      ["fractional bytes", (value) => { value.measurement.rawBytes = 1.5; }],
      ["unsafe bytes", (value) => {
        value.measurement.rawBytes = Number.MAX_SAFE_INTEGER + 1;
      }],
      ["invalid SHA", (value) => { value.measurement.sha256 = "0"; }],
      ["uppercase SHA", (value) => { value.measurement.sha256 = "A".repeat(64); }],
      ["ceiling plus one", (value) => { value.policy.maxRawBytes += 1; }],
      ["ceiling minus one", (value) => { value.policy.maxRawBytes -= 1; }],
      ["derivation drift", (value) => { value.policy.derivation = "guessed"; }],
      ["gzip gate", (value) => { value.policy.gzip = 123; }],
      ["missing policy field", (value) => { delete value.policy.gzip; }],
      ["whole-call cycle", (value) => { value.wholeCallArtifact = "forbidden"; }],
      ["ratchet regression", (value) => {
        value.measurement.rawBytes = v3.policy.maxRawBytes + 1;
        value.policy.maxRawBytes = v3.policy.maxRawBytes + 1;
      }],
      ["key reorder", (value) => ({
        budgetId: value.budgetId,
        schemaVersion: value.schemaVersion,
        artifact: value.artifact,
        buildRecipe: value.buildRecipe,
        measurement: value.measurement,
        policy: value.policy,
      })],
    ];
    assert.equal(schemaMutations.length, 25, "v4 schema mutation set changed");
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
        '  "schemaVersion": 3,\n',
        '  "schemaVersion": 3,\n  "schemaVersion": 3,\n',
      ),
    );
    assert.throws(run, /canonical JSON/u, "duplicate JSON fields must fail");

    const canonical = checker.evaluateWasmBudget(fixture, bytes, "linux-x64");
    assert.equal(canonical.status, "PASS");
    assert.equal(canonical.artifactSha, "match");

    const sameSizeMutation = Buffer.from(bytes);
    sameSizeMutation[15] = 1;
    assert.throws(
      () => checker.evaluateWasmBudget(fixture, sameSizeMutation, "linux-x64"),
      /SHA-256 mismatch/u,
      "same-size byte drift must fail",
    );
    assert.throws(
      () => checker.evaluateWasmBudget(
        fixture,
        Buffer.concat([bytes, Buffer.from([0])]),
        "linux-x64",
      ),
      /length mismatch/u,
      "append must fail",
    );
    assert.throws(
      () => checker.evaluateWasmBudget(fixture, bytes.subarray(0, 15), "linux-x64"),
      /length mismatch/u,
      "truncate must fail",
    );
    assert.equal(
      checker.evaluateWasmBudget(fixture, sameSizeMutation, "darwin-arm64").status,
      "DIAGNOSTIC",
      "non-canonical hosts report evidence without admitting it",
    );
    assert.equal(
      checker.evaluateWasmBudget(
        fixture,
        Buffer.concat([bytes, Buffer.from([0])]),
        "darwin-arm64",
      ).status,
      "DIAGNOSTIC",
      "non-canonical growth remains diagnostic",
    );
    assert.equal(
      checker.evaluateWasmBudget(fixture, bytes.subarray(0, 15), "darwin-arm64").status,
      "DIAGNOSTIC",
      "non-canonical shrink remains diagnostic",
    );

    const coordinatedMutation = structuredClone(fixture);
    coordinatedMutation.measurement.sha256 = sha256(sameSizeMutation);
    const coordinatedBytes = Buffer.from(canonicalJson(coordinatedMutation));
    assert.throws(
      () => checker.parseBudgetDocument(coordinatedBytes, v4Path),
      /immutable v4 file SHA-256 mismatch/u,
      "coordinated artifact and document drift must still fail the default identity",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("feasibility benchmark keeps V1/V2 history and admits exact V3 Core subjects", () => {
  const contractNames = readdirSync(join(
    root,
    "crates",
    "labcolors-core",
    "contracts",
  )).filter((name) => /^wcag22-feasibility-benchmark-v[0-9]+\.json$/u.test(name));
  assert.deepEqual(contractNames.sort(), [
    "wcag22-feasibility-benchmark-v1.json",
    "wcag22-feasibility-benchmark-v2.json",
    "wcag22-feasibility-benchmark-v3.json",
  ]);

  const checker = join(
    root,
    "scripts",
    "check_wcag22_feasibility_applicability.py",
  );
  assert.doesNotThrow(() => {
    execFileSync("python3", [
      checker,
      join(
        root,
        "crates",
        "labcolors-core",
        "contracts",
        "wcag22-feasibility-benchmark-v3.json",
      ),
      "--artifact-sha256",
      "28c4af13a83a04f4668c61fe3399a8e1e91355cd71f02e27af47fce150fc001a",
      "--self-test",
    ], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  });

  const benchmarkChecker = join(
    root,
    "scripts",
    "check_wcag22_feasibility_benchmark.py",
  );
  const canonicalArtifact = readFileSync(join(
    root,
    "crates",
    "labcolors-core",
    "contracts",
    "wcag22-feasibility-benchmark-v3.json",
  ));
  const canonicalPayload = JSON.parse(canonicalArtifact.toString("utf8"));
  const rustcRelease = canonicalPayload.environment.rustcVerbose
    .match(/^rustc ([^ ]+) /u)?.[1];
  const targetTriple = canonicalPayload.environment.rustcVerbose
    .match(/^host: (.+)$/mu)?.[1];
  assert.ok(rustcRelease, "canonical benchmark records its rustc release");
  assert.ok(targetTriple, "canonical benchmark records its target triple");
  const admissionPins = [
    "--admit-revision",
    canonicalPayload.environment.gitRevision,
    "--admit-rustc-release",
    rustcRelease,
    "--admit-target-triple",
    targetTriple,
    "--admit-target-arch",
    canonicalPayload.environment.targetArch,
    "--admit-target-os",
    canonicalPayload.environment.targetOs,
    "--admit-pointer-width-bits",
    String(canonicalPayload.environment.pointerWidthBits),
    "--admit-package-version",
    canonicalPayload.environment.packageVersion,
    "--admit-sample-count",
    String(canonicalPayload.sampleCount),
  ];
  const temporary = mkdtempSync(join(tmpdir(), "labcolors-benchmark-json-"));
  try {
    const hostileArtifact = join(temporary, "duplicate-key.json");
    writeFileSync(
      hostileArtifact,
      Buffer.concat([
        Buffer.from(
          `{"schemaVersion":${JSON.stringify(canonicalPayload.schemaVersion)},`,
        ),
        canonicalArtifact.subarray(1),
      ]),
    );
    assert.throws(
      () => execFileSync("python3", [
        benchmarkChecker,
        hostileArtifact,
        ...admissionPins,
      ], {
        cwd: root,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      }),
      (error) => {
        assert.notEqual(error.status, 0, "duplicate-key admission must fail");
        assert.match(error.stderr, /duplicate JSON key: schemaVersion/u);
        return true;
      },
      "the admission CLI must route artifact bytes through the strict loader",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }

  const ci = read(".github", "workflows", "ci.yml");
  const unmergedDraftAdmission =
    /v3_snapshot=|b777b1d95dd7693220621600dd49042a2046dab5|5781d4ab84b39a585d437e8e04604b25ef891cf1|5e5fdb34586452f3171b20113ab6f6a9412bcd82|ff2ed3c522192fe7c1e1492d59a466dd78c90ba2d5a243474cd4073f93362f53|e701d2e5ea8db96e446f6ac428b44374cd219caf09711bcac109639fbb405efd|d7f0f1c3ef0810eb5e3a8aecfcb0b67be7603ee9a6b23f8401c2284c5532bace|feasibility-benchmark-v4|admission-raw-v4/u;
  for (const [path, source] of [
    [".github/workflows/ci.yml", ci],
    ["CHANGELOG.md", read("CHANGELOG.md")],
    ["docs/verification-map.md", read("docs", "verification-map.md")],
    [
      "native harness",
      read("crates", "labcolors-core", "benches", "wcag22_feasibility_admission.rs"),
    ],
    ["native checker", read("scripts", "check_wcag22_feasibility_benchmark.py")],
    ["native applicability", read("scripts", "check_wcag22_feasibility_applicability.py")],
    [
      "WASM boundary checker",
      read("packages", "colors", "bench", "wcag22-feasibility-boundary.bench.mjs"),
    ],
  ]) {
    assert.doesNotMatch(
      source,
      unmergedDraftAdmission,
      `${path} must not retain an unmerged draft admission`,
    );
  }
  assert.match(
    ci,
    /historical_checker_snapshot=6001cf41e0a8364f25543e7955ceaf64d50129b4[\s\S]*?git worktree add --detach "\$historical_root" "\$historical_checker_snapshot"[\s\S]*?\(\n\s+cd "\$historical_root"\n\s+python3 scripts\/check_wcag22_feasibility_benchmark\.py[\s\S]*?--verify-current-subjects[\s\S]*?--artifact-sha256 7e9ffcbdd9d5d50fe681f511c34fc5c5dd270e9c475ce23ae56e9776922a3c5e[\s\S]*?--self-test/u,
    "the unchanged checker must replay in its exact clean Slice-A snapshot",
  );
  assert.match(
    ci,
    /historical_applicability_snapshot=94efeeeb1811f5515558ab2d79014a5e4c3a570a[\s\S]*?git worktree add --detach "\$historical_root" "\$historical_applicability_snapshot"[\s\S]*?python3 scripts\/check_wcag22_feasibility_applicability\.py[\s\S]*?--artifact-sha256 7e9ffcbdd9d5d50fe681f511c34fc5c5dd270e9c475ce23ae56e9776922a3c5e[\s\S]*?--self-test/u,
    "the V1 applicability law must replay where its verifier first existed",
  );
  assert.match(
    ci,
    /v2_artifact="crates\/labcolors-core\/contracts\/wcag22-feasibility-benchmark-v2\.json"[\s\S]*?v2_snapshot=4afe61b124b05e13b999f83a9de580ef43405080[\s\S]*?git worktree add --detach "\$historical_root" "\$v2_snapshot"[\s\S]*?python3 scripts\/check_wcag22_feasibility_benchmark\.py[\s\S]*?--artifact-sha256 d8d5c7f3eda834bca9912d835fe3ada13d9dcd5a11cb47a131736716b0b51202[\s\S]*?python3 scripts\/check_wcag22_feasibility_applicability\.py[\s\S]*?--artifact-sha256 d8d5c7f3eda834bca9912d835fe3ada13d9dcd5a11cb47a131736716b0b51202[\s\S]*?--self-test/u,
    "V2 must replay through its exact stable verifier snapshot",
  );
  assert.match(
    ci,
    /trap - EXIT[\s\S]*?current_artifact="crates\/labcolors-core\/contracts\/wcag22-feasibility-benchmark-v3\.json"[\s\S]*?--admit-revision cffc7758ba7c3919378b3ebe5fcd60fb43adc085[\s\S]*?python3 scripts\/check_wcag22_feasibility_benchmark\.py[\s\S]*?--verify-current-subjects[\s\S]*?--artifact-sha256 28c4af13a83a04f4668c61fe3399a8e1e91355cd71f02e27af47fce150fc001a[\s\S]*?python3 scripts\/check_wcag22_feasibility_applicability\.py[\s\S]*?--artifact-sha256 28c4af13a83a04f4668c61fe3399a8e1e91355cd71f02e27af47fce150fc001a[\s\S]*?--self-test/u,
    "V3 must bind the current generic kernel without an intermediate worktree",
  );
  assert.equal(
    ci.match(/python3 scripts\/check_wcag22_feasibility_benchmark\.py/gu)?.length,
    3,
    "CI must validate exactly two historical and one current benchmark artifact",
  );
  assert.equal(
    ci.match(/git worktree add --detach/gu)?.length,
    3,
    "only the three main-reachable historical verifier snapshots may use worktrees",
  );
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
  assert.match(conformanceReadme, /manifest\.packVersion`, сейчас `5\.0\.0`/u);
  assert.match(conformanceReadme, /4\.0\.0 → 5\.0\.0/u);
  assert.match(conformanceReadme, /3\.0\.0 → 4\.0\.0/u);
  assert.match(conformanceReadme, /`wcag22\.json`/u);
  assert.match(conformanceReadme, /`wcag22-feasibility\.json`/u);
  assert.match(
    conformanceReadme,
    /contrasts, ladders, alpha, solve, muddiness, wcag22,\s*wcag22-feasibility/u,
  );
  assert.doesNotMatch(conformanceReadme, /сейчас `[34]\.0\.0`/u);
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
  assert.match(prepare, /wasm: \{ bytes: wasm\.length, sha256: sha256\(wasm\) \}/);

  const verifier = read("scripts", "verify-package-release.mjs");
  assert.match(verifier, /import \{ workspaceVersion \} from "\.\/cargo-workspace\.mjs";/);
  assert.match(verifier, /function validateBuildMetadata/);
  assert.match(verifier, /isDeepStrictEqual\(metadata, expected\)/);
  assert.match(verifier, /require\.resolve\("@labpics\/colors\/build-metadata\.json"\)/);
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
  assert.match(verifier, /artifacts: \{ tarball, wasm, buildMetadata \}/);
});

test("package root curates public types while keeping feasibility internals private", () => {
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
  const feasibilityInternals = new Set([
    "Bytes32V1",
    "DecimalU64V1",
    "Srgb8BytesV1",
    "Wcag22FeasibilityApplicableRelationV1",
    "Wcag22FeasibilityAtomicErrorV1",
    "Wcag22FeasibilityCompilerInvariantV1",
    "Wcag22FeasibilityCoreErrorV1",
    "Wcag22FeasibilityEvaluatedV1",
    "Wcag22FeasibilityEvaluatorInvariantV1",
    "Wcag22FeasibilityInvalidRequestV1",
    "Wcag22FeasibilityNotApplicableRelationV1",
    "Wcag22FeasibilityNotEvaluatedResultV1",
    "Wcag22FeasibilityProofV1",
    "Wcag22FeasibilityProtocolErrorV1",
    "Wcag22FeasibilityRelationV1",
    "Wcag22FeasibilityResourceDimensionV1",
    "Wcag22FeasibilityTransportErrorV1",
    "Wcag22FeasibilityV1",
  ]);
  for (const name of feasibilityInternals) {
    assert.ok(generatedNames.includes(name), `generated declarations omit ${name}`);
    assert.ok(!exportedNames.includes(name), `package root leaks internal ${name}`);
  }
  for (const publicName of [
    "Wcag22FeasibilityRequestV1",
    "Wcag22FeasibilityOutcomeV1",
  ]) {
    assert.ok(exportedNames.includes(publicName), `package root omits ${publicName}`);
  }
  assert.deepEqual(
    [...exportedNames].sort(),
    generatedNames.filter((name) => !feasibilityInternals.has(name)).sort(),
    "root types must equal the reviewed public subset exactly",
  );
  assert.doesNotMatch(rootTypes, /InitOutput|__wbg_/u, "raw wasm ABI must stay private");
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

test("conformance docs define every feasibility count as an oracle output", () => {
  const readme = read("conformance", "README.md");
  for (const range of [
    "#000000…#040404",
    "#FEFEFE…#FFFFFF",
    "#757575…#767676",
    "#000000…#2D2D2D",
    "#D2D2D2…#FFFFFF",
    "#5A5A5A…#949494",
  ]) {
    assert.ok(readme.includes(range), `feasibility count docs omit exact range ${range}`);
  }
  assert.match(readme, /256/u);
  assert.match(readme, /scripts\/verify_wcag22_neutral_axis\.py/u);
  assert.match(readme, /wcag22-neutral-axis-oracle-v1\.json/u);
  assert.match(readme, /не параметры/u);
});
