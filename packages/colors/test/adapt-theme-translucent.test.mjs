// Class-lock: the adaptive controller must preserve EVERY reachable role, not
// just `kind === "color"` ones.
//
// A loaded engine (labui) resolves translucent roles too (Resolved::Translucent
// — a tint + alpha, emitted in `result.vars`). The controller only tracks
// `kind === "color"` roles for easing, and every apply goes through
// `applyTheme`, which FIRST clears all inline `--lab-*` then writes only what it
// is given. So the pre-fix controller (writing only color hexes) silently (1)
// erased translucent vars on the first apply, (2) erased them on every ease
// frame, and (3) never updated them on `setTheme`. This suite closes that class:
// the controller carries the full `result.vars` (baseVars) and merges the eased
// color overlay on top, so translucent roles are always present and current.

import { test } from "node:test";
import assert from "node:assert/strict";

import { adaptTheme } from "../adapt-theme.js";

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

// A resolve result carrying BOTH a color role and a translucent role. `labelVar`
// is the color role's canonical (oklch) string in `vars`; `hex` is its solved
// hex (what the ease interpolates over). `panelVar` is the translucent role's
// canonical string — present only in `vars`, never a "color" role.
const makeResult = (hex, labelVar, panelVar, floorRatio = null) => ({
  vars: { "--lab-label": labelVar, "--lab-panel": panelVar },
  roles: {
    label: { kind: "color", cssVar: "--lab-label", hex, lc: 100, floorRatio },
    panel: { kind: "translucent", cssVar: "--lab-panel" },
  },
});

