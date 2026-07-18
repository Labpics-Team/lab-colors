import type { ResolvedTheme } from "./index.js";

/**
 * Apply a resolved theme's CSS variables to an element.
 *
 * Admits the complete snapshot before touching CSSOM. An ordinary Unreachable
 * throws a structural `OutputConflictError`; explicit None, Unresolved, and
 * numerical indeterminacy remain value-less metadata.
 *
 * @param element The target element (e.g. `document.documentElement`).
 * @param result A `LabColors.resolveTheme(...)` result.
 */
export declare function applyTheme(
  element: HTMLElement,
  result: ResolvedTheme,
): void;
