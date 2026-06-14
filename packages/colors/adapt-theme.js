// Adaptive theme controller — the lazy/hysteresis runtime. Zero dependencies.
//
// `watchTheme` re-resolves the whole set whenever the background changes. For a
// CONTINUOUSLY changing background (animation/scroll/blur) that is both expensive
// (a full solve every frame) and jittery (colours twitch frame to frame).
//
// `adaptTheme` is the elegant alternative, and the way real systems behave: it
// does NOT re-solve per frame. Each frame it cheaply RE-CHECKS whether the
// current colours still pass their contrast against the (new) background — one
// CAM16 forward for the background plus one per role, no solve. While they pass
// it does nothing (no churn, no jitter). Only when a role's perceptual contrast
// stays below target for a sustained moment does it re-solve and **ease** to the
// fresh colours over a short transition. The result: fewer computations, no
// flicker, and a smooth, calm adaptation.
//
// Control law (principled defaults; all tunable):
//   * Schmitt trigger — re-solve only when a role's achieved |Lc| drops by more
//     than `dropFraction` of its surplus, not merely touches the line, so a
//     background hovering on a boundary cannot make it chatter.
//   * Debounce — the breach must persist `sustainMs` before acting, so a dark
//     object scrolling past for a couple of frames never triggers.
//   * Min dwell — at least `dwellMs` between re-solves, capping the effective
//     transition rate well under the flash threshold.
//   * Ease — a non-overshooting ease-out crossfade of `easeMs`; under
//     `prefers-reduced-motion` a gentle short fade (NOT a jarring snap — an
//     instant state change is more stressful than a soft one, and a colour
//     crossfade is not "motion").
//   * Theme switches are a deliberate INTENT, not a drift: applied instantly
//     (a single quick crossfade), never run through the hysteresis machinery.
//
// Floor-clamp modes:
//   * Default (free ease): the crossfade does not floor-clamp each frame.
//     Reading comprehension is far slower than a ~300ms transition and surfaces
//     usually sit on a substrate, so a brief dip of the aesthetic *surplus*
//     during the ease is imperceptible while the freshly-solved destination is
//     always legal.
//   * Strict (`strict: true`): the WCAG legal floor is HELD every frame. For
//     text directly on animated content or under `prefers-contrast`, an
//     intermediate colour is only shown while it still clears the role's
//     `legalFloor` against the live background; a role whose eased intermediate
//     would dip below its floor is advanced (monotonically) to the least blend
//     that stays legal — never below the line, never a backwards flicker. Roles
//     with no legal floor (decorative / JND) ease freely either way.

import { applyTheme } from "./apply-theme.js";
import { effectiveBackground, parseCssColor, toHex } from "./effective-bg.js";

/** Cubic ease-out: fast start, gentle settle, no overshoot. */
function easeOut(t) {
  const u = 1 - Math.min(1, Math.max(0, t));
  return 1 - u * u * u;
}

/** WCAG 2.1 relative luminance of `#RRGGBB` — a faithful transcription of the
 * normative definition (0.03928 split, 2.4 exponent), so the strict floor-clamp
 * agrees byte-for-byte with the core's `legalFloor` semantics. */
function relativeLuminanceHex(hex) {
  const rgb = parseCssColor(hex) ?? [0, 0, 0, 1];
  const lin = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2]);
}

/** WCAG contrast ratio from two relative luminances: `(L+0.05)/(L+0.05)`. */
function wcagRatio(lumA, lumB) {
  const hi = Math.max(lumA, lumB);
  const lo = Math.min(lumA, lumB);
  return (hi + 0.05) / (lo + 0.05);
}

/** Interpolate two `#RRGGBB` hexes at `t ∈ [0,1]` (per-channel; role colours are
 * near-neutral, so a straight sRGB blend has no muddy hue midpoint). */
