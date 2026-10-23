import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { distributionNotices, licenseInventory } from "../../../scripts/distribution-licenses.mjs";

const output = process.argv[2], toolchain = process.env.RUST_TOOLCHAIN;
if (!output || !toolchain) throw new Error("usage: RUST_TOOLCHAIN=... licenses.mjs SDK_DIRECTORY");
const metadata = JSON.parse(execFileSync("cargo", [`+${toolchain}`, "metadata", "--locked", "--format-version", "1"],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
const root = metadata.packages.find((pkg) => pkg.name === "labcolors-ffi" && metadata.workspace_members.includes(pkg.id));
if (!root) throw new Error("native product package is missing");
const seen = new Set(), pending = [root.id];
while (pending.length) {
  const id = pending.pop();
  if (seen.has(id)) continue;
  seen.add(id);
  const node = nodes.get(id);
  if (!node) throw new Error(`dependency node is missing: ${id}`);
  for (const dependency of node.deps) {
    if (dependency.dep_kinds.some((kind) => kind.kind !== "dev")) pending.push(dependency.pkg);
  }
}
// Нормальные и build-зависимости намеренно перечислены вместе. Это инвентарь
// сборки, а не утверждение, что весь код компилятора встроен в runtime.
const components = [...seen].sort().map((id) => {
  const pkg = packages.get(id);
  if (!pkg?.license) throw new Error(`dependency license is missing: ${id}`);
  return { type: "library", name: pkg.name, version: pkg.version,
    "bom-ref": `cargo:${pkg.name}@${pkg.version}#${pkg.source ?? "workspace"}`,
    licenses: [{ expression: pkg.license }] };
});
const inventory = licenseInventory({ components });
await writeFile(resolve(output, "dependency-licenses.json"), JSON.stringify(inventory, null, 2) + "\n");
await writeFile(resolve(output, "THIRD-PARTY-NOTICES.md"), await distributionNotices(metadata, { components }));
