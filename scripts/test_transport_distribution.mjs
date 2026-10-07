import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildSbom, verifyBundle } from "./transport-distribution.mjs";
import { distributionNotices, licenseInventory, verifyLicenseInventory } from "./distribution-licenses.mjs";

function metadataFixture() {
  const root = { id: "root", name: "labcolors-transport-cli", version: "1.0.0", source: null, license: "MIT" };
  const core = { id: "core", name: "labcolors-core", version: "1.0.0", source: null, license: "MIT" };
  const serde = { id: "serde", name: "serde", version: "1.0.0", source: "registry+https://github.com/rust-lang/crates.io-index", license: "MIT OR Apache-2.0" };
  const dev = { id: "dev", name: "dev-only", version: "1.0.0", source: "registry+https://github.com/rust-lang/crates.io-index", license: "MIT" };
  return {
    packages: [root, core, serde, dev],
    resolve: { nodes: [
      { id: "root", deps: [
        { pkg: "core", dep_kinds: [{ kind: null, target: null }] },
        { pkg: "dev", dep_kinds: [{ kind: "dev", target: null }] },
      ] },
      { id: "core", deps: [{ pkg: "serde", dep_kinds: [{ kind: "build", target: null }] }] },
      { id: "serde", deps: [] },
      { id: "dev", deps: [] },
    ] },
  };
}

test("SBOM follows normal/build closure and excludes dev-only packages", () => {
  const source = "a".repeat(40);
  const sbom = buildSbom(metadataFixture(), source);
  assert.deepEqual(sbom.components.map(({ name }) => name).sort(), ["labcolors-core", "labcolors-transport-cli", "serde"]);
  assert.equal(sbom.metadata.properties.find(({ name }) => name === "labpics:source-commit").value, source);
  assert.match(sbom.serialNumber, /^urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u);
  assert.equal(sbom.serialNumber, "urn:uuid:ef741685-4555-54bc-b355-6037cba081ec");
  assert.equal(buildSbom(metadataFixture(), source).serialNumber, sbom.serialNumber);
  assert.notEqual(buildSbom(metadataFixture(), "b".repeat(40)).serialNumber, sbom.serialNumber);
  assert.ok(sbom.components.every((component) => component.licenses?.length === 1));
});

