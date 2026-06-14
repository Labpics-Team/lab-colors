// Behaviour tests for the framework-free runtime — pure logic + the reactive
// controller driven through injected fakes, so they run under plain `node --test`
// with no browser and no WASM. (The WASM↔native parity is covered separately by
// the headless-Chrome `wasm_parity` test.)

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseCssColor,
  compositeOver,
  toHex,
  compositeStackToHex,
  effectiveBackground,
} from "../effective-bg.js";
import { watchTheme } from "../watch-theme.js";

test("parseCssColor handles the forms computed style yields", () => {
  assert.deepEqual(parseCssColor("rgb(255, 0, 0)"), [255, 0, 0, 1]);
  assert.deepEqual(parseCssColor("rgba(0, 128, 255, 0.5)"), [0, 128, 255, 0.5]);
  assert.deepEqual(parseCssColor("rgb(10 20 30 / 0.25)"), [10, 20, 30, 0.25]);
  assert.deepEqual(parseCssColor("#FFFFFF"), [255, 255, 255, 1]);
  assert.deepEqual(parseCssColor("#0a0"), [0, 170, 0, 1]);
  assert.deepEqual(parseCssColor("transparent"), [0, 0, 0, 0]);
  assert.equal(parseCssColor("rebeccapurple"), null); // unknown keyword → no layer
  assert.equal(parseCssColor(""), null);
  assert.equal(parseCssColor(42), null);
});

test("compositeOver is true source-over alpha", () => {
  // Opaque over anything → the top colour.
  assert.deepEqual(compositeOver([10, 20, 30, 1], [200, 200, 200, 1]), [10, 20, 30, 1]);
  // 50% black over white → mid grey, opaque.
  const r = compositeOver([0, 0, 0, 0.5], [255, 255, 255, 1]);
  assert.equal(Math.round(r[0]), 128);
  assert.equal(r[3], 1);
  // Fully transparent top → bottom unchanged.
  assert.deepEqual(compositeOver([9, 9, 9, 0], [40, 50, 60, 1]), [40, 50, 60, 1]);
});

test("toHex rounds and clamps", () => {
  assert.equal(toHex([255, 255, 255, 1]), "#FFFFFF");
  assert.equal(toHex([0, 0, 0, 1]), "#000000");
  assert.equal(toHex([127.6, 17, 300]), "#80115B".slice(0, 5) + "FF"); // 300→FF, 127.6→80, 17→11
});

test("compositeStackToHex composites front-to-back over an opaque base", () => {
  // 50% black panel over white base → #808080.
  assert.equal(compositeStackToHex([[0, 0, 0, 0.5]], [255, 255, 255, 1]), "#808080");
  // Empty stack → the base itself.
  assert.equal(compositeStackToHex([], [18, 18, 22, 1]), "#121216");
});

// A tiny fake element tree for effectiveBackground: each node carries a
// background-color string and a parent. The injected getStyle/parentOf read it.
function fakeTree(chain) {
  // chain: array of bg strings, index 0 = leaf, last = root.
  const nodes = chain.map((bg) => ({ bg, parent: null }));
  for (let i = 0; i < nodes.length - 1; i++) nodes[i].parent = nodes[i + 1];
  const getStyle = (el) => ({ getPropertyValue: () => el.bg });
  const parentOf = (el) => el.parent;
  return { leaf: nodes[0], getStyle, parentOf };
}

test("effectiveBackground stops at the first opaque ancestor", () => {
  const { leaf, getStyle, parentOf } = fakeTree([
    "rgba(0, 0, 0, 0)", // leaf transparent
    "rgba(255, 255, 255, 0.5)", // translucent panel
    "rgb(0, 0, 0)", // opaque black base
    "rgb(255, 0, 0)", // (never reached — behind the opaque)
  ]);
  // 50% white over black → #808080; the red below the opaque black is ignored.
  assert.equal(effectiveBackground(leaf, { getStyle, parentOf }), "#808080");
});

