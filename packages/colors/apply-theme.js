// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: write a resolved theme's
// reachable colours onto an element as `--lab-*` custom properties.

import { admitSnapshot, writeVars } from "./snapshot.js";

/**
 * Применяет CSS-переменные решённой темы к элементу.
 *
 * Полный результат допускается до первого обращения к CSSOM. Обычный
 * Unreachable атомарно отклоняется как `OutputConflictError`; явные None,
 * Unresolved и численная неопределённость остаются метаданными без значения.
 * Затем функция записи удаляет прежние встроенные переменные `--lab-*` и
 * записывает выбранные значения из `result.vars`.
 *
 * @param {HTMLElement} element - Целевой элемент, например `document.documentElement`.
 * @param {{ vars: Record<string, string>, roles: Record<string, object> }} result
 *   Полный результат `resolveTheme(...)`.
 * @returns {void}
 */
export function applyTheme(element, result) {
  if (!element || typeof element.style?.setProperty !== "function") {
    throw new TypeError("applyTheme: first argument must be an element with a style");
  }
  const snapshot = admitSnapshot(result, "applyTheme");
  writeVars(element, snapshot.vars, "applyTheme");
}
