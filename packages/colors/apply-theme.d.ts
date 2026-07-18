import type { ResolvedTheme } from "./index.js";

/**
 * Применяет CSS-переменные решённой темы к элементу.
 *
 * Полный снимок допускается до обращения к CSSOM. Обычный Unreachable вызывает
 * структурный `OutputConflictError`; явные None, Unresolved и численная
 * неопределённость остаются метаданными без значения.
 *
 * @param element Целевой элемент, например `document.documentElement`.
 * @param result Результат `LabColors.resolveTheme(...)`.
 */
export declare function applyTheme(
  element: HTMLElement,
  result: ResolvedTheme,
): void;
