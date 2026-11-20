import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const NativeFloat64Array = Float64Array;
const allocationFailure = new RangeError("controlled output table allocation refusal");
let refuseCopy = false;
let refusedCopies = 0;
const OutputArray = new Proxy(NativeFloat64Array, {
  construct(Target, args) {
    // A WASM view borrows an ArrayBuffer; only the independent JS copy fails.
    if (refuseCopy && !(args[0] instanceof ArrayBuffer)) {
      refusedCopies += 1;
      throw allocationFailure;
    }
    return Reflect.construct(Target, args);
  },
});

let api;
try {
  // The real binding captures its allocator at module evaluation.
  globalThis.Float64Array = OutputArray;
  api = await import("@labpics/colors");
} finally {
  globalThis.Float64Array = NativeFloat64Array;
}
await api.default({
  module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))),
});
// The generated initializer exposes memory for observation, not allocation.
// Every operation under test still enters through the public snapshot API.
const { memory } = await (await import("../pkg/labcolors.js")).default();
const { outputTable: fixture } = JSON.parse(
  readFileSync(new URL("../../../conformance/program-runtime-v1.json", import.meta.url), "utf8"),
);

for (const attached of [false, true]) {
  test(`${attached ? "attached" : "standalone"} output copy failures release Rust storage and recover`, () => {
    const row = fixture.rows[0];
    const owner = attached
      ? api.attachProgramWire(Uint8Array.from(Buffer.from(row.wire, "hex")), 1,
        row.slot, 501, 71, 61, () => true)
      : api.compileProgramWire(Uint8Array.from(Buffer.from(fixture.wire, "hex")), 1);
    const snapshot = owner.updateObserved(1n, new Uint32Array([101]),
      new Uint8Array([255, 255, 255]), 1);
    const species = Object.getOwnPropertyDescriptor(NativeFloat64Array, Symbol.species);
    try {
      assert.equal(snapshot.state, "ready");
      const healthy = snapshot.copyOutputTable();
      assert.ok(healthy instanceof NativeFloat64Array);
      assert.ok(healthy.byteLength > 0);

      // This also exercises the former generated .slice() copier with exactly
      // the same allocation refusal; subarray's borrowed views remain usable.
      Object.defineProperty(NativeFloat64Array, Symbol.species, {
        configurable: true,
        value: OutputArray,
      });
      refuseCopy = true;
      let throws = 0;
      let typedErrors = 0;
      const failCopy = () => {
        try {
          snapshot.copyOutputTable();
        } catch (error) {
          throws += 1;
          if (api.isProgramError(error) && error.code === "program_resource_exhausted"
            && error.operation === "copyOutputTable") typedErrors += 1;
        }
      };
      const refusedBefore = refusedCopies;
      const warmup = 64;
      for (let index = 0; index < warmup; index += 1) failCopy();
      const bytesBefore = memory.buffer.byteLength;
      // Leaking every table would exceed the entire current linear memory
      // twice. A freed temporary buffer can serve every synchronous attempt.
      const attempts = Math.ceil(2 * bytesBefore / healthy.byteLength);
      for (let index = 0; index < attempts; index += 1) failCopy();
      assert.equal(refusedCopies - refusedBefore, warmup + attempts,
        "every attempted copy must reach the controlled allocation failure");
      assert.equal(throws, warmup + attempts);
      assert.equal(memory.buffer.byteLength, bytesBefore,
        "failed JS copies must not accumulate owned Rust output buffers");
      assert.equal(typedErrors, throws);

      refuseCopy = false;
      assert.equal(snapshot.state, "ready");
      const recovered = snapshot.copyOutputTable();
      assert.deepEqual(recovered, healthy);
      assert.notEqual(recovered.buffer, healthy.buffer);
      for (let index = 0; index < snapshot.outputCount(); index += 1) {
        assert.equal(recovered[index * 5], snapshot.outputSlot(index));
        assert.deepEqual([...recovered.subarray(index * 5 + 1, index * 5 + 4)],
          [...snapshot.outputRgb(index)]);
        assert.equal(recovered[index * 5 + 4], snapshot.outputOpacity(index));
      }
    } finally {
      refuseCopy = false;
      if (species === undefined) delete NativeFloat64Array[Symbol.species];
      else Object.defineProperty(NativeFloat64Array, Symbol.species, species);
      snapshot.free();
      if (attached) owner.dispose(true);
      owner.free();
    }
  });
}
