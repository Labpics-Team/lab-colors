import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire } from "../index.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

function runtime(nominalProfile) {
  const b = new ProgramWireBuilderV3()
    .source(1, [255, 255, 255]).fixedTarget(2, 1).surfaceInputPort(3)
    .opacityInput(4, 0.5).solidPaint(5, 2).opacityPaint(6, 5, 4)
    .inputSurface(7, 3).pointOccurrence(8, 6, 7)
    .presentationRoot(9, 8).presentationTarget(9, 8)
    .output(12, 6).assignment([]);
  if (nominalProfile) b.declaredSrgb8CleanSet(false, 11, 9, 8);
  return compileProgramWire(b.finish(), 1);
}

function observe(owner, revision, backdrops) {
  return owner.updateObserved(revision,
    new Uint32Array(backdrops.map((_, i) => i + 1)),
    new Uint8Array(backdrops.flat()), 1);
}

const nominal = (status, accepted, rejected, unavailable) => ({
  status, profile: "nominal-srgb8-v2", basis: "declared-convention",
  scope: "declared-final-points", accepted, rejected, unavailable,
});

test("current readiness without a declared profile does not imply cleanliness conformance", () => {
  const owner = runtime(false);
  let snapshot;
  try {
    snapshot = observe(owner, 1n, [[1, 1, 1]]);
    assert.equal(snapshot.state, "ready");
    assert.deepEqual(snapshot.cleanlinessAssessment(), {
      status: "not-assessed", reason: "no-declared-profile",
    });
    assert.deepEqual(snapshot.assessmentReport().cleanliness, snapshot.cleanlinessAssessment());
  } finally {
    snapshot?.free(); owner.free();
  }
});

test("scoped assessment covers final composites, unknown domains and immutable prior results", () => {
  const owner = runtime(true), held = [];
  try {
    const first = observe(owner, 1n, [[1, 1, 1], [1, 1, 3], [255, 255, 255]]); held.push(first);
    assert.equal(first.state, "ready");
    const summary = first.cleanlinessAssessment();
    assert.deepEqual(summary, nominal("violates", 1, 1, 1));
    assert.ok(Object.isFrozen(summary));
    assert.throws(() => { summary.accepted = 100; }, TypeError);
    assert.deepEqual(first.assessmentReport().cleanliness, summary);

    const absent = observe(owner, 2n, [[255, 255, 255]]); held.push(absent);
    assert.deepEqual(absent.cleanlinessAssessment(), nominal("unavailable", 0, 0, 1));
    const accepted = observe(owner, 3n, [[1, 1, 1], [1, 1, 1]]); held.push(accepted);
    assert.deepEqual(accepted.cleanlinessAssessment(), nominal("conforms", 1, 0, 0));
    assert.equal(accepted.assessmentReport().scenarioProvenance.length, 2);
    assert.deepEqual(first.cleanlinessAssessment(), summary);

    const unknown = owner.updateUnknown(4n, 7); held.push(unknown);
    assert.equal(unknown.cleanlinessAssessment(), null);
    assert.equal(unknown.assessmentReport(), null);
    assert.deepEqual(accepted.cleanlinessAssessment(), nominal("conforms", 1, 0, 0));
  } finally {
    for (const snapshot of held.reverse()) snapshot.free();
    owner.free();
  }
});
