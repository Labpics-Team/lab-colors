import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { isProgramError } from "../index.js";

const index = readFileSync(new URL("../index.js", import.meta.url), "utf8");
const types = readFileSync(new URL("../index.d.ts", import.meta.url), "utf8");
const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

const retired = [
  "LabColors", "resolveTheme", "loadConfig", "RoleRecipe", "ThemeConfig",
  "ResolvedTheme", "applyTheme", "watchTheme", "adaptTheme", "solveGlowPoint",
];

test("terminal C7c root exports one Program runtime and no legacy facade", () => {
  for (const symbol of retired) {
    assert.doesNotMatch(index, new RegExp(`\\b${symbol}\\b`, "u"), symbol);
    assert.doesNotMatch(types, new RegExp(`\\b${symbol}\\b`, "u"), symbol);
  }
  for (const required of ["compileProgramWire", "ProgramRuntime", "ProgramSnapshot"]) {
    assert.match(index, new RegExp(`\\b${required}\\b`, "u"), required);
    assert.match(types, new RegExp(`\\b${required}\\b`, "u"), required);
  }
});

test("Program errors narrow through stable code and operation fields", () => {
  const error = Object.assign(new Error("diagnostic text is not parsed"), {
    code: "program_update",
    operation: "updateObserved",
  });
  assert.equal(isProgramError(error), true);
  assert.equal(isProgramError(Object.assign(new Error("x"), { code: "future", operation: "updateObserved" })), false);
  assert.equal(isProgramError(Object.assign(new Error("x"), { code: "program_update", operation: "compileProgramWire" })), false);
  assert.equal(isProgramError(Object.assign(new Error("x"), { code: "program_wire", operation: "updateUnknown" })), false);
  assert.equal(isProgramError({ code: "program_update", operation: "updateObserved" }), false);
});

test("package exports contain no legacy browser subpath", () => {
  assert.deepEqual(Object.keys(pkg.exports).sort(), [
    ".", "./build-metadata.json", "./package.json", "./pkg/labcolors_bg.wasm",
    "./program-wire/abi-v1.js",
  ]);
  for (const file of pkg.files) {
    assert.doesNotMatch(file, /apply-theme|watch-theme|adapt-theme|private-program/u);
  }
});
