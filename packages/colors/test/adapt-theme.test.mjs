// Behaviour tests for the adaptive debounced-re-solve controller, driven through injected
// fakes (a fake engine, a fake element, an injected clock and background) so the
// control law runs under plain `node --test` with no browser, no rAF, no WASM.

import { test } from "node:test";
import assert from "node:assert/strict";

import { adaptTheme } from "../adapt-theme.js";

// A fake LabColors engine. `resolveTheme` returns a controllable role set;
// `recheckContrast` returns controllable signed Lc per role (interleaved with a
// dummy wcag). Records call counts.
function fakeColors(initial) {
  let resolveCount = 0;
  let resolve = initial;
  let lastResolveBg = null;
  let recheckByBg = null; // optional: per-sample { bgHex: lc[] }
  let recheckLc = initial.roles
    ? Object.values(initial.roles)
        .filter((r) => r.kind === "color")
        .map((r) => r.lc)
    : [];
  return {
    resolveCount: () => resolveCount,
    lastResolveBg: () => lastResolveBg,
    setResolve(r) {
      resolve = r;
    },
    setRecheckLc(lcs) {
      recheckLc = lcs;
      recheckByBg = null;
    },
    // Drive recheck per background sample, so worst-case logic can be tested.
    setRecheckByBg(map) {
      recheckByBg = map;
    },
    resolveTheme(bg) {
      resolveCount++;
      lastResolveBg = bg;
      return resolve;
    },
    recheckContrast(bg) {
      const lcs = recheckByBg ? (recheckByBg[bg] ?? recheckLc) : recheckLc;
      const out = [];
      for (const lc of lcs) {
        out.push(lc);
        out.push(10);
      }
      return out;
    },
  };
}

function fakeElement() {
  const props = new Map();
  return {
    props,
    style: {
      get length() {
        return props.size;
      },
      item: (i) => [...props.keys()][i] ?? null,
      setProperty: (k, v) => props.set(k, v),
      removeProperty: (k) => props.delete(k),
    },
  };
}

const oneRole = (hex, lc) => ({
  vars: { "--lab-label-primary": hex },
  roles: { "label-primary": { kind: "color", cssVar: "--lab-label-primary", hex, lc } },
});

// A role set that carries an explicit `legalFloor` (4.5 / 3.0 / null), the field
// the strict floor-clamp reads.
const floorRole = (hex, lc, legalFloor) => ({
  vars: { "--lab-label-primary": hex },
  roles: {
    "label-primary": { kind: "color", cssVar: "--lab-label-primary", hex, lc, legalFloor },
  },
});

// Contrast ratio between two #RRGGBB in the frozen original WCAG 2.1 (2018)
// profile (0.03928 split, 2.4 exponent), matching the versioned core contract.
function wcagContrast(fg, bg) {
  const lum = (hex) => {
    const n = parseInt(hex.slice(1), 16);
    const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
      const s = c / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
  };
  const a = lum(fg);
  const b = lum(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function harness(opts = {}) {
  const colors = fakeColors(oneRole("#000000", 100));
  const el = fakeElement();
  let bg = "#FFFFFF";
  let now = 1000;
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => now,
    win: {}, // no rAF/matchMedia
    easeMs: 100,
    sustainMs: 120,
    dwellMs: 250,
    dropFraction: 0.2,
    ...opts,
  });
  return {
    ctrl,
    colors,
    el,
    setBg: (b) => (bg = b),
    setNow: (n) => (now = n),
    advance: (ms) => (now += ms),
  };
}

test("applies the resolved set immediately on creation", () => {
  const h = harness();
  assert.equal(h.colors.resolveCount(), 1);
  assert.equal(h.el.props.get("--lab-label-primary"), "#000000");
});

test("current() reports the logical target, not the painted mid-ease value", () => {
  const h = harness();
  h.colors.setResolve(oneRole("#FFFFFF", 100));
  h.colors.setRecheckLc([0]);
  h.setBg("#EEEEEE");

  h.ctrl.tick(1000); // begin the sustained breach window
  h.ctrl.tick(1300); // resolve and start the ease at t=0

  assert.equal(h.el.props.get("--lab-label-primary"), "#000000");
  assert.equal(h.ctrl.current()["--lab-label-primary"], "#FFFFFF");
});

test("a failed worst-sample resolve leaves DOM and logical state unchanged", () => {
  const el = fakeElement();
  let samples = ["#FFFFFF", "#000000"];
  const recheckThemes = [];
  const colors = {
    resolveTheme(bg, theme) {
      if (theme === "dark" && bg === "#000000") {
        throw new Error("rejected: invalid_input");
      }
      return oneRole(theme === "dark" ? "#FFFFFF" : "#111111", 100);
    },
    recheckContrast(bg, _foregrounds, theme) {
      recheckThemes.push(theme);
      return [bg === "#000000" ? 0 : 100, 10];
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => samples,
    target: el,
    now: () => 1000,
    win: {},
  });
  const beforeDom = new Map(el.props);
  const beforeCurrent = ctrl.current();

  assert.throws(() => ctrl.setTheme("dark"), /rejected: invalid_input/);
  assert.deepEqual(el.props, beforeDom, "failed resolve must not partially repaint");
  assert.deepEqual(
    ctrl.current(),
    beforeCurrent,
    "failed resolve must not publish a provisional logical target",
  );

  samples = ["#FEFEFE", "#000000"];
  ctrl.tick();
  assert.equal(recheckThemes.at(-1), "light", "failed setTheme must not publish hidden intent");
});

test("a failed stable-Glow reconciliation does not publish its candidate", () => {
  const el = fakeElement();
  const cssVar = "--lab-fx";
  const stable = {
    vars: {
      [cssVar]: "oklch(70% 0.1 280)",
      [`${cssVar}-core`]: "oklch(80% 0.1 280)",
      [`${cssVar}-alpha`]: "0.5",
    },
    roles: {
      fx: {
        kind: "glow",
        cssVar,
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        decisionGuarantee: { kind: "bit-exact" },
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: null,
        constraintLayer: "halo",
        targetStatus: "exact-noop-unreachable",
        degraded: true,
      },
    },
  };
  const colors = {
    resolveTheme(_bg, theme) {
      if (theme !== "dark") return oneRole("#111111", 100);
      return stable;
    },
    recheckContrast() {
      return [];
    },
    isStableGlowPointNoop() {
      throw new Error("internal_error: certificate recheck failed");
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: "#FFFFFF",
    target: el,
    now: () => 1000,
    win: {},
  });
  const beforeDom = new Map(el.props);
  const beforeCurrent = ctrl.current();

  assert.throws(
    () => ctrl.setTheme("dark"),
    /internal_error: certificate recheck failed/,
  );
  assert.deepEqual(el.props, beforeDom);
  assert.deepEqual(ctrl.current(), beforeCurrent);
});

