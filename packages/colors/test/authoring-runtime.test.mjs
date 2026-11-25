import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors, evaluateWcag22 } from "../index.js";


await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const hex = (rgb) => "#" + rgb.map((channel) => channel.toString(16).padStart(2, "0")).join("");
const textPasses = (rgb, background) => evaluateWcag22(hex(rgb), hex(background), "sc-1.4.3-text-default").decision === "pass";

test("a whole affine source resolves as an immutable modeled frame through the real Core", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const program = await c.compile({ ink: c.fixed("ink", "#ffffff").on(canvas) });
  const from = [0, 0, 0], to = [1, 0, 0];
  try {
    const frame = program.resolve({ canvas: { kind: "affine-srgb8", from, to } });
    assert.equal(frame.state, "modeled");
    assert.equal(frame.scope, "nominal-model");
    assert.deepEqual(frame.coverage, { model: "complete", rendering: "unknown",
      sourceModel: "opaque-affine-encoded-srgb8-half-up-v1" });
    assert.deepEqual(frame.values.ink.rgb, [255, 255, 255]);
    assert.equal(frame.field.regionCount, 5);
    assert.deepEqual(frame.field.witnesses, [
      { coordinate: [0, 1], nominal: [0, 0, 0] },
      { coordinate: [1, 2], nominal: [1, 0, 0] },
    ]);
    from[0] = 255;
    to[1] = 255;
    assert.deepEqual(frame.context.canvas, { kind: "affine-srgb8", from: [0, 0, 0], to: [1, 0, 0] });
    assert.equal(frame.contextCss.canvas, "linear-gradient(90deg in srgb, rgb(0 0 0 / 1) 0%, rgb(1 0 0 / 1) 100%)");
    const reversed = program.resolve({ canvas: { kind: "affine-srgb8", from: [1, 0, 0], to: [0, 0, 0] } });
    assert.equal(reversed.state, "modeled");
    assert.deepEqual(reversed.values, frame.values);
    assert.notDeepEqual(reversed.context, frame.context);
    assert.deepEqual(frame.field.witnesses[1], { coordinate: [1, 2], nominal: [1, 0, 0] });
    const constant = program.resolve({ canvas: { kind: "affine-srgb8", from: "#010000", to: "#010000" } });
    assert.equal(constant.state, "modeled", "equal endpoints preserve the declared field kind");
    assert.equal(constant.field.regionCount, 3);
    assert.equal(constant.field.witnesses.length, 1);
    assert.deepEqual(constant.context.canvas, { kind: "affine-srgb8", from: [1, 0, 0], to: [1, 0, 0] });
  } finally { program.dispose(); }
});

test("one composition covers every positioned segment and retains a tie-only witness", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const program = await c.compile({ ink: c.fixed("ink", "#ffffff").on(canvas, { contrast: "wcag22-text" }) });
  const stops = [{at: 0, color: [2, 0, 0]}, {at: 0.125, color: [2, 1, 0]}, {at: 1, color: [1, 2, 0]}];
  try {
    const frame = program.resolve({ canvas: { kind: "linear-srgb8", stops } });
    assert.equal(frame.state, "modeled");
    assert.equal(frame.field.regionCount, 9);
    assert.equal(frame.coverage.rendering, "unknown");
    assert.deepEqual(frame.values.ink.rgb, [255, 255, 255]);
    assert.deepEqual(frame.field.witnesses.find(w => w.nominal.join() === "2,2,0"),
      {segmentIndex: 1, coordinate: [1, 2], nominal: [2, 2, 0]});
    stops[1].color[0] = 255;
    const rejected = program.resolve({canvas: {kind: "linear-srgb8", stops: [
      {at: 0, color: "#000000"}, {at: 0.1, color: "#ffffff"}, {at: 1, color: "#000000"}]}});
    assert.equal(rejected.state, "conflict");
    assert.equal(rejected.coverage.model, "violated");
    assert.ok(rejected.diagnostics.some(d => d.fieldWitness !== undefined));
    assert.equal(program.resolve({canvas: "#000000"}).state, "ready");
    assert.deepEqual(frame.context.canvas.stops[1], {at: 0.125, color: [2, 1, 0]});
  } finally { program.dispose(); }
});

