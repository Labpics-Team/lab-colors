import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20]).fixedTarget(21, 11).surfaceInputPort(31)
  .solidPaint(41, 21).inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();
const observe = (runtime, revision, count = 1) => runtime.updateObserved(
  revision, new Uint32Array([1]), new Uint8Array([255, 255, 255]), count,
);
const read = (snapshot) => ({
  state: snapshot.state,
  count: snapshot.outputCount(),
  slot: snapshot.outputSlot(0),
  rgb: [...snapshot.outputRgb(0)],
  opacity: snapshot.outputOpacity(0),
});
const expected = { state: "ready", count: 1, slot: 91, rgb: [20, 20, 20], opacity: 1 };
const programError = (code, operation) => (error) =>
  isProgramError(error) && error.code === code && error.operation === operation;

// Даже на старом дефектном артефакте неожиданный успешный вызов не оставляет
// owning snapshot/runtime: RED доказывает неверный допуск, не утечку теста.
function rejects(invoke, error) {
  let unexpected;
  try {
    assert.throws(() => { unexpected = invoke(); }, error);
  } finally {
    unexpected?.free?.();
  }
}

const invalidNumbers = [
  ["negative", -1], ["fraction", 0.5], ["fraction-truncates-to-one", 1.5],
  ["overflow", 2 ** 32], ["overflow-wraps-to-one", 2 ** 32 + 1],
  ["negative-wraps-to-one", 1 - 2 ** 32], ["NaN", Number.NaN],
  ["infinity", Infinity], ["negative-infinity", -Infinity],
  ["string", "1"], ["boolean", true], ["null", null],
  ["undefined", undefined], ["bigint", 1n], ["symbol", Symbol("number")],
];
const invalidRevisions = [
  ["negative", -1n], ["overflow", 1n << 64n], ["overflow-wraps-to-one", (1n << 64n) + 1n],
  ["number", 1], ["string", "2"], ["boolean", true], ["null", null],
  ["undefined", undefined], ["symbol", Symbol("revision")],
];

for (const [label, value] of invalidNumbers) {
  test(`compile: ${label} stream id is not coerced`, () => {
    rejects(() => compileProgramWire(wire, value), programError("program_instantiate", "compileProgramWire"));
  });
  for (const operation of ["updateObserved", "updateUnknown"]) {
    test(`${operation}: ${label} integer input refuses atomically`, () => {
      const runtime = compileProgramWire(wire, 1);
      let previous;
      let retry;
      try {
        previous = observe(runtime, 1n);
        rejects(
          () => operation === "updateObserved" ? observe(runtime, 2n, value) : runtime.updateUnknown(2n, value),
          programError("program_update", operation),
        );
        assert.deepEqual(read(previous), expected);
        retry = observe(runtime, 2n);
        assert.deepEqual(read(retry), expected);
      } finally {
        retry?.free();
        previous?.free();
        runtime.free();
      }
    });
  }
}

for (const [label, revision] of invalidRevisions) {
  for (const operation of ["updateObserved", "updateUnknown"]) {
    test(`${operation}: ${label} revision cannot advance or wrap the session`, () => {
      const runtime = compileProgramWire(wire, 1);
      let previous;
      let retry;
      try {
        previous = observe(runtime, 1n);
        rejects(
          () => operation === "updateObserved" ? observe(runtime, revision) : runtime.updateUnknown(revision, 1),
          programError("program_update", operation),
        );
        assert.deepEqual(read(previous), expected);
        retry = observe(runtime, 2n);
        assert.deepEqual(read(retry), expected);
      } finally {
        retry?.free();
        previous?.free();
        runtime.free();
      }
    });
  }
}

for (const method of ["outputSlot", "outputRgb", "outputOpacity"]) {
  test(`${method}: invalid indices cannot alias output zero`, () => {
    const runtime = compileProgramWire(wire, 1);
    const snapshot = observe(runtime, 1n);
    try {
      for (const value of [-0.5, 0.5, 2 ** 32, -(2 ** 32), "0", null, false, NaN]) {
        assert.throws(() => snapshot[method](value), Error);
      }
      assert.deepEqual(read(snapshot), expected);
    } finally {
      snapshot.free();
      runtime.free();
    }
  });
}

for (const field of ["stream", "observed-revision", "unknown-revision", "surface-count", "reason", "index"]) {
  test(`${field}: admission does not invoke a caller coercion hook`, () => {
    let calls = 0;
    const value = { [Symbol.toPrimitive]() { calls += 1; return field.includes("revision") ? 2n : 1; } };
    const runtime = compileProgramWire(wire, 1);
    const previous = observe(runtime, 1n);
    let retry;
    try {
      const operations = {
        stream: () => compileProgramWire(wire, value),
        "observed-revision": () => observe(runtime, value),
        "unknown-revision": () => runtime.updateUnknown(value, 1),
        "surface-count": () => observe(runtime, 2n, value),
        reason: () => runtime.updateUnknown(2n, value),
        index: () => previous.outputSlot(value),
      };
      rejects(operations[field], Error);
      assert.equal(calls, 0);
      assert.deepEqual(read(previous), expected);
      retry = observe(runtime, 2n);
      assert.deepEqual(read(retry), expected);
    } finally {
      retry?.free();
      previous.free();
      runtime.free();
    }
  });
}

test("exact u32 stream and u64 revision endpoints remain usable", () => {
  for (const stream of [0, 0xffff_ffff]) {
    const runtime = compileProgramWire(wire, stream);
    const held = [];
    try {
      for (const revision of [0n, (1n << 64n) - 2n, (1n << 64n) - 1n]) {
        const snapshot = observe(runtime, revision);
        held.push(snapshot);
        assert.deepEqual(read(snapshot), expected);
      }
      // Настоящий предел автомата не обходится: меньшая ревизия всё ещё ошибка.
      rejects(() => observe(runtime, 1n), programError("program_update", "updateObserved"));
      for (const snapshot of held) assert.deepEqual(read(snapshot), expected);
    } finally {
      for (const snapshot of held.reverse()) snapshot.free();
      runtime.free();
    }
  }
});

test("unknown with the u32 reason endpoint retains lifecycle semantics", () => {
  const runtime = compileProgramWire(wire, 1);
  const previous = observe(runtime, 0n);
  let unknown;
  let recovered;
  try {
    unknown = runtime.updateUnknown(1n, 0xffff_ffff);
    assert.equal(unknown.state, "stale");
    assert.deepEqual(read(previous), expected);
    recovered = observe(runtime, 2n);
    assert.deepEqual(read(recovered), expected);
  } finally {
    recovered?.free();
    unknown?.free();
    previous.free();
    runtime.free();
  }
});
