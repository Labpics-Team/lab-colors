import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { runInNewContext } from "node:vm";

const packageRoot = new URL("../", import.meta.url);
const require = createRequire(import.meta.url);

async function importRootWithInstrumentedWasm(t) {
  const fixture = await mkdtemp(join(tmpdir(), "labcolors-feasibility-host-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  await mkdir(join(fixture, "pkg"));
  await writeFile(join(fixture, "package.json"), '{"type":"module"}\n');
  await writeFile(join(fixture, "index.js"), await readFile(new URL("../index.js", import.meta.url)));
  await writeFile(
    join(fixture, "pkg/labcolors.js"),
    `
globalThis.__labcolorsFeasibilityCalls = { evaluate: [], max: [], oversize: [] };
let initialized = false;
export default async function init() { initialized = true; }
export function initSync() { initialized = true; }
export class LabColors {}
export function evaluateWcag22() {}
export function numericalCapabilityManifest() {}
export function wcag22FeasibilityMaxRequestBytesV1() {
  if (!initialized) throw new Error("WASM not initialized");
  globalThis.__labcolorsFeasibilityCalls.max.push(true);
  return 657380;
}
export function evaluateWcag22FeasibilityV1(request) {
  if (!initialized) throw new Error("WASM not initialized");
  globalThis.__labcolorsFeasibilityCalls.evaluate.push(request);
  return { schemaVersion: 1, outcome: "success", feasibility: { status: "notEvaluated", result: {} } };
}
export function wcag22FeasibilityEnvelopeTooLargeV1(requestedBytes) {
  if (!initialized) throw new Error("WASM not initialized");
  globalThis.__labcolorsFeasibilityCalls.oversize.push(requestedBytes);
  return {
    schemaVersion: 1,
    outcome: "failure",
    error: {
      source: "transport",
      error: {
        code: "envelopeTooLarge",
        requestedBytes: requestedBytes.toString(),
        limitBytes: "657380",
      },
    },
  };
}
`,
  );
  for (const [file, exports] of [
    ["apply-theme.js", "export function applyTheme() {}\n"],
    ["watch-theme.js", "export function watchTheme() {}\n"],
    ["adapt-theme.js", "export function adaptTheme() {}\n"],
    [
      "effective-bg.js",
      "export function effectiveBackground() {}\nexport function parseCssColor() {}\nexport function compositeOver() {}\nexport function compositeStackToHex() {}\nexport function toHex() {}\nexport function oklabLerp() {}\n",
    ],
  ]) {
    await writeFile(join(fixture, file), exports);
  }
  return import(`${pathToFileURL(join(fixture, "index.js")).href}?case=${Date.now()}`);
}

test("package root rejects an oversized envelope before the avoidable WASM copy", async (t) => {
  const root = await importRootWithInstrumentedWasm(t);
  assert.deepEqual(
    globalThis.__labcolorsFeasibilityCalls.max,
    [],
    "import must not touch uninitialized WASM",
  );
  root.initSync();
  assert.equal(root.wcag22FeasibilityMaxBytes(), 657380);

  const oversized = new Uint8Array(root.wcag22FeasibilityMaxBytes() + 1);
  const outcome = root.evaluateWcag22Feasibility(oversized);
  assert.equal(outcome.outcome, "failure");
  assert.equal(outcome.error.error.code, "envelopeTooLarge");
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.evaluate, []);
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.oversize, [657381n]);

  const exactLimit = new Uint8Array(root.wcag22FeasibilityMaxBytes());
  assert.equal(root.evaluateWcag22Feasibility(exactLimit).outcome, "success");
  assert.equal(globalThis.__labcolorsFeasibilityCalls.evaluate.length, 1);
  assert.strictEqual(globalThis.__labcolorsFeasibilityCalls.evaluate[0], exactLimit);
});

test("package root rejects non-Uint8Array inputs before touching WASM", async (t) => {
  const root = await importRootWithInstrumentedWasm(t);
  const invalidInputs = [
    ["Array", []],
    ["Int8Array", new Int8Array()],
    ["DataView", new DataView(new ArrayBuffer(0))],
    ["array-like object", { byteLength: 0, length: 0 }],
    [
      "tag-spoofed array-like object",
      { byteLength: 0, length: 0, [Symbol.toStringTag]: "Uint8Array" },
    ],
  ];

  for (const [label, input] of invalidInputs) {
    assert.throws(
      () => root.evaluateWcag22Feasibility(input),
      {
        name: "TypeError",
        message: "evaluateWcag22Feasibility request must be a Uint8Array",
      },
      label,
    );
  }
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls, {
    evaluate: [],
    max: [],
    oversize: [],
  });

  root.initSync();
  const crossRealm = runInNewContext("new Uint8Array([123])");
  assert.equal(root.evaluateWcag22Feasibility(crossRealm).outcome, "success");
  assert.strictEqual(globalThis.__labcolorsFeasibilityCalls.evaluate[0], crossRealm);
});

