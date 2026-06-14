// Public types for the adaptive hysteresis controller.

import type { LabColors, ThemeName } from "./pkg/labcolors.js";

export interface AdaptThemeOptions {
  /** An initialised engine — needs `resolveTheme` AND `recheckContrast`. */
  colors: Pick<LabColors, "resolveTheme" | "recheckContrast">;
  theme: ThemeName;
  /** Explicit effective background, overriding the ancestor-composite. */
  background?: string | (() => string);
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
 * **lazily and smoothly**: re-check each frame, hold while colours still pass,
 * and re-solve + ease only when contrast stably degrades. The calm, cheap
 * alternative to re-solving every frame.
 */
export declare function adaptTheme(element: HTMLElement, options: AdaptThemeOptions): AdaptController;
