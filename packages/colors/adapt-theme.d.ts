// Public types for the adaptive hysteresis controller.

import type { LabColors, ThemeName } from "./index.js";

export interface AdaptThemeOptions {
  /**
   * An initialised engine — needs resolve + contrast recheck. The exact
   * `isStableGlowPointNoop` capability is conditionally required when a result
   * contains a stable Glow role; its absence then fails explicitly.
   * `recheckContrastMulti` is optional: when present, a multi-sample backdrop
   * is rechecked in ONE batched call per frame (byte-identical to the
   * per-sample loop, locked by the wasm boundary parity test); when absent,
   * the controller falls back to N `recheckContrast` calls.
   */
  colors: Pick<LabColors, "resolveTheme" | "recheckContrast"> &
    Partial<Pick<LabColors, "recheckContrastMulti" | "isStableGlowPointNoop">>;
  theme: ThemeName;
  /**
   * Explicit background evidence, overriding the ancestor reference estimate.
   * A single hex is one solid surface; an array (or a function returning one)
   * is a finite, caller-supplied sample set for a varying backdrop (gradient /
   * image / video). The controller compares every supplied point and bases its
   * decision on the worst returned metric; it does not infer between samples
   * or observe the whole field. With one sample this is identical to plain
   * single-background mode.
   */
  background?: string | string[] | (() => string | string[]);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /** Base colour when the ancestor chain is fully translucent. Default `"#FFFFFF"`. */
  fallback?: string;
  /** Fraction of a role's contrast surplus that may be lost before a re-solve. Default `0.2`. */
  dropFraction?: number;
  /** A breach must persist this many ms before re-solving (debounce). Default `120`. */
  sustainMs?: number;
  /** Minimum ms between re-solves (dwell / rate cap). Default `250`. */
  dwellMs?: number;
  /** Crossfade duration in ms. Default `280` (capped to a short fade under reduced motion). */
  easeMs?: number;
  /**
   * Enable the legacy characterized per-frame clamp. The current
   * Oklab→clip→sRGB8 path is not globally monotone, so this option is not a
   * universal floor/least-blend or legibility certificate. Use it only when an
   * integration explicitly needs the characterized legacy clamp. Default
   * `false`.
   */
  strict?: boolean;
  /** Override reduced-motion detection (default reads `matchMedia`). */
  reducedMotion?: boolean;
  /** Clock injection (default `performance.now`/`Date.now`). */
  now?: () => number;
  /** Window-like host (rAF, matchMedia). Defaults to `globalThis`. */
  win?: Window;
  /** Injection seam for the computed style of an element (testing). */
  getStyle?: (element: unknown) => { getPropertyValue(property: string): string };
  /** Injection seam for an element's parent (testing). */
  parentOf?: (element: unknown) => unknown;
}

export interface AdaptController {
  /** Drive one step. Cheap (a re-check); re-solves only on a sustained breach. */
  tick(now?: number): void;
  /** Switch theme INSTANTLY (intent, not drift) — bypasses the hysteresis. */
  setTheme(theme: ThemeName): void;
  /** Begin an internal `requestAnimationFrame` loop. */
  start(): void;
  /** Stop the loop. */
  stop(): void;
  /** The currently-applied `--lab-*` variables. */
  current(): Record<string, string>;
}

/**
 * Keep an element's `--lab-*` variables adapting to its (changing) background
 * without re-solving every frame: re-check the declared sample set, compare
 * its returned metrics with the last resolved baseline, and re-solve + ease
 * only after a sustained relative drop. This does not establish legibility
 * outside the supplied samples or between them.
 */
export declare function adaptTheme(element: HTMLElement, options: AdaptThemeOptions): AdaptController;
