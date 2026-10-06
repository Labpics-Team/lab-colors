import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import init, * as api from "../index.js";
import * as generated from "../pkg/labcolors.js";
import { referenceDecode, referencePacket } from "./helpers/certificate-wire-reference.mjs";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const git = (...args) => execFileSync("git", args, {
  cwd: root,
  encoding: "utf8",
  env: Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("GIT_"))),
}).trim();

// Ожидаемый кортеж строится из Git-объекта, без чтения выдаваемой producer-ом идентичности.
function independentSourceEnvelope() {
  assert.equal(git("status", "--porcelain", "--untracked-files=all", "--", "crates/labcolors-core"), "",
    "real source producer proof requires a clean committed Core tree");
  const tree = git("rev-parse", "HEAD:crates/labcolors-core");
  assert.match(tree, /^[0-9a-f]{40}$/u);
  assert.equal(git("cat-file", "-t", tree), "tree");
  const descriptor = Buffer.concat([Buffer.from("LCST", "ascii"), Buffer.from([0, 1, 1]), Buffer.from(tree, "ascii")]);
  assert.equal(descriptor.length, 47);
  const identity = createHash("sha256")
    .update(Buffer.from("labpics.colors/core-source-tree-descriptor/v1\0", "ascii"))
    .update(descriptor).digest();
  const runtime = `labcolors-core:source-tree-v1:${identity.toString("hex")}`;
  const context = "core-source-tree-transport-v1";
  assert.equal(Buffer.byteLength(runtime), 94);
  assert.equal(Buffer.byteLength(context), 29);
  const { bytes } = referencePacket({ runtime, revision: tree, identity, context, payload: Buffer.from([1]) });
  assert.equal(bytes.length, 283);
  return bytes;
}

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

test("real WASM source issuance matches the independently selected Git tree and exact wire", () => {
  const expected = independentSourceEnvelope();
  const bytes = api.issueSourceCertificateEnvelope();
  assert(bytes instanceof Uint8Array);
  assert.deepEqual(Buffer.from(bytes), expected);
  const decoded = api.decodeCertificateEnvelope(bytes);
  assert.deepEqual({
    ...decoded,
    producerContentIdentity: [...decoded.producerContentIdentity],
    payloadSha256: [...decoded.payloadSha256],
    bindingSha256: [...decoded.bindingSha256],
  }, referenceDecode(expected));
  assert.equal(Object.isFrozen(decoded), true);
  for (const field of ["admit", "attestation", "payload"]) assert.equal(field in decoded, false);
  bytes.fill(0);
  assert.deepEqual(Buffer.from(api.issueSourceCertificateEnvelope()), expected,
    "caller mutation must not change the next producer snapshot");
});

test("real WASM issuance accepts no caller identity, payload or capability constructor", () => {
  const expected = independentSourceEnvelope();
  const poisoned = new Proxy({}, { get() { throw new Error("Caller identity was read"); } });
  for (const surface of [api, generated]) {
    assert.equal(surface.issueSourceCertificateEnvelope.length, 0);
    assert.deepEqual(Buffer.from(surface.issueSourceCertificateEnvelope(poisoned, new Uint8Array([255]))), expected);
    for (const name of ["admit", "TrustedProducerAttestationV1", "SourceCertificateV1", "issueFromTrustedProducer"]) {
      assert.equal(name in surface, false);
    }
  }
});
