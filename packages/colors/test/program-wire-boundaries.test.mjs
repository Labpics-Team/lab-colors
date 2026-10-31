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
      yield* values(2);
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

for (const kind of ["finite", "relation"]) {
  const invalidLengths = [
    () => Number.NaN, () => undefined, () => -1, () => 0.5,
    () => "1", () => 1n, () => Symbol("length"), () => Object.create(null),
    (onCoercion) => ({ [Symbol.toPrimitive]: onCoercion }),
  ];
  for (const [index, makeLength] of invalidLengths.entries()) {
    test(`${kind}: invalid array length ${index} refuses before coercion or element access`, () => {
      let reads = 0;
      let coercions = 0;
      const length = makeLength(() => { coercions += 1; return 1; });
      const candidates = new Proxy([], {
        get(target, key, receiver) {
          if (key === "length") return length;
          if (key === "0") reads += 1;
          return Reflect.get(target, key, receiver);
        },
      });
      const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
      const before = builder.finish();
      const declare = () => kind === "finite"
        ? builder.finiteTarget(1, candidates)
        : builder.exactIntrinsicRelationHard(1, 0, candidates);
      assert.throws(declare, typed(PROGRAM_WIRE_INVALID_DECLARATION));
      assert.equal(reads, 0);
      assert.equal(coercions, 0);
      assert.deepEqual(builder.finish(), before);
    });
  }
}

// The same physical ranges are admitted by Rust PaintValueV1,
// AdmittedOpacityV1 and AppearanceContextV1.
const physicalDomains = [
  ["finite paint opacity",
    (builder, value) => builder.finiteTarget(8, [{ id: 1, rgb: [20, 30, 40], opacity: value }]),
    [-0.1, -Number.MIN_VALUE, 1 + Number.EPSILON, 2, Infinity, -Infinity, NaN, undefined]],
  ["opacity input",
    (builder, value) => builder.opacityInput(8, value),
    [-0.1, -Number.MIN_VALUE, 1 + Number.EPSILON, 2, Infinity, -Infinity, NaN, undefined]],
  ["adapting luminance",
    (builder, value) => builder.sourceOverOccurrence(8, 1, 2, value, 0.2, 1),
    [0, -0, -Number.MIN_VALUE, -1, Infinity, -Infinity, NaN, undefined]],
  ["background luminance ratio",
    (builder, value) => builder.sourceOverOccurrence(8, 1, 2, 64, value, 1),
    [0, -0, -Number.MIN_VALUE, -1, 1 + Number.EPSILON, 2, Infinity, -Infinity, NaN, undefined]],
];

for (const [domain, declare, invalidValues] of physicalDomains) {
  for (const value of invalidValues) {
    test(`${domain}: ${String(value)} refuses before changing wire`, () => {
      const builder = new ProgramWireBuilderV1().source(11, [20, 20, 20]);
      const before = builder.finish();
      assert.throws(() => declare(builder, value), typed(PROGRAM_WIRE_INVALID_DECLARATION));
      assert.deepEqual(builder.finish(), before);
    });
  }
}

test("binary64 domain boundaries preserve finite admissible values", () => {
  for (const opacity of [0, Number.MIN_VALUE, 0.5, 1]) {
    assert.ok(new ProgramWireBuilderV1()
      .finiteTarget(1, [{ id: 1, rgb: [0, 0, 0], opacity }]).finish().length > 0);
    assert.ok(new ProgramWireBuilderV1().opacityInput(1, opacity).finish().length > 0);
  }
  for (const adapting of [Number.MIN_VALUE, 1, 64, Number.MAX_VALUE]) {
    assert.ok(new ProgramWireBuilderV1()
      .sourceOverOccurrence(1, 2, 3, adapting, Number.MIN_VALUE, 1)
      .finish().length > 0);
  }
  assert.ok(new ProgramWireBuilderV1()
    .sourceOverOccurrence(1, 2, 3, 64, 1, 1).finish().length > 0);
});

test("negative zero and positive zero produce identical canonical opacity wires", () => {
  for (const declare of [
    (alpha) => new ProgramWireBuilderV1().opacityInput(1, alpha).finish(),
    (alpha) => new ProgramWireBuilderV1()
      .finiteTarget(1, [{ id: 1, rgb: [0, 0, 0], opacity: alpha }]).finish(),
  ]) {
    assert.deepEqual(declare(-0), declare(0));
  }
});

test("physical scalar admission never coerces caller-owned objects", () => {
  for (const [, declare] of physicalDomains) {
    let conversions=0;
    const value = { valueOf() { conversions++; throw new Error("coercion called"); } };
    const builder = new ProgramWireBuilderV1().source(1, [0, 0, 0]);
    const before = builder.finish();
    assert.throws(() => declare(builder, value), typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.equal(conversions, 0);
    assert.deepEqual(builder.finish(), before);
  }
});

test("wire header u32 bound rejects oversized sparse section before allocation", () => {
  const builder = new ProgramWireBuilderV1();
  const section = builder.section("sources");
  section.sink = { bytes: { length: 0xffff_ffff } };
  assert.throws(() => builder.finish(), typed(PROGRAM_WIRE_TOO_MANY_ENTRIES));
  assert.equal(section.count, 0);
});

test("wire refuses corrupted section counts before serialization", () => {
  for (const count of [-1, 0.5, NaN, Infinity]) {
    const builder = new ProgramWireBuilderV1();
    builder.section("sources").count = count;
    assert.throws(() => builder.finish(), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  }
});


test("wire refuses invalid section byte lengths before copying", () => {
  for (const length of [-1, NaN, Infinity]) {
    const builder = new ProgramWireBuilderV1();
    builder.section("sources").sink = { bytes: { length } };
    assert.throws(() => builder.finish(), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  }
});


test("finished wire owns its bytes across section growth and caller mutation", () => {
  const builder = new ProgramWireBuilderV1().source(1, [1, 2, 3]);
  const first = builder.finish();
  const expected = first.slice();
  first.fill(0xff);
  assert.deepEqual(builder.finish(), expected);
  const beforeGrowth = builder.finish();
  for (let id = 2; id <= 512; id++) builder.source(id, [0, 0, 0]);
  const grown = builder.finish();
  assert.deepEqual(beforeGrowth, expected);
  assert.equal(grown.length - expected.length, 511 * (4 + 3));
});