test("tick does not commit a Glow class change before a later set resolve succeeds", () => {
  const el = fakeElement();
  let samples = ["#FFFFFF"];
  let now = 0;
  let blackResolves = 0;
  const cssVar = "--lab-fx";
  const colorRole = {
    kind: "color",
    cssVar: "--lab-label",
    hex: "#111111",
    lc: 100,
  };
  const determinate = {
    vars: {
      "--lab-label": "#111111",
      [cssVar]: "oklch(70% 0.1 280)",
      [`${cssVar}-core`]: "oklch(80% 0.1 280)",
      [`${cssVar}-alpha`]: "0.5",
    },
    roles: {
      label: colorRole,
      fx: {
        kind: "glow",
        cssVar,
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        decisionGuarantee: { kind: "bit-exact" },
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: null,
        constraintLayer: "halo",
        targetStatus: "exact-noop-unreachable",
        degraded: true,
      },
    },
  };
  const colors = {
    resolveTheme(background) {
      if (background === "#000000") {
        blackResolves++;
        return determinate;
      }
      if (blackResolves > 0) throw new Error("rejected: invalid_input");
      return determinate;
    },
    recheckContrast(background) {
      return [background === "#000000" ? 0 : 100, 1];
    },
    isStableGlowPointNoop(_source, background) {
      return background === "#FFFFFF";
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => samples,
    target: el,
    now: () => now,
    win: {},
    sustainMs: 0,
    dwellMs: 0,
    easeMs: 0,
  });
  const beforeDom = new Map(el.props);
  const beforeCurrent = ctrl.current();

  samples = ["#FFFFFF", "#000000"];
  now = 1;
  assert.throws(() => ctrl.tick(), /rejected: invalid_input/);
  assert.equal(blackResolves, 1, "the worst-sample candidate is prepared before failure");
  assert.deepEqual(el.props, beforeDom);
  assert.deepEqual(ctrl.current(), beforeCurrent);
});

test("an in-flight ease does not advance before the tick's fallible checks succeed", () => {
  const el = fakeElement();
  let bg = "#FFFFFF";
  let now = 0;
  let resolves = 0;
  let rechecks = 0;
  const colors = {
    resolveTheme() {
      resolves++;
      return oneRole(resolves === 1 ? "#000000" : "#FFFFFF", 100);
    },
    recheckContrast() {
      rechecks++;
      if (rechecks === 1) return [0, 1];
      throw new Error("internal_error: recheck failed");
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => now,
    win: {},
    sustainMs: 0,
    dwellMs: 0,
    easeMs: 100,
  });

  bg = "#000000";
  now = 1;
  ctrl.tick();
  const beforeDom = new Map(el.props);
  const beforeCurrent = ctrl.current();
  now = 51;

  assert.throws(() => ctrl.tick(), /recheck failed/);
  assert.deepEqual(el.props, beforeDom);
  assert.deepEqual(ctrl.current(), beforeCurrent);
});

test("a transient recheck failure retries the same changed sample", () => {
  const el = fakeElement();
  let bg = "#FFFFFF";
  let rechecks = 0;
  const colors = {
    resolveTheme() {
      return oneRole("#111111", 100);
    },
    recheckContrast() {
      rechecks++;
      if (rechecks === 1) throw new Error("internal_error: transient recheck");
      return [100, 1];
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => 1,
    win: {},
  });

  bg = "#FEFEFE";
  assert.throws(() => ctrl.tick(), /transient recheck/);
  ctrl.tick();
  assert.equal(rechecks, 2, "failed evidence must not mark the sample as processed");
});

test("the internal frame loop can restart after a transient tick failure", () => {
  const el = fakeElement();
  let bg = "#FFFFFF";
  let rechecks = 0;
  let nextFrameId = 1;
  const frames = [];
  const win = {
    requestAnimationFrame(callback) {
      frames.push(callback);
      return nextFrameId++;
    },
    cancelAnimationFrame() {},
  };
  const colors = {
    resolveTheme() {
      return oneRole("#111111", 100);
    },
    recheckContrast() {
      rechecks++;
      if (rechecks === 1) throw new Error("internal_error: transient recheck");
      return [100, 1];
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => 1,
    win,
  });

  bg = "#FEFEFE";
  ctrl.start();
  assert.equal(frames.length, 1);
  assert.throws(() => frames.shift()(), /transient recheck/);
  ctrl.start();
  assert.equal(frames.length, 1, "start must enqueue a new frame after fail-stop");
  frames.shift()();
  assert.equal(rechecks, 2);
  ctrl.stop();
});

test("the internal frame loop can restart after the host rejects a requeue", () => {
  const el = fakeElement();
  let requests = 0;
  let nextFrameId = 1;
  const frames = [];
  const win = {
    requestAnimationFrame(callback) {
      requests++;
      if (requests === 2) throw new Error("host frame queue unavailable");
      frames.push(callback);
      return nextFrameId++;
    },
    cancelAnimationFrame() {},
  };
  const ctrl = adaptTheme(el, {
    colors: fakeColors(oneRole("#111111", 100)),
    theme: "light",
    background: "#FFFFFF",
    target: el,
    now: () => 1,
    win,
  });

  ctrl.start();
  assert.throws(() => frames.shift()(), /host frame queue unavailable/);
  ctrl.start();
  assert.equal(frames.length, 1, "start must recover after a failed host requeue");
  ctrl.stop();
});

test("a reentrant stop and start inside tick owns exactly one next frame", () => {
  const el = fakeElement();
  const frames = [];
  let nextFrameId = 1;
  let reenter = false;
  let ctrl;
  const win = {
    requestAnimationFrame(callback) {
      const frame = { id: nextFrameId++, callback };
      frames.push(frame);
      return frame.id;
    },
    cancelAnimationFrame(id) {
      const index = frames.findIndex((frame) => frame.id === id);
      if (index >= 0) frames.splice(index, 1);
    },
  };
  ctrl = adaptTheme(el, {
    colors: fakeColors(oneRole("#111111", 100)),
    theme: "light",
    background() {
      if (reenter) {
        reenter = false;
        ctrl.stop();
        ctrl.start();
      }
      return "#FFFFFF";
    },
    target: el,
    now: () => 1,
    win,
  });

  ctrl.start();
  const current = frames.shift();
  reenter = true;
  current.callback();
  assert.equal(frames.length, 1, "the restarted epoch must own exactly one frame");
  ctrl.stop();
});

test("a stale frame surviving a failed cancel cannot capture a restarted loop", () => {
  const el = fakeElement();
  const frames = [];
  let nextFrameId = 1;
  let failCancel = true;
  const win = {
    requestAnimationFrame(callback) {
      const frame = { id: nextFrameId++, callback };
      frames.push(frame);
      return frame.id;
    },
    cancelAnimationFrame() {
      if (failCancel) {
        failCancel = false;
        throw new Error("host cancel failed");
      }
    },
  };
  const ctrl = adaptTheme(el, {
    colors: fakeColors(oneRole("#111111", 100)),
    theme: "light",
    background: "#FFFFFF",
    target: el,
    now: () => 1,
    win,
  });

  ctrl.start();
  assert.throws(() => ctrl.stop(), /host cancel failed/);
  ctrl.start();
  const stale = frames.shift();
  stale.callback();
  assert.equal(frames.length, 1, "a stale callback must not schedule beside the new epoch");
  ctrl.stop();
});

