import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  initSync,
  __over,
} from "../pkg/labcolors.js";

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

const INVALID_RGB24 = 0xFFFFFFFF;

test("hidden point bridge preserves the byte-domain half tie", () => {
  assert.equal(
    __over(0xC0B2FA, 0.122, 0x000000),
    0x17161F,
  );
});

test("hidden point bridge preserves the declared affine operation order", () => {
  const alphas = [
    0.81299212598425186,
    0.81299212598425197,
    0.81299212598425208,
  ];
  assert.deepEqual(
    alphas.map((alpha) =>
      (__over(0xFF0000, alpha, 0x010000) >> 16) & 0xFF,
    ),
    [207, 208, 208],
  );
});

test("hidden point bridge matches an independent rational oracle on every byte pair", () => {
  let comparisons = 0;
  for (let source = 0; source <= 255; source++) {
    for (let backdrop = 0; backdrop <= 255; backdrop++) {
      const actual =
        __over(source << 16, 0.122, backdrop << 16) >> 16;
      const expected = Math.floor((122 * source + 878 * backdrop + 500) / 1000);
      assert.equal(actual, expected, `source=${source}, backdrop=${backdrop}`);
      comparisons++;
    }
  }
  assert.equal(comparisons, 65_536, "full single-channel domain must be exercised");
});

test("hidden point bridge has one invalid-opacity rejection channel", () => {
  for (const opacity of [NaN, -Infinity, -0.1, 1.1, Infinity]) {
    assert.equal(__over(0, opacity, 0), INVALID_RGB24);
  }
  assert.equal(__over(0, -0, 0x123456), 0x123456);
});
