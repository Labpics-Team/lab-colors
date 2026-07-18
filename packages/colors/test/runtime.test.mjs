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
import { applyTheme } from "../apply-theme.js";
import { watchTheme } from "../watch-theme.js";

function captureOutputConflict(fn, expectedConflicts) {
  let error = null;
  try {
    fn();
  } catch (caught) {
    error = caught;
  }
  assert.ok(error, "ordinary Unreachable must reject the whole output");
  assert.equal(error.name, "OutputConflictError");
  assert.equal(error.code, "output_conflict");
  assert.deepEqual(error.conflicts, expectedConflicts);
  assert.equal(Object.hasOwn(error, "vars"), false, "the error must not expose partial CSS");
  for (const conflict of error.conflicts) {
    assert.deepEqual(Object.keys(conflict).sort(), ["code", "message", "role"]);
  }
  return error;
}

function forgedOutputConflict(value = "#123456") {
  return {
    vars: { "--lab-safe": value },
    roles: {
      safe: {
        kind: "color",
        cssVar: "--lab-safe",
        hex: value,
        lc: 100,
      },
      first: {
        kind: "failure",
        cssVar: "--lab-first",
        category: "unreachable",
        code: "floor_unreachable",
        message: "first contract has no solution",
      },
      unresolved: {
        kind: "failure",
        cssVar: "--lab-unresolved",
        category: "unresolved",
        code: "bounded_search_exhausted",
        message: "bounded search did not decide",
      },
      second: {
        kind: "failure",
        cssVar: "--lab-second",
        category: "unreachable",
        code: "exceeds_range",
        message: "second contract has no solution",
      },
    },
  };
}

function observedElement(initial = []) {
  const props = new Map(initial);
  const mutations = [];
  return {
    props,
    mutations,
    style: {
      get length() {
        return props.size;
      },
      item: (index) => [...props.keys()][index] ?? null,
      setProperty(key, value) {
        mutations.push(["set", key, value]);
        props.set(key, value);
      },
      removeProperty(key) {
        mutations.push(["remove", key]);
        props.delete(key);
      },
    },
  };
}

test("applyTheme rejects a forged partial Unreachable snapshot before any DOM mutation", () => {
  const element = observedElement([["--lab-old", "#111111"]]);
  captureOutputConflict(() => applyTheme(element, forgedOutputConflict()), [
    {
      role: "first",
      code: "floor_unreachable",
      message: "first contract has no solution",
    },
    {
      role: "second",
      code: "exceeds_range",
      message: "second contract has no solution",
    },
  ]);
  assert.deepEqual(element.mutations, [], "preflight must precede clear-then-write");
  assert.deepEqual([...element.props], [["--lab-old", "#111111"]]);
});

test("applyTheme rejects accessor-backed snapshots before their getters can create TOCTOU", () => {
  const element = observedElement([["--lab-old", "#111111"]]);
  const roles = {
    safe: { kind: "color", cssVar: "--lab-safe", hex: "#123456", lc: 100 },
  };
  const vars = {};
  Object.defineProperty(vars, "--lab-safe", {
    enumerable: true,
    get() {
      roles.late = {
        kind: "failure",
        category: "unreachable",
        code: "floor_unreachable",
        message: "late conflict",
      };
      return "#123456";
    },
  });

  assert.throws(
    () => applyTheme(element, { vars, roles }),
    /data|accessor|property/u,
  );
  assert.deepEqual(element.mutations, []);
  assert.deepEqual(Object.keys(roles), ["safe"], "admission must not invoke the getter");
});

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
  const unchanged = ctrl.refresh();
  assert.equal(colors.calls.length, 1);
  assert.equal(unchanged.theme, "light");
  assert.equal(unchanged.background, "#FFFFFF");

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

test("watchTheme failed setTheme preserves the committed theme for a later refresh", () => {
  const el = fakeElement("rgb(255,255,255)");
  let background = "#123456";
  const calls = [];
  const colors = {
    resolveTheme(bg, theme) {
      calls.push({ bg, theme });
      if (theme === "dark") throw new Error("rejected: invalid_input");
      return {
        theme,
        background: bg,
        vars: { "--lab-x": `${theme}:${bg}` },
        roles: {},
      };
    },
  };
  const ctrl = watchTheme(el, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
  });
  const before = el.props.get("--lab-x");

  assert.throws(() => ctrl.setTheme("dark"), /rejected: invalid_input/u);
  assert.equal(el.props.get("--lab-x"), before, "the failed candidate must not repaint");

  background = "#654321";
  assert.doesNotThrow(() => ctrl.refresh());
  assert.deepEqual(calls.at(-1), { bg: "#654321", theme: "light" });
  assert.equal(el.props.get("--lab-x"), "light:#654321");
});

