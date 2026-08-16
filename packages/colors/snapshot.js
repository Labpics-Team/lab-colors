// Внутренняя pure-data граница допуска снимков решённой темы.
//
// Успешный снимок может содержать явный None, незавершённый bounded search или
// численную неопределённость. Ordinary Unreachable означает, что объявленный
// output-контракт невыполним: применение остальных vars превратило бы ambient
// CSS в неявный fallback.

import { isCanonicalOutputBindingName } from "./output-bindings.js";

const admittedSnapshots = new WeakSet();
const NO_CHECKPOINT = () => {};

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function malformed(context, detail) {
  return new TypeError(`${context}: ${detail}`);
}

/** Копирует только JSON-подобные data properties и замораживает результат.
 * Getter/setter не вызываются: иначе проверенный `roles` мог измениться между
 * допуском и чтением `vars` в output sink. */
function materialize(value, context, path, active, checkpoint, token) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw malformed(context, `${path} contains a non-finite number`);
    return value;
  }
  if (typeof value !== "object") {
    throw malformed(context, `${path} must contain data properties only`);
  }
  if (active.has(value)) throw malformed(context, `${path} contains a cycle`);

  const array = Array.isArray(value);
  const prototype = Object.getPrototypeOf(value);
  checkpoint(token);
  if (!array && prototype !== null) {
    // `Object.prototype` is realm-local, so identity would reject a valid
    // iframe/worker payload. A plain object's direct prototype is itself a
    // root prototype; the second reflection distinguishes it from class
    // instances without invoking names, coercion, or inherited accessors.
    const prototypeParent = Object.getPrototypeOf(prototype);
    checkpoint(token);
    if (prototypeParent !== null) {
      throw malformed(context, `${path} must be a plain data object`);
    }
  }
  // Reflection is deliberately incremental. A Proxy trap can re-enter its
  // controller and revoke this candidate; the checkpoint must run before the
  // next trap instead of letting a stale object manufacture more intent.
  const keys = Reflect.ownKeys(value);
  checkpoint(token);
  const descriptors = [];
  for (const key of keys) {
    if (typeof key === "symbol") {
      throw malformed(context, `${path} must not contain symbol properties`);
    }
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    checkpoint(token);
    if (!descriptor) {
      throw malformed(context, `${path}.${key} changed during admission`);
    }
    descriptors.push([key, descriptor]);
  }
  let arrayElementCount = 0;
  for (const [key, descriptor] of descriptors) {
    if (array && key === "length") continue;
    if (array) {
      const index = Number(key);
      if (
        Number.isInteger(index) &&
        index >= 0 &&
        index < 0xffff_ffff &&
        String(index) === key
      ) {
        arrayElementCount++;
      }
    }
    if (!descriptor.enumerable || descriptor.get || descriptor.set) {
      throw malformed(context, `${path}.${key} must be an enumerable data property`);
    }
  }

  // Never preserve a client prototype. Mandatory fields and role members must
  // come from admitted own data, not inherited getters or prototype pollution.
  const copy = array ? [] : Object.create(null);
  active.add(value);
  try {
    for (const [key, descriptor] of descriptors) {
      if (array && key === "length") continue;
      Object.defineProperty(copy, key, {
        value: materialize(
          descriptor.value,
          context,
          `${path}.${key}`,
          active,
          checkpoint,
          token,
        ),
        enumerable: true,
        writable: false,
        configurable: false,
      });
    }
  } finally {
    active.delete(value);
  }
  if (array) {
    const length = descriptors.find(([key]) => key === "length")?.[1]?.value;
    // Считаем только канонические индексы: строковое свойство не должно
    // маскировать дыру, а обход до `length` позволил бы разреженному hostile
    // input с огромной длиной превратить admission в линейный DoS.
    if (arrayElementCount !== length || copy.length !== length) {
      throw malformed(context, `${path} must be a dense data array`);
    }
  }
  return Object.freeze(copy);
}

export function conflictError(conflicts) {
  const payload = Object.freeze(conflicts.map((conflict) => Object.freeze(conflict)));
  const error = new Error(
    `output_conflict: ${payload.map(({ role, category, code }) => (
      `${role} (${code === null ? category : code})`
    )).join(", ")}`,
  );
  error.name = "OutputConflictError";
  error.code = "output_conflict";
  error.conflicts = payload;
  return error;
}

/**
 * Допустить полный resolver snapshot до первого императивного эффекта.
 *
 * Это намеренно не проверка provenance: здесь запрещается observable ordinary
 * Unreachable и повреждённый контейнер, который иначе стёр бы или испортил CSS.
 *
 * @param {*} result
 * @param {string} context
 * @param {(token: unknown) => void} [checkpoint] Internal cancellation seam;
 *   controllers inject an owner assertion between client-owned Proxy traps.
 * @param {*} [token]
 * @returns {*}
 */
export function admitSnapshot(result, context, checkpoint = NO_CHECKPOINT, token) {
  if (isRecord(result) && admittedSnapshots.has(result)) return result;
  const snapshot = materialize(result, context, "result", new Set(), checkpoint, token);
  if (
    !isRecord(snapshot) ||
    !Array.isArray(snapshot.outputBindings) ||
    !isRecord(snapshot.vars) ||
    !isRecord(snapshot.roles)
  ) {
    throw malformed(context, "resolveTheme must return {outputBindings, vars, roles}");
  }

  const outputBindings = new Set();
  for (const name of snapshot.outputBindings) {
    if (!isCanonicalOutputBindingName(name)) {
      throw malformed(
        context,
        "outputBindings must contain canonical lower-case ASCII CSS custom-property names",
      );
    }
    if (outputBindings.has(name)) {
      throw malformed(context, `outputBindings contains duplicate '${name}'`);
    }
    outputBindings.add(name);
  }

  const conflicts = [];
  for (const [roleKey, role] of Object.entries(snapshot.roles)) {
    if (!isRecord(role)) {
      throw malformed(context, `role '${roleKey}' must be an object`);
    }
    if (typeof role.kind !== "string") {
      throw malformed(context, `role '${roleKey}' lacks kind`);
    }
    if (
      role.cssVar !== undefined &&
      (typeof role.cssVar !== "string" || !outputBindings.has(role.cssVar))
    ) {
      throw malformed(context, `role '${roleKey}' references an undeclared output binding`);
    }
    if (role.kind !== "failure") continue;
    if (typeof role.code !== "string" || typeof role.message !== "string") {
      throw malformed(context, `failure '${roleKey}' lacks code/message`);
    }
    if (role.category === "unresolved") continue;
    if (role.category !== "unreachable") {
      throw malformed(context, `failure '${roleKey}' has an unknown category`);
    }
    conflicts.push({
      role: roleKey,
      category: "unreachable",
      code: role.code,
      message: role.message,
    });
  }
  if (conflicts.length > 0) throw conflictError(conflicts);

  for (const [name, value] of Object.entries(snapshot.vars)) {
    if (!outputBindings.has(name) || typeof value !== "string") {
      throw malformed(context, "vars must be a string-valued subset of outputBindings");
    }
  }
  admittedSnapshots.add(snapshot);
  return snapshot;
}
