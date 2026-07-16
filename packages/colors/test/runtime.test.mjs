// Behaviour tests for the framework-free runtime — pure logic + the reactive
// controller driven through injected fakes, so they run under plain `node --test`
// with no browser and no WASM. JS-boundary↔core parity внутри одного wasm runtime
// отдельно проверяет headless-Chrome `wasm_parity` test.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseCssColor,
  compositeOver,
  toHex,
  compositeStackToHex,
  effectiveBackground,
  oklabLerp,
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

  // Half-seam из Rust-регрессора: фиксирует тот же порядок binary64-операций,
  // чтобы reference-композит и официальный потребитель не разошлись на LSB.
  assert.equal(toHex(compositeOver([0, 5, 5, 0.1], [5, 5, 5, 1])), "#050505");

  // Expanded-форма на этих соседних alpha давала 208→207→208. Affine-
  // reference фиксирует другой, объявленный operation order; этот конкретный
  // seam характеризуется без утверждения глобальной монотонности.
  const centre = 0.812992125984252;
  const predecessor = centre - Number.EPSILON / 2;
  const successor = centre + Number.EPSILON / 2;
  const seam = [predecessor, centre, successor].map(
    (alpha) => compositeOver([255, 0, 0, alpha], [1, 0, 0, 1])[0],
  );
  assert.ok(seam[0] <= seam[1] && seam[1] <= seam[2], String(seam));
});

test("material alpha rechecks in the declared byte-scale affine legacy-WCAG profile", () => {
  // Independent consumer oracle for the Rust material solver. The product's
  // versioned profile freezes the original WCAG 2.1 (2018) 0.03928 split; it is
  // not the current W3C formula and JS `**` is not a cross-runtime powf proof.
  // These fixtures verify the emitted alpha in the declared JS operation order.
  const srgbToLinear = (byte) => {
    const encoded = byte / 255;
    return encoded <= 0.03928
      ? encoded / 12.92
      : ((encoded + 0.055) / 1.055) ** 2.4;
  };
  const contrastAgainstWhite = (rgb) => {
    const luminance =
      0.2126 * srgbToLinear(rgb[0]) +
      0.7152 * srgbToLinear(rgb[1]) +
      0.0722 * srgbToLinear(rgb[2]);
    return 1.05 / (luminance + 0.05);
  };

  for (const { tint, oldAlpha, selectedAlpha, floor } of [
    {
      tint: [2, 2, 2],
      oldAlpha: 0.41945837958353488,
      selectedAlpha: 0.41945837958353827,
      floor: 3,
    },
    {
      tint: [0, 0, 0],
      oldAlpha: 0.65080978737170625,
      selectedAlpha: 0.65080978737171102,
      floor: 7,
    },
  ]) {
    const bottom = [255, 255, 255, 1];
    const oldContrast = contrastAgainstWhite(compositeOver([...tint, oldAlpha], bottom));
    const selectedContrast = contrastAgainstWhite(
      compositeOver([...tint, selectedAlpha], bottom),
    );
    assert.ok(oldContrast < floor, `old ${oldContrast} must miss ${floor}`);
    assert.ok(selectedContrast >= floor, `selected ${selectedContrast} must hold ${floor}`);
  }

  // Endpoint-only evaluation used to accept the old alpha exactly at the
  // requested floor, while an admissible interior backdrop crossed the
  // downward 0.03928 EOTF seam and missed it. The selected alpha comes from the
  // conservative channel envelope that includes both seam sides.
  const seamFloor = 19.7963;
  const oldSeamAlpha = 0.96071066801335769;
  const selectedSeamAlpha = 0.96072042755466869;
  const interiorByte = 0.9997624803942831 * 255;
  const interior = [interiorByte, interiorByte, interiorByte, 1];
  const oldInteriorContrast = contrastAgainstWhite(
    compositeOver([0, 0, 0, oldSeamAlpha], interior),
  );
  const selectedInteriorContrast = contrastAgainstWhite(
    compositeOver([0, 0, 0, selectedSeamAlpha], interior),
  );
  assert.ok(
    oldInteriorContrast < seamFloor,
    `endpoint-only alpha ${oldInteriorContrast} must expose the interior seam miss`,
  );
  assert.ok(
    selectedInteriorContrast >= seamFloor,
    `enveloped alpha ${selectedInteriorContrast} must hold ${seamFloor}`,
  );
});