test("watchTheme retries the same changed background after a transient resolve failure", () => {
  const el = fakeElement("rgb(255,255,255)");
  let background = "#123456";
  let failedBgAttempts = 0;
  const colors = {
    resolveTheme(bg, theme) {
      if (bg === "#654321") {
        failedBgAttempts++;
        if (failedBgAttempts === 1) throw new Error("internal_error: transient resolve");
      }
      return {
        theme,
        background: bg,
        vars: { "--lab-x": `${theme}:${bg}` },
        roles: {},
      };
    },
  };
  const ctrl = watchTheme(el, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
  });
  const before = el.props.get("--lab-x");

  background = "#654321";
  assert.throws(() => ctrl.refresh(), /transient resolve/u);
  assert.equal(ctrl.background(), "#123456");
  assert.equal(el.props.get("--lab-x"), before);

  assert.doesNotThrow(() => ctrl.refresh());
  assert.equal(failedBgAttempts, 2, "failed background evidence must remain retryable");
  assert.equal(ctrl.background(), "#654321");
  assert.equal(el.props.get("--lab-x"), "light:#654321");
});

test("watchTheme quarantines a forged partial conflict and retries the same observation", () => {
  const element = observedElement();
  let background = "#FFFFFF";
  let conflictedAttempts = 0;
  let conflictOnce = true;
  const colors = {
    resolveTheme(bg) {
      if (bg === "#000000" && conflictOnce) {
        conflictOnce = false;
        conflictedAttempts++;
        return forgedOutputConflict("#123456");
      }
      if (bg === "#000000") conflictedAttempts++;
      return {
        vars: { "--lab-safe": bg },
        roles: {
          safe: { kind: "color", cssVar: "--lab-safe", hex: bg, lc: 100 },
        },
      };
    },
  };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
  });
  const committed = new Map(element.props);
  element.mutations.length = 0;

  background = "#000000";
  captureOutputConflict(() => ctrl.refresh(), [
    {
      role: "first",
      code: "floor_unreachable",
      message: "first contract has no solution",
    },
    {
      role: "second",
      code: "exceeds_range",
      message: "second contract has no solution",
    },
  ]);
  assert.deepEqual(element.mutations, [], "conflict must not clear or write CSS");
  assert.deepEqual(element.props, committed, "the previous committed snapshot stays painted");
  assert.equal(ctrl.background(), "#FFFFFF", "failed evidence is not published");

  ctrl.refresh();
  assert.equal(conflictedAttempts, 2, "the identical failed observation remains retryable");
  assert.equal(ctrl.background(), "#000000");
  assert.equal(element.props.get("--lab-safe"), "#000000");
});

test("watchTheme keeps an immutable admitted snapshot for dirty repair", () => {
  const element = observedElement();
  let failWrite = false;
  const originalSet = element.style.setProperty;
  element.style.setProperty = (key, value) => {
    if (failWrite) {
      failWrite = false;
      throw new Error("cssom write failed");
    }
    originalSet(key, value);
  };
  const colors = {
    resolveTheme() {
      return {
        vars: { "--lab-safe": "#123456" },
        roles: {
          safe: { kind: "color", cssVar: "--lab-safe", hex: "#123456", lc: 100 },
        },
      };
    },
  };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  const exposed = ctrl.refresh();
  assert.throws(() => {
    exposed.roles.late = {
      kind: "failure",
      category: "unreachable",
      code: "floor_unreachable",
      message: "injected",
    };
  }, TypeError);

  failWrite = true;
  assert.throws(() => ctrl.refresh(true), /cssom write failed/u);
  assert.doesNotThrow(() => ctrl.refresh());
  assert.equal(element.props.get("--lab-safe"), "#123456");
  assert.equal(element.props.has("--lab-injected"), false);
});

test("watchTheme rejects a missing engine", () => {
  assert.throws(() => watchTheme(fakeElement("#fff"), { theme: "light", observe: false }), TypeError);
});

