import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "@labpics/colors";
import {
  ProgramWireBuilderV1, SURROUND_AVERAGE_V1, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
} from "@labpics/colors/program-wire/abi-v1.js";

await init({
  module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))),
});

function outputs(snapshot) {
  const result = new Map();
  for (let index = 0; index < snapshot.outputCount(); index += 1) {
    const slot = snapshot.outputSlot(index);
    assert.ok(!result.has(slot), "output slots must be unique");
    result.set(slot, { rgb: [...snapshot.outputRgb(index)], opacity: snapshot.outputOpacity(index) });
  }
  return result;
}

function independentGraph(reportOnly) {
  const branches = Array.from({ length: 7 }, (_, index) => ({
    id: 11 + index * 17,
    port: 701 - index * 19,
    slot: 901 - index * 13,
    rgb: Array(3).fill(index % 2 === 0 ? 0 : 255),
    backdrop: Array(3).fill(index % 2 === 0 ? 255 : 0),
  }));
  const builder = new ProgramWireBuilderV1();
  // Порядок деклараций намеренно не совпадает с canonical порядком inputs/outputs.
  for (const { id, port, slot, rgb } of [...branches].reverse()) {
    builder.source(id, rgb).fixedTarget(id, id).surfaceInputPort(port)
      .solidPaint(id, id).inputSurface(id, port)
      .sourceOverOccurrence(id, id, id, 64, 0.2, SURROUND_AVERAGE_V1)
      .presentationRoot(id, id).presentationTarget(id, id)
      .wcag22VisibleUnary(true, id, id, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
      .output(slot, id);
    if (reportOnly) builder.exactVisibleUnary(false, 1001 + id, id, rgb.map((channel) => 255 - channel));
  }
  branches.sort((a, b) => a.port - b.port);
  return {
    wire: builder.finish(), branches,
    expected: new Map(branches.map(({ slot, rgb }) => [slot, { rgb, opacity: 1 }])),
  };
}

for (const reportOnly of [false, true]) {
  test(`one changed surface invalidates its contrast with unchanged visible RGB, report-only=${reportOnly}`, () => {
    const { wire, branches, expected } = independentGraph(reportOnly);
    let runtime = compileProgramWire(wire, 13);
    const held = [];
    const rows = [0, 1].map(() => branches.flatMap(({ backdrop }) => backdrop));
    const ids = new Uint32Array([29, 7]);
    let revision = 0n;
    const observe = (state) => {
      const snapshot = runtime.updateObserved(++revision, ids, new Uint8Array(rows.flat()), branches.length);
      held.push({ snapshot, state });
      assert.equal(snapshot.state, state);
      assert.deepEqual(outputs(snapshot), expected);
      return snapshot;
    };
    try {
      const initial = observe("ready");
      observe("ready");
      for (const [index, branch] of branches.entries()) {
        const row = rows[index % rows.length];
        // Opaque source не меняется. Только фон переключает точный WCAG
        // результат между 21:1 и 1:1; visible-RGB cache здесь недостаточен.
        row.splice(index * 3, 3, ...branch.rgb);
        observe("failed");
        assert.equal(initial.state, "ready");
        assert.deepEqual(outputs(initial), expected);
        row.splice(index * 3, 3, ...branch.backdrop);
        observe("ready");
      }
      const unknown = runtime.updateUnknown(++revision, 1);
      held.push({ snapshot: unknown, state: "stale" });
      assert.equal(unknown.state, "stale");
      assert.deepEqual(outputs(unknown), expected);
      rows[1].splice(0, 3, ...branches[0].rgb);
      observe("failed");
      rows[1].splice(0, 3, ...branches[0].backdrop);
      observe("ready");
      runtime.free();
      runtime = undefined;
      for (const { snapshot, state } of held) {
        assert.equal(snapshot.state, state);
        assert.deepEqual(outputs(snapshot), expected);
      }
    } finally {
      for (const { snapshot } of held.reverse()) snapshot.free();
      runtime?.free();
    }
  });
}

function nestedGraph(depth) {
  const builder = new ProgramWireBuilderV1()
    .source(11, [0, 0, 0]).fixedTarget(21, 11).surfaceInputPort(31)
    .opacityInput(32, 0.25).solidPaint(41, 21).opacityPaint(42, 41, 32)
    .inputSurface(51, 31);
  for (let layer = 0; layer < depth; layer += 1) {
    builder.sourceOverOccurrence(61 + layer, 42, 51 + layer, 64, 0.2, SURROUND_AVERAGE_V1)
      .occurrenceSurface(52 + layer, 61 + layer);
  }
  const terminal = 61 + depth;
  builder.sourceOverOccurrence(terminal, 41, 51 + depth, 64, 0.2, SURROUND_AVERAGE_V1)
    .presentationRoot(71, terminal).presentationTarget(71, terminal)
    .wcag22VisibleUnary(true, 81, terminal, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
    .output(91, 41);
  return { wire: builder.finish(), terminal };
}

for (const depth of [1, 3]) {
  test(`nested backdrop changes propagate through ${depth} layers despite the same opaque terminal RGB`, () => {
    const { wire, terminal } = nestedGraph(depth);
    const host = { point: null, operations: [] };
    let attachment = attachProgramWire(wire, 17, 91, 501, 71, terminal, (intent) => {
      host.operations.push(intent.operation);
      host.point = intent.point;
      return true;
    });
    const held = [];
    const observe = (revision, level) => {
      const snapshot = attachment.updateObserved(revision, new Uint32Array([1]),
        new Uint8Array([level, level, level]), 1);
      held.push(snapshot);
      return snapshot;
    };
    try {
      const initial = observe(1n, 255);
      assert.equal(initial.state, "ready");
      const authority = attachment.materializationAuthority();
      held.push(authority);
      assert.deepEqual([...authority.terminalCompositeRgb()], [0, 0, 0]);
      assert.equal(observe(2n, 254).state, "ready");
      const failed = observe(3n, 0);
      assert.equal(failed.state, "failed");
      assert.equal(host.point, null);
      assert.equal(host.operations.at(-1), "revokeAll");
      assert.throws(() => attachment.materializationAuthority(), (error) =>
        isProgramError(error) && error.code === "program_materialization_not_ready");
      const recovered = observe(4n, 255);
      assert.equal(recovered.state, "ready");
      assert.deepEqual(outputs(recovered), new Map([[91, { rgb: [0, 0, 0], opacity: 1 }]]));
      assert.equal(initial.state, "ready");
      assert.equal(failed.state, "failed");
      attachment.dispose(true);
      attachment.free();
      attachment = undefined;
      assert.equal(initial.state, "ready");
      assert.deepEqual(outputs(initial), outputs(recovered));
      assert.deepEqual([...authority.terminalCompositeRgb()], [0, 0, 0]);
      assert.equal(authority.revision(), 1n);
    } finally {
      for (const handle of held.reverse()) handle.free();
      if (attachment !== undefined) {
        attachment.dispose(true);
        attachment.free();
      }
    }
  });
}
