import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

const packageRoot = resolve(process.argv[2] ?? dirname(fileURLToPath(new URL("../index.js", import.meta.url))));
const pattern = process.argv[3] ?? "repeated";
const nodes = Number(process.argv[4] ?? 8);
const samples = Number(process.argv[5] ?? 7);
const adaptive = ["v3-gray", "v3-limit"].includes(pattern);
assert.ok(["repeated", "distinct", "shared", "grouped", "ring", "v3-gray", "v3-limit"].includes(pattern), "pattern: repeated | distinct | shared | grouped | ring | v3-gray | v3-limit");
assert.ok(Number.isInteger(nodes) && nodes >= 1 && nodes <= 4096, "nodes: 1..4096");
assert.ok(pattern !== "ring" || (nodes >= 3 && nodes <= 2048), "ring nodes: 3..2048 (two hard constraints per node)");
assert.ok(!adaptive || nodes === 1, "V3 witnesses use exactly one computed target: nodes=1");
assert.ok(Number.isInteger(samples) && samples >= 3 && samples <= 100, "samples: 3..100");
const api = await import(pathToFileURL(resolve(packageRoot, "index.js")));
const abi = await import(pathToFileURL(resolve(packageRoot, adaptive ? "program-wire/abi-v3.js" : "program-wire/abi-v1.js")));
const started = performance.now();
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
await api.default({ module_or_path: wasm });
const initializationMs = performance.now() - started;
const producer = api.decodeCertificateEnvelope(api.issueSourceCertificateEnvelope()).producerRevision;

function color(index) {
  if (pattern === "repeated" || pattern === "shared" || pattern === "ring") return [20, 20, 20];
  if (pattern === "grouped") return [20 + index % 4, 20, 20];
  return [index % 128, Math.floor(index / 128) % 128, 20];
}

// Независимый WCAG oracle из assignment-runtime witness: ближайшая граница
// допустимого gray-интервала находится с одной стороны authored anchor.
function grayLuminance(channel) {
  const s = channel / 255;
  return s <= .04045 ? s / 12.92 : ((s + .055) / 1.055) ** 2.4;
}
function grayContrast(a, b) {
  const [low, high] = [grayLuminance(a), grayLuminance(b)].sort((x, y) => x - y);
  return (high + .05) / (low + .05);
}
function grayOracle(lower, upper, anchor, background) {
  const valid = Array.from({ length: upper - lower + 1 }, (_, index) => lower + index)
    .filter(channel => grayContrast(channel, background) >= 4.5);
  if (valid.includes(anchor)) return anchor;
  if (!valid.length) return null;
  assert.ok(valid.every(value => value < anchor) || valid.every(value => value > anchor));
  return valid.every(value => value < anchor) ? Math.max(...valid) : Math.min(...valid);
}
const grayBackgrounds = [255, 128, 0, 255];
const grayExpected = adaptive ? grayBackgrounds.map(background => grayOracle(0, 96, 24, background)) : [];
if (adaptive) {
  assert.equal(grayExpected[0], 24);
  assert.notEqual(grayExpected[1], 24);
  assert.notEqual(grayExpected[1], null);
  assert.equal(grayExpected[2], null);
  assert.equal(grayExpected[3], 24);
}

function adaptiveStep(index) {
  const phase = index % 4;
  const limited = pattern === "v3-limit";
  return {
    phase,
    revision: limited ? BigInt(3 * Math.floor(index / 4) + [1, 2, 2, 3][phase]) : BigInt(index + 1),
    background: limited ? [255, 0, 0, 255][phase] : grayBackgrounds[phase],
    expected: limited ? (phase === 0 || phase === 3 ? 24 : null) : grayExpected[phase],
  };
}
function adaptiveRead(snapshot, step) {
  assert.equal(snapshot.snapshotVersion, 2);
  assert.equal(snapshot.revision, step.revision);
  if (step.expected === null) {
    if (pattern === "v3-limit") {
      // Улучшенное доказательство пустоты законно; limit не выдаётся за proof.
      assert.ok(["unsatisfied", "incomplete"].includes(snapshot.state));
    } else assert.equal(snapshot.state, "unsatisfied");
    assert.equal(snapshot.reason, snapshot.state === "incomplete" ? "limit" : undefined);
    assert.equal(snapshot.outputCount(), 0);
    assert.equal(snapshot.assessmentReport(), null);
    for (const method of ["outputRgb", "outputSlot", "outputOpacity"]) {
      assert.throws(() => snapshot[method](0), error => api.isBindingError(error)
        && error.operation === "readProgramOutput" && error.code === "invalid_output_index");
    }
    return { state: snapshot.state, reason: snapshot.reason ?? null, revision: String(snapshot.revision), outputs: [] };
  }
  assert.equal(snapshot.state, "ready");
  assert.equal(snapshot.reason, undefined);
  assert.equal(snapshot.outputCount(), 1);
  assert.equal(snapshot.outputSlot(0), 91);
  const rgb = [...snapshot.outputRgb(0)];
  assert.deepEqual(rgb, [step.expected, step.expected, step.expected]);
  assert.equal(snapshot.outputOpacity(0), 1);
  assert.notEqual(snapshot.assessmentReport(), null);
  return { state: "ready", reason: null, revision: String(snapshot.revision), outputs: [{ slot: 91, rgb, opacity: 1 }] };
}
function adaptiveChecksum(checksum, reading) {
  const state = { ready: 1, unsatisfied: 2, incomplete: 3 }[reading.state];
  const values = [state, reading.reason === "limit" ? 1 : 0, Number(reading.revision), reading.outputs.length];
  for (const output of reading.outputs) values.push(output.slot, ...output.rgb, output.opacity);
  for (const value of values) checksum = Math.imul(checksum ^ value, 16777619) >>> 0;
  return checksum;
}

