// Внутренняя граница допуска и DOM writer для снимков решённой темы.
//
// Успешный снимок может содержать явный None, незавершённый bounded search или
// численную неопределённость. Ordinary Unreachable означает, что объявленный
// output-контракт невыполним: применение остальных vars превратило бы ambient
// CSS в неявный fallback.

const LAB_VAR_PREFIX = "--lab-";
const admittedSnapshots = new WeakSet();

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function malformed(context, detail) {
  return new TypeError(`${context}: ${detail}`);
}

/** Копирует только JSON-подобные data properties и замораживает результат.
 * Getter/setter не вызываются: иначе проверенный `roles` мог измениться между
 * допуском и чтением `vars` в DOM writer. */
function materialize(value, context, path, active) {
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
  if (!array && prototype !== Object.prototype && prototype !== null) {
    throw malformed(context, `${path} must be a plain data object`);
  }
  if (Object.getOwnPropertySymbols(value).length > 0) {
    throw malformed(context, `${path} must not contain symbol properties`);
  }
  const descriptors = Object.getOwnPropertyDescriptors(value);
  for (const [key, descriptor] of Object.entries(descriptors)) {
    if (array && key === "length") continue;
    if (!descriptor.enumerable || descriptor.get || descriptor.set) {
      throw malformed(context, `${path}.${key} must be an enumerable data property`);
    }
  }

  const copy = array ? [] : Object.create(prototype);
  active.add(value);
  try {
    for (const [key, descriptor] of Object.entries(descriptors)) {
      if (array && key === "length") continue;
      Object.defineProperty(copy, key, {
        value: materialize(descriptor.value, context, `${path}.${key}`, active),
        enumerable: true,
        writable: false,
        configurable: false,
      });
    }
  } finally {
    active.delete(value);
  }
  if (array && copy.length !== descriptors.length.value) {
    throw malformed(context, `${path} must be a dense data array`);
  }
  return Object.freeze(copy);
}

function conflictError(conflicts) {
  const payload = Object.freeze(conflicts.map((conflict) => Object.freeze(conflict)));
  const error = new Error(
    `output_conflict: ${payload.map(({ role, code }) => `${role} (${code})`).join(", ")}`,
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
 * @returns {*}
 */
export function admitSnapshot(result, context) {
  if (isRecord(result) && admittedSnapshots.has(result)) return result;
  const snapshot = materialize(result, context, "result", new Set());
  if (!isRecord(snapshot) || !isRecord(snapshot.vars) || !isRecord(snapshot.roles)) {
    throw malformed(context, "resolveTheme must return {vars, roles}");
  }

  const conflicts = [];
  for (const [roleKey, role] of Object.entries(snapshot.roles)) {
    if (!isRecord(role)) {
      throw malformed(context, `role '${roleKey}' must be an object`);
    }
    if (typeof role.kind !== "string") {
      throw malformed(context, `role '${roleKey}' lacks kind`);
    }
    if (role.kind !== "failure") continue;
    if (typeof role.code !== "string" || typeof role.message !== "string") {
      throw malformed(context, `failure '${roleKey}' lacks code/message`);
    }
    if (role.category === "unresolved") continue;
    if (role.category !== "unreachable") {
      throw malformed(context, `failure '${roleKey}' has an unknown category`);
    }
    conflicts.push({ role: roleKey, code: role.code, message: role.message });
  }
  if (conflicts.length > 0) throw conflictError(conflicts);

  for (const [name, value] of Object.entries(snapshot.vars)) {
    if (!name.startsWith(LAB_VAR_PREFIX) || typeof value !== "string") {
      throw malformed(context, "vars must contain only string --lab-* entries");
    }
  }
  admittedSnapshots.add(snapshot);
  return snapshot;
}

/**
 * Записать уже допущенный словарь vars. Adaptive-кадры используют этот
 * приватный примитив, потому что интерполированный overlay не является новым
 * resolver snapshot; публичный путь проходит через `admitSnapshot`.
 *
 * @param {*} element
 * @param {Record<string, string>} vars
 * @param {string} context
 * @param {() => boolean} [owns] Проверка владения для reentrant controller write.
 * @returns {boolean} `false`, если более новая операция отозвала владение.
 */
export function writeVars(element, vars, context, owns = () => true) {
  if (!element || typeof element.style?.setProperty !== "function") {
    throw malformed(context, "first argument must be an element with a style");
  }
  if (!isRecord(vars)) {
    throw malformed(context, "vars must be an object");
  }
  const entries = Object.entries(vars);
  for (const [name, value] of entries) {
    if (!name.startsWith(LAB_VAR_PREFIX) || typeof value !== "string") {
      throw malformed(context, "vars must contain only string --lab-* entries");
    }
  }
  if (!owns()) return false;

  // Inline style — live-список: фиксируем удаляемые имена до мутации.
  const stale = [];
  for (let i = 0; i < element.style.length; i++) {
    const name = element.style.item(i);
    if (typeof name === "string" && name.startsWith(LAB_VAR_PREFIX)) stale.push(name);
  }
  for (const name of stale) {
    if (!owns()) return false;
    element.style.removeProperty(name);
    if (!owns()) return false;
  }
  for (const [name, value] of entries) {
    if (!owns()) return false;
    element.style.setProperty(name, value);
    if (!owns()) return false;
  }
  return true;
}