test("stop and restart preserve an in-flight ease until its canonical target", () => {
  const frames = [];
  const h = harness({
    sustainMs: 0,
    dwellMs: 0,
    easeMs: 100,
    win: {
      requestAnimationFrame(callback) {
        frames.push(callback);
        return frames.length;
      },
      cancelAnimationFrame() {},
    },
  });
  h.colors.setResolve(oneRole("#FFFFFF", 100));
  h.colors.setRecheckLc([0]);
  h.setBg("#000000");
  h.setNow(1001);
  h.ctrl.tick();

  h.colors.setRecheckLc([100]);
  h.setNow(1051);
  h.ctrl.tick();
  const midpoint = h.el.props.get("--lab-label-primary");
  assert.notEqual(midpoint, "#000000");
  assert.notEqual(midpoint, "#FFFFFF");

  h.ctrl.stop();
  h.setNow(1151);
  h.ctrl.start();
  frames.shift()();
  assert.equal(h.el.props.get("--lab-label-primary"), "#FFFFFF");
  assert.equal(h.ctrl.current()["--lab-label-primary"], "#FFFFFF");
  h.ctrl.stop();
});

test("a later tick repairs the canonical DOM after a CSSOM write failure", () => {
  const props = new Map();
  let rejectNextWrite = false;
  const el = {
    props,
    style: {
      get length() {
        return props.size;
      },
      item: (index) => [...props.keys()][index] ?? null,
      setProperty(key, value) {
        if (rejectNextWrite) {
          rejectNextWrite = false;
          throw new Error("host CSSOM write failed");
        }
        props.set(key, value);
      },
      removeProperty: (key) => props.delete(key),
    },
  };
  const colors = {
    resolveTheme(_background, theme) {
      return oneRole(theme === "dark" ? "#FFFFFF" : "#111111", 100);
    },
    recheckContrast() {
      return [100, 1];
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: "#FFFFFF",
    target: el,
    now: () => 1,
    win: {},
  });

  rejectNextWrite = true;
  assert.throws(() => ctrl.setTheme("dark"), /host CSSOM write failed/);
  assert.equal(el.props.has("--lab-label-primary"), false, "the host failed after clear");
  assert.equal(ctrl.current()["--lab-label-primary"], "#FFFFFF");

  ctrl.tick();
  assert.equal(el.props.get("--lab-label-primary"), "#FFFFFF");
});

test("stable Glow class changes re-resolve and clear/restore satellites synchronously", () => {
  const el = fakeElement();
  let bg = "#FFFFFF";
  let resolveCount = 0;
  const cssVar = "--lab-fx";
  const determinate = {
    vars: {
      [cssVar]: "oklch(70% 0.1 280)",
      [`${cssVar}-core`]: "oklch(80% 0.1 280)",
      [`${cssVar}-alpha`]: "0.5",
    },
    roles: {
      fx: {
        kind: "glow",
        cssVar,
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        decisionGuarantee: { kind: "bit-exact" },
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: null,
        constraintLayer: "halo",
        targetStatus: "exact-noop-unreachable",
        degraded: true,
      },
    },
  };
  const indeterminate = {
    vars: {},
    roles: {
      fx: {
        kind: "glow-indeterminate",
        cssVar,
        sourceHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        numericalSiteId: "glow-target-or-maximum-v1",
        constraintLayer: "halo",
        reason: "sound-bound-unavailable",
        bounds: { kind: "unavailable" },
      },
    },
  };
  const colors = {
    resolveTheme(background) {
      resolveCount++;
      return background === "#FFFFFF" ? determinate : indeterminate;
    },
    recheckContrast() {
      return [];
    },
    isStableGlowPointNoop(_source, background) {
      return background === "#FFFFFF";
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => 1000,
    win: {},
    sustainMs: 10_000,
    dwellMs: 10_000,
  });
  assert.equal(resolveCount, 1);
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.ok(el.props.has(key), `${key} present for exact no-op`);
  }

  bg = "#FEFEFE";
  ctrl.tick();
  assert.equal(resolveCount, 2, "class transition bypasses contrast sustain/dwell");
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.equal(el.props.has(key), false, `${key} cleared in the same tick`);
  }

  bg = "#FDFDFD";
  ctrl.tick();
  assert.equal(resolveCount, 2, "same Indeterminate class keeps the cheap adaptive path");

  bg = "#FFFFFF";
  ctrl.tick();
  assert.equal(resolveCount, 3, "return to exact no-op selectively re-resolves");
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.ok(el.props.has(key), `${key} restored synchronously`);
  }
});

test("stable Glow rejects a missing exact recheck capability or malformed evidence", () => {
  const stable = {
    vars: {
      "--lab-fx": "oklch(70% 0.1 280)",
      "--lab-fx-core": "oklch(80% 0.1 280)",
      "--lab-fx-alpha": "0.5",
    },
    roles: {
      fx: {
        kind: "glow",
        cssVar: "--lab-fx",
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        decisionGuarantee: { kind: "bit-exact" },
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: null,
        constraintLayer: "halo",
        targetStatus: "exact-noop-unreachable",
        degraded: true,
      },
    },
  };
  const options = {
    theme: "light",
    background: () => "#FFFFFF",
    win: {},
  };
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: { resolveTheme: () => stable, recheckContrast: () => [] },
      }),
    /isStableGlowPointNoop/u,
  );

  const malformed = structuredClone(stable);
  malformed.roles.fx.decisionGuarantee = { kind: "legacy-platform-dependent-v1" };
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => malformed,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => true,
        },
      }),
    /lacks BitExact evidence/u,
  );

  const falseSelectionDiagnostic = structuredClone(stable);
  falseSelectionDiagnostic.roles.fx.selectionDiagnosticProfile =
    "cam16-ucs-jprime-li2017-v1";
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => falseSelectionDiagnostic,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => true,
        },
      }),
    /lacks BitExact evidence/u,
  );

  for (const field of [
    "layerRecipeProfile",
    "appearanceDiagnosticProfile",
    "selectionDiagnosticProfile",
  ]) {
    const missingProfile = structuredClone(stable);
    delete missingProfile.roles.fx[field];
    assert.throws(
      () =>
        adaptTheme(fakeElement(), {
          ...options,
          colors: {
            resolveTheme: () => missingProfile,
            recheckContrast: () => [],
            isStableGlowPointNoop: () => true,
          },
        }),
      /lacks BitExact evidence/u,
      `${field} must not collapse at the adaptive boundary`,
    );
  }

  const genericStatus = structuredClone(stable);
  genericStatus.roles.fx.targetStatus = "unreachable";
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => genericStatus,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => true,
        },
      }),
    /lacks BitExact evidence/u,
    "generic unreachable must not erase exact provenance",
  );

  const falseDegraded = structuredClone(stable);
  falseDegraded.roles.fx.degraded = false;
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => falseDegraded,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => true,
        },
      }),
    /lacks BitExact evidence/u,
  );

  const incomplete = structuredClone(stable);
  delete incomplete.vars["--lab-fx-alpha"];
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => incomplete,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => true,
        },
      }),
    /lacks BitExact evidence/u,
  );

  const wrongSite = {
    vars: {},
    roles: {
      fx: {
        kind: "glow-indeterminate",
        cssVar: "--lab-fx",
        sourceHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        numericalSiteId: "some-future-site",
        constraintLayer: "halo",
        reason: "sound-bound-unavailable",
        bounds: { kind: "unavailable" },
      },
    },
  };
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => wrongSite,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => false,
        },
      }),
    /lacks lawful Indeterminate evidence/u,
  );

  const mismatchedCssVar = structuredClone(wrongSite);
  mismatchedCssVar.roles.fx.numericalSiteId = "glow-target-or-maximum-v1";
  mismatchedCssVar.roles.fx.cssVar = "--lab-other";
  mismatchedCssVar.vars = { "--lab-fx": "forbidden-fallback" };
  const mismatchTarget = fakeElement();
  assert.throws(
    () =>
      adaptTheme(mismatchTarget, {
        ...options,
        colors: {
          resolveTheme: () => mismatchedCssVar,
          recheckContrast: () => [],
          isStableGlowPointNoop: () => false,
        },
      }),
    /non-canonical cssVar/u,
  );
  assert.equal(
    mismatchTarget.props.size,
    0,
    "mismatched cssVar must not smuggle a fallback var into the target",
  );

  for (const decisionProfile of [undefined, "stable-v2", "stable-v1-typo"]) {
    const unknownProfile = structuredClone(stable);
    if (decisionProfile === undefined) delete unknownProfile.roles.fx.decisionProfile;
    else unknownProfile.roles.fx.decisionProfile = decisionProfile;
    assert.throws(
      () =>
        adaptTheme(fakeElement(), {
          ...options,
          colors: {
            resolveTheme: () => unknownProfile,
            recheckContrast: () => [],
            isStableGlowPointNoop: () => true,
          },
        }),
      /lacks an explicit known decisionProfile/u,
      `unknown profile ${String(decisionProfile)} must not retain un-rechecked Glow vars`,
    );
  }

  const legacyIndeterminate = structuredClone(wrongSite);
  legacyIndeterminate.roles.fx.decisionProfile = "legacy-platform-dependent-v1";
  legacyIndeterminate.roles.fx.numericalSiteId = "glow-target-or-maximum-v1";
  assert.throws(
    () =>
      adaptTheme(fakeElement(), {
        ...options,
        colors: {
          resolveTheme: () => legacyIndeterminate,
          recheckContrast: () => [],
        },
      }),
    /legacy Glow 'fx' cannot be Indeterminate/u,
  );
});

