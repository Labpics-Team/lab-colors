import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileAttachedProgramWire, compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

const errorWith = (code, operation) => Object.assign(new Error("candidate"), { code, operation });

const acceptedPairs = new Set([
  ...["program_wire", "program_compile", "program_family_artifacts_required", "program_instantiate"]
    .map((code) => `${code}\0compileProgramWire`),
  "program_update\0updateObserved",
  "program_update\0updateUnknown",
  ...["attached_program_wire", "attached_program_compile", "attached_root_consumed_downstream", "attached_family_artifacts_required"]
    .map((code) => `${code}\0compileAttachedProgramWire`),
  ...["attached_resource_exhausted", "attached_instantiate", "attached_invalid_bindings", "attached_scope_changed", "attached_epoch_exhausted", "attached_attach"]
    .map((code) => `${code}\0attachAttachedProgram`),
  ...[
    "attached_resource_exhausted", "attached_update", "attached_patch_scope_mismatch",
    "attached_stamp_mismatch", "attached_revision_mismatch", "attached_already_installed",
    "attached_host", "attached_sink", "attached_internal_invariant", "attached_non_terminal_root",
    "attached_root_consumed_downstream", "attached_published_revision_mismatch",
    "attached_program_identity_mismatch", "attached_foreign_owner_generation",
    "attached_foreign_binding_epoch", "attached_missing_exact_point_absence_proof",
    "attached_empty_final_owned_domain", "attached_authority",
  ].flatMap((code) => [
    `${code}\0updateAttachedObserved`,
    `${code}\0updateAttachedUnknown`,
  ]),
  ...[
    "attached_non_terminal_root", "attached_root_consumed_downstream",
    "attached_published_revision_mismatch", "attached_program_identity_mismatch",
    "attached_foreign_owner_generation", "attached_foreign_binding_epoch",
    "attached_missing_exact_point_absence_proof", "attached_empty_final_owned_domain",
    "attached_resource_exhausted", "attached_authority",
  ].map((code) => `${code}\0validateAttachedAuthority`),
  "attached_authority_unavailable\0takeAttachedAuthority",
]);
const allCodes = [...new Set([...acceptedPairs].map((pair) => pair.split("\0")[0]))];
const allOperations = [...new Set([...acceptedPairs].map((pair) => pair.split("\0")[1]))];

test("error admission recognizes exactly the operation/code relation", () => {
  for (const code of [...allCodes, "other", undefined, 0, {}]) {
    for (const operation of [...allOperations, "other", undefined]) {
      const expected = typeof code === "string" && typeof operation === "string"
        && acceptedPairs.has(`${code}\0${operation}`);
      assert.equal(isProgramError(errorWith(code, operation)), expected, `${String(code)} @ ${String(operation)}`);
    }
  }
  for (const value of [null, undefined, false, 0, "error", Symbol("error"), 1n,
    {}, { code: "program_update", operation: "updateObserved" }, new Error("ordinary")]) {
    assert.equal(isProgramError(value), false);
  }
});

test("unreadable error identity refuses without replacing the caught exception", () => {
  const pair = Proxy.revocable(new Error("original"), {});
  pair.revoke();
  const prototypeTrap = new Proxy(new Error("original"), { getPrototypeOf() { throw new Error("trap"); } });
  for (const error of [pair.proxy, prototypeTrap]) {
    let result;
    assert.doesNotThrow(() => { result = isProgramError(error); });
    assert.equal(result, false);
  }
});

for (const property of ["operation", "code"]) {
  test(`a throwing ${property} getter is not a classifier failure`, () => {
    const original = errorWith("program_update", "updateObserved");
    let reads = 0;
    Object.defineProperty(original, property, { get() { reads += 1; throw new Error("unreadable"); } });
    let result;
    assert.doesNotThrow(() => { result = isProgramError(original); });
    assert.equal(result, false);
    assert.equal(reads, 1);
  });
}

test("operation is observed once, so an unstable getter cannot splice branches", () => {
  let reads = 0;
  const error = errorWith("program_update", "updateObserved");
  Object.defineProperty(error, "operation", { get() {
    reads += 1;
    return reads === 1 ? "not-a-program-operation" : "updateObserved";
  } });
  assert.equal(isProgramError(error), false);
  assert.equal(reads, 1);
});

test("readable accessor and inherited contracts remain valid", () => {
  for (const [code, operation] of [
    ["program_wire", "compileProgramWire"],
    ["program_update", "updateObserved"],
    ["program_update", "updateUnknown"],
    ["attached_program_wire", "compileAttachedProgramWire"],
    ["attached_host", "updateAttachedObserved"],
    ["attached_foreign_binding_epoch", "validateAttachedAuthority"],
    ["attached_authority_unavailable", "takeAttachedAuthority"],
  ]) {
    const counts = { operation: 0, code: 0 };
    const prototype = Object.create(Error.prototype, {
      operation: { get() { counts.operation += 1; return operation; } },
      code: { get() { counts.code += 1; return code; } },
    });
    const error = Object.create(prototype);
    assert.equal(isProgramError(error), true);
    assert.deepEqual(counts, { operation: 1, code: 1 });
  }
});

test("classification never converts field values", () => {
  let conversions = 0;
  const hostile = { [Symbol.toPrimitive]() { conversions += 1; throw new Error("coercion"); } };
  assert.equal(isProgramError(errorWith(hostile, "compileProgramWire")), false);
  assert.equal(isProgramError(errorWith("program_update", hostile)), false);
  assert.equal(conversions, 0);
});

test("real WASM detached and attached errors remain distinguishable", async () => {
  await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
  assert.throws(() => compileProgramWire(new Uint8Array(), 1), (error) =>
    isProgramError(error) && error.code === "program_wire" && error.operation === "compileProgramWire");
  assert.throws(() => compileAttachedProgramWire(new Uint8Array()), (error) =>
    isProgramError(error) && error.code === "attached_program_wire"
      && error.operation === "compileAttachedProgramWire");

  const wire = new ProgramWireBuilderV1().source(11, [20, 20, 20]).fixedTarget(21, 11)
    .surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
    .sourceOverOccurrence(61, 41, 51, 64, 0.2, 1).presentationRoot(71, 61)
    .presentationTarget(71, 61).wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();
  const runtime = compileProgramWire(wire, 1);
  let snapshot;
  try {
    assert.throws(() => runtime.updateObserved(1n, new Uint32Array(), new Uint8Array(), 1), (error) =>
      isProgramError(error) && error.code === "program_update" && error.operation === "updateObserved");
    snapshot = runtime.updateObserved(1n, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1);
    assert.equal(snapshot.state, "ready");
    assert.deepEqual([...snapshot.outputRgb(0)], [20, 20, 20]);
  } finally {
    snapshot?.free();
    runtime.free();
  }
});