test("verifier rejects tampered source identity before trusting digest metadata", async () => {
  const good = "a".repeat(40);
  const dir = await mkdtemp(join(tmpdir(), "labcolors-transport-dist-test-"));
  try {
    await writeFile(join(dir, "labcolors-transport"), "binary");
    await writeFile(join(dir, "transport.sbom.cdx.json"), JSON.stringify(buildSbom(metadataFixture(), good)));
    await writeFile(join(dir, "transport.licenses.json"), JSON.stringify(licenseInventory(buildSbom(metadataFixture(), good))));
    await writeFile(join(dir, "NOTICES.txt"), "Fixture license notice\n");
    await writeFile(join(dir, "transport.benchmark.json"), JSON.stringify({ schemaVersion: 1, workload: { measuredRounds: 31 }, candidate: { parse: { n: 31, minMs: 1, medianMs: 1, p95Ms: 1 }, inspect: { n: 31, minMs: 1, medianMs: 1, p95Ms: 1 }, serialize: { n: 31, minMs: 1, medianMs: 1, p95Ms: 1 } } }));
    const crypto = await import("node:crypto");
    const rec = async (path) => {
      const bytes = await readFile(join(dir, path));
      return { path, bytes: bytes.length, sha256: crypto.createHash("sha256").update(bytes).digest("hex") };
    };
    const evidence = {
      binary: await rec("labcolors-transport"),
      sbom: await rec("transport.sbom.cdx.json"),
      licenses: await rec("transport.licenses.json"),
      notices: await rec("NOTICES.txt"),
      benchmark: await rec("transport.benchmark.json"),
    };
    const statement = {
      _type: "https://in-toto.io/Statement/v1",
      subject: [{ name: "labcolors-transport", digest: { sha256: evidence.binary.sha256 } }],
      predicateType: "https://lab.pics/attestations/transport-distribution/v1",
      predicate: {
        schemaVersion: 1,
        source: { repository: "https://github.com/Labpics-Team/lab-colors", commit: good, tree: "b".repeat(40) },
        build: { noRebuild: true, target: "x86_64-unknown-linux-gnu" },
        evidence,
      },
    };
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(statement));
    await assert.rejects(() => verifyBundle(dir, "c".repeat(40)), /source identity mismatch/);
    await verifyBundle(dir, good);
    const originalLicenses = await readFile(join(dir, "transport.licenses.json"));
    for (const mutate of [
      (inventory) => { inventory.components[0].name = "incorrect-name"; },
      (inventory) => { inventory.components[0].licenseExpressions = ["incorrect-license"]; },
      (inventory) => { inventory.components[0] = {}; },
      (inventory) => { inventory.components[0] = inventory.components[1]; },
    ]) {
      const inventory = JSON.parse(originalLicenses);
      mutate(inventory);
      await writeFile(join(dir, "transport.licenses.json"), JSON.stringify(inventory));
      const alteredStatement = structuredClone(statement);
      alteredStatement.predicate.evidence.licenses = await rec("transport.licenses.json");
      await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(alteredStatement));
      await assert.rejects(() => verifyBundle(dir, good), /license inventory differs from the SBOM/);
    }
    await writeFile(join(dir, "transport.licenses.json"), originalLicenses);
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(statement));
    await verifyBundle(dir, good);


    const originalSbom = await readFile(join(dir, "transport.sbom.cdx.json"));
    const serialMutant = JSON.parse(originalSbom);
    serialMutant.serialNumber = "urn:uuid:00000000-0000-5000-8000-000000000000";
    await writeFile(join(dir, "transport.sbom.cdx.json"), JSON.stringify(serialMutant));
    const serialStatement = structuredClone(statement);
    serialStatement.predicate.evidence.sbom = await rec("transport.sbom.cdx.json");
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(serialStatement));
    await assert.rejects(() => verifyBundle(dir, good), /not attestable CycloneDX 1\.6/);
    await writeFile(join(dir, "transport.sbom.cdx.json"), originalSbom);
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(statement));

    await rm(join(dir, "transport.intoto.json"));
    await writeFile(join(dir, "outside-attestation.json"), JSON.stringify(statement));
    await symlink(join(dir, "outside-attestation.json"), join(dir, "transport.intoto.json"));
    await assert.rejects(() => verifyBundle(dir, good), /transport\.intoto\.json must be a regular file/);
    await rm(join(dir, "transport.intoto.json"));
    await rm(join(dir, "outside-attestation.json"));
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(statement));
    const tamperCases = [
      ["labcolors-transport", /binary evidence bytes changed/],
      ["transport.sbom.cdx.json", /sbom evidence bytes changed/],
      ["transport.licenses.json", /licenses evidence bytes changed/],
      ["NOTICES.txt", /notices evidence bytes changed/],
      ["transport.benchmark.json", /benchmark evidence bytes changed/],
    ];
    for (const [path, expected] of tamperCases) {
      const original = await readFile(join(dir, path));
      await writeFile(join(dir, path), Buffer.concat([original, Buffer.from("tamper")]));
      await assert.rejects(() => verifyBundle(dir, good), expected);
      await writeFile(join(dir, path), original);
    }

    const traversalMutant = structuredClone(statement);
    traversalMutant.predicate.evidence.sbom.path = "../transport.sbom.cdx.json";
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(traversalMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical sbom evidence path/);

    const absoluteMutant = structuredClone(statement);
    absoluteMutant.predicate.evidence.benchmark.path = "/tmp/transport.benchmark.json";
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(absoluteMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical benchmark evidence path/);

    const renamedMutant = structuredClone(statement);
    renamedMutant.predicate.evidence.licenses.path = "licenses.json";
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(renamedMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical licenses evidence path/);

    await writeFile(join(dir, "unexpected.txt"), "unexpected");
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(statement));
    await assert.rejects(() => verifyBundle(dir, good), /canonical closed file set/);
    await rm(join(dir, "unexpected.txt"));

    const originalBinary = await readFile(join(dir, "labcolors-transport"));
    await rm(join(dir, "labcolors-transport"));
    await writeFile(join(dir, "outside-binary"), originalBinary);
    await symlink(join(dir, "outside-binary"), join(dir, "labcolors-transport"));
    await assert.rejects(() => verifyBundle(dir, good), /canonical closed file set|regular file/);
    await rm(join(dir, "labcolors-transport"));
    await rm(join(dir, "outside-binary"));
    await writeFile(join(dir, "labcolors-transport"), originalBinary);

    const subjectMutant = structuredClone(statement);
    subjectMutant.subject[0].digest.sha256 = "0".repeat(64);
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(subjectMutant));
    await assert.rejects(() => verifyBundle(dir, good), /subject does not bind/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});