test("legacy Glow validates correlated provenance before adopting external results", () => {
  const cssVar = "--lab-fx";
  const legacyReached = {
    vars: {
      [cssVar]: "oklch(70% 0.1 280)",
      [`${cssVar}-core`]: "oklch(80% 0.1 280)",
      [`${cssVar}-alpha`]: "0.5",
    },
    roles: {
      fx: {
        kind: "glow",
        cssVar,
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        alpha: 0.5,
        alphaCss: "0.5",
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        decisionProfile: "legacy-platform-dependent-v1",
        decisionGuarantee: { kind: "legacy-platform-dependent-v1" },
        constraintLayer: "halo",
        targetDj: 2.3006,
        targetStatus: "legacy-reached",
        haloCompositeHex: "#C0B2FA",
        haloAchievedDj: 2.4,
        coreCompositeHex: "#D8CEFF",
        coreAchievedDj: 3.1,
        achievedDj: 2.4,
        degraded: false,
        css: "oklch(70% 0.1 280)",
      },
    },
  };
  const options = {
    theme: "light",
    background: () => "#FFFFFF",
    win: {},
  };
  const colorsFor = (result) => ({
    resolveTheme: () => result,
    recheckContrast: () => [],
  });

  const accepted = fakeElement();
  const controller = adaptTheme(accepted, {
    ...options,
    colors: colorsFor(legacyReached),
  });
  assert.equal(accepted.props.get(cssVar), legacyReached.vars[cssVar]);
  controller.stop();

  const legacyUnreachable = structuredClone(legacyReached);
  legacyUnreachable.roles.fx.targetStatus = "legacy-unreachable";
  legacyUnreachable.roles.fx.degraded = true;
  const acceptedUnreachable = fakeElement();
  adaptTheme(acceptedUnreachable, {
    ...options,
    colors: colorsFor(legacyUnreachable),
  }).stop();
  assert.equal(
    acceptedUnreachable.props.get(cssVar),
    legacyUnreachable.vars[cssVar],
  );

  const malformedCases = [
    ["bit-exact legacy decision", (role) => (role.decisionGuarantee = { kind: "bit-exact" })],
    ["null legacy selection", (role) => (role.selectionDiagnosticProfile = null)],
    ["generic exact status", (role) => (role.targetStatus = "exact-noop-unreachable")],
    ["reached marked degraded", (role) => (role.degraded = true)],
    ["unknown recipe", (role) => (role.layerRecipeProfile = "future-recipe-v2")],
    ["unknown composite", (role) => (role.compositeProfile = "browser-screen-v1")],
  ];
  for (const [name, mutate] of malformedCases) {
    const malformed = structuredClone(legacyReached);
    mutate(malformed.roles.fx);
    const target = fakeElement();
    assert.throws(
      () =>
        adaptTheme(target, {
          ...options,
          colors: colorsFor(malformed),
        }),
      /legacy Glow 'fx' lacks lawful legacy evidence/u,
      name,
    );
    assert.equal(target.props.size, 0, `${name}: invalid result must not be applied`);
  }
});

