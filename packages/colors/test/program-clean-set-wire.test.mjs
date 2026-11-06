import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "../index.js";
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
  { name: "accepted opaque final", source: [0, 199, 63], opacity: 1,
    backdrop: [0, 0, 0], final: [0, 199, 63], state: "ready" },
  { name: "rejected opaque final", source: [0, 199, 64], opacity: 1,
    backdrop: [0, 0, 0], final: [0, 199, 64], state: "failed" },
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
  const source = [0, 199, 64];
  const builder = pointBuilder(source, 1, source).declaredSrgb8CleanSet(false, 81, 71, 61);
  assert.deepEqual(resolve(builder, [0, 0, 0]), {
    state: "ready", outputs: [{ slot: 91, rgb: source, opacity: 1 }],
  });
});

test("declared clean-set presentation references remain Core compile obligations", () => {
  const builder = pointBuilder([0, 199, 63], 1, [0, 199, 63])
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

function usingRuntime(builder, run) {
  const runtime = compileProgramWire(builder.finish(), 101);
  const snapshots = [];
  const observed = (revision, ids, surfaces, surfaceCount = 1) => {
    const snapshot = runtime.updateObserved(revision, new Uint32Array(ids), new Uint8Array(surfaces), surfaceCount);
    snapshots.push(snapshot);
    return snapshot;
  };
  const unknown = (revision) => {
    const snapshot = runtime.updateUnknown(revision, 99);
    snapshots.push(snapshot);
    return snapshot;
  };
  try { run({ observed, unknown }); }
  finally {
    for (const snapshot of snapshots) snapshot.free();
    runtime.free();
  }
}

test("public report retains nominal rejection and provenance without veto or mutation", () => {
  const source = [0, 199, 64];
  const builder = pointBuilder(source, 1, source).declaredSrgb8CleanSet(false, 81, 71, 61);
  usingRuntime(builder, ({ observed }) => {
    const snapshot = observed(1n, [9, 7], [0, 0, 0, 0, 0, 0]);
    const report = snapshot.assessmentReport();
    assert.equal(snapshot.state, "ready");
    assert.deepEqual([...snapshot.outputRgb(0)], source);
    assert.equal(snapshot.outputOpacity(0), 1);
    assert.equal(report.streamId, 101);
    assert.equal(report.revision, 1n);
    assert.equal(report.contentIdentity.length, 32);
    assert.deepEqual(report.scenarioProvenance, [
      { caseIndex: 0, scenarioId: 7 }, { caseIndex: 0, scenarioId: 9 },
    ]);
    assert.deepEqual(report.cells, [
      { constraintId: 81, caseIndex: 0, hard: false, assessment: {
        kind: "declared-srgb8-clean-set", verdict: "violation", reason: "rejected",
        visible: source, rejectedBlueInterval: [64, 108],
      } },
      { constraintId: 82, caseIndex: 0, hard: true, assessment: {
        kind: "exact-srgb8", verdict: "pass",
      } },
    ]);
    const originalIdentity = [...report.contentIdentity];
    report.contentIdentity.fill(0);
    assert.deepEqual([...snapshot.assessmentReport().contentIdentity], originalIdentity);
    assert.throws(() => { report.cells[0].assessment.visible[0] = 255; }, TypeError);
    const reordered = observed(2n, [7, 9], [0, 0, 0, 0, 0, 0]).assessmentReport();
    assert.deepEqual(reordered.scenarioProvenance, report.scenarioProvenance);
    assert.deepEqual(reordered.cells, report.cells);
  });
});

test("public lint reports the final alpha point and explicit domain absence", () => {
  const examples = [
    { source: [128, 128, 129], opacity: 0.5, backdrop: [128, 128, 127], final: [128, 128, 128],
      expected: { kind: "declared-srgb8-clean-set", verdict: "pass", visible: [128, 128, 128] } },
    { source: [255, 255, 255], opacity: 0.5, backdrop: [1, 1, 3], final: [128, 128, 129],
      expected: { kind: "declared-srgb8-clean-set", verdict: "violation", reason: "rejected",
        visible: [128, 128, 129] } },
    { source: [0, 0, 0], opacity: 1, backdrop: [0, 0, 0], final: [0, 0, 0],
      expected: { kind: "declared-srgb8-clean-set", verdict: "violation", reason: "final-owned-domain-absent" } },
  ];
  for (const example of examples) {
    const builder = pointBuilder(example.source, example.opacity, example.final)
      .declaredSrgb8CleanSet(false, 81, 71, 61);
    usingRuntime(builder, ({ observed }) => {
      const snapshot = observed(1n, [1], example.backdrop);
      assert.equal(snapshot.state, "ready");
      assert.deepEqual([...snapshot.outputRgb(0)], example.source);
      assert.equal(snapshot.outputOpacity(0), example.opacity);
      const assessment = snapshot.assessmentReport().cells[0].assessment;
      if (example.expected.reason === "rejected") {
        const { rejectedBlueInterval, ...summary } = assessment;
        assert.equal(rejectedBlueInterval.length, 2);
        assert.deepEqual(summary, example.expected);
      } else {
        assert.deepEqual(assessment, example.expected);
      }
    });
  }
});

test("report availability rejects incomplete point declarations and distinguishes off, waiting, stale and failed", () => {
  const emptyBuilder = new ProgramWireBuilderV1()
    .source(1, [0, 0, 0]).fixedTarget(2, 1).solidPaint(3, 2).output(17, 3);
  assert.throws(() => compileProgramWire(emptyBuilder.finish(), 101),
    (error) => isProgramError(error) && error.operation === "compileProgramWire"
      && error.code === "program_compile");
  usingRuntime(pointBuilder([0, 199, 63], 1, [0, 199, 63]), ({ observed, unknown }) => {
    const waiting = unknown(1n);
    assert.equal(waiting.state, "waiting");
    assert.equal(waiting.assessmentReport(), null);
    const ready = observed(2n, [1], [0, 0, 0]);
    assert.equal(ready.state, "ready");
    const report = ready.assessmentReport();
    assert.equal(report.cells.length, 1);
    assert.equal(report.cells[0].assessment.kind, "exact-srgb8");
  });
  const builder = pointBuilder([128, 128, 129], 0.5, [128, 128, 128])
    .declaredSrgb8CleanSet(false, 81, 71, 61);
  usingRuntime(builder, ({ observed, unknown }) => {
    const first = observed(1n, [1], [128, 128, 127]);
    const stale = unknown(2n);
    assert.equal(stale.state, "stale");
    assert.equal(stale.outputCount(), 1);
    assert.equal(stale.assessmentReport(), null);
    const fresh = observed(3n, [1], [128, 128, 127]);
    assert.equal(fresh.assessmentReport().revision, 3n);
    const failed = observed(4n, [1], [0, 0, 0]);
    assert.equal(failed.state, "failed");
    assert.equal(failed.outputCount(), 1);
    assert.equal(failed.assessmentReport(), null);
    assert.equal(first.assessmentReport().revision, 1n);
    assert.deepEqual(first.assessmentReport().cells[0].assessment.visible, [128, 128, 128]);
  });
});

test("attached snapshot uses the same final report owner and removes it after revoke", () => {
  const source = [0, 199, 64];
  const builder = pointBuilder(source, 1, source).declaredSrgb8CleanSet(false, 81, 71, 61);
  const intents = [];
  const attachment = attachProgramWire(builder.finish(), 101, 91, 901, 71, 61, (intent) => {
    intents.push(intent);
    return true;
  });
  let ready;
  let stale;
  try {
    ready = attachment.updateObserved(1n, new Uint32Array([1]), new Uint8Array([0, 0, 0]), 1);
    assert.equal(ready.assessmentReport().cells[0].assessment.reason, "rejected");
    assert.deepEqual([...ready.outputRgb(0)], source);
    assert.equal(intents.at(-1).operation, "setAll");
    stale = attachment.updateUnknown(2n, 99);
    assert.equal(stale.assessmentReport(), null);
    assert.equal(intents.at(-1).operation, "revokeAll");
    assert.equal(ready.assessmentReport().revision, 1n);
  } finally {
    stale?.free();
    ready?.free();
    attachment.dispose(true);
    attachment.free();
  }
});

test("report read error classes remain in the existing public error contract", () => {
  for (const code of ["program_resource_exhausted", "program_internal_invariant"]) {
    assert.equal(isProgramError(Object.assign(new Error(code), { code, operation: "assessmentReport" })), true);
  }
  assert.equal(isProgramError(Object.assign(new Error("wrong"), { code: "program_update", operation: "assessmentReport" })), false);
});
