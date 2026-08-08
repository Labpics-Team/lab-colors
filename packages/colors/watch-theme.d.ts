/// <reference lib="esnext.disposable" />

// Public types for the reactive theme runtime.

import type { LabColors, ResolvedTheme, ThemeName } from "./index.js";

export interface WatchThemeOptions {
  /** An initialised `LabColors` engine (after `await init()`). */
  colors: Pick<LabColors, "resolveTheme">;
  /** Theme name. */
  theme: ThemeName;
  /**
   * Explicit point background evidence, overriding computed-CSS observation. A
   * hex sampled from image/gradient/blur content remains one declared point, not
   * an observation of the whole field. The value must be a non-empty string;
   * invalid explicit evidence is rejected instead of being reinterpreted.
   */
  background?: string | (() => string);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /**
   * Caller-declared opaque page canvas used only when the supported ancestor
   * chain is fully translucent. Without it that state is `Unknown`; no white
   * canvas is invented. The value must be an opaque supported CSS colour.
   */
  canvas?: string;
  /** Auto-refresh on `style`/`class` attribute changes in the observed subtree. Default `true`. */
  observe?: boolean;
  /**
   * Receives failures from observer updates and startup after observer
   * acquisition. A typed computed-CSS `Unknown` is not an error and causes no
   * resolve or DOM write. Explicit `refresh()`/`setTheme()` failures still throw.
   */
  onError?: (error: unknown) => void;
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
   * Re-resolve and re-apply when a supported Point observation or theme changes.
   * `force` re-applies a known Point unconditionally. On computed-CSS `Unknown`,
   * no engine/DOM update occurs and the last committed result is returned.
   */
  refresh(force?: boolean): ResolvedTheme | null;
  /** Switch theme intent; a rejected candidate keeps the committed theme/output. */
  setTheme(theme: ThemeName): void;
  /** The point background last committed, or `null` before any successful commit. */
  background(): string | null;
  /** Disconnect observers and stop watching. */
  stop(): void;
  /** Stop and atomically revoke only this attachment's output bindings. */
  dispose(): void;
  [Symbol.dispose](): void;
}

/**
 * Aligns an element's `--lab-*` variables with explicit point evidence or the
 * package-private strict computed-CSS `Point | Unknown` observation gate.
 * Unsupported colours/effects, a translucent root without `canvas`, cycles and
 * depth exhaustion never become an invented hex and never call the resolver.
 */
export declare function watchTheme(
  element: HTMLElement,
  options: WatchThemeOptions,
): WatchController;
