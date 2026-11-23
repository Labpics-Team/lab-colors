import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { measurementProtocol, splitMeasurements } from "../bench/measurement-protocol.mjs";

test("a fixed cutoff preserves cold, warming and retained pauses without claiming stabilization", () => {
  const protocol = measurementProtocol({ samples: 3, warmups: 4, frames: 8 });
  const results = [900, 20, 400, 10, 10, 300, 10].map((cycleMs) => ({ cycleMs, checksum: 42 }));
  const split = splitMeasurements(results, protocol);
  assert.deepEqual([split.cold, ...split.warmupSamples, ...split.samplesAfterWarmup], results);
  assert.equal(split.cold.cycleMs, 900);
  assert.deepEqual(split.warmupSamples.map((sample) => sample.cycleMs), [20, 400, 10]);
  assert.deepEqual(split.samplesAfterWarmup.map((sample) => sample.cycleMs), [10, 300, 10]);
  assert.equal(protocol.stabilization, "unverified");
  assert.equal(protocol.measurementMode, "diagnostic");
});

test("default protocol keeps the short workload and requires complete failure/recovery frames", () => {
  const protocol = measurementProtocol();
  assert.deepEqual([protocol.samples, protocol.warmups, protocol.frames], [7, 1, 64]);
  for (const frames of [0, 1, 6, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => measurementProtocol({ frames }), /frames/);
  }
  for (const warmups of [0, -1, 1.5, Number.NaN]) {
    assert.throws(() => measurementProtocol({ warmups }), /warmups/);
  }
});

test("comparison preserves independent processes and rejects incomplete or unequal work", () => {
  execFileSync(process.platform === "win32" ? "python" : "python3", [fileURLToPath(new URL("../../../scripts/test_program_performance.py", import.meta.url))],
    { encoding: "utf8", timeout: 30_000 });
});