test("package root derives the envelope ceiling from WASM instead of copying a literal", async () => {
  const source = await readFile(new URL("../index.js", import.meta.url), "utf8");
  const declarations = await readFile(new URL("../index.d.ts", import.meta.url), "utf8");
  assert.doesNotMatch(source, /657380/u);
  assert.match(source, /wcag22FeasibilityMaxBytes/u);
  assert.match(source, /wcag22FeasibilityMaxRequestBytesV1/u);
  assert.match(source, /wcag22FeasibilityEnvelopeTooLargeV1/u);
  assert.match(source, /evaluateWcag22FeasibilityV1/u);
  assert.match(declarations, /wcag22FeasibilityMaxBytes\(\): number/u);
  assert.match(declarations, /request: Uint8Array/u);
  assert.match(declarations, /Wcag22FeasibilityRequestV1/u);
  assert.match(declarations, /Wcag22FeasibilityOutcomeV1/u);
  for (const internal of [
    "Wcag22FeasibilityProofV1",
    "Wcag22FeasibilityCoreErrorV1",
    "Wcag22FeasibilityTransportErrorV1",
  ]) {
    assert.doesNotMatch(
      declarations,
      new RegExp(`\\b${internal},`, "u"),
      `${internal} must not widen the curated package-root type menu`,
    );
  }
});

