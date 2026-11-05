import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, isBindingError, isProgramError } from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";
import { ProgramWireBuilderV3 } from "@labpics/colors/program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });

const bindings = {
  emissions: [{ slot: 92, sinkOutput: 402 }, { slot: 91, sinkOutput: 401 }],
  presentations: [
    { slot: 91, root: 73, occurrence: 63 },
    { slot: 92, root: 72, occurrence: 62 },
    { slot: 91, root: 71, occurrence: 61 },
  ],
};
const firstSource = [20, 40, 60], secondSource = [80, 100, 120];
const over = (source, quarters, background) => Array.from(source, (channel, index) =>
  Math.floor((quarters * channel + (4 - quarters) * background[index] + 2) / 4));
const observe = (attachment, revision, rows) => attachment.updateObserved(revision,
  new Uint32Array(rows.map(([id]) => id)), new Uint8Array(rows.flatMap(([, rgb]) => rgb)), 1);
const programError = (code) => (error) => isProgramError(error) && error.code === code;

function graph() {
  return new ProgramWireBuilderV1()
    .source(11, firstSource).source(12, secondSource).fixedTarget(21, 11).fixedTarget(22, 12)
    .surfaceInputPort(31).opacityInput(32, .5).opacityInput(33, .25)
    .solidPaint(41, 21).opacityPaint(42, 41, 32)
    .solidPaint(43, 22).opacityPaint(44, 43, 33).inputSurface(51, 31)
    .sourceOverOccurrence(61, 42, 51, 64, .2, 1)
    .sourceOverOccurrence(62, 44, 51, 64, .2, 1)
    .sourceOverOccurrence(63, 42, 51, 64, .2, 1)
    .presentationRoot(71, 61).presentationTarget(71, 61)
    .presentationRoot(72, 62).presentationTarget(72, 62)
    .presentationRoot(73, 63).presentationTarget(73, 63)
    .exactVisibleUnary(false, 81, 61, firstSource)
    .exactVisibleUnary(false, 82, 62, secondSource).output(92, 44).output(91, 42).finish();
}

function patch(intent) {
  return {
    scope: [...intent.scope],
    points: intent.points.map(({ slot, sinkOutput, source, opacity }) =>
      ({ slot, sinkOutput, source: [...source], opacity })),
    composites: intent.presentationComposites.map(({ root, scenario, source }) =>
      ({ root, scenario, source: [...source] })),
  };
}

function sink(batch = true) {
  const host = { sequence: 0n, epoch: null, current: null, calls: [], reject: false };
  host.install = (intent) => {
    if (batch) {
      assert.equal(intent.point, null, "batch не имеет неявной первой точки");
      assert.equal(intent.sinkOutput, null);
    }
    assert.equal(intent.expectedSequence, host.sequence);
    if (host.epoch !== null) assert.equal(intent.bindingEpoch, host.epoch);
    const projected = patch(intent);
    host.calls.push({ operation: intent.operation, revision: intent.revision, ...projected });
    if (host.reject) return false;
    if (intent.operation === "confirmExact") assert.deepEqual(projected, host.current);
    else host.current = projected;
    host.sequence = intent.desiredSequence;
    host.epoch = intent.bindingEpoch;
    return true;
  };
  return host;
}

function expectPatch(value, rows) {
  assert.deepEqual(value.scope, [401, 402]);
  assert.deepEqual(value.points, [
    { slot: 91, sinkOutput: 401, source: firstSource, opacity: .5 },
    { slot: 92, sinkOutput: 402, source: secondSource, opacity: .25 },
  ]);
  const expected = rows.flatMap(([scenario, background]) => [
    { root: 71, scenario, source: over(firstSource, 2, background) },
    { root: 72, scenario, source: over(secondSource, 1, background) },
    { root: 73, scenario, source: over(firstSource, 2, background) },
  ]);
  const ordered = (entries) => entries.toSorted((a, b) => a.root - b.root || a.scenario - b.scenario);
  assert.deepEqual(ordered(value.composites), ordered(expected));
}

