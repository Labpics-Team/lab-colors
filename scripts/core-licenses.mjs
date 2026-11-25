import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { packageLicense } from "./cargo-workspace.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

// Канонические материалы кода и встроенных данных принадлежат Core.
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