test("watchTheme acquires no observer when its initial resolve fails", () => {
  let constructed = 0;
  let observed = 0;
  const MutationObserver = function () {
    constructed++;
    return {
      observe() {
        observed++;
      },
      disconnect() {},
    };
  };
  const win = { MutationObserver, document: { documentElement: {} } };

  assert.throws(
    () =>
      watchTheme(fakeElement("rgb(255,255,255)"), {
        colors: {
          resolveTheme() {
            throw new Error("initial resolve failed");
          },
        },
        theme: "light",
        win,
      }),
    /initial resolve failed/u,
  );
  assert.equal(constructed, 0, "failed construction must not acquire an observer");
  assert.equal(observed, 0, "failed construction must not leave a live observation");
});

test("watchTheme observes its own initial CSS mutation and catches up once", async () => {
  let callback = null;
  let active = false;
  let background = "#FFFFFF";
  const props = new Map();
  const element = {
    style: {
      length: 0,
      item: () => null,
      removeProperty: (key) => props.delete(key),
      setProperty(key, value) {
        props.set(key, value);
        if (key === "--lab-x" && background === "#FFFFFF") {
          background = "#000000";
          if (active) queueMicrotask(callback);
        }
      },
    },
  };
  const colors = fakeEngine();
  const MutationObserver = function (fn) {
    callback = fn;
    return {
      observe() {
        active = true;
      },
      disconnect() {
        active = false;
      },
    };
  };
  const win = { MutationObserver, document: { documentElement: {} } };

  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    win,
  });
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(colors.calls.length, 2, "the initial write-induced background must be re-resolved");
  assert.equal(ctrl.background(), "#000000");
  assert.equal(props.get("--lab-x"), "#000000");
  ctrl.stop();
});

test("watchTheme cleans an observer candidate when observe fails before any write", () => {
  let disconnects = 0;
  let writes = 0;
  const MutationObserver = function () {
    return {
      observe() {
        throw new Error("observe failed");
      },
      disconnect() {
        disconnects++;
      },
    };
  };
  const element = fakeElement("rgb(255,255,255)");
  element.style.setProperty = () => {
    writes++;
  };
  const win = { MutationObserver, document: { documentElement: {} } };

  assert.throws(
    () => watchTheme(element, { colors: fakeEngine(), theme: "light", win }),
    /observe failed/u,
  );
  assert.equal(disconnects, 1, "a partially-acquired observer must be released");
  assert.equal(writes, 0, "observe failure must precede the initial DOM commit");
});

test("watchTheme disconnects and cancels a queued refresh when initial apply fails", async () => {
  let callback = null;
  let active = false;
  let disconnects = 0;
  const colors = fakeEngine();
  const MutationObserver = function (fn) {
    callback = fn;
    return {
      observe() {
        active = true;
      },
      disconnect() {
        active = false;
        disconnects++;
      },
    };
  };
  const element = fakeElement("rgb(255,255,255)");
  element.style.setProperty = () => {
    if (active) queueMicrotask(callback);
    throw new Error("initial apply failed");
  };
  const win = { MutationObserver, document: { documentElement: {} } };

  assert.throws(
    () => watchTheme(element, { colors, theme: "light", win }),
    /initial apply failed/u,
  );
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(active, false);
  assert.equal(disconnects, 1);
  assert.equal(colors.calls.length, 1, "the failed constructor must cancel its queued refresh");
});

test("watchTheme rejects an invalid async error handler before acquiring an observer", () => {
  let constructed = 0;
  const MutationObserver = function () {
    constructed++;
    return { observe() {}, disconnect() {} };
  };
  const win = { MutationObserver, document: { documentElement: {} } };

  assert.throws(
    () =>
      watchTheme(fakeElement("rgb(255,255,255)"), {
        colors: fakeEngine(),
        theme: "light",
        onError: "not-a-function",
        win,
      }),
    /onError/u,
  );
  assert.equal(constructed, 0);
});

