// Публикация атомарной операции `wcag22-explicit-selection-v1` в compiler-роли
// (#296-C3): собранный WASM обязан повекторно воспроизвести закоммиченное
// conformance-семейство байт-в-байт, а host-обвязка — повторять hostile-законы
// feasibility-входа (preflight типа/oversize до избегаемой ABI-копии).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");

const compilerModule = await import("../compiler.js");
const {
  default: init,
  evaluateWcag22ExplicitSelection,
  wcag22ExplicitSelectionMaxBytes,
} = compilerModule;
const wasmBytes = readFileSync(
  join(root, "packages/colors/compiler/labcolors_compiler_bg.wasm"),
);
await init({ module_or_path: wasmBytes });

const family = JSON.parse(
  readFileSync(
    join(root, "conformance/vectors/wcag22-explicit-selection.json"),
    "utf8",
  ),
);

test("built compiler replays the committed explicit-selection family byte-for-byte", () => {
  assert.equal(family.length, 15, "committed corpus cardinality drifted");
  for (const vector of family) {
    const outcome = evaluateWcag22ExplicitSelection(
      new TextEncoder().encode(vector.requestJson),
    );
    assert.equal(
      JSON.stringify(outcome),
      vector.outcomeJson,
      `${vector.caseId}: compiler outcome bytes drifted from the committed corpus`,
    );
  }
});

test("atomic ceiling is derived from WASM and matches the protocol constant", () => {
  assert.equal(wcag22ExplicitSelectionMaxBytes(), 3_889_322);
});

test("oversize envelope fails closed before the avoidable WASM copy", () => {
  const oversized = new Uint8Array(wcag22ExplicitSelectionMaxBytes() + 1);
  const outcome = evaluateWcag22ExplicitSelection(oversized);
  assert.equal(outcome.outcome, "failure");
  assert.equal(outcome.error.source, "transport");
  assert.equal(outcome.error.error.code, "envelopeTooLarge");
  assert.equal(
    outcome.error.error.requestedBytes,
    String(wcag22ExplicitSelectionMaxBytes() + 1),
  );
});

test("non-Uint8Array and detached views are rejected before WASM", () => {
  for (const hostile of [null, "bytes", [1, 2, 3], new Float32Array(4)]) {
    assert.throws(
      () => evaluateWcag22ExplicitSelection(hostile),
      TypeError,
      String(hostile),
    );
  }
  const buffer = new ArrayBuffer(8);
  const view = new Uint8Array(buffer);
  structuredClone(buffer, { transfer: [buffer] });
  assert.throws(() => evaluateWcag22ExplicitSelection(view), TypeError);
});

test("published surface keeps the neutral feasibility operation intact", () => {
  // Обе операции живут в одной compiler-роли; публикация атомарной не
  // сдвигает нейтральную: тот же модуль отдаёт обе точки входа.
  assert.equal(typeof compilerModule.evaluateWcag22Feasibility, "function");
  assert.equal(typeof compilerModule.wcag22FeasibilityMaxBytes, "function");
});
