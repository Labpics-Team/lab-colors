import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors, evaluateWcag22 } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// Independent sRGB luminance/contrast calculation on the gray axis.
const luminance = (gray) => gray <= 10.31475 ? gray / (255 * 12.92) : ((gray / 255 + 0.055) / 1.055) ** 2.4;
const visibleGray = (value, background) => Math.round(value.rgb[0] * value.opacity + background * (1 - value.opacity));
const hex = (rgb) => "#" + rgb.map((value) => value.toString(16).padStart(2, "0")).join("");
const textPasses = (rgb, background) => evaluateWcag22(hex(rgb), hex([background, background, background]), "sc-1.4.3-text-default").decision === "pass";
const contrast = (value, background) => {
  const visible = luminance(visibleGray(value, background)), backdrop = luminance(background);
  return (Math.max(visible, backdrop) + 0.05) / (Math.min(visible, backdrop) + 0.05);
};

test("one backdrop curve alternates light layers and raises successive dark layers", async () => {
  const c = colors(), canvas = c.surface("canvas");
  const layer = c.role("layer", "#eeeeee");
  const policy = { tone: { curve: [[0, 0.14], [0.60, 0.74], [0.94, 1], [0.96, 1], [1, 0.95]] } };
  const sheet = layer.on(canvas, policy), panel = layer.on(sheet, policy), card = layer.on(panel, policy);
  const program = await c.compile({ sheet, panel, card });
  try {
    const light = program.resolve({ canvas: "#ffffff" });
    assert.equal(light.state, "ready");
    assert.deepEqual(light.values.sheet.rgb, [238, 238, 238]);
    assert.deepEqual(light.values.panel.rgb, [255, 255, 255]);
    assert.deepEqual(light.values.card.rgb, light.values.sheet.rgb);
    const dark = program.resolve({ canvas: "#161616" });
    assert.equal(dark.state, "ready");
    const [first, second, third] = ["sheet", "panel", "card"].map((key) => dark.values[key].rgb[0]);
    assert.ok(22 < first && first < second && second < third, JSON.stringify(dark.values));
    assert.deepEqual(program.resolve({ canvas: "#ffffff" }).values, light.values);
  } finally { program.dispose(); }
});

test("backdrop tone follows the composed parent while explicit tone follows its input", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const material = c.fixed("veil", "#ffffff").on(canvas, { opacity: 0.5 });
  const role = c.role("layer", "#ffffff");
  const curve = [[0, 0], [1, 1]];
  const local = role.on(material, { tone: { curve } });
  const external = role.on(material, { tone: { from: canvas, curve } });
  const program = await c.compile({ local, external });
  try {
    const dark = program.resolve({ canvas: "#000000" });
    assert.equal(dark.state, "ready");
    const [gray, green, blue] = dark.values.local.rgb;
    assert.ok(gray === 127 || gray === 128, JSON.stringify(dark.values.local));
    assert.deepEqual([green, blue], [gray, gray]);
    assert.deepEqual(dark.values.external.rgb, [0, 0, 0]);
    const light = program.resolve({ canvas: "#ffffff" });
    assert.equal(light.state, "ready");
    assert.deepEqual(light.values.local.rgb, [255, 255, 255]);
    assert.deepEqual(light.values.local.rgb, light.values.external.rgb);
  } finally { program.dispose(); }
});

