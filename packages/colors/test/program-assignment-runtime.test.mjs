import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isBindingError, isProgramError } from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";
import { ProgramWireBuilderV2 } from "@labpics/colors/program-wire/abi-v2.js";
import { ProgramWireBuilderV3, SURROUND_AVERAGE_V1, WCAG22_SC143_TEXT_DEFAULT_V1 } from "@labpics/colors/program-wire/abi-v3.js";

await init({ module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });

const sameIds = (id) => id;
const reversedIds = (id) => 0xffff_ffff - id * 17;
const gray = (lower, upper) => ({ kind: "gray", lower, upper });
const fullRgb = { kind: "box", lower: [0, 0, 0], upper: [255, 255, 255] };
const triple = (value) => [value, value, value];
const errorIs = (operation, code) => (error) => isProgramError(error) && error.operation === operation && error.code === code;
const own = (handles, handle) => (handles.push(handle), handle);
const release = (handles) => { for (const handle of handles.toReversed()) handle.free(); };
const observe = (owner, revision, backgrounds) => owner.updateObserved(revision,
  new Uint32Array(backgrounds.map((_, index) => index + 1)), new Uint8Array(backgrounds.flat()), 1);
const outputs = (snapshot) => new Map(Array.from({ length: snapshot.outputCount() }, (_, index) =>
  [snapshot.outputSlot(index), { rgb: [...snapshot.outputRgb(index)], opacity: snapshot.outputOpacity(index) }]));

function graph({ domain = gray(0, 96), anchor = [24, 24, 24], maxDelta = 1, id = sameIds,
  report = false, Builder = ProgramWireBuilderV3, assignment = [id(21)] } = {}) {
  const builder = new Builder();
  if (Builder === ProgramWireBuilderV3) builder.computedTarget(id(21), domain, anchor, maxDelta).assignment(assignment);
  else builder.source(id(11), anchor).fixedTarget(id(21), id(11));
  builder.surfaceInputPort(id(31)).solidPaint(id(41), id(21)).inputSurface(id(51), id(31))
    .sourceOverOccurrence(id(61), id(41), id(51), 64, .2, SURROUND_AVERAGE_V1)
    .presentationRoot(id(71), id(61)).presentationTarget(id(71), id(61))
    .wcag22VisibleUnary(true, id(81), id(61), WCAG22_SC143_TEXT_DEFAULT_V1).output(id(91), id(41));
  if (report) builder.exactVisibleUnary(false, id(82), id(61), [80, 80, 80]);
  return builder;
}

function current(snapshot, revision, state, expected, slot = 91) {
  assert.equal(snapshot.snapshotVersion, 2);
  assert.equal(snapshot.revision, revision);
  assert.equal(snapshot.state, state);
  assert.equal(snapshot.reason, state === "incomplete" ? "limit" : undefined);
  if (state === "ready") assert.deepEqual(outputs(snapshot), new Map([[slot, { rgb: expected, opacity: 1 }]]));
  else {
    assert.equal(snapshot.outputCount(), 0);
    for (const method of ["outputRgb", "outputSlot", "outputOpacity"]) {
      assert.throws(() => snapshot[method](0), (error) => isBindingError(error)
        && error.operation === "readProgramOutput" && error.code === "invalid_output_index");
    }
  }
}

// Независимый WCAG 2.2 oracle: полный перебор 256 серых каналов. В серой области
// расстояние до серой опоры монотонно по обе стороны от неё; в примерах ниже
// допустимые значения находятся только с одной стороны. Solver/его метрика не вызываются.
function luminance(channel) {
  const s = channel / 255;
  return s <= .04045 ? s / 12.92 : ((s + .055) / 1.055) ** 2.4;
}
function contrast(a, b) {
  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);
  return (high + .05) / (low + .05);
}
function grayOracle(lower, upper, anchor, background) {
  const valid = Array.from({ length: upper - lower + 1 }, (_, index) => lower + index)
    .filter((channel) => contrast(channel, background) >= 4.5);
  if (valid.includes(anchor)) return anchor;
  if (!valid.length) return null;
  assert.ok(valid.every((value) => value < anchor) || valid.every((value) => value > anchor));
  return valid.every((value) => value < anchor) ? Math.max(...valid) : Math.min(...valid);
}

