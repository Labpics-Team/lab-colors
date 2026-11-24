import assert from "node:assert/strict";
import test from "node:test";
import { copyColorFrame, parseColorContext } from "../authoring-frame.js";

const node = Object.freeze({ role: { id: "ink" }, handle: Object.freeze({}) });
const graph = {
  inputs: [{ name: "canvas" }, { name: "paper" }], outputs: [{ name: "ink", slot: 1 }],
  targets: new Map([[1, node]]), occurrences: new Map([[2, node]]),
  constraints: new Map([[7, { node, requirement: "contrast" }]]),
  outputNames: new Map([[node, Object.freeze(["ink"])]])
};
const descriptor = () => ({ kind: "affine-srgb8", from: [0, 0, 0], to: [1, 0, 0] });
const assessment = () => ({ inputIndex: 0, from: [0, 0, 0], to: [1, 0, 0],
  sourceModel: "opaque-affine-encoded-srgb8-half-up-v1", domain: "whole-field", rendering: "unknown",
  regionCount: 5, witnesses: [
    { coordinate: [0, 1], nominal: [0, 0, 0] },
    { coordinate: [1, 2], nominal: [1, 0, 0] },
  ] });
const snapshot = (state = "ready", report = assessment()) => ({
  state, revision: 5n, optimality: "exact", fieldAssessment: () => report,
  cleanlinessAssessment: () => Object.freeze({ status: "not-assessed", reason: "no-declared-profile" }),
  referenceSelections: () => [], copyOutputTable: () => new Float64Array([1, 255, 255, 255, 1]),
  diagnostics: () => ({ searchVisited: 3n, searchBudget: 3n, witness: { violations: [
    { kind: "hard-constraint", constraintId: 7, caseIndex: 1 },
  ] } }),
});

test("affine admission copies one descriptor and constants without expanding user samples", () => {
  const field = descriptor();
  const context = parseColorContext(graph, { canvas: field, paper: "#ffffff" });
  field.from[0] = 99;
  field.to[0] = 255;
  field.kind = "changed";
  assert.deepEqual(context.field, { inputIndex: 0, name: "canvas", descriptor: descriptor() });
  assert.deepEqual([...context.rgb], [0, 0, 0, 255, 255, 255]);
  assert.equal(context.rgb.length, graph.inputs.length * 3);
  assert.equal(Object.isFrozen(context.field.descriptor), true);
  assert.equal(Object.isFrozen(context.field.descriptor.from), true);
  assert.equal(Object.isFrozen(context.field.descriptor.to), true);
  assert.deepEqual(context.unknown, []);
});

test("affine descriptors reject unsupported and malformed declarations with typed context errors", () => {
  for (const value of [
    {}, { kind: "affine-oklab", from: "#000000", to: "#ffffff" },
    { ...descriptor(), samples: [[0, 0, 0]] }, { ...descriptor(), opacity: 0.5 },
    { ...descriptor(), from: "#00000000" }, { ...descriptor(), to: [256, 0, 0] },
    Object.create({ kind: "affine-srgb8", from: "#000000", to: "#ffffff" }),
    { ...descriptor(), get from() { throw new Error("caller getter"); } },
  ]) {
    assert.throws(() => parseColorContext(graph, { canvas: value, paper: "#ffffff" }),
      { code: "color_invalid_context" });
  }
  assert.throws(() => parseColorContext(graph, { canvas: descriptor(), paper: descriptor() }),
    { code: "color_unsupported_context", path: "context.paper" });
});