test("полный attachment доставляет fan-out, source alpha и каждую композицию до ACK", () => {
  for (const reverse of [false, true]) {
    const declared = reverse ? {
      emissions: bindings.emissions.toReversed(), presentations: bindings.presentations.toReversed(),
    } : bindings;
    const host = sink(), attachment = attachProgramWire(graph(), 8, declared, host.install), held = [];
    const own = (handle) => (held.push(handle), handle);
    try {
      const rows = [[101, [10, 20, 30]]], first = own(observe(attachment, 1n, rows));
      assert.equal(first.state, "ready");
      assert.equal(first.outputCount(), 2);
      assert.equal(first.renderCount(), 3);
      assert.equal(first.hasRender(), true);
      assert.equal(first.render(), undefined);
      expectPatch(host.current, rows);
      const renders = new Map();
      for (let index = 0; index < first.renderCount(); index += 1) {
        const render = own(first.renderAt(index));
        renders.set(render.presentationRoot(), render);
        assert.equal(render.revision(), 1n);
        assert.deepEqual([...render.terminalCompositeRgb()],
          over(render.outputRgb(), render.outputOpacity() * 4, rows[0][1]));
      }
      assert.deepEqual([...renders.keys()].sort(), [71, 72, 73]);
      assert.deepEqual([...renders.get(71).outputRgb()], firstSource);
      assert.equal(renders.get(71).outputOpacity(), .5);
      assert.throws(() => attachment.materializationAuthority(), programError("program_materialization_ambiguous_bindings"));
      assert.throws(() => attachment.materializationAuthorityFor(1n, renders.get(71).contentIdentity(), host.epoch),
        programError("program_materialization_ambiguous_bindings"));
      const authority = own(attachment.materializationAuthorityForBinding(72, 62));
      assert.equal(authority.outputSlot(), 92);
      assert.deepEqual([...authority.terminalCompositeRgb()], over(secondSource, 1, rows[0][1]));
      assert.throws(() => attachment.materializationAuthorityForBinding(72, 61),
        programError("program_materialization_terminal_binding_mismatch"));
      assert.throws(() => first.renderAt(first.renderCount()), (error) =>
        isBindingError(error) && error.code === "invalid_output_index");

      const prior = structuredClone(host.current), changed = [[102, [100, 120, 140]]];
      host.reject = true;
      assert.throws(() => observe(attachment, 2n, changed), programError("program_attachment_host_rejected"));
      assert.deepEqual(host.current, prior);
      assert.equal(own(attachment.materializationAuthorityForBinding(71, 61)).revision(), 1n);
      host.reject = false;
      own(observe(attachment, 2n, changed));
      expectPatch(host.current, changed);
      assert.notDeepEqual(host.current.composites, prior.composites);
      assert.deepEqual(host.current.points, prior.points, "смена подложки сохраняет source и alpha");
      assert.equal(own(attachment.materializationAuthorityForBinding(71, 61)).revision(), 2n);
      own(observe(attachment, 2n, changed));
      assert.equal(host.calls.at(-1).operation, "confirmExact");
      const calls = host.calls.length;
      assert.throws(() => observe(attachment, 1n, rows), programError("program_attachment_update"));
      assert.throws(() => observe(attachment, 2n, rows), programError("program_attachment_update"));
      assert.equal(host.calls.length, calls);
      const stale = own(attachment.updateUnknown(3n, 99));
      assert.equal(stale.renderCount(), 0);
      assert.deepEqual(host.current, { scope: [401, 402], points: [], composites: [] });
      assert.equal(host.calls.at(-1).operation, "revokeAll");
      assert.throws(() => attachment.materializationAuthorityForBinding(71, 61), programError("program_materialization_not_ready"));
      own(observe(attachment, 4n, rows));
      expectPatch(host.current, rows);
      assert.equal(first.revision, 1n);
    } finally {
      for (const handle of held.toReversed()) handle.free();
      host.current = null;
      attachment.dispose(true);
      attachment.free();
    }
  }
});