test("V3 назначает цвет по двум декларациям и сохраняет опоры независимо от opaque IDs", () => {
  for (const id of [sameIds, reversedIds]) for (const fixture of [
    { lower: 0, upper: 96, anchor: 24, backgrounds: [255, 128, 0] },
    { lower: 160, upper: 255, anchor: 230, backgrounds: [0, 110, 255] },
  ]) {
    const runtime = compileProgramWire(graph({ id, domain: gray(fixture.lower, fixture.upper),
      anchor: triple(fixture.anchor), report: true }).finish(), 1), held = [];
    try {
      const expected = fixture.backgrounds.map((background) => grayOracle(fixture.lower, fixture.upper, fixture.anchor, background));
      assert.equal(expected[0], fixture.anchor);
      assert.notEqual(expected[1], fixture.anchor);
      assert.notEqual(expected[1], null);
      assert.equal(expected[2], null);
      for (const [index, background] of fixture.backgrounds.entries()) {
        const result = own(held, observe(runtime, BigInt(index + 1), [triple(background)]));
        current(result, BigInt(index + 1), expected[index] === null ? "unsatisfied" : "ready",
          expected[index] === null ? null : triple(expected[index]), id(91));
      }
      assert.ok(contrast(fixture.anchor < 128 ? 0 : 255, fixture.backgrounds[0]) > contrast(fixture.anchor, fixture.backgrounds[0]));
      current(held[0], 1n, "ready", triple(fixture.anchor), id(91));
    } finally { release(held); runtime.free(); }
  }
});

test("V3 фиксированные светлый и тёмный источники не заменяются ради контраста", () => {
  for (const [fixed, anchor] of [[[255, 255, 255], [12, 50, 90]], [[0, 0, 0], [210, 230, 245]]]) {
    const wire = new ProgramWireBuilderV3().source(11, fixed).fixedTarget(22, 11)
      .computedTarget(21, fullRgb, anchor, 1).assignment([21])
      .surfaceInputPort(31).solidPaint(41, 21).solidPaint(42, 22).inputSurface(51, 31)
      .sourceOverOccurrence(61, 41, 51, 64, .2, 1).occurrenceSurface(52, 61)
      .sourceOverOccurrence(62, 42, 52, 64, .2, 1).presentationRoot(71, 62).presentationTarget(71, 62)
      .wcag22VisibleUnary(true, 81, 62, WCAG22_SC143_TEXT_DEFAULT_V1).output(91, 41).output(92, 42).finish();
    const runtime = compileProgramWire(wire, 1); let result;
    try {
      result = observe(runtime, 1n, [[128, 128, 128]]);
      assert.equal(result.state, "ready");
      assert.deepEqual(outputs(result), new Map([[91, { rgb: anchor, opacity: 1 }], [92, { rgb: fixed, opacity: 1 }]]));
    } finally { result?.free(); runtime.free(); }
  }
});

