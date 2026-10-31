import assert from "node:assert/strict";
import test from "node:test";
import {
  ProgramWireBuilderV1,
  ProgramWireError,
  PROGRAM_WIRE_INVALID_DECLARATION,
} from "../program-wire/abi-v1.js";

const typed = (code) => (error) => error instanceof ProgramWireError && error.code === code;

for (const [name, values, declare] of [
  ["source RGB", [20, 21, 22], (builder, value) => builder.source(1, value)],
  ["finite candidates", [{ id: 1, rgb: [20, 21, 22], opacity: 1 }],
    (builder, value) => builder.finiteTarget(1, value)],
  ["relation candidates", [1, 2], (builder, value) => builder.exactIntrinsicRelationHard(1, 0, value)],
  ["family release", Array(32).fill(0), (builder, value) => builder.family(1, value)],
  ["expected RGB", [20, 21, 22], (builder, value) => builder.exactVisibleUnary(true, 1, 2, value)],
]) {
  for (const property of ["length", "0"]) {
    test(`${name}: throwing ${property} read is a typed atomic refusal`, () => {
      const callerError = new Error("caller-owned property read failed");
      const hostile = new Proxy(values, {
        get(target, key, receiver) {
          if (key === property) throw callerError;
          return Reflect.get(target, key, receiver);
        },
      });
      const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
      const before = builder.finish();
      assert.throws(() => declare(builder, hostile), typed(PROGRAM_WIRE_INVALID_DECLARATION));
      assert.deepEqual(builder.finish(), before);
    });
  }
  test(`${name}: a revoked array Proxy is a typed atomic refusal`, () => {
    const { proxy, revoke } = Proxy.revocable(values, {});
    revoke();
    const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
    const before = builder.finish();
    assert.throws(() => declare(builder, proxy), typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  });
}

for (const property of ["id", "rgb", "opacity"]) {
  test(`finite candidate: throwing ${property} read is a typed atomic refusal`, () => {
    const candidate = { id: 1, rgb: [20, 21, 22], opacity: 1 };
    Object.defineProperty(candidate, property, { get() { throw new Error("caller property"); } });
    const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
    const before = builder.finish();
    assert.throws(() => builder.finiteTarget(1, [candidate]), typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  });
}


test("source RGB stores asymmetric unsigned channel bytes and snapshots each index once", () => {
  const reads = [];
  const rgb = new Proxy([0x80, 0x01, 0xff], {
    get(target, key) {
      reads.push(key);
      return Reflect.get(target, key);
    },
  });
  const wire = new ProgramWireBuilderV1().source(17, rgb).finish();
  assert.deepEqual(reads, ["length", "0", "1", "2"]);
  // Header, source count and ID precede the first source's three wire bytes.
  const offset = 4 + 2 + 4 + 4 + 4;
  assert.deepEqual([...wire.subarray(offset, offset + 3)], [0x80, 0x01, 0xff]);
});
