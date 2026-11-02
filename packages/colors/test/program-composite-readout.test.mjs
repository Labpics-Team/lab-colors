import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isBindingError, isProgramError } from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "@labpics/colors/program-wire/abi-v2.js";
import { ProgramWireBuilderV3 } from "@labpics/colors/program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });

function graph({ source = [20, 40, 60], alpha = .5, Builder = ProgramWireBuilderV3,
  second = false, expected, rename = 0 } = {}) {
  const id = (value) => value + rename;
  const b = new Builder();
  if (Builder === ProgramWireBuilderV3) b.computedTarget(id(21),
    { kind: "box", lower: source, upper: source }, source, 0).assignment([id(21)]);
  else b.source(id(11), source).fixedTarget(id(21), id(11));
  b.surfaceInputPort(id(31)).opacityInput(id(32), alpha)
    .solidPaint(id(41), id(21)).opacityPaint(id(42), id(41), id(32))
    .inputSurface(id(51), id(31))
    .sourceOverOccurrence(id(61), id(42), id(51), 64, .2, 1)
    .presentationRoot(id(71), id(61)).presentationTarget(id(71), id(61))
    .output(id(91), id(42));
  if (second) b.source(id(12), [80, 100, 120]).fixedTarget(id(22), id(12))
    .solidPaint(id(43), id(22)).opacityPaint(id(44), id(43), id(32))
    .sourceOverOccurrence(id(62), id(44), id(51), 64, .2, 1)
    .presentationRoot(id(72), id(62)).presentationTarget(id(72), id(62))
    .exactVisibleUnary(false, id(82), id(62), [80, 100, 120]).output(id(92), id(44));
  b.exactVisibleUnary(Boolean(expected), id(81), id(61), expected ?? source);
  return b.finish();
}
const observe = (runtime, revision, rows) => runtime.updateObserved(revision,
  new Uint32Array(rows.map(([id]) => id)), new Uint8Array(rows.flatMap(([, rgb]) => rgb)), 1);
const read = (snapshot, root, scenario) => [...snapshot.presentationCompositeRgb(root, scenario)];
const unavailable = (snapshot, root, scenario) => assert.throws(
  () => snapshot.presentationCompositeRgb(root, scenario),
  (error) => isBindingError(error) && error.operation === "readPresentationComposite"
    && error.code === "presentation_composite_unavailable");

test("normal composite remains readable when the target has no causal contribution", () => {
  for (const Builder of [ProgramWireBuilderV1, ProgramWireBuilderV2, ProgramWireBuilderV3]) {
    for (const alpha of [0, .078, .322, .5, .722, 1]) {
      const source = [39, 75, 147], runtime = compileProgramWire(graph({ source, alpha, Builder }), 7);
      let snapshot;
      try {
        snapshot = observe(runtime, 1n, [[101, source]]);
        assert.equal(snapshot.state, "ready");
        assert.deepEqual(read(snapshot, 71, 101), source);
        assert.deepEqual([...snapshot.outputRgb(0)], source);
        assert.equal(snapshot.outputOpacity(0), alpha);
      } finally { snapshot?.free(); runtime.free(); }
    }
  }
  for (const background of [[0, 0, 0], [255, 255, 255]]) {
    const runtime = compileProgramWire(graph({ alpha: 0 }), 7);
    let snapshot;
    try {
      snapshot = observe(runtime, 1n, [[101, background]]);
      assert.deepEqual(read(snapshot, 71, 101), background);
    } finally { snapshot?.free(); runtime.free(); }
  }
});

test("root and scenario identity survive multiple outputs, physical deduplication and updates", () => {
  for (const rename of [0, 5000]) {
    const runtime = compileProgramWire(graph({ second: true, rename }), 8), held = [];
    try {
      const first = observe(runtime, 1n, [[103, [10, 20, 30]], [101, [100, 120, 140]], [102, [10, 20, 30]]]);
      held.push(first);
      assert.equal(first.outputCount(), 2);
      for (const scenario of [102, 103]) {
        assert.deepEqual(read(first, 71 + rename, scenario), [15, 30, 45]);
        assert.deepEqual(read(first, 72 + rename, scenario), [45, 60, 75]);
      }
      assert.deepEqual(read(first, 71 + rename, 101), [60, 80, 100]);
      assert.deepEqual(read(first, 72 + rename, 101), [90, 110, 130]);
      unavailable(first, 91 + rename, 101);
      unavailable(first, 71 + rename, 999);
      const detached = first.presentationCompositeRgb(71 + rename, 102);
      detached.fill(0);
      assert.deepEqual(read(first, 71 + rename, 102), [15, 30, 45]);
      const next = observe(runtime, 2n, [[102, [0, 0, 0]], [103, [100, 120, 140]]]);
      held.push(next);
      assert.deepEqual(read(next, 71 + rename, 102), [10, 20, 30]);
      assert.deepEqual(read(next, 71 + rename, 103), [60, 80, 100]);
      unavailable(next, 71 + rename, 101);
      const stale = runtime.updateUnknown(3n, 44); held.push(stale);
      assert.equal(stale.state, "stale");
      unavailable(stale, 71 + rename, 102);
      assert.deepEqual(read(first, 71 + rename, 102), [15, 30, 45]);
      assert.equal(first.revision, 1n);
    } finally { for (const snapshot of held.reverse()) snapshot.free(); runtime.free(); }
  }
});

