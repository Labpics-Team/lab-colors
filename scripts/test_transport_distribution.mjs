import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildSbom, verifyBundle } from "./transport-distribution.mjs";

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
  assert.ok(sbom.components.every((component) => component.licenses?.length === 1));
});

test("verifier rejects tampered source identity before trusting digest metadata", async () => {
  const dir = await mkdtemp(join(tmpdir(), "labcolors-transport-dist-test-"));
  try {
    await writeFile(join(dir, "labcolors-transport"), "binary");
    await writeFile(join(dir, "transport.sbom.cdx.json"), JSON.stringify({ bomFormat: "CycloneDX", specVersion: "1.6", components: [{ type: "application", name: "labcolors-transport-cli", licenses: [{ expression: "MIT" }] }] }));
    await writeFile(join(dir, "transport.licenses.json"), JSON.stringify({ schemaVersion: 1, components: [{}] }));
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
      benchmark: await rec("transport.benchmark.json"),
    };
    const good = "a".repeat(40);
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
    const tamperCases = [
      ["labcolors-transport", /binary evidence bytes changed/],
      ["transport.sbom.cdx.json", /sbom evidence bytes changed/],
      ["transport.licenses.json", /licenses evidence bytes changed/],
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

    const subjectMutant = structuredClone(statement);
    subjectMutant.subject[0].digest.sha256 = "0".repeat(64);
    await writeFile(join(dir, "transport.intoto.json"), JSON.stringify(subjectMutant));
    await assert.rejects(() => verifyBundle(dir, good), /subject does not bind/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
