// Public types for the reactive theme runtime.

import type { LabColors, ResolvedTheme, ThemeName } from "./index.js";

export interface WatchThemeOptions {
  /** An initialised `LabColors` engine (after `await init()`). */
  colors: Pick<LabColors, "resolveTheme">;
  /** Theme name. */
  theme: ThemeName;
  /**
   * Explicit reference background, overriding the ancestor estimate. A hex
   * sampled from image/gradient/blur content remains one declared point, not an
   * observation of the whole field.
   */
  background?: string | (() => string);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /** Base colour when the ancestor chain is fully translucent. Default `"#FFFFFF"`. */
  fallback?: string;
  /** Auto-refresh on `style`/`class` attribute changes in the observed subtree. Default `true`. */
  observe?: boolean;
  /** Mutation-observer root. Defaults to the document element. */
  root?: Node;
  /** Window-like host (for `MutationObserver`). Defaults to `globalThis`. */
  win?: Window;
  /** Injection seam for the computed style of an element (testing). */
  getStyle?: (element: unknown) => { getPropertyValue(property: string): string };
  /** Injection seam for an element's parent (testing). */
  parentOf?: (element: unknown) => unknown;
}

export interface WatchController {
  /**
   * Re-resolve and re-apply if the background/reference input (or theme) changed;
   * `force` re-applies unconditionally. Returns the now-applied result, or the
   * cached one when nothing changed.
   */
  refresh(force?: boolean): ResolvedTheme | null;
  /** Switch theme and re-apply. */
  setTheme(theme: ThemeName): void;
  /** The background/reference hex last resolved. */
  background(): string;
  /** Disconnect observers and stop watching. */
  stop(): void;
}

/**
 * Keep an element's `--lab-*` variables aligned with an explicit background or
 * the supported ancestor-chain reference estimate.
 *
 * `style`/`class` attribute changes in the observed subtree schedule a refresh;
 * continuous inputs are driven by calling `refresh()` from a
 * `requestAnimationFrame` loop. Pixel/layout changes are not observed.
 */
export declare function watchTheme(
  element: HTMLElement,
  options: WatchThemeOptions,
): WatchController;
