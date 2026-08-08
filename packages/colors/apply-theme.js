// Vanilla DOM helper — zero dependencies.
//
// The WASM core returns data and never touches the DOM (that separation is
// deliberate; full reactive injection is the css-injection-runtime chapter).
// This helper is the minimal, framework-free bridge: publish a resolved theme
// through the same owned atomic sink used by the reactive adapters.

import { acquireOutputLease } from "./output-sink.js";
import { admitSnapshot } from "./snapshot.js";

const CANCELLED = Symbol("applyTheme.cancelled");
const applied = new WeakMap();
const operationOwners = new WeakMap();
const sameBindings = (left, right) =>
  left.length === right.length && left.every((name, index) => name === right[index]);

const beginOperation = (target) => {
  const owner = { error: null };
  operationOwners.set(target, owner);
  return owner;
};

const ownsOperation = (target, owner) => operationOwners.get(target) === owner;

const checkpoint = ({ target, owner }) => {
  if (!ownsOperation(target, owner)) throw CANCELLED;
};

const cancelledAttachment = (target, fallback) => {
  const current = applied.get(target);
  const attachment =
    (current?.published === true ? current.attachment : null) ??
    (fallback?.published === true ? fallback.attachment : null);
  if (attachment !== null) return attachment;
  throw new Error("applyTheme: operation lost ownership before an attachment was established");
};

const settleStaleOperation = (target, error, fallback) => {
  // A newer nested operation owns its own failure. Only the internal
  // cancellation sentinel, or an error produced by the now-stale outer
  // candidate, may collapse to the currently committed attachment.
  if (error !== CANCELLED && operationOwners.get(target)?.error === error) throw error;
  return cancelledAttachment(target, fallback);
};

const attachmentFor = (target, lease) => {
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    const current = applied.get(target);
    if (!current || current.lease !== lease) {
      disposed = true;
      return;
    }
    // Disposal is itself a newer target operation. It invalidates any
    // candidate still reflecting caller data before retryable host cleanup.
    const owner = beginOperation(target);
    try {
      const released = lease.dispose(() => ownsOperation(target, owner));
      if (!released && lease.state !== "disposed") {
        throw new Error("applyTheme: output lease revocation did not complete");
      }
      if (lease.state === "disposed") {
        if (applied.get(target)?.lease === lease) applied.delete(target);
        disposed = true;
      }
    } catch (error) {
      owner.error = error;
      throw error;
    }
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
 * Core-authored `outputBindings` связываются с одним target-owned lease. Точные
 * inline-декларации из owned `outputBindings` проверяются на конфликт до
 * публикации; несвязанные inline-декларации остаются нетронутыми.
 *
 * @param {HTMLElement} element - Output target: either its document's
 *   `documentElement` (`:root`) or the host of its own open `ShadowRoot` (`:host`).
 * @param {{ outputBindings: readonly string[], vars: Record<string, string>, roles: Record<string, object> }} result
 *   Полный результат `resolveTheme(...)`.
 * @returns {{dispose: () => void, [Symbol.dispose]?: () => void}} Владелец с
 *   универсальным `dispose()`; псевдоним `Symbol.dispose` присутствует только
 *   в host-средах с поддержкой Explicit Resource Management.
 */
export function applyTheme(element, result) {
  // Ownership precedes the first reflection of caller-controlled snapshot data.
  // A nested apply/dispose therefore invalidates this candidate at the next
  // admission checkpoint instead of being overwritten by the stale outer call.
  const owner = beginOperation(element);
  try {
    return applyOwned(element, result, owner);
  } catch (error) {
    // The current owner is the provenance channel for exceptions crossing a
    // stale caller. Identity, rather than timing, distinguishes a nested
    // operation's failure from an obsolete outer candidate's own failure.
    owner.error = error;
    throw error;
  }
}

function applyOwned(element, result, owner) {
  const operation = { target: element, owner };
  const previous = applied.get(element) ?? null;
  let snapshot;
  try {
    snapshot = admitSnapshot(result, "applyTheme", checkpoint, operation);
    checkpoint(operation);
  } catch (error) {
    if (error === CANCELLED || !ownsOperation(element, owner)) {
      return settleStaleOperation(element, error, previous);
    }
    throw error;
  }

  let entry = applied.get(element);
  let acquiredHere = false;
  if (entry) {
    if (!sameBindings(entry.lease.outputBindings, snapshot.outputBindings)) {
      throw new TypeError("applyTheme: outputBindings changed; dispose the prior attachment first");
    }
  } else {
    let lease;
    try {
      lease = acquireOutputLease(element, snapshot.outputBindings, "applyTheme");
    } catch (error) {
      if (!ownsOperation(element, owner)) {
        return settleStaleOperation(element, error, previous);
      }
      throw error;
    }
    if (!ownsOperation(element, owner)) {
      const released = lease.dispose();
      if (!released && lease.state !== "disposed") {
        throw new Error("applyTheme: failed to release a stale uncommitted output lease");
      }
      return cancelledAttachment(element, previous);
    }
    entry = { lease, attachment: null, published: false };
    entry.attachment = attachmentFor(element, lease);
    acquiredHere = true;
    // Retain a recoverable handle if publication and cleanup both fail. A
    // later call with the same manifest can retry instead of colliding with an
    // unreachable lease hidden only in the target registry.
    applied.set(element, entry);
  }

  let cancelled = false;
  try {
    if (!entry.lease.publish(snapshot.vars, () => ownsOperation(element, owner))) {
      if (!ownsOperation(element, owner)) {
        cancelled = true;
      } else {
        throw new Error("applyTheme: output lease lost ownership before publication");
      }
    }
  } catch (error) {
    // Only the operation that acquired an unpublished lease may revoke it.
    // A reentrant call can observe that entry while its creator is inside the
    // sink's exclusive section; treating the shared entry as locally acquired
    // would turn the expected busy failure into destructive double-cleanup.
    if (acquiredHere) {
      let cleanupError = null;
      try {
        const released = entry.lease.dispose();
        if (!released && entry.lease.state !== "disposed") {
          throw new Error("applyTheme: failed to release an uncommitted output lease");
        }
      } catch (failure) {
        cleanupError = failure;
      }
      if (entry.lease.state === "disposed" && applied.get(element) === entry) {
        applied.delete(element);
      }
      if (cleanupError !== null) {
        throw new AggregateError(
          [error, cleanupError],
          "applyTheme: publication and uncommitted lease cleanup failed",
        );
      }
    }
    if (!ownsOperation(element, owner)) {
      return settleStaleOperation(element, error, previous);
    }
    throw error;
  }
  if (cancelled) {
    if (acquiredHere) {
      let cleanupError = null;
      try {
        const released = entry.lease.dispose();
        if (!released && entry.lease.state !== "disposed") {
          throw new Error("applyTheme: failed to release a cancelled output lease");
        }
      } catch (failure) {
        cleanupError = failure;
      }
      if (entry.lease.state === "disposed" && applied.get(element) === entry) {
        applied.delete(element);
      }
      if (cleanupError !== null) {
        throw new AggregateError(
          [new Error("applyTheme: publication was cancelled"), cleanupError],
          "applyTheme: cancelled publication and uncommitted lease cleanup failed",
        );
      }
    }
    return settleStaleOperation(element, CANCELLED, previous);
  }
  entry.published = true;
  return entry.attachment;
}
