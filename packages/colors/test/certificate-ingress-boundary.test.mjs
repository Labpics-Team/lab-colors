import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { ENVELOPE_LIMIT, referencePacket } from "./helpers/certificate-wire-reference.mjs";

// Подменяется только импорт generated binding. Facade копируется byte-exact;
// счётчик фиксирует вход до места, где wasm-bindgen копирует argument в WASM.
// Подмена намеренно возвращает тот же resource error после входа: один лишь
// код ошибки не способен отличить ранний отказ от уже выполненного copy.
const instrumentedBinding = `
export const copierEntries = [];
export let lastInput;
export default async function init() {}
export function initSync() {}
export function issueSourceCertificateEnvelope() { throw new Error("Unexpected producer call in ingress test"); }
export function decodeCertificateEnvelope(bytes) {
  lastInput = bytes;
  copierEntries.push({ byteLength: bytes.byteLength, plain: Object.getPrototypeOf(bytes) === Uint8Array.prototype });
  if (bytes.byteLength > 2_097_152) {
    throw Object.assign(new Error("Certificate envelope operation failed"), {
      code: "certificate_resource_limit_exceeded", operation: "decodeCertificateEnvelope",
    });
  }
  return "generated-binding-entered";
}
export function attachProgramWire() { throw new Error("Unexpected attachment call in ingress test"); }
export function compileProgramWire() { throw new Error("Unexpected compiler call in ingress test"); }
export function evaluateWcag22() { throw new Error("Unexpected evaluator call in ingress test"); }
export function numericalCapabilityManifest() { throw new Error("Unexpected manifest call in ingress test"); }
export class ProgramRuntime {}
export class ProgramSnapshot {}
export class ProgramAttachment {}
export class ProgramAttachedSnapshot {}
export class ProgramAttachedRender {}
export class AttachedMaterializationAuthority {}
`;

test("certificate size refusal precedes entry into the generated WASM copier", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "labcolors-certificate-ingress-"));
  // Node сохраняет primary assertion и возможную отдельную ошибку after-hook.
  t.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, "pkg"));
  await Promise.all([
    writeFile(join(directory, "package.json"), '{"type":"module"}\n'),
    writeFile(join(directory, "index.js"), await readFile(new URL("../index.js", import.meta.url))),
    writeFile(join(directory, "pkg/labcolors.js"), instrumentedBinding),
  ]);
  const api = await import(pathToFileURL(join(directory, "index.js")).href);
  const generated = await import(pathToFileURL(join(directory, "pkg/labcolors.js")).href);
  const { copierEntries } = generated;
  await api.default();
  assert.equal(api.MAX_CERTIFICATE_ENVELOPE_BYTES, ENVELOPE_LIMIT);

  // Healthy control исключает vacuity: допустимый вызов доходит до того же порта.
  const healthy = new Uint8Array(referencePacket().bytes);
  assert.equal(api.decodeCertificateEnvelope(healthy), "generated-binding-entered");
  assert.deepEqual(copierEntries, [{ byteLength: healthy.byteLength, plain: true }]);

  const atCap = new Uint8Array(ENVELOPE_LIMIT);
  assert.equal(api.decodeCertificateEnvelope(atCap), "generated-binding-entered");
  assert.equal(copierEntries.at(-1).byteLength, ENVELOPE_LIMIT);

  const resizable = new ArrayBuffer(1, { maxByteLength: ENVELOPE_LIMIT + 1 });
  new Uint8Array(resizable).set([0x41]);
  let resizeReads = 0;
  class ChangesDuringValidation extends Uint8Array {
    get length() {
      resizeReads += 1;
      resizable.resize(ENVELOPE_LIMIT + 1);
      Object.setPrototypeOf(this, Uint8Array.prototype);
      return 1;
    }
  }
  assert.equal(api.decodeCertificateEnvelope(new ChangesDuringValidation(resizable)), "generated-binding-entered");
  assert.equal(resizeReads, 0);
  assert.equal(resizable.byteLength, 1);
  assert.deepEqual(copierEntries.at(-1), { byteLength: 1, plain: true });
  assert.deepEqual([...generated.lastInput], [0x41]);

  const shrinking = new ArrayBuffer(2, { maxByteLength: 2 });
  new Uint8Array(shrinking).set([0x42, 0x43]);
  let shrinkReads = 0;
  class ShrinksDuringValidation extends Uint8Array {
    get length() {
      shrinkReads += 1;
      shrinking.resize(1);
      return 2;
    }
  }
  assert.equal(api.decodeCertificateEnvelope(new ShrinksDuringValidation(shrinking)), "generated-binding-entered");
  assert.equal(shrinkReads, 0);
  assert.equal(shrinking.byteLength, 2);
  assert.deepEqual(copierEntries.at(-1), { byteLength: 2, plain: true });
  assert.deepEqual([...generated.lastInput], [0x42, 0x43]);

  for (const oversized of [
    new Uint8Array(ENVELOPE_LIMIT + 1),
    new (class extends Uint8Array {})(ENVELOPE_LIMIT + 1),
  ]) {
    const before = copierEntries.length;
    assert.throws(() => api.decodeCertificateEnvelope(oversized), (error) => {
      assert.equal(api.isCertificateError(error), true);
      assert.equal(error.code, "certificate_resource_limit_exceeded");
      return true;
    });
    assert.equal(copierEntries.length, before, "oversized input reached the generated WASM copier");
  }
});
