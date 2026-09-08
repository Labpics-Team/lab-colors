import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  copyFileSync,
  cpSync,
  existsSync,
  linkSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import * as fsPromises from "node:fs/promises";
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

test("package-smoke reaches tarball installation without dev dependencies while snippet parsing fails closed", () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-smoke-no-devdeps-"));
  try {
    const scripts = join(fixture, "scripts");
    mkdirSync(scripts);
    const scriptFiles = [
      ...PREPACK_FIXTURE_SCRIPT_FILES,
      "verify-package-release.mjs",
      "point-support-release-contract.cjs",
    ];
    assertPrepackFixtureScriptClosure(scriptFiles);
    for (const file of scriptFiles) {
      copyFileSync(join(root, "scripts", file), join(scripts, file));
    }
    assert.equal(existsSync(join(fixture, "node_modules")), false);
    assert.equal(existsSync(join(fixture, "packages", "colors", "node_modules")), false);
    const options = {
      cwd: fixture,
      encoding: "utf8",
      timeout: 30_000,
      env: { ...process.env, NODE_PATH: "", NODE_OPTIONS: "" },
    };
    // Отдельный процесс исключает кеш импортов и глобальные пути зависимостей.
    const witness = spawnSync(process.execPath, ["--no-global-search-paths", "--input-type=module", "-e", `
      import assert from "node:assert/strict";
      import { createRequire } from "node:module";
      const require = createRequire(new URL("./packages/colors/package.json", import.meta.url));
      assert.throws(() => require.resolve("es-module-lexer"), { code: "MODULE_NOT_FOUND" });
      const { runtimeSnippetPaths } = await import("./scripts/package-runtime-snippets.mjs");
      await assert.rejects(runtimeSnippetPaths("export const local = true;"), {
        code: "MODULE_NOT_FOUND",
        message: /es-module-lexer/u,
      });
    `], options);
    assert.equal(witness.error, undefined);
    assert.equal(witness.status, 0, witness.stderr);

    const smoke = spawnSync(process.execPath, [
      "--no-global-search-paths",
      join(scripts, "verify-package-release.mjs"),
      "--package-smoke",
      join(fixture, "missing-package.tgz"),
    ], options);
    assert.equal(smoke.error, undefined);
    assert.equal(smoke.status, 1);
    assert.doesNotMatch(smoke.stderr, /es-module-lexer/u);
    // Несуществующий tarball доказывает вход в реальный smoke, не только import.
    assert.match(smoke.stderr, /install --offline --ignore-scripts/u);
    assert.match(smoke.stderr, /ENOENT/u);
    assert.match(smoke.stderr, /missing-package\.tgz/u);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure is exact and rejects arbitrary paths", async () => {
  const { retainImportedRuntimeSnippets, runtimeSnippetPaths } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  assert.deepEqual(
    await runtimeSnippetPaths('import { x } from "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";'),
    ["snippets/labcolors-wasm-0123456789abcdef/inline0.js"],
  );
  assert.deepEqual(await runtimeSnippetPaths("export const local = true;"), []);
  assert.deepEqual(await runtimeSnippetPaths("export const url = import.meta.url;"), []);
  for (const source of [
    'import "./snippets/other/inline0.js";',
    'import "./snippets/labcolors-wasm-0123456789abcdef/payload.js";',
    'import("./snippets/labcolors-wasm-0123456789abcdef/inline0.js");',
    'import "bare-package";',
    'import "/absolute.js";',
    'import "C:/absolute.js";',
    'import "https://example.invalid/module.js";',
    'export { value } from "bare-package";',
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
    const snippet = join(generated, "inline0.js");
    for (const source of [
      'import "bare-package";\n',
      'import "/absolute.js";\n',
      'import "https://example.invalid/module.js";\n',
      'export { value } from "./nested.js";\n',
      'await import("./nested.js");\n',
    ]) {
      writeFileSync(snippet, source);
      await assert.rejects(
        retainImportedRuntimeSnippets(
          fixture,
          'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
        ),
        /dynamic or non-literal import|must not import or re-export another module/u,
      );
    }
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

test("generated runtime snippet closure rejects an orphan when the runtime imports none without deleting it", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-orphan-"));
  try {
    const snippets = join(fixture, "pkg", "snippets");
    const orphan = join(snippets, "labcolors-wasm-0123456789abcdef", "inline0.js");
    mkdirSync(dirname(orphan), { recursive: true });
    writeFileSync(orphan, "export const orphan = true;\n");

    await assert.rejects(
      retainImportedRuntimeSnippets(fixture, "export const local = true;"),
      /generated snippet inventory does not exactly match runtime imports/u,
    );
    assert.equal(readFileSync(orphan, "utf8"), "export const orphan = true;\n");
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure rejects a valid-shaped extra without deleting imported or orphan files", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-retain-"));
  try {
    const snippets = join(fixture, "pkg", "snippets");
    const imported = join(snippets, "labcolors-wasm-0123456789abcdef", "inline0.js");
    const orphan = join(snippets, "labcolors-wasm-fedcba9876543210", "inline0.js");
    mkdirSync(dirname(imported), { recursive: true });
    mkdirSync(dirname(orphan), { recursive: true });
    writeFileSync(imported, "export const retained = true;\n");
    writeFileSync(orphan, "export const orphan = true;\n");

    await assert.rejects(
      retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      /generated snippet inventory does not exactly match runtime imports/u,
    );
    assert.equal(readFileSync(imported, "utf8"), "export const retained = true;\n");
    assert.equal(readFileSync(orphan, "utf8"), "export const orphan = true;\n");
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure rejects directory junctions or records privilege skip", async (context) => {
  if (process.platform !== "win32") {
    context.skip("Windows junction/reparse behavior is platform-specific");
    return;
  }
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-junction-"));
  try {
    const snippets = join(fixture, "pkg", "snippets");
    const target = join(fixture, "junction-target");
    mkdirSync(snippets, { recursive: true });
    mkdirSync(target);
    writeFileSync(join(target, "inline0.js"), "export const linked = true;\n");
    const junction = join(snippets, "labcolors-wasm-0123456789abcdef");
    try {
      symlinkSync(target, junction, "junction");
    } catch (error) {
      if (error?.code === "EPERM") {
        context.skip("Windows privilege prevents creating a junction fixture");
        return;
      }
      throw error;
    }
    await assert.rejects(
      retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      /unexpected generated snippet entry|not a canonical directory/u,
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure leaves an external victim untouched after a junction swap", async (context) => {
  if (process.platform !== "win32") {
    context.skip("Windows junction/reparse behavior is platform-specific");
    return;
  }
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-junction-swap-"));
  try {
    const snippets = join(fixture, "pkg", "snippets");
    const generated = join(snippets, "labcolors-wasm-0123456789abcdef");
    const displaced = join(fixture, "displaced");
    const external = join(fixture, "external");
    const victim = join(external, "inline0.js");
    mkdirSync(generated, { recursive: true });
    mkdirSync(external);
    writeFileSync(join(generated, "inline0.js"), "export const orphan = true;\n");
    writeFileSync(victim, "export const victim = true;\n");
    let swapped = false;
    const io = {
      lstat: fsPromises.lstat,
      open: fsPromises.open,
      opendir: async (path) => {
        if (!swapped && path === generated) {
          swapped = true;
          renameSync(generated, displaced);
          symlinkSync(external, generated, "junction");
        }
        return fsPromises.opendir(path);
      },
      realpath: fsPromises.realpath,
    };

    await assert.rejects(
      retainImportedRuntimeSnippets(fixture, "export const local = true;", io),
      /not a canonical directory|canonical path differs|exactly match runtime imports/u,
    );
    assert.equal(swapped, true, "anti-vacuum: the junction swap was not injected");
    assert.equal(readFileSync(victim, "utf8"), "export const victim = true;\n");
    assert.equal(readFileSync(join(displaced, "inline0.js"), "utf8"), "export const orphan = true;\n");
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure tolerates a supported import.meta expression", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-import-meta-"));
  try {
    const generated = join(fixture, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    writeFileSync(join(generated, "inline0.js"), "export const url = import.meta.url;\n");
    assert.deepEqual(
      await retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      ["pkg/snippets/labcolors-wasm-0123456789abcdef/inline0.js"],
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure distinguishes identity changes before and during read", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const runtime = 'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";';

  for (const phase of ["before", "during"]) {
    const fixture = realpathSync(mkdtempSync(join(tmpdir(), `labcolors-runtime-identity-${phase}-`)));
    try {
      const generated = join(fixture, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
      mkdirSync(generated, { recursive: true });
      const snippet = join(generated, "inline0.js");
      const replacement = join(fixture, "replacement.js");
      writeFileSync(snippet, "export const original = true;\n");
      writeFileSync(replacement, "export const replacement = true;\n");
      let swapped = false;
      const io = {
        lstat: fsPromises.lstat,
        open: async (path, flags) => {
          const handle = await fsPromises.open(path, flags);
          const swap = () => {
            if (swapped || path !== snippet) return;
            swapped = true;
            rmSync(snippet);
            copyFileSync(replacement, snippet);
          };
          if (phase === "before") swap();
          return phase === "during"
            ? {
                close: handle.close.bind(handle),
                read: async (...args) => {
                  const result = await handle.read(...args);
                  swap();
                  return result;
                },
                stat: handle.stat.bind(handle),
              }
            : handle;
        },
        opendir: fsPromises.opendir,
        realpath: fsPromises.realpath,
      };
      await assert.rejects(
        retainImportedRuntimeSnippets(fixture, runtime, io),
        new RegExp(`identity changed ${phase} read`, "u"),
      );
      assert.equal(swapped, true, `anti-vacuum: the ${phase}-read identity swap was not injected`);
    } finally {
      rmSync(fixture, { recursive: true, force: true });
    }
  }
});

test("generated runtime snippet identity compares native nanosecond timestamps", async () => {
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = realpathSync(mkdtempSync(join(tmpdir(), "labcolors-runtime-nanoseconds-")));
  try {
    const generated = join(fixture, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    const snippet = join(generated, "inline0.js");
    writeFileSync(snippet, "export const original = true;\n");
    let snippetStats = 0;
    let injected = false;
    const io = {
      lstat: async (path, options) => {
        const metadata = await fsPromises.lstat(path, options);
        if (path !== snippet || ++snippetStats === 1) return metadata;
        injected = true;
        return new Proxy(metadata, {
          get(target, property) {
            if (property === "ctimeNs") return target.ctimeNs + 1n;
            const value = Reflect.get(target, property);
            return typeof value === "function" ? value.bind(target) : value;
          },
        });
      },
      open: fsPromises.open,
      opendir: fsPromises.opendir,
      realpath: fsPromises.realpath,
    };
    await assert.rejects(
      retainImportedRuntimeSnippets(
        fixture,
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
        io,
      ),
      /identity changed during read/u,
    );
    assert.equal(injected, true, "anti-vacuum: the nanosecond-only change was not injected");
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure accepts symlinked package parents", async (context) => {
  if (process.platform === "win32") {
    context.skip("directory symlink setup is privilege-dependent on Windows");
    return;
  }
  const { retainImportedRuntimeSnippets } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-parent-link-"));
  try {
    const canonicalParent = join(fixture, "canonical");
    const linkedParent = join(fixture, "linked");
    const packageDirectory = join(canonicalParent, "package");
    const generated = join(packageDirectory, "pkg", "snippets", "labcolors-wasm-0123456789abcdef");
    mkdirSync(generated, { recursive: true });
    writeFileSync(join(generated, "inline0.js"), "export const original = true;\n");
    symlinkSync(canonicalParent, linkedParent, "dir");

    assert.deepEqual(
      await retainImportedRuntimeSnippets(
        join(linkedParent, "package"),
        'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
      ),
      ["pkg/snippets/labcolors-wasm-0123456789abcdef/inline0.js"],
    );
    for (const [directory, expected] of [
      [packageDirectory, /package directory is not canonical/u],
      [join(packageDirectory, "pkg"), /pkg is not a canonical directory/u],
      [join(packageDirectory, "pkg", "snippets"), /snippets root is not a canonical directory/u],
      [generated, /unexpected generated snippet entry/u],
    ]) {
      const displaced = join(fixture, "displaced");
      renameSync(directory, displaced);
      try {
        symlinkSync(displaced, directory, "dir");
        await assert.rejects(
          retainImportedRuntimeSnippets(
            join(linkedParent, "package"),
            'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
          ),
          expected,
        );
      } finally {
        rmSync(directory);
        renameSync(displaced, directory);
      }
    }
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("generated runtime snippet closure enforces entry-count and total-byte caps", async () => {
  const {
    MAX_SNIPPET_DIRECTORIES,
    MAX_SNIPPET_FILES,
    MAX_TOTAL_SNIPPET_BYTES,
    retainImportedRuntimeSnippets,
  } = await import(pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs")));
  const runtime = 'import "./snippets/labcolors-wasm-0000000000000000/inline0.js";';

  const entriesFixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-entries-"));
  try {
    for (let index = 0; index <= MAX_SNIPPET_DIRECTORIES; index += 1) {
      const directory = join(
        entriesFixture,
        "pkg",
        "snippets",
        `labcolors-wasm-${index.toString(16).padStart(16, "0")}`,
      );
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, "inline0.js"), "export const value = true;\n");
    }
    await assert.rejects(
      retainImportedRuntimeSnippets(entriesFixture, runtime),
      /directory count exceeds/u,
    );
  } finally {
    rmSync(entriesFixture, { recursive: true, force: true });
  }

  const filesFixture = realpathSync(mkdtempSync(join(tmpdir(), "labcolors-runtime-files-")));
  try {
    const directory = join(filesFixture, "pkg", "snippets", "labcolors-wasm-0000000000000000");
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, "inline0.js"), "export const value = true;\n");
    const realOpendir = fsPromises.opendir;
    const io = {
      lstat: fsPromises.lstat,
      open: fsPromises.open,
      opendir: async (path) => {
        if (path === directory) {
          async function* entries() {
            for (let index = 0; index <= MAX_SNIPPET_FILES; index += 1) {
              yield {
                isFile: () => true,
                name: "inline0.js",
              };
            }
          }
          return entries();
        }
        return realOpendir(path);
      },
      realpath: fsPromises.realpath,
    };
    await assert.rejects(
      retainImportedRuntimeSnippets(filesFixture, runtime, io),
      /file count exceeds/u,
    );
  } finally {
    rmSync(filesFixture, { recursive: true, force: true });
  }

  const bytesFixture = mkdtempSync(join(tmpdir(), "labcolors-runtime-total-bytes-"));
  try {
    const bytesPerFile = Math.floor(MAX_TOTAL_SNIPPET_BYTES / 3) + 1;
    for (let index = 0; index < 3; index += 1) {
      const directory = join(
        bytesFixture,
        "pkg",
        "snippets",
        `labcolors-wasm-${index.toString(16).padStart(16, "0")}`,
      );
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, "inline0.js"), "x".repeat(bytesPerFile));
    }
    await assert.rejects(
      retainImportedRuntimeSnippets(bytesFixture, runtime),
      /snippet bytes exceed/u,
    );
  } finally {
    rmSync(bytesFixture, { recursive: true, force: true });
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
    linkSync(target, snippet);
    await assert.rejects(retainImportedRuntimeSnippets(fixture, runtime), /identity changed before read/u);
    assert.equal(readFileSync(target, "utf8"), "export const linked = true;\n");
    rmSync(snippet);
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

test("npm 11.9.0 packs canonical hex while the release guard rejects hostile snippet paths", async () => {
  const { retainImportedRuntimeSnippets, runtimeSnippetPaths } = await import(
    pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))
  );
  const packageJson = JSON.parse(readFileSync(join(root, "packages", "colors", "package.json"), "utf8"));
  const selector = packageJson.files.find((path) => path.startsWith("pkg/snippets/"));
  assert.equal(typeof selector, "string");

  const fixture = mkdtempSync(join(tmpdir(), "labcolors-npm-selector-"));
  try {
    writeFileSync(
      join(fixture, "package.json"),
      `${JSON.stringify({ name: "selector-fixture", version: "1.0.0", files: [selector] })}\n`,
    );
    const generatedPackage = join(fixture, "pkg");
    mkdirSync(generatedPackage, { recursive: true });
    writeFileSync(join(generatedPackage, ".gitignore"), "*\n");
    writeFileSync(join(generatedPackage, ".npmignore"), "");
    for (const directory of [
      "labcolors-wasm-0123456789abcdef",
      "labcolors-wasm-0123456789abcdeF",
      "labcolors-wasm-0123456789abcdeg",
    ]) {
      const snippet = join(fixture, "pkg", "snippets", directory);
      mkdirSync(snippet, { recursive: true });
      writeFileSync(join(snippet, "inline0.js"), `export const fixture = ${JSON.stringify(directory)};\n`);
    }

    const npx = process.platform === "win32"
      ? { command: process.env.ComSpec ?? "cmd.exe", prefix: ["/d", "/s", "/c", "npx.cmd"] }
      : { command: "npx", prefix: [] };
    assert.equal(
      command(npx.command, [...npx.prefix, "--yes", "npm@11.9.0", "--version"], fixture),
      "11.9.0",
    );
    const pack = JSON.parse(
      command(
        npx.command,
        [...npx.prefix, "--yes", "npm@11.9.0", "pack", "--dry-run", "--json"],
        fixture,
      ),
    );
    const packedPaths = pack[0].files.map(({ path }) => path);
    assert.equal(
      packedPaths.includes("pkg/snippets/labcolors-wasm-0123456789abcdef/inline0.js"),
      true,
    );
    assert.equal(
      packedPaths.includes("pkg/snippets/labcolors-wasm-0123456789abcdeg/inline0.js"),
      false,
    );
    // npm glob не гарантирует регистр; импорт и содержимое каталога проверяет release guard.
    const canonicalImport = 'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";';
    for (const directory of ["labcolors-wasm-0123456789abcdeF", "labcolors-wasm-0123456789abcdeg"]) {
      await assert.rejects(
        runtimeSnippetPaths(`import "./snippets/${directory}/inline0.js";`),
        /unsupported relative module specifier/u,
      );
      rmSync(join(generatedPackage, "snippets"), { recursive: true });
      const snippet = join(generatedPackage, "snippets", directory);
      mkdirSync(snippet, { recursive: true });
      writeFileSync(join(snippet, "inline0.js"), "export const fixture = true;\n");
      await assert.rejects(
        retainImportedRuntimeSnippets(fixture, canonicalImport),
        /unexpected generated snippet entry/u,
      );
    }
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("prepack includes the generated snippet despite wasm-pack gitignore without widening the archive", async () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-prepack-ignore-"));
  const previousSha = process.env.GITHUB_SHA;
  try {
    const scripts = copyPrepackFixture(fixture);
    const packageDirectory = join(fixture, "packages", "colors");
    const generated = join(packageDirectory, "pkg");
    const snippet = "pkg/snippets/labcolors-wasm-0123456789abcdef/inline0.js";
    const packageJson = JSON.parse(readFileSync(join(root, "packages", "colors", "package.json"), "utf8"));
    packageJson.scripts = {};
    writeFileSync(join(packageDirectory, "package.json"), JSON.stringify(packageJson));
    for (const name of ["LICENSE", "Cargo.toml"]) {
      copyFileSync(join(root, name), join(fixture, name));
    }
    const contracts = join(fixture, "crates", "labcolors-core", "contracts");
    mkdirSync(contracts, { recursive: true });
    for (const file of packageJson.files.filter((file) => file.startsWith("evidence/"))) {
      const name = file.slice("evidence/".length);
      copyFileSync(join(root, "crates", "labcolors-core", "contracts", name), join(contracts, name));
    }
    const conformance = join(fixture, "conformance", "vectors");
    mkdirSync(conformance, { recursive: true });
    for (const name of ["manifest.json", "contrasts.json", "alpha.json", "solve.json", "wcag22.json"]) {
      copyFileSync(join(root, "conformance", "vectors", name), join(conformance, name));
    }
    for (const file of packageJson.files.filter((file) => !file.includes("[0-9a-f]") && !file.startsWith("evidence/"))) {
      const destination = join(packageDirectory, file);
      mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, "fixture\n");
    }
    writeFileSync(join(packageDirectory, "README.md"), "fixture\n");
    mkdirSync(dirname(join(packageDirectory, snippet)), { recursive: true });
    writeFileSync(join(packageDirectory, snippet), "export const fixture = true;\n");
    writeFileSync(join(generated, "labcolors.js"), `import "./${snippet.slice("pkg/".length)}";\n`);
    writeFileSync(join(generated, "labcolors_bg.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    writeFileSync(join(generated, ".gitignore"), "*\n");
    writeFileSync(join(generated, "LICENSE"), "nested licence must not ship\n");
    writeFileSync(join(generated, "package.json"), "{}\n");
    writeFileSync(join(generated, "unexpected.js"), "must not ship\n");
    writeFileSync(join(fixture, ".gitignore"), "node_modules/\npackages/colors/pkg/\npackages/colors/evidence/\npackages/colors/LICENSE\npackages/colors/build-metadata.json\n");
    command("git", ["init", "--quiet"], fixture);
    command("git", ["add", "."], fixture);
    command("git", ["-c", "user.name=Lab Colors release test", "-c", "user.email=release-test@example.invalid", "commit", "--quiet", "-m", "fixture"], fixture);
    delete process.env.GITHUB_SHA;
    const { prepareNpmPackage } = await import(pathToFileURL(join(scripts, "prepare-npm-package.mjs")));
    await prepareNpmPackage();
    assert.equal(readFileSync(join(generated, ".gitignore"), "utf8"), "*\n");
    const npx = process.platform === "win32"
      ? { command: process.env.ComSpec ?? "cmd.exe", prefix: ["/d", "/s", "/c", "npx.cmd"] }
      : { command: "npx", prefix: [] };
    const [pack] = JSON.parse(command(npx.command, [...npx.prefix, "--yes", "npm@11.9.0", "pack", "--dry-run", "--json", "--ignore-scripts"], packageDirectory));
    const packedPaths = pack.files.map(({ path }) => path).sort();
    assert.deepEqual(packedPaths, [
      ...packageJson.files.filter((file) => !file.includes("[0-9a-f]")),
      snippet, "README.md", "package.json",
    ].sort());
  } finally {
    if (previousSha === undefined) delete process.env.GITHUB_SHA;
    else process.env.GITHUB_SHA = previousSha;
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("canonical package inventory requires one exact snippet and has 18 members", async () => {
  const { expectedPackedFiles } = await import(
    pathToFileURL(join(root, "scripts", "verify-package-release.mjs"))
  );
  const packageJson = JSON.parse(readFileSync(join(root, "packages", "colors", "package.json"), "utf8"));
  const selector = "pkg/snippets/labcolors-wasm-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]/inline0.js";
  assert.equal(packageJson.files.filter((path) => path === selector).length, 1);
  assert.equal(packageJson.files.filter((path) => path.includes("[0-9a-f]")).length, 1);
  assert.equal(packageJson.files.some((path) => path.includes("?") || path.includes("*")), false);

  const zeroSnippetRuntime = "export const diagnosticOnly = true;";
  assert.deepEqual(
    await (await import(pathToFileURL(join(root, "scripts", "package-runtime-snippets.mjs"))))
      .runtimeSnippetPaths(zeroSnippetRuntime),
    [],
  );
  await assert.rejects(
    expectedPackedFiles(packageJson, zeroSnippetRuntime),
    /requires exactly one generated runtime snippet, got 0/u,
  );
  const inventory = await expectedPackedFiles(
    packageJson,
    'import "./snippets/labcolors-wasm-0123456789abcdef/inline0.js";',
  );
  assert.equal(inventory.length, 18);
  assert.deepEqual(
    inventory.filter((path) => path.startsWith("pkg/snippets/")),
    ["pkg/snippets/labcolors-wasm-0123456789abcdef/inline0.js"],
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
