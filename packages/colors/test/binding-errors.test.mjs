import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, evaluateWcag22, isBindingError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20]).fixedTarget(21, 11).surfaceInputPort(31)
  .solidPaint(41, 21).inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();

const refusal = (code, operation) => (error) => {
  assert.ok(error instanceof Error);
  assert.equal(isBindingError(error), true);
  assert.equal(error.code, code);
  assert.equal(error.operation, operation);
  return true;
};

test("WCAG refusals expose their operation and code without parsing prose", () => {
  const criterion = "sc-1.4.3-text-default";
  assert.equal(evaluateWcag22("#000000", "#FFFFFF", criterion).kind, "evaluated");
  for (const colors of [["#GGGGGG", "#FFFFFF"], ["#000000", "bad"]]) {
    assert.throws(() => evaluateWcag22(...colors, criterion), refusal("invalid_color", "evaluateWcag22"));
  }
  assert.throws(() => evaluateWcag22("#000000", "#FFFFFF", "bad"),
    refusal("unknown_wcag22_criterion", "evaluateWcag22"));
});

test("output index refusal preserves a readable snapshot", () => {
  const runtime = compileProgramWire(wire, 1);
  let snapshot;
  try {
    snapshot = runtime.updateObserved(1n, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1);
    assert.equal(snapshot.state, "ready");
    const expected = [...snapshot.outputRgb(0)];
    for (const method of ["outputSlot", "outputRgb", "outputOpacity"]) {
      for (const index of [-1, 0.5, 1, 2 ** 32, NaN, "0"]) {
        assert.throws(() => snapshot[method](index), refusal("invalid_output_index", "readProgramOutput"));
        assert.deepEqual([...snapshot.outputRgb(0)], expected);
      }
    }
  } finally {
    snapshot?.free();
    runtime.free();
  }
});


test("binding error classification observes one closed pair and never coerces", () => {
  const allowed = new Map([
    ["evaluateWcag22", ["invalid_color", "unknown_wcag22_criterion", "internal_error"]],
    ["numericalCapabilityManifest", ["internal_error"]],
    ["readProgramOutput", ["invalid_output_index"]],
  ]);
  const codes = [...new Set([...allowed.values()].flat()), "future", undefined, {}];
  for (const operation of [...allowed.keys(), "future", undefined]) {
    for (const code of codes) {
      const error = Object.assign(new Error("arbitrary prose"), { code, operation });
      assert.equal(isBindingError(error), allowed.get(operation)?.includes(code) ?? false);
    }
  }
  for (const value of [null, undefined, {}, "invalid_color", new Error("invalid_color")])
    assert.equal(isBindingError(value), false);
  for (const property of ["code", "operation"]) {
    let calls = 0;
    const error = Object.assign(new Error("original"), { code: "invalid_color", operation: "evaluateWcag22" });
    Object.defineProperty(error, property, { get() { calls += 1; throw new Error("inaccessible"); } });
    assert.equal(isBindingError(error), false);
    assert.equal(calls, 1);
  }
  let reads = 0;
  const unstable = Object.assign(new Error("original"), { code: "invalid_color" });
  Object.defineProperty(unstable, "operation", { get() { reads += 1; return reads === 1 ? "future" : "evaluateWcag22"; } });
  assert.equal(isBindingError(unstable), false);
  assert.equal(reads, 1);
  const proxy = Proxy.revocable(new Error("original"), {});
  proxy.revoke();
  assert.equal(isBindingError(proxy.proxy), false);
});
