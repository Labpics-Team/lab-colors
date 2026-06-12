import type { ResolvedTheme } from "./pkg/labcolors.js";

/**
 * Apply a resolved theme's CSS variables to an element.
 *
 * Writes every reachable role from `result.vars` onto `element.style` via
 * `setProperty`. Unreachable and zero-token roles are absent from `vars` and
 * are not written, so the caller's CSS fallbacks stay in effect for them.
 *
 * @param element The target element (e.g. `document.documentElement`).
 * @param result A `LabColors.resolveTheme(...)` result.
 */
export declare function applyTheme(
  element: HTMLElement,
  result: Pick<ResolvedTheme, "vars">,
): void;
