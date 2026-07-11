// Adaptive theme controller — the lazy debounced-re-solve runtime. Zero dependencies.
//
// `watchTheme` re-resolves the whole set whenever the background changes. For a
// CONTINUOUSLY changing background (animation/scroll/blur) that is both expensive
// (a full solve every frame) and jittery (colours twitch frame to frame).
//
// `adaptTheme` is the elegant alternative, and the way real systems behave: it
// does NOT re-solve per frame. Each frame it cheaply RE-CHECKS whether the
// current colours still pass their contrast against the (new) background — one
// perceptual-model forward for the background plus one per role, no solve. While they pass
// it does nothing (no churn, no jitter). Only when a role's perceptual contrast
// stays below target for a sustained moment does it re-solve and **ease** to the
// fresh colours over a short transition. The result: fewer computations, no
// flicker, and a smooth, calm adaptation.
//
// Control law (principled defaults; all tunable):
//   * Margin threshold — re-solve only when a role's achieved |Lc| falls below
//     `(1 - dropFraction)` of its target |Lc| (a `dropFraction` margin under the
//     target), not merely when it touches the line. This is a SINGLE-threshold
//     detector: the same level gates entering and leaving the breach state — NOT a
//     Schmitt trigger, which would need two distinct levels. The margin keeps a
//     background sitting right at the target from counting as a breach, but it
//     does NOT by itself stop chatter for values straddling the threshold.
//   * Debounce — the breach must persist `sustainMs` before acting, so a dark
//     object scrolling past for a couple of frames never triggers. Together with
//     min-dwell this is what actually prevents oscillation near the threshold.
//   * Min dwell — at least `dwellMs` between re-solves, capping the effective
//     transition rate well under the flash threshold.
//   * Ease — a non-overshooting ease-out crossfade of `easeMs`; under
//     `prefers-reduced-motion` a gentle short fade (NOT a jarring snap — an
//     instant state change is more stressful than a soft one, and a colour
//     crossfade is not "motion").
//   * Theme switches are a deliberate INTENT, not a drift: applied instantly
//     (a single quick crossfade), never run through the debounce/dwell machinery.
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
//     with no legal floor (decorative) ease freely either way.

import { applyTheme } from "./apply-theme.js";
import {
  effectiveBackground,
  parseCssColor,
  oklabLerp,
  compileLerpPair,
  lerpPairHex,
  lerpPairLuminance,
  wcagLuminanceCached,
} from "./effective-bg.js";

/** Cubic ease-out: fast start, gentle settle, no overshoot. A non-finite `t`
 * (e.g. a NaN clock making `(now - easeStart) / easeMs` NaN) is treated as a
 * completed ease (1), so the crossfade can never emit `#NANNANNAN` CSS. */
