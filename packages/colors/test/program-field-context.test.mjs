import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV3 } from "../program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV3()
  .source(1, [255, 255, 255]).fixedTarget(2, 1).surfaceInputPort(3)
  .solidPaint(5, 2).inputSurface(7, 3).pointOccurrence(8, 5, 7)
  .presentationRoot(9, 8).presentationTarget(9, 8).output(12, 5).assignment([]).finish();
const bytes = (value) => new Uint8Array(value);
const surfaces = () => bytes([17, 19, 23]);
const from = () => bytes([0, 0, 0]);
const to = () => bytes([2, 0, 0]);
const ids = new Uint32Array([1]);
const programError = (operation, code = "program_update") => error =>
  isProgramError(error) && error.operation === operation && error.code === code;

function rejectsOwned(invoke, error, owned) {
  let unexpected;
  try { assert.throws(() => { unexpected = invoke(); }, error); }
  finally {
    // A defective admission might invoke accept and return that same snapshot.
    // Preserve the original RED assertion without a duplicate free in cleanup.
    if (unexpected !== undefined) {
      owned?.delete(unexpected);
      unexpected?.free?.();
    }
  }
}

test("declared field shape and scalar errors do not invoke accept or consume a revision", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  let calls = 0;
  const accept = snapshot => { calls += 1; owned.add(snapshot); return true; };
  try {
    const initial = runtime.updateDeclaredField(1n, surfaces(), 0, from(), to()); owned.add(initial);
    // Each case targets a distinct coercion/shape boundary; this is not a
    // Cartesian repetition of the established generic typed-array protocol.
    for (const [operation, args] of [
      ["updateDeclaredField", [1, surfaces(), 0, from(), to()]],
      ["updateDeclaredField", [-1n, surfaces(), 0, from(), to()]],
      ["updateDeclaredFieldWith", [(1n << 64n) + 2n, surfaces(), 0, from(), to(), accept]],
      ["updateDeclaredField", [2n, surfaces(), 0.5, from(), to()]],
      ["updateDeclaredField", [2n, surfaces(), "0", from(), to()]],
      ["updateDeclaredFieldWith", [2n, surfaces(), 1, from(), to(), accept]],
      ["updateDeclaredField", [2n, surfaces(), 0, new Uint8Array(2), to()]],
      ["updateDeclaredFieldWith", [2n, surfaces(), 0, from(), new Uint8Array(4), accept]],
      ["updateDeclaredFieldWith", [2n, new Uint8Array(2), 0, from(), to(), accept]],
      ["updateDeclaredField", [2n, new Uint8Array(6), 0, from(), to()]],
      ["updateDeclaredFieldWith", [2n, surfaces(), 0, from(), to(), undefined]],
    ]) rejectsOwned(() => runtime[operation](...args), programError(operation), owned);
    assert.equal(calls, 0);
    assert.equal(initial.revision, 1n);
    assert.deepEqual(initial.fieldAssessment().to, [2, 0, 0]);
    const retry = runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), accept);
    assert.equal(calls, 1);
    assert.equal(retry.revision, 2n);
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("field arrays use actual storage and reject shared or proxy ingress before host effects", () => {
  const shared = new SharedArrayBuffer(3), sharedView = new Uint8Array(shared);
  sharedView.set([0, 0, 0]);
  // An own ordinary buffer and an ordinary-looking prototype must not hide SAB.
  Object.setPrototypeOf(shared, ArrayBuffer.prototype);
  Object.defineProperty(sharedView, "buffer", { value: new ArrayBuffer(3) });
  const revoked = Proxy.revocable(to(), {}); revoked.revoke();
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  let reads = 0, calls = 0;
  const accept = snapshot => { calls += 1; owned.add(snapshot); return true; };
  const hostileLength = Object.defineProperty(surfaces(), "length", {
    get() { reads += 1; throw new Error("caller length must not be read"); },
  });
  try {
    const initial = runtime.updateDeclaredField(1n, surfaces(), 0, from(), to()); owned.add(initial);
    for (const args of [
      [2n, new Proxy(surfaces(), {}), 0, from(), to(), accept],
      [2n, surfaces(), 0, sharedView, to(), accept],
      [2n, surfaces(), 0, from(), revoked.proxy, accept],
      [2n, hostileLength, 0, from(), to(), accept],
    ]) rejectsOwned(() => runtime.updateDeclaredFieldWith(...args), programError("updateDeclaredFieldWith"), owned);
    assert.equal(reads, 0);
    assert.equal(calls, 0);
    const retry = runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), accept);
    assert.equal(retry.state, "ready");
    assert.equal(calls, 1);
    assert.equal(initial.revision, 1n);
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("rejecting a field proposal retains the accepted revision and permits same-revision retry", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  try {
    const initial = runtime.updateObserved(1n, ids, bytes([255, 255, 255]), 1); owned.add(initial);
    let proposed;
    const result = runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), snapshot => {
      owned.add(snapshot); proposed = snapshot;
      assert.equal(snapshot.state, "ready");
      assert.equal(snapshot.revision, 2n);
      assert.equal(snapshot.fieldAssessment().rendering, "unknown");
      return false;
    });
    assert.equal(result, undefined);
    const replay = runtime.updateObserved(1n, ids, bytes([255, 255, 255]), 1); owned.add(replay);
    assert.equal(replay.revision, 1n);
    assert.equal(replay.fieldAssessment(), null);
    assert.deepEqual(replay.copyOutputTable(), initial.copyOutputTable());
    let callbacks = 0;
    const accepted = runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), snapshot => {
      callbacks += 1; owned.add(snapshot);
      return true;
    });
    assert.equal(callbacks, 1);
    assert.ok(owned.has(accepted), "accepted and callback snapshot are the same owned object");
    assert.deepEqual(accepted.fieldAssessment(), proposed.fieldAssessment());
    assert.equal(initial.fieldAssessment(), null);
    assert.equal(proposed.revision, 2n, "rejected snapshots remain usable historical data");
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("field callback reentry and thrown or asynchronous acceptance roll back before commit", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set(), failure = new Error("host refused");
  try {
    const initial = runtime.updateDeclaredField(1n, surfaces(), 0, from(), to()); owned.add(initial);
    assert.throws(() => runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), snapshot => {
      owned.add(snapshot);
      for (const [operation, invoke] of [
        ["updateDeclaredField", () => runtime.updateDeclaredField(3n, surfaces(), 0, from(), to())],
        ["updateObserved", () => runtime.updateObserved(3n, ids, bytes([255, 255, 255]), 1)],
        ["updateUnknown", () => runtime.updateUnknown(3n, 1)],
        ["runtimeFree", () => runtime.free()],
        ["snapshotFree", () => snapshot.free()],
      ]) assert.throws(invoke, programError(operation, "program_busy"));
      assert.equal(snapshot.fieldAssessment().domain, "whole-field");
      throw failure;
    }), error => error === failure);
    assert.throws(() => runtime.updateDeclaredFieldWith(2n, surfaces(), 0, from(), to(), snapshot => {
      owned.add(snapshot);
      return Promise.resolve(true);
    }), programError("updateDeclaredFieldWith", "program_host_protocol"));
    const replay = runtime.updateDeclaredField(1n, surfaces(), 0, from(), to()); owned.add(replay);
    assert.equal(replay.revision, 1n);
    const retry = runtime.updateDeclaredField(2n, surfaces(), 0, from(), to()); owned.add(retry);
    assert.equal(retry.revision, 2n);
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("field identity keeps endpoint direction while the unused varying slot cannot change it", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  try {
    const first = runtime.updateDeclaredField(1n, surfaces(), 0, from(), to()); owned.add(first);
    const replay = runtime.updateDeclaredField(1n, bytes([199, 201, 203]), 0, from(), to()); owned.add(replay);
    assert.deepEqual(replay.fieldAssessment(), first.fieldAssessment());
    rejectsOwned(() => runtime.updateDeclaredField(1n, surfaces(), 0, to(), from()),
      programError("updateDeclaredField"));
    const reversed = runtime.updateDeclaredField(2n, surfaces(), 0, to(), from()); owned.add(reversed);
    assert.deepEqual(reversed.fieldAssessment().from, [2, 0, 0]);
    assert.deepEqual(reversed.fieldAssessment().to, [0, 0, 0]);
    assert.deepEqual(first.fieldAssessment().from, [0, 0, 0]);
    assert.deepEqual(reversed.copyOutputTable(), first.copyOutputTable());
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
});

