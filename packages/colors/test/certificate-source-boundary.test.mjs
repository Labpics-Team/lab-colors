import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const binding = `
export const issuerCalls = [];
export let failure;
export function failWith(error) { failure = error; }
export default async function init() {}
export function initSync() {}
export function issueSourceCertificateEnvelope(...args) {
  issuerCalls.push(args);
  if (failure !== undefined) throw failure;
  return new Uint8Array([76, 67, 69, 78]);
}
export function decodeCertificateEnvelope() { throw new Error("Unexpected decoder call"); }
export function attachProgramWire() { throw new Error("Unexpected attachment call"); }
export function compileProgramWire() {}
export function evaluateWcag22() {}
export function numericalCapabilityManifest() {}
export class ProgramRuntime {}
export class ProgramSnapshot {}
export class ProgramAttachment {}
export class ProgramAttachedSnapshot {}
export class ProgramAttachedRender {}
export class AttachedMaterializationAuthority {}
`;

async function facade(t) {
  const directory = await mkdtemp(join(tmpdir(), "labcolors-source-projection-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, "pkg"));
  await Promise.all([
    writeFile(join(directory, "package.json"), '{"type":"module"}\n'),
    writeFile(join(directory, "index.js"), await readFile(new URL("../index.js", import.meta.url))),
    writeFile(join(directory, "pkg/labcolors.js"), binding),
  ]);
  return {
    api: await import(pathToFileURL(join(directory, "index.js")).href),
    generated: await import(pathToFileURL(join(directory, "pkg/labcolors.js")).href),
  };
}

// Подмена проверяет только JS-порт; wire проверяется отдельно на реальной WASM-сборке.
test("source issuance facade has no caller-owned identity or payload arguments", async (t) => {
  const { api, generated } = await facade(t);
  assert.equal(typeof api.issueSourceCertificateEnvelope, "function");
  assert.equal(api.issueSourceCertificateEnvelope.length, 0);
  const poisoned = new Proxy({}, { get() { throw new Error("Caller input was read"); } });
  for (const args of [[], [poisoned], [new Uint8Array([255]), "forged-runtime", "forged-context"]]) {
    const bytes = api.issueSourceCertificateEnvelope(...args);
    assert(bytes instanceof Uint8Array);
    assert.deepEqual([...bytes], [76, 67, 69, 78]);
    assert.deepEqual(generated.issuerCalls.at(-1), []);
    bytes[0] = 0;
  }
  assert.deepEqual([...api.issueSourceCertificateEnvelope()], [76, 67, 69, 78]);
  for (const name of ["admit", "TrustedProducerAttestationV1", "SourceCertificateV1", "issueFromTrustedProducer"]) {
    assert.equal(name in api, false);
  }
});

test("source issuance errors preserve the operation-specific closed vocabulary", async (t) => {
  const { api, generated } = await facade(t);
  assert.equal(typeof api.issueSourceCertificateEnvelope, "function");
  for (const code of ["certificate_producer_identity_unavailable", "certificate_resource_limit_exceeded"]) {
    const failure = Object.assign(new Error("Certificate envelope operation failed"), {
      code, operation: "issueSourceCertificateEnvelope",
    });
    generated.failWith(failure);
    assert.throws(() => api.issueSourceCertificateEnvelope(), (actual) => {
      assert.strictEqual(actual, failure);
      assert.equal(api.isCertificateError(actual), true);
      assert(Buffer.byteLength(actual.message, "utf8") <= 256);
      return true;
    });
  }
  for (const [operation, code] of [
    ["decodeCertificateEnvelope", "certificate_producer_identity_unavailable"],
    ["issueSourceCertificateEnvelope", "certificate_invalid_input"],
    ["issueSourceCertificateEnvelope", "certificate_future_error"],
    ["other", "certificate_producer_identity_unavailable"],
  ]) {
    assert.equal(api.isCertificateError(Object.assign(new Error("candidate"), { operation, code })), false);
  }
  const revoked = Proxy.revocable(new Error("candidate"), {});
  revoked.revoke();
  assert.equal(api.isCertificateError(revoked.proxy), false);
});
