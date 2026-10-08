import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

// Счётчик различает отказ snapshot allocation и вход в generated copier.
const binding = `
export let copierCalls = 0;
export default async function init() {}
export function initSync() {}
export function issueSourceCertificateEnvelope() {}
export function decodeCertificateEnvelope() { copierCalls += 1; }
export function compileProgramWire() { copierCalls += 1; }
export function attachProgramWire() { copierCalls += 1; return new ProgramAttachment(); }
export function evaluateWcag22() {}
export function numericalCapabilityManifest() {}
export class ProgramRuntime { updateObserved() { copierCalls += 1; } }
export class ProgramSnapshot {}
export class ProgramAttachment {
  updateObserved() { copierCalls += 1; }
  materializationAuthorityFor() { copierCalls += 1; }
  dispose() {}
  free() {}
}
export class ProgramAttachedSnapshot {}
export class ProgramAttachedRender {}
export class AttachedMaterializationAuthority {}
`;

test("snapshot allocation failures keep operation-specific codes before the copier", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "labcolors-storage-allocation-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, "pkg"));
  await Promise.all([
    writeFile(join(directory, "package.json"), '{"type":"module"}\n'),
    writeFile(join(directory, "index.js"), await readFile(new URL("../index.js", import.meta.url))),
    writeFile(join(directory, "pkg/labcolors.js"), binding),
  ]);

  const NativeBytes = Uint8Array;
  const NativeIds = Uint32Array;
  let refuse = false;
  let allocations = 0;
  const allocationTrap = { construct(Target, args) {
    allocations += 1;
    if (refuse) throw new RangeError("controlled allocation refusal");
    return Reflect.construct(Target, args);
  } };
  let api;
  try {
    // Facade захватывает конструкторы один раз; только они получают fault.
    globalThis.Uint8Array = new Proxy(NativeBytes, allocationTrap);
    globalThis.Uint32Array = new Proxy(NativeIds, allocationTrap);
    api = await import(pathToFileURL(join(directory, "index.js")).href);
  } finally {
    globalThis.Uint8Array = NativeBytes;
    globalThis.Uint32Array = NativeIds;
  }
  const generated = await import(pathToFileURL(join(directory, "pkg/labcolors.js")).href);
  const bytes = new NativeBytes(32);
  const ids = new NativeIds([1]);
  const runtime = new api.ProgramRuntime();
  const attachment = api.attachProgramWire(bytes, 1, 1, 1, 1, 1, () => true);
  const calls = generated.copierCalls;
  refuse = true;

  for (const [operation, code, invoke] of [
    ["compileProgramWire", "program_resource_exhausted", () => api.compileProgramWire(bytes, 1)],
    ["attachProgramWire", "program_resource_exhausted", () => api.attachProgramWire(bytes, 1, 1, 1, 1, 1, () => true)],
    ["updateObserved", "program_resource_exhausted", () => runtime.updateObserved(1n, ids, bytes, 1)],
    ["attachmentUpdateObserved", "program_attachment_resource_exhausted", () => attachment.updateObserved(1n, ids, bytes, 1)],
    ["materializationAuthority", "program_attachment_resource_exhausted", () => attachment.materializationAuthorityFor(1n, bytes, 1n)],
  ]) {
    assert.throws(invoke, (error) => api.isProgramError(error) && error.operation === operation && error.code === code);
    assert.equal(generated.copierCalls, calls);
  }
  const beforeInvalidLength = allocations;
  assert.throws(() => attachment.materializationAuthorityFor(1n, new NativeBytes(31), 1n), (error) =>
    api.isProgramError(error) && error.code === "program_materialization_stale_identity");
  assert.equal(allocations, beforeInvalidLength);
  assert.throws(() => api.decodeCertificateEnvelope(bytes), (error) =>
    api.isCertificateError(error) && error.code === "certificate_invalid_input");
  assert.equal(generated.copierCalls, calls);

  refuse = false;
  runtime.updateObserved(1n, ids, bytes, 1);
  attachment.updateObserved(1n, ids, bytes, 1);
  attachment.materializationAuthorityFor(1n, bytes, 1n);
  api.decodeCertificateEnvelope(bytes);
  assert.equal(generated.copierCalls, calls + 4);
  attachment.dispose(true);
  attachment.free();
});