test("V3 совместный выбор следует authored порядку, а не порядку ID или независимому repair", () => {
  // Полное допустимое множество получается целочисленным source-over с opacity 1/2.
  const feasible = [];
  for (let a = 0; a <= 4; a += 1) for (let b = 0; b <= 4; b += 1) {
    if (Math.floor((a + b + 1) / 2) === 2) feasible.push([a, b]);
  }
  assert.ok(!feasible.some(([a, b]) => a === 0 && b === 0));
  for (const id of [sameIds, reversedIds]) for (const reverse of [false, true]) {
    const order = reverse ? [22, 21] : [21, 22];
    const expected = feasible.toSorted((a, b) => reverse ? a[1] - b[1] || a[0] - b[0] : a[0] - b[0] || a[1] - b[1])[0];
    const wire = new ProgramWireBuilderV3()
      .computedTarget(id(21), gray(0, 4), [0, 0, 0], 1).computedTarget(id(22), gray(0, 4), [0, 0, 0], 1)
      .assignment(order.map(id)).surfaceInputPort(id(31)).opacityInput(id(32), .5)
      .solidPaint(id(41), id(21)).solidPaint(id(42), id(22)).opacityPaint(id(43), id(42), id(32))
      .inputSurface(id(51), id(31)).sourceOverOccurrence(id(61), id(41), id(51), 64, .2, 1)
      .occurrenceSurface(id(52), id(61)).sourceOverOccurrence(id(62), id(43), id(52), 64, .2, 1)
      .presentationRoot(id(71), id(62)).presentationTarget(id(71), id(62))
      .exactVisibleUnary(true, id(81), id(62), [2, 2, 2]).output(id(91), id(41)).output(id(92), id(43)).finish();
    const runtime = compileProgramWire(wire, 1); let result;
    try {
      result = observe(runtime, 1n, [[0, 0, 0], [255, 255, 255]]);
      assert.equal(result.state, "ready");
      assert.deepEqual(outputs(result), new Map([[id(91), { rgb: triple(expected[0]), opacity: 1 }],
        [id(92), { rgb: triple(expected[1]), opacity: .5 }]]));
    } finally { result?.free(); runtime.free(); }
  }
});

test("V3 опора вне области и нулевой допуск не создают скрытый допустимый цвет", () => {
  for (const domain of [gray(20, 21), { kind: "box", lower: [20, 20, 20], upper: [20, 20, 21] }]) {
    for (const maxDelta of [0, 1]) {
      const runtime = compileProgramWire(graph({ domain, anchor: [24, 24, 24], maxDelta }).finish(), 1); let result;
      try {
        result = observe(runtime, 1n, [[255, 255, 255]]);
        if (maxDelta === 0) current(result, 1n, "unsatisfied");
        else {
          assert.equal(result.state, "ready");
          const rgb = outputs(result).get(91).rgb;
          assert.notDeepEqual(rgb, [24, 24, 24]);
          assert.ok(domain.kind === "gray" ? rgb.every((value) => value === rgb[0] && value >= 20 && value <= 21)
            : rgb[0] === 20 && rgb[1] === 20 && rgb[2] >= 20 && rgb[2] <= 21);
        }
      } finally { result?.free(); runtime.free(); }
    }
  }
});

test("V3 пустота, unknown и UInt64 revision не возвращают historical outputs", () => {
  const runtime = compileProgramWire(graph().finish(), 1), held = [], revision = 1n << 63n;
  try {
    current(own(held, runtime.updateUnknown(revision, 1)), revision, "waiting");
    current(own(held, observe(runtime, revision + 1n, [[255, 255, 255]])), revision + 1n, "ready", [24, 24, 24]);
    current(own(held, observe(runtime, revision + 2n, [[0, 0, 0]])), revision + 2n, "unsatisfied");
    current(own(held, runtime.updateUnknown(revision + 3n, 2)), revision + 3n, "stale");
    current(held[1], revision + 1n, "ready", [24, 24, 24]);
    current(own(held, observe(runtime, revision + 4n, [[255, 255, 255]])), revision + 4n, "ready", [24, 24, 24]);
  } finally { release(held); runtime.free(); }
});

