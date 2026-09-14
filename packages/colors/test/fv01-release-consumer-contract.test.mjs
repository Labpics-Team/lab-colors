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

test("clean consumer remains downstream of the exact verified tarball", () => {
  const source = releaseVerifierSource();
  const call = source.indexOf("await verifyCleanConsumer(");
  const tarballRead = source.indexOf("const tarballBytes = await readFile(tarballPath)");
  const tarballVerification = source.indexOf("await inspectNpmTarball(");
  assert.notEqual(call, -1);
  assert.notEqual(tarballRead, -1);
  assert.notEqual(tarballVerification, -1);
  assert.ok(tarballRead < call, "clean consumer ran before the packed bytes were bound");
  assert.ok(tarballVerification < call, "clean consumer ran before tarball inspection");
});
