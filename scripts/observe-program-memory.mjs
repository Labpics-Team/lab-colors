// Phase and post-consumer diagnostics run only in separate instrumented processes.
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
const packageRoot = resolve(process.argv[2]);
const { initSync } = await import(pathToFileURL(resolve(packageRoot, "pkg/labcolors.js")));
const phases = [];
const hook = Symbol.for("labcolors.bench.program.phase");
let memory;
globalThis[hook] = (phase, cycle = null) => {
  // First called after initialization. Never instantiate a second module.
  memory ??= initSync().memory;
  const { rss, heapUsed, external } = process.memoryUsage();
  phases.push({ phase, cycle, rss, heapUsed, external, maxRssKiB: process.resourceUsage().maxRSS,
    linearMemoryBytes: memory.buffer.byteLength });
};
try {
  await import(pathToFileURL(harness));
} finally {
  delete globalThis[hook];
}
assert.ok(memory instanceof WebAssembly.Memory);
const expected = [["declaration", null]];
for (let cycle = 0; cycle <= Number(process.argv[5]); cycle++) {
  for (const phase of ["compiled", "snapshot", "outputs-read", "released"]) expected.push([phase, cycle]);
}
assert.deepEqual(phases.map(({ phase, cycle }) => [phase, cycle]), expected);
const report = {
  phase: "after-consumer",
  phases,
  linearMemoryBytes: memory.buffer.byteLength,
  processMemory: process.memoryUsage(),
  heap: getHeapStatistics(),
  code: getHeapCodeStatistics(),
};
await writeFile(output, JSON.stringify(report) + "\n");
