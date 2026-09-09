import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
const readRoot = () => readFileSync(new URL("../../../README.md", import.meta.url), "utf8");

test("README describes the terminal Program runtime, not recipe roles", () => {
  const readme = read("README.md");
  for (const required of [
    "compileProgramWire", "ProgramRuntime", "ProgramSnapshot", "атомарно", "typed-отказы",
  ]) assert.match(readme, new RegExp(required, "u"), required);
  for (const retired of ["RoleRecipe", "ThemeConfig", "resolveTheme", "applyTheme"]) {
    assert.doesNotMatch(readme, new RegExp(`\\b${retired}\\b`, "u"), retired);
  }
});

test("root README delegates executable API guidance to the package contract", () => {
  const root = readRoot();
  assert.match(root, /\[packages\/colors\/README\.md\]\(packages\/colors\/README\.md\)/u);
  for (const duplicateRuntimeSurface of [
    "compileProgramWire", "ProgramWireBuilderV1", "updateObserved", "runtime.update",
  ]) {
    assert.doesNotMatch(root, new RegExp(duplicateRuntimeSurface.replace(".", "\\."), "u"));
  }
  assert.doesNotMatch(root, /All workflows green/iu);
});

test("README first route emits the session-verified output through the public entrypoints", () => {
  const example = read("README.md").match(/```ts\r?\n([\s\S]*?)\r?\n```/u)?.[1];
  assert.equal(typeof example, "string");
  const result = execFileSync(process.execPath, ["--input-type=module"], {
    cwd: new URL("../", import.meta.url),
    encoding: "utf8",
    timeout: 30_000,
    input: `
      import { readFileSync } from "node:fs";
      import { init as initialize } from "@labpics/colors";
      await initialize({ module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });
      const outputs = [];
      console.log = (slot, rgb, opacity) => outputs.push({ slot, rgb: [...rgb], opacity });
      ${example}
      process.stdout.write(JSON.stringify(outputs));
    `,
  });
  assert.deepEqual(JSON.parse(result), [{ slot: 91, rgb: [20, 20, 20], opacity: 1 }]);
});

test("package version marks the terminal major contract", () => {
  const pkg = JSON.parse(read("package.json"));
  assert.equal(pkg.version, "1.0.0");
  assert.match(pkg.description, /Program wire/u);
});
