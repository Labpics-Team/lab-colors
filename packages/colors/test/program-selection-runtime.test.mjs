import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "../program-wire/abi-v2.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const candidate = (id, level, opacity = 1) => ({ id, rgb: [level, level, level], opacity });
const state = (id, choices) => ({ id, choices: choices.map(([target, candidate]) => ({ target, candidate })) });
const normalRanks = () => [[state(1, [[21, 201]])], [state(2, [[21, 202]])], [state(3, [[21, 203]])]];

function graph(Builder = ProgramWireBuilderV2, ranks = normalRanks(), reversed = false) {
  const values = [candidate(201, 180), candidate(202, 80), candidate(203, 0)];
  if (reversed) values.reverse();
  const b = new Builder().finiteTarget(21, values).surfaceInputPort(31)
    .solidPaint(41, 21).inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
    .presentationRoot(71, 61).presentationTarget(71, 61)
    .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41);
  if (b.selectionRelease && ranks !== null) b.selectionRelease(1n, ranks);
  return b.finish();
}

function observe(owner, revision, surfaces) {
  return owner.updateObserved(revision, new Uint32Array(surfaces.map((_, i) => i + 1)), new Uint8Array(surfaces.flat()), 1);
}
const output = (snapshot) => [...snapshot.outputRgb(0)];
const compileFailure = (error) => isProgramError(error) && ["program_wire", "program_compile"].includes(error.code);

test("public finite selection chooses one admissible value for every required background", () => {
  let legacy;
  try { assert.throws(() => { legacy = compileProgramWire(graph(ProgramWireBuilderV1), 1); }, compileFailure); }
  finally { legacy?.free(); }
  const runtime = compileProgramWire(graph(), 1), held = [];
  try {
    const first = observe(runtime, 1n, [[255, 255, 255]]); held.push(first);
    assert.equal(first.state, "ready"); assert.deepEqual(output(first), [80, 80, 80]);
    const both = observe(runtime, 2n, [[255, 255, 255], [100, 100, 100]]); held.push(both);
    assert.equal(both.state, "ready"); assert.deepEqual(output(both), [0, 0, 0]);
    const conflict = observe(runtime, 3n, [[255, 255, 255], [0, 0, 0]]); held.push(conflict);
    assert.equal(conflict.state, "failed");
    const unknown = runtime.updateUnknown(4n, 1); held.push(unknown); assert.equal(unknown.state, "stale");
    const restored = observe(runtime, 5n, [[255, 255, 255]]); held.push(restored);
    assert.equal(restored.state, "ready"); assert.deepEqual(output(restored), [80, 80, 80]);
    assert.deepEqual(output(first), [80, 80, 80]);
  } finally { for (const snapshot of held.reverse()) snapshot.free(); runtime.free(); }
});

test("attachment applies the same chosen plan and revokes it when no candidate satisfies all contexts", () => {
  const host = { point: null, reject: false, operations: [] };
  const attachment = attachProgramWire(graph(), 1, 91, 501, 71, 61, (intent) => {
    host.operations.push(intent.operation);
    if (host.reject) return false;
    host.point = intent.point; return true;
  });
  const held = [];
  try {
    const ready = observe(attachment, 1n, [[255, 255, 255]]); held.push(ready);
    assert.equal(ready.state, "ready"); assert.deepEqual(output(ready), [80, 80, 80]);
    const authority = attachment.materializationAuthority(); held.push(authority);
    assert.deepEqual([...authority.terminalCompositeRgb()], [80, 80, 80]);
    const failed = observe(attachment, 2n, [[255, 255, 255], [0, 0, 0]]); held.push(failed);
    assert.equal(failed.state, "failed"); assert.equal(host.point, null); assert.equal(host.operations.at(-1), "revokeAll");
    assert.throws(() => attachment.materializationAuthority(), (e) => isProgramError(e) && e.code === "program_materialization_not_ready");
    host.reject = true;
    let rejected;
    try { assert.throws(() => { rejected = observe(attachment, 3n, [[0, 0, 0]]); }, (e) => isProgramError(e) && e.code === "program_attachment_host_rejected"); }
    finally { rejected?.free(); }
    host.reject = false;
    const retry = observe(attachment, 3n, [[0, 0, 0]]); held.push(retry);
    assert.equal(retry.state, "ready"); assert.deepEqual(output(retry), [180, 180, 180]);
    assert.deepEqual([...authority.terminalCompositeRgb()], [80, 80, 80]);
  } finally { host.point = null; for (const handle of held.reverse()) handle.free(); attachment.dispose(true); attachment.free(); }
});

test("the existing joint solver selects a complete two-target tuple rather than independent local choices", () => {
  const ranks = [[state(1, [[21, 201], [22, 301]])], [state(2, [[22, 302], [21, 201]])],
    [state(3, [[21, 202], [22, 301]])], [state(4, [[21, 202], [22, 302]])]];
  const b = new ProgramWireBuilderV2().finiteTarget(21, [candidate(201, 160, .5), candidate(202, 0, .5)])
    .finiteTarget(22, [candidate(301, 255), candidate(302, 80)]).selectionRelease(1n, ranks)
    .surfaceInputPort(31).solidPaint(41, 21).solidPaint(42, 22).inputSurface(51, 31)
    .sourceOverOccurrence(61, 42, 51, 64, .2, 1).occurrenceSurface(52, 61)
    .sourceOverOccurrence(62, 41, 52, 64, .2, 1).presentationRoot(71, 62).presentationTarget(71, 62)
    .exactVisibleUnary(true, 81, 62, [120, 120, 120]).output(91, 41).output(92, 42);
  const runtime = compileProgramWire(b.finish(), 1); let result;
  try {
    result = observe(runtime, 1n, [[255, 255, 255], [0, 0, 0]]);
    assert.equal(result.state, "ready"); assert.equal(result.outputCount(), 2);
    assert.deepEqual(output(result), [160, 160, 160]); assert.equal(result.outputOpacity(0), .5);
    assert.deepEqual([...result.outputRgb(1)], [80, 80, 80]); assert.equal(result.outputOpacity(1), 1);
  } finally { result?.free(); runtime.free(); }
});

test("rank groups override declaration order, and tied states use only their declared keys", () => {
  const rows = [state(9, [[21, 201]]), state(2, [[21, 202]]), state(1, [[21, 203]])];
  for (const reverse of [false, true]) {
    const runtime = compileProgramWire(graph(ProgramWireBuilderV2, [reverse ? [...rows].reverse() : rows], reverse), 1);
    let result;
    try { result = observe(runtime, 1n, [[255, 255, 255]]); assert.deepEqual(output(result), [0, 0, 0]); }
    finally { result?.free(); runtime.free(); }
  }
});

test("missing, repeated and foreign assignments never acquire a runtime", () => {
  const ranks = normalRanks();
  const invalid = [null, [ranks[0]], [ranks[0], ranks[1], ranks[0]],
    [[state(1, [[21, 201]])], [state(1, [[21, 202]])], ranks[2]],
    [[state(1, [[99, 201]])], ranks[1], ranks[2]],
    [[state(1, [[21, 999]])], ranks[1], ranks[2]],
    [[state(1, [[21, 201], [21, 201]])], ranks[1], ranks[2]]];
  for (const rows of invalid) {
    let accidental;
    try { assert.throws(() => { accidental = compileProgramWire(graph(ProgramWireBuilderV2, rows), 1); }, compileFailure); }
    finally { accidental?.free(); }
  }
});
