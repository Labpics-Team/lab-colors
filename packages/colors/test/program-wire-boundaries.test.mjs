import assert from "node:assert/strict";
import test from "node:test";
import {
  ProgramWireBuilderV1,
  ProgramWireError,
  PROGRAM_WIRE_INVALID_DECLARATION,
  PROGRAM_WIRE_TOO_MANY_ENTRIES,
} from "../program-wire/abi-v1.js";

const typed = (code) => (error) => error instanceof ProgramWireError && error.code === code;
const fields = [
  ["u32", (builder, value) => builder.source(value, [0, 0, 0])],
  ["byte", (builder, value) => builder.source(1, [value, 0, 0])],
  ["f64", (builder, value) => builder.opacityInput(1, value)],
  ["enum", (builder, value) => builder.wcag22VisibleUnary(true, 1, 2, value)],
];

for (const [field, declare] of fields) {
  for (const kind of ["symbol", "null-prototype", "coercion-hook"]) {
    test(`${field}: ${kind} refuses atomically without diagnostic coercion`, () => {
      let coercions = 0;
      const value = kind === "symbol"
        ? Symbol("invalid")
        : kind === "null-prototype"
          ? Object.create(null)
          : {
            [Symbol.toPrimitive]() {
              coercions += 1;
              throw new Error("caller-controlled coercion");
            },
          };
      const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
      const before = builder.finish();
      assert.throws(() => declare(builder, value), typed(PROGRAM_WIRE_INVALID_DECLARATION));
      assert.equal(coercions, 0);
      assert.deepEqual(builder.finish(), before);
    });
  }
}

for (const kind of ["finite", "relation"]) {
  const values = (count) => Array.from({ length: count }, (_, id) => kind === "finite"
    ? { id: id + 1, rgb: [0, 0, 0], opacity: 1 }
    : id + 1);
  const declare = (builder, candidates) => kind === "finite"
    ? builder.finiteTarget(1, candidates)
    : builder.exactIntrinsicRelationHard(1, 0, candidates);

  test(`${kind}: the wire count bound admits 4096 nested entries`, () => {
    const bytes = declare(new ProgramWireBuilderV1(), values(4096)).finish();
    // Независимые offsets грамматики: header + предыдущие пустые секции + entry.
    const countOffset = kind === "finite" ? 23 : 67;
    assert.equal(new DataView(bytes.buffer).getUint32(countOffset, true), 4096);
  });

  test(`${kind}: 4097 nested entries refuse without changing the builder`, () => {
    const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
    const before = builder.finish();
    assert.throws(() => declare(builder, values(4097)), typed(PROGRAM_WIRE_TOO_MANY_ENTRIES));
    assert.deepEqual(builder.finish(), before);
  });

  test(`${kind}: count admission precedes candidate access`, () => {
    const candidates = Array(4097);
    let reads = 0;
    Object.defineProperty(candidates, "0", {
      get() {
        reads += 1;
        throw new Error("candidate read before count admission");
      },
    });
    assert.throws(
      () => declare(new ProgramWireBuilderV1(), candidates),
      typed(PROGRAM_WIRE_TOO_MANY_ENTRIES),
    );
    assert.equal(reads, 0);
  });

  test(`${kind}: an array iterator cannot expand the admitted count`, () => {
    const candidates = values(1);
    const expected = declare(new ProgramWireBuilderV1(), candidates).finish();
    candidates[Symbol.iterator] = function* () {
      yield* values(4097);
    };
    assert.deepEqual(declare(new ProgramWireBuilderV1(), candidates).finish(), expected);
  });
}

for (const [name, values, declare] of [
  ["source RGB", [20, 21, 22], (builder, value) => builder.source(1, value)],
  ["finite candidates", [{ id: 1, rgb: [20, 21, 22], opacity: 1 }],
    (builder, value) => builder.finiteTarget(1, value)],
  ["relation candidates", [1, 2], (builder, value) => builder.exactIntrinsicRelationHard(1, 0, value)],
  ["family release", Array(32).fill(0), (builder, value) => builder.family(1, value)],
  ["expected RGB", [20, 21, 22], (builder, value) => builder.exactVisibleUnary(true, 1, 2, value)],
]) {
  test(`${name}: wire arrays use indexed values without consulting caller iterators`, () => {
    const expected = declare(new ProgramWireBuilderV1(), values).finish();
    const hostile = [...values];
    let reads = 0;
    Object.defineProperty(hostile, Symbol.iterator, {
      get() {
        reads += 1;
        throw new Error("caller iterator must not run");
      },
    });
    assert.deepEqual(declare(new ProgramWireBuilderV1(), hostile).finish(), expected);
    assert.equal(reads, 0);
  });
}
