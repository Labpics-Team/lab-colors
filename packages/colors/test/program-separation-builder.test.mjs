import assert from "node:assert/strict";
import test from "node:test";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV3, ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION } from "../program-wire/abi-v3.js";

test("V3 separation encodes the exact declared inclusive bounds", () => {
  const bytes = new ProgramWireBuilderV3().visibleSeparationHard(7, 13, 0.125, 0.5).finish();
  const expected = new Uint8Array(10 + 14 * 4 + 25);
  const view = new DataView(expected.buffer);
  expected.set([0x4c, 0x43, 0x50, 0x57, 3, 0]);
  view.setUint32(6, expected.length, true);
  const section = 10 + 11 * 4;
  view.setUint32(section, 1, true);
  view.setUint32(section + 4, 7, true);
  view.setUint8(section + 8, 11);
  view.setUint32(section + 9, 13, true);
  view.setFloat64(section + 13, 0.125, true);
  view.setFloat64(section + 21, 0.5, true);
  assert.deepEqual(bytes, expected);
  assert.equal(typeof new ProgramWireBuilderV1().visibleSeparationHard, "undefined");
});

test("invalid separation bounds or IDs never mutate the admitted byte snapshot", () => {
  const builder = new ProgramWireBuilderV3().source(3, [1, 2, 3]);
  const before = builder.finish();
  for (const args of [
    [1, 2, NaN, 1], [1, 2, 0, Infinity], [1, 2, -0.01, 1],
    [1, 2, 0.5, 0.4], [1, 2, 0, 2.01], [1, 2, "0", 1],
    [-1, 2, 0, 1], [1, 2.5, 0, 1],
  ]) {
    assert.throws(() => builder.visibleSeparationHard(...args),
      (error) => error instanceof ProgramWireError && error.code === PROGRAM_WIRE_INVALID_DECLARATION);
    assert.deepEqual(builder.finish(), before);
  }
  assert.deepEqual(
    new ProgramWireBuilderV3().visibleSeparationHard(1, 2, -0, -0).finish(),
    new ProgramWireBuilderV3().visibleSeparationHard(1, 2, 0, 0).finish(),
  );
});

test("invalid pair members and bounds leave the declaration unchanged", () => {
  const builder = new ProgramWireBuilderV3().source(3, [1, 2, 3]);
  const before = builder.finish();
  for (const args of [
    [-1, 2, 3, 0, 1], [1, 2.5, 3, 0, 1], [1, 2, 2 ** 32, 0, 1], [1, 2, 2, 0, 1],
    [1, 2, 3, NaN, 1], [1, 2, 3, 0, Infinity], [1, 2, 3, -0.01, 1],
    [1, 2, 3, 0.5, 0.4], [1, 2, 3, 0, 2.01], [1, 2, 3, "0", 1],
  ]) {
    assert.throws(() => builder.visiblePairSeparationHard(...args),
      (error) => error instanceof ProgramWireError && error.code === PROGRAM_WIRE_INVALID_DECLARATION);
    assert.deepEqual(builder.finish(), before);
  }
  assert.equal(typeof new ProgramWireBuilderV1().visiblePairSeparationHard, "undefined");
});