test("toHex rounds and clamps", () => {
  assert.equal(toHex([255, 255, 255, 1]), "#FFFFFF");
  assert.equal(toHex([0, 0, 0, 1]), "#000000");
  assert.equal(toHex([127.6, 17, 300]), "#80115B".slice(0, 5) + "FF"); // 300→FF, 127.6→80, 17→11
});

test("toHex coerces non-finite channels to 0 (valid CSS, never #NAN…)", () => {
  // A malformed Rgba (NaN/Infinity channels) must still yield a valid #RRGGBB,
  // not an invalid CSS string. Reachable via the public toHex/compositeStackToHex.
  assert.equal(toHex([NaN, 0, 0]), "#000000");
  assert.equal(toHex([Infinity, 128, -Infinity]), "#008000"); // any non-finite → 0 (not clamped)
  assert.equal(toHex([undefined, 255, 255]), "#00FFFF");
  assert.match(toHex([NaN, NaN, NaN]), /^#[0-9A-F]{6}$/);
});

test("oklabLerp returns opaque RGB endpoints byte-exactly", () => {
  assert.equal(oklabLerp("#000000", "#F0F0F0", 0), "#000000");
  assert.equal(oklabLerp("#000000", "#F0F0F0", 1), "#F0F0F0");
  // Out-of-range t is clamped to the endpoints.
  assert.equal(oklabLerp("#102030", "#A0B0C0", -1), "#102030");
  assert.equal(oklabLerp("#102030", "#A0B0C0", 2), "#A0B0C0");
  // Endpoints are re-normalised through toHex (uppercase, 6-digit).
  assert.equal(oklabLerp("#0a0", "#fff", 0), "#00AA00");
});

test("oklabLerp discards endpoint alpha because its output is opaque", () => {
  assert.equal(oklabLerp("rgba(255, 0, 0, 0.25)", "#0000FF", 0), "#FF0000");
  assert.equal(oklabLerp("transparent", "#FFFFFF", 0), "#000000");
  assert.equal(oklabLerp("#000000", "rgba(0, 0, 255, 0.25)", 1), "#0000FF");
});

test("oklabLerp grey midpoint follows Oklab coordinates, not encoded sRGB", () => {
  // A straight sRGB blend of black→white at t=0.5 is #808080 (channel 128). The
  // Oklab-coordinate midpoint L=0.5 converts to an encoded channel near 99.
  const mid = oklabLerp("#000000", "#FFFFFF", 0.5);
  const ch = parseCssColor(mid)[0];
  assert.ok(ch > 90 && ch < 110, `Oklab midpoint channel near 99, got ${ch}`);
  assert.ok(ch < 128, "must be darker than the sRGB midpoint (128)");
});

test("oklabLerp lightness is monotone across t for a grey ramp", () => {
  let prev = -1;
  for (let i = 0; i <= 10; i++) {
    const ch = parseCssColor(oklabLerp("#000000", "#FFFFFF", i / 10))[0];
    assert.ok(ch >= prev, `grey ramp must be monotone: ${prev} → ${ch}`);
    prev = ch;
  }
});

test("oklabLerp keeps chromatic interpolation in gamut and parseable", () => {
  // Red→blue midpoint: a valid #RRGGBB (clamped if needed), never NaN/garbage.
  const mid = oklabLerp("#FF0000", "#0000FF", 0.5);
  assert.match(mid, /^#[0-9A-F]{6}$/);
  assert.ok(parseCssColor(mid) !== null);
});

test("oklabLerp falls back to the valid endpoint on unparseable input", () => {
  assert.equal(oklabLerp("not-a-color", "#123456", 0.3), "#123456");
  assert.equal(oklabLerp("#123456", "garbage", 0.7), "#123456");
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
