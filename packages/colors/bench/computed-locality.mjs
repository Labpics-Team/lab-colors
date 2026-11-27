import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";
import { measurementProtocol, splitMeasurements } from "./measurement-protocol.mjs";

const packageRoot = process.argv[2] === undefined
  ? fileURLToPath(new URL("../", import.meta.url))
  : resolve(process.argv[2]);
const components = Number(process.argv[3] ?? 8);
const protocol = measurementProtocol({ samples: process.argv[4], warmups: process.argv[5], frames: process.argv[6] });
const pattern = process.argv[7] ?? "independent";
assert.ok(Number.isInteger(components) && components >= 1 && components <= 256, "components: 1..256");
assert.ok(["independent", "shared-input"].includes(pattern), "pattern: independent | shared-input");
const inputCount = pattern === "independent" ? components : 1;
const inputs = Array.from({ length: inputCount }, (_, index) => `canvas${index}`);
const phases = ["localChange", "unchanged", "restore", "unchanged"];
const white = [255, 255, 255], black = [0, 0, 0];
const anchors = [68, 102];
// A fixed neutral-domain witness makes both package versions do identical work.
// The independent contrast and hierarchy checks below also reject degraded colors.
const expectedOnBlack = [177, 134];
const luminance = (gray) => gray <= 10.31475 ? gray / (255 * 12.92) : ((gray / 255 + 0.055) / 1.055) ** 2.4;
const contrast = (foreground, background) => {
  const a = luminance(foreground), b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
};
const requiredHierarchy = contrast(anchors[0], 255) / contrast(anchors[1], 255);

const loadStarted = performance.now();
const api = await import(pathToFileURL(resolve(packageRoot, "index.js")));
const importMs = performance.now() - loadStarted;
const initializationStarted = performance.now();
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
await api.default({ module_or_path: wasm });
const initializationMs = performance.now() - initializationStarted;
let producer = null, producerIdentityError = null;
try {
  producer = api.decodeCertificateEnvelope(api.issueSourceCertificateEnvelope()).producerRevision;
} catch (error) {
  // A dirty development build has no release producer identity. Keep this fact
  // explicit; the measured artifact is still bound below by its actual SHA-256.
  if (error.code !== "certificate_producer_identity_unavailable") throw error;
  producerIdentityError = error.code;
}

async function compile() {
  const reference = Object.fromEntries(inputs.map((name) => [name, white]));
  const c = api.colors({ reference });
  const surfaces = inputs.map((name) => c.surface(name));
  const heading = c.role("heading", [68, 68, 68], { domain: "neutral" });
  const description = c.role("description", [102, 102, 102], { domain: "neutral" });
  const outputs = {};
  for (let index = 0; index < components; index += 1) {
    const surface = surfaces[pattern === "independent" ? index : 0];
    const strong = heading.on(surface, { contrast: "wcag22-text" });
    const weak = description.on(surface, { contrast: "wcag22-text" });
    c.preserveTonalHierarchy([strong, weak]);
    outputs[`heading${index}`] = strong;
    outputs[`description${index}`] = weak;
  }
  return c.compile(outputs);
}

function check(frame, context, revision, digest) {
  assert.equal(frame.state, "ready");
  // Only the authored reference proves the continuous symmetric minimum.
  // Every adapted frame still has to match the finite witness below.
  const authored = inputs.every((name) => context[name][0] === 255);
  assert.equal(frame.optimality, authored ? "exact" : "unfinished");
  assert.equal(frame.revision, BigInt(revision));
  assert.deepEqual(frame.context, context);
  assert.equal(Object.keys(frame.values).length, components * 2);
  assert.equal(frame.reference.length, components * 2);
  assert.equal(frame.selection.length, components);
  assert.ok(frame.selection.every((part) => part.mode === "joint-reference-peers-v1"));
  assert.deepEqual(frame.cleanliness, { status: "not-assessed", reason: "no-declared-profile" });
  const physical = [];
  for (let index = 0; index < components; index += 1) {
    const background = context[inputs[pattern === "independent" ? index : 0]][0];
    const expected = background === 255 ? anchors : expectedOnBlack;
    const values = [frame.values[`heading${index}`], frame.values[`description${index}`]];
    for (let role = 0; role < values.length; role += 1) {
      const value = values[role];
      assert.deepEqual(value.rgb, [expected[role], expected[role], expected[role]]);
      assert.equal(value.opacity, 1);
      assert.equal(value.css, `rgb(${expected[role]} ${expected[role]} ${expected[role]} / 1)`);
      assert.ok(contrast(value.rgb[0], background) >= 4.5);
      physical.push(...value.rgb, value.opacity);
    }
    assert.ok(contrast(values[0].rgb[0], background) / contrast(values[1].rgb[0], background) >= requiredHierarchy);
  }
  for (const selection of frame.reference) {
    assert.equal(selection.preference, "reference-strength");
    assert.equal(selection.capacity, null);
    assert.equal(selection.outputs.length, 1);
    assert.equal(selection.cases.length, 1);
    const name = selection.outputs[0];
    const index = Number(name.match(/\d+$/u)[0]);
    const role = name.startsWith("heading") ? 0 : 1;
    const observed = context[inputs[pattern === "independent" ? index : 0]];
    const evidence = selection.cases[0];
    assert.deepEqual(evidence.referenceBackdrop, white);
    assert.deepEqual(evidence.referenceVisible, [anchors[role], anchors[role], anchors[role]]);
    assert.deepEqual(evidence.actualBackdrop, observed);
    assert.deepEqual(evidence.actualVisible, frame.values[name].rgb);
  }
  digest.update(JSON.stringify([frame.state, frame.optimality, physical]));
}

