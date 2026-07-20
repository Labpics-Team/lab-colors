import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { initSync } from "../pkg/labcolors.js";
import { observePointBackground } from "../background-observation.js";

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

const INITIALS = Object.freeze({
  "background-image": "none",
  "background-blend-mode": "normal",
  "background-clip": "border-box",
  "box-shadow": "none",
  "mix-blend-mode": "normal",
  filter: "none",
  "backdrop-filter": "none",
  "-webkit-backdrop-filter": "none",
  "mask-image": "none",
  "-webkit-mask-image": "none",
  "clip-path": "none",
  opacity: "1",
  display: "block",
  visibility: "visible",
  "content-visibility": "visible",
});

function style(backgroundColor, overrides = {}) {
  const values = { ...INITIALS, "background-color": backgroundColor, ...overrides };
  return {
    getPropertyValue(property) {
      return values[property] ?? "";
    },
  };
}

function tree(entries) {
  const nodes = entries.map((entry) => ({
    style: style(entry.color, entry.effects),
    parent: null,
  }));
  for (let index = 0; index < nodes.length - 1; index++) nodes[index].parent = nodes[index + 1];
  return {
    leaf: nodes[0],
    getStyle: (node) => node.style,
    parentOf: (node) => node.parent,
  };
}

test("C8c checks group effects above the first opaque colour", () => {
  for (const [property, value, reason] of [
    ["opacity", "0.5", "element-opacity"],
    ["filter", "blur(1px)", "filter"],
    ["mix-blend-mode", "multiply", "mix-blend-mode"],
    ["background-clip", "padding-box", "background-clip"],
    ["box-shadow", "0 0 2px black", "box-shadow"],
    ["clip-path", "circle(40%)", "clip-path"],
  ]) {
    const host = tree([
      { color: "rgba(255, 0, 0, 0.5)" },
      { color: "rgb(0, 0, 255)" },
      { color: "rgb(0, 255, 0)", effects: { [property]: value } },
    ]);
    assert.deepEqual(observePointBackground(host.leaf, host), {
      kind: "unknown",
      reason,
    });
  }
});

test("C8c ignores only background colours hidden behind an opaque base", () => {
  const host = tree([
    { color: "rgba(255, 0, 0, 0.5)" },
    { color: "rgb(0, 0, 255)" },
    { color: "rgb(0, 255, 0)" },
  ]);
  assert.deepEqual(observePointBackground(host.leaf, host), {
    kind: "point",
    hex: "#800080",
  });
});

test("C8c does not claim a point for non-rendered elements", () => {
  for (const [property, value, reason] of [
    ["display", "none", "display-none"],
    ["display", "contents", "display-contents"],
    ["visibility", "hidden", "visibility"],
    ["visibility", "collapse", "visibility"],
    ["content-visibility", "hidden", "content-visibility"],
    ["content-visibility", "auto", "content-visibility"],
  ]) {
    const host = tree([{ color: "rgb(1, 2, 3)", effects: { [property]: value } }]);
    assert.deepEqual(observePointBackground(host.leaf, host), {
      kind: "unknown",
      reason,
    });
  }
});

test("C8c validates host seams before the first read", () => {
  const host = tree([{ color: "rgb(1, 2, 3)" }]);
  let reads = 0;
  const getStyle = (node) => {
    reads++;
    return node.style;
  };

  assert.throws(
    () => observePointBackground(host.leaf, { getStyle, parentOf: 1 }),
    /getStyle and parentOf must be functions/u,
  );
  assert.throws(
    () => observePointBackground(host.leaf, { getStyle, parentOf: host.parentOf, checkpoint: 1 }),
    /checkpoint must be a function/u,
  );
  assert.equal(reads, 0);
});
