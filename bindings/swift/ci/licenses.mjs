import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { distributionNotices, licenseInventory } from "../../../scripts/distribution-licenses.mjs";

const [output, artifactInventory] = process.argv.slice(2), toolchain = process.env.RUST_TOOLCHAIN;
if (!output || !artifactInventory || !toolchain) throw new Error("usage: RUST_TOOLCHAIN=... licenses.mjs SDK_DIRECTORY BUILD_PACKAGES_JSON");
const metadata = JSON.parse(execFileSync("cargo", [`+${toolchain}`, "metadata", "--locked", "--format-version", "1"],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
// Владельцем состава служат compiler-artifact сообщения реальных release-сборок,
// включая proc-macro и build dependencies. Cargo metadata без фильтра платформ
// содержит лишние ОС и dev-feature unions, поэтому не определяет эту выборку.
const identities = JSON.parse(await readFile(artifactInventory, "utf8"));
if (!Array.isArray(identities) || identities.length === 0 || new Set(identities).size !== identities.length) {
  throw new Error("native build package identities are missing or ambiguous");
}
if (!identities.some((id) => packages.get(id)?.name === "labcolors-ffi")) throw new Error("native product artifact is missing");
const components = [...identities].sort().map((id) => {
  const pkg = packages.get(id);
  if (!pkg?.license) throw new Error(`compiled dependency license is missing: ${id}`);
  return { type: "library", name: pkg.name, version: pkg.version,
    "bom-ref": `cargo:${pkg.name}@${pkg.version}#${pkg.source ?? "workspace"}`,
    licenses: [{ expression: pkg.license }] };
});
await writeFile(resolve(output, "dependency-licenses.json"), JSON.stringify(licenseInventory({ components }), null, 2) + "\n");
const upstream = JSON.parse(await readFile(new URL("../../../crates/labcolors-ffi/third-party-notices.json", import.meta.url), "utf8"));
if (upstream.schemaVersion !== 1 || !Array.isArray(upstream.notices)) throw new Error("unknown upstream notice inventory");
await writeFile(resolve(output, "THIRD-PARTY-NOTICES.md"),
  await distributionNotices(metadata, { components }, undefined, upstream.notices));