function changeContext(context, index) {
  const input = inputs[Math.floor(index / 4) % inputCount];
  context[input] = index % 4 < 2 ? white : black;
}

async function measure() {
  const start = performance.now();
  const program = await compile();
  const compileMs = performance.now() - start;
  const context = Object.fromEntries(inputs.map((name) => [name, black]));
  const digest = createHash("sha256");
  const updatesMs = [], byPhaseMs = { localChange: [], unchanged: [], restore: [] };
  let firstResolveMs, firstUsefulMs;
  try {
    const resolveStart = performance.now();
    const initial = program.resolve(context);
    check(initial, context, 1, digest);
    firstResolveMs = performance.now() - resolveStart;
    firstUsefulMs = performance.now() - start;
    for (let index = 0; index < protocol.frames; index += 1) {
      const updateStart = performance.now();
      changeContext(context, index);
      const frame = program.resolve(context);
      check(frame, context, index + 2, digest);
      const elapsed = performance.now() - updateStart;
      updatesMs.push(elapsed);
      byPhaseMs[phases[index % 4]].push(elapsed);
    }
    // The first useful frame remains valid even after later authored resolutions.
    assert.deepEqual(initial.values.heading0.rgb, [177, 177, 177]);
    assert.deepEqual(initial.context.canvas0, black);
  } finally {
    program.dispose();
  }
  return { compileMs, firstResolveMs, firstUsefulMs, updatesMs, byPhaseMs,
    cycleMs: performance.now() - start, checksum: digest.digest("hex") };
}

const results = [];
for (let sample = 0; sample < protocol.warmups + protocol.samples; sample += 1) results.push(await measure());

// An optional untimed probe separates live state from garbage awaiting V8 GC.
// RSS/external include the JS engine and WASM allocator capacity, not just objects.
async function retainedMemory() {
  if (typeof globalThis.gc !== "function") return null;
  globalThis.gc();
  const before = process.memoryUsage();
  let program = await compile(), frame;
  const context = Object.fromEntries(inputs.map((name) => [name, black]));
  const digest = createHash("sha256");
  try {
    frame = program.resolve(context);
    check(frame, context, 1, digest);
    for (let index = 0; index < protocol.frames; index += 1) {
      changeContext(context, index);
      frame = program.resolve(context);
      check(frame, context, index + 2, digest);
    }
    assert.equal(digest.digest("hex"), results[0].checksum);
    globalThis.gc();
    const withProgramAndFrame = process.memoryUsage();
    assert.equal(frame.state, "ready");
    program.dispose();
    program = null;
    frame = null;
    globalThis.gc();
    return { before, withProgramAndFrame, afterDispose: process.memoryUsage() };
  } finally {
    program?.dispose();
  }
}

const retainedMemoryBytes = await retainedMemory();
console.log(JSON.stringify({
  node: process.version, v8: process.versions.v8, producer, producerIdentityError, pattern, components, inputCount,
  outputs: components * 2, ...protocol, importMs, initializationMs,
  wasmBytes: wasm.length, wasmSha256: createHash("sha256").update(wasm).digest("hex"),
  ...splitMeasurements(results, protocol), retainedMemoryBytes,
  memoryAtEndBytes: process.memoryUsage(), maxRssKiB: process.resourceUsage().maxRSS,
}));
