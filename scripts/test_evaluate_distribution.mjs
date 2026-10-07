import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildSbom, verifyBundle } from "./evaluate-distribution.mjs";
import { licenseInventory } from "./distribution-licenses.mjs";

function metadataFixture() {
  const root = { id: "root", name: "labcolors-evaluate-cli", version: "1.0.0", source: null, license: "MIT" };
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

function benchmarkFixture() {
  const summary = { n: 31, minMs: 1, medianMs: 1, p95Ms: 1 };
  return {
    schemaVersion: 1,
    workload: { id: "evaluate-declared-point-cli-process-v1", measuredRounds: 31 },
    correctness: { goodTerminalSrgb8: [128, 128, 128], rejectedExit: 4 },
    candidate: { "stdin-json": summary, "file-json": summary, "stdin-jsonl": summary },
  };
}

test("SBOM follows normal/build closure and excludes dev-only packages", () => {
  const source = "a".repeat(40);
  const sbom = buildSbom(metadataFixture(), source);
  assert.deepEqual(sbom.components.map(({ name }) => name).sort(), ["labcolors-core", "labcolors-evaluate-cli", "serde"]);
  assert.equal(sbom.metadata.properties.find(({ name }) => name === "labpics:source-commit").value, source);
  assert.equal(sbom.metadata.properties.find(({ name }) => name === "labpics:evidence-kind").value, "evaluate-distribution");
  assert.match(sbom.serialNumber, /^urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u);
  assert.equal(sbom.serialNumber, "urn:uuid:882ff767-1871-5774-981e-ed1b54f8588b");
  assert.equal(buildSbom(metadataFixture(), source).serialNumber, sbom.serialNumber);
  assert.notEqual(buildSbom(metadataFixture(), "b".repeat(40)).serialNumber, sbom.serialNumber);
  assert.ok(sbom.components.every((component) => component.licenses?.length === 1));
});

test("verifier rejects tampered source identity before trusting digest metadata", async () => {
  const good = "a".repeat(40);
  const dir = await mkdtemp(join(tmpdir(), "labcolors-evaluate-dist-test-"));
  try {
    await writeFile(join(dir, "labcolors-evaluate"), "binary");
    await writeFile(join(dir, "evaluate.sbom.cdx.json"), JSON.stringify(buildSbom(metadataFixture(), good)));
    await writeFile(join(dir, "evaluate.licenses.json"), JSON.stringify(licenseInventory(buildSbom(metadataFixture(), good))));
    await writeFile(join(dir, "NOTICES.txt"), "Fixture license notice\n");
    await writeFile(join(dir, "evaluate.benchmark.json"), JSON.stringify(benchmarkFixture()));
    const crypto = await import("node:crypto");
    const rec = async (path) => {
      const bytes = await readFile(join(dir, path));
      return { path, bytes: bytes.length, sha256: crypto.createHash("sha256").update(bytes).digest("hex") };
    };
    const evidence = {
      binary: await rec("labcolors-evaluate"),
      sbom: await rec("evaluate.sbom.cdx.json"),
      licenses: await rec("evaluate.licenses.json"),
      notices: await rec("NOTICES.txt"),
      benchmark: await rec("evaluate.benchmark.json"),
    };
    const statement = {
      _type: "https://in-toto.io/Statement/v1",
      subject: [{ name: "labcolors-evaluate", digest: { sha256: evidence.binary.sha256 } }],
      predicateType: "https://lab.pics/attestations/evaluate-distribution/v1",
      predicate: {
        schemaVersion: 1,
        source: { repository: "https://github.com/Labpics-Team/lab-colors", commit: good, tree: "b".repeat(40) },
        build: { noRebuild: true, target: "x86_64-unknown-linux-gnu" },
        evidence,
      },
    };
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));
    await assert.rejects(() => verifyBundle(dir, "c".repeat(40)), /source identity mismatch/);
    await verifyBundle(dir, good);
    const originalLicenses = await readFile(join(dir, "evaluate.licenses.json"));
    for (const mutate of [
      (inventory) => { inventory.components[0].name = "incorrect-name"; },
      (inventory) => { inventory.components[0].licenseExpressions = ["incorrect-license"]; },
      (inventory) => { inventory.components[0] = {}; },
      (inventory) => { inventory.components[0] = inventory.components[1]; },
    ]) {
      const inventory = JSON.parse(originalLicenses);
      mutate(inventory);
      await writeFile(join(dir, "evaluate.licenses.json"), JSON.stringify(inventory));
      const alteredStatement = structuredClone(statement);
      alteredStatement.predicate.evidence.licenses = await rec("evaluate.licenses.json");
      await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(alteredStatement));
      await assert.rejects(() => verifyBundle(dir, good), /license inventory differs from the SBOM/);
    }
    await writeFile(join(dir, "evaluate.licenses.json"), originalLicenses);
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));
    await verifyBundle(dir, good);


    const originalSbom = await readFile(join(dir, "evaluate.sbom.cdx.json"));
    const serialMutant = JSON.parse(originalSbom);
    serialMutant.serialNumber = "urn:uuid:00000000-0000-5000-8000-000000000000";
    await writeFile(join(dir, "evaluate.sbom.cdx.json"), JSON.stringify(serialMutant));
    const serialStatement = structuredClone(statement);
    serialStatement.predicate.evidence.sbom = await rec("evaluate.sbom.cdx.json");
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(serialStatement));
    await assert.rejects(() => verifyBundle(dir, good), /not attestable CycloneDX 1\.6/);
    await writeFile(join(dir, "evaluate.sbom.cdx.json"), originalSbom);
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));

    const workloadMutant = JSON.parse(await readFile(join(dir, "evaluate.benchmark.json")));
    workloadMutant.workload.id = "certificate-envelope-reference-vector-cli-process-v1";
    await writeFile(join(dir, "evaluate.benchmark.json"), JSON.stringify(workloadMutant));
    const workloadStatement = structuredClone(statement);
    workloadStatement.predicate.evidence.benchmark = await rec("evaluate.benchmark.json");
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(workloadStatement));
    await assert.rejects(() => verifyBundle(dir, good), /not the declared-point process/);
    await writeFile(join(dir, "evaluate.benchmark.json"), JSON.stringify(benchmarkFixture()));
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));

    const correctnessMutant = JSON.parse(await readFile(join(dir, "evaluate.benchmark.json")));
    correctnessMutant.correctness.goodTerminalSrgb8 = [0, 0, 0];
    await writeFile(join(dir, "evaluate.benchmark.json"), JSON.stringify(correctnessMutant));
    const correctnessStatement = structuredClone(statement);
    correctnessStatement.predicate.evidence.benchmark = await rec("evaluate.benchmark.json");
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(correctnessStatement));
    await assert.rejects(() => verifyBundle(dir, good), /correctness is not the declared GOOD\/REJECTED pair/);
    await writeFile(join(dir, "evaluate.benchmark.json"), JSON.stringify(benchmarkFixture()));
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));

    await rm(join(dir, "evaluate.intoto.json"));
    await writeFile(join(dir, "outside-attestation.json"), JSON.stringify(statement));
    await symlink(join(dir, "outside-attestation.json"), join(dir, "evaluate.intoto.json"));
    await assert.rejects(() => verifyBundle(dir, good), /evaluate\.intoto\.json must be a regular file/);
    await rm(join(dir, "evaluate.intoto.json"));
    await rm(join(dir, "outside-attestation.json"));
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));
    const tamperCases = [
      ["labcolors-evaluate", /binary evidence bytes changed/],
      ["evaluate.sbom.cdx.json", /sbom evidence bytes changed/],
      ["evaluate.licenses.json", /licenses evidence bytes changed/],
      ["NOTICES.txt", /notices evidence bytes changed/],
      ["evaluate.benchmark.json", /benchmark evidence bytes changed/],
    ];
    for (const [path, expected] of tamperCases) {
      const original = await readFile(join(dir, path));
      await writeFile(join(dir, path), Buffer.concat([original, Buffer.from("tamper")]));
      await assert.rejects(() => verifyBundle(dir, good), expected);
      await writeFile(join(dir, path), original);
    }

    const traversalMutant = structuredClone(statement);
    traversalMutant.predicate.evidence.sbom.path = "../evaluate.sbom.cdx.json";
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(traversalMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical sbom evidence path/);

    const absoluteMutant = structuredClone(statement);
    absoluteMutant.predicate.evidence.benchmark.path = "/tmp/evaluate.benchmark.json";
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(absoluteMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical benchmark evidence path/);

    const renamedMutant = structuredClone(statement);
    renamedMutant.predicate.evidence.licenses.path = "licenses.json";
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(renamedMutant));
    await assert.rejects(() => verifyBundle(dir, good), /non-canonical licenses evidence path/);

    await writeFile(join(dir, "unexpected.txt"), "unexpected");
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(statement));
    await assert.rejects(() => verifyBundle(dir, good), /canonical closed file set/);
    await rm(join(dir, "unexpected.txt"));

    const originalBinary = await readFile(join(dir, "labcolors-evaluate"));
    await rm(join(dir, "labcolors-evaluate"));
    await writeFile(join(dir, "outside-binary"), originalBinary);
    await symlink(join(dir, "outside-binary"), join(dir, "labcolors-evaluate"));
    await assert.rejects(() => verifyBundle(dir, good), /canonical closed file set|regular file/);
    await rm(join(dir, "labcolors-evaluate"));
    await rm(join(dir, "outside-binary"));
    await writeFile(join(dir, "labcolors-evaluate"), originalBinary);

    const subjectMutant = structuredClone(statement);
    subjectMutant.subject[0].digest.sha256 = "0".repeat(64);
    await writeFile(join(dir, "evaluate.intoto.json"), JSON.stringify(subjectMutant));
    await assert.rejects(() => verifyBundle(dir, good), /subject does not bind/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
