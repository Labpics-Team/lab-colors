// Post-consumer diagnostics run only in separate instrumented processes.
// The ordinary latency harness and its public API workload stay unchanged.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { getHeapCodeStatistics, getHeapStatistics } from "node:v8";

const harness = resolve(process.argv[2]);
const output = process.env.LABCOLORS_MEMORY_REPORT;
assert.ok(output, "LABCOLORS_MEMORY_REPORT must identify the diagnostic output");
// Restore the exact argv shape expected by the original consumer.
process.argv.splice(1, 1);
await import(pathToFileURL(harness));

const packageRoot = resolve(process.argv[2]);
const { initSync } = await import(pathToFileURL(resolve(packageRoot, "pkg/labcolors.js")));
// wasm-bindgen returns the already initialized exports before inspecting input.
// This does not create another instance or expose exports through the package.
const { memory } = initSync();
assert.ok(memory instanceof WebAssembly.Memory);
const report = {
  phase: "after-consumer",
  linearMemoryBytes: memory.buffer.byteLength,
  processMemory: process.memoryUsage(),
  heap: getHeapStatistics(),
  code: getHeapCodeStatistics(),
};
await writeFile(output, JSON.stringify(report) + "\n");
