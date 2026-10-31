import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "../program-wire/abi-v2.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

const fixture = JSON.parse(readFileSync(new URL("../../../conformance/program-runtime-v1.json", import.meta.url), "utf8"));
assert.equal(fixture.schemaVersion, 1);
await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const bytes = (key) => new Uint8Array(Buffer.from(fixture[key], "hex"));
const graph = (builder) => builder.surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
  .sourceOverOccurrence(61, 41, 51, 64, .2, 1).presentationRoot(71, 61).presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, 1).output(91, 41);

test("native and Web execute the same versioned runtime history", () => {
  const runtime = compileProgramWire(bytes("adaptive"), 1), held = [];
  try {
    for (const step of fixture.history) {
      const snapshot = step.unknownReason === undefined
        ? runtime.updateObserved(BigInt(step.revision), new Uint32Array(step.backgrounds.map((_, i) => i + 1)), new Uint8Array(step.backgrounds.flat()), 1)
        : runtime.updateUnknown(BigInt(step.revision), step.unknownReason);
      held.push(snapshot); assert.equal(snapshot.state, step.state);
      assert.equal(snapshot.snapshotVersion, 1); assert.equal(snapshot.revision, BigInt(step.revision)); assert.equal(snapshot.reason, undefined);
      if (step.rgb) {
        assert.equal(snapshot.outputCount(), 1); assert.equal(snapshot.outputSlot(0), 91);
        assert.deepEqual([...snapshot.outputRgb(0)], step.rgb); assert.equal(snapshot.outputOpacity(0), 1);
      }
    }
    assert.deepEqual([...held[0].outputRgb(0)], [0, 0, 0]);
  } finally { for (const snapshot of held.reverse()) snapshot.free(); runtime.free(); }
});

test("native authoring fixtures match both real Web builders byte for byte", () => {
  assert.deepEqual(graph(new ProgramWireBuilderV1().source(11, [20, 20, 20]).fixedTarget(21, 11)).finish(), bytes("fixed"));
  const adaptive = new ProgramWireBuilderV2().finiteTarget(21, [
    { id: 201, rgb: [0, 0, 0], opacity: 1 }, { id: 202, rgb: [255, 255, 255], opacity: 1 },
  ]).selectionRelease(1n, [
    [{ id: 1, choices: [{ target: 21, candidate: 201 }] }], [{ id: 2, choices: [{ target: 21, candidate: 202 }] }],
  ]);
  assert.deepEqual(graph(adaptive).finish(), bytes("adaptive"));
  const all = new ProgramWireBuilderV1().source(11, [1, 2, 3]).fixedTarget(21, 11)
    .finiteTarget(22, [{ id: 201, rgb: [0, 0, 0], opacity: .5 }, { id: 202, rgb: [255, 255, 255], opacity: 1 }])
    .family(90, Array(32).fill(0)).surfaceInputPort(31).opacityInput(32, .5)
    .solidPaint(41, 21).opacityPaint(42, 41, 32).inputSurface(51, 31).occurrenceSurface(52, 61)
    .sourceOverOccurrence(61, 42, 51, 64, .2, 2).presentationRoot(71, 61).presentationTarget(71, 61)
    .exactVisibleUnary(true, 81, 61, [1, 2, 3]).wcag22VisibleUnary(false, 82, 61, 3)
    .declaredSrgb8CleanSet(true, 83, 71, 61).exactIntrinsicRelationHard(84, 11, [11]).output(91, 42);
  assert.deepEqual(all.finish(), bytes("encodingOnly"));
  const computed = fixture.computed;
  assert.deepEqual(new ProgramWireBuilderV3()
    .computedTarget(0x10203040, { kind: "box", lower: [1, 2, 3], upper: [4, 5, 6] }, [250, 0, 7], 0.25)
    .computedTarget(19, { kind: "gray", lower: 10, upper: 12 }, [9, 8, 7], 8)
    .assignment([0x10203040, 19]).finish(), new Uint8Array(Buffer.from(computed.encodingOnly, "hex")));
  for (const domain of [{ kind: "gray", lower: 0, upper: 255 }, { kind: "box", lower: [0, 0, 0], upper: [255, 255, 255] }]) {
    const wire = new ProgramWireBuilderV3().computedTarget(21, domain, [64, 64, 64], 1).assignment([21])
      .surfaceInputPort(31).opacityInput(32, .5).solidPaint(41, 21).opacityPaint(42, 41, 32).inputSurface(51, 31)
      .sourceOverOccurrence(61, 42, 51, 64, .2, 2).presentationRoot(71, 61).presentationTarget(71, 61)
      .exactVisibleUnary(true, 81, 61, [96, 96, 96]).output(91, 42).finish();
    assert.deepEqual(wire, new Uint8Array(Buffer.from(computed[domain.kind], "hex")));
  }
});

test("native and Web V3 snapshots distinguish current outputs from retained history", () => {
  for (const domain of ["gray", "box"]) {
    const runtime = compileProgramWire(new Uint8Array(Buffer.from(fixture.computed[domain], "hex")), 0xffff_ffff), held = [];
    try {
      const waiting = runtime.updateUnknown(0n, 0xffff_ffff);
      try {
        assert.equal(waiting.snapshotVersion, 2); assert.equal(waiting.revision, 0n);
        assert.equal(waiting.state, "waiting"); assert.equal(waiting.reason, undefined); assert.equal(waiting.outputCount(), 0);
      } finally { waiting.free(); }
      for (const step of fixture.computed[domain + "History"]) {
        const snapshot = step.unknownReason === undefined
          ? runtime.updateObserved(BigInt(step.revision), new Uint32Array([1]), new Uint8Array(step.backgrounds.flat()), 1)
          : runtime.updateUnknown(BigInt(step.revision), step.unknownReason);
        held.push(snapshot);
        assert.equal(snapshot.snapshotVersion, 2); assert.equal(snapshot.revision, BigInt(step.revision));
        assert.equal(snapshot.state, step.state); assert.equal(snapshot.reason, step.reason);
        if (step.rgb) {
          assert.equal(snapshot.outputCount(), 1); assert.equal(snapshot.outputSlot(0), 91);
          assert.deepEqual([...snapshot.outputRgb(0)], step.rgb); assert.equal(snapshot.outputOpacity(0), .5);
        } else {
          assert.equal(snapshot.outputCount(), 0);
          assert.throws(() => snapshot.outputRgb(0), error => error.code === "invalid_output_index");
        }
      }
      assert.deepEqual([...held[0].outputRgb(0)], [64, 64, 64]); assert.equal(held[0].revision, 1n);
      const high = runtime.updateObserved(0xffff_ffff_ffff_ffffn, new Uint32Array([1]), new Uint8Array([128, 128, 128]), 1);
      try { assert.equal(high.revision, 0xffff_ffff_ffff_ffffn); assert.equal(high.state, "ready"); }
      finally { high.free(); }
    } finally { for (const snapshot of held.reverse()) snapshot.free(); runtime.free(); }
  }
});
