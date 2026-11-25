import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { coreLicenseInputs } from "./core-licenses.mjs";
import { WASM_LICENSE_PATHS, verifyWasmLicenses } from "./wasm-build-licenses.mjs";
export { PACKAGE_LICENSE_SOURCES, coreLicenseInputs } from "./core-licenses.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

export async function packageLicenseInputs(packageJson, sourceRoot = REPO_ROOT) {
  const { expression, files } = await coreLicenseInputs(sourceRoot);
  if (packageJson.license !== expression) {
    throw new Error("npm license must match the embedded Core data license expression");
  }
  for (const path of [...files.map(({ path }) => path), ...WASM_LICENSE_PATHS]) {
    if (!Array.isArray(packageJson.files) ||
        packageJson.files.filter((entry) => entry === path).length !== 1) {
      throw new Error(`npm files must include exactly one ${path}`);
    }
  }
  return files;
}

export async function verifyPackageLicenses(packageDirectory, sourceRoot = REPO_ROOT) {
  const packageJson = JSON.parse(await readFile(resolve(packageDirectory, "package.json"), "utf8"));
  const inputs = await packageLicenseInputs(packageJson, sourceRoot);
  for (const { path, bytes } of inputs) {
    if (!(await readFile(resolve(packageDirectory, path))).equals(bytes)) {
      throw new Error(`packed license differs from canonical source: ${path}`);
    }
  }
  await verifyWasmLicenses(packageDirectory);
}
