import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { test } from "node:test";

const ROOT = resolve(import.meta.dirname, "../../..");
const read = (...parts) => readFileSync(join(ROOT, ...parts), "utf8");

test("effective-background math stays internal to the browser shell", () => {
  const manifest = JSON.parse(read("packages", "colors", "package.json"));
  const rootRuntime = read("packages", "colors", "index.js");
  const rootTypes = read("packages", "colors", "index.d.ts");
  const releaseVerifier = read("scripts", "verify-package-release.mjs");

  assert.equal(manifest.exports["./effective-bg"], undefined);
  assert.doesNotMatch(releaseVerifier, /from "@labpics\/colors\/effective-bg"/u);
  assert.match(releaseVerifier, /import\("@labpics\/colors\/effective-bg"\)/u);
  for (const name of [
    "effectiveBackground",
    "parseCssColor",
    "compositeOver",
    "compositeStackToHex",
    "toHex",
    "oklabLerp",
  ]) {
    assert.doesNotMatch(rootRuntime, new RegExp(`\\b${name}\\b`, "u"));
    assert.doesNotMatch(rootTypes, new RegExp(`\\b${name}\\b`, "u"));
  }
  assert.ok(
    manifest.files.includes("effective-bg.js"),
    "watch/adapt still need the internal estimate until occurrence cutover",
  );
});

test("the unsupported strict transition recipe cannot re-enter source or shipped declarations", () => {
  const runtime = read("packages", "colors", "adapt-theme.js");
  const declarations = read("packages", "colors", "adapt-theme.d.ts");
  const consumer = read("packages", "colors", "smoke.consumer.ts");
  const docs = read("packages", "colors", "README.md");
  const sourceClaims = [
    read("crates", "labcolors-wasm", "src", "lib.rs"),
    read("crates", "labcolors-wasm", "src", "dto.rs"),
    read("packages", "colors", "pkg", "labcolors.d.ts"),
  ];

  for (const source of [runtime, declarations, consumer, docs]) {
    assert.doesNotMatch(source, /\bstrict\s*\??\s*:/u);
  }
  for (const source of sourceClaims) {
    assert.doesNotMatch(source, /`strict`/u);
  }
  assert.doesNotMatch(runtime, /floorBlend|lerpPairLuminance|wcagLuminanceCached/u);
});

test("the unshipped JavaScript FNV mirror stays deleted", () => {
  for (const path of [
    ["packages", "colors", "fnv1a.js"],
    ["packages", "colors", "fnv1a.d.ts"],
    ["packages", "colors", "test", "fnv1a-differential.test.mjs"],
  ]) {
    assert.equal(existsSync(join(ROOT, ...path)), false, path.join("/"));
  }
  assert.doesNotMatch(read("crates", "labcolors-core", "src", "hash.rs"), /packages\/colors\/fnv1a/u);
});
