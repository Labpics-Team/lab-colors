import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");

test("README describes the terminal Program runtime, not recipe roles", () => {
  const readme = read("README.md");
  for (const required of [
    "compileProgramWire", "ProgramRuntime", "ProgramSnapshot", "атомарно", "typed-отказы",
  ]) assert.match(readme, new RegExp(required, "u"), required);
  for (const retired of ["RoleRecipe", "ThemeConfig", "resolveTheme", "applyTheme"]) {
    assert.doesNotMatch(readme, new RegExp(`\\b${retired}\\b`, "u"), retired);
  }
});

test("package version marks the terminal major contract", () => {
  const pkg = JSON.parse(read("package.json"));
  assert.equal(pkg.version, "1.0.0");
  assert.match(pkg.description, /Program wire/u);
});
