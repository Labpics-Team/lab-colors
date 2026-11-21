import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";
import { measurementProtocol, splitMeasurements } from "./measurement-protocol.mjs";

const packageRoot = resolve(process.argv[2] ?? dirname(fileURLToPath(new URL("../index.js", import.meta.url))));
const pattern = process.argv[3] ?? "repeated";
const nodes = Number(process.argv[4] ?? 8);
const protocol = measurementProtocol({ samples: process.argv[5], warmups: process.argv[7], frames: process.argv[8] });
const { samples, warmups, frames } = protocol;
const requestedReadout = process.argv[6] ?? "auto";
assert.ok(["repeated", "distinct", "shared", "grouped", "ring", "sparse", "complete", "bipartite"].includes(pattern), "pattern: repeated | distinct | shared | grouped | ring | sparse | complete | bipartite");
assert.ok(Number.isInteger(nodes) && nodes >= 1 && nodes <= 4096, "nodes: 1..4096");
assert.ok(pattern !== "ring" || (nodes >= 3 && nodes <= 2048), "ring nodes: 3..2048 (two hard constraints per node)");
assert.ok(!["complete", "bipartite"].includes(pattern) || (nodes >= 2 && nodes <= 32), "dense diagnostic nodes: 2..32");
assert.ok(pattern !== "bipartite" || nodes % 2 === 0, "bipartite nodes must be even");
assert.ok(["auto", "table", "getters"].includes(requestedReadout), "readout: auto | table | getters");
const api = await import(pathToFileURL(resolve(packageRoot, "index.js")));
const abi = await import(pathToFileURL(resolve(packageRoot, "program-wire/abi-v1.js")));
const hasOutputTable = typeof api.ProgramSnapshot.prototype.copyOutputTable === "function";
assert.ok(requestedReadout !== "table" || hasOutputTable, "the package must provide copyOutputTable");
const readout = requestedReadout === "getters" || !hasOutputTable ? "getters" : "table";
const started = performance.now();
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
await api.default({ module_or_path: wasm });
const initializationMs = performance.now() - started;
const producer = api.decodeCertificateEnvelope(api.issueSourceCertificateEnvelope()).producerRevision;

function color(index) {
  if (pattern === "grouped") return [20 + index % 4, 20, 20];
  if (pattern === "distinct") return [index % 128, Math.floor(index / 128) % 128, 20];
  return [20, 20, 20];
}

const builder = new abi.ProgramWireBuilderV1();
if (pattern !== "sparse") builder.surfaceInputPort(1).inputSurface(1, 1);
if (pattern === "shared") builder.source(1, color(0));
for (let index = 0; index < nodes; index += 1) {
  const id = index + 1;
  if (pattern === "sparse") builder.surfaceInputPort(id).inputSurface(id, id);
  if (pattern !== "shared") builder.source(id, color(index));
  builder.fixedTarget(id, pattern === "shared" ? 1 : id).solidPaint(id, id)
    .sourceOverOccurrence(id, id, pattern === "sparse" ? id : 1, 64, 0.2, abi.SURROUND_AVERAGE_V1)
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
} else if (pattern === "complete" || pattern === "bipartite") {
  for (let id = 1; id <= nodes; id += 1) {
    const candidates = [];
    for (let other = 1; other <= nodes; other += 1) {
      const connected = pattern === "complete"
        ? other !== id
        : (id <= nodes / 2) !== (other <= nodes / 2);
      if (connected) candidates.push(other);
    }
    builder.exactIntrinsicRelationHard(nodes + id, id, candidates);
  }
}
const wire = builder.finish();
const results = [];
for (let sample = 0; sample < warmups + samples; sample += 1) {
  const start = performance.now();
  const runtime = api.compileProgramWire(wire, 1);
  const compileMs = performance.now() - start;
  const sparseBackgrounds = pattern === "sparse" ? new Uint8Array(nodes * 3).fill(255) : null;
  const updates = [];
  let checksum = 0;
  try {
    for (let index = 0; index < frames; index += 1) {
      const refused = index % 4 === 2;
      const background = refused ? [0, 0, 0] : [255 - index % 2, 255, 255];
      if (sparseBackgrounds !== null) {
        // Каждая четвёрка меняет только одну независимую подложку и возвращает
        // её в исходное состояние перед переходом к следующей ветви.
        const branch = Math.floor(index / 4) % nodes;
        const phase = index % 4;
        sparseBackgrounds.set(refused ? [0, 0, 0] : [phase === 3 ? 255 : 254 - phase, 255, 255], branch * 3);
      }
      const updateStart = performance.now();
      const snapshot = runtime.updateObserved(BigInt(index + 1), new Uint32Array([1]),
        sparseBackgrounds ?? new Uint8Array(background), sparseBackgrounds === null ? 1 : nodes);
      try {
        assert.equal(snapshot.state, refused ? "failed" : "ready");
        if (!refused) {
          assert.equal(snapshot.outputCount(), nodes);
          const table = readout === "table" ? snapshot.copyOutputTable() : null;
          if (table !== null) assert.equal(table.length, nodes * 5);
          for (let output = 0; output < nodes; output += 1) {
            const row = output * 5;
            const slot = table === null ? snapshot.outputSlot(output) : table[row];
            const rgb = table === null ? snapshot.outputRgb(output) : null;
            const red = table === null ? rgb[0] : table[row + 1];
            const green = table === null ? rgb[1] : table[row + 2];
            const blue = table === null ? rgb[2] : table[row + 3];
            const opacity = table === null ? snapshot.outputOpacity(output) : table[row + 4];
            const expected = color(slot - 1);
            // Одинаковый oracle проверяет каждое поле без дополнительной RGB-копии.
            assert.equal(slot, output + 1);
            assert.equal(red, expected[0]);
            assert.equal(green, expected[1]);
            assert.equal(blue, expected[2]);
            assert.equal(opacity, 1);
            checksum = (checksum + slot + red) >>> 0;
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
console.log(JSON.stringify({
  node: process.version, v8: process.versions.v8, producer, pattern, nodes, readout, ...protocol,
  wasmBytes: wasm.length, wasmSha256: createHash("sha256").update(wasm).digest("hex"),
  initializationMs, ...splitMeasurements(results, protocol), maxRssKiB: process.resourceUsage().maxRSS,
}));
