// Public types for the adaptive hysteresis controller.

import type { LabColors, ThemeName } from "./index.js";

export interface AdaptThemeOptions {
  /**
   * An initialised engine — needs resolve + packed contrast recheck. The exact
   * `isStableGlowPointNoop` capability is conditionally required when a result
   * contains a stable Glow role. `recheckContrastMulti` is optional and batches
   * a finite explicit sample set without changing its point-wise semantics.
   * `themeHandle` is optional: when present, the theme key is lowered to its
   * numeric handle once per theme and addressed numerically in the recheck loop.
   */
  colors: Pick<LabColors, "resolveTheme" | "recheckContrast"> &
    Partial<Pick<LabColors, "recheckContrastMulti" | "themeHandle" | "isStableGlowPointNoop">>;
  theme: ThemeName;
  /**
   * Explicit point evidence, overriding computed-CSS observation. A string array
   * is a finite, caller-supplied sample set for a varying backdrop; the controller
   * checks every supplied point and does not infer a Raster or Field between samples.
   */
  background?: string | string[] | (() => string | string[]);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /**
   * Caller-declared opaque page canvas for a fully translucent supported ancestor
   * chain. Without it computed observation is `Unknown`; no white base is invented.
   */
  canvas?: string;
  /**
   * Finite fraction of a role's contrast surplus that may be lost before a
   * re-solve, in the closed interval `[0, 1]`. Default `0.2`.
   */
  dropFraction?: number;
  /** Finite non-negative breach duration in ms. Default `120`. */
  sustainMs?: number;
  /** Finite non-negative minimum time between re-solves in ms. Default `250`. */
  dwellMs?: number;
  /** Finite non-negative crossfade duration in ms. Default `280`. */
  easeMs?: number;
  /** Override reduced-motion detection. */
  reducedMotion?: boolean;
  /** Clock injection. */
  now?: () => number;
  /** Window-like host. */
  win?: Window;
  /** Injection seam for computed style (testing). */
  getStyle?: (element: unknown) => { getPropertyValue(property: string): string };
  /** Injection seam for an element's parent (testing). */
  parentOf?: (element: unknown) => unknown;
}

export interface AdaptController {
  /**
   * Read one finite sample set or one strict computed-CSS Point. A computed
   * `Unknown` performs no resolver/recheck/DOM work and preserves committed state.
   */
  tick(now?: number): void;
  /** Switch theme intent immediately when Point evidence is available. */
  setTheme(theme: ThemeName): void;
  /** Start the internal `requestAnimationFrame` loop. */
  start(): void;
  /** Stop the internal loop without discarding an unfinished transition. */
  stop(): void;
  /** Канонические логические цели committed state; empty before the first supported Point commit. */
  current(): Record<string, string>;
}

/**
 * Adapts an element to explicit finite point evidence or the strict package-
 * private `Point | Unknown` computed-CSS gate. Unsupported effects, transparent
 * root without `canvas`, cycles and depth exhaustion never become a fallback hex.
 */
export declare function adaptTheme(element: HTMLElement, options: AdaptThemeOptions): AdaptController;
