import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
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

test("README Node.js first route initializes a fresh module and emits the session-verified output", () => {
  const readme = read("README.md");
  const exampleUnder = (heading) => {
    const section = readme.split(`### ${heading}\n`)[1]?.split(/\n#{1,3} /u)[0];
    const example = section?.match(/```ts\r?\n([\s\S]*?)\r?\n```/u)?.[1];
    assert.equal(typeof example, "string", `README example: ${heading}`);
    return example;
  };
  const initialization = exampleUnder("Node.js");
  const example = exampleUnder("Объявление графа и чтение результата");
  const result = execFileSync(process.execPath, ["--input-type=module"], {
    cwd: new URL("../", import.meta.url),
    encoding: "utf8",
    timeout: 30_000,
    input: `
      const outputs = [];
      console.log = (slot, rgb, opacity) => outputs.push({ slot, rgb: [...rgb], opacity });
      ${initialization}
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