function lerpHex(from, to, t) {
  const a = parseCssColor(from) ?? [0, 0, 0, 1];
  const b = parseCssColor(to) ?? [0, 0, 0, 1];
  return toHex([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
}

/**
 * @typedef {object} AdaptController
 * @property {(now?: number) => void} tick  Drive one step (call from rAF, or let
 *   `start()` do it). Cheap: a re-check; a re-solve only on a sustained breach.
 * @property {(theme: string) => void} setTheme  Switch theme INSTANTLY (intent,
 *   not drift) — re-resolve and apply, bypassing the hysteresis.
 * @property {() => void} start  Begin an internal requestAnimationFrame loop.
 * @property {() => void} stop   Stop the loop and disconnect.
 * @property {() => Record<string,string>} current  The currently-applied vars.
 */

/**
 * Keep an element's `--lab-*` variables adapting to its (changing) background
 * lazily and smoothly. Applies the resolved set immediately, then holds it while
 * it still passes, re-solving + easing only when contrast stably degrades.
 *
 * @param {*} element
 * @param {object} options
 * @param {{ resolveTheme: (bg:string,theme:string)=>any, recheckContrast:(bg:string,fgs:string[],theme:string)=>ArrayLike<number> }} options.colors
 * @param {string} options.theme
 * @param {string | (() => string)} [options.background]  explicit effective bg
 * @param {*} [options.target=element]  element to write vars onto
 * @param {string} [options.fallback="#FFFFFF"]
 * @param {number} [options.dropFraction=0.2]  surplus fraction lost before re-solve
 * @param {number} [options.sustainMs=120]  breach must persist this long
 * @param {number} [options.dwellMs=250]  minimum between re-solves
 * @param {number} [options.easeMs=280]  crossfade duration
 * @param {boolean} [options.strict=false]  hold each role's WCAG legal floor
 *   every frame of the ease (for text on animated content / `prefers-contrast`)
 * @param {boolean} [options.reducedMotion]  override; default reads matchMedia
 * @param {() => number} [options.now]  clock (default performance.now/Date.now)
 * @param {*} [options.win=globalThis]
 * @param {(el:*)=>*} [options.getStyle]  effectiveBackground seam (testing)
 * @param {(el:*)=>*} [options.parentOf]  effectiveBackground seam (testing)
 * @returns {AdaptController}
 */
export function adaptTheme(element, options) {
  if (
    !options ||
    typeof options.colors?.resolveTheme !== "function" ||
    typeof options.colors?.recheckContrast !== "function"
  ) {
    throw new TypeError("adaptTheme: options.colors needs resolveTheme + recheckContrast");
  }
  const colors = options.colors;
  const target = options.target ?? element;
  const fallback = options.fallback ?? "#FFFFFF";
  const dropFraction = options.dropFraction ?? 0.2;
  const sustainMs = options.sustainMs ?? 120;
  const dwellMs = options.dwellMs ?? 250;
  const strict = options.strict ?? false;
  const win = options.win ?? (typeof globalThis !== "undefined" ? globalThis : undefined);
  const reducedMotion =
    options.reducedMotion ??
    (win?.matchMedia ? win.matchMedia("(prefers-reduced-motion: reduce)").matches : false);
  // Reduced motion → a gentle SHORT fade, never a hard snap.
  const easeMs = reducedMotion ? Math.min(options.easeMs ?? 280, 80) : (options.easeMs ?? 280);
  const clock = options.now ?? (() => (win?.performance?.now ? win.performance.now() : Date.now()));

  let theme = options.theme;
  /** @type {{ cssVar: string, key: string, lc: number, hex: string, legalFloor: number|null }[]} stable role order */
  let roles = [];
  /** @type {Map<string,{from:string,to:string}>} in-flight ease per cssVar */
  let easing = new Map();
  let easeStart = 0;
  let breachSince = null;
  let lastSolveAt = -Infinity;
  let lastBg = null;

  const readBackground = () => {
    const b = options.background;
    if (typeof b === "function") return b();
    if (typeof b === "string") return b;
    return effectiveBackground(element, {
      fallback,
      getStyle: options.getStyle,
      parentOf: options.parentOf,
    });
  };

  // Resolve a fresh set and adopt it as the current colours (no ease).
  const solveAndAdopt = (bg, now) => {
    const result = colors.resolveTheme(bg, theme);
    roles = Object.entries(result.roles)
      .filter(([, r]) => r && r.kind === "color")
      .map(([key, r]) => ({
        cssVar: r.cssVar,
        key,
        lc: r.lc,
        hex: r.hex,
        legalFloor: typeof r.legalFloor === "number" ? r.legalFloor : null,
      }));
    lastSolveAt = now;
    breachSince = null;
    return result;
  };

  const applyHexes = (hexByVar) => applyTheme(target, { vars: hexByVar });

  const applyRolesDirect = () => {
    const vars = {};
    for (const r of roles) vars[r.cssVar] = r.hex;
    applyHexes(vars);
  };

  // Begin an ease from the currently-applied colours toward the role colours.
  // `held` latches the per-role displayed blend so it only ever advances toward
  // the destination (strict mode) — see `stepEase`.
  const beginEase = (fromByVar, now) => {
    easing = new Map();
    for (const r of roles) {
      const from = fromByVar[r.cssVar] ?? r.hex;
      if (from !== r.hex) easing.set(r.cssVar, { from, to: r.hex, held: 0 });
    }
    easeStart = now;
    if (easing.size === 0) applyRolesDirect();
  };

  // Strict mode: the least blend in [e, 1] whose interpolated colour clears
  // `floor` against background luminance `bgLum`. The destination (`to`, blend
  // 1) is a freshly-solved legal colour, so it anchors the search; we bisect
  // toward it from the natural ease value `e`. Returns `e` unchanged when the
  // eased colour is already legal (the common case — no intervention). The
  // returned blend is always floor-legal, except in the unavoidable case where
  // even `to` is illegal against a bg that drifted further this frame — then it
  // returns 1 (the most-legal colour we have) and the recheck loop re-solves.
  const floorBlend = (seg, e, bgLum, floor) => {
    const legalAt = (blend) =>
      wcagRatio(relativeLuminanceHex(lerpHex(seg.from, seg.to, blend)), bgLum) >= floor;
    if (legalAt(e)) return e;
    let lo = e;
    let hi = 1;
    for (let k = 0; k < 14; k++) {
      const mid = (lo + hi) / 2;
      if (legalAt(mid)) hi = mid;
      else lo = mid;
    }
    return hi; // hi is always legal (or blend 1, the most-legal we have)
  };

  const stepEase = (now, bg) => {
    const t = easeMs <= 0 ? 1 : (now - easeStart) / easeMs;
    const e = easeOut(t);
    const bgLum = strict ? relativeLuminanceHex(bg) : 0;
    const vars = {};
    for (const r of roles) {
      const seg = easing.get(r.cssVar);
      if (!seg) {
        vars[r.cssVar] = r.hex;
        continue;
      }
      let blend = e;
      if (strict && r.legalFloor != null) {
        // Hold the floor, then LATCH: the displayed blend may only advance
        // toward the destination, never retreat. `floorBlend` is stateless and
        // depends on the live (drifting) background, so on a frame where the bg
        // drifts favourably it could return a *lower* blend than last frame — a
        // backwards step toward the old colour, the precise jarring reversal
        // this mode exists to avoid. `held` clamps that out: the colour
        // progresses monotonically from→to and never below the legal line.
        blend = Math.max(floorBlend(seg, e, bgLum, r.legalFloor), seg.held);
        seg.held = blend;
      }
      vars[r.cssVar] = lerpHex(seg.from, seg.to, blend);
    }
    applyHexes(vars);
    if (t >= 1) easing = new Map();
  };

  const currentApplied = () => {
    const vars = {};
    for (const r of roles) {
      const seg = easing.get(r.cssVar);
      vars[r.cssVar] = seg ? seg.to : r.hex; // logical target during/after ease
    }
    return vars;
  };

  const tick = (nowArg) => {
    const now = nowArg ?? clock();
    const bg = readBackground();
    // Advance any in-flight ease first (against the live bg, so strict mode holds
    // the legal floor every frame as the background keeps drifting under it).
    if (easing.size > 0) stepEase(now, bg);

    // Steady state: a static background with no in-flight ease and no pending
    // breach needs no work. A PENDING breach keeps us live even on a static bg, so
    // the sustain timer can fire on a background that changed once to a failing
    // value and then held.
    if (bg === lastBg && easing.size === 0 && breachSince === null) return;
    lastBg = bg;
    if (roles.length === 0) return;

    // Cheap re-check: do the current role colours still pass against `bg`?
    const fgs = roles.map((r) => r.hex);
    const flat = colors.recheckContrast(bg, fgs, theme);
    let breached = false;
    for (let i = 0; i < roles.length; i++) {
      const lcNow = Math.abs(flat[2 * i]);
      const want = Math.abs(roles[i].lc) * (1 - dropFraction);
      if (lcNow < want) {
        breached = true;
        break;
      }
    }

    if (!breached) {
      breachSince = null;
      return; // hold — the common case for a slowly-drifting background
    }
    if (breachSince === null) breachSince = now;
    if (now - breachSince < sustainMs || now - lastSolveAt < dwellMs) return; // debounce / dwell

    // Sustained breach: re-solve and ease toward the fresh colours.
    const fromByVar = currentApplied();
    solveAndAdopt(bg, now);
    beginEase(fromByVar, now);
    stepEase(now, bg);
  };

  let rafId = null;
  const loop = () => {
    tick();
    if (win?.requestAnimationFrame) rafId = win.requestAnimationFrame(loop);
  };

  // Apply the initial set immediately.
  {
    const bg = readBackground();
    lastBg = bg;
    solveAndAdopt(bg, clock());
    applyRolesDirect();
  }

  return {
    tick,
    setTheme(next) {
      theme = next;
      const bg = readBackground();
      lastBg = bg;
      easing = new Map();
      solveAndAdopt(bg, clock());
      applyRolesDirect(); // instant — a theme switch is intent, not drift
    },
    start() {
      if (rafId == null && win?.requestAnimationFrame) rafId = win.requestAnimationFrame(loop);
    },
    stop() {
      if (rafId != null && win?.cancelAnimationFrame) win.cancelAnimationFrame(rafId);
      rafId = null;
      easing = new Map();
    },
    current: currentApplied,
  };
}
