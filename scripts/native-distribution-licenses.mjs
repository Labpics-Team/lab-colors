import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { compiledLicenseComponents, distributionNotices, licenseInventory } from "./distribution-licenses.mjs";

const [output, artifactInventory] = process.argv.slice(2);
if (!output || !artifactInventory) throw new Error("usage: licenses.mjs SDK_DIRECTORY BUILD_PACKAGES_JSON");
const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
// Владельцем состава служат compiler-artifact сообщения реальных release-сборок,
// включая proc-macro и build dependencies. Cargo metadata без фильтра платформ
// содержит лишние ОС и dev-feature unions, поэтому не определяет эту выборку.
const identities = JSON.parse(await readFile(artifactInventory, "utf8"));
const components = compiledLicenseComponents(metadata, identities, "labcolors-ffi");
await writeFile(resolve(output, "dependency-licenses.json"), JSON.stringify(licenseInventory({ components }), null, 2) + "\n");
const upstream = JSON.parse(await readFile(new URL("../crates/labcolors-ffi/third-party-notices.json", import.meta.url), "utf8"));
if (upstream.schemaVersion !== 1 || !Array.isArray(upstream.notices)) throw new Error("unknown upstream notice inventory");
await writeFile(resolve(output, "THIRD-PARTY-NOTICES.md"),
  await distributionNotices(metadata, { components }, undefined, upstream.notices));
