// Транзакционный owned sink для публикации `--lab-*` output.
//
// Ownership живёт в типе lease, а не выводится из имени ключа: контроллер
// объявляет точный набор output keys, и sink никогда не трогает чужие ключи —
// ни второго контроллера на том же элементе, ни client-authored `--lab-*`.
// Префиксное сканирование style запрещено.
//
// Атомарность: commit/patch валидируют весь batch и захватывают pre-batch
// снимок затрагиваемых ключей; исключение CSSOM, abort сигнала или потеря
// владения посреди batch откатывают DOM к этому снимку. Ownership-реестр
// переключается только после успешного batch, поэтому owned set и DOM не
// расходятся. Если lease отозван внутри batch (reentrant dispose), итог
// детерминирован: отозванное состояние (owned keys удалены), а не частичный
// commit.
//
// Stale completion: commit через неактивный lease — no-op (`false`), поэтому
// устаревшая async-операция прошлой генерации не может перезаписать или
// удалить output новой генерации.

export class LeaseConflictError extends Error {
  constructor(message) {
    super(message);
    this.name = "LeaseConflictError";
    this.code = "output_lease_conflict";
  }
}

const registry = new WeakMap();

// Минимальный контракт CSSStyleDeclaration: setProperty/removeProperty плюс
// getPropertyValue для rollback. Test doubles без getPropertyValue получают
// presence-only fallback через live-список; значение тогда восстановлено быть
// не может, и это осознанное ограничение таких doubles, а не тихий успех.
const readKey = (style, key) => {
  if (typeof style.getPropertyValue === "function") {
    const value = style.getPropertyValue(key);
    return value === "" || value === undefined || value === null
      ? { present: false, value: "" }
      : { present: true, value: String(value) };
  }
  if (typeof style.length === "number" && typeof style.item === "function") {
    for (let i = 0; i < style.length; i++) {
      if (style.item(i) === key) return { present: true, value: "" };
    }
  }
  return { present: false, value: "" };
};

const removeKey = (style, key) => {
  if (typeof style.removeProperty === "function") style.removeProperty(key);
};

// Удаление только по факту присутствия: removeProperty несуществующего ключа —
// физический no-op, который не должен попадать в журнал записи контроллера.
const removeIfPresent = (style, key) => {
  if (readKey(style, key).present) removeKey(style, key);
};

const abortError = (signal) =>
  signal.reason instanceof Error
    ? signal.reason
    : new Error("sink: commit aborted by signal");

const LOST_OWNERSHIP = Symbol("sink.lostOwnership");
const REVOKED_MID_BATCH = Symbol("sink.revokedMidBatch");

/**
 * Создать output lease на элементе. Пересечение ключей с активным чужим
 * lease — typed conflict: два writer-а одного output недопустимы.
 *
 * @param {*} element Элемент с `.style`.
 * @param {Iterable<string>} initialKeys Полный начальный набор owned keys.
 */
