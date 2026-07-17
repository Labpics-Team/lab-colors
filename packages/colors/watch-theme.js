// Reactive theme runtime — zero dependencies.
//
// `applyTheme` writes a resolved theme's `--lab-*` variables once. `watchTheme`
// repeats that operation when an explicitly supplied background input or the
// helper's supported `background-color` reference estimate changes. It does not
// observe rendered pixels or infer a whole backdrop field.
//
// It serves both regimes:
//   * DISCRETE `style`/`class` attribute changes in the observed subtree schedule
//     a refresh through `MutationObserver`; layout and pixel changes do not.
//   * CONTINUOUS changes (a CSS-animated or per-frame-scripted background that
//     never mutates inline style) are driven by the caller calling `refresh()`
//     inside its own `requestAnimationFrame` loop. `refresh()` re-resolves only
//     when the supplied/reference background string changes.
//
// The fallback estimate alpha-composites the supported ancestor
// `background-color` chain (`effective-bg.js`). For images/gradients/blur, pass
// an explicit reference hex; one sample does not represent the whole field.

import { applyTheme } from "./apply-theme.js";
import { effectiveBackground } from "./effective-bg.js";

/**
 * @typedef {object} WatchController
 * @property {(force?: boolean) => object} refresh  Re-resolve+apply if the
 *   background (or theme) changed; `force` re-applies unconditionally. Returns the
 *   закоммиченный результат `resolveTheme` (кэшированный снимок, если ничего не менялось).
 * @property {(theme: string) => void} setTheme  Switch theme and re-apply.
 * @property {() => string} background  The background/reference hex last resolved.
 * @property {() => void} stop  Disconnect observers and stop watching.
 */

/**
 * Keep `element`'s `--lab-*` variables aligned with a supplied background or
 * the supported ancestor-chain reference estimate.
 *
 * @param {*} element  The surface to read the background from and (by default)
 *   write the variables onto.
 * @param {object} options
 * @param {{ resolveTheme: (bgHex: string, theme: string) => object }} options.colors
 *   An initialised `LabColors` engine (already `await init()`-ed).
 * @param {string} options.theme  Theme name (`"light" | "dark" | …`).
 * @param {string | (() => string)} [options.background]  Explicit reference
 *   background, overriding the ancestor estimate. A sample from an
 *   image/gradient/blur remains one declared point, not whole-field evidence.
 * @param {*} [options.target=element]  Element to write the variables onto.
 * @param {string} [options.fallback="#FFFFFF"]  Base for a fully-translucent chain.
 * @param {boolean} [options.observe=true]  Auto-refresh on `style`/`class`
 *   attribute changes in the observed subtree.
 * @param {(error: unknown) => void} [options.onError]  Receives failures from
 *   observer-обновлений. Явные `refresh`/`setTheme` по-прежнему бросают.
 * @param {*} [options.root]  Mutation-observer root (default: the document element).
 * @param {*} [options.win=globalThis]  Window-like host (for MutationObserver).
 * @param {(el:*)=>*} [options.getStyle]  Injection seam for `effectiveBackground`.
 * @param {(el:*)=>*} [options.parentOf]  Injection seam for `effectiveBackground`.
 * @returns {WatchController}
 */
