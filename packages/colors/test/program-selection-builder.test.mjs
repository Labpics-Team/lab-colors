import assert from "node:assert/strict";
import test from "node:test";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV2, ProgramWireError,
  PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES } from "../program-wire/abi-v2.js";

const state = (id = 1, target = 21, candidate = 201) => ({ id, choices: [{ target, candidate }] });
const typed = (code) => (error) => error instanceof ProgramWireError && error.code === code;

function releaseBytes(revision, groups) {
  const payload = [];
  const u32 = (n) => { for (let i = 0; i < 4; i += 1) payload.push(Number((BigInt(n) >> BigInt(8*i)) & 255n)); };
  u32(1);
  for (let i = 0; i < 8; i += 1) payload.push(Number((revision >> BigInt(8*i)) & 255n));
  u32(groups.length);
  for (const group of groups) {
    u32(group.length);
    for (const row of group) {
      u32(row.id); u32(row.choices.length);
      for (const choice of row.choices) { u32(choice.target); u32(choice.candidate); }
    }
  }
  return payload;
}

test("V2 extends only its explicit section and keeps the V1 byte grammar intact", () => {
  const v1 = new ProgramWireBuilderV1().source(11, [20, 30, 40]).fixedTarget(21, 11).finish();
  const v2 = new ProgramWireBuilderV2().source(11, [20, 30, 40]).fixedTarget(21, 11).finish();
  assert.equal(new DataView(v1.buffer).getUint16(4, true), 1);
  assert.equal(new DataView(v2.buffer).getUint16(4, true), 2);
  v2[4] = 1;
  assert.deepEqual(v2, v1);
  assert.equal(typeof new ProgramWireBuilderV1().selectionRelease, "undefined");
});

test("selection release has independently specified little-endian framing", () => {
  const groups = [[state(17, 23, 29), state(31, 37, 41)], [state(43, 47, 53)]];
  const revision = 0x1020304050607080n;
  const bytes = new ProgramWireBuilderV2().selectionRelease(revision, groups).finish();
  const fixedSections = new Uint8Array(4 * 14);
  const payload = releaseBytes(revision, groups);
  const body = [...fixedSections.slice(0, 12), ...payload, ...fixedSections.slice(16)];
  const expected = new Uint8Array(10 + body.length);
  expected.set([0x4c, 0x43, 0x50, 0x57, 2, 0]);
  new DataView(expected.buffer).setUint32(6, expected.length, true);
  expected.set(body, 10);
  assert.deepEqual(bytes, expected);
});

test("all new input refusals leave the existing graph byte snapshot unchanged", () => {
  const builder = new ProgramWireBuilderV2().source(11, [0, 0, 0]);
  const original = builder.finish();
  const bad = [
    () => builder.selectionRelease(1, [[state()]]),
    () => builder.selectionRelease(-1n, [[state()]]),
    () => builder.selectionRelease(1n << 64n, [[state()]]),
    () => builder.selectionRelease(1n, []),
    () => builder.selectionRelease(1n, [[]]),
    () => builder.selectionRelease(1n, [[{ id: 1, choices: [] }]]),
    () => builder.selectionRelease(1n, [[state(-1)]]),
    () => builder.selectionRelease(1n, [[state(1, NaN)]]),
    () => builder.selectionRelease(1n, [[state(1, 21, "201")]]),
    () => builder.selectionRelease(1n, [[{ get id() { throw new Error("read failure"); } }]]),
  ];
  for (const run of bad) {
    assert.throws(run, typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), original);
  }
  builder.selectionRelease(0n, [[state()]]);
  const accepted = builder.finish();
  assert.throws(() => builder.selectionRelease(1n, [[state()]]), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  assert.deepEqual(builder.finish(), accepted);
});

test("states and choices have total bounded counts before emission", () => {
  const builder = new ProgramWireBuilderV2();
  const before = builder.finish();
  const cases = [
    Array.from({ length: 4097 }, () => [state()]),
    [Array.from({ length: 4097 }, (_, i) => state(i))],
    [[{ id: 1, choices: Array.from({ length: 2048 }, () => ({ target: 21, candidate: 201 })) }],
      [{ id: 2, choices: Array.from({ length: 2049 }, () => ({ target: 21, candidate: 201 })) }]],
  ];
  for (const groups of cases) {
    assert.throws(() => builder.selectionRelease(1n, groups), typed(PROGRAM_WIRE_TOO_MANY_ENTRIES));
    assert.deepEqual(builder.finish(), before);
  }
});

test("caller iterators are ignored and each accepted declaration is an owned snapshot", () => {
  const groups = [[state(7)]];
  for (const array of [groups, groups[0], groups[0][0].choices]) {
    array[Symbol.iterator] = () => { throw new Error("iterator must not run"); };
  }
  const builder = new ProgramWireBuilderV2().selectionRelease(1n, groups);
  const expected = builder.finish();
  groups[0][0].id = 9;
  groups[0][0].choices[0].candidate = 999;
  groups.length = 0;
  assert.deepEqual(builder.finish(), expected);
});

test("a reentrant input cannot overwrite a previously admitted release", () => {
  const builder = new ProgramWireBuilderV2();
  let committed;
  const nested = { get id() {
    builder.selectionRelease(1n, [[state(11)]]);
    committed = builder.finish();
    return 12;
  }, choices: [{ target: 21, candidate: 201 }] };
  assert.throws(() => builder.selectionRelease(2n, [[nested]]), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  assert.deepEqual(builder.finish(), committed);
});
