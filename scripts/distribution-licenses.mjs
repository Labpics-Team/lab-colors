import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { coreLicenseInputs } from "./package-licenses.mjs";
import { workspaceLicense } from "./cargo-workspace.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const NOTICE_NAME = /^(?:licen[cs]e|copying|copyright|notice|unlicense)(?:[._-].*)?$/iu;
const NOTICE_DIRECTORY = /^licen[cs]es$/iu;
const utf8 = new TextDecoder("utf-8", { fatal: true });

export function licenseInventory(sbom) {
  const refs = new Set();
  if (!Array.isArray(sbom?.components) || sbom.components.length === 0) {
    throw new Error("license inventory requires SBOM components");
  }
  const components = sbom.components.map((component) => {
    const ref = component["bom-ref"];
    if (typeof ref !== "string" || !ref || refs.has(ref) ||
        typeof component.name !== "string" || !component.name ||
        typeof component.version !== "string" || !component.version ||
        !Array.isArray(component.licenses) || component.licenses.length === 0) {
      throw new Error("license inventory contains an invalid or duplicate component");
    }
    refs.add(ref);
    const licenseExpressions = component.licenses.map((entry) => entry.expression);
    if (licenseExpressions.some((value) => typeof value !== "string" || !value.trim()) ||
        new Set(licenseExpressions).size !== licenseExpressions.length) {
      throw new Error("license inventory contains an invalid license expression");
    }
    return { ref, name: component.name, version: component.version,
      licenseExpressions: licenseExpressions.sort() };
  });
  return { schemaVersion: 1, components };
}

export function verifyLicenseInventory(inventory, sbom) {
  if (!isDeepStrictEqual(inventory, licenseInventory(sbom))) {
    throw new Error("license inventory differs from the SBOM");
  }
}

function within(directory, path) {
  const local = relative(directory, path);
  return local !== ".." && !local.startsWith(`..${sep}`) && !isAbsolute(local);
}

async function noticeBytes(directory, path) {
  const absolute = resolve(directory, path);
  if (!within(directory, absolute) || !within(directory, await realpath(absolute)) ||
      !(await lstat(absolute)).isFile()) {
    throw new Error(`license notice must be a regular file inside its package: ${path}`);
  }
  const bytes = await readFile(absolute);
  const text = utf8.decode(bytes);
  if (!text.trim()) throw new Error(`license notice is empty: ${path}`);
  return text;
}

async function crateNotices(pkg) {
  if (typeof pkg.manifest_path !== "string" || !isAbsolute(pkg.manifest_path)) {
    throw new Error(`Cargo manifest path is missing: ${pkg.name}`);
  }
  const directory = await realpath(dirname(pkg.manifest_path));
  const paths = new Set();
  if (pkg.license_file) {
    const path = relative(directory, resolve(directory, pkg.license_file));
    if (!within(directory, resolve(directory, path))) {
      throw new Error(`declared license is outside its package: ${pkg.name}`);
    }
    paths.add(path);
  }
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (NOTICE_NAME.test(entry.name)) paths.add(entry.name);
    if (NOTICE_DIRECTORY.test(entry.name)) {
      if (!entry.isDirectory()) throw new Error(`license directory is not a directory: ${pkg.name}`);
      for (const file of await readdir(resolve(directory, entry.name))) {
        paths.add(`${entry.name}/${file}`);
      }
    }
  }
  if (paths.size === 0) throw new Error(`Cargo package has no license notices: ${pkg.name}`);
  return Promise.all([...paths].sort().map(async (path) => ({
    path: path.split(sep).join("/"), text: await noticeBytes(directory, path),
  })));
}

export async function distributionNotices(metadata, sbom, sourceRoot = REPO_ROOT) {
  const inventory = licenseInventory(sbom);
  const byRef = new Map(metadata.packages.map((pkg) => [
    `cargo:${pkg.name}@${pkg.version}#${pkg.source ?? "workspace"}`, pkg,
  ]));
  if (byRef.size !== metadata.packages.length) throw new Error("duplicate Cargo package identity");
  const workspace = new Set(metadata.workspace_members ?? []);
  const root = await realpath(sourceRoot);
  const workspaceExpression = workspaceLicense(await readFile(resolve(root, "Cargo.toml"), "utf8"));
  const core = inventory.components.find((component) => component.name === "labcolors-core" &&
    workspace.has(byRef.get(component.ref)?.id));
  const sections = ["Lab Colors: лицензии и атрибуция зависимостей\n"];
  if (core) {
    const materials = await coreLicenseInputs(root);
    if (!isDeepStrictEqual(core.licenseExpressions, [materials.expression])) {
      throw new Error("Core SPDX expression differs from its source");
    }
    sections.push(...materials.files.map(({ path, bytes }) =>
      `## ${path}\n\n${utf8.decode(bytes).trimEnd()}\n`));
  }
  for (const component of inventory.components) {
    const pkg = byRef.get(component.ref);
    if (!pkg || !isDeepStrictEqual(component.licenseExpressions, [pkg.license])) {
      throw new Error(`license component differs from Cargo metadata: ${component.ref}`);
    }
    sections.push(`## ${component.name} ${component.version}\n\n${component.ref}\n${pkg.license}\n`);
    if (workspace.has(pkg.id)) {
      if (!within(root, await realpath(pkg.manifest_path))) {
        throw new Error(`workspace manifest is outside its source: ${pkg.name}`);
      }
      if (core?.ref === component.ref) continue;
      if (pkg.license === workspaceExpression) {
        sections.push(`### LICENSE\n\n${(await noticeBytes(root, "LICENSE")).trimEnd()}\n`);
        continue;
      }
    }
    for (const { path, text } of await crateNotices(pkg)) {
      sections.push(`### ${path}\n\n${text.trimEnd()}\n`);
    }
  }
  return sections.join("\n");
}
