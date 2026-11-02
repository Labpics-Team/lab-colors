import assert from "node:assert/strict";
import test from "node:test";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "../program-wire/abi-v2.js";
import { ProgramWireBuilderV3, ProgramWireError,
  PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES } from "../program-wire/abi-v3.js";

const typed = (code) => (error) => error instanceof ProgramWireError && error.code === code;
const gray = () => ({ kind: "gray", lower: 0, upper: 255 });
const box = () => ({ kind: "box", lower: [0, 0, 0], upper: [255, 255, 255] });

test("V3 сохраняет общую грамматику V1/V2 и не наследует finite selection", () => {
  const make = (Builder) => new Builder().source(11, [20, 30, 40]).fixedTarget(21, 11).finish();
  const old = make(ProgramWireBuilderV1);
  for (const [Builder, version] of [[ProgramWireBuilderV2, 2], [ProgramWireBuilderV3, 3]]) {
    const bytes = make(Builder);
    assert.equal(new DataView(bytes.buffer).getUint16(4, true), version);
    bytes[4] = 1;
    assert.deepEqual(bytes, old);
  }
  assert.equal(typeof new ProgramWireBuilderV3().selectionRelease, "undefined");
  assert.equal(typeof new ProgramWireBuilderV2().computedTarget, "undefined");
  assert.equal(typeof new ProgramWireBuilderV1().assignment, "undefined");
});

test("V3 box, gray и явный порядок имеют независимо заданные LE-байты", () => {
  const bytes = new ProgramWireBuilderV3()
    .computedTarget(0x10203040, { kind: "box", lower: [1, 2, 3], upper: [4, 5, 6] }, [250, 0, 7], 0.25)
    .computedTarget(19, { kind: "gray", lower: 10, upper: 12 }, [9, 8, 7], 8)
    .assignment([0x10203040, 19]).finish();
  // Эталон из wire-контракта: опоры вне domain допустимы, ID не сортируются.
  const body = [
    0, 0, 0, 0,
    2, 0, 0, 0,
    0x40, 0x30, 0x20, 0x10, 3, 1, 1, 2, 3, 4, 5, 6, 250, 0, 7,
    0, 0, 0, 0, 0, 0, 0xd0, 0x3f,
    19, 0, 0, 0, 3, 2, 10, 12, 9, 8, 7,
    0, 0, 0, 0, 0, 0, 0x20, 0x40,
    0, 0, 0, 0,
    1, 0, 0, 0, 2, 0, 2, 0, 0, 0, 0x40, 0x30, 0x20, 0x10, 19, 0, 0, 0,
    ...Array(40).fill(0),
  ];
  const expected = new Uint8Array(10 + body.length);
  expected.set([0x4c, 0x43, 0x50, 0x57, 3, 0]);
  new DataView(expected.buffer).setUint32(6, expected.length, true);
  expected.set(body, 10);
  assert.deepEqual(bytes, expected);
});

test("нулевой допуск каноничен, границы domain включены", () => {
  for (const domain of [gray(), box(), { kind: "gray", lower: 19, upper: 19 },
    { kind: "box", lower: [1, 2, 3], upper: [1, 2, 3] }]) {
    const make = (delta) => new ProgramWireBuilderV3().computedTarget(0xffff_ffff, domain, [0, 0, 0], delta).finish();
    assert.deepEqual(make(-0), make(0));
  }
});

