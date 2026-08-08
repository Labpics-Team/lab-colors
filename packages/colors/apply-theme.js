// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: publish a resolved theme
// through the same owned atomic sink used by the reactive adapters.

import { acquireOutputLease } from "./output-sink.js";
import { admitSnapshot } from "./snapshot.js";

const applied = new WeakMap();
const sameBindings = (left, right) =>
  left.length === right.length && left.every((name, index) => name === right[index]);

const attachmentFor = (target, lease) => {
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    const current = applied.get(target);
    if (!current || current.lease !== lease) {
      disposed = true;
      return;
    }
    lease.dispose();
    applied.delete(target);
    disposed = true;
  };
  const attachment = { dispose };
  if (typeof Symbol.dispose === "symbol") attachment[Symbol.dispose] = dispose;
  return Object.freeze(attachment);
};

/**
 * Применяет CSS-переменные решённой темы к элементу.
 *
 * Полный результат допускается до первого обращения к CSSOM. Обычный
 * Unreachable атомарно отклоняется как `OutputConflictError`; явные None,
 * Unresolved и численная неопределённость остаются метаданными без значения.
 * Core-authored `outputBindings` связываются с одним target-owned lease. Sink
 * публикует полный dedicated stylesheet одной атомарной заменой; чужие output
 * sets и inline declarations не сканируются и не изменяются.
 *
 * @param {HTMLElement} element - Целевой элемент, например `document.documentElement`.
 * @param {{ vars: Record<string, string>, roles: Record<string, object> }} result
 *   Полный результат `resolveTheme(...)`.
 * @returns {{dispose: () => void}} Владелец, атомарно отзывающий этот output set.
 */
export function applyTheme(element, result) {
  const snapshot = admitSnapshot(result, "applyTheme");
  let entry = applied.get(element);
  let acquiredHere = false;
  if (entry) {
    if (!sameBindings(entry.lease.outputBindings, snapshot.outputBindings)) {
      throw new TypeError("applyTheme: outputBindings changed; dispose the prior attachment first");
    }
  } else {
    const lease = acquireOutputLease(element, snapshot.outputBindings, "applyTheme");
    entry = { lease, attachment: null, published: false };
    entry.attachment = attachmentFor(element, lease);
    acquiredHere = true;
    // Retain a recoverable handle if publication and cleanup both fail. A
    // later call with the same manifest can retry instead of colliding with an
    // unreachable lease hidden only in the target registry.
    applied.set(element, entry);
  }

  try {
    if (!entry.lease.publish(snapshot.vars)) {
      throw new Error("applyTheme: output lease lost ownership before publication");
    }
  } catch (error) {
    if (acquiredHere || !entry.published) {
      let cleanupError = null;
      try {
        const released = entry.lease.dispose();
        if (!released && entry.lease.state !== "disposed") {
          throw new Error("applyTheme: failed to release an uncommitted output lease");
        }
      } catch (failure) {
        cleanupError = failure;
      }
      if (entry.lease.state === "disposed") applied.delete(element);
      if (cleanupError !== null) {
        throw new AggregateError(
          [error, cleanupError],
          "applyTheme: publication and uncommitted lease cleanup failed",
        );
      }
    }
    throw error;
  }
  entry.published = true;
  return entry.attachment;
}