export function watchTheme(element, options) {
  if (!options || typeof options.colors?.resolveTheme !== "function") {
    throw new TypeError("watchTheme: options.colors must be an initialised LabColors engine");
  }
  if (typeof options.theme !== "string") {
    throw new TypeError("watchTheme: options.theme must be a theme name string");
  }
  if (options.onError !== undefined && typeof options.onError !== "function") {
    throw new TypeError("watchTheme: options.onError must be a function");
  }

  const target = options.target ?? element;
  const fallback = options.fallback ?? "#FFFFFF";
  const win = options.win ?? (typeof globalThis !== "undefined" ? globalThis : undefined);
  const onError = options.onError;
  const enqueueMicrotask =
    typeof win?.queueMicrotask === "function"
      ? win.queueMicrotask.bind(win)
      : globalThis.queueMicrotask.bind(globalThis);
  const reportAsyncError = (error) => {
    if (onError) {
      onError(error);
    } else if (typeof win?.reportError === "function") {
      win.reportError(error);
    } else {
      // Сохранить видимое host-исключение, не создавая rejected Promise.
      enqueueMicrotask(() => {
        throw error;
      });
    }
  };
  let theme = options.theme;
  // Поколение операций: prepareFor исполняет пользовательский код
  // (background()/resolveTheme), который может reentrant-но вызвать
  // stop()/setTheme()/refresh(). Внешняя транзакция обязана проверить
  // владение ПОСЛЕ prepare и не коммитить устаревшего кандидата.
  let generation = 0;
  let lastBg = null;
  let lastTheme = null;
  let lastResult = null;
  let dirty = false;

  const readBackground = () => {
    const b = options.background;
    if (typeof b === "function") return b();
    if (typeof b === "string") return b;
    return effectiveBackground(element, {
      fallback,
      getStyle: options.getStyle,
      parentOf: options.parentOf,
    });
  };

  const prepareFor = (candidateTheme, force = false) => {
    const bg = readBackground();
    if (!force && bg === lastBg && candidateTheme === lastTheme) {
      // Прошлое CSSOM-исключение могло оставить inline-стиль записанным
      // частично. Переиспользуем закоммиченный физический снимок: чинить
      // императивную оболочку резолвером не нужно.
      return dirty ? { bg, candidateTheme, result: lastResult } : null;
    }
    const result = options.colors.resolveTheme(bg, candidateTheme);
    return { bg, candidateTheme, result };
  };

  const commitPrepared = ({ bg, candidateTheme, result }) => {
    try {
      applyTheme(target, result);
    } catch (error) {
      dirty = true;
      throw error;
    }
    // Публикуем запрошенную тему только после успеха и резолва, и записи в
    // DOM: отклонённый кандидат не может стать скрытым входом позднейшего
    // фонового refresh.
    theme = candidateTheme;
    lastBg = bg;
    lastTheme = candidateTheme;
    lastResult = result;
    dirty = false;
    return result;
  };

  const refreshFor = (candidateTheme, force = false) => {
    const gen = ++generation;
    const prepared = prepareFor(candidateTheme, force);
    if (stopped || gen !== generation) {
      // Изнутри prepare случился stop() либо более новая операция: наш
      // кандидат устарел — вернуть закоммиченное состояние без записи.
      return lastResult;
    }
    return prepared === null ? lastResult : commitPrepared(prepared);
  };

  const refresh = (force = false) => refreshFor(theme, force);

  // Решаем первого кандидата до захвата долгоживущего host-ресурса,
  // но не применяем сразу: observer обязан быть активным во время первой
  // CSS-записи, чтобы variable-driven мутация фона не потерялась.
  const initialGen = ++generation;
  const initial = prepareFor(theme, true);

  // Coalesce a burst of mutations into a single refresh on the next microtask.
  let scheduled = false;
  let stopped = false;
  const schedule = () => {
    if (scheduled || stopped) return;
    scheduled = true;
    enqueueMicrotask(() => {
      scheduled = false;
      // A `stop()` between scheduling and this microtask must cancel the refresh
      // — the watcher is done, no late writes.
      if (!stopped) {
        try {
          refresh();
        } catch (error) {
          reportAsyncError(error);
        }
      }
    });
  };

  let observer = null;
  try {
    if (options.observe !== false && win && typeof win.MutationObserver === "function") {
      const root =
        options.root ??
        (typeof win.document !== "undefined" ? win.document.documentElement : null);
      if (root) {
        observer = new win.MutationObserver(schedule);
        // Фон может смениться на самом элементе ИЛИ любом предке — через inline-стиль
        // or a class swap — so watch attribute changes across the subtree.
        observer.observe(root, {
          subtree: true,
          attributes: true,
          attributeFilter: ["style", "class"],
        });
      }
    }
    if (!stopped && initialGen === generation) {
      commitPrepared(initial);
    }
  } catch (error) {
    // Упавшая конструкция не смеет оставить недосягаемый observer или
    // поздний refresh. Помечаем stopped до disconnect, чтобы уже поставленный
    // в очередь callback был инертен, даже если host-очистка сама упала.
    stopped = true;
    const acquired = observer;
    observer = null;
    if (acquired) {
      try {
        acquired.disconnect();
      } catch (disconnectError) {
        throw new AggregateError(
          [error, disconnectError],
          "watchTheme: construction failed and observer cleanup also failed",
        );
      }
    }
    throw error;
  }

  return {
    refresh,
    setTheme(next) {
      refreshFor(next);
    },
    background() {
      return lastBg;
    },
    stop() {
      stopped = true;
      generation++;
      if (observer) observer.disconnect();
      observer = null;
    },
  };
}
