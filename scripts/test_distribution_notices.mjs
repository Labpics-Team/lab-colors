import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cp, mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { compiledLicenseComponents, distributionNotices } from "./distribution-licenses.mjs";
import { verifyWasmLicenses, wasmBuildPackageIds, writeWasmLicenses } from "./wasm-build-licenses.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "distribution-notices-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const dependency = join(root, "dependency");
  await mkdir(dependency);
  await writeFile(join(root, "Cargo.toml"), '[workspace.package]\nlicense = "MIT"\n');
  await writeFile(join(dependency, "Cargo.toml"), '[package]\nname = "fixture"\n');
  const revision = "a".repeat(40), repository = "https://example.invalid/fixture";
  await writeFile(join(dependency, ".cargo_vcs_info.json"), JSON.stringify({ git: { sha1: revision } }));
  const text = "License fixture, not a real dependency license.\n";
  await writeFile(join(root, "upstream.txt"), text);
  const pkg = { id: "fixture-id", name: "fixture", version: "1.0.0", source: "registry+fixture",
    license: "MPL-2.0", repository, manifest_path: join(dependency, "Cargo.toml") };
  const metadata = { packages: [pkg], workspace_members: [] };
  const sbom = { components: [{ name: pkg.name, version: pkg.version,
    "bom-ref": "cargo:fixture@1.0.0#registry+fixture", licenses: [{ expression: pkg.license }] }] };
  const entry = { repository, revision, license: pkg.license, upstreamPath: "LICENSE", path: "upstream.txt",
    sha256: createHash("sha256").update(text).digest("hex") };
  return { root, dependency, text, entry, metadata, sbom };
}

test("missing packaged notices use only exact published source provenance", async (t) => {
  const f = await fixture(t);
  const result = await distributionNotices(f.metadata, f.sbom, f.root, [f.entry]);
  assert.ok(result.includes(f.text));
  assert.ok(result.includes(f.entry.repository + "@" + f.entry.revision + "/LICENSE"));
});

test("foreign, stale, mismatched and ambiguous upstream notices are rejected", async (t) => {
  const f = await fixture(t);
  for (const entries of [[], [{ ...f.entry, repository: "https://example.invalid/foreign" }],
    [{ ...f.entry, revision: "b".repeat(40) }], [{ ...f.entry, license: "MIT" }],
    [{ ...f.entry, sha256: "0".repeat(64) }], [f.entry, f.entry]]) {
    await assert.rejects(distributionNotices(f.metadata, f.sbom, f.root, entries));
  }
  await writeFile(join(f.dependency, ".cargo_vcs_info.json"), JSON.stringify({ git: {} }));
  await assert.rejects(distributionNotices(f.metadata, f.sbom, f.root, [f.entry]), /provenance/u);
});

test("shipped notices remain usable without a supplemental inventory", async (t) => {
  const f = await fixture(t);
  await writeFile(join(f.dependency, "LICENSE"), "Shipped license fixture.\n");
  const result = await distributionNotices(f.metadata, f.sbom, f.root);
  assert.ok(result.includes("Shipped license fixture."));
  assert.ok(!result.includes(f.text));
});

test("supplemental notice paths cannot escape the source owner", async (t) => {
  const f = await fixture(t);
  for (const path of ["../upstream.txt", join(f.root, "upstream.txt"), "a/../../upstream.txt"]) {
    await assert.rejects(distributionNotices(f.metadata, f.sbom, f.root, [{ ...f.entry, path }]), /provenance/u);
  }
  await writeFile(join(f.root, "upstream.txt"), "changed source text\n");
  await assert.rejects(distributionNotices(f.metadata, f.sbom, f.root, [f.entry]), /digest/u);
});

const artifact = (package_id, product = false) => ({
  reason: "compiler-artifact", package_id, fresh: true, profile: { test: false },
  target: { name: product ? "labcolors_wasm" : "dependency", crate_types: [product ? "cdylib" : "lib"] },
  filenames: product ? ["/target/wasm32-unknown-unknown/release/labcolors_wasm.wasm"] : [],
});
const finished = { reason: "build-finished", success: true };
const messages = (rows) => rows.map((row) => JSON.stringify(row)).join("\n");

test("WASM inventory uses complete successful Cargo artifacts, including fresh build dependencies", () => {
  const rows = [artifact("build-script"), artifact("dependency"), artifact("dependency"), artifact("product", true), finished];
  assert.deepEqual(wasmBuildPackageIds(`ordinary wasm-pack progress\n${messages(rows)}\n`),
    ["build-script", "dependency", "product"]);
  for (const broken of [[], rows.slice(0, -1), rows.filter((row) => row.package_id !== "product"),
    [...rows.slice(0, -1), { ...finished, success: false }], [...rows, finished],
    [...rows, artifact("late")], [{ ...artifact("test"), profile: { test: true } }, ...rows],
    [{ ...artifact(""), package_id: null }, ...rows]]) {
    assert.throws(() => wasmBuildPackageIds(messages(broken)), /WASM/u);
  }
  assert.throws(() => wasmBuildPackageIds(`${messages(rows)}\n{broken JSON`), SyntaxError);
  assert.throws(() => wasmBuildPackageIds(messages(rows), "different-product"), /lack the product artifact/u);
});

