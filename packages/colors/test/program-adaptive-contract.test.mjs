import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "@labpics/colors";
import {
  ProgramWireBuilderV2, SURROUND_AVERAGE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC143_TEXT_DEFAULT_V1,
} from "@labpics/colors/program-wire/abi-v2.js";

await init({
  module_or_path: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))),
});

// Контекст принадлежит тестовому клиенту. Exact-предикаты ниже не используют
// appearance-модель; эти параметры не являются порогами выбора цвета.
const viewing = { adaptingLuminance: 64, backgroundRatio: 0.2, surround: SURROUND_AVERAGE_V1 };
const identity = (id) => id;
const reverseIds = (id) => 0xffff_ffff - id;
const sameRgb = (a, b) => a.every((channel, i) => channel === b[i]);
const differentRgb = ([r, g, b]) => [r ^ 1, g, b];
const value = (rgb, quarters) => ({ rgb, quarters });

// Независимый целочисленный oracle: source-over с alpha=q/4 и half-up
// округлением после КАЖДОГО occurrence в объявленном encoded-sRGB8 domain.
// Ни solver, ни его floating-point helper не участвуют в ожидаемом результате.
function over({ rgb, quarters }, backdrop) {
  const q = BigInt(quarters);
  return rgb.map((channel, i) => Number(
    (BigInt(channel) * q + BigInt(backdrop[i]) * (4n - q) + 2n) / 4n,
  ));
}

function states(lower, upper) {
  return lower.flatMap((_, a) => upper.map((_, b) => [a, b]));
}

function admissible(fixture, backgrounds, order = fixture.order) {
  return order.filter(([a, b]) => backgrounds.every((backdrop) => {
    const lower = over(fixture.lower[a], backdrop);
    const terminal = over(fixture.upper[b], lower);
    return sameRgb(lower, fixture.expectedLower) && sameRgb(terminal, fixture.expectedFinal);
  }));
}

function graph(fixture, { id = identity, reverse = false, order = fixture.order, report = true } = {}) {
  const b = new ProgramWireBuilderV2();
  const declarations = [
    [21, fixture.lower], [22, fixture.upper],
  ];
  if (reverse) declarations.reverse();
  for (const [target, candidates] of declarations) {
    const values = candidates.map((candidate, index) => ({
      id: id(index + 1), rgb: candidate.rgb, opacity: candidate.quarters / 4,
    }));
    b.finiteTarget(id(target), reverse ? values.reverse() : values);
  }
  b.selectionRelease(1n, order.map(([a, c], index) => {
    const choices = [{ target: id(21), candidate: id(a + 1) }, { target: id(22), candidate: id(c + 1) }];
    // Singleton ranks: state keys не вносят дополнительного tie-break.
    return [{ id: index + 1, choices: reverse ? choices.reverse() : choices }];
  }));
  b.surfaceInputPort(id(31)).solidPaint(id(41), id(21)).solidPaint(id(42), id(22))
    .inputSurface(id(51), id(31)).occurrenceSurface(id(52), id(61));
  const occurrences = [[61, 41, 51], [62, 42, 52]];
  if (reverse) occurrences.reverse();
  for (const [occurrence, paint, surface] of occurrences) {
    b.sourceOverOccurrence(id(occurrence), id(paint), id(surface),
      viewing.adaptingLuminance, viewing.backgroundRatio, viewing.surround);
  }
  b.presentationRoot(id(71), id(62)).presentationTarget(id(71), id(62));
  const constraints = [[81, 61, fixture.expectedLower], [82, 62, fixture.expectedFinal]];
  if (reverse) constraints.reverse();
  for (const [constraint, occurrence, expected] of constraints) {
    b.exactVisibleUnary(true, id(constraint), id(occurrence), expected);
  }
  if (report) b.exactVisibleUnary(false, id(83), id(62), differentRgb(fixture.expectedFinal));
  const outputs = [[91, 41], [92, 42]];
  if (reverse) outputs.reverse();
  for (const [slot, paint] of outputs) b.output(id(slot), id(paint));
  return b.finish();
}

