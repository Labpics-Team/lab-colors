// Behaviour tests for the adaptive hysteresis controller, driven through injected
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
  let recheckLc = initial.roles
    ? Object.values(initial.roles)
        .filter((r) => r.kind === "color")
        .map((r) => r.lc)
    : [];
  return {
    resolveCount: () => resolveCount,
    setResolve(r) {
      resolve = r;
    },
    setRecheckLc(lcs) {
      recheckLc = lcs;
    },
    resolveTheme() {
      resolveCount++;
      return resolve;
    },
    recheckContrast() {
      const out = [];
      for (const lc of recheckLc) {
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

test("holds (no re-solve) while colours still pass; Schmitt tolerates small drops", () => {
  const h = harness();
  // A small drop (95 of 100; tolerance keeps to 80) → still passing → hold.
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

test("prefers-reduced-motion caps the ease to a short gentle fade (not a snap)", () => {
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

test("rejects a colours engine missing recheckContrast", () => {
  assert.throws(
    () => adaptTheme(fakeElement(), { theme: "light", colors: { resolveTheme() {} }, win: {} }),
    TypeError,
  );
});
