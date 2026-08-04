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
  const backdropRuntime = read("packages", "colors", "effective-bg.js");
  const observationRuntime = read("packages", "colors", "background-observation.js");
  const adaptRuntime = read("packages", "colors", "adapt-theme.js");
  const releaseVerifier = read("scripts", "verify-package-release.mjs");

  assert.equal(manifest.exports["./effective-bg"], undefined);
  assert.equal(manifest.exports["./background-observation"], undefined);
  assert.doesNotMatch(releaseVerifier, /from "@labpics\/colors\/effective-bg"/u);
  assert.match(releaseVerifier, /import\("@labpics\/colors\/effective-bg"\)/u);
  for (const name of [
    "effectiveBackground",
    "observePointBackground",
    "parseCssColor",
    "compositeOver",
    "compositeStackToHex",
    "toHex",
    "oklabLerp",
    "__over",
  ]) {
    assert.doesNotMatch(rootRuntime, new RegExp(`\\b${name}\\b`, "u"));
    assert.doesNotMatch(rootTypes, new RegExp(`\\b${name}\\b`, "u"));
  }
  assert.ok(
    manifest.files.includes("effective-bg.js"),
    "controllers still need package-private parsing and interpolation helpers",
  );
  assert.ok(
    manifest.files.includes("background-observation.js"),
    "controllers need the package-private Point | Unknown bridge in the tarball",
  );
  assert.equal(manifest.exports["./pkg/labcolors.js"], undefined);
  assert.doesNotMatch(backdropRuntime, /__over|effectiveBackground/u);
  assert.match(observationRuntime, /__over/u);
  assert.match(adaptRuntime, /import\s*\{\s*__over\s*\}/u);
  assert.doesNotMatch(adaptRuntime, /\.compositeHex\b|\["compositeHex"\]/u);
  assert.match(observationRuntime, /export function observePointBackground/u);
  assert.doesNotMatch(
    backdropRuntime,
    /export function compositeOver|function compositeOver|compositeStackToHex/u,
  );
});

test("public initialisation cannot leak raw WASM exports", async () => {
  const publicRoot = await import("../index.js");
  const result = publicRoot.initSync({
    module: new WebAssembly.Module(
      readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)),
    ),
  });

  assert.equal(result, undefined);
  assert.equal(publicRoot.__over, undefined);
});

test("the parse memo never exposes its shared cache entry", async () => {
  const backdrop = await import("../effective-bg.js");
  assert.equal(backdrop.parseCssColorCached, undefined);
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

test("runtime documentation names the declared canvas instead of the removed fallback option", () => {
  const docs = read("packages", "colors", "README.md");

  assert.doesNotMatch(docs, /^\s*fallback\??\s*:/mu);
  assert.match(docs, /^\s*canvas\??\s*:/mu);
});

test("repository docs preserve the Point-or-Unknown observation contract", () => {
  const rootReadme = read("README.md");
  const whitepaper = read("docs", "whitepaper.md");

  assert.doesNotMatch(rootReadme, /\beffectiveBackground\b/u);
  assert.doesNotMatch(
    whitepaper,
    /legacy helper.*(?:белую базу|white base)/isu,
  );
  for (const documentation of [rootReadme, whitepaper]) {
    assert.doesNotMatch(
      documentation,
      /package-private helpers|observation boundary/u,
    );
  }
  assert.doesNotMatch(
    whitepaper,
    /reference estimate|occurrence-\s*контракт|\bcaller\b/u,
  );
  assert.match(rootReadme, /границы наблюдения/u);
  assert.match(whitepaper, /граница наблюдения/u);
  assert.match(rootReadme, /типизированным `Unknown`/u);
  assert.match(whitepaper, /типизированный исход `Unknown`/u);
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