test("declared surface tone survives inferred text contrast across nested themes", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const tone = (dark, light) => ({ from: canvas, curve: [[0, dark], [0.25, dark], [1, light]] });
  const panel = c.role("panel", "#f2f2f2", { domain: "neutral" }).on(canvas, { tone: tone(0.23, 0.96) });
  const sheet = c.role("sheet", "#ffffff", { domain: "neutral" }).on(panel, { tone: tone(0.31, 1) });
  const strong = c.role("primary", "#171717", { domain: "neutral" });
  const weak = c.role("secondary", "#606060", { domain: "neutral" });
  const outputs = { panel, sheet };
  for (const [name, surface] of [["base", canvas], ["panel", panel], ["sheet", sheet]]) {
    const primary = strong.on(surface, { contrast: "wcag22-text" });
    const secondary = weak.on(surface, { contrast: "wcag22-text" });
    c.preserveTonalHierarchy([primary, secondary]);
    outputs[`${name}Primary`] = primary;
    outputs[`${name}Secondary`] = secondary;
  }
  // On the neutral axis, Oklab L is the cube root of linear intensity.
  const nearest = (lightness) => Array.from({ length: 256 }, (_, value) => value)
    .reduce((a, b) => Math.abs(Math.cbrt(luminance(b)) - lightness) < Math.abs(Math.cbrt(luminance(a)) - lightness) ? b : a);
  const program = await c.compile(outputs);
  try {
    const light = program.resolve({ canvas: "#ffffff" });
    assert.equal(light.state, "ready");
    assert.equal(light.values.panel.rgb[0], nearest(0.96));
    assert.equal(light.values.sheet.rgb[0], 255);
    const authoredRatio = contrast(light.values.basePrimary, 255) / contrast(light.values.baseSecondary, 255);
    for (const canvasGray of [0, 22]) {
      const frame = program.resolve({ canvas: [canvasGray, canvasGray, canvasGray] });
      assert.equal(frame.state, "ready");
      const { values } = frame;
      assert.equal(values.panel.rgb[0], nearest(0.23));
      assert.equal(values.sheet.rgb[0], nearest(0.31));
      for (const [name, background] of [["base", canvasGray], ["panel", values.panel.rgb[0]], ["sheet", values.sheet.rgb[0]]]) {
        const primary = values[`${name}Primary`], secondary = values[`${name}Secondary`];
        assert.ok(primary.rgb[0] > secondary.rgb[0] && secondary.rgb[0] > background);
        assert.ok(textPasses(primary.rgb, background) && textPasses(secondary.rgb, background));
        assert.ok(contrast(primary, background) / contrast(secondary, background) >= authoredRatio - 1e-12);
      }
      assert.notDeepEqual(values.panelSecondary.rgb, values.sheetSecondary.rgb);
    }
    assert.deepEqual(program.resolve({ canvas: "#ffffff" }).values, light.values);
  } finally { program.dispose(); }
});

test("nested linear surface tones choose the nearest readable colors across disconnected feasible ranges", async () => {
  const grays = Array.from({ length: 256 }, (_, value) => value);
  const cr = (a, b) => (luminance(Math.max(a, b)) + 0.05) / (luminance(Math.min(a, b)) + 0.05);
  const requiredRatio = cr(23, 255) / cr(96, 255);
  const feasible = grays.filter((background) => [false, true].some((lighter) => {
    const ratios = grays.filter((gray) => (lighter ? gray > background : gray < background)
      && cr(gray, background) >= 4.5).map((gray) => cr(gray, background));
    return ratios.length && Math.max(...ratios) / Math.min(...ratios) >= requiredRatio;
  }));
  const nearest = (tone) => feasible.reduce((a, b) =>
    Math.abs(Math.cbrt(luminance(b)) - tone) < Math.abs(Math.cbrt(luminance(a)) - tone) ? b : a);
  const frames = [0, 22, 217, 255, 22];
  const results = [];
  for (const reverse of [false, true]) {
    const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
    const tone = (dark, light) => ({ from: canvas, curve: [[0, dark], [1, light]] });
    const panel = c.role("panel", "#f2f2f2", { domain: "neutral" }).on(canvas, { tone: tone(0.23, 0.96) });
    const sheet = c.role("sheet", "#ffffff", { domain: "neutral" }).on(panel, { tone: tone(0.31, 1) });
    const strong = c.role("primary", "#171717", { domain: "neutral" });
    const weak = c.role("secondary", "#606060", { domain: "neutral" });
    const outputs = { panel, sheet };
    const surfaces = [["base", canvas], ["panel", panel], ["sheet", sheet]];
    if (reverse) surfaces.reverse();
    for (const [name, surface] of surfaces) {
      const primary = strong.on(surface, { contrast: "wcag22-text" });
      const secondary = weak.on(surface, { contrast: "wcag22-text" });
      c.preserveTonalHierarchy([primary, secondary]);
      outputs[`${name}Primary`] = primary;
      outputs[`${name}Secondary`] = secondary;
    }
    const program = await c.compile(outputs);
    try {
      for (const [index, background] of frames.entries()) {
        const frame = program.resolve({ canvas: [background, background, background] });
        assert.equal(frame.state, "ready", `canvas ${background}`);
        const { values } = frame;
        const t = Math.cbrt(luminance(background));
        assert.equal(values.panel.rgb[0], nearest(0.23 + t * (0.96 - 0.23)));
        assert.equal(values.sheet.rgb[0], nearest(0.31 + t * (1 - 0.31)));
        for (const [name, gray] of [["base", background], ["panel", values.panel.rgb[0]], ["sheet", values.sheet.rgb[0]]]) {
          const primary = values[`${name}Primary`], secondary = values[`${name}Secondary`];
          assert.ok(textPasses(primary.rgb, gray) && textPasses(secondary.rgb, gray));
          assert.ok((primary.rgb[0] - gray) * (secondary.rgb[0] - gray) > 0);
          assert.ok(contrast(primary, gray) / contrast(secondary, gray) >= requiredRatio - 1e-12);
        }
        const physical = Object.fromEntries(Object.entries(values).map(([name, value]) => [name, [value.rgb, value.opacity]]));
        if (reverse) assert.deepEqual(physical, results[index]);
        else results.push(physical);
      }
    } finally { program.dispose(); }
  }
  assert.deepEqual(results[1], results[4]);
});

