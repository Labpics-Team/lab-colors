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
assert.ok(["repeated", "distinct", "shared", "grouped"].includes(pattern), "pattern: repeated | distinct | shared | grouped");
assert.ok(Number.isInteger(nodes) && nodes >= 1 && nodes <= 4096, "nodes: 1..4096");
assert.ok(Number.isInteger(samples) && samples >= 3 && samples <= 100, "samples: 3..100");
const api = await import(pathToFileURL(resolve(packageRoot, "index.js")));
const abi = await import(pathToFileURL(resolve(packageRoot, "program-wire/abi-v1.js")));
const started = performance.now();
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
await api.default({ module_or_path: wasm });
const initializationMs = performance.now() - started;
const producer = api.decodeCertificateEnvelope(api.issueSourceCertificateEnvelope()).producerRevision;

function color(index) {
  if (pattern === "repeated" || pattern === "shared") return [20, 20, 20];
  if (pattern === "grouped") return [20 + index % 4, 20, 20];
  return [index % 128, Math.floor(index / 128) % 128, 20];
}

const builder = new abi.ProgramWireBuilderV1().surfaceInputPort(1).inputSurface(1, 1);
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
const wire = builder.finish();
const results = [];
for (let sample = 0; sample <= samples; sample += 1) {
  const start = performance.now();
  const runtime = api.compileProgramWire(wire, 1);
  const compileMs = performance.now() - start;
  const updates = [];
  let checksum = 0;
  try {
    for (let index = 0; index < 64; index += 1) {
      const refused = index % 4 === 2;
      const background = refused ? [0, 0, 0] : [255 - index % 2, 255, 255];
      const updateStart = performance.now();
      const snapshot = runtime.updateObserved(BigInt(index + 1), new Uint32Array([1]), new Uint8Array(background), 1);
      try {
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
