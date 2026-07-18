// Adaptive theme controller. Zero dependencies.
//
// Each `tick` reads one finite, caller-declared sample set. An unchanged sample
// key with no pending breach/ease skips metric evaluation. Otherwise the
// controller compares the current resolved colours with the returned Lc/WCAG
// metrics, waits for the configured relative-drop interval, then resolves again
// and interpolates colour-role coordinates toward the new target.
//
// These are runtime mechanics, not a whole-field or human-readability proof.
// `dropFraction`, `sustainMs`, `dwellMs`, `easeMs`, and the shorter transition
// selected for the host motion preference are compatibility parameters, not
// standard-derived thresholds. Default easing does not verify a floor on every
// frame. `strict: true` enables the characterized per-frame clamp, whose current
// Oklab→clip→sRGB8 path is not globally monotone and is not a floor certificate.

import {
  effectiveBackground,
  parseCssColor,
  oklabLerp,
  compileLerpPair,
  lerpPairHex,
  lerpPairLuminance,
  wcagLuminanceCached,
} from "./effective-bg.js";
import { admitSnapshot, writeVars } from "./snapshot.js";

/** Cubic ease-out: fast start, gentle settle, no overshoot. A non-finite `t`
 * (e.g. a NaN clock making `(now - easeStart) / easeMs` NaN) is treated as a
 * completed ease (1), so the crossfade can never emit `#NANNANNAN` CSS. */
function easeOut(t) {
  const clamped = Number.isFinite(t) ? Math.min(1, Math.max(0, t)) : 1;
  const u = 1 - clamped;
  return 1 - u * u * u;
}

/** Relative luminance of `#RRGGBB` in the frozen original WCAG 2.1 (2018)
 * profile (0.03928 split, 2.4 exponent), so the strict floor-clamp agrees
 * byte-for-byte with the core's versioned `legalFloor` semantics. */
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

/** Linearly interpolate an ease segment's Oklab coordinates at `t ∈ [0,1]`.
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
 *   `start()` do it). Unchanged idle input exits before metric evaluation; a
 *   re-solve occurs only on a sustained breach.
 * @property {(theme: string) => void} setTheme  Switch theme INSTANTLY (intent,
 *   not drift) — re-resolve and apply, bypassing the debounce/dwell machinery.
 * @property {() => void} start  Begin an internal requestAnimationFrame loop.
 * @property {() => void} stop   Stop the internal loop without discarding an
 *   незавершённый ease; последующий `start()`/`tick()` продолжит его по своим часам.
 * @property {() => Record<string,string>} current  Canonical resolved targets;
 *   during an ease these differ from the values painted into the DOM.
 */

