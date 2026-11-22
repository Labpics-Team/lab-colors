import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { extname, join, resolve } from "node:path";
import { setTimeout } from "node:timers/promises";

const [address, directory] = process.argv.slice(2);
if (!address || !directory) {
  throw new Error("usage: check-color-demo.mjs <HTTP base URL> <prepared site directory>");
}
const site = resolve(directory);
const base = new URL(address.endsWith("/") ? address : `${address}/`);
const mime = new Map([
  [".html", ["text/html"]], [".css", ["text/css"]],
  [".js", ["application/javascript", "text/javascript"]],
  [".json", ["application/json"]], [".wasm", ["application/wasm"]],
]);

// A bounded startup wait does not hide a failed request after readiness.
for (let attempt = 0; ; attempt += 1) {
  try {
    const response = await fetch(base, { signal: AbortSignal.timeout(1000) });
    assert.equal(response.status, 200);
    await response.arrayBuffer();
    break;
  } catch (error) {
    if (attempt === 39) throw error;
    await setTimeout(250);
  }
}

async function files(path = "") {
  const result = [];
  for (const entry of await readdir(join(site, path), { withFileTypes: true })) {
    const name = path ? `${path}/${entry.name}` : entry.name;
    if (entry.isDirectory()) result.push(...await files(name));
    else if (entry.isFile() && mime.has(extname(name))) result.push(name);
  }
  return result.sort();
}

const names = await files();
assert(names.includes("release.json"));
assert(names.includes("node_modules/@labpics/colors/pkg/labcolors_bg.wasm"));
for (const name of names) {
  const response = await fetch(new URL(name, base), { signal: AbortSignal.timeout(5000) });
  assert.equal(response.status, 200, name);
  const actualMime = response.headers.get("content-type")?.split(";")[0];
  assert(mime.get(extname(name)).includes(actualMime), `${name}: ${actualMime}`);
  assert.equal(response.headers.get("x-content-type-options"), "nosniff", name);
  assert.equal(response.headers.get("cache-control"), "no-cache", name);
  assert.deepEqual(Buffer.from(await response.arrayBuffer()), await readFile(join(site, name)), name);
}
const index = await fetch(base);
assert.deepEqual(Buffer.from(await index.arrayBuffer()), await readFile(join(site, "index.html")));
const missing = await fetch(new URL("__missing_color_demo_asset__", base));
assert.equal(missing.status, 404, "Missing assets must not become the HTML entry point");
console.log(JSON.stringify({ checkedFiles: names.length, exactBytes: true, wasmMime: "application/wasm" }));
