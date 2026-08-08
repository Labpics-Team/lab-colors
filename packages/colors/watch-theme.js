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
// Omitted input passes through the strict package-private Point | Unknown
// observation gate. Unsupported effects or missing canvas evidence never become
// a fallback hex and never invoke the resolver.

import { observePointBackground } from "./background-observation.js";
import { acquireOutputLease } from "./output-sink.js";
import { admitSnapshot } from "./snapshot.js";

const CANCELLED = Symbol("watchTheme.cancelled");
const BUILTIN_QUEUE_MICROTASK =
  typeof globalThis.queueMicrotask === "function"
    ? globalThis.queueMicrotask.bind(globalThis)
    : null;
const BUILTIN_SET_TIMEOUT = globalThis.setTimeout.bind(globalThis);

const deferOutsideInjectedHost = (callback) => {
  let delivered = false;
  const deliverOnce = () => {
    if (delivered) return;
    delivered = true;
    callback();
  };
  if (BUILTIN_QUEUE_MICROTASK) {
    try {
      BUILTIN_QUEUE_MICROTASK(deliverOnce);
      return;
    } catch {
      // Task-fallback сохраняет обычное host-исключение вместо rejected
      // Promise; one-shot не даёт очереди, поставившей callback перед своим
      // исключением, продублировать доставку.
    }
  }
  BUILTIN_SET_TIMEOUT(deliverOnce, 0);
};

/**
 * @typedef {object} WatchController
 * @property {(force?: boolean) => (object | null)} refresh  Re-resolve+apply if the
 *   background (or theme) changed; `force` re-applies unconditionally. Returns the
 *   закоммиченный результат `resolveTheme` (кэшированный снимок, если ничего не менялось),
 *   либо `null`, если первый commit не состоялся после захвата observer.
 * @property {(theme: string) => void} setTheme  Switch theme and re-apply.
 * @property {() => (string | null)} background  The background/reference hex last committed.
 * @property {() => void} stop  Disconnect observers and stop watching.
 * @property {() => void} dispose  Stop and revoke only this attachment's outputs.
 */

/**
 * Keep an exact output target aligned with a supplied background or the
 * supported ancestor-chain reference estimate observed from `element`.
 *
 * @param {*} element  The surface to read the background from. When it is also
 *   the default output target, it must be its document's `documentElement` or
 *   the host of its own open `shadowRoot`.
 * @param {object} options
 * @param {{ resolveTheme: (bgHex: string, theme: string) => object }} options.colors
 *   An initialised `LabColors` engine (already `await init()`-ed).
 * @param {string} options.theme  Theme name (`"light" | "dark" | …`).
 * @param {string | (() => string)} [options.background]  Explicit reference
 *   background, overriding the ancestor estimate. A sample from an
 *   image/gradient/blur remains one declared point, not whole-field evidence.
 *   When supplied, it must be a non-empty string; invalid explicit evidence is
 *   rejected instead of being reinterpreted as the omitted-input fallback.
 * @param {*} [options.target=element]  Exact `:root`/`:host` output target.
 * @param {string} [options.canvas]  Caller-declared opaque page canvas.
 * @param {boolean} [options.observe=true]  Auto-refresh on `style`/`class`
 *   attribute changes in the observed subtree.
 * @param {(error: unknown) => void} [options.onError]  Receives failures from
 *   observer-обновлений и startup после захвата observer. Явные
 *   `refresh`/`setTheme` по-прежнему бросают.
 * @param {*} [options.root]  Mutation-observer root (default: the document element).
 * @param {*} [options.win=globalThis]  Window-like host (for MutationObserver).
 * @param {(el:*)=>*} [options.getStyle]  Injection seam for strict point observation.
 * @param {(el:*)=>*} [options.parentOf]  Injection seam for strict point observation.
 * @returns {WatchController}
 */
