import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { atomicWriteGeneratedFile } from "./atomic-write.mjs";
import { compiledLicenseComponents, distributionNotices, licenseInventory } from "./distribution-licenses.mjs";

export const WASM_LICENSE_PATHS = Object.freeze([
  "pkg/dependency-licenses.json", "pkg/THIRD-PARTY-NOTICES.md",
]);
const WASM_PATH = "pkg/labcolors_bg.wasm";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// Cargo emits fresh artifacts too. Keep build/proc-macro dependencies, as in
// native distributions, and never infer the build from a workspace-wide graph.
export function wasmBuildPackageIds(messages, productPackageId) {
  const identities = new Set();
  let finished = false;
  let product = false;
  for (const line of messages.split(/\r?\n/u)) {
    // wasm-pack may interleave ordinary progress output with Cargo's JSON.
    if (!line.trimStart().startsWith("{")) continue;
    const message = JSON.parse(line);
    if (message.reason === "compiler-artifact") {
      if (finished || typeof message.package_id !== "string" || !message.package_id || message.profile?.test !== false) {
        throw new Error("invalid WASM compiler-artifact message");
      }
      identities.add(message.package_id);
      if ((!productPackageId || message.package_id === productPackageId) &&
          message.target?.name === "labcolors_wasm" &&
          message.target.crate_types?.includes("cdylib") &&
          message.filenames?.some((path) => /(?:^|[/\\])labcolors_wasm\.wasm$/u.test(path))) {
        product = true;
      }
    } else if (message.reason === "build-finished") {
      if (finished || message.success !== true) throw new Error("WASM Cargo build did not finish successfully");
      finished = true;
    }
  }
  if (!finished || !product || identities.size === 0) {
    throw new Error("WASM build messages are incomplete or lack the product artifact");
  }
  return [...identities].sort();
}

export async function writeWasmLicenses(messages, sourceRoot = process.cwd(), metadata) {
  metadata ??= JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"],
    { cwd: sourceRoot, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
  const products = metadata.packages.filter((pkg) => pkg.name === "labcolors-wasm" &&
    metadata.workspace_members.includes(pkg.id));
  if (products.length !== 1) throw new Error("WASM product package is missing or ambiguous");
  const identities = wasmBuildPackageIds(messages, products[0].id);
  const components = compiledLicenseComponents(metadata, identities, "labcolors-wasm");
  const packageDirectory = resolve(sourceRoot, "packages/colors");
  const notices = Buffer.from(await distributionNotices(metadata, { components }, sourceRoot));
  const wasm = await readFile(resolve(packageDirectory, WASM_PATH));
  if (!WebAssembly.validate(wasm)) throw new Error("license inventory requires the final valid WASM artifact");
  const licenses = licenseInventory({ components });
  const inventory = {
    ...licenses,
    build: {
      inventorySha256: sha256(JSON.stringify(licenses)),
      cargoLockSha256: sha256(await readFile(resolve(sourceRoot, "Cargo.lock"))),
      wasmSha256: sha256(wasm),
      noticesSha256: sha256(notices),
    },
  };
  await atomicWriteGeneratedFile(resolve(packageDirectory, WASM_LICENSE_PATHS[1]), notices);
  await atomicWriteGeneratedFile(resolve(packageDirectory, WASM_LICENSE_PATHS[0]), `${JSON.stringify(inventory, null, 2)}\n`);
  await verifyWasmLicenses(packageDirectory, sourceRoot);
}

// This check also runs in an installed npm consumer without Cargo, a registry
// cache, or a producer checkout. Prepack additionally binds the source lockfile.
// These hashes check consistency; published artifact attestations authenticate
// the producer. The canonical inventory itself is part of the build binding.
export async function verifyWasmLicenses(packageDirectory, sourceRoot) {
  const inventory = JSON.parse(await readFile(resolve(packageDirectory, WASM_LICENSE_PATHS[0]), "utf8"));
  const components = inventory.components?.map((component) => ({
    name: component.name, version: component.version, "bom-ref": component.ref,
    licenses: component.licenseExpressions?.map((expression) => ({ expression })),
  }));
  const expected = licenseInventory({ components });
  const wasm = await readFile(resolve(packageDirectory, WASM_PATH));
  const notices = await readFile(resolve(packageDirectory, WASM_LICENSE_PATHS[1]));
  const build = inventory.build;
  if (!WebAssembly.validate(wasm) || notices.length === 0 ||
      !/^[0-9a-f]{64}$/u.test(build?.cargoLockSha256 ?? "") ||
      !isDeepStrictEqual(inventory, { ...expected, build: {
        inventorySha256: sha256(JSON.stringify(expected)),
        cargoLockSha256: build.cargoLockSha256,
        wasmSha256: sha256(wasm), noticesSha256: sha256(notices),
      } }) || !["labcolors-core", "labcolors-wasm"].every((name) =>
        expected.components.some((component) => component.name === name && component.ref.endsWith("#workspace")))) {
    throw new Error("WASM dependency licenses differ from the build artifacts");
  }
  if (sourceRoot && build.cargoLockSha256 !== sha256(await readFile(resolve(sourceRoot, "Cargo.lock")))) {
    throw new Error("WASM dependency licenses differ from the source Cargo.lock");
  }
  return inventory;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [messages, sourceRoot] = process.argv.slice(2);
  if (!messages) throw new Error("usage: wasm-build-licenses.mjs CARGO_MESSAGES_JSONL [SOURCE_ROOT]");
  await writeWasmLicenses(await readFile(messages, "utf8"), sourceRoot);
}
