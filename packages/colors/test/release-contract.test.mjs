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
});

test("publish accepts only canonical exact-SHA workflow runs and their immutable CI artifact", () => {
  const publish = read(".github", "workflows", "publish.yml");
  assert.match(publish, /^\s*NODE_TOOLCHAIN: "24\.14\.0"$/m);
  assert.match(publish, /^\s*NPM_TOOLCHAIN: "11\.9\.0"$/m);
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
    /run: npm publish --ignore-scripts "\$\{\{ steps\.verified-artifact\.outputs\.tarball \}\}"/,
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

test("canonical-run guard executes against workflow-scoped runs and jobs", () => {
  const publish = read(".github", "workflows", "publish.yml");
  const selector = workflowNodeScript(
    publish,
    "name: guard — canonical exact-SHA workflow runs and their own jobs",
  );
  const requiredCiJobs = [
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
      schemaVersion: 1,
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
  assert.match(verifier, /GITHUB_OUTPUT/);
  assert.match(verifier, /familySetSha256: sha256\(Buffer\.concat\(familyBuffers\)\)/);
  assert.match(verifier, /sha256: sha256\(familyBuffers\[index\]\)/);
  assert.match(verifier, /numericalSites: conformance\.numericalSites/);
  assert.doesNotMatch(
    verifier,
    /numericalSites:\s*\[\s*\{/,
    "release manifest must copy the generated conformance registry, not duplicate it",
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
  assert.match(verifier, /function validateBuildMetadata/);
  assert.match(verifier, /isDeepStrictEqual\(metadata, expected\)/);
  assert.match(verifier, /require\.resolve\("@labpics\/colors\/build-metadata\.json"\)/);
  assert.match(verifier, /installedBuildMetadata/);
  assert.match(verifier, /isDeepStrictEqual\(installedBuildMetadata, expectedBuildMetadata\)/);
  assert.match(verifier, /"--offline"/);
  assert.match(verifier, /node_modules\/typescript\/package\.json/);
  assert.doesNotMatch(verifier, /`typescript@\$\{typescriptVersion\}`/);
  assert.match(verifier, /artifacts: \{ tarball, wasm, buildMetadata \}/);
});

test("package root curates every custom schema/result type without exporting wasm ABI", () => {
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

  const rootTypes = read("packages", "colors", "index.d.ts").match(
    /export type \{([\s\S]*?)\} from "\.\/pkg\/labcolors\.js";/u,
  )?.[1];
  assert.ok(rootTypes, "curated root type export block not found");
  const exportedNames = [...rootTypes.matchAll(/^\s{2}([A-Za-z][A-Za-z0-9_]*),$/gmu)].map(
    (match) => match[1],
  );
  assert.deepEqual(
    [...exportedNames].sort(),
    [...generatedNames].sort(),
    "root types must equal the custom public section exactly",
  );
  assert.doesNotMatch(rootTypes, /InitOutput|__wbg_/u, "raw wasm ABI must stay private");
});