test("effectiveBackground falls back to white when the chain is fully translucent", () => {
  const { leaf, getStyle, parentOf } = fakeTree(["transparent", "rgba(0,0,0,0)"]);
  assert.equal(effectiveBackground(leaf, { getStyle, parentOf }), "#FFFFFF");
  const tinted = fakeTree(["rgba(0, 0, 0, 0.5)"]);
  // 50% black over the default white fallback → #808080.
  assert.equal(
    effectiveBackground(tinted.leaf, { getStyle: tinted.getStyle, parentOf: tinted.parentOf }),
    "#808080",
  );
});

// A fake LabColors engine + element for watchTheme.
function fakeEngine() {
  const calls = [];
  return {
    calls,
    resolveTheme(bg, theme) {
      calls.push({ bg, theme });
      return { theme, background: bg, vars: { "--lab-x": bg }, roles: {} };
    },
  };
}

function fakeElement(bg) {
  const props = new Map();
  return {
    bg,
    style: {
      length: 0,
      item: () => null,
      setProperty: (k, v) => props.set(k, v),
      removeProperty: (k) => props.delete(k),
    },
    props,
  };
}

test("watchTheme applies on creation and re-resolves only when the bg changes", () => {
  const colors = fakeEngine();
  const el = fakeElement("rgb(255, 255, 255)");
  const ctrl = watchTheme(el, {
    colors,
    theme: "light",
    observe: false, // no DOM observer in node
    getStyle: (e) => ({ getPropertyValue: () => e.bg }),
    parentOf: () => null,
  });

  // Applied immediately.
  assert.equal(colors.calls.length, 1);
  assert.equal(colors.calls[0].bg, "#FFFFFF");
  assert.equal(el.props.get("--lab-x"), "#FFFFFF");
  assert.equal(ctrl.background(), "#FFFFFF");

  // No change → no re-resolve.
  ctrl.refresh();
  assert.equal(colors.calls.length, 1);

  // Background changes → one more resolve.
  el.bg = "rgb(0, 0, 0)";
  ctrl.refresh();
  assert.equal(colors.calls.length, 2);
  assert.equal(colors.calls[1].bg, "#000000");
  assert.equal(el.props.get("--lab-x"), "#000000");

  // force re-applies unconditionally.
  ctrl.refresh(true);
  assert.equal(colors.calls.length, 3);
});

test("watchTheme setTheme re-resolves under the new theme; explicit background wins", () => {
  const colors = fakeEngine();
  const el = fakeElement("rgb(255,255,255)");
  const ctrl = watchTheme(el, {
    colors,
    theme: "light",
    background: "#123456", // explicit — ancestor walk is bypassed
    observe: false,
  });
  assert.equal(colors.calls[0].bg, "#123456");
  ctrl.setTheme("dark");
  // Theme changed → re-resolve even though bg string is identical.
  assert.equal(colors.calls.length, 2);
  assert.equal(colors.calls[1].theme, "dark");
  assert.equal(colors.calls[1].bg, "#123456");
});

test("watchTheme rejects a missing engine", () => {
  assert.throws(() => watchTheme(fakeElement("#fff"), { theme: "light", observe: false }), TypeError);
});

test("watchTheme: stop() cancels a refresh already scheduled by a mutation", async () => {
  const colors = fakeEngine();
  const el = fakeElement("rgb(255,255,255)");
  // A fake MutationObserver whose callback we can fire on demand.
  let cb = null;
  const fakeObserver = function (fn) {
    cb = fn;
    return { observe() {}, disconnect() {} };
  };
  const win = { MutationObserver: fakeObserver, document: { documentElement: {} } };
  const ctrl = watchTheme(el, {
    colors,
    theme: "light",
    win,
    getStyle: (e) => ({ getPropertyValue: () => e.bg }),
    parentOf: () => null,
  });
  assert.equal(colors.calls.length, 1); // applied on creation

  el.bg = "rgb(0,0,0)";
  cb(); // a mutation schedules a refresh on the next microtask
  ctrl.stop(); // …but we stop before the microtask runs
  await Promise.resolve(); // let the microtask drain
  await Promise.resolve();
  assert.equal(colors.calls.length, 1, "no refresh must fire after stop()");
});