export function watchTheme(element, options) {
  const colors = options?.colors;
  const resolveThemeCapability = colors?.resolveTheme;
  if (typeof resolveThemeCapability !== "function") {
    throw new TypeError("watchTheme: options.colors must be an initialised LabColors engine");
  }
  const resolveTheme = resolveThemeCapability.bind(colors);
  if (typeof options.theme !== "string") {
    throw new TypeError("watchTheme: options.theme must be a theme name string");
  }
  if (options.onError !== undefined && typeof options.onError !== "function") {
    throw new TypeError("watchTheme: options.onError must be a function");
  }

  const target = options.target ?? element;
  const canvas = options.canvas;
  const backgroundSource = options.background;
  const getStyle = options.getStyle;
  const parentOf = options.parentOf;
  const win = options.win ?? (typeof globalThis !== "undefined" ? globalThis : undefined);
  const onError = options.onError;
  const enqueueMicrotask =
    typeof win?.queueMicrotask === "function"
      ? win.queueMicrotask.bind(win)
      : globalThis.queueMicrotask.bind(globalThis);
  const deliverAsyncError = (error) => {
    if (onError) {
      try {
        onError(error);
      } catch (reportingError) {
        const reported = new AggregateError(
          [error, reportingError],
          "watchTheme: asynchronous operation and its error handler both failed",
        );
        if (typeof win?.reportError === "function") {
          win.reportError(reported);
        } else {
          throw reported;
        }
      }
    } else if (typeof win?.reportError === "function") {
      win.reportError(error);
    } else {
      throw error;
    }
  };
  const reportAsyncError = (error) => {
    if (onError || typeof win?.reportError === "function") {
      deliverAsyncError(error);
    } else {
      // Сохранить видимое host-исключение, не создавая rejected Promise.
      enqueueMicrotask(() => {
        deliverAsyncError(error);
      });
    }
  };
  // После захвата host-ресурса ownership сначала должен стать достижимым
  // через возвращённый controller. Поэтому construction-failure нельзя
  // сообщать callback-ом внутри текущего стека.
  const deferAsyncError = (error) =>
    deferOutsideInjectedHost(() => deliverAsyncError(error));
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
  let commitDepth = 0;
  let stopped = false;
  let disposed = false;
  let disposeRequested = false;
  let disposeActive = false;
  let observerDisconnecting = false;
  const pendingOperations = [];
  let drainingOperations = false;
  let executingOperation = false;
  let queuedStopActive = false;
  let queuedDisposeActive = false;
  let observer = null;
  let outputLease = null;
  let outputBindings = null;

  const sameBindings = (left, right) =>
    left.length === right.length && left.every((name, index) => name === right[index]);

  const checkpoint = (owner) => {
    if (owner !== generation) throw CANCELLED;
  };
  const requireBackground = (value) => {
    if (typeof value !== "string" || value.length === 0) {
      throw new TypeError("watchTheme: background must be a non-empty string");
    }
    return value;
  };

  const readObservation = (owner) => {
    if (typeof backgroundSource === "function") {
      const value = backgroundSource();
      checkpoint(owner);
      return { kind: "point", hex: requireBackground(value) };
    }
    if (backgroundSource !== undefined) {
      return { kind: "point", hex: requireBackground(backgroundSource) };
    }
    const observation = observePointBackground(element, {
      canvas,
      getStyle,
      parentOf,
      checkpoint,
      checkpointToken: owner,
    });
    checkpoint(owner);
    return observation;
  };

  const prepareFor = (candidateTheme, force, owner) => {
    const observation = readObservation(owner);
    checkpoint(owner);
    if (observation.kind === "unknown") {
      return { kind: "unknown", candidateTheme, reason: observation.reason };
    }
    const bg = observation.hex;
    if (!force && bg === lastBg && candidateTheme === lastTheme) {
      // A failed atomic publication leaves the prior live bytes intact. Retry
      // the already admitted snapshot without re-running the resolver.
      return dirty ? { kind: "point", bg, candidateTheme, result: lastResult } : null;
    }
    // Допуск принадлежит prepare-фазе: конфликт ещё не затронул DOM или
    // controller state, поэтому то же observation можно повторить.
    const raw = resolveTheme(bg, candidateTheme);
    checkpoint(owner);
    const result = admitSnapshot(raw, "watchTheme", checkpoint, owner);
    checkpoint(owner);
    return { kind: "point", bg, candidateTheme, result };
  };

  const commitPrepared = ({ bg, candidateTheme, result }, owner) => {
    commitDepth++;
    let acquiredHere = false;
    try {
      if (outputLease === null) {
        outputLease = acquireOutputLease(target, result.outputBindings, "watchTheme");
        outputBindings = result.outputBindings;
        acquiredHere = true;
      } else if (!sameBindings(outputBindings, result.outputBindings)) {
        throw new TypeError("watchTheme: outputBindings changed; dispose and reattach");
      }
      const complete = outputLease.publish(result.vars, () => owner === generation);
      if (!complete) {
        if (acquiredHere) {
          const released = outputLease.dispose();
          if (!released && outputLease.state !== "disposed") {
            throw new Error("watchTheme: failed to release an uncommitted output lease");
          }
          if (outputLease.state === "disposed") {
            outputLease = null;
            outputBindings = null;
          }
        }
        return lastResult;
      }
    } catch (error) {
      let cleanupError = null;
      if (acquiredHere && outputLease !== null) {
        try {
          const released = outputLease.dispose();
          if (!released && outputLease.state !== "disposed") {
            throw new Error("watchTheme: failed to release an uncommitted output lease");
          }
        } catch (failure) {
          cleanupError = failure;
        }
        if (outputLease.state === "disposed") {
          outputLease = null;
          outputBindings = null;
        }
      }
      if (!stopped && owner === generation) dirty = true;
      if (cleanupError !== null) {
        throw new AggregateError(
          [error, cleanupError],
          "watchTheme: publication and uncommitted lease cleanup failed",
        );
      }
      throw error;
    } finally {
      commitDepth--;
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
    if (stopped) return lastResult;
    const gen = ++generation;
    let prepared;
    try {
      prepared = prepareFor(candidateTheme, force, gen);
    } catch (error) {
      // Ошибка кандидата, уже отозванного более новой reentrant-операцией,
      // является тем же stale outcome, что и успешный поздний return. Только
      // текущий owner вправе оборвать serial transaction и очистить её suffix.
      if (error === CANCELLED || stopped || gen !== generation) return lastResult;
      throw error;
    }
    if (stopped || gen !== generation) {
      // Изнутри prepare случился stop() либо более новая операция: наш
      // кандидат устарел — вернуть закоммиченное состояние без записи.
      return lastResult;
    }
    if (prepared === null) return lastResult;
    if (prepared.kind === "unknown") {
      theme = candidateTheme;
      return lastResult;
    }
    return commitPrepared(prepared, gen);
  };

  const disconnectObserver = () => {
    if (observerDisconnecting) return;
    const acquired = observer;
    observer = null;
    if (acquired) {
      observerDisconnecting = true;
      try {
        acquired.disconnect();
      } catch (error) {
        // Detach during the callback to prevent recursive double-disconnect, but
        // retain ownership after a transient failure so a later stop can retry.
        if (observer === null) observer = acquired;
        throw error;
      } finally {
        observerDisconnecting = false;
      }
    }
  };

  const runStop = () => {
    stopped = true;
    pendingOperations.length = 0;
    disconnectObserver();
    pendingOperations.length = 0;
    // A dispose requested from inside disconnect is stronger than stop. Its
    // explicit intent survives queue clearing and runs only after this observer
    // attempt has released the handle.
    if (disposeRequested && !disposeActive) runDispose();
  };

  const runQueuedStop = () => {
    queuedStopActive = true;
    try {
      runStop();
    } finally {
      queuedStopActive = false;
    }
  };

  const runDispose = () => {
    if (disposed || disposeActive) return;
    disposeActive = true;
    const failures = [];
    try {
      stopped = true;
      pendingOperations.length = 0;
      if (outputLease !== null) {
        const acquired = outputLease;
        commitDepth++;
        try {
          const released = acquired.dispose();
          if (!released && acquired.state !== "disposed") {
            throw new Error("watchTheme: output lease revocation did not complete");
          }
        } catch (error) {
          failures.push(error);
        } finally {
          commitDepth--;
          if (acquired.state === "disposed" && outputLease === acquired) {
            outputLease = null;
            outputBindings = null;
          }
        }
      }
      try {
        disconnectObserver();
      } catch (error) {
        failures.push(error);
      }
      pendingOperations.length = 0;
    } finally {
      disposeActive = false;
      disposed =
        disposeRequested &&
        outputLease === null &&
        observer === null &&
        !observerDisconnecting;
    }
    if (failures.length === 1) throw failures[0];
    if (failures.length > 1) {
      throw new AggregateError(failures, "watchTheme: output and observer cleanup both failed");
    }
  };

  const runQueuedDispose = () => {
    queuedDisposeActive = true;
    queuedStopActive = true;
    try {
      runDispose();
    } finally {
      queuedStopActive = false;
      queuedDisposeActive = false;
    }
  };

  const drainOperations = () => {
    if (drainingOperations || commitDepth > 0) return;
    if (stopped) {
      pendingOperations.length = 0;
      return;
    }
    drainingOperations = true;
    try {
      while (pendingOperations.length > 0 && !stopped) {
        const operation = pendingOperations.shift();
        if (operation.kind === "dispose") {
          runQueuedDispose();
        } else if (operation.kind === "stop") {
          runQueuedStop();
        } else if (operation.kind === "theme") {
          refreshFor(operation.theme);
        } else {
          // Refresh means "re-read the currently committed theme". Capturing
          // it at enqueue time would let an older value undo a preceding
          // queued setTheme after a reentrant CSS callback returns.
          refreshFor(theme, operation.force);
        }
      }
    } catch (error) {
      failOperation(error);
    } finally {
      drainingOperations = false;
    }
  };

  const drainControlsAfterFailure = () => {
    const mustDispose = pendingOperations.some((operation) => operation.kind === "dispose");
    const mustStop = mustDispose || pendingOperations.some((operation) => operation.kind === "stop");
    pendingOperations.length = 0;
    if (!mustStop) return [];

    const failures = [];
    const wasDraining = drainingOperations;
    drainingOperations = true;
    try {
      try {
        if (mustDispose) runQueuedDispose();
        else runQueuedStop();
      } catch (error) {
        failures.push(error);
      }
    } finally {
      // Stop is terminal and idempotent: a nested stop erases stale work but
      // does not recursively retry a host cleanup that is currently failing.
      pendingOperations.length = 0;
      drainingOperations = wasDraining;
    }
    return failures;
  };

  const failOperation = (primaryError) => {
    const cleanupFailures = drainControlsAfterFailure();
    if (cleanupFailures.length > 0) {
      throw new AggregateError(
        [primaryError, ...cleanupFailures],
        "watchTheme: operation failed and observer cleanup also failed",
      );
    }
    throw primaryError;
  };

  const runPublicOperation = (kind, value) => {
    let failed = false;
    let primaryError;
    executingOperation = true;
    try {
      if (kind === "theme") refreshFor(value);
      else refreshFor(theme, value);
    } catch (error) {
      failed = true;
      primaryError = error;
    } finally {
      executingOperation = false;
    }
    if (failed) failOperation(primaryError);
    drainOperations();
    return lastResult;
  };

  const refresh = (force = false) => {
    if (commitDepth > 0) {
      pendingOperations.push({ kind: "refresh", force });
      return lastResult;
    }
    if (executingOperation || drainingOperations) {
      // Вызов из prepare уже исполняемой queued-операции хронологически новее
      // оставшегося FIFO. Ставим его в хвост и сразу отзываем ещё не
      // закоммиченного кандидата; иначе старый хвост перезапишет новый intent.
      pendingOperations.push({ kind: "refresh", force });
      generation++;
      return lastResult;
    }
    return runPublicOperation("refresh", force);
  };

  const setTheme = (next) => {
    if (commitDepth > 0) {
      pendingOperations.push({ kind: "theme", theme: next });
      return;
    }
    if (executingOperation || drainingOperations) {
      pendingOperations.push({ kind: "theme", theme: next });
      generation++;
      return;
    }
    runPublicOperation("theme", next);
  };

  // Решаем первого кандидата до захвата долгоживущего host-ресурса,
  // но не применяем сразу: observer обязан быть активным во время первой
  // CSS-записи, чтобы variable-driven мутация фона не потерялась.
  const initialGen = ++generation;
  const initial = prepareFor(theme, true, initialGen);

  // Coalesce a burst of mutations into a single refresh on the next microtask.
  let scheduled = false;
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

  const dispose = () => {
    if (disposed) return;
    disposeRequested = true;
    if (commitDepth > 0) {
      pendingOperations.length = 0;
      pendingOperations.push({ kind: "dispose" });
      return;
    }
    if (executingOperation || drainingOperations) {
      pendingOperations.length = 0;
      if (!queuedDisposeActive) pendingOperations.push({ kind: "dispose" });
      generation++;
      return;
    }
    generation++;
    runDispose();
  };

  const controller = {
    refresh,
    setTheme,
    background() {
      return lastBg;
    },
    stop() {
      if (commitDepth > 0) {
        // The live replacement is the linearization point. Finish it, then run
        // the stronger terminal intent in FIFO order.
        pendingOperations.length = 0;
        pendingOperations.push({ kind: "stop" });
        return;
      }
      if (executingOperation || drainingOperations) {
        pendingOperations.length = 0;
        if (!queuedStopActive) pendingOperations.push({ kind: "stop" });
        generation++;
        return;
      }
      generation++;
      runStop();
    },
    dispose,
  };
  if (typeof Symbol.dispose === "symbol") controller[Symbol.dispose] = dispose;

  let observerAcquired = false;
  try {
    if (options.observe !== false && win && typeof win.MutationObserver === "function") {
      const root =
        options.root ??
        (typeof win.document !== "undefined" ? win.document.documentElement : null);
      if (root) {
        observer = new win.MutationObserver(schedule);
        observerAcquired = true;
        // Фон может смениться на самом элементе ИЛИ любом предке — через inline-стиль
        // or a class swap — so watch attribute changes across the subtree.
        observer.observe(root, {
          subtree: true,
          attributes: true,
          attributeFilter: ["style", "class"],
        });
      }
    }
    if (!stopped && initialGen === generation && initial.kind === "point") {
      commitPrepared(initial, initialGen);
    }
  } catch (error) {
    // До acquisition исключение безопасно синхронно: долгоживущего ownership
    // ещё нет. После acquisition controller обязан стать достижимым даже при
    // отказе cleanup; `runStop` сохраняет неосвобождённый handle для retry.
    if (!observerAcquired && outputLease === null) throw error;
    let reported = error;
    try {
      runStop();
    } catch (disconnectError) {
      reported = new AggregateError(
        [error, disconnectError],
        "watchTheme: construction failed and observer cleanup also failed",
      );
    }
    deferAsyncError(reported);
  }

  return controller;
}
