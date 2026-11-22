import assert from "node:assert/strict";
import test from "node:test";
import { ColorAuthoringError, colors } from "../authoring.js";

test("cleanliness profiles are admitted explicitly without accepting invented ratings", () => {
  assert.doesNotThrow(() => colors({ cleanliness: "nominal-srgb8-v2" }));
  for (const cleanliness of [null, true, false, 0.8, "auto", "whole-field", {}, []]) {
    assert.throws(() => colors({ cleanliness }),
      (error) => error instanceof ColorAuthoringError && error.path === "colors.options.cleanliness");
  }
  assert.throws(() => colors({ qualityScore: 1 }), ColorAuthoringError);
  const hostile = { get cleanliness() { throw new Error("caller getter"); } };
  assert.throws(() => colors(hostile), (error) => error.code === "color_invalid_definition");
});

test("one immutable role creates local uses and can share a use explicitly", () => {
  const c = colors();
  const canvas = c.surface("canvas"), island = c.surface("island");
  const source = [0, 103, 244];
  const accent = c.role("акцент.ссылка", source);
  source[0] = 255;
  const link = accent.on(canvas), otherLink = accent.on(island);
  assert.deepEqual(accent.anchor, [0, 103, 244]);
  assert.equal(link.role, accent);
  assert.equal(otherLink.role, accent);
  assert.notEqual(link, otherLink);
  assert.notEqual(accent.on(canvas), link);
  const label = c.role("label", "#ffffff", { domain: "neutral" });
  assert.equal(label.on(link).against, link);
  assert.equal(label.on(link).against, link);
  assert.equal(Object.isFrozen(accent), true);
  assert.equal(Object.isFrozen(accent.anchor), true);
  assert.equal("adaptive" in accent, false);
});

test("references from another declaration and fabricated references are rejected", () => {
  const a = colors(), b = colors();
  const surface = a.surface("surface"), role = a.role("role", "#0067f4");
  for (const foreign of [b.surface("surface"), b.role("role", "#0067f4").on(b.surface("other")), { ...surface }, null]) {
    assert.throws(() => role.on(foreign), (error) => error instanceof ColorAuthoringError && error.code === "color_foreign_reference");
  }
  assert.throws(() => role.on(surface, { tone: { from: b.surface("tone"), curve: [[0, 0.1], [1, 0.9]] } }),
    (error) => error.code === "color_foreign_reference");
});

test("an explicitly fixed source rejects an adaptation preference", () => {
  const c = colors(), canvas = c.surface("canvas");
  const fixed = c.fixed("print", "#f1ede7");
  assert.doesNotThrow(() => fixed.on(canvas, { opacity: 0.8 }));
  assert.throws(() => fixed.on(canvas, { tone: { from: canvas, curve: [[0, 0.1], [1, 0.9]] } }),
    (error) => error.code === "color_invalid_definition" && error.path === "role.on.policy.tone");
});

test("malformed physical policies fail before compilation", () => {
  const c = colors(), canvas = c.surface("canvas"), role = c.role("role", "#c45f35");
  const curves = [[], [[0, 0.1]], [[0.1, 0.1], [1, 0.9]], [[0, 0.1], [0, 0.8], [1, 0.9]], [[0, 0.1], [1, Number.NaN]]];
  for (const curve of curves) {
    assert.throws(() => role.on(canvas, { tone: { from: canvas, curve } }), ColorAuthoringError);
  }
  for (const opacity of [-0.1, 1.1, Infinity, "0.8"]) {
    assert.throws(() => role.on(canvas, { opacity }), ColorAuthoringError);
  }
  assert.throws(() => role.on(canvas, { contrast: "automatic-human-readability" }), ColorAuthoringError);
  assert.throws(() => c.role("unsafe", "#112233", { identity: { maxHueDeviation: 181 } }), ColorAuthoringError);
  assert.throws(() => c.role("unsafe", "#112233", { identity: { chroma: [0, 1.1] } }), ColorAuthoringError);
  assert.throws(() => c.role("unsafe", "#112233", { maxChange: Number.MAX_VALUE }), ColorAuthoringError);
  assert.throws(() => c.role("unsafe", "#112233", { saturation: 1 }), ColorAuthoringError);
  assert.doesNotThrow(() => c.role("unsafe", "#112233"));
  assert.doesNotThrow(() => c.role("neutral", "#333333", { domain: "neutral", identity: { lightness: [0.1, 0.8] } }));
});

test("a nested declaration cannot be overwritten by a getter during admission", () => {
  const c = colors();
  let nested;
  const source = [0, 103, 244];
  Object.defineProperty(source, 0, { get() { nested = c.role("shared", "#5a9bff"); return 0; } });
  assert.throws(() => c.role("shared", source), (error) => error.code === "color_invalid_definition");
  assert.deepEqual(nested.anchor, [90, 155, 255]);
  assert.throws(() => c.role("shared", "#0067f4"), ColorAuthoringError);
});

test("output map accepts only reachable local uses", async () => {
  const c = colors(), surface = c.surface("surface"), role = c.role("role", "#0067f4");
  await assert.rejects(c.compile({ color: role }), (error) => error.code === "color_foreign_reference");
  await assert.rejects(c.compile({ color: surface }), (error) => error.code === "color_foreign_reference");
  await assert.rejects(c.compile({}), (error) => error.code === "color_invalid_definition");
  const other = colors();
  await assert.rejects(c.compile({ color: other.role("role", "#0067f4").on(other.surface("surface")) }),
    (error) => error.code === "color_foreign_reference");
});