test("a role adapts once across all gradient segments when the stop list changes", async () => {
  const c = colors(), canvas = c.surface("canvas"), ink = c.role("ink", "#666666").on(canvas, {contrast: "wcag22-text"});
  const program = await c.compile({ink});
  const gradient = gray => ({canvas: {kind: "linear-srgb8",
    stops: gray.map((value, i) => ({at: [0, 0.35, 1][i], color: [value, value, value]}))}});
  try {
    const light = program.resolve(gradient([255, 221, 238])), dark = program.resolve(gradient([17, 51, 34]));
    for (const [frame, lo, hi] of [[light, 221, 255], [dark, 17, 51]]) {
      assert.equal(frame.state, "modeled");
      for (let gray = lo; gray <= hi; gray++) assert.ok(textPasses(frame.values.ink.rgb, [gray, gray, gray]));
    }
    assert.notDeepEqual(light.values.ink.rgb, dark.values.ink.rgb);
  } finally { program.dispose(); }
});

test("a changed material field recovers the same frame after an unknown observation as an independent program", async () => {
  const c = colors({ reference: { canvas: "#e6ddd0" } }), canvas = c.surface("canvas");
  const fill = c.role("fill", "#0067f4").on(canvas, { separation: [0.1, 2], opacity: 0.9 });
  const label = c.role("label", "#242424").on(fill, { contrast: "wcag22-text" });
  const gradient = end => ({ canvas: { kind: "affine-srgb8", from: "#d7c5a8", to: end } });
  const program = await c.compile({ fill, label }), fresh = await c.compile({ fill, label });
  try {
    assert.equal(program.resolve(gradient("#dbc9a9")).state, "modeled");
    const unknown = program.resolve({ canvas: null });
    assert.ok(unknown.diagnostics.some(item => item.kind === "unknown-input" && item.input === "canvas"));
    assert.equal("values" in unknown, false);
    const changed = gradient("#dbcaac");
    const actual = program.resolve(changed), expected = fresh.resolve(changed);
    assert.equal(actual.state, "modeled");
    assert.deepEqual(actual, { ...expected, revision: actual.revision });
  } finally {
    program.dispose();
    fresh.dispose();
  }
});

test("an interior field failure is scoped to the nominal model with a Core coordinate witness", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const program = await c.compile({ ink: c.fixed("ink", "#808080").on(canvas, { separation: [0.1, 2] }) });
  try {
    assert.equal(program.resolve({ canvas: "#000000" }).state, "ready");
    assert.equal(program.resolve({ canvas: "#ffffff" }).state, "ready");
    const frame = program.resolve({ canvas: { kind: "affine-srgb8", from: "#000000", to: "#ffffff" } });
    assert.equal(frame.state, "conflict");
    assert.equal(frame.scope, "nominal-model");
    assert.equal(frame.coverage.model, "violated");
    assert.equal(frame.coverage.rendering, "unknown");
    assert.equal("values" in frame, false);
    const violation = frame.diagnostics.find((item) => item.requirement === "separation");
    assert.ok(violation.fieldWitness);
    assert.equal(violation.fieldWitness.input, "canvas");
    assert.deepEqual(violation.fieldWitness.coordinate, frame.field.witnesses[violation.caseIndex].coordinate);
    const [numerator, denominator] = violation.fieldWitness.coordinate;
    assert.ok(numerator > 0 && numerator < denominator);
  } finally { program.dispose(); }
});

test("affine contexts reject multiple fields and retain missing constant inputs as unknown", async () => {
  const c = colors(), first = c.surface("first"), second = c.surface("second");
  const program = await c.compile({ a: c.fixed("a", "#000000").on(first), b: c.fixed("b", "#ffffff").on(second) });
  const field = { kind: "affine-srgb8", from: "#000000", to: "#010000" };
  try {
    assert.throws(() => program.resolve({ first: field, second: field }), { code: "color_unsupported_context" });
    const missing = program.resolve({ first: field });
    assert.equal(missing.state, "unknown");
    assert.deepEqual(missing.context.first, { kind: "affine-srgb8", from: [0, 0, 0], to: [1, 0, 0] });
    assert.equal("coverage" in missing, false);
    assert.deepEqual(missing.diagnostics, [{ kind: "unknown-input", input: "second" }]);
    assert.equal(program.resolve({ first: "#ffffff", second: "#000000" }).state, "ready");
  } finally { program.dispose(); }
});

