import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("private Program second client shares the ABI and vectors without importing the first client", async () => {
  const [consumer, workerClient, proof, packageManifest] = await Promise.all([
    readFile(new URL("../private-program/consumer.js", import.meta.url), "utf8"),
    readFile(new URL("../../../fixtures/private-program-browser/worker-client.mjs", import.meta.url), "utf8"),
    readFile(new URL("../../../fixtures/private-program-browser/proof.mjs", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
  ]);

  assert.match(consumer, /from "\.\/abi-v2\.js"/u);
  assert.match(workerClient, /from "\/installed\/private-program\/abi-v2\.js"/u);
  assert.doesNotMatch(workerClient, /consumer\.js/u);
  assert.match(proof, /from "\.\/vectors\.mjs"/u);
  assert.match(workerClient, /from "\.\/vectors\.mjs"/u);
  assert.match(workerClient, /if \(response\.ok !== true\)/u);
  assert.match(proof, /worker\.addEventListener\("messageerror"/u);
  const manifest = JSON.parse(packageManifest);
  assert.equal(manifest.files.includes("private-program/abi-v2.js"), true);
  assert.equal(Object.hasOwn(manifest.exports, "./private-program/abi-v2.js"), false);
});

test("differential acceptance cannot pass without executing the independent worker", async () => {
  const proof = await readFile(
    new URL("../../../fixtures/private-program-browser/proof.mjs", import.meta.url),
    "utf8",
  );
  assert.match(proof, /const worker = new Worker\("\.\/worker-client\.mjs", \{ type: "module" \}\)/u);
  assert.match(proof, /equal\(workerResult\.initial, initialFingerprint/u);
  assert.match(proof, /equal\(workerResult\.updated, updatedFingerprint/u);
  assert.match(proof, /equal\(workerResult\.changed, changedFingerprint/u);
  assert.match(proof, /checks\.push\("independent-worker-client-parity"\)/u);
});