test("полный scope сохраняет locked цвет и применяет off/lint/auto у общего Core owner", () => {
  const dirty = [0, 199, 64], clean = [0, 199, 63], locked = [255, 255, 255];
  for (const mode of ["off", "lint", "auto"]) {
    const b = new ProgramWireBuilderV3()
      .computedTarget(21, { kind: "box", lower: clean, upper: dirty }, dirty, 1).assignment([21])
      .source(12, locked).fixedTarget(22, 12).surfaceInputPort(31)
      .solidPaint(41, 21).solidPaint(42, 22).inputSurface(51, 31)
      .sourceOverOccurrence(61, 41, 51, 64, .2, 1)
      .sourceOverOccurrence(62, 42, 51, 64, .2, 1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .presentationRoot(72, 62).presentationTarget(72, 62)
      .wcag22VisibleUnary(true, 81, 61, 1).wcag22VisibleUnary(true, 82, 62, 1)
      .output(91, 41).output(92, 42);
    if (mode !== "off") b.declaredSrgb8CleanSet(mode === "auto", 83, 71, 61);
    const host = sink(), attachment = attachProgramWire(b.finish(), 9, {
      emissions: bindings.emissions,
      presentations: bindings.presentations.filter(({ root }) => root !== 73),
    }, host.install), held = [];
    try {
      const first = observe(attachment, 1n, [[101, [0, 0, 0]]]); held.push(first);
      assert.equal(first.state, "ready");
      assert.deepEqual(host.current.points.map(({ source }) => source), [mode === "auto" ? clean : dirty, locked]);
      const cell = first.assessmentReport().cells.find(({ constraintId }) => constraintId === 83);
      if (mode === "off") assert.equal(cell, undefined);
      else {
        assert.equal(cell.hard, mode === "auto");
        assert.equal(cell.assessment.verdict, mode === "auto" ? "pass" : "violation");
        assert.deepEqual(cell.assessment.visible, mode === "auto" ? clean : dirty);
      }
      const failed = observe(attachment, 2n, [[101, locked]]); held.push(failed);
      assert.equal(failed.state, "unsatisfied");
      assert.deepEqual(host.current, { scope: [401, 402], points: [], composites: [] });
      assert.equal(failed.outputCount(), 0);
      const recovered = observe(attachment, 3n, [[101, [0, 0, 0]]]); held.push(recovered);
      assert.equal(recovered.state, "ready");
      assert.deepEqual(host.current.points[1].source, locked);
    } finally {
      for (const handle of held.toReversed()) handle.free();
      host.current = null;
      attachment.dispose(true);
      attachment.free();
    }
  }
});

test("binding admission отвергает неполный scope и accessors до host effect", () => {
  let reads = 0, calls = 0;
  const accessor = Object.defineProperty({}, "emissions", { get() { reads += 1; return bindings.emissions; } });
  const revoked = Proxy.revocable([], {});
  revoked.revoke();
  for (const declared of [
    { ...bindings, emissions: bindings.emissions.slice(0, 1) },
    { ...bindings, emissions: [bindings.emissions[0], bindings.emissions[0]] },
    { ...bindings, presentations: bindings.presentations.slice(0, 2) },
    accessor,
    { ...bindings, emissions: revoked.proxy },
    { ...bindings, presentations: revoked.proxy },
    { ...bindings, emissions: [{ slot: 2 ** 32, sinkOutput: 1 }, bindings.emissions[1]] },
  ]) {
    let unexpected;
    try {
      assert.throws(() => { unexpected = attachProgramWire(graph(), 10, declared, () => { calls += 1; return true; }); },
        programError("program_attachment_binding"));
    } finally {
      if (unexpected) { unexpected.dispose(true); unexpected.free(); }
    }
  }
  assert.equal(reads, 0);
  assert.equal(calls, 0);
});

function mixedTargetGraph() {
  return new ProgramWireBuilderV1()
    .source(1, firstSource).fixedTarget(2, 1).surfaceInputPort(6).opacityInput(5, .5)
    .solidPaint(3, 2).opacityPaint(4, 3, 5).inputSurface(7, 6)
    .sourceOverOccurrence(8, 4, 7, 64, .2, 1).occurrenceSurface(11, 8)
    .sourceOverOccurrence(12, 4, 11, 64, .2, 1)
    .presentationRoot(9, 12).presentationTarget(9, 8).presentationTarget(9, 12)
    .exactVisibleUnary(false, 10, 12, firstSource).output(17, 4).finish();
}

function aliasGraph(distinctPaint = false) {
  const builder = new ProgramWireBuilderV1()
    .source(1, firstSource).fixedTarget(2, 1).surfaceInputPort(6).opacityInput(5, .5)
    .solidPaint(3, 2).opacityPaint(4, 3, 5).inputSurface(7, 6)
    .sourceOverOccurrence(8, 4, 7, 64, .2, 1)
    .presentationRoot(9, 8).presentationTarget(9, 8)
    .exactVisibleUnary(false, 10, 8, firstSource).output(17, 4);
  if (distinctPaint) builder.source(11, firstSource).fixedTarget(12, 11)
    .solidPaint(14, 12).opacityPaint(15, 14, 5)
    .sourceOverOccurrence(16, 15, 7, 64, .2, 1).exactVisibleUnary(false, 21, 16, firstSource);
  return builder.output(18, distinctPaint ? 15 : 4).finish();
}

test("ancestor evaluation targets сохраняют полный snapshot при одном terminal binding", () => {
  const host = sink(false), attachment = attachProgramWire(mixedTargetGraph(), 11, {
    emissions: [{ slot: 17, sinkOutput: 501 }],
    presentations: [{ slot: 17, root: 9, occurrence: 12 }],
  }, host.install), held = [];
  const own = (handle) => (held.push(handle), handle);
  try {
    const rows = [[101, [4, 8, 12]], [102, [100, 120, 140]]];
    const first = own(observe(attachment, 1n, rows));
    assert.equal(first.state, "ready");
    assert.equal(first.outputCount(), 1);
    assert.equal(first.renderCount(), 1);
    const render = own(first.renderAt(0));
    assert.equal(render.presentationRoot(), 9);
    assert.equal(render.occurrence(), 12);
    assert.deepEqual(host.current.scope, [501]);
    assert.deepEqual(host.current.points, [{ slot: 17, sinkOutput: 501, source: firstSource, opacity: .5 }]);
    assert.equal(host.current.composites.length, 2, "host получает конечный RGB по уникальным root/scenario");
    for (const value of host.current.composites) {
      const [, background] = rows.find(([scenario]) => scenario === value.scenario);
      assert.equal(value.root, 9);
      assert.deepEqual(value.source, over(firstSource, 2, over(firstSource, 2, background)));
    }
    assert.throws(() => attachment.materializationAuthorityForBinding(9, 8),
      programError("program_materialization_terminal_binding_mismatch"));
    const changed = own(observe(attachment, 2n, [[103, [40, 80, 120]]]));
    assert.equal(changed.renderCount(), 1);
    const authority = own(attachment.materializationAuthorityForBinding(9, 12));
    assert.equal(authority.outputSlot(), 17);
    assert.deepEqual([...authority.terminalCompositeRgb()], over(firstSource, 2, over(firstSource, 2, [40, 80, 120])));
  } finally {
    for (const handle of held.toReversed()) handle.free();
    host.current = null;
    attachment.dispose(true);
    attachment.free();
  }
});

test("alias output slots публикуются целиком с выбранным клиентом terminal binding", () => {
  for (const slot of [17, 18]) {
    const host = sink(), attachment = attachProgramWire(aliasGraph(), 12, {
      emissions: [{ slot: 18, sinkOutput: 502 }, { slot: 17, sinkOutput: 501 }],
      presentations: [{ slot, root: 9, occurrence: 8 }],
    }, host.install), held = [];
    const own = (handle) => (held.push(handle), handle);
    try {
      const initial = own(observe(attachment, 1n, [[101, [4, 8, 12]]]));
      assert.equal(initial.state, "ready");
      assert.equal(initial.outputCount(), 2);
      assert.equal(initial.renderCount(), 1);
      assert.equal(own(initial.renderAt(0)).outputSlot(), slot);
      assert.equal(own(attachment.materializationAuthorityForBinding(9, 8)).outputSlot(), slot);
      assert.deepEqual(host.current.scope, [501, 502]);
      assert.deepEqual(host.current.points, [
        { slot: 17, sinkOutput: 501, source: firstSource, opacity: .5 },
        { slot: 18, sinkOutput: 502, source: firstSource, opacity: .5 },
      ]);
      assert.deepEqual(host.current.composites,
        [{ root: 9, scenario: 101, source: over(firstSource, 2, [4, 8, 12]) }]);
      const previous = structuredClone(host.current);
      host.reject = true;
      assert.throws(() => observe(attachment, 2n, [[102, [100, 120, 140]]]),
        programError("program_attachment_host_rejected"));
      assert.deepEqual(host.current, previous);
      assert.equal(own(attachment.materializationAuthorityForBinding(9, 8)).revision(), 1n);
      host.reject = false;
      own(observe(attachment, 2n, [[102, [100, 120, 140]]]));
      assert.deepEqual(host.current.points, previous.points);
      assert.notDeepEqual(host.current.composites, previous.composites);
      own(observe(attachment, 2n, [[102, [100, 120, 140]]]));
      assert.equal(host.calls.at(-1).operation, "confirmExact");
      const stale = own(attachment.updateUnknown(3n, 99));
      assert.equal(stale.state, "stale");
      assert.equal(stale.outputCount(), 2);
      for (let index = 0; index < stale.outputCount(); index += 1) {
        assert.equal(stale.outputSlot(index), 17 + index);
        assert.deepEqual([...stale.outputRgb(index)], firstSource);
        assert.equal(stale.outputOpacity(index), .5);
      }
      assert.equal(stale.renderCount(), 0);
      assert.deepEqual(host.current, { scope: [501, 502], points: [], composites: [] });
      assert.throws(() => attachment.materializationAuthorityForBinding(9, 8),
        programError("program_materialization_not_ready"));
    } finally {
      for (const handle of held.toReversed()) handle.free();
      host.current = null;
      attachment.dispose(true);
      attachment.free();
    }
  }
});

test("nonterminal, duplicate и другой Paint не дают ложного полного scope", () => {
  let calls = 0;
  const host = () => { calls += 1; return true; };
  const emissions = [{ slot: 17, sinkOutput: 501 }, { slot: 18, sinkOutput: 502 }];
  for (const presentations of [
    [{ slot: 17, root: 9, occurrence: 8 }],
    [{ slot: 17, root: 9, occurrence: 12 }, { slot: 17, root: 9, occurrence: 8 }],
  ]) {
    assert.throws(() => attachProgramWire(mixedTargetGraph(), 13, {
      emissions: emissions.slice(0, 1), presentations,
    }, host), programError("program_attachment_non_terminal_target"));
  }
  assert.throws(() => attachProgramWire(mixedTargetGraph(), 13, {
    emissions: emissions.slice(0, 1), presentations: [],
  }, host), programError("program_attachment_binding"));
  assert.throws(() => attachProgramWire(aliasGraph(), 13, {
    emissions, presentations: [{ slot: 17, root: 9, occurrence: 8 }, { slot: 18, root: 9, occurrence: 8 }],
  }, host), programError("program_attachment_binding"));
  assert.throws(() => attachProgramWire(aliasGraph(true), 13, {
    emissions, presentations: [{ slot: 17, root: 9, occurrence: 8 }],
  }, host), programError("program_attachment_binding"));
  assert.equal(calls, 0);
});

async function projectionFault() {
  const { default: assert } = await import("node:assert/strict");
  const { readFileSync } = await import("node:fs");
  const nativeFreeze = Object.freeze;
  let refuse = false, faults = 0;
  Object.freeze = (value) => {
    if (refuse) { faults += 1; throw new RangeError("controlled host projection refusal"); }
    return nativeFreeze(value);
  };
  let attachment;
  const held = [], host = { calls: 0, point: null };
  try {
    const api = await import("@labpics/colors");
    const { ProgramWireBuilderV1 } = await import("@labpics/colors/program-wire/abi-v1.js");
    Object.freeze = nativeFreeze;
    await api.default({ module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });
    const wire = new ProgramWireBuilderV1().source(11, [20, 40, 60]).fixedTarget(21, 11)
      .surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
      .sourceOverOccurrence(61, 41, 51, 64, .2, 1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .exactVisibleUnary(false, 81, 61, [20, 40, 60]).output(91, 41).finish();
    attachment = api.attachProgramWire(wire, 1, 91, 501, 71, 61, (intent) => {
      host.calls += 1; host.point = intent.point; return true;
    });
    const read = (revision) => attachment.updateObserved(revision, new Uint32Array([1]), new Uint8Array([0, 0, 0]), 1);
    const first = read(1n); held.push(first);
    const previous = host.point, calls = host.calls;
    refuse = true;
    assert.throws(() => read(2n), (error) => api.isProgramError(error)
      && error.code === "program_attachment_resource_exhausted");
    refuse = false;
    assert.ok(faults > 0, "fault должен достигнуть caught WASM host projection");
    assert.equal(host.calls, calls);
    assert.equal(host.point, previous);
    const authority = attachment.materializationAuthority(); held.push(authority);
    assert.equal(authority.revision(), 1n);
    const retry = read(2n); held.push(retry);
    assert.equal(retry.state, "ready");
    assert.equal(retry.revision, 2n);
  } finally {
    refuse = false;
    Object.freeze = nativeFreeze;
    for (const handle of held.toReversed()) handle.free();
    if (attachment) { host.point = null; attachment.dispose(true); attachment.free(); }
  }
}

test("исключение JS-проекции до ACK сохраняет head и освобождает Rust Busy/lease", () => {
  execFileSync(process.execPath, ["--input-type=module", "-e", "await (" + projectionFault.toString() + ")();"],
    { timeout: 30_000, stdio: "pipe" });
});
