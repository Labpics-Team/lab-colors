import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const verifierUrl = new URL("../../../scripts/verify-package-release.mjs", import.meta.url);

function releaseVerifierSource() {
  return readFileSync(verifierUrl, "utf8");
}

function cleanConsumerBody(source) {
  const start = source.indexOf("async function verifyCleanConsumer(");
  const end = source.indexOf(
    "// Execute the same packed-package runtime smoke under the caller's Node binary.",
    start,
  );
  assert.notEqual(start, -1, "clean release consumer owner is absent");
  assert.notEqual(end, -1, "clean release consumer boundary is unterminated");
  return source.slice(start, end);
}

function functionBody(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.notEqual(start, -1, `${startMarker} owner is absent`);
  assert.notEqual(end, -1, `${startMarker} boundary is unterminated`);
  return source.slice(start, end);
}

test("release verification owns exactly one clean consumer and invokes it", () => {
  const source = releaseVerifierSource();
  assert.equal(source.match(/async function verifyCleanConsumer\(/gu)?.length, 1);
  assert.equal(source.match(/await verifyCleanConsumer\(/gu)?.length, 1);

  const body = cleanConsumerBody(source);
  for (const contract of [
    "runtimeSmokeSource()",
    "typeSmokeSource()",
    "clean-installed package",
    "clean-installed build metadata differs from the verified release inputs",
  ]) {
    assert.match(body, new RegExp(contract.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  }
});

test("clean consumer receives bytes only from an inspected canonical pack", () => {
  const source = releaseVerifierSource();
  const packBody = functionBody(
    source,
    "export async function packInto(destination, packageJson)",
    "export function runtimeSmokeSource()",
  );
  assert.match(
    packBody,
    /const inspected = await inspectNpmTarball\(path, expected, packResult\);/u,
  );
  assert.match(
    packBody,
    /return \{ path, tarballName, expected, packResult, \.\.\.inspected \};/u,
  );

  const releaseBody = functionBody(
    source,
    "export async function verifyPackageRelease()",
    "async function writeGithubOutputs(",
  );
  const pack = releaseBody.indexOf("const canonicalPack = await packInto(RELEASE_DIR, packageJson);");
  const consumer = releaseBody.indexOf("await verifyCleanConsumer(\n    canonicalPack.bytes,");
  const snapshot = releaseBody.indexOf("const verifiedTarball = await materializeVerifiedTarballSnapshot(canonicalPack);");
  assert.notEqual(pack, -1);
  assert.notEqual(consumer, -1);
  assert.notEqual(snapshot, -1);
  assert.ok(pack < consumer, "clean consumer ran before the canonical pack was produced and inspected");
  assert.ok(consumer < snapshot, "verified tarball snapshot was materialized before clean-consumer admission");
});
