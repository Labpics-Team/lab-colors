import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire } from "../index.js";
import { ProgramWireBuilderV1, PROGRAM_WIRE_INVALID_DECLARATION } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

for (const hard of [true, false]) {
  test(`visible nominal ${hard ? "hard" : "report"} emits the independent additive wire vector`, () => {
    const builder = new ProgramWireBuilderV1();
    assert.equal(builder.nominalSrgb8Visible(hard, 0x12345678, 0x10203040), builder);
    // 14 sections, 75 bytes: constraint id u32 + kind 12 + occurrence u32.
    const expected = "4c43505701004b000000" + "00000000".repeat(hard ? 11 : 12)
      + "01000000785634120c40302010" + "00000000".repeat(hard ? 2 : 1);
    assert.equal(Buffer.from(builder.finish()).toString("hex"), expected);
  });
}

test("visible nominal rejects invalid declaration scalars before mutating the builder", () => {
  const builder = new ProgramWireBuilderV1().nominalSrgb8Visible(true, 7, 8);
  const before = builder.finish();
  for (const args of [
    [1, 7, 8], [true, -1, 8], [false, 7, 0x1_0000_0000], [true, 7, {}],
  ]) {
    assert.throws(() => builder.nominalSrgb8Visible(...args),
      error => error.code === PROGRAM_WIRE_INVALID_DECLARATION);
    assert.deepEqual(builder.finish(), before);
  }
});

function point(source, opacity, hard) {
  return new ProgramWireBuilderV3()
    .source(1, source).fixedTarget(2, 1).surfaceInputPort(3)
    .opacityInput(4, opacity).solidPaint(5, 2).opacityPaint(6, 5, 4)
    .inputSurface(7, 3).pointOccurrence(8, 6, 7)
    .nominalSrgb8Visible(hard, 9, 8).output(10, 6).assignment([]);
}

function resolve(builder, backdrop, check) {
  const owner = compileProgramWire(builder.finish(), 1);
  let snapshot;
  try {
    snapshot = owner.updateObserved(1n, new Uint32Array([1]), new Uint8Array(backdrop), 1);
    check(snapshot);
  } finally {
    snapshot?.free(); owner.free();
  }
}

test("ordered assignment finishes the discrete search for a conforming chromatic color", () => {
  const builder = new ProgramWireBuilderV3()
    .colorTarget(1, [40, 70, 190], { appearance: 5 }).surfaceInputPort(2)
    .solidPaint(3, 1).inputSurface(4, 2).pointOccurrence(5, 3, 4)
    .nominalSrgb8Visible(true, 6, 5).output(7, 3).assignment([1]);
  resolve(builder, [255, 255, 255], (snapshot) => {
    assert.equal(snapshot.state, "ready");
    assert.equal(snapshot.optimality, "exact");
    assert.deepEqual([...snapshot.outputRgb(0)], [46, 79, 200]);
    assert.equal(snapshot.cleanlinessAssessment().status, "conforms");
  });
});

test("visible nominal keeps white and transparent materials without presentation authority", () => {
  for (const opacity of [1, 0.5, 0]) {
    resolve(point([255, 255, 255], opacity, true), [255, 255, 255], snapshot => {
      assert.equal(snapshot.state, "ready");
      assert.deepEqual([...snapshot.outputRgb(0)], [255, 255, 255]);
      assert.equal(snapshot.outputOpacity(0), opacity);
      const summary = snapshot.cleanlinessAssessment();
      assert.equal(summary.status, "conforms");
      assert.equal(summary.accepted, 1);
      assert.equal(summary.unavailable, 0);
      assert.deepEqual(snapshot.assessmentReport().cells[0].assessment, {
        kind: "nominal-srgb8-visible", verdict: "pass", visible: [255, 255, 255],
      });
    });
  }
});

test("visible nominal reports the rejected final composite and the hard condition vetoes it", () => {
  for (const hard of [false, true]) {
    resolve(point([255, 255, 255], 0.5, hard), [1, 1, 3], snapshot => {
      if (hard) {
        assert.equal(snapshot.state, "unsatisfied");
        assert.equal(snapshot.outputCount(), 0);
      } else {
        assert.equal(snapshot.state, "ready");
        assert.deepEqual([...snapshot.outputRgb(0)], [255, 255, 255]);
        assert.equal(snapshot.cleanlinessAssessment().status, "violates");
        const assessment = snapshot.assessmentReport().cells[0].assessment;
        assert.equal(assessment.kind, "nominal-srgb8-visible");
        assert.equal(assessment.verdict, "violation");
        assert.deepEqual(assessment.visible, [128, 128, 129]);
        const [lower, upper] = assessment.rejectedBlueInterval;
        assert.ok(lower <= 129 && upper >= 129);
        assert.ok(Object.isFrozen(assessment));
      }
    });
  }
});
