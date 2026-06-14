// Public types for the reactive theme runtime.

import type { LabColors, ResolvedTheme, ThemeName } from "./pkg/labcolors.js";

export interface WatchThemeOptions {
  /** An initialised `LabColors` engine (after `await init()`). */
  colors: Pick<LabColors, "resolveTheme">;
  /** Theme name. */
  theme: ThemeName;
  /**
   * Explicit effective background, overriding the ancestor-composite. Use a hex
   * string or a `() => hex` you sample yourself for image/gradient/blur surfaces
   * that have no single readable `background-color`.
   */
  background?: string | (() => string);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /** Base colour when the ancestor chain is fully translucent. Default `"#FFFFFF"`. */
  fallback?: string;
  /** Auto-refresh on DOM attribute mutations. Default `true`. */
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
   * Re-resolve and re-apply if the effective background (or theme) changed;
   * `force` re-applies unconditionally. Returns the now-applied result, or the
   * cached one when nothing changed.
   */
  refresh(force?: boolean): ResolvedTheme | null;
  /** Switch theme and re-apply. */
  setTheme(theme: ThemeName): void;
  /** The effective background hex last resolved. */
  background(): string;
  /** Disconnect observers and stop watching. */
  stop(): void;
}

/**
 * Keep an element's `--lab-*` variables in sync with its (effective) background.
 *
 * Discrete background/theme changes are caught by a `MutationObserver`;
 * continuous (animated) backgrounds are driven by calling `refresh()` from a
 * `requestAnimationFrame` loop (cheap — re-resolves only on real change).
 */
export declare function watchTheme(
  element: HTMLElement,
  options: WatchThemeOptions,
): WatchController;
