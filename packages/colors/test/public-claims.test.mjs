import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { evaluateWcag22 } from "../index.js";

const read = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const hex = (rgb) => "#" + rgb.map((channel) => channel.toString(16).padStart(2, "0")).join("");
const textPasses = (foreground, background) =>
  evaluateWcag22(hex(foreground), hex(background), "sc-1.4.3-text-default").decision === "pass";

function exampleUnder(heading) {
  const section = read("README.md")
    .split(/^#{2,3} /mu)
    .find((section) => section.split(/\r?\n/u, 1)[0] === heading);
  const example = section?.match(/```js\r?\n([\s\S]*?)\r?\n```/u)?.[1];
  assert.equal(typeof example, "string", `README example: ${heading}`);
  return example;
}

function runReadme(declaration) {
  const result = execFileSync(process.execPath, ["--input-type=module"], {
    cwd: new URL("../", import.meta.url),
    encoding: "utf8",
    timeout: 30_000,
    input: `
      const outputs = [];
      console.log = (...row) => outputs.push(row);
      ${exampleUnder("Node.js")}
      ${exampleUnder(declaration)}
      ${exampleUnder("Результат без DOM")}
      process.stdout.write(JSON.stringify(outputs, (_, value) => typeof value === "bigint" ? value.toString() : value));
    `,
  });
  return JSON.parse(result).map(([frame]) => frame);
}

test("README starts with local roles and retains the direct Program reference", () => {
  const readme = read("README.md");
  for (const required of ["colors", "program.attach", "view.set", "compileProgramWire", "ProgramRuntime", "ProgramSnapshot", "атомарно"]) {
    assert.ok(readme.includes(required), required);
  }
  for (const retired of ["RoleRecipe", "ThemeConfig", "resolveTheme", "applyTheme"]) {
    assert.doesNotMatch(readme, new RegExp(`\\b${retired}\\b`, "u"), retired);
  }
});

test("README first route runs in a fresh Node module and adapts one identity across local uses", () => {
  const frames = runReadme("Объявление ролей");
  assert.equal(frames.length, 2);
  for (const frame of frames) {
    assert.equal(frame.state, "ready");
    assert.equal(textPasses(frame.values.label.rgb, frame.values.fill.rgb), true);
    assert.equal(textPasses(frame.values.link.rgb, frame.context.canvas), true);
    assert.equal(textPasses(frame.values.text.rgb, frame.context.canvas), true);
  }
  assert.notDeepEqual(frames[0].values.link.rgb, frames[1].values.link.rgb);
  assert.deepEqual(frames.map((frame) => frame.context.canvas), [[230, 221, 208], [40, 53, 72]]);
});

test("README reusable surface policy runs two client topologies and preserves its fixed island", () => {
  const frames = runReadme("Одно правило для разных композиций");
  for (const frame of frames) {
    assert.equal(frame.state, "ready");
    assert.equal(textPasses(frame.values.panelText.rgb, frame.values.panel.rgb), true);
    assert.equal(textPasses(frame.values.islandText.rgb, frame.values.island.rgb), true);
    assert.equal(frame.values.notice.opacity, 0.72);
    assert.deepEqual(frame.values.island.rgb, [244, 234, 219]);
  }
  assert.notDeepEqual(frames[0].values.panel.rgb, frames[1].values.panel.rgb);
});

test("пакет 1.x предоставляет authoring и Program API", async () => {
  const pkg = JSON.parse(read("package.json"));
  assert.match(pkg.version, /^1\.\d+\.\d+$/u);
  const api = await import("@labpics/colors");
  for (const name of ["colors", "ColorAuthoringError", "compileProgramWire", "ProgramRuntime", "ProgramSnapshot"]) {
    assert.equal(typeof api[name], "function", name);
  }
  for (const name of ["RoleRecipe", "ThemeConfig", "resolveTheme", "applyTheme"]) {
    assert.equal(Object.hasOwn(api, name), false, name);
  }
});
