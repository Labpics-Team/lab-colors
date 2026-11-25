import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors, evaluateWcag22 } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const hex = (rgb) => "#" + rgb.map((value) => value.toString(16).padStart(2, "0")).join("");
const textPasses = (foreground, background) =>
  evaluateWcag22(hex(foreground), hex(background), "sc-1.4.3-text-default").decision === "pass";
const grayL = (gray) => Math.cbrt(gray <= 10.31475
  ? gray / (255 * 12.92) : ((gray / 255 + 0.055) / 1.055) ** 2.4);

async function hierarchy({ reverse = false, extra = false, aliases = false } = {}) {
  const c = colors({ reference: { canvas: "#ffffff", ...(extra ? { aside: "#ffffff" } : {}) } });
  const canvas = c.surface("canvas");
  const roles = [c.role(reverse ? "renamed a" : "heading", "#444444"),
    c.role(reverse ? "renamed b" : "description", "#666666")];
  const uses = [];
  const tone = { from: canvas, curve: [[0, 0.7], [1, 0.7]] };
  for (const index of reverse ? [1, 0] : [0, 1]) {
    uses[index] = roles[index].on(canvas, { contrast: "wcag22-text", tone });
  }
  c.preserveTonalHierarchy(uses);
  const outputs = { a: uses[0], b: uses[1] };
  if (aliases) outputs.alias = uses[0];
  if (extra) outputs.other = c.role("unrelated", "#3155be").on(c.surface("aside"), { contrast: "wcag22-text" });
  return { program: await c.compile(outputs), uses };
}

test("ordinary compose balances a connected Point hierarchy independently of creation order and names", async () => {
  const left = await hierarchy(), right = await hierarchy({ reverse: true, aliases: true });
  try {
    const first = left.program.resolve({ canvas: "#000000" });
    const renamed = right.program.resolve({ canvas: "#000000" });
    for (const frame of [first, renamed]) {
      assert.equal(frame.state, "ready");
      assert.equal(frame.optimality, "unfinished", "finite proposals do not prove a continuous optimum");
      assert.deepEqual(frame.selection.map((component) => component.mode), ["joint-point-v1"]);
      assert.equal(frame.reference.length, 2, "Point selection retains reference observations");
      for (const reference of frame.reference) {
        assert.equal(reference.preference, "explicit-tone");
        assert.equal(reference.capacity, null, "explicit tone does not claim implicit strength capacity");
        assert.equal(reference.cases.length, 1);
        assert.deepEqual(reference.cases[0].actualBackdrop, [0, 0, 0]);
        assert.deepEqual(reference.cases[0].actualVisible, frame.values[reference.outputs[0]].rgb);
        assert.ok(Object.isFrozen(reference.cases[0].actualVisible));
      }
      assert.ok(textPasses(frame.values.a.rgb, [0, 0, 0]));
      assert.ok(textPasses(frame.values.b.rgb, [0, 0, 0]));
      // Independent gray-domain witness: either ordered extreme loses > .13 L.
      // The known feasible balanced tuple [181,137] loses < .074 on both uses.
      const worstLoss = Math.max(...[frame.values.a, frame.values.b].map((value) => Math.abs(grayL(value.rgb[0]) - 0.7)));
      assert.ok(worstLoss < 0.1, `a feasible balanced result must beat the ordered extremes: ${worstLoss}`);
    }
    assert.deepEqual(first.values.a, renamed.values.a);
    assert.deepEqual(first.values.b, renamed.values.b);
    assert.deepEqual(renamed.values.alias, renamed.values.a);
    assert.deepEqual(new Set(first.selection[0].uses.map((use) => use.use)), new Set(left.uses));
    assert.ok(Object.isFrozen(first.selection));
    assert.ok(Object.isFrozen(first.selection[0].uses));
  } finally { left.program.dispose(); right.program.dispose(); }
});

