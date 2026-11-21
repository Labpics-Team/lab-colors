import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";
import { measurementProtocol, splitMeasurements } from "./measurement-protocol.mjs";

const packageRoot = resolve(process.argv[2] ?? "packages/colors");
const nodes = Number(process.argv[3] ?? 64);
const protocol = measurementProtocol({ samples: process.argv[4], warmups: process.argv[5], frames: process.argv[6] });
const { samples, warmups, frames } = protocol;
assert.ok(Number.isInteger(nodes) && nodes >= 1 && nodes <= 4096, "nodes: 1..4096");
const api = await import(pathToFileURL(resolve(packageRoot, "index.js")));
const abi = await import(pathToFileURL(resolve(packageRoot, "program-wire/abi-v2.js")));
const wasm = await readFile(resolve(packageRoot, "pkg/labcolors_bg.wasm"));
await api.default({ module_or_path: wasm });

// Один объявленный цвет применяется в нескольких местах интерфейса. Общий
// resolver должен выбрать его для всех наблюдаемых фонов, включая конфликт.
const builder = new abi.ProgramWireBuilderV2()
  .finiteTarget(21, [
    { id: 201, rgb: [180, 180, 180], opacity: 1 },
    { id: 202, rgb: [80, 80, 80], opacity: 1 },
    { id: 203, rgb: [0, 0, 0], opacity: 1 },
  ])
  .selectionRelease(1n, [
    [{ id: 1, choices: [{ target: 21, candidate: 201 }] }],
    [{ id: 2, choices: [{ target: 21, candidate: 202 }] }],
    [{ id: 3, choices: [{ target: 21, candidate: 203 }] }],
  ])
  .surfaceInputPort(31).inputSurface(51, 31);
for (let id = 1; id <= nodes; id += 1) {
  builder.solidPaint(id, 21)
    .sourceOverOccurrence(id, id, 51, 64, .2, abi.SURROUND_AVERAGE_V1)
    .presentationRoot(id, id).presentationTarget(id, id)
    .wcag22VisibleUnary(true, id, id, abi.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
    .output(id, id);
}
const wire = builder.finish();
const cases = [
  { backgrounds: [255, 255, 255], scenarios: [1], state: "ready", level: 80 },
  { backgrounds: [255, 255, 255, 100, 100, 100], scenarios: [1, 2], state: "ready", level: 0 },
  { backgrounds: [255, 255, 255, 0, 0, 0], scenarios: [1, 2], state: "failed" },
  { backgrounds: [0, 0, 0], scenarios: [1], state: "ready", level: 180 },
];
const results = [];
for (let sample = 0; sample < warmups + samples; sample += 1) {
  const start = performance.now();
  const runtime = api.compileProgramWire(wire, 1);
  const compileMs = performance.now() - start;
  const updatesMs = [];
  let checksum = 0;
  try {
    for (let index = 0; index < frames; index += 1) {
      const expected = cases[index % cases.length];
      const updateStart = performance.now();
      const snapshot = runtime.updateObserved(BigInt(index + 1),
        new Uint32Array(expected.scenarios), new Uint8Array(expected.backgrounds), 1);
      try {
        assert.equal(snapshot.state, expected.state);
        if (expected.state === "ready") {
          assert.equal(snapshot.outputCount(), nodes);
          const table = typeof snapshot.copyOutputTable === "function" ? snapshot.copyOutputTable() : null;
          if (table !== null) assert.equal(table.length, nodes * 5);
          for (let output = 0; output < nodes; output += 1) {
            const offset = output * 5;
            const slot = table === null ? snapshot.outputSlot(output) : table[offset];
            const rgb = table === null ? snapshot.outputRgb(output) : null;
            const red = table === null ? rgb[0] : table[offset + 1];
            const green = table === null ? rgb[1] : table[offset + 2];
            const blue = table === null ? rgb[2] : table[offset + 3];
            const opacity = table === null ? snapshot.outputOpacity(output) : table[offset + 4];
            assert.equal(slot, output + 1);
            assert.equal(red, expected.level);
            assert.equal(green, expected.level);
            assert.equal(blue, expected.level);
            assert.equal(opacity, 1);
            checksum = (checksum + slot + red) >>> 0;
          }
        }
      } finally {
        snapshot.free();
      }
      updatesMs.push(performance.now() - updateStart);
    }
  } finally {
    runtime.free();
  }
  results.push({ compileMs, cycleMs: performance.now() - start, updatesMs, checksum });
}
console.log(JSON.stringify({
  node: process.version, v8: process.versions.v8, pattern: "selection", nodes, ...protocol,
  wasmBytes: wasm.length, wasmSha256: createHash("sha256").update(wasm).digest("hex"),
  ...splitMeasurements(results, protocol), maxRssKiB: process.resourceUsage().maxRSS,
}));