test("unknown clears current field scope and old immutable model data survives release", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  const start = from(), end = to();
  let retained;
  try {
    const first = runtime.updateDeclaredField(1n, surfaces(), 0, start, end); owned.add(first);
    retained = first.fieldAssessment();
    assert.equal(retained.sourceModel, "opaque-affine-encoded-srgb8-half-up-v1");
    assert.equal(retained.regionCount, 7);
    assert.deepEqual(retained.witnesses, [
      { coordinate: [0, 1], nominal: [0, 0, 0] },
      { coordinate: [1, 4], nominal: [1, 0, 0] },
      { coordinate: [3, 4], nominal: [2, 0, 0] },
    ]);
    start.fill(255); end.fill(255);
    assert.deepEqual(first.fieldAssessment().from, [0, 0, 0]);
    for (const object of [retained, retained.from, retained.to, retained.witnesses,
      retained.witnesses[1], retained.witnesses[1].coordinate, retained.witnesses[1].nominal]) {
      assert.equal(Object.isFrozen(object), true);
    }
    assert.throws(() => { retained.witnesses[1].coordinate[0] = 100; }, TypeError);
    const unknown = runtime.updateUnknown(2n, 7); owned.add(unknown);
    assert.equal(unknown.state, "stale");
    assert.equal(unknown.fieldAssessment(), null);
    const solid = runtime.updateObserved(3n, ids, bytes([255, 255, 255]), 1); owned.add(solid);
    assert.equal(solid.fieldAssessment(), null);
    assert.deepEqual(first.fieldAssessment(), retained);
  } finally {
    for (const snapshot of owned) snapshot.free();
    runtime.free();
  }
  assert.deepEqual(retained.from, [0, 0, 0]);
  assert.deepEqual(retained.to, [2, 0, 0]);
  assert.deepEqual(retained.witnesses[1], { coordinate: [1, 4], nominal: [1, 0, 0] });
});