test("неверные скаляры и domains отклоняются до изменения декларации", () => {
  const builder = new ProgramWireBuilderV3().source(11, [20, 30, 40]);
  const before = builder.finish();
  const invalidDomains = [
    null, undefined, 1, "gray", {}, { kind: "unknown" }, { kind: Symbol("box") },
    { kind: "gray", lower: -1, upper: 255 }, { kind: "gray", lower: 0, upper: 256 },
    { kind: "gray", lower: 2, upper: 1 }, { kind: "gray", lower: 0.5, upper: 1 },
    { kind: "box", lower: [0, 0], upper: [255, 255, 255] },
    { kind: "box", lower: [0, 0, 0], upper: [255, 256, 255] },
    ...[0, 1, 2].map((channel) => {
      const lower = [0, 0, 0]; lower[channel] = 2;
      return { kind: "box", lower, upper: [1, 1, 1] };
    }),
  ];
  const actions = [
    ...[-1, 0x1_0000_0000, 1.5, NaN, "1"].map((id) => () => builder.computedTarget(id, gray(), [1, 2, 3], 1)),
    ...invalidDomains.map((domain) => () => builder.computedTarget(21, domain, [1, 2, 3], 1)),
    ...[-1, -Number.MIN_VALUE, NaN, Infinity, -Infinity, Number.MAX_VALUE, 1e155, "1"].map(
      (delta) => () => builder.computedTarget(21, gray(), [1, 2, 3], delta)),
    ...[[], [1, 2], [1, 2, 3, 4], [1, 2, 256], [1, -1, 2], new Uint8Array(3)].map(
      (anchor) => () => builder.computedTarget(21, gray(), anchor, 1)),
  ];
  for (const run of actions) {
    assert.throws(run, typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  }
  builder.computedTarget(21, gray(), [1, 2, 3], 1);
  assert.notDeepEqual(builder.finish(), before);
});

test("ошибки пользовательских getters сохраняют typed отказ и прежние байты", () => {
  const builder = new ProgramWireBuilderV3().source(11, [20, 30, 40]);
  const before = builder.finish();
  const throwing = (value, key) => new Proxy(value, {
    get(target, property, receiver) {
      if (property === key) throw new Error("caller read");
      return Reflect.get(target, property, receiver);
    },
  });
  const revoked = (value) => { const proxy = Proxy.revocable(value, {}); proxy.revoke(); return proxy.proxy; };
  const actions = [
    ...["kind", "lower", "upper"].map((field) => () => builder.computedTarget(21, throwing(box(), field), [1, 2, 3], 1)),
    () => builder.computedTarget(21, revoked(gray()), [1, 2, 3], 1),
    ...["length", "0", "2"].flatMap((field) => [
      () => builder.computedTarget(21, { ...box(), lower: throwing([0, 0, 0], field) }, [1, 2, 3], 1),
      () => builder.computedTarget(21, box(), throwing([1, 2, 3], field), 1),
    ]),
    () => builder.computedTarget(21, box(), revoked([1, 2, 3]), 1),
    () => builder.assignment(throwing([21], "length")),
    () => builder.assignment(throwing([21], "0")),
    () => builder.assignment(revoked([21])),
  ];
  for (const run of actions) {
    assert.throws(run, typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  }
});

test("диагностика не вызывает преобразования отклонённого значения", () => {
  let coercions = 0;
  const value = { [Symbol.toPrimitive]() { coercions += 1; throw new Error("coercion"); } };
  const builder = new ProgramWireBuilderV3();
  const before = builder.finish();
  for (const run of [
    () => builder.computedTarget(value, gray(), [1, 2, 3], 1),
    () => builder.computedTarget(21, { kind: value }, [1, 2, 3], 1),
    () => builder.computedTarget(21, { kind: "gray", lower: value, upper: 255 }, [1, 2, 3], 1),
    () => builder.computedTarget(21, gray(), [1, value, 3], 1),
    () => builder.computedTarget(21, gray(), [1, 2, 3], value),
    () => builder.assignment([value]),
  ]) {
    assert.throws(run, typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  }
  assert.equal(coercions, 0);
});

test("декларации копируют индексные значения и не вызывают iterator клиента", () => {
  const domain = box(), anchor = [1, 2, 3], order = [29, 21];
  for (const array of [domain.lower, domain.upper, anchor, order]) {
    Object.defineProperty(array, Symbol.iterator, { get() { throw new Error("iterator"); } });
  }
  const builder = new ProgramWireBuilderV3().computedTarget(21, domain, anchor, 1).assignment(order);
  const before = builder.finish();
  domain.lower[0] = 99; domain.upper[1] = 0; domain.kind = "gray";
  anchor[1] = 99; order.reverse();
  assert.deepEqual(builder.finish(), before);
});

test("assignment ограничен одним списком из 4096 u32 и сохраняет явный порядок", () => {
  const builder = new ProgramWireBuilderV3();
  const before = builder.finish();
  for (const values of [null, undefined, [], [NaN], [-1], [0x1_0000_0000], ["21"], [1.5]]) {
    assert.throws(() => builder.assignment(values), typed(PROGRAM_WIRE_INVALID_DECLARATION));
    assert.deepEqual(builder.finish(), before);
  }
  const tooMany = Array(4097);
  Object.defineProperty(tooMany, "0", { get() { throw new Error("admission must precede reads"); } });
  assert.throws(() => builder.assignment(tooMany), typed(PROGRAM_WIRE_TOO_MANY_ENTRIES));
  assert.deepEqual(builder.finish(), before);
  builder.assignment(Array.from({ length: 4096 }, (_, i) => 4095 - i));
  const accepted = builder.finish();
  const view = new DataView(accepted.buffer);
  assert.equal(view.getUint32(28, true), 4096);
  assert.equal(view.getUint32(32, true), 4095);
  assert.equal(view.getUint32(32 + 4095 * 4, true), 0);
  assert.throws(() => builder.assignment([1]), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  assert.deepEqual(builder.finish(), accepted);
});

test("reentrant getter не заменяет уже принятое assignment", () => {
  const builder = new ProgramWireBuilderV3();
  let committed;
  const order = [21];
  Object.defineProperty(order, "0", { get() {
    builder.assignment([29]);
    committed = builder.finish();
    return 21;
  } });
  assert.throws(() => builder.assignment(order), typed(PROGRAM_WIRE_INVALID_DECLARATION));
  assert.deepEqual(builder.finish(), committed);
});
