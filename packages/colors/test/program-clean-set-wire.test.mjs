import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, isProgramError } from "../index.js";
import {
  PROGRAM_WIRE_INVALID_DECLARATION,
  ProgramWireBuilderV1,
  ProgramWireError,
} from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// Независимый wire-вектор: LCPW/v1, 79 bytes, 14 секций; constraint =
// id u32 + KIND_CLEAN_SET (10) + root u32 + occurrence u32, всё LE.
const emptySection = "00000000";
const record = "785634120aefcdab8940302010";
for (const hard of [true, false]) {
  test(`declared clean-set ${hard ? "hard" : "report"} emits canonical Rust wire bytes`, () => {
    const builder = new ProgramWireBuilderV1();
    assert.equal(builder.declaredSrgb8CleanSet(hard, 0x12345678, 0x89abcdef, 0x10203040), builder);
    const expected = "4c43505701004f000000" + emptySection.repeat(hard ? 11 : 12)
      + "01000000" + record + emptySection.repeat(hard ? 2 : 1);
    assert.equal(Buffer.from(builder.finish()).toString("hex"), expected);
  });
}

function pointBuilder(source, opacity, final) {
  const builder = new ProgramWireBuilderV1()
    .source(11, source).fixedTarget(21, 11).surfaceInputPort(31)
    .opacityInput(32, opacity).solidPaint(42, 21).opacityPaint(41, 42, 32)
    .inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
    .presentationRoot(71, 61).presentationTarget(71, 61)
    .exactVisibleUnary(true, 82, 61, final).output(91, 41);
  return builder;
}

function resolve(builder, backdrop) {
  const runtime = compileProgramWire(builder.finish(), 1);
  let snapshot;
  try {
    snapshot = runtime.updateObserved(1n, new Uint32Array([1]), new Uint8Array(backdrop), 1);
    return {
      state: snapshot.state,
      outputs: Array.from({ length: snapshot.outputCount() }, (_, index) => ({
        slot: snapshot.outputSlot(index),
        rgb: [...snapshot.outputRgb(index)],
        opacity: snapshot.outputOpacity(index),
      })),
    };
  } finally {
    snapshot?.free();
    runtime.free();
  }
}

// Эти точки проверяют только явно объявленную sRGB8-конвенцию, не человеческую
// оценку. Alpha-пары меняют вердикт относительно source; exact-visible constraint
// отдельно подтверждает конечные байты композиции существующим Core evaluator.
const cases = [
  { name: "accepted opaque final", source: [0, 200, 70], opacity: 1,
    backdrop: [0, 0, 0], final: [0, 200, 70], state: "ready" },
  { name: "rejected opaque final", source: [0, 200, 71], opacity: 1,
    backdrop: [0, 0, 0], final: [0, 200, 71], state: "failed" },
  { name: "rejected source becomes accepted final after alpha", source: [128, 128, 129], opacity: 0.5,
    backdrop: [128, 128, 127], final: [128, 128, 128], state: "ready" },
  { name: "accepted source becomes rejected final after alpha", source: [255, 255, 255], opacity: 0.5,
    backdrop: [1, 1, 3], final: [128, 128, 129], state: "failed" },
  { name: "absent final owned domain fails hard", source: [0, 0, 0], opacity: 1,
    backdrop: [0, 0, 0], final: [0, 0, 0], state: "failed" },
];

for (const { name, source, opacity, backdrop, final, state } of cases) {
  test(`declared clean-set: ${name} through the existing WASM Program`, () => {
    const expectedOutput = [{ slot: 91, rgb: source, opacity }];
    assert.deepEqual(resolve(pointBuilder(source, opacity, final), backdrop), {
      state: "ready", outputs: expectedOutput,
    });
    const builder = pointBuilder(source, opacity, final).declaredSrgb8CleanSet(true, 81, 71, 61);
    assert.deepEqual(resolve(builder, backdrop), {
      state, outputs: state === "ready" ? expectedOutput : [],
    });
  });
}

test("declared clean-set report mode retains a rejected fixed output", () => {
  const source = [0, 200, 71];
  const builder = pointBuilder(source, 1, source).declaredSrgb8CleanSet(false, 81, 71, 61);
  assert.deepEqual(resolve(builder, [0, 0, 0]), {
    state: "ready", outputs: [{ slot: 91, rgb: source, opacity: 1 }],
  });
});

test("declared clean-set presentation references remain Core compile obligations", () => {
  const builder = pointBuilder([0, 200, 70], 1, [0, 200, 70])
    .declaredSrgb8CleanSet(true, 81, 72, 61);
  let unexpected;
  try {
    assert.throws(() => { unexpected = compileProgramWire(builder.finish(), 1); },
      (error) => isProgramError(error) && error.code === "program_compile");
  } finally {
    unexpected?.free();
  }
});

const invalidIds = [
  -1, 0.5, 2 ** 32, NaN, Infinity, -Infinity,
  "1", true, false, null, undefined, 1n, Symbol("id"),
];
const invalidFlags = [0, 1, "true", "false", null, undefined, 1n, Symbol("flag")];
const isInvalid = (error) => error instanceof ProgramWireError
  && error.code === PROGRAM_WIRE_INVALID_DECLARATION;

for (const hard of [true, false]) {
  test(`declared clean-set ${hard ? "hard" : "report"} rejects hostile scalars atomically`, () => {
    let calls = 0;
    const hostile = new Proxy({}, {
      get() { calls += 1; throw new Error("caller property must not be read"); },
    });
    const revoked = Proxy.revocable({}, {});
    revoked.revoke();
    const builder = new ProgramWireBuilderV1().declaredSrgb8CleanSet(hard, 0, 0xffff_ffff, 0);
    const before = builder.finish();
    for (let field = 0; field < 4; field += 1) {
      const values = field === 0 ? invalidFlags : invalidIds;
      for (const value of [...values, hostile, revoked.proxy]) {
        const args = [hard, 81, 71, 61];
        args[field] = value;
        assert.throws(() => builder.declaredSrgb8CleanSet(...args), isInvalid);
        assert.deepEqual(builder.finish(), before);
      }
    }
    assert.equal(calls, 0);
    builder.declaredSrgb8CleanSet(!hard, 0xffff_ffff, 0, 0xffff_ffff);
    const fresh = new ProgramWireBuilderV1()
      .declaredSrgb8CleanSet(hard, 0, 0xffff_ffff, 0)
      .declaredSrgb8CleanSet(!hard, 0xffff_ffff, 0, 0xffff_ffff);
    assert.deepEqual(builder.finish(), fresh.finish());
  });
}