function observe(owner, revision, backgrounds) {
  return owner.updateObserved(revision,
    new Uint32Array(backgrounds.map((_, index) => index + 101)),
    new Uint8Array(backgrounds.flat()), 1);
}

function outputs(snapshot) {
  const result = new Map();
  for (let index = 0; index < snapshot.outputCount(); index += 1) {
    const slot = snapshot.outputSlot(index);
    assert.ok(!result.has(slot), "output slots must be unique");
    result.set(slot, { rgb: [...snapshot.outputRgb(index)], opacity: snapshot.outputOpacity(index) });
  }
  return result;
}

function assertChoice(snapshot, fixture, choice, id = identity) {
  assert.equal(snapshot.state, "ready");
  const [a, b] = choice;
  const selected = [fixture.lower[a], fixture.upper[b]];
  assert.equal(snapshot.outputCount(), selected.length);
  assert.deepEqual(outputs(snapshot), new Map(selected.map((candidate, index) => [
    id(91 + index), { rgb: candidate.rgb, opacity: candidate.quarters / 4 },
  ])));
}

function fixture(first, qa, second, qb, backdrop) {
  const expectedLower = over(value(first, qa), backdrop);
  const expectedFinal = over(value(second, qb), expectedLower);
  const lower = [value(first, qa), value(expectedLower, 4), value(differentRgb(expectedLower), 4)];
  const upper = [value(second, qb), value(expectedFinal, 4), value(differentRgb(expectedFinal), 4)];
  // Сначала недопустимые варианты: предпочтение не может отменить ни один hard.
  const priority = [2, 0, 1];
  const order = priority.flatMap((a) => priority.map((b) => [a, b]));
  return { lower, upper, expectedLower, expectedFinal, order };
}

test("finite declarations obey joint constraints, caller preference and every context", () => {
  // Границы каналов и разные цветовые направления — входы клиента, не golden-палитра.
  const palettes = [[0, 1, 255], [1, 127, 254], [127, 128, 129], [255, 254, 0]];
  let adaptations = 0, multipleChoices = 0, rejectedChoices = 0;
  const selectedStates = new Set();
  for (let seed = 0; seed < palettes.length; seed += 1) {
    const first = palettes[seed], second = palettes[(seed + 1) % palettes.length];
    const backdrop = palettes[(seed + 2) % palettes.length], alternate = palettes[(seed + 3) % palettes.length];
    for (const qa of [0, 1, 2, 3]) {
      for (const qb of [0, 1, 2, 3]) {
        const declaration = fixture(first, qa, second, qb, backdrop);
        for (const order of [declaration.order, [...declaration.order].reverse()]) {
          for (const reverse of [false, true]) {
            const id = reverse ? reverseIds : identity;
            const runtime = compileProgramWire(graph(declaration, { order, id, reverse }), 1);
            const held = [];
            let previous;
            try {
              const contexts = [[backdrop], [alternate], [backdrop, alternate], [alternate, backdrop, backdrop]];
              for (const [index, backgrounds] of contexts.entries()) {
                const feasible = admissible(declaration, backgrounds, order);
                assert.ok(feasible.length > 0, "the authored opaque pair is a lawful solution");
                if (feasible.length > 1) multipleChoices += 1;
                if (feasible.length < order.length) rejectedChoices += 1;
                const chosen = feasible[0], key = chosen.join(",");
                if (previous !== undefined && key !== previous) adaptations += 1;
                previous = key;
                selectedStates.add(key);
                const snapshot = observe(runtime, BigInt(index + 1), backgrounds);
                held.push(snapshot);
                assertChoice(snapshot, declaration, chosen, id);
              }
              const initial = admissible(declaration, [backdrop], order)[0];
              assertChoice(held[0], declaration, initial, id);
            } finally {
              for (const snapshot of held.reverse()) snapshot.free();
              runtime.free();
            }
          }
        }
      }
    }
  }
  assert.ok(adaptations > 0, "the contexts must actually change selected authored values");
  assert.ok(multipleChoices > 0, "preference must decide between admissible states");
  assert.ok(rejectedChoices > 0, "hard constraints must reject preferred states");
  assert.ok(selectedStates.size > 1, "a constant output must not satisfy this corpus");
});