test("feasibility TypeScript is exhaustive and excludes forged/proportional states", async (t) => {
  const fixture = await mkdtemp(join(tmpdir(), "labcolors-feasibility-types-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  const rust = await readFile(
    new URL("../../../crates/labcolors-wasm/src/lib.rs", import.meta.url),
    "utf8",
  );
  const marker = 'const TS_RESULT_TYPES: &\'static str = r##"';
  const start = rust.indexOf(marker);
  const end = rust.indexOf('"##;', start + marker.length);
  assert.notEqual(start, -1, "wasm-bindgen custom types section exists");
  assert.notEqual(end, -1, "wasm-bindgen custom types section is closed");
  const declarations = rust.slice(start + marker.length, end);
  await writeFile(
    join(fixture, "labcolors.d.ts"),
    `${declarations}\nexport declare function evaluateWcag22FeasibilityV1(request: Uint8Array): Wcag22FeasibilityOutcomeV1;\n`,
  );
  await writeFile(
    join(fixture, "consumer.ts"),
    `
import {
  evaluateWcag22FeasibilityV1,
  type DecimalU64V1,
  type Wcag22FeasibilityEvaluatedV1,
  type Wcag22FeasibilityOutcomeV1,
  type Wcag22FeasibilityRequestV1,
} from "./labcolors.js";

const request: Wcag22FeasibilityRequestV1 = {
  schemaVersion: 1,
  domainId: "srgb8-neutral-axis-v1",
  resourceProfileId: "compile-v1",
  relations: [{
    relationId: "opaque-client-id",
    occurrenceId: "opaque-occurrence-id",
    kind: "applicable",
    criterion: "sc-1.4.3-text-default",
    adjacent: [[0, 0, 0]],
  }],
};
const bytes = new TextEncoder().encode(JSON.stringify(request));
const outcome = evaluateWcag22FeasibilityV1(bytes);

function consume(value: Wcag22FeasibilityOutcomeV1): string {
  if (value.outcome === "success") {
    switch (value.feasibility.status) {
      case "feasible": return value.feasibility.result.proof.logicalAssessments;
      case "infeasible": return value.feasibility.result.proof.logicalAssessments;
      case "notEvaluated": return value.feasibility.result.relationSetDigest.join(",");
      default: {
        const exhaustive: never = value.feasibility;
        return exhaustive;
      }
    }
  }
  switch (value.error.source) {
    case "transport": return value.error.error.code;
    case "core": return value.error.error.code;
    case "incompatibleCoreContract": return value.error.source;
    default: {
      const exhaustive: never = value.error;
      return exhaustive;
    }
  }
}
consume(outcome);

declare const exact: DecimalU64V1;
const exactText: string = exact;
void exactText;
// @ts-expect-error decimal u64 values are sealed Rust output, not forgeable text
const forgedNegative: DecimalU64V1 = "-1";
// @ts-expect-error even in-range-looking text must come from the protocol output
const forgedPositive: DecimalU64V1 = "524032";
// @ts-expect-error values beyond u64 are not accepted by a loose bigint template
const forgedOverflow: DecimalU64V1 = "18446744073709551616";
type HasCells = "cells" extends keyof Wcag22FeasibilityEvaluatedV1 ? true : false;
type HasAssessments = "assessments" extends keyof Wcag22FeasibilityEvaluatedV1 ? true : false;
type HasCandidates = "feasibleCandidates" extends keyof Wcag22FeasibilityEvaluatedV1 ? true : false;
const noCells: false = false as HasCells;
const noAssessments: false = false as HasAssessments;
const noCandidates: false = false as HasCandidates;
void [noCells, noAssessments, noCandidates, forgedNegative, forgedPositive, forgedOverflow];

// @ts-expect-error byte API has no string overload
evaluateWcag22FeasibilityV1("{}");
// @ts-expect-error a successful outcome cannot omit its feasibility terminal
const forgedSuccess: Wcag22FeasibilityOutcomeV1 = { schemaVersion: 1, outcome: "success" };
const forgedFailure: Wcag22FeasibilityOutcomeV1 = {
  schemaVersion: 1,
  outcome: "failure",
  error: { source: "incompatibleCoreContract" },
  // @ts-expect-error a failure cannot carry a successful terminal
  feasibility: { status: "notEvaluated", result: {} },
};
void [forgedSuccess, forgedFailure];
`,
  );
  const tsc = require.resolve("typescript/bin/tsc");
  execFileSync(
    process.execPath,
    [
      tsc,
      "--noEmit",
      "--strict",
      "--skipLibCheck",
      "false",
      "--target",
      "ES2022",
      "--module",
      "ESNext",
      "--moduleResolution",
      "Bundler",
      join(fixture, "consumer.ts"),
    ],
    { stdio: "inherit" },
  );
});

function packedBit(bytes, index) {
  return (bytes[Math.floor(index / 8)] & (1 << (index % 8))) !== 0;
}

function assertPackedPartition(result) {
  const edges = Number(result.proof.applicableEdges);
  assert.equal(result.domain.length, 256);
  assert.deepEqual(result.domain[0], [0, 0, 0]);
  assert.deepEqual(result.domain[255], [255, 255, 255]);
  assert.equal(result.failureMatrix.length, 32 * edges);
  assert.equal(result.proof.partition.length, 32);
  for (let candidate = 0; candidate < result.domain.length; candidate += 1) {
    let hasFailure = false;
    for (let edge = 0; edge < edges; edge += 1) {
      hasFailure ||= packedBit(result.failureMatrix, candidate * edges + edge);
    }
    assert.equal(
      packedBit(result.proof.partition, candidate),
      !hasFailure,
      `candidate ${candidate}: partition must be the LSB0 all-edge reduction`,
    );
  }
}

test("built package root replays pack 5 through an independent packed consumer", async () => {
  const glueUrl = new URL("../pkg/labcolors.js", import.meta.url);
  const wasmBytes = await readFile(new URL("../pkg/labcolors_bg.wasm", import.meta.url));
  const raw = await import(glueUrl.href);
  raw.initSync({ module: new WebAssembly.Module(wasmBytes) });
  const root = await import(`../index.js?feasibility=${Date.now()}`);
  assert.equal(
    root.wcag22FeasibilityMaxBytes(),
    raw.wcag22FeasibilityMaxRequestBytesV1(),
  );

  const vectors = JSON.parse(
    await readFile(
      new URL("../../../conformance/vectors/wcag22-feasibility.json", import.meta.url),
      "utf8",
    ),
  );
  const expectedCounts = new Map([
    ["text-default-seven", 7],
    ["text-default-two", 2],
    ["text-default-zero", 0],
    ["text-large-scale-ninety-two", 92],
    ["ui-component-ninety-two", 92],
    ["graphical-object-ninety-two", 92],
    ["ui-component-fifty-nine", 59],
  ]);
  const encoder = new TextEncoder();
  let mutationSubject;
  for (const vector of vectors) {
    const outcome = root.evaluateWcag22Feasibility(encoder.encode(vector.requestJson));
    assert.equal(JSON.stringify(outcome), vector.outcomeJson, `${vector.caseId}: wire drift`);
    if (outcome.outcome !== "success" || outcome.feasibility.status === "notEvaluated") {
      continue;
    }
    const result = outcome.feasibility.result;
    assertPackedPartition(result);
    const expected = expectedCounts.get(vector.caseId);
    if (expected !== undefined) {
      const actual = result.proof.partition.reduce(
        (count, byte) => count + byte.toString(2).replaceAll("0", "").length,
        0,
      );
      assert.equal(actual, expected, vector.caseId);
    }
    mutationSubject ??= structuredClone(result);
  }
  assert.ok(mutationSubject, "corpus contains an evaluated packed result");
  mutationSubject.proof.partition[0] ^= 1;
  assert.throws(
    () => assertPackedPartition(mutationSubject),
    /partition must be the LSB0 all-edge reduction/u,
  );
});