test("an independent implicit-reference component neither changes Point choice nor changes its selection mode", async () => {
  const plain = await hierarchy(), extended = await hierarchy({ extra: true });
  try {
    const baseline = plain.program.resolve({ canvas: "#000000" });
    for (const aside of ["#ffffff", "#808080", "#000000"]) {
      const frame = extended.program.resolve({ canvas: "#000000", aside });
      assert.equal(frame.state, "ready");
      assert.equal(frame.optimality, "unfinished", "the Point hierarchy has only a byte-domain result");
      assert.deepEqual(frame.values.a, baseline.values.a);
      assert.deepEqual(frame.values.b, baseline.values.b);
      assert.deepEqual(frame.selection.map((component) => component.mode).sort(), ["joint-point-v1", "ordered-reference-v1"]);
      const point = frame.selection.find((component) => component.mode === "joint-point-v1");
      assert.deepEqual(new Set(point.uses.flatMap((use) => use.outputs)), new Set(["a", "b"]));
    }
  } finally { plain.program.dispose(); extended.program.dispose(); }
});

test("ordinary Point search constructs a readable first result from an infeasible authored source", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const ink = c.role("ink", "#eeeeee").on(canvas, { contrast: "wcag22-text" });
  const program = await c.compile({ ink });
  try {
    const white = program.resolve({ canvas: "#ffffff" });
    assert.equal(white.state, "ready");
    assert.equal(white.optimality, "unfinished", "the feasible byte repair has no continuous proof");
    assert.ok(textPasses(white.values.ink.rgb, [255, 255, 255]));
    assert.notDeepEqual(white.values.ink.rgb, [238, 238, 238]);
    assert.equal(white.selection[0].mode, "joint-point-v1");
    const black = program.resolve({ canvas: "#000000" });
    assert.equal(black.state, "ready");
    assert.equal(black.optimality, "exact", "the authored color satisfies contrast and has zero loss");
    assert.ok(textPasses(black.values.ink.rgb, [0, 0, 0]));
    assert.deepEqual(program.resolve({ canvas: "#ffffff" }).values, white.values);
    assert.ok(Object.isFrozen(white.values));
  } finally { program.dispose(); }
});

test("an independent completed reference search preserves the proven optimum of a constant Point use", async () => {
  const c = colors({ reference: { canvas: "#ffffff", paper: "#ffffff" } });
  const canvas = c.surface("canvas"), paper = c.surface("paper");
  const overlay = c.role("overlay", "#888888").on(canvas, {
    opacity: 0, tone: { from: canvas, curve: [[0, 0.55], [1, 0.55]] },
  });
  const ink = c.role("ink", "#222222").on(paper, { contrast: "wcag22-text" });
  const program = await c.compile({ overlay, ink });
  try {
    // Only the ink's support changes. The reused Point proof stays attached to its own component.
    for (const paper of ["#ffffff", "#000000", "#000000", "#ffffff"]) {
      const frame = program.resolve({ canvas: "#ffffff", paper });
      assert.equal(frame.state, "ready");
      assert.deepEqual(frame.selection.map((component) => component.mode).sort(), ["joint-point-v1", "ordered-reference-v1"]);
      assert.equal(frame.optimality, "exact");
      assert.deepEqual(frame.values.overlay.rgb, [136, 136, 136]);
      if (paper === "#000000") assert.notDeepEqual(frame.values.ink.rgb, [34, 34, 34]);
    }
  } finally { program.dispose(); }
});