test("client preference preserves atomic opacity and report-only constraints", () => {
  const backdrop = [0, 0, 0];
  const lower = [value([3, 5, 7], 2), value([2, 3, 4], 4)];
  const expectedLower = over(lower[0], backdrop);
  const upperFirst = value([5, 7, 9], 2);
  const expectedFinal = over(upperFirst, expectedLower);
  const upper = [upperFirst, value(expectedFinal, 4)];
  const declaration = { lower, upper, expectedLower, expectedFinal, order: states(lower, upper) };
  const results = [];
  for (const order of [declaration.order, [...declaration.order].reverse()]) {
    assert.equal(admissible(declaration, [backdrop], order).length, order.length);
    for (const report of [false, true]) {
      const runtime = compileProgramWire(graph(declaration, { order, report }), 1);
      let snapshot;
      try {
        snapshot = observe(runtime, 1n, [backdrop]);
        assertChoice(snapshot, declaration, order[0]);
        results.push(outputs(snapshot));
      } finally { snapshot?.free(); runtime.free(); }
    }
  }
  assert.deepEqual(results[0], results[1]);
  assert.deepEqual(results[2], results[3]);
  assert.notDeepEqual(results[0], results[2], "changing the declared policy must remain effective");
});

test("a finite attachment revokes an impossible context and recovers without rewriting intent", () => {
  const backdrop = [0, 0, 0], other = [255, 255, 255];
  const lower = [value([3, 5, 7], 2), value([7, 11, 15], 1)];
  const expectedLower = over(lower[0], backdrop);
  const upper = [value([5, 7, 9], 2)];
  const expectedFinal = over(upper[0], expectedLower);
  const declaration = { lower, upper, expectedLower, expectedFinal, order: states(lower, upper) };
  assert.equal(admissible(declaration, [backdrop]).length, lower.length);
  assert.deepEqual(admissible(declaration, [backdrop, other]), []);
  const host = { point: null, operations: [] };
  const attachment = attachProgramWire(graph(declaration), 1, 92, 501, 71, 62, (intent) => {
    host.operations.push(intent.operation);
    host.point = intent.point;
    return true;
  });
  const held = [];
  try {
    const initial = observe(attachment, 1n, [backdrop]); held.push(initial);
    assertChoice(initial, declaration, declaration.order[0]);
    const authority = attachment.materializationAuthority(); held.push(authority);
    assert.deepEqual([...authority.terminalCompositeRgb()], expectedFinal);
    const before = outputs(initial);
    const conflict = observe(attachment, 2n, [backdrop, other]); held.push(conflict);
    assert.equal(conflict.state, "failed");
    assert.equal(host.point, null);
    assert.equal(host.operations.at(-1), "revokeAll");
    assert.throws(() => attachment.materializationAuthority(), (error) =>
      isProgramError(error) && error.code === "program_materialization_not_ready");
    const unknown = attachment.updateUnknown(3n, 1); held.push(unknown);
    assert.equal(unknown.state, "stale");
    const recovered = observe(attachment, 4n, [backdrop]); held.push(recovered);
    assertChoice(recovered, declaration, declaration.order[0]);
    const current = attachment.materializationAuthority(); held.push(current);
    assert.equal(current.revision(), 4n);
    assert.deepEqual([...current.terminalCompositeRgb()], expectedFinal);
    assert.deepEqual(outputs(initial), before);
    assert.equal(authority.revision(), 1n);
    // Retained outputs в failed/stale не объявляются свежим результатом.
  } finally {
    for (const handle of held.reverse()) handle.free();
    host.point = null;
    attachment.dispose(true);
    attachment.free();
  }
});