test("gradient stop positions are revision-bound even when supports and paints agree", () => {
  const runtime = compileProgramWire(wire, 1), owned = new Set();
  const colors = new Uint8Array([2, 0, 0, 2, 1, 0, 1, 2, 0]);
  const pos = at => new Float64Array([0, at, 1]);
  try {
    const field = runtime.updateGradient(1n, surfaces(), 0, pos(0.25), colors); owned.add(field);
    const scope = field.fieldAssessment();
    assert.deepEqual(scope.stops[1], {at: 0.25, color: [2, 1, 0]});
    assert.throws(() => runtime.updateGradient(1n, surfaces(), 0, pos(0.75), colors), programError("updateGradient"));
    let calls = 0;
    for (const positions of [pos(0), pos(1), pos(Infinity), pos(NaN), new Float32Array([0, 0.5, 1])]) {
      assert.throws(() => runtime.updateGradientWith(2n, surfaces(), 0, positions, colors, () => { calls++; return true; }),
        programError("updateGradientWith"));
    }
    assert.equal(calls, 0);
    assert.equal(runtime.updateGradientWith(2n, surfaces(), 0, pos(0.75), colors, snapshot => {
      owned.add(snapshot);
      assert.equal(snapshot.fieldAssessment().stops[1].at, 0.75);
      return false;
    }), undefined);
    const retry = runtime.updateGradient(2n, surfaces(), 0, pos(0.5), colors); owned.add(retry);
    assert.deepEqual([...retry.copyOutputTable()], [...field.copyOutputTable()]);
    assert.equal(scope.stops[1].at, 0.25);
    const legacy = runtime.updateDeclaredField(3n, surfaces(), 0, from(), to()); owned.add(legacy);
    const equivalent = runtime.updateGradient(3n, surfaces(), 0, new Float64Array([0, 1]), new Uint8Array([...from(), ...to()])); owned.add(equivalent);
    assert.deepEqual(legacy.fieldAssessment(), equivalent.fieldAssessment());
  } finally { for (const snapshot of owned) snapshot.free(); runtime.free(); }
});