function easeOut(t) {
  const clamped = Number.isFinite(t) ? Math.min(1, Math.max(0, t)) : 1;
  const u = 1 - clamped;
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

/** Interpolate an ease segment at `t ∈ [0,1]` in Oklab, so the crossfade is
 * perceptually even (no lingering-bright sRGB midpoint, no muddy chroma path).
 * Segments carry a compiled pair (`compileLerpPair`) when both endpoints
 * parse — the always-case in practice, both being engine-emitted `#RRGGBB` —
 * so per-frame interpolation runs on pre-parsed Oklab coordinates instead of
 * re-parsing both endpoint strings every frame. A `null` pair falls back to
 * `oklabLerp`, which owns the unparseable-endpoint fallback semantics.
 * Byte-identical either way (locked by test/hotpath-parity.test.mjs). */
function segHex(seg, t) {
  return seg.pair ? lerpPairHex(seg.pair, t) : oklabLerp(seg.from, seg.to, t);
}

/** WCAG relative luminance of `segHex(seg, t)` — numeric fast path on the
 * compiled pair (no `#RRGGBB` round-trip), string path otherwise. Strict
 * mode's `floorBlend` bisection calls this up to 14× per role per frame. */
function segLum(seg, t) {
  return seg.pair ? lerpPairLuminance(seg.pair, t) : relativeLuminanceHex(segHex(seg, t));
}

/**
 * @typedef {object} AdaptController
 * @property {(now?: number) => void} tick  Drive one step (call from rAF, or let
 *   `start()` do it). Cheap: a re-check; a re-solve only on a sustained breach.
 * @property {(theme: string) => void} setTheme  Switch theme INSTANTLY (intent,
 *   not drift) — re-resolve and apply, bypassing the debounce/dwell machinery.
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
 * @param {{ resolveTheme: (bg:string,theme:string)=>any, recheckContrast:(bg:string,fgs:string[],theme:string)=>ArrayLike<number>, isStableGlowPointNoop?:(tint:string,bg:string)=>boolean }} options.colors
 * @param {string} options.theme
 * @param {string | string[] | (() => string | string[])} [options.background]
 *   explicit effective background. An ARRAY (or a function returning one) is a
 *   set of worst-case samples of a varying backdrop (gradient / image): the
 *   colours are held legible against the hardest sample. The caller does the
 *   pixel sampling; this consumes the samples worst-case.
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
  /** Stable Glow roles need an exact class recheck in addition to color
   * contrast rechecks. The only determinate stable state is the core-certified
   * quantised screen point no-op; every other state is typed Indeterminate and must have
   * no halo/core/alpha vars. */
  let stableGlows = [];
  /** Full canonical var set from the last solve — the `result.vars` of EVERY
   * reachable role (color AND translucent), each an oklch string. Every apply is
   * `applyTheme(target, {...baseVars, ...easedColorOverlay})`, so translucent
   * roles are never dropped by `applyTheme`'s clear-then-write and always carry
   * the current theme's value. Only `kind === "color"` roles (in `roles`) ease. */
  let baseVars = {};
  /** @type {Map<string,{from:string,to:string,held:number,pair:object|null}>} in-flight ease per cssVar */
  let easing = new Map();
  let easeStart = 0;
  let breachSince = null;
  let lastSolveAt = -Infinity;
  let lastKey = null;

  const readBackground = () => {
    const b = options.background;
    if (typeof b === "function") return b();
    if (typeof b === "string" || Array.isArray(b)) return b;
    return effectiveBackground(element, {
      fallback,
      getStyle: options.getStyle,
      parentOf: options.parentOf,
    });
  };

  // The background is a SET of samples. A solid surface is one sample; a varying
  // backdrop (gradient / image / video the caller sampled) is several. Every
  // decision is worst-case over the set: a role "passes" only if it passes
  // against EVERY sample, and we re-solve against the HARDEST sample. With one
  // sample this collapses to plain single-background behaviour, bit-for-bit.
  const readSamples = () => {
    const v = readBackground();
    const arr = Array.isArray(v) ? v.filter((s) => typeof s === "string" && s.length > 0) : [v];
    return arr.length > 0 ? arr : [fallback];
  };

  // Recheck the current colours against every sample. A role breaches if its
  // achieved |Lc| drops below its single-threshold margin (`|lc| * (1 -
  // dropFraction)`) against ANY sample. `worstIdx` is the sample with the least
  // set-wide margin — the one to re-solve against, so the constraint we solve to
  // is the same constraint we check hardest.
  // The current foreground hexes, refreshed on adopt. Rechecks run per changed
  // frame and previously rebuilt this identical array from `roles` each time.
  let fgsCache = [];

  // Batch path (many samples): collapse the shared per-foreground CAM16 forward
  // across every sample into ONE engine call. `recheckContrastMulti` returns a
  // background-major flat buffer where sample `s`, foreground `i` sits at
  // `(s * stride + i) * 2` (Lc) — the batched analogue of the per-sample loop's
  // `flat[2 * i]`. Byte-identical to N `recheckContrast` calls (locked by the
  // wasm boundary parity test), so the worst-margin / breach / worstIdx decision
  // below is bit-for-bit the same as the fallback loop.
  const canBatch =
    typeof colors.recheckContrastMulti === "function";

  const recheckSamples = (samples) => {
    let breached = false;
    let worstIdx = 0;
    let worstMargin = Infinity;
    const stride = fgsCache.length;
    const batch =
      canBatch && samples.length > 1
        ? colors.recheckContrastMulti(samples, fgsCache, theme)
        : null;
    for (let s = 0; s < samples.length; s++) {
      // Per-sample flat buffer, or a background-major window into the batch one.
      const flat = batch ? null : colors.recheckContrast(samples[s], fgsCache, theme);
      const base = s * stride;
      let sampleMargin = Infinity;
      for (let i = 0; i < roles.length; i++) {
        const want = Math.abs(roles[i].lc) * (1 - dropFraction);
        const lcNow = Math.abs(batch ? batch[(base + i) * 2] : flat[2 * i]);
        if (lcNow < want) breached = true;
        const margin = want > 0 ? lcNow / want : Infinity;
        if (margin < sampleMargin) sampleMargin = margin;
      }
      if (sampleMargin < worstMargin) {
        worstMargin = sampleMargin;
        worstIdx = s;
      }
    }
    return { breached, worstIdx };
  };

  const stableGlowsFrom = (result) => {
    const out = [];
    for (const [key, role] of Object.entries(result.roles ?? {})) {
      if (!role) continue;
      const isGlow = role.kind === "glow" || role.kind === "glow-indeterminate";
      if (!isGlow) {
        if (role.decisionProfile !== undefined) {
          throw new TypeError(`adaptTheme: decision profile on non-Glow role '${key}'`);
        }
        continue;
      }
      if (role.decisionProfile === "legacy-platform-dependent-v1") {
        if (role.kind !== "glow") {
          throw new TypeError(
            `adaptTheme: legacy Glow '${key}' cannot be Indeterminate`,
          );
        }
        continue;
      }
      if (role.decisionProfile !== "stable-v1") {
        throw new TypeError(`adaptTheme: Glow '${key}' lacks an explicit known decisionProfile`);
      }
      if (typeof role.cssVar !== "string") {
        throw new TypeError(`adaptTheme: stable Glow '${key}' lacks cssVar`);
      }
      const emittedKeys = [role.cssVar, `${role.cssVar}-core`, `${role.cssVar}-alpha`];
      if (role.kind === "glow") {
        if (
          role.decisionGuarantee?.kind !== "bit-exact" ||
          role.compositeProfile !== "encoded-srgb8-screen-v1" ||
          role.compositeGuarantee !== "bit-exact" ||
          role.diagnosticProfile !== null ||
          role.constraintLayer !== "halo" ||
          role.targetStatus !== "unreachable" ||
          typeof role.haloHex !== "string" ||
          emittedKeys.some((emittedKey) => typeof result.vars?.[emittedKey] !== "string")
        ) {
          throw new TypeError(`adaptTheme: stable Glow '${key}' lacks BitExact evidence`);
        }
        out.push({ key, cssVar: role.cssVar, sourceHex: role.haloHex, indeterminate: false });
        continue;
      }
      if (role.kind === "glow-indeterminate") {
        const expectedSite =
          role.numericalSiteId === "glow-target-or-maximum-v1" &&
          role.constraintLayer === "halo";
        const unavailable =
          role.reason === "sound-bound-unavailable" && role.bounds?.kind === "unavailable";
        const soundOverlap =
          role.reason === "interval-overlap" &&
          role.bounds?.kind === "outward" &&
          Number.isFinite(role.bounds.lower) &&
          Number.isFinite(role.bounds.upper) &&
          role.bounds.lower <= role.bounds.upper;
        const lawfulEvidence = expectedSite && (unavailable || soundOverlap);
        if (
          !lawfulEvidence ||
          typeof role.sourceHex !== "string" ||
          emittedKeys.some((emittedKey) => Object.hasOwn(result.vars ?? {}, emittedKey))
        ) {
          throw new TypeError(`adaptTheme: stable Glow '${key}' lacks lawful Indeterminate evidence`);
        }
        out.push({ key, cssVar: role.cssVar, sourceHex: role.sourceHex, indeterminate: true });
        continue;
      }
      throw new TypeError(`adaptTheme: stable decision profile on unknown Glow role '${key}'`);
    }
    return out;
  };

  // Adopt one already-resolved set as the current colours (no ease).
  const adoptResolved = (result, now) => {
    // Carry the FULL canonical var set (color + translucent) so no reachable
    // role is dropped by a subsequent apply; only color roles feed the ease.
    baseVars = result.vars && typeof result.vars === "object" ? result.vars : {};
    roles = Object.entries(result.roles)
      .filter(([, r]) => r && r.kind === "color")
      .map(([key, r]) => ({
        cssVar: r.cssVar,
        key,
        lc: r.lc,
        hex: r.hex,
        legalFloor: typeof r.legalFloor === "number" ? r.legalFloor : null,
      }));
    stableGlows = stableGlowsFrom(result);
    fgsCache = roles.map((r) => r.hex);
    // The adopt may change the var/role KEY SET — force the next write through
    // `applyTheme`'s full clear-then-write instead of the mid-ease diff path.
    written = null;
    lastSolveAt = now;
    breachSince = null;
    return result;
  };

  // Resolve a fresh set and adopt it as the current colours (no ease).
  const solveAndAdopt = (bg, now) => adoptResolved(colors.resolveTheme(bg, theme), now);

  // Solve+adopt against the hardest of `samples`. With one sample this is a
  // single solve; with several it does a provisional solve to learn the role
  // colours, picks the worst sample for them, and re-solves against it if that
  // is not the one already chosen — so the adopted set is the strongest the
  // backdrop demands. Used where there is no current set to recheck (initial
  // apply, theme switch); the tick path already knows the worst sample from its
  // own recheck and calls `solveAndAdopt` directly.
  const solveAndAdoptWorst = (samples, now) => {
    solveAndAdopt(samples[0], now);
    if (samples.length > 1) {
      const { worstIdx } = recheckSamples(samples);
      if (worstIdx !== 0) solveAndAdopt(samples[worstIdx], now);
    }
  };

  // Every write goes through the full canonical set with the (optional) eased
  // color overlay on top — so translucent roles in `baseVars` persist through
  // the apply, and non-eased color roles keep their canonical oklch form.
  // `overlay` carries only in-flight color roles as hex.
  //
  // WRITE STRATEGY — full vs diff. `written` holds the vars of the last write;
  // `null` forces the next write through `applyTheme`'s full clear-then-write.
  // Between two adopts the composed key set is invariant (always `baseVars`'
  // keys), so mid-ease frames DIFF against `written`: only values that changed
  // this frame hit `setProperty` (≈ the roles actually easing), instead of
  // remove+set of EVERY `--lab-*` var on every frame — the dominant DOM cost
  // of an ease and pure churn for the style engine. The final style state is
  // byte-identical to a full rewrite (locked by the golden fingerprints in
  // test/hotpath-parity.test.mjs). Every adopt nulls `written`, so key-set
  // changes and any external clobbering self-heal at the next solve — the same
  // guarantee the always-full-rewrite gave, which also wrote nothing between
  // eases while steady.
  let written = null;
  const applyHexes = (overlay) => {
    const vars = { ...baseVars, ...overlay };
    if (written === null) {
      applyTheme(target, { vars });
    } else {
      for (const k in vars) {
        const v = vars[k];
        if (written[k] !== v) target.style.setProperty(k, v);
      }
    }
    written = vars;
  };

  // Apply the canonical set as-is (no ease in flight): color roles show their
  // oklch form, translucent roles their tint+alpha.
  const applyRolesDirect = () => applyHexes({});

  const stableVarKeys = (role) => [
    role.cssVar,
    `${role.cssVar}-core`,
    `${role.cssVar}-alpha`,
  ];

  /**
   * Recheck the background-dependent stable Glow decision class. This is not a
   * contrast surplus and therefore never passes through sustain/dwell/easing.
   * A stable Glow result requires the core-owned exact predicate. Missing
   * capability is a typed integration error, never a hidden full-solve loop.
   */
  const reconcileStableGlows = (samples) => {
    if (stableGlows.length === 0) return false;

    if (typeof colors.isStableGlowPointNoop !== "function") {
      throw new TypeError(
        "adaptTheme: stable Glow requires colors.isStableGlowPointNoop",
      );
    }
    const desired = new Map();
    for (const role of stableGlows) {
      desired.set(
        role.key,
        samples.some((bg) => !colors.isStableGlowPointNoop(role.sourceHex, bg)),
      );
    }

    const changed = stableGlows.some(
      (role) => desired.get(role.key) !== role.indeterminate,
    );
    if (!changed) return false;

    // Re-resolve exactly once to refresh certificates/source metadata. Only
    // stable Glow satellites are adopted; color/translucent roles remain under
    // the existing adaptive contrast controller and do not snap.
    const fresh = colors.resolveTheme(samples[0], theme);
    const freshStable = stableGlowsFrom(fresh);
    const freshByKey = new Map(freshStable.map((role) => [role.key, role]));
    const nextVars = { ...baseVars };
    for (const previous of stableGlows) {
      const current = freshByKey.get(previous.key);
      if (!current) {
        throw new TypeError(`adaptTheme: stable Glow role '${previous.key}' disappeared`);
      }
      for (const key of stableVarKeys(previous)) delete nextVars[key];
      const indeterminate = desired.get(previous.key);
      if (!indeterminate) {
        for (const key of stableVarKeys(current)) {
          if (typeof fresh.vars?.[key] !== "string") {
            throw new TypeError(
              `adaptTheme: determinate stable Glow '${previous.key}' lacks '${key}'`,
            );
          }
          nextVars[key] = fresh.vars[key];
        }
      }
      current.indeterminate = indeterminate;
    }

    const previousVars = baseVars;
    baseVars = nextVars;
    stableGlows = freshStable.map((role) => {
      const state = desired.get(role.key);
      return state === undefined ? role : { ...role, indeterminate: state };
    });
    if (written !== null) {
      // A stable certificate transition may coincide with an in-flight color
      // ease. Patch only Glow satellites so the already-painted color overlay
      // remains continuous; resetting `written`/`easing` here would snap every
      // color role to its canonical destination.
      const nextWritten = { ...written };
      const stableKeys = new Set(
        stableGlows.flatMap((role) => stableVarKeys(role)),
      );
      for (const key of stableKeys) {
        if (typeof nextVars[key] === "string") {
          if (previousVars[key] !== nextVars[key] || written[key] !== nextVars[key]) {
            target.style.setProperty(key, nextVars[key]);
          }
          nextWritten[key] = nextVars[key];
        } else {
          target.style.removeProperty(key);
          delete nextWritten[key];
        }
      }
      written = nextWritten;
    }
    return true;
  };

  // Begin an ease from the currently-applied colours toward the role colours.
  // `held` latches the per-role displayed blend so it only ever advances toward
  // the destination (strict mode) — see `stepEase`.
  const beginEase = (fromByVar, now) => {
    easing = new Map();
    for (const r of roles) {
      const from = fromByVar[r.cssVar] ?? r.hex;
      if (from !== r.hex) {
        easing.set(r.cssVar, { from, to: r.hex, held: 0, pair: compileLerpPair(from, r.hex) });
      }
    }
    easeStart = now;
    if (easing.size === 0) applyRolesDirect();
  };

  // Strict mode: the least blend in [e, 1] whose interpolated colour clears
  // `floor` against EVERY background sample in `bgLums`. The destination (`to`,
  // blend 1) is a freshly-solved legal colour, so it anchors the search; we
  // bisect toward it from the natural ease value `e`. Returns `e` unchanged when
  // the eased colour is already legal everywhere (the common case — no
  // intervention). The returned blend is always floor-legal against the worst
  // sample, except in the unavoidable case where even `to` is illegal against a
  // sample that drifted further this frame — then it returns 1 (the most-legal
  // colour we have) and the recheck loop re-solves.
  const floorBlend = (seg, e, bgLums, floor) => {
    const legalAt = (blend) => {
      const lum = segLum(seg, blend);
      for (let i = 0; i < bgLums.length; i++) {
        if (wcagRatio(lum, bgLums[i]) < floor) return false;
      }
      return true;
    };
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

  // Per-key memo of the samples' WCAG luminances. Strict mode reads them in
  // both `stepEase` and `paintedNow` within a tick, and across consecutive
  // frames of a static backdrop mid-ease; the tick already computes the
  // samples key, so this costs one map per DISTINCT backdrop, not per call.
  let lumsKey = null;
  let lums = null;
  const bgLumsFor = (samples, key) => {
    if (key !== lumsKey) {
      lums = samples.map(wcagLuminanceCached);
      lumsKey = key;
    }
    return lums;
  };

  const stepEase = (now, samples, key) => {
    const t = easeMs <= 0 ? 1 : (now - easeStart) / easeMs;
    // Terminate the ease when it is done (`t >= 1`) OR when the clock went
    // non-finite (a NaN/±∞ `now` making `t` non-finite): drop the segments and
    // re-apply the canonical set, so the just-eased color roles snap back from
    // their interpolated hex to their oklch form (translucent roles stay put).
    // The non-finite guard is load-bearing for STATE, not just paint: without it
    // a persistently bad clock would leave `easing` in flight forever, freezing
    // color roles at their hex destination and never reverting to canonical
    // oklch. (`easeOut` separately guards the interpolation math from
    // `#NANNANNAN`; this guards the controller's easing state.)
    if (t >= 1 || !Number.isFinite(t)) {
      easing = new Map();
      applyRolesDirect();
      return;
    }
    const e = easeOut(t);
    const bgLums = strict ? bgLumsFor(samples, key) : null;
    // Overlay carries ONLY in-flight color roles (as interpolated hex); every
    // other role — non-eased color and all translucent — keeps its canonical
    // `baseVars` value under the merge in `applyHexes`.
    const overlay = {};
    for (const r of roles) {
      const seg = easing.get(r.cssVar);
      if (!seg) continue;
      let blend = e;
      if (strict && r.legalFloor != null) {
        // Hold the floor (against the worst sample), then LATCH: the displayed
        // blend may only advance toward the destination, never retreat.
        // `floorBlend` is stateless and depends on the live (drifting) samples,
        // so on a frame where they drift favourably it could return a *lower*
        // blend than last frame — a backwards step toward the old colour, the
        // precise jarring reversal this mode exists to avoid. `held` clamps that
        // out: the colour progresses monotonically from→to and never below the
        // legal line on any sample.
        blend = Math.max(floorBlend(seg, e, bgLums, r.legalFloor), seg.held);
        seg.held = blend;
      }
      overlay[r.cssVar] = segHex(seg, blend);
    }
    applyHexes(overlay);
  };

  // The full applied picture: the canonical set (color + translucent) with each
  // in-flight color role reported at its LOGICAL target (`seg.to`), so a reader
  // sees where the ease is going, not a mid-transition frame.
  const currentApplied = () => {
    const vars = { ...baseVars };
    for (const r of roles) {
      const seg = easing.get(r.cssVar);
      if (seg) vars[r.cssVar] = seg.to;
    }
    return vars;
  };

  // The colour each role is PAINTED right now — exactly what `stepEase` writes
  // this frame: an in-flight segment sampled at `now` (with the SAME strict
  // floor-hold + latch against the worst sample when `strict`), else the static
  // hex. Mirrors `stepEase`'s blend math byte-for-byte so the begin-from value
  // equals what is on screen, including the strict-mode `held` clamp — otherwise
  // an overlapping re-solve in strict mode would start one frame BELOW the
  // painted (floored) colour.
  const paintedNow = (now, samples, key) => {
    const t = easeMs <= 0 ? 1 : (now - easeStart) / easeMs;
    const e = easeOut(t);
    const bgLums = strict ? bgLumsFor(samples, key) : null;
    const vars = {};
    for (const r of roles) {
      const seg = easing.get(r.cssVar);
      if (!seg) {
        vars[r.cssVar] = r.hex;
        continue;
      }
      const blend =
        strict && r.legalFloor != null
          ? Math.max(floorBlend(seg, e, bgLums, r.legalFloor), seg.held)
          : e;
      vars[r.cssVar] = segHex(seg, blend);
    }
    return vars;
  };

  const tick = (nowArg) => {
    const now = nowArg ?? clock();
    const samples = readSamples();
    const key = samples.join("|");
    // Advance any in-flight ease first (against the live samples, so strict mode
    // holds the legal floor every frame as the backdrop keeps drifting under it).
    if (easing.size > 0) stepEase(now, samples, key);

    // Steady state: a static backdrop with no in-flight ease and no pending
    // breach needs no work. A PENDING breach keeps us live even on a static
    // backdrop, so the sustain timer can fire on one that changed once to a
    // failing value and then held.
    if (key === lastKey && easing.size === 0 && breachSince === null) return;
    if (key !== lastKey) reconcileStableGlows(samples);
    lastKey = key;
    if (roles.length === 0) return;

    // Cheap worst-case re-check: do the current colours still pass against every
    // sample? `worstIdx` is the hardest sample, the one to re-solve against.
    const { breached, worstIdx } = recheckSamples(samples);

    if (!breached) {
      breachSince = null;
      return; // hold — the common case for a slowly-drifting backdrop
    }
    if (breachSince === null) breachSince = now;
    if (now - breachSince < sustainMs || now - lastSolveAt < dwellMs) return; // debounce / dwell

    // Sustained breach: re-solve against the worst sample and ease toward fresh,
    // starting from the colour each role is PAINTED right now (the in-flight ease
    // sampled at `now`) — never the in-flight TARGET. Starting from the target
    // would SNAP the element to the old target for one frame before easing,
    // reintroducing flicker when a re-solve overlaps a previous ease.
    const fromByVar = paintedNow(now, samples, key);
    solveAndAdopt(samples[worstIdx], now);
    reconcileStableGlows(samples);
    beginEase(fromByVar, now);
    stepEase(now, samples, key);
  };

  let rafId = null;
  const loop = () => {
    tick();
    if (win?.requestAnimationFrame) rafId = win.requestAnimationFrame(loop);
  };

  // Apply the initial set immediately (against the worst sample of the backdrop).
  {
    const samples = readSamples();
    lastKey = samples.join("|");
    solveAndAdoptWorst(samples, clock());
    reconcileStableGlows(samples);
    applyRolesDirect();
  }

  return {
    tick,
    setTheme(next) {
      theme = next;
      const samples = readSamples();
      lastKey = samples.join("|");
      easing = new Map();
      solveAndAdoptWorst(samples, clock());
      reconcileStableGlows(samples);
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