test("stable Glow invalidation does not snap an in-flight color ease", () => {
  const el = fakeElement();
  let bg = "#FFFFFF";
  let now = 1000;
  let labelHex = "#000000";
  const cssVar = "--lab-fx";
  const isWhite = (value) => value.replace(/^#/u, "").toUpperCase() === "FFFFFF";
  const colors = {
    resolveTheme(background) {
      const noop = isWhite(background);
      return {
        vars: {
          "--lab-label": labelHex,
          ...(noop
            ? {
                [cssVar]: "oklch(70% 0.1 280)",
                [`${cssVar}-core`]: "oklch(80% 0.1 280)",
                [`${cssVar}-alpha`]: "0.5",
              }
            : {}),
        },
        roles: {
          label: {
            kind: "color",
            cssVar: "--lab-label",
            hex: labelHex,
            lc: 100,
          },
          fx: noop
            ? {
                kind: "glow",
                cssVar,
                coreHex: "#D8CEFF",
                haloHex: "#C0B2FA",
                decisionProfile: "stable-v1",
                decisionGuarantee: { kind: "bit-exact" },
                compositeProfile: "encoded-srgb8-screen-v1",
                compositeGuarantee: "bit-exact",
                layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
                appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
                selectionDiagnosticProfile: null,
                constraintLayer: "halo",
                targetStatus: "exact-noop-unreachable",
                degraded: true,
              }
            : {
                kind: "glow-indeterminate",
                cssVar,
                sourceHex: "#C0B2FA",
                decisionProfile: "stable-v1",
                numericalSiteId: "glow-target-or-maximum-v1",
                constraintLayer: "halo",
                reason: "sound-bound-unavailable",
                bounds: { kind: "unavailable" },
              },
        },
      };
    },
    recheckContrast() {
      return [10, 10];
    },
    isStableGlowPointNoop(_source, background) {
      return isWhite(background);
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => bg,
    target: el,
    now: () => now,
    win: {},
    easeMs: 100,
    sustainMs: 120,
    dwellMs: 250,
  });

  bg = "FFFFFF";
  ctrl.tick(); // arm contrast breach while stable Glow remains exact no-op
  labelHex = "#F0F0F0";
  now = 1300;
  bg = "#fff";
  ctrl.tick(); // sustained color breach -> begin ease

  now = 1350;
  bg = "#FEFEFE";
  ctrl.tick(); // paint midpoint, then synchronously invalidate only Glow
  const painted = el.props.get("--lab-label");
  assert.match(painted, /^#[0-9A-Fa-f]{6}$/u);
  assert.notEqual(painted, "#000000");
  assert.notEqual(painted, "#F0F0F0", "stable invalidation must preserve painted midpoint");
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.equal(el.props.has(key), false, `${key} cleared without snapping label`);
  }
});

test("a color re-solve cannot reintroduce stable Glow vars unsafe for another sample", () => {
  const el = fakeElement();
  let samples = ["#FFFFFF", "#FEFEFE"];
  let now = 1000;
  let failing = false;
  const cssVar = "--lab-fx";
  const colors = {
    resolveTheme(background) {
      const noop = background === "#FFFFFF";
      return {
        vars: {
          "--lab-label": "#000000",
          ...(noop
            ? {
                [cssVar]: "oklch(70% 0.1 280)",
                [`${cssVar}-core`]: "oklch(80% 0.1 280)",
                [`${cssVar}-alpha`]: "0.5",
              }
            : {}),
        },
        roles: {
          label: {
            kind: "color",
            cssVar: "--lab-label",
            hex: "#000000",
            lc: 100,
          },
          fx: noop
            ? {
                kind: "glow",
                cssVar,
                coreHex: "#D8CEFF",
                haloHex: "#C0B2FA",
                decisionProfile: "stable-v1",
                decisionGuarantee: { kind: "bit-exact" },
                compositeProfile: "encoded-srgb8-screen-v1",
                compositeGuarantee: "bit-exact",
                layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
                appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
                selectionDiagnosticProfile: null,
                constraintLayer: "halo",
                targetStatus: "exact-noop-unreachable",
                degraded: true,
              }
            : {
                kind: "glow-indeterminate",
                cssVar,
                sourceHex: "#C0B2FA",
                decisionProfile: "stable-v1",
                numericalSiteId: "glow-target-or-maximum-v1",
                constraintLayer: "halo",
                reason: "sound-bound-unavailable",
                bounds: { kind: "unavailable" },
              },
        },
      };
    },
    recheckContrast(background) {
      return [failing && background === "#FFFFFF" ? 10 : 100, 10];
    },
    isStableGlowPointNoop(_source, background) {
      return background === "#FFFFFF";
    },
  };
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => samples,
    target: el,
    now: () => now,
    win: {},
    sustainMs: 120,
    dwellMs: 250,
    easeMs: 100,
  });
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.equal(el.props.has(key), false, `${key} unsafe for the second initial sample`);
  }

  samples = ["#FFFFFF", "#FDFDFD"];
  failing = true;
  ctrl.tick(); // arm color breach; stable class remains aggregate Indeterminate
  now = 1300;
  ctrl.tick(); // color worstIdx=0 (white) -> must reconcile second sample again
  for (const key of [cssVar, `${cssVar}-core`, `${cssVar}-alpha`]) {
    assert.equal(el.props.has(key), false, `${key} must not reappear after color re-solve`);
  }
});

test("holds while returned Lc stays above the relative-drop trigger", () => {
  const h = harness();
  // A small drop (95 of 100; trigger is 80) does not request a new solve.
  h.colors.setRecheckLc([95]);
  h.setBg("#FEFEFE");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1, "must not re-solve while passing");
  // Even a drop to exactly the threshold (80) is not a breach.
  h.colors.setRecheckLc([80]);
  h.setBg("#FDFDFD");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1);
});

test("debounce: a transient breach shorter than sustainMs does not re-solve", () => {
  const h = harness();
  h.colors.setRecheckLc([10]); // far below threshold → breach
  h.setBg("#222222");
  h.ctrl.tick(); // arms breachSince
  assert.equal(h.colors.resolveCount(), 1);
  h.advance(50); // < sustainMs (120)
  h.setBg("#232323");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1, "transient breach must not trigger");
  // Breach clears before sustain → no re-solve.
  h.colors.setRecheckLc([100]);
  h.advance(50);
  h.setBg("#FFFFFF");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1);
});

test("sustained breach re-solves and eases to the fresh colours", () => {
  const h = harness();
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick(); // breachSince = 1000
  assert.equal(h.colors.resolveCount(), 1);
  // The re-solve will hand back a fresh (light) colour for the dark bg.
  h.colors.setResolve(oneRole("#F0F0F0", 100));
  h.advance(130); // past sustainMs (120), past dwell vs lastSolveAt(1000)? now 1130 - 1000 = 130 < 250
  // dwell not yet satisfied → still no re-solve.
  h.setBg("#202021");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1, "dwell gate holds");
  // Advance past dwell.
  h.setNow(1300); // 1300 - lastSolveAt(1000) = 300 >= 250; breach age 300 >= sustain
  h.setBg("#202022");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 2, "sustained breach past dwell re-solves");
  // Mid-ease: the applied colour is between the old (#000000) and new (#F0F0F0).
  h.setNow(1300 + 50); // half of easeMs=100
  h.setBg("#202023");
  h.ctrl.tick();
  const mid = h.el.props.get("--lab-label-primary");
  assert.notEqual(mid, "#000000");
  assert.notEqual(mid, "#F0F0F0");
  // After easeMs the colour settles exactly on the fresh target.
  h.setNow(1300 + 120);
  h.colors.setRecheckLc([100]); // new colour passes
  h.setBg("#202024");
  h.ctrl.tick();
  assert.equal(h.el.props.get("--lab-label-primary"), "#F0F0F0");
});

test("setTheme is instant — a deliberate intent, never eased", () => {
  const h = harness();
  h.colors.setResolve(oneRole("#FFFFFF", 100));
  h.ctrl.setTheme("dark");
  assert.equal(h.colors.resolveCount(), 2);
  // Applied immediately to the fresh colour, no interpolation.
  assert.equal(h.el.props.get("--lab-label-primary"), "#FFFFFF");
});

test("prefers-reduced-motion caps the configured ease duration at 80ms", () => {
  // easeMs requested 280, but reducedMotion caps to <= 80. We assert the cap by
  // observing the ease completes within the shortened window.
  const h = harness({ easeMs: 280, reducedMotion: true });
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick();
  h.colors.setResolve(oneRole("#F0F0F0", 100));
  h.setNow(2000); // well past sustain+dwell
  h.setBg("#202021");
  h.ctrl.tick(); // re-solve + begin ease
  assert.equal(h.colors.resolveCount(), 2);
  // 80ms later the ease must be DONE (a non-reduced 280ms ease would not be).
  h.setNow(2000 + 80);
  h.colors.setRecheckLc([100]);
  h.setBg("#202022");
  h.ctrl.tick();
  assert.equal(h.el.props.get("--lab-label-primary"), "#F0F0F0", "reduced-motion ease is short");
});

test("a static background with no breach does no work (no re-solve, no recheck churn)", () => {
  const h = harness();
  // bg unchanged from the initial #FFFFFF; ticks should early-out.
  for (let i = 0; i < 5; i++) {
    h.advance(16);
    h.ctrl.tick();
  }
  assert.equal(h.colors.resolveCount(), 1, "static passing bg never re-solves");
});

