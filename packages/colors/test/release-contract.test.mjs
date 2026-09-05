import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => readFileSync(new URL(name, root), "utf8");
const pkg = JSON.parse(read("package.json"));

test("terminal tar inventory is exact and excludes retired roots", () => {
  const required = new Set([
    "LICENSE", "build-metadata.json", "index.js", "index.d.ts", "wcag22.d.ts",
    "program-wire/abi-v1.js", "program-wire/abi-v1.d.ts",
    "evidence/wcag22-srgb8-v1.json", "evidence/wcag22-srgb8-q55-v1.bin",
    "evidence/wcag22-srgb8-q55-proof-v1.json",
    "evidence/point-support-reference-surplus-q55-bps-proof-v1.json",
    "pkg/labcolors.js", "pkg/labcolors.d.ts", "pkg/labcolors_bg.wasm",
    "pkg/labcolors_bg.wasm.d.ts",
  ]);
  assert.deepEqual(new Set(pkg.files), required);
});

test("terminal package has one root and no legacy subpath", () => {
  assert.equal(pkg.version, "1.0.0");
  assert.deepEqual(Object.keys(pkg.exports).sort(), [
    ".", "./build-metadata.json", "./package.json", "./pkg/labcolors_bg.wasm",
    "./program-wire/abi-v1.js",
  ]);
  assert.deepEqual(pkg.exports["./program-wire/abi-v1.js"], {
    types: "./program-wire/abi-v1.d.ts",
    default: "./program-wire/abi-v1.js",
  });
});

test("root source exports Program runtime and permanent capabilities only", () => {
  const index = read("index.js");
  for (const required of ["compileProgramWire", "ProgramRuntime", "ProgramSnapshot", "evaluateWcag22", "numericalCapabilityManifest"]) {
    assert.match(index, new RegExp(`\\b${required}\\b`, "u"));
  }
  for (const retired of ["LabColors", "applyTheme", "watchTheme", "adaptTheme"]) {
    assert.doesNotMatch(index, new RegExp(`\\b${retired}\\b`, "u"));
  }
});