test("one authored tonal relation survives a polarity change and returns without drift", async () => {
  const reference = { canvas: [255, 255, 255] };
  const c = colors({ reference }), canvas = c.surface("canvas");
  const heading = c.role("heading", "#444444").on(canvas, { contrast: "wcag22-text" });
  const description = c.role("description", "#666666").on(canvas, { contrast: "wcag22-text" });
  c.preserveTonalHierarchy([heading, description]);
  reference.canvas.fill(0);
  const program = await c.compile({ heading, description });
  try {
    const original = program.resolve({ canvas: "#ffffff" });
    assert.equal(original.state, "ready");
    assert.deepEqual(original.values.heading.rgb, [68, 68, 68]);
    assert.deepEqual(original.values.description.rgb, [102, 102, 102]);
    const authoredRatio = contrast(original.values.heading, 255) / contrast(original.values.description, 255);
    const black = program.resolve({ canvas: "#000000" });
    assert.equal(black.state, "ready");
    assert.ok(textPasses(black.values.heading.rgb, 0));
    assert.ok(textPasses(black.values.description.rgb, 0));
    assert.ok(contrast(black.values.heading, 0) / contrast(black.values.description, 0) >= authoredRatio - 1e-12);
    assert.ok(black.values.heading.rgb[0] - black.values.description.rgb[0] > 1,
      "a one-byte ordering must not replace the authored gap");
    const recovered = program.resolve({ canvas: "#ffffff" });
    assert.equal(recovered.state, "ready");
    assert.deepEqual(recovered.values, original.values);
  } finally { program.dispose(); }
});

test("reference appearance retains authored source and opacity before adaptation", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const heading = c.role("heading", "#222222").on(canvas, { contrast: "wcag22-text" });
  const description = c.role("description", "#444444").on(canvas, { contrast: "wcag22-text", opacity: 0.8 });
  c.preserveTonalHierarchy([heading, description]);
  const program = await c.compile({ heading, description });
  try {
    const white = program.resolve({ canvas: "#ffffff" });
    assert.equal(white.state, "ready");
    assert.deepEqual(white.values.heading.rgb, [34, 34, 34]);
    assert.deepEqual(white.values.description.rgb, [68, 68, 68]);
    const authoredRatio = contrast(white.values.heading, 255) / contrast(white.values.description, 255);
    const black = program.resolve({ canvas: "#000000" });
    assert.equal(black.state, "ready");
    assert.equal(black.values.description.opacity, 0.8);
    assert.ok(contrast(black.values.heading, 0) / contrast(black.values.description, 0) >= authoredRatio - 1e-12);
    const finalDescription = visibleGray(black.values.description, 0);
    assert.ok(textPasses([finalDescription, finalDescription, finalDescription], 0));
  } finally { program.dispose(); }
});