test("watchTheme reports one coalesced observer failure without publishing it", async () => {
  let callback = null;
  let background = "#FFFFFF";
  const errors = [];
  const calls = [];
  const failure = new Error("whole-set rejected");
  const colors = {
    resolveTheme(bg, theme) {
      calls.push({ bg, theme });
      if (bg === "#000000") throw failure;
      return { theme, background: bg, vars: { "--lab-x": bg }, roles: {} };
    },
  };
  const MutationObserver = function (fn) {
    callback = fn;
    return { observe() {}, disconnect() {} };
  };
  const win = { MutationObserver, document: { documentElement: {} } };
  const element = fakeElement("rgb(255,255,255)");
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    onError: (error) => errors.push(error),
    win,
  });

  background = "#000000";
  callback();
  callback();
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(calls.length, 2, "one mutation burst must make one failed attempt");
  assert.deepEqual(errors, [failure]);
  assert.equal(ctrl.background(), "#FFFFFF");
  assert.equal(element.props.get("--lab-x"), "#FFFFFF");
  ctrl.stop();
});

test("watchTheme stays active and retries after a transient observer failure", async () => {
  let callback = null;
  let background = "#FFFFFF";
  let failedAttempts = 0;
  const errors = [];
  const colors = {
    resolveTheme(bg, theme) {
      if (bg === "#000000" && failedAttempts++ === 0) {
        throw new Error("transient observer failure");
      }
      return { theme, background: bg, vars: { "--lab-x": bg }, roles: {} };
    },
  };
  const MutationObserver = function (fn) {
    callback = fn;
    return { observe() {}, disconnect() {} };
  };
  const win = { MutationObserver, document: { documentElement: {} } };
  const element = fakeElement("rgb(255,255,255)");
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    onError: (error) => errors.push(error),
    win,
  });

  background = "#000000";
  callback();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(ctrl.background(), "#FFFFFF");
  assert.equal(errors.length, 1);

  callback();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(failedAttempts, 2);
  assert.equal(ctrl.background(), "#000000");
  assert.equal(element.props.get("--lab-x"), "#000000");
  ctrl.stop();
});

test("watchTheme reports observer failure through the host when onError is absent", async () => {
  let callback = null;
  let background = "#FFFFFF";
  const reported = [];
  const failure = new Error("host-reported observer failure");
  const colors = {
    resolveTheme(bg, theme) {
      if (bg === "#000000") throw failure;
      return { theme, background: bg, vars: { "--lab-x": bg }, roles: {} };
    },
  };
  const MutationObserver = function (fn) {
    callback = fn;
    return { observe() {}, disconnect() {} };
  };
  const win = {
    MutationObserver,
    document: { documentElement: {} },
    reportError: (error) => reported.push(error),
  };
  const ctrl = watchTheme(fakeElement("rgb(255,255,255)"), {
    colors,
    theme: "light",
    background: () => background,
    win,
  });

  background = "#000000";
  callback();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(reported, [failure]);
  assert.equal(ctrl.background(), "#FFFFFF");
  ctrl.stop();
});

test("watchTheme explicit refresh throws synchronously and bypasses onError", () => {
  let background = "#FFFFFF";
  let fail = true;
  const errors = [];
  const colors = {
    resolveTheme(bg, theme) {
      if (bg === "#000000" && fail) throw new Error("explicit refresh failed");
      return { theme, background: bg, vars: { "--lab-x": bg }, roles: {} };
    },
  };
  const element = fakeElement("rgb(255,255,255)");
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    onError: (error) => errors.push(error),
    observe: false,
  });

  background = "#000000";
  assert.throws(() => ctrl.refresh(), /explicit refresh failed/u);
  assert.deepEqual(errors, []);
  assert.equal(ctrl.background(), "#FFFFFF");

  fail = false;
  assert.doesNotThrow(() => ctrl.refresh());
  assert.equal(ctrl.background(), "#000000");
});

test("watchTheme retries a dirty canonical write even when its inputs are unchanged", () => {
  const colors = fakeEngine();
  const element = fakeElement("rgb(255,255,255)");
  const write = element.style.setProperty;
  let failWrite = false;
  element.style.setProperty = (key, value) => {
    if (failWrite) {
      element.props.delete(key);
      throw new Error("cssom write failed");
    }
    write(key, value);
  };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });

  failWrite = true;
  assert.throws(() => ctrl.refresh(true), /cssom write failed/u);
  assert.equal(ctrl.background(), "#FFFFFF", "a failed write must not publish a new snapshot");
  assert.equal(element.props.get("--lab-x"), undefined, "CSSOM cannot roll back the partial write");

  failWrite = false;
  ctrl.refresh();
  assert.equal(colors.calls.length, 2, "repair should reuse the committed resolved snapshot");
  assert.equal(element.props.get("--lab-x"), "#FFFFFF");
});

