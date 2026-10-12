import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, evaluateWcag22 } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20]).fixedTarget(21, 11).surfaceInputPort(31)
  .solidPaint(41, 21).inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();

const refusal = (code, operation) => (error) => {
  assert.ok(error instanceof Error);
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