test("nominal readiness becomes modeled with copied descriptor and explicit sRGB preview CSS", () => {
  const context = parseColorContext(graph, { canvas: descriptor(), paper: "#ffffff" });
  const report = assessment();
  const frame = copyColorFrame(snapshot("ready", report), graph, context);
  assert.equal(frame.state, "modeled");
  assert.equal(frame.scope, "nominal-model");
  assert.deepEqual(frame.coverage, { model: "complete", rendering: "unknown",
    sourceModel: "opaque-affine-encoded-srgb8-half-up-v1" });
  assert.deepEqual(frame.context.canvas, descriptor());
  assert.equal(frame.contextCss.canvas, "linear-gradient(90deg in srgb, rgb(0 0 0 / 1) 0%, rgb(1 0 0 / 1) 100%)");
  assert.equal(frame.contextCss.paper, "rgb(255 255 255 / 1)");
  assert.equal(frame.values.ink.css, "rgb(255 255 255 / 1)");
  assert.equal(frame.field.input, "canvas");
  assert.equal(frame.field.regionCount, 5);
  assert.deepEqual(frame.field.witnesses, report.witnesses);
  report.witnesses[1].coordinate[0] = 99;
  report.witnesses[1].nominal[0] = 99;
  assert.deepEqual(frame.field.witnesses[1], { coordinate: [1, 2], nominal: [1, 0, 0] });
  for (const value of [frame, frame.coverage, frame.context, frame.contextCss, frame.field,
    frame.field.witnesses, frame.field.witnesses[0], frame.field.witnesses[0].coordinate]) {
    assert.equal(Object.isFrozen(value), true);
  }
});

test("field conflict and budget limit keep nominal scope and exact Core witness coordinates", () => {
  const context = parseColorContext(graph, { canvas: descriptor(), paper: "#ffffff" });
  for (const [state, expected, model] of [["unsatisfied", "conflict", "violated"], ["incomplete", "unresolved", "incomplete"]]) {
    const frame = copyColorFrame(snapshot(state), graph, context);
    assert.equal(frame.state, expected);
    assert.equal(frame.scope, "nominal-model");
    assert.equal(frame.coverage.model, model);
    assert.equal(frame.coverage.rendering, "unknown");
    assert.equal("values" in frame, false);
    assert.deepEqual(frame.context.canvas, descriptor());
    assert.deepEqual(frame.diagnostics.find((entry) => entry.kind === "requirement").fieldWitness,
      { input: "canvas", coordinate: [1, 2], nominal: [1, 0, 0] });
  }
});

test("missing constants preserve the declared source without claiming model coverage", () => {
  const context = parseColorContext(graph, { canvas: descriptor() });
  for (const state of ["waiting", "stale"]) {
    const frame = copyColorFrame({ ...snapshot(state), fieldAssessment() { throw new Error("unavailable model"); } }, graph, context);
    assert.deepEqual(frame.context.canvas, descriptor());
    assert.equal("coverage" in frame, false);
    assert.equal("field" in frame, false);
    assert.ok(frame.diagnostics.some((entry) => entry.kind === "unknown-input" && entry.input === "paper"));
  }
});

test("field results cannot borrow metadata for another source or revision", () => {
  const context = parseColorContext(graph, { canvas: descriptor(), paper: "#ffffff" });
  for (const report of [null, { ...assessment(), inputIndex: 1 },
    { ...assessment(), from: [1, 0, 0], to: [0, 0, 0] },
    { ...assessment(), rendering: "guaranteed" }, { ...assessment(), sourceModel: "other-model" }]) {
    assert.throws(() => copyColorFrame(snapshot("ready", report), graph, context),
      { code: "color_core_contract", path: "resolution.field" });
  }
});

test("ordinary solid results retain the public ready shape", () => {
  const context = parseColorContext(graph, { canvas: "#000000", paper: "#ffffff" });
  const frame = copyColorFrame({ ...snapshot(), fieldAssessment() { throw new Error("not a field"); } }, graph, context);
  assert.equal(frame.state, "ready");
  assert.deepEqual(frame.context, { canvas: [0, 0, 0], paper: [255, 255, 255] });
  for (const key of ["scope", "coverage", "field", "contextCss"]) assert.equal(key in frame, false);
});