test("exporting one member still preserves its relation; disconnected groups stay outside the program", async () => {
  const c = colors({ reference: { canvas: "#ffffff", unused: "#ffffff" } });
  const canvas = c.surface("canvas"), unused = c.surface("unused");
  const strong = c.role("strong", "#444444").on(canvas, { contrast: "wcag22-text" });
  const weak = c.role("weak", "#666666").on(canvas, { contrast: "wcag22-text" });
  c.preserveTonalHierarchy([strong, weak]);
  // This impossible reference order must not invalidate an unrelated exported composition.
  c.preserveTonalHierarchy([c.fixed("unused weak", "#eeeeee").on(unused), c.fixed("unused strong", "#000000").on(unused)]);
  const combined = await c.compile({ strong, weak });
  const aliased = await c.compile({ renamed: strong });
  try {
    assert.deepEqual(aliased.inputs, ["canvas"]);
    const full = combined.resolve({ canvas: "#000000" });
    const only = aliased.resolve({ canvas: "#000000" });
    assert.equal(full.state, "ready");
    assert.equal(only.state, "ready");
    assert.deepEqual(only.values.renamed, full.values.strong);
  } finally { combined.dispose(); aliased.dispose(); }
});

test("fixed colors expose the conflicting relation instead of silently losing it", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const heading = c.fixed("heading", "#444444").on(canvas);
  const description = c.fixed("description", "#666666").on(canvas);
  c.preserveTonalHierarchy([heading, description]);
  const program = await c.compile({ heading, description });
  try {
    assert.equal(program.resolve({ canvas: "#ffffff" }).state, "ready");
    const black = program.resolve({ canvas: "#000000" });
    assert.equal(black.state, "conflict");
    assert.equal("values" in black, false);
    const failure = black.diagnostics.find((item) => item.kind === "relation");
    assert.equal(failure.requirement, "tonal-hierarchy");
    assert.equal(failure.stronger.use, heading);
    assert.equal(failure.weaker.use, description);
    assert.ok(failure.actualGap < failure.requiredGap);
    assert.ok(failure.actualRatio < failure.requiredRatio);
    assert.equal(failure.requiredGap, Math.log(failure.requiredRatio));
    assert.equal(program.resolve({ canvas: "#ffffff" }).state, "ready");
  } finally { program.dispose(); }
});

test("a neutral reference appearance does not erase the color of a translucent material", async () => {
  const c = colors({ reference: { canvas: "#007fff" } }), canvas = c.surface("canvas");
  const orange = c.role("orange material", "#ff8000").on(canvas, { opacity: 0.5 });
  const program = await c.compile({ orange });
  try {
    const reference = program.resolve({ canvas: "#007fff" });
    assert.equal(reference.state, "ready");
    assert.deepEqual(reference.values.orange.rgb, [255, 128, 0]);
    const visible = reference.values.orange.rgb.map((v, i) => Math.round((v + [0, 127, 255][i]) / 2));
    assert.deepEqual(visible, [128, 128, 128]);
    const white = program.resolve({ canvas: "#ffffff" });
    assert.equal(white.state, "ready");
    assert.equal(white.values.orange.opacity, 0.5);
    const [red, green, blue] = white.values.orange.rgb;
    assert.ok(red > green && green > blue, "the source remains orange despite a neutral reference appearance");
    assert.deepEqual(program.resolve({ canvas: "#007fff" }).values, reference.values);
  } finally { program.dispose(); }
});

test("zero opacity keeps the authored material while the visible surface changes", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const orange = c.role("invisible orange material", "#ff8000").on(canvas, { opacity: 0 });
  const program = await c.compile({ orange });
  try {
    for (const background of ["#ffffff", "#000000", "#007fff"]) {
      const result = program.resolve({ canvas: background });
      assert.equal(result.state, "ready");
      assert.deepEqual(result.values.orange.rgb, [255, 128, 0]);
      assert.equal(result.values.orange.opacity, 0);
    }
  } finally { program.dispose(); }
});

