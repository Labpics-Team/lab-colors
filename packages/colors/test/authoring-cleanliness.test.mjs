import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

async function point(source, { fixed = false, opacity = 1, profile = true, maxChange } = {}) {
  const c = colors(profile ? { cleanliness: "nominal-srgb8-v2" } : undefined);
  const canvas = c.surface("canvas");
  const role = fixed ? c.fixed("material", source) : c.role("material", source, { maxChange });
  return { role, program: await c.compile({ material: role.on(canvas, { opacity }) }) };
}

test("ordinary readiness does not silently claim that Cleanliness was assessed", async () => {
  const { program } = await point("#283548", { profile: false });
  try {
    const frame = program.resolve({ canvas: "#f4eadb" });
    assert.equal(frame.state, "ready");
    assert.deepEqual(frame.values.material.rgb, [40, 53, 72], "no unrequested convention may erase dark tinted intent");
    assert.deepEqual(frame.cleanliness, { status: "not-assessed", reason: "no-declared-profile" });
    assert.equal(Object.isFrozen(frame.cleanliness), true);
  } finally { program.dispose(); }
});

test("the declared profile searches for a nearby conforming color and retains the authored anchor", async () => {
  // Adjacent accepted/rejected points belong to the published convention's
  // existing independent boundary vectors; this is not a perceptual threshold.
  const { role, program } = await point([0, 199, 64], { maxChange: 0.01 });
  try {
    const frame = program.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "ready");
    assert.notDeepEqual(frame.values.material.rgb, [0, 199, 64]);
    assert.ok(frame.values.material.rgb[1] > 180, "the green must not escape into an unrelated neutral");
    assert.equal(frame.values.material.opacity, 1);
    assert.deepEqual(role.anchor, [0, 199, 64]);
    assert.deepEqual(frame.cleanliness, {
      status: "conforms", profile: "nominal-srgb8-v2", basis: "declared-convention",
      scope: "declared-final-points", accepted: 1, rejected: 0, unavailable: 0,
    });
    for (let i = 0; i < 3; i += 1) {
      const repeated = program.resolve({ canvas: "#ffffff" });
      assert.equal(repeated.state, "ready");
      assert.deepEqual(repeated.values, frame.values, "repeated correction must not accumulate drift");
      assert.deepEqual(repeated.cleanliness, frame.cleanliness);
    }
  } finally { program.dispose(); }
});

test("a conforming authored color is retained by the same automatic search", async () => {
  const { program } = await point([0, 199, 63]);
  try {
    const frame = program.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "ready");
    assert.deepEqual(frame.values.material.rgb, [0, 199, 63]);
    assert.equal(frame.cleanliness.status, "conforms");
  } finally { program.dispose(); }
});

test("joint point search repairs a nominal color and identifies an incompatible maxChange", async () => {
  const { program } = await point([40, 70, 190]);
  try {
    const frame = program.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "ready");
    assert.equal(frame.selection[0].mode, "joint-point-v1");
    assert.equal(frame.optimality, "unfinished");
    assert.deepEqual(frame.values.material.rgb, [46, 79, 200]);
    assert.equal(frame.cleanliness.status, "conforms");
  } finally { program.dispose(); }

  const { program: constrained } = await point("#283548", { maxChange: 0.08 });
  try {
    const frame = constrained.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "conflict");
    assert.equal("values" in frame, false);
  } finally { constrained.dispose(); }
});

test("an explicit reference retains its own selection policy during nominal repair", async () => {
  const c = colors({ cleanliness: "nominal-srgb8-v2", reference: { canvas: "#fff7ef" } });
  const canvas = c.surface("canvas");
  const ink = c.role("ink", [200, 160, 50], { maxChange: 0.08 }).on(canvas);
  const program = await c.compile({ ink });
  try {
    const frame = program.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "ready");
    assert.equal(frame.selection[0].mode, "ordered-reference-v1");
    assert.equal(frame.optimality, "exact");
    assert.deepEqual(frame.values.ink.rgb, [211, 167, 61]);
    assert.equal(frame.cleanliness.status, "conforms");
  } finally { program.dispose(); }
});

test("conformance does not force a color to differ from its backdrop", async () => {
  for (const source of ["#ffffff", "#0067f4"]) {
    for (const opacity of [1, 0.5, 0]) {
      for (const fixed of [false, true]) {
        const { program, role } = await point(source, { opacity, fixed });
        try {
          const frame = program.resolve({ canvas: source });
          assert.equal(frame.state, "ready", `${source} alpha=${opacity} fixed=${fixed}`);
          assert.deepEqual(frame.values.material.rgb, role.anchor, "an unchanged valid appearance needs no artificial repair");
          assert.equal(frame.values.material.opacity, opacity);
          assert.equal(frame.cleanliness.status, "conforms");
        } finally { program.dispose(); }
      }
    }
  }
});

test("fixed intent and cleanliness remain separate hard requirements", async () => {
  const { role, program } = await point([0, 199, 64], { fixed: true });
  try {
    const frame = program.resolve({ canvas: "#ffffff" });
    assert.equal(frame.state, "conflict");
    assert.equal("values" in frame, false);
    assert.ok(frame.diagnostics.some((item) => item.kind === "requirement" && item.requirement === "cleanliness"));
    assert.deepEqual(role.anchor, [0, 199, 64]);
  } finally { program.dispose(); }
});

test("the profile evaluates the final transparent material rather than its source", async () => {
  const { program } = await point([128, 128, 129], { fixed: true, opacity: 0.5 });
  try {
    const accepted = program.resolve({ canvas: [128, 128, 127] });
    assert.equal(accepted.state, "ready");
    assert.deepEqual(accepted.values.material.rgb, [128, 128, 129]);
    assert.equal(accepted.values.material.opacity, 0.5);
    assert.equal(accepted.cleanliness.status, "conforms");
  } finally { program.dispose(); }
  const { program: rejected } = await point("#ffffff", { fixed: true, opacity: 0.5 });
  try {
    const frame = rejected.resolve({ canvas: [1, 1, 3] });
    assert.equal(frame.state, "conflict", "accepted source becomes a rejected final color after composition");
    assert.ok(frame.diagnostics.some((item) => item.requirement === "cleanliness"));
  } finally { rejected.dispose(); }
});

test("reachable nested uses share one profile without counting output aliases twice", async () => {
  const c = colors({ cleanliness: "nominal-srgb8-v2" });
  const canvas = c.surface("canvas");
  const paper = c.fixed("paper", "#f4eadb").on(canvas);
  const accent = c.role("accent", "#0067f4").on(paper, { contrast: "wcag22-text" });
  const program = await c.compile({ accent, alias: accent });
  try {
    const frame = program.resolve({ canvas: "#283548" });
    assert.equal(frame.state, "ready");
    assert.equal(frame.cleanliness.accepted, 2, "both final uses are assessed, even when the backdrop is not exported");
    assert.equal(frame.values.accent, frame.values.alias);
    assert.equal(frame.cleanliness.basis, "declared-convention");
  } finally { program.dispose(); }
});
