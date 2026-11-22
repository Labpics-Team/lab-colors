import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV3()
  .computedTarget(2, { kind: "gray", lower: 0, upper: 255 }, [150, 150, 150], 1)
  .computedTarget(12, { kind: "gray", lower: 0, upper: 255 }, [160, 160, 160], 1)
  .assignment([2, 12]).surfaceInputPort(1).inputSurface(3, 1)
  .solidPaint(4, 2).sourceOverOccurrence(5, 4, 3, 64, 0.2, 1)
  .occurrenceSurface(13, 5).solidPaint(14, 12)
  .sourceOverOccurrence(15, 14, 13, 64, 0.2, 1)
  .presentationRoot(8, 15).presentationTarget(8, 15)
  .wcag22VisibleUnary(true, 6, 15, 3).output(7, 4).output(17, 14).finish();
const ids = new Uint32Array([9]);
const white = new Uint8Array([255, 255, 255]);
const black = new Uint8Array([0, 0, 0]);
const programError = (operation, code) => error =>
  isProgramError(error) && error.operation === operation && error.code === code;

test("host accepts one complete snapshot and rejected proposals preserve the accepted context", () => {
  const runtime = compileProgramWire(wire, 8);
  const owned = new Set();
  let host = { background: white, colors: null };
  try {
    let calls = 0;
    const initial = runtime.updateObservedWith(1n, ids, white, 1, snapshot => {
      owned.add(snapshot);
      calls += 1;
      assert.equal(snapshot.state, "ready");
      assert.equal(snapshot.revision, 1n);
      assert.equal(snapshot.assessmentReport().revision, 1n);
      const colors = snapshot.copyOutputTable();
      assert.equal(colors.length, 10);
      host = { background: white, colors };
      return true;
    });
    assert.equal(owned.has(initial), true, "return the same owned snapshot passed to the callback");
    assert.equal(calls, 1);
    const before = host;
    let rejected;
    assert.equal(runtime.updateObservedWith(2n, ids, black, 1, snapshot => {
      owned.add(snapshot);
      rejected = snapshot;
      assert.equal(snapshot.assessmentReport().revision, 2n);
      return false;
    }), undefined);
    assert.equal(host, before);
    const replay = runtime.updateObserved(1n, ids, white, 1);
    owned.add(replay);
    assert.deepEqual(replay.copyOutputTable(), initial.copyOutputTable());
    const accepted = runtime.updateObservedWith(2n, ids, black, 1, snapshot => {
      owned.add(snapshot);
      host = { background: black, colors: snapshot.copyOutputTable() };
      return true;
    });
    assert.equal(host.background, black);
    assert.deepEqual(accepted.copyOutputTable(), rejected.copyOutputTable());
    assert.equal(initial.revision, 1n);
    assert.equal(initial.assessmentReport().revision, 1n);
    assert.equal(rejected.revision, 2n, "retained rejected candidates remain historical data");
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("callback throws or returns a non-boolean before revision commit", () => {
  const runtime = compileProgramWire(wire, 8);
  const owned = new Set();
  const failure = new Error("host write failed");
  try {
    assert.throws(() => runtime.updateObservedWith(1n, ids, white, 1, snapshot => {
      owned.add(snapshot);
      throw failure;
    }), error => error === failure);
    for (const value of [undefined, null, 1, "true", {}, Promise.resolve(true)]) {
      assert.throws(() => runtime.updateObservedWith(1n, ids, black, 1, snapshot => {
        owned.add(snapshot);
        return value;
      }), programError("updateObservedWith", "program_host_protocol"));
    }
    const retry = runtime.updateObservedWith(1n, ids, white, 1, snapshot => {
      owned.add(snapshot);
      return true;
    });
    assert.equal(retry.revision, 1n);
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("callback cannot replace or free its runtime or free its in-flight snapshot", () => {
  const runtime = compileProgramWire(wire, 8);
  let accepted;
  try {
    accepted = runtime.updateObservedWith(1n, ids, white, 1, snapshot => {
      for (const [operation, invoke] of [
        ["updateObserved", () => runtime.updateObserved(2n, ids, black, 1)],
        ["updateObservedWith", () => runtime.updateObservedWith(2n, ids, black, 1, () => true)],
        ["updateUnknown", () => runtime.updateUnknown(2n, 1)],
        ["runtimeFree", () => runtime.free()],
        ["snapshotFree", () => snapshot.free()],
      ]) assert.throws(invoke, programError(operation, "program_busy"));
      if (Symbol.dispose) {
        assert.throws(() => runtime[Symbol.dispose](), programError("runtimeFree", "program_busy"));
        assert.throws(() => snapshot[Symbol.dispose](), programError("snapshotFree", "program_busy"));
      }
      assert.equal(snapshot.outputCount(), 2);
      return true;
    });
    assert.equal(accepted.state, "ready");
    assert.equal(accepted.outputCount(), 2);
  } finally {
    accepted?.free();
    runtime.free();
  }
});

test("invalid input never invokes the host and does not consume its revision", () => {
  const runtime = compileProgramWire(wire, 8);
  let calls = 0;
  let accepted;
  try {
    const accept = () => { calls += 1; return true; };
    for (const invoke of [
      () => runtime.updateObservedWith(1n, ids, new Uint8Array(2), 1, accept),
      () => runtime.updateObservedWith(1n, ids, white, 1, undefined),
      () => runtime.updateObservedWith(-1n, ids, white, 1, accept),
      () => runtime.updateObservedWith(1n, new Uint32Array(), new Uint8Array(), 1, accept),
    ]) assert.throws(invoke, programError("updateObservedWith", "program_update"));
    assert.equal(calls, 0);
    accepted = runtime.updateObservedWith(1n, ids, white, 1, accept);
    assert.equal(calls, 1);
    assert.equal(accepted.revision, 1n);
  } finally {
    accepted?.free();
    runtime.free();
  }
});
