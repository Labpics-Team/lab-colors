import assert from "node:assert/strict";
import { readFileSync, realpathSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  invalidU32, legalStreams, criteria, manifestKeys, siteKeys, assessmentKeys, evidenceKeys,
  exactKeys, ordinaryData, freshContainers, mutateLeaves, withProjectionSetters,
  fixedWire, observe, outputs, authoredOutput,
} from "./consumer-boundary-fixtures.mjs";

// Только настоящий installed export. Отсутствующий пакет/WASM — отказ, а не skip.
const installedRoot = dirname(fileURLToPath(import.meta.resolve("@labpics/colors")));
assert.equal(basename(installedRoot), "colors");
assert.equal(basename(dirname(installedRoot)), "@labpics");
assert.equal(basename(dirname(dirname(installedRoot))), "node_modules");
assert.equal(realpathSync(installedRoot), resolve(installedRoot));
if (process.env.LABCOLORS_INSTALLED_ROOT) {
  assert.equal(installedRoot, realpathSync(process.env.LABCOLORS_INSTALLED_ROOT));
}
const metadata = JSON.parse(readFileSync(resolve(installedRoot, "build-metadata.json"), "utf8"));
assert.match(metadata.sourceSha, /^[0-9a-f]{40}$/u);
if (process.env.LABCOLORS_SOURCE_SHA) assert.equal(metadata.sourceSha, process.env.LABCOLORS_SOURCE_SHA);
const exported = (specifier, relative) => {
  const path = fileURLToPath(import.meta.resolve(specifier));
  assert.equal(path, resolve(installedRoot, relative));
  assert.equal(realpathSync(path), path, "public export must be a regular installed path");
  return import(specifier);
};
const api = await exported("@labpics/colors", "index.js");
const versions = [];
for (const version of [1, 2, 3]) {
  const module = await exported("@labpics/colors/program-wire/abi-v" + version + ".js",
    "program-wire/abi-v" + version + ".js");
  versions.push([version, module["ProgramWireBuilderV" + version]]);
}
const wasm = fileURLToPath(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"));
assert.equal(wasm, resolve(installedRoot, "pkg/labcolors_bg.wasm"));
assert.equal(realpathSync(wasm), wasm);
await api.default({ module_or_path: readFileSync(wasm) });

function checkManifest(manifest) {
  ordinaryData(manifest);
  exactKeys(manifest, manifestKeys);
  assert.equal(manifest.schemaVersion, 2);
  assert.equal(manifest.coverage, "migrated-sites-only-v1");
  assert.match(manifest.checksum, /^[0-9a-f]{8}$/u);
  assert.deepEqual(manifest.sites.map((site) => site.siteId), [
    "point-support-retained-reference-surplus-v1", "wcag22-srgb8-contrast-v1",
  ]);
  for (const site of manifest.sites) {
    exactKeys(site, siteKeys);
    for (const key of siteKeys.filter((key) => key !== "siteId")) assert.ok(Array.isArray(site[key]));
  }
}
function checkAssessment(assessment, foreground, background, criterion, decision) {
  ordinaryData(assessment);
  exactKeys(assessment, assessmentKeys);
  assert.equal(assessment.kind, "evaluated");
  assert.equal(assessment.profileId, "wcag22-srgb8-contrast-v1");
  assert.equal(assessment.foreground, foreground);
  assert.equal(assessment.background, background);
  assert.equal(assessment.criterion, criterion);
  assert.equal(assessment.decision, decision);
  for (const bounds of [assessment.foregroundLuminanceQ55, assessment.backgroundLuminanceQ55]) {
    exactKeys(bounds, ["lower", "upper"]);
    assert.match(bounds.lower, /^[0-9]+$/u);
    assert.match(bounds.upper, /^[0-9]+$/u);
  }
  exactKeys(assessment.evidence, evidenceKeys);
  assert.equal(assessment.evidence.kind, "canonical-finite-bounded");
}

test("manifest and WCAG expose recursive ordinary data with reachable pass/fail controls", () => {
  checkManifest(api.numericalCapabilityManifest());
  for (const criterion of criteria) {
    checkAssessment(api.evaluateWcag22("#000000", "#FFFFFF", criterion),
      "#000000", "#FFFFFF", criterion, "pass");
    checkAssessment(api.evaluateWcag22("#000000", "#000000", criterion),
      "#000000", "#000000", criterion, "fail");
  }
});

test("projection ignores hostile inherited setters and restores prototypes", () => {
  const manifest = withProjectionSetters(() => api.numericalCapabilityManifest());
  assert.equal(manifest.calls, 0); checkManifest(manifest.result);
  for (const criterion of criteria) {
    const assessment = withProjectionSetters(() => api.evaluateWcag22("#000000", "#FFFFFF", criterion));
    assert.equal(assessment.calls, 0);
    checkAssessment(assessment.result, "#000000", "#FFFFFF", criterion, "pass");
  }

  const traps = ["get", "set", "code", "operation"].map((key) => ({
    key, previous: Object.getOwnPropertyDescriptor(Object.prototype, key),
  }));
  let installed = 0, calls = 0;
  let manifestResult, assessmentResult, colorError, wireError, unexpectedRuntime;
  const trap = () => { calls += 1; throw new Error("inherited projection metadata accessor"); };
  try {
    for (const { key } of traps) {
      const descriptor = { __proto__: null, configurable: true };
      if (key === "get" || key === "set") descriptor.get = trap;
      else descriptor.set = trap;
      Object.defineProperty(Object.prototype, key, descriptor);
      installed += 1;
    }
    manifestResult = api.numericalCapabilityManifest();
    assessmentResult = api.evaluateWcag22("#000000", "#FFFFFF", criteria[0]);
    try { api.evaluateWcag22("invalid", "#FFFFFF", criteria[0]); }
    catch (error) { colorError = error; }
    try { unexpectedRuntime = api.compileProgramWire(new Uint8Array(), 1); }
    catch (error) { wireError = error; }
  } finally {
    for (let index = installed - 1; index >= 0; index -= 1) {
      const { key, previous } = traps[index];
      if (previous) Object.defineProperty(Object.prototype, key, { __proto__: null, ...previous });
      else delete Object.prototype[key];
    }
    unexpectedRuntime?.free?.();
  }
  assert.equal(calls, 0);
  checkManifest(manifestResult);
  checkAssessment(assessmentResult, "#000000", "#FFFFFF", criteria[0], "pass");
  assert.equal(api.isBindingError(colorError), true);
  assert.equal(api.isProgramError(wireError), true);
  for (const [error, code, operation] of [
    [colorError, "invalid_color", "evaluateWcag22"],
    [wireError, "program_wire", "compileProgramWire"],
  ]) {
    assert.equal(Object.getPrototypeOf(error), Error.prototype);
    for (const [key, value] of [["code", code], ["operation", operation]]) {
      assert.deepEqual(Object.getOwnPropertyDescriptor(error, key), {
        value, writable: true, enumerable: true, configurable: true,
      });
    }
  }
});

test("all nested manifest and WCAG containers belong to each caller result", () => {
  const producers = [() => api.numericalCapabilityManifest(),
    ...criteria.map((criterion) => () => api.evaluateWcag22("#000000", "#FFFFFF", criterion))];
  for (const produce of producers) {
    const first = produce(), baseline = structuredClone(first), sibling = produce();
    freshContainers(first, sibling);
    mutateLeaves(first);
    assert.deepEqual(sibling, baseline);
    const next = produce();
    assert.deepEqual(next, baseline);
    freshContainers(first, next); freshContainers(sibling, next);
    ordinaryData(next);
  }
});

for (const [version, Builder] of versions) {
  test("V" + version + " owning ready/refused snapshot handles survive runtime.free", () => {
    const runtime = api.compileProgramWire(fixedWire(Builder, version), 1);
    let ready, refused, live = true;
    const expectedVersion = version === 3 ? 2 : 1;
    const refusedState = version === 3 ? "unsatisfied" : "failed";
    const check = () => {
      assert.equal(ready.snapshotVersion, expectedVersion);
      assert.equal(ready.revision, 1n); assert.equal(ready.state, "ready");
      assert.deepEqual(outputs(ready), authoredOutput);
      assert.deepEqual([...ready.presentationCompositeRgb(71, 1)], [20, 20, 20]);
      assert.equal(refused.snapshotVersion, expectedVersion);
      assert.equal(refused.revision, 2n); assert.equal(refused.state, refusedState);
      assert.deepEqual(outputs(refused), version === 3 ? [] : authoredOutput);
    };
    try {
      ready = observe(runtime, 1n, 1, 255);
      refused = observe(runtime, 2n, 1, 0);
      check();
      live = false; runtime.free();
      check();
    } finally {
      try { refused?.free(); } finally { try { ready?.free(); } finally { if (live) runtime.free(); } }
    }
  });
}

test("attachment snapshot/render/authority RGB buffers remain independent after every handle is freed", () => {
  const Builder = versions[0][1], host = { point: null };
  const attachment = api.attachProgramWire(fixedWire(Builder, 1), 1, 91, 501, 71, 61, (intent) => {
    host.point = intent.point; return true;
  });
  let snapshot, render, authority;
  const retained = [];
  try {
    snapshot = observe(attachment, 1n, 1, 255);
    assert.equal(snapshot.state, "ready"); assert.equal(snapshot.hasRender(), true);
    render = snapshot.render(); assert.ok(render);
    authority = attachment.materializationAuthority();
    const readers = [
      () => snapshot.outputRgb(0), () => render.outputRgb(), () => render.terminalCompositeRgb(),
      () => authority.outputRgb(), () => authority.terminalCompositeRgb(),
    ];
    const buffers = new Set();
    for (const [index, read] of readers.entries()) {
      const first = read(), second = read();
      for (const bytes of [first, second]) {
        assert.ok(bytes instanceof Uint8Array); assert.equal(bytes.byteLength, 3);
        assert.deepEqual([...bytes], [20, 20, 20]);
        assert.equal(buffers.has(bytes.buffer), false); buffers.add(bytes.buffer);
      }
      first[0] = 200 + index;
      assert.deepEqual([...second], [20, 20, 20]);
      for (const other of readers) assert.deepEqual([...other()], [20, 20, 20]);
      retained.push({ first, second, expected: [200 + index, 20, 20] });
    }
  } finally {
    try { authority?.free(); } finally {
      try { render?.free(); } finally {
        try { snapshot?.free(); } finally {
          host.point = null;
          try { attachment.dispose(true); } finally { attachment.free(); }
        }
      }
    }
  }
  assert.equal(retained.length, 5);
  for (const { first, second, expected } of retained) {
    assert.deepEqual([...first], expected); assert.deepEqual([...second], [20, 20, 20]);
    first.fill(0);
    for (const sibling of retained) assert.deepEqual([...sibling.second], [20, 20, 20]);
  }
});

const programError = (code, operation) => (error) =>
  api.isProgramError(error) && error.code === code && error.operation === operation;
const bindingError = (error) => api.isBindingError(error)
  && error.code === "invalid_output_index" && error.operation === "readProgramOutput";
function refuses(invoke, expected) {
  let unexpected;
  try { assert.throws(() => { unexpected = invoke(); }, expected); }
  finally { unexpected?.free?.(); }
}
const scalarWire = fixedWire(versions[0][1], 1);
for (const [label, value] of invalidU32) {
  test(label + ": compile refuses u32 coercion", () => {
    refuses(() => api.compileProgramWire(scalarWire, value),
      programError("program_instantiate", "compileProgramWire"));
  });
  for (const operation of ["updateObserved", "updateUnknown"]) {
    test(label + ": " + operation + " refuses before committing the revision", () => {
      const runtime = api.compileProgramWire(scalarWire, 1);
      let first, retry;
      try {
        first = observe(runtime, 1n, 1, 255);
        refuses(() => operation === "updateObserved" ? observe(runtime, 2n, value, 255)
          : runtime.updateUnknown(2n, value), programError("program_update", operation));
        assert.deepEqual(outputs(first), authoredOutput);
        retry = observe(runtime, 2n, 1, 255);
        assert.equal(retry.state, "ready"); assert.equal(retry.revision, 2n);
        assert.deepEqual(outputs(retry), authoredOutput);
      } finally { try { retry?.free(); } finally { try { first?.free(); } finally { runtime.free(); } } }
    });
  }
  test(label + ": all output readers reject a fractional index", () => {
    const runtime = api.compileProgramWire(scalarWire, 1);
    let snapshot;
    try {
      snapshot = observe(runtime, 1n, 1, 255);
      for (const method of ["outputSlot", "outputRgb", "outputOpacity"]) {
        assert.throws(() => snapshot[method](value), bindingError);
      }
      assert.deepEqual(outputs(snapshot), authoredOutput);
    } finally { try { snapshot?.free(); } finally { runtime.free(); } }
  });
}
test("u32 signed zero and unsigned midpoint remain lawful streams; -0 still reads output zero", () => {
  for (const stream of legalStreams) {
    const runtime = api.compileProgramWire(scalarWire, stream);
    let snapshot;
    try {
      snapshot = observe(runtime, 1n, 1, 255);
      assert.equal(snapshot.state, "ready"); assert.deepEqual(outputs(snapshot), authoredOutput);
      assert.equal(snapshot.outputSlot(-0), 91);
      assert.deepEqual([...snapshot.outputRgb(-0)], [20, 20, 20]);
      assert.equal(snapshot.outputOpacity(-0), 1);
    } finally { try { snapshot?.free(); } finally { runtime.free(); } }
  }
});
