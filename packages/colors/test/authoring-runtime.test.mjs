import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors, evaluateWcag22 } from "../index.js";


await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const hex = (rgb) => "#" + rgb.map((channel) => channel.toString(16).padStart(2, "0")).join("");
const textPasses = (rgb, background) => evaluateWcag22(hex(rgb), hex(background), "sc-1.4.3-text-default").decision === "pass";

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

test("equivalent uses on a computed parent retain the same local intent", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const paper = c.role("paper", "#f4eadb").on(canvas);
  const accent = c.role("accent", "#0067f4");
  const first = accent.on(paper, { contrast: "wcag22-text" });
  const second = accent.on(paper, { contrast: "wcag22-text" });
  const combined = await c.compile({ paper, first, second });
  const isolated = await c.compile({ paper, first });
  try {
    for (const canvas of ["#e6ddd0", "#000000", "#ffffff"]) {
      const frame = combined.resolve({ canvas }), local = isolated.resolve({ canvas });
      assert.equal(frame.state, "ready");
      assert.equal(local.state, "ready");
      assert.deepEqual(frame.values.paper, local.values.paper);
      assert.deepEqual(frame.values.first, local.values.first);
      assert.deepEqual(frame.values.second, local.values.first);
    }
  } finally { combined.dispose(); isolated.dispose(); }
});

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
      assert.equal(frame.optimality, "exact", "the bounded neutral domains complete this preference comparison");
      assert.deepEqual(Object.keys(frame.values), Object.keys(outputs), "export order remains a readout property");
      frames.push(frame);
    } finally { program.dispose(); }
  }
  const first = frames[0];
  assert.equal(textPasses(first.values.label.rgb, first.values.panel.rgb), true);
  assert.notDeepEqual(first.values.label.rgb, [36, 36, 36], "the earlier panel preference can require changing its later label");
  for (const frame of frames.slice(1)) {
    assert.deepEqual(frame.values.label, first.values.label);
    if (frame.values.panel !== undefined) assert.deepEqual(frame.values.panel, first.values.panel);
  }
  assert.equal(frames[2].values.alias, frames[2].values.label);
});