export function attachOutputBindingSet(element, initialKeys) {
  if (
    !element ||
    typeof element !== "object" ||
    !element.style ||
    typeof element.style.setProperty !== "function"
  ) {
    throw new TypeError("sink: target must be an element with style.setProperty");
  }
  let bindingSet = registry.get(element);
  if (!bindingSet) {
    bindingSet = { leases: new Set(), ownedKeys: new Set(), isWriting: false };
    registry.set(element, bindingSet);
  }
  const lease = { bindingSet, element, ownedKeys: new Set(), active: true };
  bindingSet.leases.add(lease);
  try {
    registerKeys(lease, initialKeys);
  } catch (error) {
    bindingSet.leases.delete(lease);
    if (bindingSet.leases.size === 0) registry.delete(element);
    throw error;
  }

  const runBatch = (keys, apply, options) => {
    const { owns, signal } = options;
    const set = lease.bindingSet;
    if (!lease.active) return false;
    if (signal?.aborted) throw abortError(signal);
    if (set.isWriting) {
      throw new Error("sink: reentrant write during an active commit batch");
    }
    if (typeof owns === "function" && !owns()) return false;

    const style = lease.element.style;
    const previous = new Map();
    for (const key of keys) previous.set(key, readKey(style, key));

    set.isWriting = true;
    try {
      for (const key of keys) {
        if (signal?.aborted) throw abortError(signal);
        if (typeof owns === "function" && !owns()) throw LOST_OWNERSHIP;
        apply(style, key);
        if (!lease.active) {
          // Reentrant revoke(): итог — отозванное состояние. Откатывать
          // к pre-batch значениям нельзя: они уже недействительны.
          throw REVOKED_MID_BATCH;
        }
      }
      return true;
    } catch (error) {
      if (error === REVOKED_MID_BATCH || !lease.active) {
        for (const key of keys) {
          try {
            removeKey(style, key);
          } catch {
            // Detach в середине отзыва: удаление остальных ключей продолжается.
          }
        }
      } else {
        for (const [key, prior] of previous) {
          try {
            if (prior.present) style.setProperty(key, prior.value);
            else removeKey(style, key);
          } catch {
            // Исходный writer упал; rollback делает best-effort восстановление,
            // первичная ошибка всё равно уходит наружу.
          }
        }
      }
      if (error === LOST_OWNERSHIP) return false;
      if (error === REVOKED_MID_BATCH) {
        throw new Error("sink: lease revoked during an active commit batch");
      }
      throw error;
    } finally {
      set.isWriting = false;
    }
  };

  const revoke = () => {
    if (!lease.active) return;
    lease.active = false;
    const set = lease.bindingSet;
    for (const key of lease.ownedKeys) set.ownedKeys.delete(key);
    set.leases.delete(lease);
    if (set.leases.size === 0) registry.delete(lease.element);
    if (set.isWriting) {
      // Commit сейчас внутри batch: он сам переведёт owned keys в отозванное
      // состояние через ветку REVOKED_MID_BATCH — дублировать удаление нельзя.
      return;
    }
    const style = lease.element.style;
    for (const key of lease.ownedKeys) {
      try {
        removeIfPresent(style, key);
      } catch {
        // Detach/destroyed target: отзыв остаётся best-effort; lease при этом
        // уже снят с реестра, повторный revoke не нужен.
      }
    }
  };

  return {
    get active() {
      return lease.active;
    },
    /** Снимок owned keys (копия; мутация не влияет на lease). */
    keys() {
      return new Set(lease.ownedKeys);
    },
    /**
     * Полный owned snapshot: записать ключи из nextVars, удалить owned ключи,
     * отсутствующие в нём. Owned-реестр переключается только после успешного
     * batch, поэтому реестр и DOM не расходятся. `diffFrom` (карта предыдущей
     * записи) пропускает неизменённые setProperty и все remove-вызовы — только
     * для вызовов с инвариантным набором ключей (ease-кадры).
     */
    commit(nextVars, options = {}) {
      if (!lease.active) return false;
      // Столкновение и типы проверяются до первой DOM-мутации; реестр
      // меняется только после успешной записи.
      const nextKeys = Object.keys(nextVars);
      for (const key of nextKeys) {
        if (typeof nextVars[key] !== "string") {
          throw new TypeError(`sink: value for '${key}' must be a string`);
        }
        if (!lease.ownedKeys.has(key) && lease.bindingSet.ownedKeys.has(key)) {
          throw new LeaseConflictError(
            `sink: key '${key}' is already owned by another active controller on this element`,
          );
        }
      }
      const previousOwned = [...lease.ownedKeys];
      const diffFrom = options.diffFrom ?? null;
      const written = runBatch(
        [...new Set([...previousOwned, ...nextKeys])],
        (style, key) => {
          const has = Object.hasOwn(nextVars, key);
          if (diffFrom !== null) {
            if (has && diffFrom[key] !== nextVars[key]) {
              style.setProperty(key, String(nextVars[key]));
            }
            return;
          }
          if (has) style.setProperty(key, String(nextVars[key]));
          else removeIfPresent(style, key);
        },
        options,
      );
      if (!written) return false;
      for (const key of previousOwned) {
        if (!Object.hasOwn(nextVars, key)) lease.bindingSet.ownedKeys.delete(key);
      }
      lease.ownedKeys = new Set(nextKeys);
      for (const key of lease.ownedKeys) lease.bindingSet.ownedKeys.add(key);
      return true;
    },
    /**
     * Точечный patch подмножества owned keys: строка — set, любое другое
     * значение — removeProperty при фактическом присутствии. Ключи вне
     * partialVars не трогаются. Owned set не меняется.
     */
    patch(partialVars, options = {}) {
      if (!lease.active) return false;
      const keys = Object.keys(partialVars);
      for (const key of keys) {
        if (!lease.ownedKeys.has(key)) {
          throw new Error(`sink: key '${key}' is not owned by this lease`);
        }
      }
      return runBatch(
        keys,
        (style, key) => {
          const value = partialVars[key];
          if (typeof value === "string") style.setProperty(key, value);
          else removeIfPresent(style, key);
        },
        options,
      );
    },
    /** Отозвать lease: ровно owned keys удаляются из DOM, lease снимается. */
    revoke,
  };
}

const registerKeys = (lease, keys) => {
  for (const key of keys) {
    if (lease.bindingSet.ownedKeys.has(key)) {
      throw new LeaseConflictError(
        `sink: key '${key}' is already owned by another active controller on this element`,
      );
    }
  }
  for (const key of keys) {
    lease.ownedKeys.add(key);
    lease.bindingSet.ownedKeys.add(key);
  }
};
