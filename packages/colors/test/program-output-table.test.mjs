import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire } from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "@labpics/colors/program-wire/abi-v2.js";

await init({
  module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))),
});

const values = [
  { slot: 0xffff_ffff, rgb: [0, 1, 255], opacity: 0.1 },
  { slot: 17, rgb: [254, 128, 2], opacity: 1 / 3 },
  { slot: 0x8000_0001, rgb: [7, 251, 129], opacity: 1 - Number.EPSILON },
];

function fixedGraph(rows) {
  const builder = new ProgramWireBuilderV1().surfaceInputPort(31).inputSurface(51, 31);
  for (const [index, { slot, rgb, opacity }] of rows.entries()) {
    const id = (value) => value + index * 100;
    builder.source(id(11), rgb).fixedTarget(id(21), id(11))
      .opacityInput(id(32), opacity).solidPaint(id(41), id(21))
      .opacityPaint(id(42), id(41), id(32))
      .sourceOverOccurrence(id(61), id(42), 51, 64, 0.2, 1)
      .presentationRoot(id(71), id(61)).presentationTarget(id(71), id(61))
      .exactVisibleUnary(false, id(81), id(61), rgb)
      .output(slot, id(42));
  }
  return builder.finish();
}

function observe(owner, revision, backdrop) {
  return owner.updateObserved(revision, new Uint32Array([101]), new Uint8Array(backdrop), 1);
}

function assertTable(snapshot, state, rows) {
  assert.equal(snapshot.state, state);
  const table = snapshot.copyOutputTable();
  assert.ok(table instanceof Float64Array);
  assert.equal(table.length, rows.length * 5);
  assert.equal(snapshot.outputCount(), rows.length);
  const expected = new Map(rows.map(({ slot, rgb, opacity }) => [slot, [...rgb, opacity]]));
  for (let index = 0; index < rows.length; index += 1) {
    const offset = index * 5;
    const slot = table[offset];
    assert.equal(slot, snapshot.outputSlot(index));
    assert.deepEqual([...table.subarray(offset + 1, offset + 4)], [...snapshot.outputRgb(index)]);
    assert.equal(table[offset + 4], snapshot.outputOpacity(index));
    assert.deepEqual([...table.subarray(offset + 1, offset + 5)], expected.get(slot));
    assert.equal(expected.delete(slot), true, "each declared output appears once");
  }
  assert.equal(expected.size, 0);
  return table;
}

test("output table preserves every opaque slot, channel and f64 opacity in getter order", () => {
  const runtime = compileProgramWire(fixedGraph(values), 1);
  let snapshot, table, copy;
  try {
    snapshot = observe(runtime, 1n, [255, 255, 255]);
    table = assertTable(snapshot, "ready", values);
    copy = assertTable(snapshot, "ready", values);
    assert.notEqual(table.buffer, copy.buffer);
    table.fill(Number.NaN);
    assert.deepEqual(assertTable(snapshot, "ready", values), copy);
  } finally {
    snapshot?.free();
    runtime.free();
  }
  assert.ok(table.every(Number.isNaN));
  assert.equal(copy.length, values.length * 5);
  assert.ok(copy.every(Number.isFinite));
});

for (const row of values) {
  test(`attached output table preserves slot ${row.slot} and its exact opacity`, () => {
    const host = { point: null };
    const attachment = attachProgramWire(fixedGraph([row]), 1, row.slot, 501, 71, 61, (intent) => {
      host.point = intent.point;
      return true;
    });
    let snapshot, table;
    try {
      snapshot = observe(attachment, 1n, [255, 255, 255]);
      table = assertTable(snapshot, "ready", [row]);
      const copy = assertTable(snapshot, "ready", [row]);
      assert.notEqual(table.buffer, copy.buffer);
      copy.fill(Number.NaN);
      assert.deepEqual(assertTable(snapshot, "ready", [row]), table);
    } finally {
      snapshot?.free();
      host.point = null;
      attachment.dispose(true);
      attachment.free();
    }
    assert.deepEqual([...table], [row.slot, ...row.rgb, row.opacity]);
  });
}

const adaptiveSlot = 0xffff_fff1;
const dark = { slot: adaptiveSlot, rgb: [0, 0, 0], opacity: 0.25 };
const light = { slot: adaptiveSlot, rgb: [255, 255, 255], opacity: 0.75 };
const adaptiveWire = new ProgramWireBuilderV2()
  .finiteTarget(21, [dark, light].map(({ rgb, opacity }, index) => ({ id: 201 + index, rgb, opacity })))
  .selectionRelease(1n, [201, 202].map((candidate, index) => [
    { id: index + 1, choices: [{ target: 21, candidate }] },
  ]))
  .surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
  .sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  // Both authored choices produce 191 over their own background after half-up
  // rounding; neither does over 128. The expected rows do not reuse the solver.
  .exactVisibleUnary(true, 81, 61, [191, 191, 191]).output(adaptiveSlot, 41).finish();

for (const attached of [false, true]) {
  test(`${attached ? "attached" : "standalone"} tables survive updates, failure, staleness and owner disposal`, () => {
    const host = { point: null };
    const owner = attached
      ? attachProgramWire(adaptiveWire, 1, adaptiveSlot, 501, 71, 61, (intent) => {
        host.point = intent.point;
        return true;
      })
      : compileProgramWire(adaptiveWire, 1);
    const snapshots = [];
    const hold = (snapshot) => { snapshots.push(snapshot); return snapshot; };
    let firstTable, nextTable;
    try {
      const waiting = hold(owner.updateUnknown(1n, 7));
      assertTable(waiting, "waiting", []);
      const first = hold(observe(owner, 2n, [255, 255, 255]));
      firstTable = assertTable(first, "ready", [dark]);
      const next = hold(observe(owner, 3n, [0, 0, 0]));
      nextTable = assertTable(next, "ready", [light]);
      assert.notDeepEqual(nextTable, firstTable, "the context must change both RGB and opacity");
      assert.notEqual(nextTable.buffer, firstTable.buffer);
      const failed = hold(observe(owner, 4n, [128, 128, 128]));
      assertTable(failed, "failed", [light]);
      if (attached) assert.equal(host.point, null);
      const stale = hold(owner.updateUnknown(5n, 8));
      assertTable(stale, "stale", [light]);
      const recovered = hold(observe(owner, 6n, [255, 255, 255]));
      assertTable(recovered, "ready", [dark]);
      assert.deepEqual(assertTable(first, "ready", [dark]), firstTable);
      assert.deepEqual(assertTable(next, "ready", [light]), nextTable);
      const modified = assertTable(failed, "failed", [light]);
      modified.fill(-1);
      assert.deepEqual(assertTable(stale, "stale", [light]), nextTable);
      assert.deepEqual(assertTable(failed, "failed", [light]), nextTable);
    } finally {
      for (const snapshot of snapshots.reverse()) snapshot.free();
      if (attached) {
        host.point = null;
        owner.dispose(true);
      }
      owner.free();
    }
    assert.deepEqual([...firstTable], [adaptiveSlot, ...dark.rgb, dark.opacity]);
    assert.deepEqual([...nextTable], [adaptiveSlot, ...light.rgb, light.opacity]);
  });
}
