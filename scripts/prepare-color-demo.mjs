import { createHash } from "node:crypto";
import { copyFile, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [directory, version, sourceSha] = process.argv.slice(2);
if (!directory || !/^\d+\.\d+\.\d+$/.test(version ?? "") || !/^[a-f0-9]{40}$/.test(sourceSha ?? "")) {
  throw new Error("usage: prepare-color-demo.mjs <installed site directory> <version> <release source SHA>");
}
const site = resolve(directory);
const packageRoot = resolve(site, "node_modules/@labpics/colors");
const metadata = JSON.parse(await readFile(resolve(packageRoot, "build-metadata.json"), "utf8"));
const manifest = JSON.parse(await readFile(resolve(packageRoot, "package.json"), "utf8"));
if (
  manifest.name !== "@labpics/colors" || manifest.version !== version ||
  metadata.schemaVersion !== 2 || metadata.package?.name !== manifest.name ||
  metadata.package?.version !== version || metadata.sourceSha !== sourceSha
) {
  throw new Error("Installed demo package does not match the published release identity");
}
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
const sha256 = createHash("sha256").update(wasm).digest("hex");
const runtime = metadata.wasm?.find((entry) => entry.role === "runtime");
if (runtime?.path !== "pkg/labcolors_bg.wasm" || runtime.bytes !== wasm.length || runtime.sha256 !== sha256) {
  throw new Error("Installed demo WASM does not match its package metadata");
}
for (const file of ["index.html", "style.css", "main.js", "declaration.js", "interaction.js"]) {
  await copyFile(resolve(root, "examples/contextual-colors", file), resolve(site, file));
}
await writeFile(resolve(site, "release.json"), `${JSON.stringify({
  package: manifest.name, version, sourceSha, wasm: { bytes: wasm.length, sha256 },
}, null, 2)}\n`);
// npm's install-only manifest is not part of the browser example.
await rm(resolve(site, "package.json"), { force: true });
await rm(resolve(site, "node_modules/.package-lock.json"), { force: true });
console.log(JSON.stringify({ version, sourceSha, wasmBytes: wasm.length, wasmSha256: sha256 }));
