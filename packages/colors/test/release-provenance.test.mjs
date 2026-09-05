import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
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

const PREPACK_FIXTURE_SCRIPT_FILES = Object.freeze([
  "prepare-npm-package.mjs",
  "atomic-write.mjs",
  "cargo-workspace.mjs",
  "release-evidence.mjs",
  "package-runtime-snippets.mjs",
]);

test("generated runtime snippet closure is exact and rejects arbitrary paths", async () => {
  const { retainImportedRuntimeSnippets, runtimeSnippetPaths } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  assert.deepEqual(
    await runtimeSnippetPaths('import { x } from "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";'),
    ["snippets/labcolors-wasm-0123456789abcdef/inline0.js"],
  );
  assert.deepEqual(await runtimeSnippetPaths("export const local = true;"), []);
  for (const source of [
    'import "./snippets/other/inline0.js";',
    'import "./snippets/labcolors-wasm-0123456789abcdef/payload.js";',
    'import("./snippets/labcolors-wasm-0123456789abcdef/inline0.js");',
    'import "node:fs";',
    'import "C:/absolute.js";',
    'import "https://example.invalid/module.js";',
  ]) {
    await assert.rejects(
      runtimeSnippetPaths(source),
      /dynamic or non-literal import|unsupported relative module|non-relative module/u,
    );
  }
  for (const spoof of [
    '// import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";\nexport const local = true;',
    'const text = `import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";`;',
  ]) {
    assert.deepEqual(await runtimeSnippetPaths(spoof), []);
  }
  await assert.rejects(
    runtimeSnippetPaths([
      'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      'export { value } from "./snippets/labcolors-wasm-0123456789abcdef/inline1.js";',
    ].join("\n")),
    /more than one module/u,
  );

  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-snippets-"));
  try {
    const generated = join(fixture, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    writeFileSync(
      join(generated, "inline0.js"),
      'export { value } from "./nested.js";\n',
    );
    await assert.rejects(
      retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      /must not import or re-export another module/u,
    );
    writeFileSync(join(generated, "payload.js"), "export const arbitrary = true;\n");
    await assert.rejects(
      retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      /unexpected generated snippet entry/u,
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure prunes orphan output when the runtime imports none", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-orphan-"));
  try {
    const snippets = join(fixture, "pkg", "snippets");
    const generated = join(snippets, "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    writeFileSync(join(generated, "inline0.js"), "export const orphan = true;\n");
    assert.deepEqual(await retainImportedRuntimeSnippets(fixture, "export const local = true;"), []);
    assert.equal(existsSync(snippets), false);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure rejects oversized and linked artifacts", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const runtime = 'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";';
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-hostile-"));
  try {
    const generated = join(fixture, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    const snippet = join(generated, "inline0.js");
    writeFileSync(snippet, "x".repeat(16 * 1024 + 1));
    await assert.rejects(retainImportedRuntimeSnippets(fixture, runtime), /invalid byte size/u);

    rmSync(snippet);
    const target = join(fixture, "outside.js");
    writeFileSync(target, "export const linked = true;\n");
    try {
      symlinkSync(target, snippet, "file");
    } catch (error) {
      if (error?.code === "EPERM") return;
      throw error;
    }
    await assert.rejects(retainImportedRuntimeSnippets(fixture, runtime), /unexpected generated snippet entry|not a regular file/u);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("the prepack fixture closure check distinguishes incompleteness from the guard regression", () => {
  // A positive fixture test would otherwise fail with the same
  // ERR_MODULE_NOT_FOUND as the negative guard test, masking an incomplete
  // fixture list under a regression.
  assert.throws(
    () => assertPrepackFixtureScriptClosure(["prepare-npm-package.mjs"]),
    /prepack fixture is missing scripts\/atomic-write\.mjs/u,
  );
  assert.throws(
    () => assertPrepackFixtureScriptClosure(PREPACK_FIXTURE_SCRIPT_FILES.slice(0, 2)),
    /prepack fixture is missing scripts\/cargo-workspace\.mjs/u,
  );
  assert.doesNotThrow(() =>
    assertPrepackFixtureScriptClosure(PREPACK_FIXTURE_SCRIPT_FILES),
  );
});

function assertPrepackFixtureScriptClosure(scriptFiles) {
  for (const dependency of scriptFiles) {
    const source = readFileSync(join(root, "scripts", dependency), "utf8");
    for (const [, specifier] of source.matchAll(/from\s+"\.\/([\w.-]+\.mjs)"/gu)) {
      assert.ok(
        scriptFiles.includes(specifier),
        `prepack fixture is missing scripts/${specifier}`,
      );
    }
  }
}

function copyPrepackFixture(fixture, { includeAtomicWriter = true } = {}) {
  const scripts = join(fixture, "scripts");
  mkdirSync(scripts, { recursive: true });
  // Инвариант: фикстура содержит весь граф относительных импортов prepack.
  // Иначе положительный тест падал бы с ERR_MODULE_NOT_FOUND и маскировал
  // отсутствие фикстуры под регрессию source guard.
  assertPrepackFixtureScriptClosure(PREPACK_FIXTURE_SCRIPT_FILES);
  for (const dependency of PREPACK_FIXTURE_SCRIPT_FILES) {
    if (!includeAtomicWriter && dependency === "atomic-write.mjs") continue;
    copyFileSync(join(root, "scripts", dependency), join(scripts, dependency));
  }

  const bench = join(fixture, "packages", "colors", "bench");
  mkdirSync(bench, { recursive: true });
  copyFileSync(
    join(root, "packages", "colors", "bench", "wasm.json"),
    join(bench, "wasm.json"),
  );
  const fixtureModules = join(fixture, "packages", "colors", "node_modules");
  mkdirSync(fixtureModules, { recursive: true });
  cpSync(
    join(root, "packages", "colors", "node_modules", "es-module-lexer"),
    join(fixtureModules, "es-module-lexer"),
    { recursive: true },
  );
  return scripts;
}

test("prepack source graph fails closed without its atomic writer", async () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-prepack-missing-atomic-"));
  try {
    const scripts = copyPrepackFixture(fixture, { includeAtomicWriter: false });
    await assert.rejects(
      import(pathToFileURL(join(scripts, "prepare-npm-package.mjs"))),
      (error) => {
        assert.equal(error?.code, "ERR_MODULE_NOT_FOUND");
        assert.match(String(error?.message), /atomic-write\.mjs/u);
        return true;
      },
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("prepack source guard is clean-tree and exact-SHA executable evidence", async () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-prepack-source-"));
  const scripts = copyPrepackFixture(fixture);
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
    /\[workspace\.package\]\.version is absent/u,
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
    /\[workspace\.package\]\.version is absent/u,
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

test("clean-consumer smoke закрепляет terminal Program wire", () => {
  const verifier = readFileSync(
    join(root, "scripts", "verify-package-release.mjs"),
    "utf8",
  );
  for (const literal of [
    "compileProgramWire",
    "ProgramRuntime",
    "ProgramSnapshot",
    "canonical-program-wire-v1",
    "atomic-program-runtime-v1",
  ]) {
    assert.match(verifier, new RegExp(literal, "u"), `missing ${literal}`);
  }
  for (const retired of [
    "layerRecipeProfile",
    "appearanceDiagnosticProfile",
    "selectionDiagnosticProfile",
    "exact-noop-unreachable",
    "legacy-reached",
    "legacy-unreachable",
  ]) {
    assert.doesNotMatch(verifier, new RegExp(retired, "u"), `retired ${retired}`);
  }
});
