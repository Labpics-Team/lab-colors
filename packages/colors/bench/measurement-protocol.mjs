import assert from "node:assert/strict";

export function measurementProtocol({ samples = 7, warmups = 1, frames = 64 } = {}) {
  samples = Number(samples);
  warmups = Number(warmups);
  frames = Number(frames);
  assert.ok(Number.isInteger(samples) && samples >= 3 && samples <= 100, "samples: 3..100");
  assert.ok(Number.isSafeInteger(warmups) && warmups >= 1, "warmups: positive integer, including the cold cycle");
  assert.ok(Number.isSafeInteger(frames) && frames > 0 && frames % 4 === 0, "frames: positive multiple of 4");
  assert.ok(Number.isSafeInteger(samples + warmups), "total cycles must be a safe integer");
  return { samples, warmups, frames, measurementMode: "diagnostic", stabilization: "unverified" };
}

export function splitMeasurements(results, { samples, warmups }) {
  assert.equal(results.length, samples + warmups);
  assert.ok(results.every(({ checksum }) => checksum === results[0].checksum));
  // Keep every cycle. A fixed cutoff does not establish engine stabilization.
  return {
    cold: results[0],
    warmupSamples: results.slice(1, warmups),
    samplesAfterWarmup: results.slice(warmups),
  };
}