test("a single strong color preserves its reference strength without a named role or hierarchy", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const ink = c.role("arbitrary author name", "#101012").on(canvas, { contrast: "wcag22-text" });
  const program = await c.compile({ ink, alias: ink });
  try {
    const original = program.resolve({ canvas: "#ffffff" });
    assert.equal(original.state, "ready");
    assert.deepEqual(original.values.ink.rgb, [16, 16, 18]);
    assert.equal(original.reference.length, 1);
    const evidence = original.reference[0];
    assert.equal(evidence.use, ink);
    assert.equal(evidence.role, "arbitrary author name");
    assert.deepEqual(evidence.outputs, ["ink", "alias"]);
    assert.equal("targetId" in evidence, false);
    assert.equal("occurrenceId" in evidence, false);
    assert.equal(evidence.preference, "reference-strength");
    assert.deepEqual(evidence.cases[0].referenceVisible, [16, 16, 18]);
    assert.ok(evidence.cases[0].referenceContrast > 19);
    assert.ok(Object.isFrozen(evidence.cases[0].referenceVisible));
    const black = program.resolve({ canvas: "#000000" });
    assert.equal(black.state, "ready");
    assert.ok(black.reference[0].cases[0].actualContrast > 18.8,
      "meeting a 4.5 minimum must not erase a strong authored appearance");
    assert.deepEqual(evidence.cases[0].actualVisible, [16, 16, 18], "retained evidence survives the next update");
    assert.deepEqual(program.resolve({ canvas: "#ffffff" }).values, original.values);
  } finally { program.dispose(); }
});

test("unchanged readable fixed colors keep their hierarchy while their common backdrop moves", async () => {
  const c = colors({ reference: { canvas: "#ffffff" } }), canvas = c.surface("canvas");
  const heading = c.fixed("heading", "#101010").on(canvas, { contrast: "wcag22-text" });
  const description = c.fixed("description", "#404040").on(canvas, { contrast: "wcag22-text" });
  c.preserveTonalHierarchy([heading, description]);
  const program = await c.compile({ heading, description });
  try {
    for (let gray = 180; gray <= 255; gray++) {
      const frame = program.resolve({ canvas: [gray, gray, gray] });
      assert.equal(frame.state, "ready", `unchanged same-side hierarchy on gray ${gray}`);
      assert.deepEqual(frame.values.heading.rgb, [16, 16, 16]);
      assert.deepEqual(frame.values.description.rgb, [64, 64, 64]);
    }
  } finally { program.dispose(); }
});

test("colored material branches find a readable hierarchy without an explicit tone recipe", async () => {
  const c = colors({ reference: { canvas: "#f6f4f0" } }), canvas = c.surface("canvas");
  const blue = c.role("blue", "#2563eb").on(canvas, { opacity: 0.12 });
  const amber = c.role("amber", "#d97706").on(blue, { opacity: 0.10 });
  const heading = c.role("heading", "#122940").on(amber, { contrast: "wcag22-text" });
  const description = c.role("description", "#21364f").on(amber, { contrast: "wcag22-text" });
  c.preserveTonalHierarchy([heading, description]);
  const program = await c.compile({ blue, amber, heading, description });
  const composite = (paint, background) => paint.rgb.map((channel, i) =>
    Math.round(channel * paint.opacity + background[i] * (1 - paint.opacity)));
  const luminanceRgb = (rgb) => rgb.reduce((sum, channel, i) =>
    sum + luminance(channel) * [0.2126, 0.7152, 0.0722][i], 0);
  const ratio = (rgb, background) => {
    const foreground = luminanceRgb(rgb), backdrop = luminanceRgb(background);
    return (Math.max(foreground, backdrop) + 0.05) / (Math.min(foreground, backdrop) + 0.05);
  };
  try {
    const original = program.resolve({ canvas: "#f6f4f0" });
    assert.equal(original.state, "ready");
    assert.equal(original.optimality, "exact");
    const authoredBackdrop = composite(original.values.amber, composite(original.values.blue, [246, 244, 240]));
    const authoredRatio = ratio(original.values.heading.rgb, authoredBackdrop)
      / ratio(original.values.description.rgb, authoredBackdrop);
    const changed = program.resolve({ canvas: "#3f243f" });
    assert.equal(changed.state, "ready");
    const background = composite(changed.values.amber, composite(changed.values.blue, [63, 36, 63]));
    const headingRatio = ratio(changed.values.heading.rgb, background);
    const descriptionRatio = ratio(changed.values.description.rgb, background);
    assert.ok(headingRatio >= 4.5 && descriptionRatio >= 4.5);
    assert.ok(headingRatio / descriptionRatio >= authoredRatio);
    assert.equal(changed.values.blue.opacity, 0.12);
    assert.equal(changed.values.amber.opacity, 0.10);
    assert.ok(changed.reference.every((entry) => entry.preference === "reference-strength"));
    assert.deepEqual(program.resolve({ canvas: "#f6f4f0" }).values, original.values);
  } finally { program.dispose(); }
});

