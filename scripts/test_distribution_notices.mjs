import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { distributionNotices } from "./distribution-licenses.mjs";

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