test("a background that changes once to a failing value still re-solves (stable-fail)", () => {
  const h = harness();
  h.colors.setRecheckLc([10]);
  h.colors.setResolve(oneRole("#EEEEEE", 100));
  h.setBg("#181818"); // changed once to a failing bg, then held
  h.ctrl.tick(); // arms breach
  // Hold the SAME failing bg across ticks; the sustain timer must still fire.
  for (let i = 0; i < 10; i++) {
    h.advance(40);
    h.ctrl.tick();
  }
  assert.equal(h.colors.resolveCount(), 2, "stable failing bg must re-solve via the breach timer");
});

// Drive a dark-background breach that re-solves a black role to white, then ease
// across the (polarity-crossing) blend, sampling the applied colour each frame.
// Returns the contrast each frame achieved against the dark background.
function easeContrasts({ strict }) {
  const h = harness({ strict, easeMs: 100 });
  h.colors.setRecheckLc([10]); // current #000000 fails on the dark bg
  h.colors.setResolve(floorRole("#FFFFFF", 100, 4.5)); // re-solve → legal white
  h.setBg("#101010");
  h.setNow(2000);
  h.ctrl.tick(); // arms the breach timer (no re-solve yet)
  const t0 = 2000 + 130; // past sustainMs (120) and dwell vs lastSolveAt
  h.setNow(t0);
  h.setBg("#101011");
  h.ctrl.tick(); // sustained breach → re-solve + begin ease + first eased frame
  h.colors.setRecheckLc([100]); // the white destination passes henceforth
  const bg = "#101011";
  const out = [wcagContrast(h.el.props.get("--lab-label-primary"), bg)];
  for (const dt of [10, 25, 50, 75, 100]) {
    h.setNow(t0 + dt);
    h.ctrl.tick();
    out.push(wcagContrast(h.el.props.get("--lab-label-primary"), bg));
  }
  return { h, out };
}

test("legacy strict clamp holds the floor on the canonical polarity-crossing fixture", () => {
  const { h, out } = easeContrasts({ strict: true });
  assert.equal(h.colors.resolveCount(), 2);
  for (const c of out) {
    assert.ok(c >= 4.5 - 0.05, `every eased frame must clear 4.5:1, saw ${c.toFixed(2)}`);
  }
  // And it still arrives exactly at the freshly-solved destination.
  assert.equal(h.el.props.get("--lab-label-primary"), "#FFFFFF");
});

test("legacy strict fixture has non-regressing sampled contrast", () => {
  const { out } = easeContrasts({ strict: true });
  for (let i = 1; i < out.length; i++) {
    assert.ok(
      out[i] >= out[i - 1] - 0.05,
      `contrast must not regress mid-ease: ${out[i - 1].toFixed(2)} → ${out[i].toFixed(2)}`,
    );
  }
});

test("held latch never reverses the scalar blend when the background drifts favourably", () => {
  // The structural guarantee is only on the scalar blend parameter: it advances
  // from→to even when a favourably-drifting (darkening) background would
  // let the stateless floor solver pick a LOWER blend frame to frame. Without the
  // `held` latch the grey value would step back down; with it, it is monotone.
  const h = harness({ strict: true, easeMs: 400 }); // long ease so bg drift dominates
  h.colors.setRecheckLc([10]);
  h.colors.setResolve(floorRole("#FFFFFF", 100, 4.5));
  h.setBg("#303030"); // moderate dark at re-solve → forces a mid blend up front
  h.setNow(2000);
  h.ctrl.tick(); // arm breach
  const t0 = 2130;
  h.setNow(t0);
  h.setBg("#2F2F2F");
  h.ctrl.tick(); // re-solve + begin ease (first eased frame on a dark bg)
  h.colors.setRecheckLc([100]);
  const grey = () => parseInt(h.el.props.get("--lab-label-primary").slice(1, 3), 16);
  let prev = grey();
  // Drift the background DARKER mid-ease: the legal floor gets *easier*, so the
  // stateless solver would choose a smaller blend — the latch must hold the line.
  const bgs = ["#202020", "#141414", "#0C0C0C", "#060606", "#000000"];
  for (let i = 0; i < bgs.length; i++) {
    h.setNow(t0 + 20 + i * 20);
    h.setBg(bgs[i]);
    h.ctrl.tick();
    const g = grey();
    assert.ok(g >= prev - 1, `colour must not retreat toward the origin: ${prev} → ${g}`);
    prev = g;
  }
});

test("the default ease dips below the floor on the canonical strict comparison fixture", () => {
  const { out } = easeContrasts({ strict: false });
  assert.ok(
    out.some((c) => c < 4.5),
    "without strict, an early polarity-crossing frame is expected below 4.5:1",
  );
});

test("strict mode leaves floorless (decorative) roles to ease freely", () => {
  // legalFloor null → the clamp is a no-op; the role crosses low contrast freely.
  const h = harness({ strict: true, easeMs: 100 });
  h.colors.setRecheckLc([10]);
  h.colors.setResolve(floorRole("#FFFFFF", 100, null)); // no legal floor
  h.setBg("#101010");
  h.setNow(2000);
  h.ctrl.tick(); // arm breach
  h.setNow(2130);
  h.setBg("#101011");
  h.ctrl.tick(); // re-solve + begin ease (first eased frame at t=0 → #000000 end)
  h.colors.setRecheckLc([100]);
  const c0 = wcagContrast(h.el.props.get("--lab-label-primary"), "#101011");
  assert.ok(c0 < 4.5, `a floorless role must ease freely (low contrast allowed), saw ${c0.toFixed(2)}`);
});

test("worst-case recheck breaches when any sample fails (even if another passes)", () => {
  let samples = ["#FFFFFF", "#FAFAFA"]; // both pass at construction
  const h = harness({ background: () => samples });
  h.colors.setResolve(oneRole("#EEEEEE", 100));
  h.colors.setRecheckByBg({ "#FFFFFF": [100], "#FAFAFA": [100], "#202020": [10] });
  samples = ["#FFFFFF", "#202020"]; // backdrop now spans a failing region
  h.ctrl.tick(); // key changed → worst-case recheck arms the breach
  assert.equal(h.colors.resolveCount(), 1, "arms only; dwell/sustain not yet met");
  h.setNow(1300); // past sustain (120) and dwell vs lastSolveAt (1000)
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 2, "a failing sample must force a re-solve");
  assert.equal(h.colors.lastResolveBg(), "#202020", "re-solve targets the hardest sample");
});

test("worst-case recheck holds when every sample stays above its trigger", () => {
  let samples = ["#FFFFFF", "#FAFAFA"];
  const h = harness({ background: () => samples });
  h.colors.setRecheckByBg({ "#FFFFFF": [100], "#FAFAFA": [100], "#F5F5F5": [90] });
  samples = ["#FFFFFF", "#F5F5F5"]; // 90 is still above the 80 margin threshold
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 1, "all samples pass → hold, no re-solve");
});

