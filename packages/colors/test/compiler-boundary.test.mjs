import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");

test("npm exposes one compiler subpath without a package-root alias", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));

  assert.deepEqual(packageJson.exports["./compiler"], {
    types: "./compiler.d.ts",
    default: "./compiler.js",
  });
  assert.equal(
    packageJson.exports["./compiler/wasm"],
    "./compiler/labcolors_compiler_bg.wasm",
  );
  for (const artifact of [
    "compiler.js",
    "compiler.d.ts",
    "compiler/labcolors_compiler.js",
    "compiler/labcolors_compiler.d.ts",
    "compiler/labcolors_compiler_bg.wasm",
    "compiler/labcolors_compiler_bg.wasm.d.ts",
  ]) {
    assert.ok(packageJson.files.includes(artifact), `npm files omits ${artifact}`);
  }
});

test("package root cannot resolve or name the offline compiler surface", () => {
  const rootJavaScript = read("packages", "colors", "index.js");
  const rootDeclarations = read("packages", "colors", "index.d.ts");

  for (const source of [rootJavaScript, rootDeclarations]) {
    assert.doesNotMatch(source, /Feasibility|feasibility|labcolors_compiler/u);
  }
  assert.doesNotMatch(rootJavaScript, /\.\/compiler(?:\.js|\/)/u);
  assert.doesNotMatch(rootDeclarations, /\.\/compiler(?:\.js|\/)/u);
});

test("runtime and compiler have disjoint normal dependency graphs", () => {
  const compilerManifestPath = join(
    root,
    "crates",
    "labcolors-compiler-wasm",
    "Cargo.toml",
  );
  assert.ok(existsSync(compilerManifestPath), "thin compiler WASM crate is missing");

  const runtimeManifest = read("crates", "labcolors-wasm", "Cargo.toml");
  const compilerManifest = read("crates", "labcolors-compiler-wasm", "Cargo.toml");
  assert.doesNotMatch(runtimeManifest, /labcolors-protocol/u);
  assert.match(
    compilerManifest,
    /labcolors-protocol = \{ path = "\.\.\/labcolors-protocol" \}/u,
  );
  assert.doesNotMatch(compilerManifest, /labcolors-wasm|labcolors-core/u);
});

test("the package build invokes wasm-pack once per physical role", () => {
  const packageJson = JSON.parse(read("packages", "colors", "package.json"));
  const build = packageJson.scripts.build;
  const invocations = build.match(/wasm-pack build/gu) ?? [];

  assert.equal(invocations.length, 2);
  assert.match(build, /crates\/labcolors-wasm[\s\S]*--out-dir \.\.\/\.\.\/packages\/colors\/pkg/u);
  assert.match(
    build,
    /crates\/labcolors-compiler-wasm[\s\S]*--out-dir \.\.\/\.\.\/packages\/colors\/compiler/u,
  );
});