let builder;
if (adaptive) {
  const domain = pattern === "v3-limit"
    ? { kind: "box", lower: [0, 0, 0], upper: [255, 255, 255] }
    : { kind: "gray", lower: 0, upper: 96 };
  builder = new abi.ProgramWireBuilderV3().computedTarget(21, domain, [24, 24, 24], 1)
    .assignment([21]).surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
    .sourceOverOccurrence(61, 41, 51, 64, .2, abi.SURROUND_AVERAGE_V1)
    .presentationRoot(71, 61).presentationTarget(71, 61)
    .wcag22VisibleUnary(true, 81, 61, abi.WCAG22_SC143_TEXT_DEFAULT_V1).output(91, 41);
  if (pattern === "v3-limit") builder.source(11, [24, 24, 24]).fixedTarget(22, 11)
    .exactIntrinsicRelationHard(82, 22, [21]);
} else {
  builder = new abi.ProgramWireBuilderV1().surfaceInputPort(1).inputSurface(1, 1);
  if (pattern === "shared") builder.source(1, color(0));
  for (let index = 0; index < nodes; index += 1) {
    const id = index + 1;
    if (pattern !== "shared") builder.source(id, color(index));
    builder.fixedTarget(id, pattern === "shared" ? 1 : id).solidPaint(id, id)
      .sourceOverOccurrence(id, id, 1, 64, 0.2, abi.SURROUND_AVERAGE_V1)
      .presentationRoot(id, id).presentationTarget(id, id)
      .wcag22VisibleUnary(true, id, id, abi.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
      .output(id, id);
  }
  if (pattern === "ring") {
    // Equal colours keep the same output oracle. Directed relations connect all
    // target branches, so the residual component repeats each partition cell.
    for (let id = 1; id <= nodes; id += 1) {
      builder.exactIntrinsicRelationHard(nodes + id, id, [id % nodes + 1]);
    }
  }
}
const wire = builder.finish();
const results = [];
for (let sample = 0; sample <= samples; sample += 1) {
  const start = performance.now();
  const runtime = api.compileProgramWire(wire, 1);
  const compileMs = performance.now() - start;
  const updates = [];
  let checksum = 0;
  let stopped;
  try {
    for (let index = 0; index < 64; index += 1) {
      const refused = index % 4 === 2;
      const step = adaptive ? adaptiveStep(index) : null;
      const background = adaptive ? [step.background, step.background, step.background]
        : refused ? [0, 0, 0] : [255 - index % 2, 255, 255];
      const updateStart = performance.now();
      const snapshot = runtime.updateObserved(step?.revision ?? BigInt(index + 1), new Uint32Array([1]), new Uint8Array(background), 1);
      try {
        if (adaptive) {
          const reading = adaptiveRead(snapshot, step);
          if (pattern === "v3-limit" && step.phase === 1) stopped = reading;
          if (pattern === "v3-limit" && step.phase === 2) assert.deepEqual(reading, stopped, "same-revision replay changed its outcome");
          checksum = adaptiveChecksum(checksum, reading);
        } else {
          assert.equal(snapshot.state, refused ? "failed" : "ready");
          if (!refused) {
            assert.equal(snapshot.outputCount(), nodes);
            for (let output = 0; output < nodes; output += 1) {
              const slot = snapshot.outputSlot(output);
              assert.equal(slot, output + 1);
              const rgb = snapshot.outputRgb(output);
              assert.deepEqual([...rgb], color(slot - 1));
              assert.equal(snapshot.outputOpacity(output), 1);
              checksum = (checksum + slot + rgb[0]) >>> 0;
            }
          }
        }
      } finally {
        snapshot.free();
      }
      updates.push(performance.now() - updateStart);
    }
  } finally {
    runtime.free();
  }
  results.push({ compileMs, cycleMs: performance.now() - start, updatesMs: updates, checksum });
}
assert.ok(results.every((result) => result.checksum === results[0].checksum));
const cold = results.shift();
console.log(JSON.stringify({
  node: process.version, v8: process.versions.v8, producer, pattern, nodes, samples,
  wasmBytes: wasm.length, wasmSha256: createHash("sha256").update(wasm).digest("hex"),
  initializationMs, cold, samplesAfterWarmup: results, maxRssKiB: process.resourceUsage().maxRSS,
}));
