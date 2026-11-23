import assert from "node:assert/strict";
import test from "node:test";
import { ProgramWireBuilderV3, ProgramWireError } from "../program-wire/abi-v3.js";

test("V3 hierarchy wire contains endpoints and derives no client-side threshold", () => {
  const bytes = new ProgramWireBuilderV3().preserveTonalHierarchy(7, [13, 17]).finish();
  const expected = new Uint8Array(10 + 14 * 4 + 17);
  const view = new DataView(expected.buffer);
  expected.set([0x4c, 0x43, 0x50, 0x57, 3, 0]);
  view.setUint32(6, expected.length, true);
  const section = 10 + 11 * 4;
  view.setUint32(section, 1, true);
  view.setUint32(section + 4, 7, true);
  view.setUint8(section + 8, 13);
  view.setUint32(section + 9, 2, true);
  view.setUint32(section + 13, 13, true);
  view.setUint32(section + 17, 17, true);
  assert.deepEqual(bytes, expected);
});

test("reference and hierarchy admission is transactional and snapshots caller arrays", () => {
  const builder = new ProgramWireBuilderV3().source(1, [10, 20, 30]);
  const before = builder.finish();
  const failures = [
    () => builder.colorTarget(2, [0, 0, 0], { appearance: 3, referenceAppearance: 4 }),
    () => builder.colorTarget(2, [0, 0, 0], { referenceAppearance: -1 }),
    () => builder.colorTarget(2, [0, 0, 0], { referenceAppearance: null }),
    () => builder.preserveTonalHierarchy(2, [3]),
    () => builder.preserveTonalHierarchy(2, [3, 3]),
    () => builder.preserveTonalHierarchy(2, [3, NaN]),
    () => builder.assignment([], { reference: [[1, [0, 0, 0]], [1, [1, 1, 1]]] }),
    () => builder.assignment([], { reference: [[1, [256, 0, 0]]] }),
    () => builder.assignment([], { get reference() { throw new Error("getter"); } }),
  ];
  for (const run of failures) {
    assert.throws(run, ProgramWireError);
    assert.deepEqual(builder.finish(), before);
  }
  assert.notDeepEqual(
    new ProgramWireBuilderV3().colorTarget(2, [0, 0, 0], { referenceAppearance: 0 }).finish(),
    new ProgramWireBuilderV3().colorTarget(2, [0, 0, 0], { referenceAppearance: undefined }).finish(),
  );
  const rgb = [255, 255, 255], reference = [[4, rgb]], members = [5, 6];
  builder.preserveTonalHierarchy(7, members).assignment([], { reference });
  const accepted = builder.finish();
  rgb[0] = 0; reference.length = 0; members[0] = 99;
  assert.deepEqual(builder.finish(), accepted);
});

test("reference getter reentry cannot overwrite an admitted assignment", () => {
  const builder = new ProgramWireBuilderV3();
  let inner;
  assert.throws(() => builder.assignment([7], {
    get reference() {
      builder.assignment([8], { reference: [[1, [0, 0, 0]]] });
      inner = builder.finish();
      return [[1, [255, 255, 255]]];
    },
  }), ProgramWireError);
  assert.deepEqual(builder.finish(), inner);
});