test("license inventory preserves identities and rejects same-size substitutions", () => {
  const sbom = buildSbom(metadataFixture(), "a".repeat(40));
  const inventory = licenseInventory(sbom);
  verifyLicenseInventory(inventory, sbom);
  for (const mutate of [
    (value) => { value.components[0].ref = "different"; },
    (value) => { value.components[0].version = "99.0.0"; },
    (value) => { value.components[0].licenseExpressions = []; },
    (value) => { value.components[0] = value.components[1]; },
    (value) => { value.components.reverse(); },
    (value) => { value.schemaVersion = 2; },
  ]) {
    const candidate = structuredClone(inventory);
    mutate(candidate);
    assert.throws(() => verifyLicenseInventory(candidate, sbom), /differs from the SBOM/u);
    verifyLicenseInventory(inventory, sbom);
  }
  for (const mutate of [
    (value) => { value.components = []; },
    (value) => { value.components[0] = value.components[1]; },
    (value) => { value.components[0].licenses = [{ expression: "" }]; },
    (value) => { value.components[0].licenses = [{ expression: "MIT" }, { expression: "MIT" }]; },
  ]) {
    const candidate = structuredClone(sbom);
    mutate(candidate);
    assert.throws(() => licenseInventory(candidate), /license inventory/u);
  }
});

test("native notices contain actual source texts and never follow a missing or outside license", async () => {
  const directory = await mkdtemp(join(tmpdir(), "labcolors-native-notices-"));
  try {
    const workspace = join(directory, "source");
    const registry = join(directory, "registry", "serde");
    await mkdir(join(workspace, "crates", "app"), { recursive: true });
    await mkdir(join(workspace, "crates", "core", "LICENSES"), { recursive: true });
    await mkdir(registry, { recursive: true });
    await writeFile(join(workspace, "Cargo.toml"), '[workspace.package]\nlicense = "MIT"\n');
    await writeFile(join(workspace, "LICENSE"), "Workspace copyright and MIT terms\n");
    await writeFile(join(workspace, "crates", "app", "Cargo.toml"), '[package]\nlicense = "MIT"\n');
    await writeFile(join(workspace, "crates", "core", "Cargo.toml"), '[package]\nlicense = "Apache-2.0"\n');
    await writeFile(join(workspace, "crates", "core", "LICENSES", "LICENSE-Apache.txt"), "Local package Apache terms\n");
    await writeFile(join(registry, "Cargo.toml"), '[package]\nlicense = "MIT OR Apache-2.0"\n');
    await writeFile(join(registry, "LICENSE-MIT"), "Registry copyright and MIT terms\n");
    await writeFile(join(registry, "NOTICE"), "Registry attribution notice\n");
    const metadata = metadataFixture();
    metadata.workspace_members = ["root", "core"];
    metadata.packages[0].manifest_path = join(workspace, "crates", "app", "Cargo.toml");
    metadata.packages[1].name = "local-engine";
    metadata.packages[1].license = "Apache-2.0";
    metadata.packages[1].manifest_path = join(workspace, "crates", "core", "Cargo.toml");
    metadata.packages[2].manifest_path = join(registry, "Cargo.toml");
    const sbom = buildSbom(metadata, "a".repeat(40));
    const collect = () => distributionNotices(metadata, sbom, workspace);
    const notices = await collect();
    for (const value of ["Workspace copyright", "Local package Apache terms", "Registry copyright", "Registry attribution notice"]) {
      assert.ok(notices.includes(value), value);
    }
    assert.equal(notices.includes(directory), false);
    assert.equal(notices.includes("dev-only"), false);
    assert.equal(await collect(), notices);
    metadata.packages[2].license_file = "../outside.txt";
    await assert.rejects(collect(), /declared license is outside its package/u);
    metadata.packages[2].license_file = "missing.txt";
    await assert.rejects(collect(), { code: "ENOENT" });
    delete metadata.packages[2].license_file;
    await writeFile(join(registry, "LICENSE-MIT"), "");
    await assert.rejects(collect(), /license notice is empty/u);
    await rm(join(registry, "LICENSE-MIT"));
    await rm(join(registry, "NOTICE"));
    await assert.rejects(collect(), /has no license notices/u);
    await writeFile(join(directory, "outside.txt"), "Outside source\n");
    await symlink(join(directory, "outside.txt"), join(registry, "LICENSE"));
    await assert.rejects(collect(), /regular file inside its package/u);
    await rm(join(registry, "LICENSE"));
    await writeFile(join(registry, "LICENSE"), "Recovered source license\n");
    assert.ok((await collect()).includes("Recovered source license"));
    metadata.packages.push({ ...metadata.packages[2] });
    await assert.rejects(collect(), /duplicate Cargo package identity/u);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