test("Alert reference hierarchy keeps feasible updates and exposes an impossible colored backdrop", async () => {
  // Exact roles/paints of Lab UI's Alert composition probe. Its text hierarchy
  // and identical 4.5 contrast requirements are retained without a tone recipe.
  const composite = (paint, background) => paint.rgb.map((value, i) =>
    Math.round(value * paint.opacity + background[i] * (1 - paint.opacity)));
  const y = (rgb) => rgb.reduce((sum, channel, i) => sum + luminance(channel) * [0.2126, 0.7152, 0.0722][i], 0);
  const ratio = (left, right) => (Math.max(y(left), y(right)) + 0.05) / (Math.min(y(left), y(right)) + 0.05);
  const blue = [40, 60, 100];
  for (const dark of [false, true]) {
    const reference = dark ? [28, 28, 30] : [255, 255, 255];
    const c = colors({ reference: { canvas: reference } }), canvas = c.surface("canvas");
    const heading = c.role("heading", dark ? [246, 248, 250] : [16, 16, 18]).on(canvas, { contrast: "wcag22-text" });
    const description = c.role("description", dark ? [176, 176, 185] : [60, 60, 67]).on(canvas, { opacity: 0.722, contrast: "wcag22-text" });
    const dangerIcon = c.role("dangerIcon", dark ? [255, 58, 58] : [255, 59, 48]).on(canvas, { contrast: "wcag22-graphic" });
    const close = c.role("close", dark ? [176, 176, 185] : [60, 60, 67]).on(canvas, { opacity: 0.722, contrast: "wcag22-ui" });
    const border = c.role("border", [120, 120, 128]).on(canvas, { opacity: dark ? 0.2 : 0.161 });
    const focus = c.role("focus", dark ? [74, 143, 255] : [0, 122, 255]).on(canvas, { contrast: "wcag22-ui" });
    c.preserveTonalHierarchy([heading, description]);
    const program = await c.compile({ heading, description, dangerIcon, close, border, focus });
    try {
      const original = program.resolve({ canvas: reference });
      assert.equal(original.state, "ready");
      const requiredRatio = ratio(original.values.heading.rgb, reference)
        / ratio(composite(original.values.description, reference), reference);
      const maximumBlueContrast = Math.max(ratio([255, 255, 255], blue), ratio([0, 0, 0], blue));
      assert.ok(4.5 * requiredRatio > maximumBlueContrast,
        "even the entire sRGB domain cannot satisfy this authored hierarchy and both contrast floors on blue");
      const feasible = program.resolve({ canvas: dark ? [255, 255, 255] : [0, 0, 0] });
      assert.equal(feasible.state, "ready");
      assert.equal(feasible.values.description.opacity, 0.722);
      const impossible = program.resolve({ canvas: blue });
      assert.ok(["unresolved", "conflict"].includes(impossible.state));
      assert.equal("values" in impossible, false);
      assert.deepEqual(program.resolve({ canvas: reference }).values, original.values);
    } finally { program.dispose(); }
  }
});
