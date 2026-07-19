import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import init, { initSync, LabColors } from "../index.js";

test("public initialization has one owner across async and sync routes", async () => {
  const module = new WebAssembly.Module(
    readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)),
  );
  assert.throws(
    () => initSync({ module: new Uint8Array([0]) }),
    WebAssembly.CompileError,
  );
  await assert.rejects(
    init({ module_or_path: new Uint8Array([0]) }),
    WebAssembly.CompileError,
  );

  let release;
  const delayed = new Promise((resolve) => {
    release = resolve;
  });

  const first = init({ module_or_path: delayed });
  const second = init({ module_or_path: delayed });
  assert.equal(first, second, "concurrent async callers must share one flight");
  assert.throws(
    () => initSync({ module }),
    /asynchronous initialization is in progress/u,
  );

  release(module);
  await first;
  assert.equal(await second, undefined);
  assert.equal(initSync({ module }), undefined, "ready initialization is idempotent");

  const engine = new LabColors();
  engine.free();
});