test("finite selection requires the whole declared product, not merely the right count", () => {
  const declaration = fixture([3, 5, 7], 2, [5, 7, 9], 1, [0, 0, 0]);
  const incomplete = declaration.order.map((choice) => [...choice]);
  incomplete[incomplete.length - 1] = [...incomplete[0]];
  assert.equal(incomplete.length, declaration.order.length);
  let accidental;
  try {
    assert.throws(() => { accidental = compileProgramWire(graph(declaration, { order: incomplete }), 1); },
      (error) => isProgramError(error) && ["program_compile", "program_wire"].includes(error.code));
  } finally { accidental?.free(); }
  const runtime = compileProgramWire(graph(declaration), 1);
  let healthy;
  try {
    healthy = observe(runtime, 1n, [[0, 0, 0]]);
    assertChoice(healthy, declaration, admissible(declaration, [[0, 0, 0]])[0]);
  } finally { healthy?.free(); runtime.free(); }
});

test("contrast declarations adapt across the entire sRGB corner cube without a palette recipe", () => {
  const corners = Array.from({ length: 2 ** 3 }, (_, bits) =>
    [0, 1, 2].map((channel) => ((bits >> channel) & 1) * 255));
  // WCAG 2.2, 12 December 2024, relative luminance and contrast ratio:
  // https://www.w3.org/TR/2024/REC-WCAG22-20241212/#dfn-relative-luminance
  // На концах sRGB transfer равен точно 0/1. Поэтому веса 0.2126/0.7152/0.0722
  // и добавка 0.05 представлены целыми с общим знаменателем 10000, без pow/epsilon.
  const luminance = (rgb) => [2126n, 7152n, 722n].reduce(
    (sum, weight, channel) => sum + (rgb[channel] === 255 ? weight : 0n), 0n);
  const criteria = [
    { id: WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, numerator: 3n, denominator: 1n },
    { id: WCAG22_SC143_TEXT_DEFAULT_V1, numerator: 9n, denominator: 2n },
  ];
  const contexts = [
    [corners[0]], [corners[7]], [corners[1], corners[2]],
    [corners[0], corners[7]], corners, [corners[0]],
  ];
  const chosenColors = new Set();
  let ready = 0, failed = 0;
  for (const criterion of criteria) {
    for (let offset = 0; offset < corners.length; offset += 1) {
      const preference = corners.map((_, index) => (index + offset) % corners.length);
      const bytes = new ProgramWireBuilderV2()
        .finiteTarget(21, corners.map((rgb, index) => ({ id: index + 1, rgb, opacity: 1 })))
        .selectionRelease(1n, preference.map((index) => [{
          id: index + 1, choices: [{ target: 21, candidate: index + 1 }],
        }]))
        .surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
        .sourceOverOccurrence(61, 41, 51, viewing.adaptingLuminance, viewing.backgroundRatio, viewing.surround)
        .presentationRoot(71, 61).presentationTarget(71, 61)
        .wcag22VisibleUnary(true, 81, 61, criterion.id).output(91, 41).finish();
      const runtime = compileProgramWire(bytes, 1);
      const held = [];
      try {
        for (const [revision, backgrounds] of contexts.entries()) {
          const chosen = preference.find((index) => backgrounds.every((backdrop) => {
            const a = luminance(corners[index]), b = luminance(backdrop);
            const light = a > b ? a : b, dark = a > b ? b : a;
            return (light + 500n) * criterion.denominator >= (dark + 500n) * criterion.numerator;
          }));
          const snapshot = observe(runtime, BigInt(revision + 1), backgrounds); held.push(snapshot);
          if (chosen === undefined) {
            assert.equal(snapshot.state, "failed");
            failed += 1;
          } else {
            assert.equal(snapshot.state, "ready");
            assert.deepEqual(outputs(snapshot), new Map([[91, { rgb: corners[chosen], opacity: 1 }]]));
            chosenColors.add(chosen);
            ready += 1;
          }
        }
      } finally {
        for (const snapshot of held.reverse()) snapshot.free();
        runtime.free();
      }
    }
  }
  assert.ok(ready > 0 && failed > 0, "both useful selection and impossible declarations are exercised");
  assert.ok(chosenColors.size > 2, "a black/white or lightest-color recipe must not pass");
});