// A fake engine whose current result and recheck Lc are settable (same shape as
// the main adapt-theme test's fake, plus a translucent role in the result).
function fakeColors(initial) {
  let resolveCount = 0;
  let current = initial;
  let recheckLc = [100]; // one color role, passing by default
  return {
    resolveCount: () => resolveCount,
    setResolve: (r) => (current = r),
    setRecheckLc: (l) => (recheckLc = l),
    resolveTheme() {
      resolveCount++;
      return current;
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

const LIGHT = makeResult("#1A1A1A", "oklch(20.000% 0 0)", "oklch(96.000% 0.01 260 / 0.6)");

function harness(opts = {}) {
  const colors = fakeColors(LIGHT);
  const el = fakeElement();
  let bg = "#FFFFFF";
  let now = 1000;
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
    dropFraction: 0.2,
    ...opts,
  });
  return { ctrl, colors, el, setBg: (b) => (bg = b), setNow: (n) => (now = n) };
}

test("initial apply keeps the translucent role's var present (canonical form)", () => {
  const h = harness();
  assert.equal(h.el.props.get("--lab-panel"), "oklch(96.000% 0.01 260 / 0.6)");
  // The color role is written in its canonical (vars) form, not a raw hex.
  assert.equal(h.el.props.get("--lab-label"), "oklch(20.000% 0 0)");
});

test("setTheme updates the translucent role's var to the new theme", () => {
  const h = harness();
  const DARK = makeResult("#E5E5E5", "oklch(90.000% 0 0)", "oklch(30.000% 0.02 260 / 0.6)");
  h.colors.setResolve(DARK);
  h.ctrl.setTheme("dark");
  assert.equal(h.el.props.get("--lab-panel"), "oklch(30.000% 0.02 260 / 0.6)");
  assert.equal(h.el.props.get("--lab-label"), "oklch(90.000% 0 0)");
});

test("an in-flight ease frame does not erase the translucent var", () => {
  const h = harness();
  // Arm a breach, then re-solve to a DIFFERENT colour so an ease actually runs.
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick(); // breachSince = 1000
  h.colors.setResolve(makeResult("#F0F0F0", "oklch(94.000% 0 0)", "oklch(94.000% 0.01 260 / 0.6)"));
  h.setNow(1300); // past sustain (120) + dwell (250 vs lastSolveAt 1000)
  h.setBg("#202021");
  h.ctrl.tick(); // re-solve + begin ease (#1A1A1A → #F0F0F0)
  h.colors.setRecheckLc([100]); // destination passes → no re-arm
  // Mid-ease frame.
  h.setNow(1350);
  h.setBg("#202022");
  h.ctrl.tick();
  const panel = h.el.props.get("--lab-panel");
  assert.notEqual(panel, undefined, "translucent var must survive an ease frame");
  assert.equal(panel, "oklch(94.000% 0.01 260 / 0.6)", "and carry the freshly-solved value");
  // The color role is mid-ease → a hex overlay, proving the ease is genuinely live.
  assert.match(h.el.props.get("--lab-label"), /^#[0-9A-Fa-f]{6}$/);
});

test("when the ease completes, color vars revert to canonical oklch (not stuck in hex)", () => {
  const h = harness();
  h.colors.setRecheckLc([10]);
  h.setBg("#202020");
  h.ctrl.tick(); // arm
  h.colors.setResolve(makeResult("#F0F0F0", "oklch(94.000% 0 0)", "oklch(94.000% 0.01 260 / 0.6)"));
  h.setNow(1300);
  h.setBg("#202021");
  h.ctrl.tick(); // re-solve + begin ease
  h.colors.setRecheckLc([100]);
  // Advance well past the ease window (easeMs 100).
  h.setNow(1500);
  h.setBg("#202099");
  h.ctrl.tick();
  assert.equal(
    h.el.props.get("--lab-label"),
    "oklch(94.000% 0 0)",
    "settled color role returns to canonical oklch form",
  );
  assert.equal(h.el.props.get("--lab-panel"), "oklch(94.000% 0.01 260 / 0.6)");
});

test("current() reports the full applied picture, including translucent roles", () => {
  const h = harness();
  const snap = h.ctrl.current();
  assert.equal(snap["--lab-panel"], "oklch(96.000% 0.01 260 / 0.6)");
  assert.equal(snap["--lab-label"], "oklch(20.000% 0 0)");
});

// baseVars is a REPLACE of the last solve's vars, never a merge. These lock the
// "stuck key" class: a role present in an earlier solve but ABSENT from a later
// one must not linger. A merge (`{...baseVars, ...result.vars}`) would leave the
// stale var on the element AND in current() — invisible to every test that keeps
// the role set constant, so it is pinned explicitly here.
const THREE = {
  vars: {
    "--lab-label": "oklch(20.000% 0 0)",
    "--lab-panel": "oklch(96.000% 0.01 260 / 0.6)",
    "--lab-extra": "oklch(50.000% 0.1 120 / 0.4)",
  },
  roles: {
    label: { kind: "color", cssVar: "--lab-label", hex: "#1A1A1A", lc: 100, floorRatio: null },
    panel: { kind: "translucent", cssVar: "--lab-panel" },
    extra: { kind: "translucent", cssVar: "--lab-extra" },
  },
};
const TWO = {
  vars: { "--lab-label": "oklch(90.000% 0 0)", "--lab-panel": "oklch(30.000% 0.02 260 / 0.6)" },
  roles: {
    label: { kind: "color", cssVar: "--lab-label", hex: "#E5E5E5", lc: 100, floorRatio: null },
    panel: { kind: "translucent", cssVar: "--lab-panel" },
  },
};
const ONE = {
  vars: { "--lab-label": "oklch(20.000% 0 0)" },
  roles: { label: { kind: "color", cssVar: "--lab-label", hex: "#1A1A1A", lc: 100, floorRatio: null } },
};

test("a later solve that DROPS a role removes its var from target and current()", () => {
  const colors = fakeColors(THREE);
  const el = fakeElement();
  const ctrl = adaptTheme(el, { colors, theme: "light", background: () => "#FFFFFF", target: el, now: () => 1000, win: {} });
  assert.equal(el.props.get("--lab-extra"), "oklch(50.000% 0.1 120 / 0.4)", "present initially");
  colors.setResolve(TWO); // extra role gone
  ctrl.setTheme("dark");
  assert.equal(el.props.get("--lab-extra"), undefined, "dropped role's var must vanish from target");
  assert.ok(!("--lab-extra" in ctrl.current()), "and be absent from current()");
  // The surviving roles are updated (not stale).
  assert.equal(el.props.get("--lab-panel"), "oklch(30.000% 0.02 260 / 0.6)");
  assert.equal(el.props.get("--lab-label"), "oklch(90.000% 0 0)");
});

test("a later solve that ADDS a role writes its new var", () => {
  const colors = fakeColors(ONE);
  const el = fakeElement();
  const ctrl = adaptTheme(el, { colors, theme: "light", background: () => "#FFFFFF", target: el, now: () => 1000, win: {} });
  assert.equal(el.props.get("--lab-panel"), undefined, "absent initially");
  colors.setResolve(TWO); // panel role added
  ctrl.setTheme("dark");
  assert.equal(el.props.get("--lab-panel"), "oklch(30.000% 0.02 260 / 0.6)", "added role's var must appear");
});

// Differential lock for the OTHER color-string consumer in this file: the
// strict floor-clamp reads each background sample through `relativeLuminanceHex`
// → `parseCssColor`. An explicit oklch background must drive the clamp EXACTLY
// like its hex equivalent; if oklch were unparsed (→ null → black luminance),
// the floor math would use the wrong luminance and the eased frames diverge.
//
// The background alternates between two dark colours so the key changes each
// tick (defeating the steady-state early-out); `bgHex`/`bgOklch` are the SAME
// two colours in each representation, so a correct parse makes the two runs
// bit-identical. Two solid fixtures, live-emitted: #1A1A1A / #000000.
const BG_HEX = ["#1A1A1A", "#000000"];
const BG_OKLCH = ["oklch(21.77865% 0.000000 89.876)", "oklch(0.00000% 0.000000 0.000)"];

function strictEasePaints(seq) {
  const colors = fakeColors(makeResult("#000000", "oklch(0.000% 0 0)", "oklch(96.000% 0.01 260 / 0.6)", 4.5));
  const el = fakeElement();
  let now = 2000;
  let i = 0; // constructor reads seq[0]; each tick advances first
  const ctrl = adaptTheme(el, {
    colors,
    theme: "light",
    background: () => seq[i % seq.length],
    target: el,
    now: () => now,
    win: {},
    strict: true,
    easeMs: 100,
    sustainMs: 120,
    dwellMs: 250,
  });
  colors.setRecheckLc([10]); // black fails on the dark bg → will breach
  colors.setResolve(makeResult("#FFFFFF", "oklch(100.000% 0 0)", "oklch(30.000% 0.02 260 / 0.6)", 4.5));
  const tickAt = (t) => {
    i++;
    now = t;
    ctrl.tick();
  };
  tickAt(2130); // arm breach (key changed)
  tickAt(2260); // breach sustained (130≥120) + dwell met (260≥250) → re-solve + ease
  colors.setRecheckLc([100]); // destination passes → no re-arm; pure ease henceforth
  const paints = [el.props.get("--lab-label")];
  for (const t of [2270, 2285, 2310, 2335]) {
    tickAt(t);
    paints.push(el.props.get("--lab-label"));
  }
  return paints;
}

test("strict floor-clamp reads an oklch background sample identically to its hex equivalent", () => {
  const hexPaints = strictEasePaints(BG_HEX);
  const oklchPaints = strictEasePaints(BG_OKLCH);
  // Guard: every captured frame is a mid-ease hex → the clamp is genuinely
  // active (not a trivial no-ease case that would pass vacuously).
  for (const p of hexPaints) assert.match(p, /^#[0-9A-Fa-f]{6}$/);
  assert.deepEqual(oklchPaints, hexPaints, "oklch background must drive the strict clamp like its hex form");
});