test("context inputs require own properties even when their names exist on the prototype", async () => {
  for (const name of ["toString", "constructor", "__proto__", "hasOwnProperty"]) {
    const c = colors(), surface = c.surface(name);
    const program = await c.compile({ ink: c.fixed("ink", "#242424").on(surface) });
    try {
      const inherited = Object.create({ [name]: "#ffffff" });
      const hostile = Object.create(Object.defineProperty({}, name, {
        get() { throw new Error("inherited input must not be read"); },
      }));
      for (const context of [{}, inherited, hostile]) {
        const frame = program.resolve(context);
        assert.equal(frame.state, "unknown");
        assert.deepEqual(frame.diagnostics, [{ kind: "unknown-input", input: name }]);
      }
      const ready = program.resolve({ [name]: "#e6ddd0" });
      assert.equal(ready.state, "ready");
      assert.deepEqual(ready.context[name], [230, 221, 208]);
      assert.throws(() => program.resolve({ [name]: "not-a-color" }),
        { code: "color_invalid_context", path: `context.${name}` });
    } finally { program.dispose(); }
  }
});

test("omitted opacity is opaque and explicit zero remains transparent", async () => {
  const c = colors(), canvas = c.surface("canvas"), role = c.fixed("ink", "#242424");
  const program = await c.compile({ opaque: role.on(canvas), transparent: role.on(canvas, { opacity: 0 }) });
  try {
    const frame = program.resolve({ canvas: "#e6ddd0" });
    assert.equal(frame.state, "ready");
    assert.equal(frame.values.opaque.opacity, 1);
    assert.equal(frame.values.transparent.opacity, 0);
  } finally { program.dispose(); }
});

test("one source identity produces independent local colors through the public authoring path", async () => {
  const c = colors(), first = c.surface("first"), second = c.surface("second");
  const source = [0, 103, 244];
  const accent = c.role("accent", source);
  const a = accent.on(first, { contrast: "wcag22-text" });
  const b = accent.on(second, { contrast: "wcag22-text" });
  const program = await c.compile({ a, alias: a, b });
  try {
    source[0] = 255;
    const frame = program.resolve({ first: "#e6ddd0", second: "#283548" });
    assert.equal(frame.state, "ready");
    assert.notDeepEqual(frame.values.a.rgb, frame.values.b.rgb);
    assert.equal(frame.values.a, frame.values.alias, "reuse is one physical paint, including its output aliases");
    assert.equal(textPasses(frame.values.a.rgb, [230, 221, 208]), true);
    assert.equal(textPasses(frame.values.b.rgb, [40, 53, 72]), true);
    assert.deepEqual(accent.anchor, [0, 103, 244]);
    const changed = program.resolve({ first: "#283548", second: "#e6ddd0" });
    assert.equal(changed.state, "ready");
    assert.equal(textPasses(changed.values.a.rgb, [40, 53, 72]), true);
    assert.equal(textPasses(changed.values.b.rgb, [230, 221, 208]), true);
    assert.deepEqual(frame.context.first, [230, 221, 208], "retained copies are not overwritten by the next update");
  } finally { program.dispose(); }
});

test("equivalent local uses keep the same color when resolved together", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const accent = c.role("accent", "#0067f4");
  const first = accent.on(canvas, { contrast: "wcag22-text" });
  const second = accent.on(canvas, { contrast: "wcag22-text" });
  const combined = await c.compile({ first, second });
  const isolated = await c.compile({ first });
  try {
    for (const canvas of ["#f4eadb", "#e6ddd0", "#000000"]) {
      const frame = combined.resolve({ canvas });
      const local = isolated.resolve({ canvas });
      assert.equal(frame.state, "ready");
      assert.equal(local.state, "ready");
      assert.deepEqual(frame.values.first, local.values.first);
      assert.deepEqual(frame.values.second, local.values.first,
        "equal roles and contexts must not compete for the quality of their result");
    }
  } finally { combined.dispose(); isolated.dispose(); }
});

test("an independent local surface keeps its colors when another context changes", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const accent = c.role("accent", "#0067f4");
  const changing = accent.on(canvas, { contrast: "wcag22-text" });
  const island = c.fixed("paper", "#f4eadb").on(canvas);
  const stable = accent.on(island, { contrast: "wcag22-text" });
  const combined = await c.compile({ changing, island, stable });
  const isolated = await c.compile({ island, stable });
  let first;
  try {
    for (const canvas of ["#ffffff", "#e6ddd0", "#000000", "#ffffff"]) {
      const frame = combined.resolve({ canvas });
      const local = isolated.resolve({ canvas });
      assert.equal(frame.state, "ready");
      assert.equal(local.state, "ready");
      first ??= local.values.stable;
      assert.deepEqual(frame.values.island, local.values.island);
      assert.deepEqual(frame.values.stable, first,
        "changing an unrelated backdrop must not dull the accent on a fixed surface");
      assert.deepEqual(frame.values.stable, local.values.stable);
      assert.equal(textPasses(frame.values.stable.rgb, frame.values.island.rgb), true);
    }
  } finally { combined.dispose(); isolated.dispose(); }
});