test("waiting and failed snapshots never expose historical composites", () => {
  for (const Builder of [ProgramWireBuilderV1, ProgramWireBuilderV3]) {
    const runtime = compileProgramWire(graph({ Builder, expected: [15, 30, 45] }), 9), held = [];
    try {
      const waiting = runtime.updateUnknown(1n, 2); held.push(waiting);
      unavailable(waiting, 71, 101);
      const first = observe(runtime, 2n, [[101, [10, 20, 30]]]); held.push(first);
      assert.equal(first.state, "ready");
      const failed = observe(runtime, 3n, [[101, [0, 0, 0]]]); held.push(failed);
      assert.notEqual(failed.state, "ready");
      unavailable(failed, 71, 101);
      assert.deepEqual(read(first, 71, 101), [15, 30, 45]);
    } finally { for (const snapshot of held.reverse()) snapshot.free(); runtime.free(); }
  }
});

test("different targets of one nested root read the same normal final composite", () => {
  const wire = new ProgramWireBuilderV1()
    .source(11, [20, 40, 60]).source(12, [80, 100, 120])
    .fixedTarget(21, 11).fixedTarget(22, 12).surfaceInputPort(31).opacityInput(32, .5)
    .solidPaint(41, 21).opacityPaint(42, 41, 32).solidPaint(43, 22).opacityPaint(44, 43, 32)
    .inputSurface(51, 31).occurrenceSurface(52, 61)
    .sourceOverOccurrence(61, 42, 51, 64, .2, 1).sourceOverOccurrence(62, 44, 52, 64, .2, 1)
    .presentationRoot(71, 62).presentationTarget(71, 61).presentationTarget(71, 62)
    .exactVisibleUnary(false, 81, 61, [15, 30, 45])
    .exactVisibleUnary(false, 82, 62, [48, 65, 83]).output(91, 42).output(92, 44).finish();
  const runtime = compileProgramWire(wire, 12);
  let snapshot;
  try {
    snapshot = observe(runtime, 1n, [[777, [10, 20, 30]]]);
    assert.equal(snapshot.state, "ready");
    assert.deepEqual(read(snapshot, 71, 777), [48, 65, 83]);
    unavailable(snapshot, 61, 777);
  } finally { snapshot?.free(); runtime.free(); }
});

test("readout rejects malformed IDs without coercion and is never materialization authority", () => {
  const wire = graph({ source: [39, 75, 147], alpha: 0 });
  const runtime = compileProgramWire(wire, 10);
  const attachment = attachProgramWire(wire, 11, 91, 501, 71, 61, () => true);
  let snapshot, attached;
  try {
    snapshot = observe(runtime, 1n, [[101, [39, 75, 147]]]);
    for (const invalid of [-1, .5, 2 ** 32, NaN, Infinity, "71", 71n, null, undefined,
      { valueOf() { throw new Error("must not coerce"); } }]) {
      for (const pair of [[invalid, 101], [71, invalid]]) assert.throws(
        () => snapshot.presentationCompositeRgb(...pair),
        (error) => isBindingError(error) && error.operation === "readPresentationComposite"
          && error.code === "invalid_composite_id");
    }
    attached = observe(attachment, 1n, [[101, [39, 75, 147]]]);
    assert.equal(attached.state, "ready");
    assert.deepEqual(read(snapshot, 71, 101), [39, 75, 147]);
    assert.throws(() => attachment.materializationAuthority(), (error) => isProgramError(error)
      && error.code === "program_materialization_missing_point_absence_proof");
    assert.equal("materializationAuthority" in snapshot, false);
  } finally { attached?.free(); snapshot?.free(); runtime.free(); attachment.dispose(true); attachment.free(); }
});