/**
 * Adapt an element's `--lab-*` variables to a finite declared sample set.
 * Applies the first resolved set immediately; later metric evaluation occurs
 * only for changed samples or pending controller state, and a sustained relative
 * drop starts a new resolve plus coordinate interpolation.
 *
 * @param {*} element
 * @param {object} options
 * @param {{ resolveTheme: (bg:string,theme:string)=>any, recheckContrast:(bg:string,fgs:string[],theme:string)=>ArrayLike<number>, isStableGlowPointNoop?:(tint:string,bg:string)=>boolean }} options.colors
 * @param {string} options.theme
 * @param {string | string[] | (() => string | string[])} [options.background]
 *   explicit background evidence. An ARRAY (or a function returning one) is a
 *   finite sample set for a varying backdrop (gradient / image). The caller owns
 *   sampling; the controller compares only those points and uses the lowest
 *   returned metric, without inferring the field between them.
 * @param {*} [options.target=element]  element to write vars onto
 * @param {string} [options.fallback="#FFFFFF"]
 * @param {number} [options.dropFraction=0.2]  surplus fraction lost before re-solve
 * @param {number} [options.sustainMs=120]  breach must persist this long
 * @param {number} [options.dwellMs=250]  minimum between re-solves
 * @param {number} [options.easeMs=280]  crossfade duration
 * @param {boolean} [options.strict=false]  enable the legacy characterized
 *   per-frame clamp; the current non-monotone interpolation path is not a
 *   universal floor certificate
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
   * reachable role (color AND translucent), each an oklch string. Every write
   * composes `{...baseVars, ...easedColorOverlay}`, so translucent roles are
   * never dropped by clear-then-write. Only `kind === "color"` roles ease. */
  let baseVars = {};
  /** @type {Map<string,{from:string,to:string,held:number,pair:object|null}>} in-flight ease per cssVar */
  let easing = new Map();
  let easeStart = 0;
  let breachSince = null;
  let lastSolveAt = -Infinity;
  let lastKey = null;
  // Владение цветовой операцией отдельно от владения rAF: любой более новый
  // tick/setTheme (и stop до начала commit) делает старую подготовку
  // недействительной. Уже начатая синхронная CSS-запись завершается целиком:
  // у CSSOM нет rollback, а остановка посередине оставила бы hybrid snapshot.
  let operationGeneration = 0;
  let commitDepth = 0;
  const beginOperation = () => ++operationGeneration;
  const ownsOperation = (owner) => owner === operationGeneration;

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

  const recheckSamples = (
    samples,
    roleSet = roles,
    foregrounds = fgsCache,
    themeName = theme,
  ) => {
    let breached = false;
    let worstIdx = 0;
    let worstMargin = Infinity;
    const stride = foregrounds.length;
    const batch =
      canBatch && samples.length > 1
        ? colors.recheckContrastMulti(samples, foregrounds, themeName)
        : null;
    for (let s = 0; s < samples.length; s++) {
      // Per-sample flat buffer, or a background-major window into the batch one.
      const flat = batch
        ? null
        : colors.recheckContrast(samples[s], foregrounds, themeName);
      if (!batch && (flat?.length ?? -1) !== roleSet.length * 2) {
        throw new RangeError(
          "adaptTheme: recheckContrast вернул буфер неверной длины " +
            `(${flat?.length} вместо ${roleSet.length * 2}) — ` +
            "битый результат нельзя молча принять за отсутствие пробоя",
        );
      }
      const base = s * stride;
      let sampleMargin = Infinity;
      for (let i = 0; i < roleSet.length; i++) {
        const want = Math.abs(roleSet[i].lc) * (1 - dropFraction);
        const lcRaw = batch ? batch[(base + i) * 2] : flat[2 * i];
        if (!Number.isFinite(lcRaw)) {
          throw new RangeError(
            `adaptTheme: recheckContrast отдал нефинитный Lc (${lcRaw}) — ` +
              "NaN-сравнения молча заморозили бы устаревшие цвета",
          );
        }
        const lcNow = Math.abs(lcRaw);
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

  const stableVarKeys = (role) => [
    role.cssVar,
    `${role.cssVar}-core`,
    `${role.cssVar}-alpha`,
  ];

  const hasDeterminateGlowEnvelope = (result, role, emittedKeys) =>
    role.compositeProfile === "encoded-srgb8-screen-v1" &&
    role.compositeGuarantee === "bit-exact" &&
    role.layerRecipeProfile === "cam16-jprime-oklab-cusp-v1" &&
    role.appearanceDiagnosticProfile === "cam16-ucs-jprime-li2017-v1" &&
    role.constraintLayer === "halo" &&
    typeof role.coreHex === "string" &&
    typeof role.haloHex === "string" &&
    emittedKeys.every((emittedKey) => typeof result.vars?.[emittedKey] === "string");

  const validatedStableGlowsFrom = (result) => {
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
      const expectedCssVar = `--lab-${key}`;
      if (role.cssVar !== expectedCssVar) {
        throw new TypeError(
          `adaptTheme: Glow '${key}' has non-canonical cssVar`,
        );
      }
      const emittedKeys = stableVarKeys(role);
      if (role.decisionProfile === "legacy-platform-dependent-v1") {
        if (role.kind !== "glow") {
          throw new TypeError(
            `adaptTheme: legacy Glow '${key}' cannot be Indeterminate`,
          );
        }
        const reached = role.targetStatus === "legacy-reached";
        const unreachable = role.targetStatus === "legacy-unreachable";
        if (
          !hasDeterminateGlowEnvelope(result, role, emittedKeys) ||
          role.decisionGuarantee?.kind !== "legacy-platform-dependent-v1" ||
          role.selectionDiagnosticProfile !== "cam16-ucs-jprime-li2017-v1" ||
          (!reached && !unreachable)
        ) {
          throw new TypeError(
            `adaptTheme: legacy Glow '${key}' lacks lawful legacy evidence`,
          );
        }
        continue;
      }
      if (role.decisionProfile !== "stable-v1") {
        throw new TypeError(`adaptTheme: Glow '${key}' lacks an explicit known decisionProfile`);
      }
      if (role.kind === "glow") {
        if (
          role.decisionGuarantee?.kind !== "bit-exact" ||
          !hasDeterminateGlowEnvelope(result, role, emittedKeys) ||
          role.selectionDiagnosticProfile !== null ||
          role.targetStatus !== "exact-noop-unreachable"
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

  // Сначала собрать неизменяемый кандидат: состояние контроллера и DOM не
  // меняются, пока каждый шаг транзакции (resolve/recheck/сертификат) не пройдёт.
  const resolvedCandidate = (result, now) => {
    const snapshot = admitSnapshot(result, "adaptTheme");
    const nextStableGlows = validatedStableGlowsFrom(snapshot);
    const nextBaseVars = snapshot.vars;
    const nextRoles = Object.entries(snapshot.roles)
      .filter(([, r]) => r && r.kind === "color")
      .map(([key, r]) => ({
        cssVar: r.cssVar,
        key,
        lc: r.lc,
        hex: r.hex,
        legalFloor: typeof r.legalFloor === "number" ? r.legalFloor : null,
      }));
    return {
      result: snapshot,
      baseVars: nextBaseVars,
      roles: nextRoles,
      stableGlows: nextStableGlows,
      fgsCache: nextRoles.map((r) => r.hex),
      lastSolveAt: now,
      breachSince: null,
    };
  };

  const commitResolved = (candidate, owner) => {
    if (!ownsOperation(owner)) return false;
    baseVars = candidate.baseVars;
    roles = candidate.roles;
    stableGlows = candidate.stableGlows;
    fgsCache = candidate.fgsCache;
    lastSolveAt = candidate.lastSolveAt;
    breachSince = candidate.breachSince;
    // Пере-решённый кандидат может сменить набор ключей: следующая запись
    // обязана идти полным clear-then-write, но лишь после коммита транзакции.
    written = null;
    return true;
  };

  const solveCandidate = (bg, now, themeName) =>
    resolvedCandidate(colors.resolveTheme(bg, themeName), now);

  // Choose an initial result from `samples`. With one sample this is a single
  // solve. With several, solve against the first sample, evaluate that
  // provisional role set over every supplied sample, then re-solve at most once
  // against its lowest-metric sample. The second result is not rechecked over
  // the set, so this is a bounded initialization heuristic, not a final
  // worst-sample certificate. The tick path instead starts from its own current
  // результата и пере-решает по худшему (минимальная метрика) из образцов.
  const solveWorstCandidate = (samples, now, themeName) => {
    const sample0 = solveCandidate(samples[0], now, themeName);
    let candidate = sample0;
    if (samples.length > 1) {
      const { worstIdx } = recheckSamples(
        samples,
        candidate.roles,
        candidate.fgsCache,
        themeName,
      );
      if (worstIdx !== 0) {
        candidate = solveCandidate(samples[worstIdx], now, themeName);
      }
    }
    return { candidate, sample0Result: sample0.result };
  };

  // Every write goes through the full canonical set with the (optional) eased
  // color overlay on top — so translucent roles in `baseVars` persist through
  // the apply, and non-eased color roles keep their canonical oklch form.
  // `overlay` carries only in-flight color roles as hex.
  //
  // WRITE STRATEGY — full vs diff. `written` holds the vars of the last write;
  // `null` принуждает следующую запись пройти полный clear-then-write.
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
  const loseWriteOwnership = (basis) => {
    // Если более новая операция не успела опубликовать собственный полный
    // снимок, частично записанная физическая база неизвестна и требует repair.
    if (written === basis) written = null;
    return false;
  };
  const applyHexes = (overlay, owner) => {
    if (!ownsOperation(owner)) return false;
    const vars = { ...baseVars, ...overlay };
    const basis = written;
    commitDepth++;
    try {
      try {
        if (written === null) {
          if (!writeVars(target, vars, "adaptTheme", () => ownsOperation(owner))) {
            return loseWriteOwnership(basis);
          }
        } else {
          for (const k in vars) {
            if (!ownsOperation(owner)) return loseWriteOwnership(basis);
            const v = vars[k];
            if (basis[k] !== v) target.style.setProperty(k, v);
            if (!ownsOperation(owner)) return loseWriteOwnership(basis);
          }
        }
      } catch (error) {
        // У CSSOM нет транзакций/отката. Забываем diff-базу, чтобы следующий
        // явный tick переписал весь канонический снимок, а не считал частично
        // записанный DOM закоммиченным.
        if (ownsOperation(owner)) written = null;
        throw error;
      }
      if (!ownsOperation(owner)) return loseWriteOwnership(basis);
      written = vars;
      return true;
    } finally {
      commitDepth--;
    }
  };

  // Apply the canonical set as-is (no ease in flight): color roles show their
  // oklch form, translucent roles their tint+alpha.
  const applyRolesDirect = (owner) => applyHexes({}, owner);

  /**
   * Recheck the background-dependent stable Glow decision class. This is not a
   * contrast surplus and therefore never passes through sustain/dwell/easing.
   * A stable Glow result requires the core-owned exact predicate. Missing
   * capability is a typed integration error, never a hidden full-solve loop.
   */
  const prepareStableGlowReconciliation = (
    samples,
    state,
    themeName,
    sample0Result = null,
  ) => {
    if (state.stableGlows.length === 0) return null;

    if (typeof colors.isStableGlowPointNoop !== "function") {
      throw new TypeError(
        "adaptTheme: stable Glow requires colors.isStableGlowPointNoop",
      );
    }
    const desired = new Map();
    for (const role of state.stableGlows) {
      desired.set(
        role.key,
        samples.some((bg) => !colors.isStableGlowPointNoop(role.sourceHex, bg)),
      );
    }

    const changed = state.stableGlows.some(
      (role) => desired.get(role.key) !== role.indeterminate,
    );
    if (!changed) return null;

    // Re-resolve exactly once to refresh certificates/source metadata. Only
    // stable Glow satellites are adopted; color/translucent roles remain under
    // the existing adaptive contrast controller and do not snap.
    const fresh = admitSnapshot(
      sample0Result ?? colors.resolveTheme(samples[0], themeName),
      "adaptTheme",
    );
    const freshStable = validatedStableGlowsFrom(fresh);
    const freshByKey = new Map(freshStable.map((role) => [role.key, role]));
    const nextVars = { ...state.baseVars };
    for (const previous of state.stableGlows) {
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
    }

    const nextStableGlows = freshStable.map((role) => {
      const state = desired.get(role.key);
      return state === undefined ? role : { ...role, indeterminate: state };
    });
    return { baseVars: nextVars, stableGlows: nextStableGlows };
  };

  const withStableGlowReconciliation = (
    candidate,
    samples,
    themeName,
    sample0Result = null,
  ) => {
    const prepared = prepareStableGlowReconciliation(
      samples,
      candidate,
      themeName,
      sample0Result,
    );
    return prepared === null ? candidate : { ...candidate, ...prepared };
  };

  const commitStableGlowReconciliation = (prepared, owner) => {
    if (!ownsOperation(owner)) return false;
    if (prepared === null) return true;
    const previousVars = baseVars;
    baseVars = prepared.baseVars;
    stableGlows = prepared.stableGlows;
    if (written !== null) {
      // A stable certificate transition may coincide with an in-flight color
      // ease. Patch only Glow satellites so the already-painted color overlay
      // remains continuous; resetting `written`/`easing` here would snap every
      // color role to its canonical destination.
      const nextWritten = { ...written };
      const stableKeys = new Set(
        stableGlows.flatMap((role) => stableVarKeys(role)),
      );
      commitDepth++;
      try {
        for (const key of stableKeys) {
          if (!ownsOperation(owner)) return false;
          if (typeof prepared.baseVars[key] === "string") {
            if (
              previousVars[key] !== prepared.baseVars[key] ||
              written[key] !== prepared.baseVars[key]
            ) {
              target.style.setProperty(key, prepared.baseVars[key]);
            }
            nextWritten[key] = prepared.baseVars[key];
          } else {
            target.style.removeProperty(key);
            delete nextWritten[key];
          }
          if (!ownsOperation(owner)) return false;
        }
      } catch (error) {
        if (ownsOperation(owner)) written = null;
        throw error;
      } finally {
        commitDepth--;
      }
      if (!ownsOperation(owner)) return false;
      written = nextWritten;
    }
    return true;
  };

  // Begin an ease from the currently-applied colours toward the role colours.
  // `held` latches the per-role displayed blend so it only ever advances toward
  // the destination (strict mode) — see `stepEase`.
  const prepareEase = (roleSet, fromByVar, now) => {
    const nextEasing = new Map();
    for (const r of roleSet) {
      const from = fromByVar[r.cssVar] ?? r.hex;
      if (from !== r.hex) {
        nextEasing.set(r.cssVar, {
          from,
          to: r.hex,
          held: 0,
          pair: compileLerpPair(from, r.hex),
        });
      }
    }
    return { easing: nextEasing, easeStart: now };
  };

  const commitEase = (prepared, owner) => {
    if (!ownsOperation(owner)) return false;
    easing = prepared.easing;
    easeStart = prepared.easeStart;
    return easing.size === 0 ? applyRolesDirect(owner) : true;
  };

  // Legacy strict clamp: fixed-step bisection from the natural ease value `e`
  // toward the freshly-solved destination. Oklab→clip→sRGB8 legality is not
  // globally monotone, so this is a characterized compatibility selector, not
  // a proof of the least or universally legal blend. If even `to` fails after
  // background drift, the selector returns 1 and the recheck loop requests
  // another solve.
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
    return hi; // upper search bound, or 1 when the destination also fails
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

  const stepEase = (now, samples, key, owner) => {
    if (!ownsOperation(owner)) return false;
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
      return applyRolesDirect(owner);
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
        // out: the scalar blend parameter never retreats. This latch alone is
        // not a proof that the quantized colour stays above every floor.
        blend = Math.max(floorBlend(seg, e, bgLums, r.legalFloor), seg.held);
        seg.held = blend;
      }
      overlay[r.cssVar] = segHex(seg, blend);
    }
    return applyHexes(overlay, owner);
  };

  const easeCompletesAt = (now) => {
    const t = easeMs <= 0 ? 1 : (now - easeStart) / easeMs;
    return t >= 1 || !Number.isFinite(t);
  };

  // The canonical target picture (color + translucent). An in-flight colour role
  // is reported at `seg.to`, not at the value currently painted into the DOM.
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

  const runTick = (nowArg) => {
    const owner = beginOperation();
    const now = nowArg ?? clock();
    if (!ownsOperation(owner)) return;
    if (!Number.isFinite(now)) {
      throw new RangeError(
        `adaptTheme: часы обязаны быть конечными, получено ${now} — ` +
          "нефинитное время отравило бы breach/ease-тайминг навсегда",
      );
    }
    const samples = readSamples();
    if (!ownsOperation(owner)) return;
    const key = samples.join("|");
    const hasEase = easing.size > 0;

    // Завершившийся ease на неизменном idle-образце не содержит fallible-
    // работы: финализируем напрямую, сохраняя старый fast-path без recheck.
    if (
      key === lastKey &&
      breachSince === null &&
      (!hasEase || easeCompletesAt(now))
    ) {
      if (hasEase) stepEase(now, samples, key, owner);
      else if (written === null) applyRolesDirect(owner);
      return;
    }

    // Glow-only набор всё равно реагирует на смену подложки. Готовим точный
    // class-переход до публикации и ключа, и CSS-состояния.
    if (roles.length === 0) {
      const preparedGlow =
        key === lastKey
          ? null
          : prepareStableGlowReconciliation(
              samples,
              { baseVars, stableGlows },
              theme,
            );
      if (!ownsOperation(owner)) return;
      if (!commitStableGlowReconciliation(preparedGlow, owner)) return;
      lastKey = key;
      if (hasEase) stepEase(now, samples, key, owner);
      else if (written === null) applyRolesDirect(owner);
      return;
    }

    // Compare the current colours with every declared sample. `worstIdx` is the
    // lowest-metric sample, the one used for the next resolve.
    const { breached, worstIdx } = recheckSamples(samples);
    if (!ownsOperation(owner)) return;
    let nextBreachSince = breachSince;

    if (!breached) {
      nextBreachSince = null;
    } else if (nextBreachSince === null) {
      nextBreachSince = now;
    }

    const shouldResolve =
      breached &&
      now - nextBreachSince >= sustainMs &&
      now - lastSolveAt >= dwellMs;

    if (!shouldResolve) {
      const preparedGlow =
        key === lastKey
          ? null
          : prepareStableGlowReconciliation(
              samples,
              { baseVars, stableGlows },
              theme,
            );
      if (!ownsOperation(owner)) return;
      // Вся работа resolver/recheck/Glow-валидации успешна. Только теперь
      // публикуем сертификатный переход и учёт контроллера, затем двигаем
      // текущий ease по тому же снимку образцов.
      if (!commitStableGlowReconciliation(preparedGlow, owner)) return;
      lastKey = key;
      breachSince = nextBreachSince;
      if (hasEase) stepEase(now, samples, key, owner);
      else if (written === null) applyRolesDirect(owner);
      return;
    }

    // Sustained breach: re-solve against the worst sample and ease toward fresh,
    // starting from the colour each role is PAINTED right now (the in-flight ease
    // sampled at `now`) — never the in-flight TARGET. Starting from the target
    // would SNAP the element to the old target for one frame before easing,
    // reintroducing flicker when a re-solve overlaps a previous ease.
    const fromByVar = paintedNow(now, samples, key);
    let candidate = solveCandidate(samples[worstIdx], now, theme);
    if (!ownsOperation(owner)) return;
    candidate = withStableGlowReconciliation(
      candidate,
      samples,
      theme,
      worstIdx === 0 ? candidate.result : null,
    );
    if (!ownsOperation(owner)) return;
    const preparedEase = prepareEase(candidate.roles, fromByVar, now);
    // До этой точки ни состояние контроллера, ни DOM не менялись. Публикуем
    // решённого кандидата, ease и ключ образцов одной commit-фазой.
    if (!commitResolved(candidate, owner)) return;
    if (!commitEase(preparedEase, owner)) return;
    lastKey = key;
    stepEase(now, samples, key, owner);
  };

  let rafId = null;
  let running = false;
  let frameEpoch = 0;
  let frameCallback = null;
  const queueNextFrame = (callback) => {
    try {
      rafId = win.requestAnimationFrame(callback);
    } catch (error) {
      running = false;
      rafId = null;
      frameCallback = null;
      frameEpoch++;
      throw error;
    }
  };
  const runFrame = (epoch) => {
    // Callback может пережить враждебную/несработавшую отмену. Владение
    // эпохой держит его инертным и, главное, не даёт ему гасить или
    // продлевать цикл, запущенный позже.
    if (!running || epoch !== frameEpoch) return;
    rafId = null;
    try {
      tick();
    } catch (error) {
      // Гасить можно только СВОЮ эпоху: если tick() внутри успел сделать
      // stop()/start(), новая эпоха уже владеет циклом, и падение старого
      // кадра не смеет её глушить.
      if (epoch === frameEpoch) {
        running = false;
        frameCallback = null;
        frameEpoch++;
      }
      throw error;
    }
    if (
      running &&
      epoch === frameEpoch &&
      rafId === null &&
      frameCallback !== null &&
      win?.requestAnimationFrame
    ) {
      queueNextFrame(frameCallback);
    }
  };

  // Apply the initial set immediately (against the worst sample of the backdrop).
  {
    const owner = beginOperation();
    const samples = readSamples();
    const nextKey = samples.join("|");
    const now = clock();
    const prepared = solveWorstCandidate(samples, now, theme);
    let candidate = withStableGlowReconciliation(
      prepared.candidate,
      samples,
      theme,
      prepared.sample0Result,
    );
    if (!commitResolved(candidate, owner)) {
      throw new Error("adaptTheme: initial operation lost ownership");
    }
    lastKey = nextKey;
    applyRolesDirect(owner);
  }

  const pendingOperations = [];
  let drainingOperations = false;

  const runSetTheme = (next) => {
    const owner = beginOperation();
    const samples = readSamples();
    if (!ownsOperation(owner)) return;
    const nextKey = samples.join("|");
    const now = clock();
    if (!ownsOperation(owner)) return;
    const prepared = solveWorstCandidate(samples, now, next);
    if (!ownsOperation(owner)) return;
    const candidate = withStableGlowReconciliation(
      prepared.candidate,
      samples,
      next,
      prepared.sample0Result,
    );
    if (!ownsOperation(owner)) return;
    // Вся до-записьная работа resolver/recheck/evidence завершена.
    // Публикуем новую тему и решённое состояние, затем — фаза CSSOM-записи.
    theme = next;
    lastKey = nextKey;
    easing = new Map();
    if (!commitResolved(candidate, owner)) return;
    applyRolesDirect(owner); // instant — a theme switch is intent, not drift
  };

  const drainOperations = () => {
    if (drainingOperations || commitDepth > 0) return;
    drainingOperations = true;
    try {
      while (pendingOperations.length > 0) {
        const operation = pendingOperations.shift();
        if (operation.kind === "tick") runTick(operation.now);
        else runSetTheme(operation.theme);
      }
    } catch (error) {
      // Одна внешняя операция и её reentrant FIFO — одна serial transaction.
      // После первого отказа старый suffix не смеет пережить transaction и
      // перезаписать более новое намерение при следующем публичном вызове.
      pendingOperations.length = 0;
      throw error;
    } finally {
      drainingOperations = false;
    }
  };

  const runPublicOperation = (kind, value) => {
    try {
      if (kind === "tick") runTick(value);
      else runSetTheme(value);
    } catch (error) {
      // Первичная ошибка важнее невыполненного queued suffix; drain из finally
      // заменил бы её вторичным исключением и оставил бы хвост жить дальше.
      pendingOperations.length = 0;
      throw error;
    }
    drainOperations();
  };

  const tick = (nowArg) => {
    if (commitDepth > 0) {
      pendingOperations.push({ kind: "tick", now: nowArg });
      return;
    }
    if (drainingOperations) {
      // Prepare reentrancy is newer than the not-yet-run FIFO suffix. Enqueue
      // chronologically and revoke the active uncommitted candidate now.
      pendingOperations.push({ kind: "tick", now: nowArg });
      operationGeneration++;
      return;
    }
    runPublicOperation("tick", nowArg);
  };

  const setTheme = (next) => {
    if (commitDepth > 0) {
      pendingOperations.push({ kind: "theme", theme: next });
      return;
    }
    if (drainingOperations) {
      pendingOperations.push({ kind: "theme", theme: next });
      operationGeneration++;
      return;
    }
    runPublicOperation("theme", next);
  };

  return {
    tick,
    setTheme,
    start() {
      if (!running && win?.requestAnimationFrame) {
        running = true;
        const epoch = ++frameEpoch;
        frameCallback = () => runFrame(epoch);
        queueNextFrame(frameCallback);
      }
    },
    stop() {
      if (commitDepth === 0) operationGeneration++;
      pendingOperations.length = 0;
      running = false;
      frameEpoch++;
      frameCallback = null;
      const pending = rafId;
      rafId = null;
      if (pending != null && win?.cancelAnimationFrame) win.cancelAnimationFrame(pending);
    },
    current: currentApplied,
  };
}