test("V1/V2 сохраняют прежние historical outputs при отказе и unknown", () => {
  for (const Builder of [ProgramWireBuilderV1, ProgramWireBuilderV2]) {
    const runtime = compileProgramWire(graph({ Builder }).finish(), 1), held = [];
    try {
      const first = own(held, observe(runtime, 1n, [[255, 255, 255]]));
      assert.equal(first.state, "ready");
      for (const result of [own(held, observe(runtime, 2n, [[0, 0, 0]])), own(held, runtime.updateUnknown(3n, 1))]) {
        assert.equal(result.snapshotVersion, 1);
        assert.equal(result.reason, undefined);
        assert.deepEqual(outputs(result), outputs(first));
      }
      assert.equal(held[1].state, "failed"); assert.equal(held[1].revision, 2n);
      assert.equal(held[2].state, "stale"); assert.equal(held[2].revision, 3n);
    } finally { release(held); runtime.free(); }
  }
});

test("V3 incomplete отзывает текущий point, replay сохраняет исход, новая revision восстанавливает ready", () => {
  const host = { point: null, reject: false, operations: [] };
  const attachment = attachProgramWire(graph({ domain: fullRgb }).finish(), 1, 91, 501, 71, 61, (intent) => {
    host.operations.push(intent.operation);
    if (host.reject) return false;
    host.point = intent.point; return true;
  }), held = [];
  try {
    const first = own(held, observe(attachment, 1n, [[255, 255, 255]]));
    current(first, 1n, "ready", [24, 24, 24]); assert.equal(first.hasRender(), true);
    const authority = own(held, attachment.materializationAuthority());
    host.reject = true;
    assert.throws(() => own(held, observe(attachment, 2n, [[0, 0, 0]])), errorIs("attachmentUpdateObserved", "program_attachment_host_rejected"));
    assert.notEqual(host.point, null);
    const unchanged = own(held, attachment.materializationAuthority()); assert.equal(unchanged.revision(), 1n);
    host.reject = false;
    const stopped = own(held, observe(attachment, 2n, [[0, 0, 0]]));
    current(stopped, 2n, "incomplete"); assert.equal(stopped.hasRender(), false);
    assert.equal(stopped.render(), undefined); assert.equal(host.point, null);
    assert.equal(host.operations.at(-1), "revokeAll");
    assert.throws(() => attachment.materializationAuthority(), errorIs("materializationAuthority", "program_materialization_not_ready"));
    current(own(held, observe(attachment, 2n, [[0, 0, 0]])), 2n, "incomplete");
    assert.equal(host.operations.at(-1), "confirmExact");
    const calls = host.operations.length;
    for (const [revision, background] of [[1n, [255, 255, 255]], [2n, [255, 255, 255]]]) {
      assert.throws(() => own(held, observe(attachment, revision, [background])), errorIs("attachmentUpdateObserved", "program_attachment_update"));
      assert.equal(host.operations.length, calls);
    }
    current(own(held, observe(attachment, 3n, [[255, 255, 255]])), 3n, "ready", [24, 24, 24]);
    assert.equal(host.operations.at(-1), "setAll"); assert.notEqual(host.point, null);
    assert.deepEqual([...authority.terminalCompositeRgb()], [24, 24, 24]);
    current(first, 1n, "ready", [24, 24, 24]);
  } finally { release(held); host.point = null; attachment.dispose(true); attachment.free(); }
});

test("V3 known unsupported и malformed wire различаются на обоих общих входах", () => {
  const valid = graph().finish();
  const unsupported = graph().finiteTarget(22, [{ id: 201, rgb: [0, 0, 0], opacity: 1 }]).finish();
  const malformed = valid.slice(); malformed[4] = 255;
  for (const [operation, create] of [["compileProgramWire", (wire) => compileProgramWire(wire, 1)],
    ["attachProgramWire", (wire) => attachProgramWire(wire, 1, 91, 501, 71, 61, () => true)]]) {
    for (const [wire, code] of [[unsupported, "program_unsupported"], [malformed, "program_wire"],
      [graph({ assignment: [99] }).finish(), "program_compile"]]) {
      assert.throws(() => create(wire), errorIs(operation, code));
    }
    const control = create(valid);
    if (operation === "attachProgramWire") control.dispose(true);
    control.free();
  }
});
