import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { packageLicense } from "./cargo-workspace.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

// Файлы для потребителя берутся у владельца встроенных данных.
export const PACKAGE_LICENSE_SOURCES = Object.freeze({
  LICENSE: "LICENSE",
  "NOTICE.md": "crates/labcolors-core/NOTICE.md",
  "LICENSES/CC-BY-4.0.txt": "crates/labcolors-core/LICENSES/CC-BY-4.0.txt",
  "LICENSES/CC-BY-SA-4.0.txt": "crates/labcolors-core/LICENSES/CC-BY-SA-4.0.txt",
});

export async function coreLicenseInputs(sourceRoot = REPO_ROOT) {
  const expression = packageLicense(await readFile(resolve(sourceRoot,
    "crates/labcolors-core/Cargo.toml"), "utf8"));
  const files = await Promise.all(Object.entries(PACKAGE_LICENSE_SOURCES).map(async ([path, source]) => {
    const bytes = await readFile(resolve(sourceRoot, source));
    if (bytes.length === 0) throw new Error(`canonical license is empty: ${source}`);
    return { path, bytes };
  }));
  return { expression, files };
}

export async function packageLicenseInputs(packageJson, sourceRoot = REPO_ROOT) {
  const { expression, files } = await coreLicenseInputs(sourceRoot);
  if (packageJson.license !== expression) {
    throw new Error("npm license must match the embedded Core data license expression");
  }
  for (const { path } of files) {
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
}