async function wasmFixture(t) {
  const f = await fixture(t);
  await writeFile(join(f.dependency, "LICENSE.txt"), "Dependency license text.\n");
  await writeFile(join(f.dependency, "LICENSE-DETAILS.md"), "Dependency attribution details.\n");
  await writeFile(join(f.root, "LICENSE"), "Workspace license.\n");
  await writeFile(join(f.root, "Cargo.lock"), "fixture Cargo lockfile\n");
  const core = join(f.root, "crates/labcolors-core");
  const wasm = join(f.root, "crates/labcolors-wasm");
  await mkdir(join(core, "LICENSES"), { recursive: true });
  await mkdir(wasm, { recursive: true });
  for (const path of ["NOTICE.md", "LICENSES/CC-BY-4.0.txt", "LICENSES/CC-BY-SA-4.0.txt"]) {
    await writeFile(join(core, path), `Core fixture material: ${path}\n`);
  }
  const members = [core, wasm].map((path, index) => ({
    id: index === 0 ? "core" : "wasm", name: index === 0 ? "labcolors-core" : "labcolors-wasm",
    version: "0.0.0", license: "MIT", source: null, manifest_path: join(path, "Cargo.toml"),
  }));
  for (const pkg of members) await writeFile(pkg.manifest_path, `[package]\nname = "${pkg.name}"\nlicense = "MIT"\n`);
  f.metadata.packages.push(...members, { ...f.metadata.packages[0], id: "unused", name: "unused-dev-dependency" });
  f.metadata.workspace_members = members.map(({ id }) => id);
  f.packageDirectory = join(f.root, "packages/colors");
  await mkdir(join(f.packageDirectory, "pkg"), { recursive: true });
  await writeFile(join(f.packageDirectory, "pkg/labcolors_bg.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  f.messages = messages([artifact("fixture-id"), artifact("core"), artifact("wasm", true), finished]);
  return f;
}

test("compiled identities select actual product dependencies without workspace dev-feature unions", async (t) => {
  const f = await wasmFixture(t);
  const ids = wasmBuildPackageIds(f.messages);
  const components = compiledLicenseComponents(f.metadata, ids, "labcolors-wasm");
  assert.deepEqual(components.map(({ name }) => name).sort(), ["fixture", "labcolors-core", "labcolors-wasm"]);
  for (const invalid of [[], [...ids, ids[0]], [...ids, "unknown"], ids.filter((id) => id !== "wasm")]) {
    assert.throws(() => compiledLicenseComponents(f.metadata, invalid, "labcolors-wasm"));
  }
  const falseProduct = messages([artifact("fixture-id", true), artifact("core"), artifact("wasm"), finished]);
  await assert.rejects(writeWasmLicenses(falseProduct, f.root, f.metadata), /lack the product artifact/u);
});

test("WASM legal files retain upstream notices and verify outside the source checkout", async (t) => {
  const f = await wasmFixture(t);
  await writeWasmLicenses(f.messages, f.root, f.metadata);
  const inventory = await verifyWasmLicenses(f.packageDirectory, f.root);
  assert.equal(inventory.components.length, 3);
  const text = await readFile(join(f.packageDirectory, "pkg/THIRD-PARTY-NOTICES.md"), "utf8");
  assert.match(text, /Dependency license text/u);
  assert.match(text, /Dependency attribution details/u);
  assert.doesNotMatch(text, /unused-dev-dependency/u);
  const installed = join(f.root, "installed");
  await cp(f.packageDirectory, installed, { recursive: true });
  await rm(f.packageDirectory, { recursive: true });
  await rm(join(f.root, "Cargo.lock"));
  assert.deepEqual(await verifyWasmLicenses(installed), inventory);
});

test("WASM license binding rejects changed final module, lockfile, notices and inventory", async (t) => {
  const f = await wasmFixture(t);
  await writeWasmLicenses(f.messages, f.root, f.metadata);
  for (const [path, replacement] of [
    [join(f.packageDirectory, "pkg/labcolors_bg.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, 0, 1, 0])],
    [join(f.packageDirectory, "pkg/THIRD-PARTY-NOTICES.md"), "Altered notice text.\n"],
    [join(f.root, "Cargo.lock"), "different dependency resolution\n"],
  ]) {
    const original = await readFile(path);
    await writeFile(path, replacement);
    await assert.rejects(verifyWasmLicenses(f.packageDirectory, f.root), /dependency licenses differ/u);
    await writeFile(path, original);
  }
  const path = join(f.packageDirectory, "pkg/dependency-licenses.json");
  const original = await readFile(path);
  for (const change of [
    (value) => { value.components.pop(); },
    (value) => { value.components.push(value.components[0]); },
    (value) => { value.build.noticesSha256 = "0".repeat(64); },
    (value) => { value.build.unexpected = true; },
  ]) {
    const altered = JSON.parse(original);
    change(altered);
    await writeFile(path, JSON.stringify(altered));
    await assert.rejects(verifyWasmLicenses(f.packageDirectory, f.root));
  }
  await writeFile(path, original);
  await verifyWasmLicenses(f.packageDirectory, f.root);
});

test("source-free verification rejects ordinary dependency removal and license substitution with unchanged build hashes", async (t) => {
  const f = await wasmFixture(t);
  await writeWasmLicenses(f.messages, f.root, f.metadata);
  const path = join(f.packageDirectory, "pkg/dependency-licenses.json");
  const original = JSON.parse(await readFile(path, "utf8"));
  const dependency = original.components.find((component) => !component.ref.endsWith("#workspace"));
  assert.ok(dependency, "fixture must include an ordinary registry dependency");
  for (const change of [
    (value) => { value.components = value.components.filter((component) => component.ref !== dependency.ref); },
    (value) => { value.components.find((component) => component.ref === dependency.ref).licenseExpressions = ["Unlicense"]; },
  ]) {
    const altered = structuredClone(original);
    change(altered);
    assert.deepEqual(altered.build, original.build, "mutations must preserve the producer's hashes");
    await writeFile(path, JSON.stringify(altered));
    await assert.rejects(verifyWasmLicenses(f.packageDirectory), /dependency licenses differ/u);
  }
  await writeFile(path, JSON.stringify(original));
  await verifyWasmLicenses(f.packageDirectory);
});
