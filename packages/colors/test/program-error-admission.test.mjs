import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

const errorWith = (code, operation) => Object.assign(new Error("candidate"), { code, operation });
const compileCodes = ["program_wire", "program_compile", "program_family_artifacts_required", "program_instantiate"];
const attachmentCodes = [
  "program_attachment_binding",
  "program_attachment_instantiate",
  "program_attachment_sink_admission",
  "program_attachment_resource_exhausted",
  "program_attachment_non_terminal_target",
];
const attachmentUpdateCodes = [
  "program_attachment_update",
  "program_attachment_resource_exhausted",
  "program_attachment_internal_invariant",
  "program_attachment_already_disposed",
  "program_attachment_patch_scope_mismatch",
  "program_attachment_stamp_mismatch",
  "program_attachment_revision_mismatch",
  "program_attachment_already_installed",
  "program_attachment_host_rejected",
  "program_attachment_host_protocol",
  "program_attachment_busy",
];
const attachmentDisposeCodes = [
  "program_attachment_already_disposed",
  "program_attachment_revoke_unconfirmed",
  "program_attachment_dispose",
  "program_attachment_busy",
];
const materializationCodes = [
  "program_materialization_not_ready",
  "program_materialization_paint_not_authority",
  "program_materialization_stale_revision",
  "program_materialization_stale_identity",
  "program_materialization_stale_sink_stamp",
  "program_materialization_foreign_binding_epoch",
  "program_materialization_terminal_binding_mismatch",
  "program_materialization_missing_point_absence_proof",
  "program_materialization_ambiguous_observation_cases",
  "program_attachment_busy",
];
const physicalIdentityCodes = ["program_attachment_unsupported_physical_identity"];
const operations = ["compileProgramWire", "updateObserved", "updateUnknown", "other", undefined];
const attachmentOperations = [
  "attachProgramWire",
  "attachmentUpdateObserved",
  "attachmentUpdateUnknown",
  "attachmentDispose",
  "attachmentFree",
  "materializationAuthority",
  "physicalIdentity",
  "other",
  undefined,
];

test("error admission recognizes exactly the operation/code relation", () => {
  for (const code of [
    ...compileCodes,
    "program_update",
    ...attachmentCodes,
    ...attachmentUpdateCodes,
    ...attachmentDisposeCodes,
    ...materializationCodes,
    ...physicalIdentityCodes,
    "program_attachment_update_unknown",
    "other",
    undefined,
    0,
    {},
  ]) {
    for (const operation of operations) {
      const expected = operation === "compileProgramWire" ? compileCodes.includes(code)
        : (operation === "updateObserved" || operation === "updateUnknown") && code === "program_update";
      assert.equal(isProgramError(errorWith(code, operation)), expected);
    }
    for (const operation of attachmentOperations) {
      const expected = operation === "attachProgramWire" ? attachmentCodes.includes(code)
        : (operation === "attachmentUpdateObserved" || operation === "attachmentUpdateUnknown")
          ? attachmentUpdateCodes.includes(code)
            : operation === "attachmentDispose" ? attachmentDisposeCodes.includes(code)
              : operation === "attachmentFree" ? code === "program_attachment_busy"
              : operation === "materializationAuthority" ? materializationCodes.includes(code)
              : operation === "physicalIdentity" ? physicalIdentityCodes.includes(code)
              : false;
      assert.equal(isProgramError(errorWith(code, operation)), expected);
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
  for (const operation of operations.slice(0, 3)) {
    const code = operation === "compileProgramWire" ? "program_wire" : "program_update";
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

test("real WASM errors stay identifiable and a rejected update stays retryable", async () => {
  await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
  assert.throws(() => compileProgramWire(new Uint8Array(), 1), (error) =>
    isProgramError(error) && error.code === "program_wire" && error.operation === "compileProgramWire");
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
