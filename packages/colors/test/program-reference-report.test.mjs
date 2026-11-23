import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire } from "../index.js";

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