test("watchTheme: stop() cancels a refresh already scheduled by a mutation", async () => {
  const colors = fakeEngine();
  const el = fakeElement("rgb(255,255,255)");
  const errors = [];
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
    onError: (error) => errors.push(error),
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
  assert.deepEqual(errors, [], "a cancelled refresh must not report an error");
});

// ── Reentrancy: stop()/setTheme() изнутри prepare не даёт поздних записей ────

test("watchTheme: reentrant stop() inside background() cancels the outer commit", () => {
  const el = fakeElement("rgb(255,255,255)");
  let w = null;
  let calls = 0;
  const colors = {
    resolveTheme: () => ({ vars: { "--lab-a": "#111111" }, roles: {} }),
  };
  const watcher = watchTheme(el, {
    colors,
    theme: "light",
    background: () => {
      calls++;
      if (calls === 2 && w) w.stop(); // reentrant stop во время refresh
      return calls === 2 ? "#EEEEEE" : "#FFFFFF";
    },
    observe: false,
  });
  w = watcher;
  assert.equal(el.props.get("--lab-a"), "#111111", "initial commit lands");
  el.props.set("--lab-a", "#SENTINEL");
  watcher.refresh(true); // prepare вызовет background() → stop() внутри
  assert.equal(
    el.props.get("--lab-a"),
    "#SENTINEL",
    "после reentrant stop() внешняя транзакция не пишет DOM",
  );
});

test("watchTheme: reentrant setTheme() wins over the stale outer transaction", () => {
  const el = fakeElement("rgb(128,128,128)");
  let w = null;
  let reentered = false;
  const colors = {
    resolveTheme: (bg, theme) => ({
      vars: { "--lab-a": theme === "dark" ? "#000000" : "#FFFFFF" },
      roles: {},
    }),
  };
  const watcher = watchTheme(el, {
    colors,
    theme: "light",
    background: () => {
      if (w === null && !reentered) {
        // Первый (конструкционный) prepare: до создания watcher reentrancy
        // недоступна — вернуть фон как есть.
        return "#808080";
      }
      if (!reentered) {
        reentered = true;
        w.setTheme("dark"); // более новая операция изнутри prepare внешней
      }
      return "#808080";
    },
    observe: false,
  });
  w = watcher;
  // Внешний refresh(true): его prepare перехвачен setTheme("dark") — commit
  // внешнего стейл-кандидата (light) обязан быть отменён, тёмный остаётся.
  watcher.refresh(true);
  assert.equal(
    el.props.get("--lab-a"),
    "#000000",
    "DOM несёт результат новой операции (dark), стейл light не перезаписал её",
  );
  watcher.stop();
});

test("watchTheme: nested setTheme() during CSS write owns DOM and committed cache", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  const resultFor = (theme) => ({
    theme,
    background: "#FFFFFF",
    vars: { "--lab-a": `${theme}-a`, "--lab-b": `${theme}-b` },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
      b: { kind: "color", cssVar: "--lab-b", hex: "#222222", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer-a") {
      armed = false;
      watcher.setTheme("inner");
      watcher.refresh(true);
    }
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        return resultFor(theme);
      },
    },
    theme: "light",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  watcher.setTheme("outer");
  const cached = watcher.refresh();

  assert.deepEqual(
    {
      dom: Object.fromEntries(element.props),
      cachedTheme: cached.theme,
    },
    {
      dom: resultFor("inner").vars,
      cachedTheme: "inner",
    },
    "the stale writer must neither overwrite the newer DOM nor publish its stale cache",
  );
  watcher.stop();
});

test("watchTheme: a before-forward CSS wrapper cannot let the stale writer follow a nested setTheme", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  const resultFor = (theme) => ({
    theme,
    background: "#FFFFFF",
    vars: { "--lab-a": `${theme}-a` },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    if (armed && key === "--lab-a" && value === "outer-a") {
      armed = false;
      watcher.setTheme("inner");
    }
    write(key, value);
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        return resultFor(theme);
      },
    },
    theme: "light",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  watcher.setTheme("outer");
  const cached = watcher.refresh();

  assert.deepEqual(
    { dom: Object.fromEntries(element.props), cachedTheme: cached.theme },
    { dom: resultFor("inner").vars, cachedTheme: "inner" },
    "the newer nested commit must own the full snapshot even when the stale wrapper forwards later",
  );
  watcher.stop();
});