test("ordinary composition adapts nested paints on translucent material from their authored colors", async () => {
  const paper = [217, 195, 166];
  // Exhausting the 256³ source bytes under the same ±3 emission bounds,
  // visible identity and contrast gives these source paints at minimum
  // continuous Point distance. Several nearby paints round to the same pixel.
  for (const [anchor, closestSource, closestVisible] of [
    ["#0d6efd", [0, 29, 132], [98, 104, 147]],
    ["#0066cc", [0, 33, 108], [98, 106, 134]],
  ]) {
    const c = colors(), surface = c.surface("paper");
    const panel = c.role("action", anchor).on(surface, { opacity: 0.55, contrast: "wcag22-ui" });
    const text = c.role("caption", "#ffffff").on(panel, { contrast: "wcag22-text" });
    const program = await c.compile({ panel, text });
    try {
      const frame = program.resolve({ paper });
      assert.equal(frame.state, "ready");
      assert.equal(frame.optimality, "unfinished");
      const paint = frame.values.panel;
      assert.equal(paint.opacity, 0.55);
      assert.deepEqual(paint.rgb, closestSource);
      const visible = paint.rgb.map((channel, i) => Math.round(paper[i] + paint.opacity * (channel - paper[i])));
      assert.deepEqual(visible, closestVisible);
      assert.equal(evaluateWcag22(hex(visible), hex(paper), "sc-1.4.11-ui-component-or-state").decision, "pass");
      assert.deepEqual(frame.values.text.rgb, [255, 255, 255]);
      assert.ok(textPasses(frame.values.text.rgb, visible));
    } finally { program.dispose(); }
  }
});

test("a translucent warm fill keeps dark ink when both paints must move to make it readable", async () => {
  const c = colors(), surface = c.surface("paper");
  const fill = c.role("fill", "#ce7332").on(surface, { opacity: 0.72, separation: [0.02, 0.6] });
  const ink = c.role("ink", "#242424").on(fill, { contrast: "wcag22-text" });
  const program = await c.compile({ fill, ink });
  try {
    const frame = program.resolve({ paper: "#000000" });
    assert.equal(frame.state, "ready");
    assert.equal(frame.optimality, "unfinished");
    const paint = frame.values.fill;
    const visible = paint.rgb.map((channel) => Math.round(paint.opacity * channel));
    assert.ok(textPasses(frame.values.ink.rgb, visible));
    // Black is within 0.27 L of the authored ink and admits a warm fill.
    // Inverting the ink to a pale gray loses more than this feasible assignment.
    const [r, g, b] = frame.values.ink.rgb;
    assert.ok(r === g && g === b);
    assert.ok(Math.abs(grayL(r) - grayL(36)) < 0.27);
  } finally { program.dispose(); }
});

test("moving a translucent fill reopens the choice of its neutral ink", async () => {
  const c = colors(), surface = c.surface("paper");
  const fill = c.role("fill", "#0066cc").on(surface, { opacity: 0.55, contrast: "wcag22-ui" });
  const ink = c.role("ink", "#555555").on(fill, { contrast: "wcag22-text" });
  const program = await c.compile({ fill, ink });
  try {
    const paper = [250, 244, 238];
    const frame = program.resolve({ paper });
    assert.equal(frame.state, "ready");
    const paint = frame.values.fill;
    assert.equal(paint.opacity, 0.55);
    const visible = paint.rgb.map((v, i) => Math.round(paper[i] + paint.opacity * (v - paper[i])));
    const low = visible.map((v) => Math.max(0, v - 3));
    const high = visible.map((v) => Math.min(255, v + 3));
    assert.equal(evaluateWcag22(hex(high), hex(paper), "sc-1.4.11-ui-component-or-state").decision, "pass");
    // Compare every neutral ink against both ends of the fill's emission range.
    // For fixed fill, a smaller ink loss strictly improves the full sorted profile.
    const distance = (gray) => Math.abs(grayL(gray) - grayL(85));
    const neutral = (gray) => [gray, gray, gray];
    const admissible = Array.from({ length: 256 }, (_, gray) => gray)
      .filter((gray) => textPasses(neutral(gray), low) && textPasses(neutral(gray), high));
    const closest = admissible.reduce((a, b) => distance(a) < distance(b) ? a : b);
    assert.deepEqual(frame.values.ink.rgb, neutral(closest));
    // Independently feasible pair: source #0022ba with ink #1b1b1b.
    assert.ok(distance(closest) <= distance(27));
  } finally { program.dispose(); }
});