for (const opacity of [1, 0.5]) {
  test(`equivalent uses on a computed parent retain local intent at opacity ${opacity}`, async () => {
    const c = colors(), canvas = c.surface("canvas");
    const paper = c.role("paper", "#f4eadb").on(canvas, { opacity });
    const accent = c.role("accent", "#0067f4");
    const first = accent.on(paper, { contrast: "wcag22-text" });
    const second = accent.on(paper, { contrast: "wcag22-text" });
    const combined = await c.compile({ paper, first, second });
    const isolated = await c.compile({ paper, first });
    try {
      for (const canvas of ["#f4eadb", "#e6ddd0", "#000000", "#ffffff"]) {
        const frame = combined.resolve({ canvas }), local = isolated.resolve({ canvas });
        assert.equal(frame.state, "ready");
        assert.equal(local.state, "ready");
        assert.deepEqual(frame.values.paper, local.values.paper, `parent on ${canvas}`);
        assert.deepEqual(frame.values.first, local.values.first, `first use on ${canvas}`);
        assert.deepEqual(frame.values.second, local.values.first, `second use on ${canvas}`);
      }
    } finally { combined.dispose(); isolated.dispose(); }
  });
}

test("intermediate fill and its label compile and satisfy the final pair together", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const fill = c.role("accent", "#0067f4").on(canvas);
  const label = c.role("label", "#242424").on(fill, { contrast: "wcag22-text" });
  const program = await c.compile({ fill, label });
  try {
    for (const background of ["#e6ddd0", "#283548", "#6b7f72"]) {
      const frame = program.resolve({ canvas: background });
      assert.equal(frame.state, "ready");
      assert.equal(textPasses(frame.values.label.rgb, frame.values.fill.rgb), true);
    }
  } finally { program.dispose(); }
});

test("fixed declarations still validate constraints and never expose historical values as current", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const ink = c.fixed("fixed", "#808080").on(canvas, { separation: [0.1, 2] });
  const program = await c.compile({ ink });
  try {
    assert.equal(program.resolve({ canvas: "#e6ddd0" }).state, "ready");
    const refused = program.resolve({ canvas: "#808080" });
    assert.equal(refused.state, "conflict");
    assert.equal("values" in refused, false);
    const witness = refused.diagnostics.find((item) => item.kind === "requirement");
    assert.equal(witness.requirement, "separation");
    assert.equal(witness.role, "fixed");
    assert.deepEqual(witness.outputs, ["ink"]);
    assert.equal(witness.evidence, "candidate-witness");
    const unknown = program.resolve({ canvas: null });
    assert.ok(unknown.state === "unknown" || unknown.state === "stale");
    assert.equal(unknown.diagnostics.some((item) => item.kind === "unknown-input" && item.input === "canvas"), true);
    assert.equal("values" in unknown, false);
  } finally { program.dispose(); }
});

test("output order, aliases and an unexported backdrop do not change graph preferences", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const panel = c.role("paper", "#aaaaaa").on(canvas, {
    tone: { from: canvas, curve: [[0, 0.15], [1, 0.95]] },
  });
  const label = c.role("ink", "#242424").on(panel, { contrast: "wcag22-text" });
  const maps = [{ panel, label }, { label, panel }, { alias: label, panel, label }, { label }];
  const frames = [];
  for (const outputs of maps) {
    const program = await c.compile(outputs);
    try {
      const frame = program.resolve({ canvas: "#283548" });
      assert.equal(frame.state, "ready");
      assert.equal(frame.optimality, "unfinished", "finite Point discovery does not certify a continuous optimum");
      assert.deepEqual(Object.keys(frame.values), Object.keys(outputs), "export order remains a readout property");
      frames.push(frame);
    } finally { program.dispose(); }
  }
  const first = frames[0];
  assert.equal(textPasses(first.values.label.rgb, first.values.panel.rgb), true);
  assert.notDeepEqual(first.values.label.rgb, [36, 36, 36], "the connected fill and label must adapt together");
  for (const frame of frames.slice(1)) {
    assert.deepEqual(frame.values.label, first.values.label);
    if (frame.values.panel !== undefined) assert.deepEqual(frame.values.panel, first.values.panel);
  }
  assert.equal(frames[2].values.alias, frames[2].values.label);
});