test("watchTheme: stop() during CSS write cannot split the already-started commit", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  const resultFor = (theme) => ({
    theme,
    background: "#FFFFFF",
    vars: { "--lab-a": `${theme}-a`, "--lab-b": `${theme}-b` },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
      b: { kind: "color", cssVar: "--lab-b", hex: "#222222", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer-a") {
      armed = false;
      watcher.stop();
    }
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        return resultFor(theme);
      },
    },
    theme: "light",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  watcher.setTheme("outer");
  const cached = watcher.refresh();

  assert.deepEqual(
    {
      dom: Object.fromEntries(element.props),
      cachedTheme: cached.theme,
    },
    {
      dom: resultFor("outer").vars,
      cachedTheme: "outer",
    },
    "stop cancels future work but cannot tear an in-progress synchronous commit",
  );
});

test("watchTheme: a failed queued operation cannot leave an older tail to overwrite a later intent", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  const resultFor = (theme) => ({
    theme,
    vars: { "--lab-a": theme },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer") {
      armed = false;
      watcher.setTheme("bad");
      watcher.setTheme("stale");
    }
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        if (theme === "bad") throw new Error("queued operation failed");
        return resultFor(theme);
      },
    },
    theme: "initial",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  assert.throws(() => watcher.setTheme("outer"), /queued operation failed/u);
  watcher.setTheme("newer");

  assert.equal(element.props.get("--lab-a"), "newer");
  assert.equal(watcher.refresh().theme, "newer");
  watcher.stop();
});

test("watchTheme: the newest intent raised during queued prepare follows the older FIFO suffix", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  let reenter = true;
  const resultFor = (theme) => ({
    theme,
    vars: { "--lab-a": theme },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer") {
      armed = false;
      watcher.setTheme("A");
      watcher.setTheme("C");
    }
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        if (theme === "A" && reenter) {
          reenter = false;
          watcher.setTheme("B");
        }
        return resultFor(theme);
      },
    },
    theme: "initial",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  watcher.setTheme("outer");

  assert.equal(element.props.get("--lab-a"), "B");
  assert.equal(watcher.refresh().theme, "B");
  watcher.stop();
});

test("watchTheme: refresh returns the result committed after nested serialized work", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  let background = "initial";
  const resultFor = (theme) => ({
    theme,
    vars: { "--lab-a": theme },
    roles: {
      a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
    },
  });
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer") {
      armed = false;
      watcher.setTheme("inner");
    }
  };
  watcher = watchTheme(element, {
    colors: { resolveTheme: (_background, theme) => resultFor(theme) },
    theme: "outer",
    background: () => background,
    target: element,
    observe: false,
  });

  armed = true;
  background = "changed";
  const returned = watcher.refresh();

  assert.equal(element.props.get("--lab-a"), "inner");
  assert.equal(returned.theme, "inner");
  watcher.stop();
});

test("watchTheme: a queued failure cannot mask the primary CSSOM failure", () => {
  const element = observedElement();
  let watcher = null;
  let armed = false;
  const write = element.style.setProperty.bind(element.style);
  element.style.setProperty = (key, value) => {
    write(key, value);
    if (armed && key === "--lab-a" && value === "outer") {
      armed = false;
      watcher.setTheme("bad");
      throw new Error("primary CSSOM failure");
    }
  };
  watcher = watchTheme(element, {
    colors: {
      resolveTheme(_background, theme) {
        if (theme === "bad") throw new Error("queued secondary failure");
        return {
          vars: { "--lab-a": theme },
          roles: {
            a: { kind: "color", cssVar: "--lab-a", hex: "#111111", lc: 100 },
          },
        };
      },
    },
    theme: "initial",
    background: "#FFFFFF",
    target: element,
    observe: false,
  });

  armed = true;
  let caught = null;
  try {
    watcher.setTheme("outer");
  } catch (error) {
    caught = error;
  }
  assert.ok(caught, "the public operation must report its failed commit");
  const visible = caught instanceof AggregateError ? caught.errors : [caught];
  assert.ok(
    visible.some((error) => error?.message === "primary CSSOM failure"),
    "the queued failure must not replace the primary commit failure",
  );
  watcher.stop();
});
