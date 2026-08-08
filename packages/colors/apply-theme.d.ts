/// <reference lib="esnext.disposable" />

import type { ResolvedTheme } from "./index.js";

export interface ApplyThemeAttachment {
  /** Atomically revoke only the output bindings owned by this application. */
  dispose(): void;
  /** Explicit-resource-management alias when the host defines `Symbol.dispose`. */
  [Symbol.dispose]?(): void;
}

/**
 * Применяет CSS-переменные решённой темы к элементу.
 *
 * Полный снимок допускается до обращения к CSSOM. Обычный Unreachable вызывает
 * структурный `OutputConflictError`; явные None, Unresolved и численная
 * неопределённость остаются метаданными без значения.
 *
 * @param element Exact output target: its document's `documentElement`, or an
 *   element whose own open `shadowRoot` is the output scope.
 * @param result Результат `LabColors.resolveTheme(...)`.
 */
export declare function applyTheme(
  element: HTMLElement,
  result: ResolvedTheme,
): ApplyThemeAttachment;