test("the worst sample is chosen by least margin, not by position", () => {
  let samples = ["#FFFFFF", "#FAFAFA"];
  const h = harness({ background: () => samples });
  h.colors.setResolve(oneRole("#EEEEEE", 100));
  // Here the FIRST sample is the failing/hardest one.
  h.colors.setRecheckByBg({ "#101010": [10], "#FFFFFF": [100], "#FAFAFA": [100] });
  samples = ["#101010", "#FFFFFF"];
  h.ctrl.tick();
  h.setNow(1300);
  h.ctrl.tick();
  assert.equal(h.colors.lastResolveBg(), "#101010", "re-solve targets the least-margin sample");
});

test("initial apply re-solves against the worst sample of a varying backdrop", () => {
  const colors = fakeColors(oneRole("#000000", 100));
  colors.setRecheckByBg({ "#FFFFFF": [100], "#202020": [10] });
  const el = fakeElement();
  adaptTheme(el, {
    colors,
    theme: "light",
    background: () => ["#FFFFFF", "#202020"],
    target: el,
    now: () => 1000,
    win: {},
  });
  // Provisional solve (#FFFFFF), then worst-case recheck picks #202020 and re-solves.
  assert.equal(colors.resolveCount(), 2, "init adopts against the worst sample");
  assert.equal(colors.lastResolveBg(), "#202020");
});

test("legacy strict clamp holds the canonical hardest-sample fixture", () => {
  let samples = ["#0A0A0A"]; // passing at construction
  const h = harness({ strict: true, easeMs: 100, background: () => samples });
  h.colors.setResolve(floorRole("#FFFFFF", 100, 4.5));
  h.colors.setRecheckByBg({ "#0A0A0A": [100], "#1A1A1A": [10], "#101010": [10] });
  samples = ["#1A1A1A", "#101010"]; // dark backdrop; #1A1A1A is the hardest (lightest)
  h.ctrl.tick(); // arm breach
  h.setNow(1300);
  h.ctrl.tick(); // re-solve against the worst sample + begin ease
  assert.equal(h.colors.lastResolveBg(), "#1A1A1A");
  for (const dt of [0, 25, 50, 75, 100]) {
    h.setNow(1300 + dt);
    h.ctrl.tick();
    const hex = h.el.props.get("--lab-label-primary");
    assert.ok(
      wcagContrast(hex, "#1A1A1A") >= 4.5 - 0.05,
      `frame ${dt}: ${hex} below the floor against the worst sample`,
    );
  }
});

test("a single-sample array behaves like a solid background (holds, no churn)", () => {
  const h = harness({ background: () => ["#FFFFFF"] });
  h.colors.setRecheckLc([100]);
  for (let i = 0; i < 4; i++) {
    h.advance(16);
    h.ctrl.tick();
  }
  assert.equal(h.colors.resolveCount(), 1, "one passing sample → identical to a solid bg");
});

// Channel distance between two `#RRGGBB`, as a max-per-channel step count.
function hexStep(a, b) {
  const n = (h, i) => parseInt(h.slice(1 + 2 * i, 3 + 2 * i), 16);
  return Math.max(Math.abs(n(a, 0) - n(b, 0)), Math.abs(n(a, 1) - n(b, 1)), Math.abs(n(a, 2) - n(b, 2)));
}

test("overlapping re-solve continues from the PAINTED colour mid-ease, not a snap to the old target", () => {
  // Regression for the #80 audit HIGH: a re-solve that fires while a previous ease
  // is still in flight must begin from the colour currently ON SCREEN. The pre-fix
  // code began from the in-flight ease's TARGET (currentApplied()'s seg.to), so the
  // first frame of the new ease SNAPPED to the old target before easing — the very
  // flicker the controller exists to remove.
  //
  // easeMs is deliberately > dwellMs so the second re-solve (gated by dwell) lands
  // while the first ease is genuinely mid-flight (painted colour != old target).
  // This is the DEFAULT regime (dwellMs 250 < easeMs 280); here easeMs 1000 just
  // widens the window so the mid-ease colour is unambiguously distinct. dwellMs is
  // 251 so an observe-only tick at 1650 precedes the overlapping re-solve at 1651.
  const h = harness({ easeMs: 1000, dwellMs: 251, sustainMs: 120 });

  // Arm a sustained breach at t=1000, then re-solve #1 to a light colour at t=1400
  // (past sustain + dwell). The first ease runs #000000 -> #F0F0F0 over [1400,2400].
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick(); // breachSince = 1000, resolveCount stays 1
  h.colors.setResolve(oneRole("#F0F0F0", 100));
  h.setNow(1400);
  h.setBg("#202021");
  h.ctrl.tick(); // re-solve #1, ease begins; first painted frame is still #000000
  assert.equal(h.colors.resolveCount(), 2);
  // re-solve #1 reset breachSince to null; re-arm it under the still-failing bg so
  // the debounce (sustainMs) for the SECOND re-solve is satisfied well before dwell.
  h.setNow(1410);
  h.setBg("#202022");
  h.ctrl.tick(); // breachSince = 1410; sustain & dwell not yet met → no re-solve

  // OBSERVE the colour actually painted mid-ease, live off the element. At 1650 the
  // breach is sustained (240ms >= 120) but dwell since 1400 is 250 < 251, so this
  // tick only STEPS ease #1 — no re-solve. The painted value lies strictly between
  // the start (#000000) and the old target (#F0F0F0).
  h.setNow(1650);
  h.setBg("#202023");
  h.ctrl.tick();
  const midPainted = h.el.props.get("--lab-label-primary");
  assert.equal(h.colors.resolveCount(), 2, "observe tick must not re-solve yet (dwell not met)");
  assert.notEqual(midPainted, "#000000");
  assert.notEqual(midPainted, "#F0F0F0", "mid-ease colour is on the path, not at either end");

  // Point re-solve #2 at a DARK target and advance one frame so the dwell gate
  // (251ms since 1400) clears. Ease #1 is still in flight (251 < easeMs 1000), so
  // this re-solve OVERLAPS it.
  h.colors.setResolve(oneRole("#202020", 100));
  h.setNow(1651);
  h.setBg("#202024");
  h.ctrl.tick();
  assert.equal(h.colors.resolveCount(), 3, "overlapping re-solve fired mid-ease");

  // The new ease must START from the painted colour (continuous, within 1 step of
  // what was on screen the previous frame) — NOT jump to the old target #F0F0F0.
  const firstNewFrame = h.el.props.get("--lab-label-primary");
  assert.ok(
    hexStep(firstNewFrame, midPainted) <= 1,
    `first frame of overlapping ease (${firstNewFrame}) must continue from the painted colour (${midPainted}), not snap`,
  );
  assert.notEqual(firstNewFrame, "#F0F0F0", "must NOT snap to the previous ease's target");
});

