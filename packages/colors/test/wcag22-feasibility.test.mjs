import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { runInNewContext } from "node:vm";

const require = createRequire(import.meta.url);

async function importCompilerWithInstrumentedWasm(t) {
  const fixture = await mkdtemp(join(tmpdir(), "labcolors-compiler-host-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  await mkdir(join(fixture, "compiler"));
  await writeFile(join(fixture, "package.json"), '{"type":"module"}\n');
  await writeFile(
    join(fixture, "compiler.js"),
    await readFile(new URL("../compiler.js", import.meta.url)),
  );
  await writeFile(
    join(fixture, "compiler/labcolors_compiler.js"),
    `
globalThis.__labcolorsFeasibilityCalls = { evaluate: [], max: [], oversize: [] };
let initialized = false;
export default async function init() { initialized = true; }
export function initSync() { initialized = true; }
export function wcag22FeasibilityMaxRequestBytesV1() {
  if (!initialized) throw new Error("WASM not initialized");
  globalThis.__labcolorsFeasibilityCalls.max.push(true);
  globalThis.__labcolorsFeasibilityBeforeForward?.();
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
  return import(`${pathToFileURL(join(fixture, "compiler.js")).href}?case=${Date.now()}`);
}

test("compiler rejects an oversized envelope before the avoidable WASM copy", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  assert.deepEqual(
    globalThis.__labcolorsFeasibilityCalls.max,
    [],
    "import must not touch uninitialized WASM",
  );
  compiler.initSync();
  assert.equal(compiler.wcag22FeasibilityMaxBytes(), 657380);

  const oversized = new Uint8Array(compiler.wcag22FeasibilityMaxBytes() + 1);
  const outcome = compiler.evaluateWcag22Feasibility(oversized);
  assert.equal(outcome.outcome, "failure");
  assert.equal(outcome.error.error.code, "envelopeTooLarge");
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.evaluate, []);
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.oversize, [657381n]);

  const exactLimit = new Uint8Array(compiler.wcag22FeasibilityMaxBytes());
  assert.equal(compiler.evaluateWcag22Feasibility(exactLimit).outcome, "success");
  assert.equal(globalThis.__labcolorsFeasibilityCalls.evaluate.length, 1);
  assert.notStrictEqual(globalThis.__labcolorsFeasibilityCalls.evaluate[0], exactLimit);
  assert.equal(globalThis.__labcolorsFeasibilityCalls.evaluate[0].buffer, exactLimit.buffer);
});

test("compiler measures the intrinsic Uint8Array length, not an own-property spoof", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  compiler.initSync();
  const oversized = new Uint8Array(compiler.wcag22FeasibilityMaxBytes() + 1);
  Object.defineProperty(oversized, "byteLength", { value: 0 });

  const outcome = compiler.evaluateWcag22Feasibility(oversized);
  assert.equal(outcome.error.error.code, "envelopeTooLarge");
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.evaluate, []);
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls.oversize, [657381n]);
});

test("compiler does not pass a spoofed Uint8Array length to generated glue", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  compiler.initSync();
  const request = new Uint8Array([1, 2, 3]);
  Object.defineProperty(request, "length", { value: 0 });

  assert.equal(compiler.evaluateWcag22Feasibility(request).outcome, "success");
  const [forwarded] = globalThis.__labcolorsFeasibilityCalls.evaluate;
  assert.equal(forwarded.length, 3);
  assert.deepEqual([...forwarded], [1, 2, 3]);
});

test("compiler normalizes hostile subclasses and rejects detached views before WASM", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  compiler.initSync();

  class HostileUint8Array extends Uint8Array {}
  Object.defineProperty(HostileUint8Array.prototype, "length", {
    get: () => 0,
  });
  const hostile = new HostileUint8Array([4, 5]);
  assert.equal(compiler.evaluateWcag22Feasibility(hostile).outcome, "success");
  assert.deepEqual([...globalThis.__labcolorsFeasibilityCalls.evaluate[0]], [4, 5]);

  const detached = new Uint8Array([6]);
  structuredClone(detached.buffer, { transfer: [detached.buffer] });
  assert.throws(
    () => compiler.evaluateWcag22Feasibility(detached),
    /request must be a live Uint8Array/u,
  );

  const normal = new Uint8Array([7]);
  assert.equal(compiler.evaluateWcag22Feasibility(normal).outcome, "success");
  assert.deepEqual([...globalThis.__labcolorsFeasibilityCalls.evaluate[1]], [7]);
});

test("compiler rejects a detached view before requiring initialized WASM", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  const detached = new Uint8Array([1]);
  structuredClone(detached.buffer, { transfer: [detached.buffer] });

  assert.throws(
    () => compiler.evaluateWcag22Feasibility(detached),
    /request must be a live Uint8Array/u,
  );
  assert.deepEqual(globalThis.__labcolorsFeasibilityCalls, {
    evaluate: [],
    max: [],
    oversize: [],
  });
});

test("compiler freezes a length-tracking shared view at the checked byte length", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
  compiler.initSync();
  const buffer = new SharedArrayBuffer(1, { maxByteLength: 2 });
  const request = new Uint8Array(buffer);
  globalThis.__labcolorsFeasibilityBeforeForward = () => buffer.grow(2);
  t.after(() => { delete globalThis.__labcolorsFeasibilityBeforeForward; });

  assert.equal(compiler.evaluateWcag22Feasibility(request).outcome, "success");
  const [forwarded] = globalThis.__labcolorsFeasibilityCalls.evaluate;
  assert.equal(buffer.byteLength, 2);
  assert.equal(forwarded.length, 1);
});

test("compiler rejects non-Uint8Array inputs before touching WASM", async (t) => {
  const compiler = await importCompilerWithInstrumentedWasm(t);
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
      () => compiler.evaluateWcag22Feasibility(input),
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

  compiler.initSync();
  const crossRealm = runInNewContext("new Uint8Array([123])");
  assert.equal(compiler.evaluateWcag22Feasibility(crossRealm).outcome, "success");
  assert.deepEqual([...globalThis.__labcolorsFeasibilityCalls.evaluate[0]], [123]);
  assert.equal(globalThis.__labcolorsFeasibilityCalls.evaluate[0].buffer, crossRealm.buffer);
});

test("compiler derives the envelope ceiling from WASM instead of copying a literal", async () => {
  const source = await readFile(new URL("../compiler.js", import.meta.url), "utf8");
  const declarations = await readFile(new URL("../compiler.d.ts", import.meta.url), "utf8");
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
      `${internal} must not widen the curated compiler type menu`,
    );
  }
});

test("feasibility TypeScript is exhaustive and excludes forged/proportional states", async (t) => {
  const fixture = await mkdtemp(join(tmpdir(), "labcolors-feasibility-types-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  let declarations;
  try {
    declarations = await readFile(
      new URL("../compiler/labcolors_compiler.d.ts", import.meta.url),
      "utf8",
    );
  } catch (error) {
    if (error?.code === "ENOENT") {
      throw new Error("compiler declarations are required; run `npm run build` before tests", {
        cause: error,
      });
    }
    throw error;
  }
  assert.match(
    declarations,
    /export function evaluateWcag22FeasibilityV1\(request: Uint8Array\): Wcag22FeasibilityOutcomeV1;/u,
    "wasm-bindgen must publish the reviewed byte API declaration",
  );
  await mkdir(join(fixture, "compiler"));
  await writeFile(join(fixture, "compiler", "labcolors_compiler.d.ts"), declarations);
  await writeFile(
    join(fixture, "wcag22.d.ts"),
    await readFile(new URL("../wcag22.d.ts", import.meta.url)),
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
} from "./compiler/labcolors_compiler.js";

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
      "--lib",
      "ES2022,DOM,ESNext.Disposable",
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

// Compile V1 assigns one 64-KiB packed-result page and a separate 64-KiB
// opaque-ID allowance; the strict JSON grammar derives the 657,380-byte cap.
const EXACT_MAX_DOMAIN_CANDIDATES = 256;
const EXACT_MAX_PACKED_RESULT_BYTES = 65536;
const EXACT_MAX_PARTITION_BYTES = EXACT_MAX_DOMAIN_CANDIDATES / 8;
const EXACT_MAX_RELATIONS =
  (EXACT_MAX_PACKED_RESULT_BYTES - EXACT_MAX_PARTITION_BYTES) / EXACT_MAX_PARTITION_BYTES;
const EXACT_MAX_FEASIBILITY = Object.freeze({
  requestBytes: 657380,
  relations: EXACT_MAX_RELATIONS,
  opaqueUtf8Bytes: 65536,
  logicalAssessments: EXACT_MAX_DOMAIN_CANDIDATES * EXACT_MAX_RELATIONS,
  failureMatrixBytes: EXACT_MAX_PARTITION_BYTES * EXACT_MAX_RELATIONS,
  partitionBytes: EXACT_MAX_PARTITION_BYTES,
});

const EXACT_MAX_ID_ALPHABET = Object.freeze([
  0, 1, 2, 3, 4, 5, 6, 7, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
  26, 27, 28, 29, 30, 31,
]);

function exactMaxFeasibilityWitness(relationCount = EXACT_MAX_FEASIBILITY.relations) {
  const relationIdWidth = 3;
  const idRadix = EXACT_MAX_ID_ALPHABET.length;
  const relationIdBytes = relationCount * relationIdWidth;
  const firstOccurrenceBytes =
    EXACT_MAX_FEASIBILITY.opaqueUtf8Bytes - relationIdBytes - (relationCount - 1);
  assert.ok(firstOccurrenceBytes > 0, "exact-max witness must have a non-empty first ID");

  const relations = Array.from({ length: relationCount }, (_, index) => ({
    relationId: String.fromCharCode(
      EXACT_MAX_ID_ALPHABET[Math.floor(index / (idRadix * idRadix)) % idRadix],
      EXACT_MAX_ID_ALPHABET[Math.floor(index / idRadix) % idRadix],
      EXACT_MAX_ID_ALPHABET[index % idRadix],
    ),
    occurrenceId: index === 0 ? "\0".repeat(firstOccurrenceBytes) : "\0",
    kind: "applicable",
    criterion: "sc-1.4.11-ui-component-or-state",
    adjacent: [[255, 255, 255]],
  }));
  const encoder = new TextEncoder();
  const request = encoder.encode(
    JSON.stringify({
      schemaVersion: 1,
      domainId: "srgb8-neutral-axis-v1",
      resourceProfileId: "compile-v1",
      relations,
    }),
  );
  const opaqueUtf8Bytes = relations.reduce(
    (total, relation) =>
      total +
      encoder.encode(relation.relationId).byteLength +
      encoder.encode(relation.occurrenceId).byteLength,
    0,
  );
  return {
    request,
    rawRelations: relations.length,
    rawApplicableRelations: relations.length,
    rawAdjacentEntries: relations.reduce(
      (total, relation) => total + relation.adjacent.length,
      0,
    ),
    opaqueUtf8Bytes,
  };
}

function assertExactMaxFeasibilityWitness(witness) {
  assert.equal(witness.request.byteLength, EXACT_MAX_FEASIBILITY.requestBytes);
  assert.equal(witness.rawRelations, EXACT_MAX_FEASIBILITY.relations);
  assert.equal(witness.rawApplicableRelations, EXACT_MAX_FEASIBILITY.relations);
  assert.equal(witness.rawAdjacentEntries, EXACT_MAX_FEASIBILITY.relations);
  assert.equal(witness.opaqueUtf8Bytes, EXACT_MAX_FEASIBILITY.opaqueUtf8Bytes);
}

function assertNoCellOrProportionalDto(value) {
  if (Array.isArray(value)) {
    for (const item of value) assertNoCellOrProportionalDto(item);
    return;
  }
  if (value === null || typeof value !== "object") return;
  for (const [key, child] of Object.entries(value)) {
    assert.doesNotMatch(key, /cell|proportional/iu);
    assertNoCellOrProportionalDto(child);
  }
}

function assertExactMaxFeasibilityResult(outcome) {
  assert.equal(outcome.outcome, "success");
  assert.equal(outcome.feasibility.status, "feasible");
  const result = outcome.feasibility.result;
  assert.deepEqual(Object.keys(result).sort(), ["domain", "failureMatrix", "proof", "relations"]);
  assert.equal(result.domain.length, EXACT_MAX_DOMAIN_CANDIDATES);
  assert.equal(result.relations.length, EXACT_MAX_FEASIBILITY.relations);
  assert.equal(
    result.relations.reduce(
      (edges, relation) => edges + (relation.kind === "applicable" ? relation.adjacent.length : 0),
      0,
    ),
    EXACT_MAX_FEASIBILITY.relations,
  );
  assert.equal(result.failureMatrix.length, EXACT_MAX_FEASIBILITY.failureMatrixBytes);
  assert.equal(result.proof.partition.length, EXACT_MAX_FEASIBILITY.partitionBytes);
  assert.equal(result.proof.canonicalRelations, String(EXACT_MAX_FEASIBILITY.relations));
  assert.equal(result.proof.applicableRelations, String(EXACT_MAX_FEASIBILITY.relations));
  assert.equal(result.proof.notApplicableRelations, "0");
  assert.equal(result.proof.applicableEdges, String(EXACT_MAX_FEASIBILITY.relations));
  assert.equal(result.proof.logicalAssessments, String(EXACT_MAX_FEASIBILITY.logicalAssessments));
  assertNoCellOrProportionalDto(outcome);
  assertPackedPartition(result);
}

let builtCompilerPromise;

function loadBuiltCompiler() {
  builtCompilerPromise ??= (async () => {
    const glueUrl = new URL("../compiler/labcolors_compiler.js", import.meta.url);
    const wasmBytes = await readFile(
      new URL("../compiler/labcolors_compiler_bg.wasm", import.meta.url),
    );
    const raw = await import(glueUrl.href);
    raw.initSync({ module: new WebAssembly.Module(wasmBytes) });
    const compiler = await import(`../compiler.js?feasibility=${Date.now()}`);
    return { compiler, raw };
  })();
  return builtCompilerPromise;
}

test("built public compiler accepts the exact maximum legal feasibility request", async () => {
  const { compiler } = await loadBuiltCompiler();
  const witness = exactMaxFeasibilityWitness();
  assertExactMaxFeasibilityWitness(witness);
  assert.throws(
    () =>
      assertExactMaxFeasibilityWitness(
        exactMaxFeasibilityWitness(EXACT_MAX_FEASIBILITY.relations - 1),
      ),
    { name: "AssertionError" },
    "an off-by-one relation witness must not certify the exact boundary",
  );

  const outcome = compiler.evaluateWcag22Feasibility(witness.request);
  assertExactMaxFeasibilityResult(outcome);
  const missingEdge = {
    ...outcome,
    feasibility: {
      ...outcome.feasibility,
      result: {
        ...outcome.feasibility.result,
        relations: outcome.feasibility.result.relations.slice(0, -1),
      },
    },
  };
  assert.throws(
    () => assertExactMaxFeasibilityResult(missingEdge),
    { name: "AssertionError" },
    "a result missing one canonical edge must not satisfy the exact-boundary proof",
  );
});

test("built compiler replays pack 5 through an independent packed consumer", async () => {
  const { compiler, raw } = await loadBuiltCompiler();
  assert.equal(
    compiler.wcag22FeasibilityMaxBytes(),
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
    const outcome = compiler.evaluateWcag22Feasibility(encoder.encode(vector.requestJson));
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
