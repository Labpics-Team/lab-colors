import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire } from "../index.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

test("reference evidence is the same owned projection in the lightweight accessor and full report", () => {
  const wire = readFileSync(new URL("../../../crates/labcolors-core/tests/fixtures/reference-hierarchy.lcpw", import.meta.url));
  const runtime = compileProgramWire(wire, 1), snapshots = [];
  try {
    const original = runtime.updateObserved(1n, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1);
    snapshots.push(original);
    assert.equal(original.state, "ready");
    const selections = original.referenceSelections(), report = original.assessmentReport();
    assert.deepEqual(selections, report.referenceSelections);
    assert.equal(selections.length, 2);
    assert.ok(Object.isFrozen(selections));
    assert.ok(Object.isFrozen(selections[0].capacity));
    assert.ok(Object.isFrozen(selections[0].cases));
    assert.equal(selections[0].targetId, 10);
    assert.equal(selections[0].occurrenceId, 5);
    const pair = report.tonalHierarchy[0];
    assert.equal(pair.requiredRatio, pair.actualRatio);
    assert.equal(pair.requiredGap, Math.log(pair.requiredRatio));
    const black = runtime.updateObserved(2n, new Uint32Array([1]), new Uint8Array([0, 0, 0]), 1);
    snapshots.push(black);
    assert.equal(black.state, "ready");
    assert.deepEqual(original.referenceSelections(), selections);
    const unknown = runtime.updateUnknown(3n, 7);
    snapshots.push(unknown);
    assert.equal(unknown.referenceSelections(), null);
    original.free(); snapshots[0] = null;
    assert.deepEqual(selections[0].cases[0].referenceBackdrop, [255, 255, 255]);
  } finally {
    for (const snapshot of snapshots) snapshot?.free();
    runtime.free();
  }
});

test("numeric occurrence IDs cannot change repair of a colored reference hierarchy", () => {
  const anchors = [[37, 99, 235], [217, 119, 6], [124, 58, 237], [13, 148, 136], [8, 21, 40], [20, 37, 59]];
  const renamings = [
    (i) => 500 + i,
    (i) => 5000 + 37 * i,
    (i) => 1000 - i,
    (i) => 500 + (i < 4 ? i : 9 - i),
  ];
  for (const priority of [[0, 1, 2, 3, 4, 5], [4, 5, 3, 2, 1, 0]]) {
    let expected;
    for (const occurrence of renamings) {
      const builder = new ProgramWireBuilderV3().surfaceInputPort(1).inputSurface(2, 1);
      for (const [i, anchor] of anchors.entries()) {
        builder.colorTarget(100 + i, anchor, { referenceAppearance: occurrence(i) }).solidPaint(200 + i, 100 + i);
        if (i < 4) builder.opacityInput(400 + i, i % 2 ? 0.1 : 0.12).opacityPaint(300 + i, 200 + i, 400 + i);
        builder.pointOccurrence(occurrence(i), (i < 4 ? 300 : 200) + i, i === 0 ? 2 : 600 + Math.min(i - 1, 3));
        if (i < 4) builder.occurrenceSurface(600 + i, occurrence(i));
        else builder.wcag22VisibleUnary(true, 700 + i, occurrence(i), 1);
        builder.output(800 + i, (i < 4 ? 300 : 200) + i);
      }
      builder.preserveTonalHierarchy(900, [occurrence(4), occurrence(5)]);
      builder.assignment(priority.map((i) => 100 + i), { reference: [[1, [246, 244, 240]]] });
      const runtime = compileProgramWire(builder.finish(), 1), frames = [];
      try {
        for (const [i, background] of [[246, 244, 240], [63, 36, 63], [246, 244, 240]].entries()) {
          const snapshot = runtime.updateObserved(BigInt(i + 1), new Uint32Array([1]), new Uint8Array(background), 1);
          try {
            assert.equal(snapshot.state, "ready", `priority ${priority}, occurrence IDs ${anchors.map((_, i) => occurrence(i))}`);
            frames.push({
              optimality: snapshot.optimality,
              colors: Array.from({ length: snapshot.outputCount() }, (_, j) =>
                [snapshot.outputSlot(j), ...snapshot.outputRgb(j), snapshot.outputOpacity(j)]),
            });
          } finally { snapshot.free(); }
        }
        assert.deepEqual(frames[2], frames[0]);
        if (expected) assert.deepEqual(frames, expected, "only nominal IDs changed");
        else expected = frames;
      } finally { runtime.free(); }
    }
  }
});