test("a NaN clock mid-ease never paints invalid #NANNANNAN CSS (easeOut NaN guard)", () => {
  // The guard lives in easeOut: t = (now - easeStart) / easeMs is NaN when the
  // clock yields NaN, and without `Number.isFinite(t) ? . : 1` the eased blend
  // is NaN → lerpHex emits "#NaNNaNNaN", invalid CSS. Drive a real in-flight ease,
  // then STEP it with a NaN clock and assert the painted value is a valid 6-hex
  // colour. The recheck is held passing on the NaN frame so the breach path can
  // NOT re-solve+applyRolesDirect over the eased paint — the value asserted is the
  // one `stepEase` (hence `easeOut`) actually wrote, which is what the guard guards.
  const isHex6 = (s) => /^#[0-9a-fA-F]{6}$/.test(s);
  const h = harness({ easeMs: 100 });

  // Begin an ease #000000 -> #F0F0F0.
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick(); // arm breach
  h.colors.setResolve(oneRole("#F0F0F0", 100));
  h.setNow(1300);
  h.setBg("#202021");
  h.ctrl.tick(); // re-solve + begin ease (in flight now)
  // From here the destination passes, so no further re-solve can fire — any
  // subsequent paint comes from stepEase advancing the in-flight segment.
  h.colors.setRecheckLc([100]);
  h.setNow(1340); // mid-ease: a genuine interpolated frame (t = 0.4)
  h.setBg("#202022");
  h.ctrl.tick();
  const midPainted = h.el.props.get("--lab-label-primary");
  assert.ok(isHex6(midPainted), "sanity: a valid interpolated hex mid-ease");
  assert.notEqual(midPainted, "#000000", "sanity: genuinely mid-path, not the origin");
  assert.notEqual(midPainted, "#F0F0F0", "sanity: genuinely mid-path, not the target");

  // Now STEP the still-in-flight ease with a NaN clock. easeOut's guard turns the
  // NaN `t` into a completed ease (1) → a valid hex; without it lerpHex emits
  // "#NaNNaNNaN". The passing recheck means this paint is NOT overwritten by a
  // re-solve, so it is exactly what easeOut produced.
  h.setBg("#202023");
  h.ctrl.tick(NaN);
  const painted = h.el.props.get("--lab-label-primary");
  assert.ok(isHex6(painted), `NaN clock must still paint a valid #RRGGBB, saw ${painted}`);
  assert.ok(!/nan/i.test(painted), `painted colour must contain no NaN channel (never #NANNANNAN), saw ${painted}`);
});

test("rejects a colours engine missing recheckContrast", () => {
  assert.throws(
    () => adaptTheme(fakeElement(), { theme: "light", colors: { resolveTheme() {} }, win: {} }),
    TypeError,
  );
});

// ── batch recheck (recheckContrastMulti) wiring ──────────────────────────────
// The controller collapses the multi-sample worst-case loop into ONE engine call
// when the engine exposes `recheckContrastMulti`. These tests prove the batch
// path is BEHAVIOURALLY identical to the per-sample fallback (same worstIdx, same
// re-solve target, same applied DOM), and that the batch method is actually used.

// A batch-capable fake: same per-bg Lc data as `fakeColors`, plus a
// `recheckContrastMulti` that assembles the background-major flat buffer the WASM
// engine returns. Records how many per-sample vs batch calls it served, so a test
// can prove the multi path (not the loop) ran.
function fakeColorsBatch(initial) {
  const base = fakeColors(initial);
  let perSampleCalls = 0;
  let multiCalls = 0;
  const perSample = base.recheckContrast;
  return {
    ...base,
    perSampleCalls: () => perSampleCalls,
    multiCalls: () => multiCalls,
    recheckContrast(bg, fgs, theme) {
      perSampleCalls++;
      return perSample(bg, fgs, theme);
    },
    // Background-major: sample s, foreground i at (s * fgs.length + i) * 2.
    recheckContrastMulti(bgs, fgs, theme) {
      multiCalls++;
      const out = [];
      for (const bg of bgs) {
        const flat = perSample(bg, fgs, theme);
        for (const v of flat) out.push(v);
      }
      return out;
    },
  };
}

function batchHarness(colors, opts = {}) {
  const el = fakeElement();
  let bg = opts.background ? undefined : "#FFFFFF";
  let now = 1000;
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: opts.background ?? (() => bg),
    target: el,
    now: () => now,
    win: {},
    easeMs: 100,
    sustainMs: 120,
    dwellMs: 250,
    dropFraction: 0.2,
    ...opts,
  });
  return { ctrl, colors, el, setNow: (n) => (now = n), advance: (ms) => (now += ms) };
}

test("batch engine uses recheckContrastMulti for a multi-sample frame, not the per-sample loop", () => {
  let samples = ["#FFFFFF", "#FAFAFA"];
  const colors = fakeColorsBatch(oneRole("#000000", 100));
  const h = batchHarness(colors, { background: () => samples });
  const perAtStart = colors.perSampleCalls();
  const multiAtStart = colors.multiCalls();
  // A frame that rechecks a >1-sample backdrop must go through the batch call.
  samples = ["#FFFFFF", "#202020"];
  h.ctrl.tick();
  assert.ok(colors.multiCalls() > multiAtStart, "multi-sample recheck must call recheckContrastMulti");
  assert.equal(
    colors.perSampleCalls(),
    perAtStart,
    "multi-sample recheck must NOT fall back to the per-sample recheckContrast loop",
  );
});

test("batch path is byte-identical to the fallback loop: same worstIdx, re-solve target, applied DOM", () => {
  // Same scenario, two engines: one batch-capable, one fallback-only. The chosen
  // worst sample, the re-solve background, and the final applied vars must match.
  const scenario = (colors) => {
    let samples = ["#FFFFFF", "#FAFAFA"];
    const h = batchHarness(colors, { background: () => samples });
    // A varying backdrop where the SECOND-position sample is the hardest.
    colors.setRecheckByBg({ "#FFFFFF": [100], "#101010": [40] });
    samples = ["#FFFFFF", "#101010"];
    h.ctrl.tick(); // sustain window not yet elapsed
    h.advance(400); // clear sustainMs + dwellMs
    h.ctrl.tick(); // sustained breach → re-solve against the worst sample
    return {
      resolveBg: colors.lastResolveBg(),
      resolveCount: colors.resolveCount(),
      applied: h.el.props.get("--lab-label-primary"),
    };
  };
  const batch = scenario(fakeColorsBatch(oneRole("#000000", 100)));
  const fallback = scenario(fakeColors(oneRole("#000000", 100)));
  assert.equal(batch.resolveBg, fallback.resolveBg, "same re-solve target (same worstIdx chosen)");
  assert.equal(batch.resolveBg, "#101010", "re-solve targets the hardest sample");
  assert.equal(batch.resolveCount, fallback.resolveCount, "same number of re-solves");
  assert.equal(batch.applied, fallback.applied, "same applied var after the re-solve");
});

test("batch engine still uses the per-sample path for a single-sample backdrop", () => {
  let sample = "#FFFFFF";
  const colors = fakeColorsBatch(oneRole("#000000", 100));
  const h = batchHarness(colors, { background: () => [sample] });
  const perAtStart = colors.perSampleCalls();
  const multiAtStart = colors.multiCalls();
  colors.setRecheckLc([95]);
  // Change the (still single-sample) backdrop so the tick actually rechecks
  // (key !== lastKey) instead of early-returning — otherwise recheckSamples is
  // never entered and the counter assertions below are vacuous.
  sample = "#202020";
  h.ctrl.tick();
  assert.ok(
    colors.perSampleCalls() > perAtStart,
    "a single-sample recheck must run through the per-sample recheckContrast path",
  );
  assert.equal(
    colors.multiCalls(),
    multiAtStart,
    "one sample must not use the batch call (nothing to collapse)",
  );
});
