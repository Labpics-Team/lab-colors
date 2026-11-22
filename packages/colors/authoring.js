import { MAX_SECTION_ENTRIES_V1 } from "./program-wire/encoding.js";

const surfaces = new WeakMap();
const roles = new WeakMap();
const uses = new WeakMap();
const contrastCriteria = new Set([
  "wcag22-text", "wcag22-large-text", "wcag22-ui", "wcag22-graphic",
]);

export class ColorAuthoringError extends Error {
  constructor(code, path, message) {
    super(`${path}: ${message}`);
    this.name = "ColorAuthoringError";
    this.code = code;
    this.path = path;
  }
}

export function authoringError(path, message, code = "color_invalid_definition") {
  throw new ColorAuthoringError(code, path, message);
}

function read(value, key, path) {
  try { return value[key]; }
  catch { authoringError(path, "property could not be read"); }
}

function record(value, path) {
  if (value === null || typeof value !== "object") authoringError(path, "expected an object");
  return value;
}

function keys(value, path) {
  record(value, path);
  try { return Object.keys(value); }
  catch { authoringError(path, "properties could not be read"); }
}

function allowedKeys(value, allowed, path) {
  for (const key of keys(value, path)) {
    if (!allowed.includes(key)) authoringError(`${path}.${key}`, "unknown option");
  }
}

function name(value, path) {
  if (typeof value !== "string" || !/^[A-Za-z_][A-Za-z0-9_-]*$/u.test(value)) {
    authoringError(path, "expected a non-empty ASCII identifier");
  }
  return value;
}

function roleId(value) {
  if (typeof value !== "string" || value.length === 0) authoringError("role.id", "expected a non-empty opaque string");
  return value;
}

function number(value, path, min, max) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) {
    authoringError(path, "number is outside the declared domain");
  }
  return value === 0 ? 0 : value;
}

function array(value, path, minLength, maxLength) {
  let valid;
  try { valid = Array.isArray(value); }
  catch { valid = false; }
  if (!valid) authoringError(path, "expected an array");
  const length = read(value, "length", path);
  if (!Number.isInteger(length) || length < minLength || length > maxLength) {
    authoringError(path, "array length is outside the declared domain");
  }
  return Array.from({ length }, (_, i) => read(value, i, `${path}[${i}]`));
}

/** Parsing/encoding only: this layer never computes a color-space value. */
export function authoredRgb(value, path) {
  if (typeof value === "string") {
    if (!/^#[\da-f]{6}$/iu.test(value)) authoringError(path, "expected #RRGGBB without alpha");
    return Object.freeze([1, 3, 5].map((offset) => Number.parseInt(value.slice(offset, offset + 2), 16)));
  }
  const channels = array(value, path, 3, 3);
  for (let i = 0; i < channels.length; i++) {
    if (!Number.isInteger(channels[i]) || channels[i] < 0 || channels[i] > 255) {
      authoringError(`${path}[${i}]`, "expected an integer sRGB8 channel");
    }
  }
  return Object.freeze(channels);
}

function range(value, path, min, max) {
  const bounds = array(value, path, 2, 2).map((item, i) => number(item, `${path}[${i}]`, min, max));
  if (bounds[0] > bounds[1]) authoringError(path, "lower bound must not exceed upper bound");
  return Object.freeze(bounds);
}

function roleOptions(value = {}) {
  const path = "role.options";
  allowedKeys(value, ["domain", "identity", "maxChange"], path);
  const domain = read(value, "domain", path) ?? "srgb";
  if (domain !== "srgb" && domain !== "neutral") authoringError(`${path}.domain`, "expected srgb or neutral");
  const identity = read(value, "identity", path);
  let copiedIdentity;
  if (identity !== undefined) {
    allowedKeys(identity, ["maxHueDeviation", "chroma", "lightness"], `${path}.identity`);
    const maxHueDeviation = read(identity, "maxHueDeviation", path);
    const chroma = read(identity, "chroma", path);
    const lightness = read(identity, "lightness", path);
    copiedIdentity = Object.freeze({
      ...(maxHueDeviation === undefined ? {} : { maxHueDeviation: number(maxHueDeviation, `${path}.identity.maxHueDeviation`, 0, 180) }),
      ...(chroma === undefined ? {} : { chroma: range(chroma, `${path}.identity.chroma`, 0, 1) }),
      ...(lightness === undefined ? {} : { lightness: range(lightness, `${path}.identity.lightness`, 0, 1) }),
    });
  }
  const maxChange = read(value, "maxChange", path);
  const checkedMaxChange = maxChange === undefined ? undefined : number(maxChange, `${path}.maxChange`, 0, 2);
  return Object.freeze({ domain, identity: copiedIdentity, maxChange: checkedMaxChange });
}

function inputSurface(value, owner, path) {
  const node = surfaces.get(value);
  if (node === undefined || node.owner !== owner) authoringError(path, "surface belongs to another declaration", "color_foreign_reference");
  return node;
}

function surface(value, owner, path) {
  const node = surfaces.get(value) ?? uses.get(value);
  if (node === undefined || node.owner !== owner) authoringError(path, "surface belongs to another declaration", "color_foreign_reference");
  return node;
}

function usePolicy(value = {}, owner) {
  const path = "role.on.policy";
  allowedKeys(value, ["contrast", "opacity", "tone", "separation"], path);
  const contrast = read(value, "contrast", path);
  if (contrast !== undefined && !contrastCriteria.has(contrast)) authoringError(`${path}.contrast`, "unknown contrast criterion");
  const opacity = number(read(value, "opacity", path) ?? 1, `${path}.opacity`, 0, 1);
  const tone = read(value, "tone", path);
  const separation = read(value, "separation", path);
  let copiedTone;
  if (tone !== undefined) {
    allowedKeys(tone, ["from", "curve"], `${path}.tone`);
    const from = inputSurface(read(tone, "from", path), owner, `${path}.tone.from`);
    const curve = array(read(tone, "curve", path), `${path}.tone.curve`, 2, MAX_SECTION_ENTRIES_V1)
      .map((point, i) => Object.freeze(array(point, `${path}.tone.curve[${i}]`, 2, 2)
        .map((coordinate, j) => number(coordinate, `${path}.tone.curve[${i}][${j}]`, 0, 1))));
    if (curve[0][0] !== 0 || curve.at(-1)[0] !== 1) authoringError(`${path}.tone.curve`, "input endpoints must be 0 and 1");
    for (let i = 1; i < curve.length; i++) {
      if (curve[i - 1][0] >= curve[i][0]) authoringError(`${path}.tone.curve`, "input coordinates must strictly increase");
    }
    copiedTone = Object.freeze({ from, curve: Object.freeze(curve) });
  }
  return Object.freeze({ contrast, opacity, tone: copiedTone,
    separation: separation === undefined ? undefined : range(separation, `${path}.separation`, 0, 2) });
}

export function colors() {
  const owner = {};
  const roleNames = new Set();
  const surfaceNames = new Set();

  const declareRole = (id, anchor, options, adaptive) => {
    const checkedId = roleId(id);
    const copiedAnchor = authoredRgb(anchor, "role.anchor");
    const copiedOptions = roleOptions(options);
    // All caller-owned reads precede mutation; nested declarations cannot be overwritten.
    if (roleNames.has(checkedId)) authoringError("role.id", "duplicate role id");
    const handle = Object.freeze({
      id: checkedId,
      anchor: copiedAnchor,
      on(against, policy) {
        const parent = surface(against, owner, "role.on.surface");
        const copiedPolicy = usePolicy(policy, owner);
        if (!adaptive && copiedPolicy.tone !== undefined) authoringError("role.on.policy.tone", "a fixed role cannot adapt its tone");
        const use = Object.freeze({ role: handle, against });
        uses.set(use, Object.freeze({ kind: "use", owner, handle: use, role: roles.get(handle), against: parent, policy: copiedPolicy }));
        return use;
      },
    });
    roles.set(handle, Object.freeze({ kind: "role", owner, id: checkedId, anchor: copiedAnchor, adaptive, options: copiedOptions }));
    roleNames.add(checkedId);
    return handle;
  };

  return Object.freeze({
    surface(id) {
      const checkedId = name(id, "surface.id");
      if (surfaceNames.has(checkedId)) authoringError("surface.id", "duplicate surface id");
      const handle = Object.freeze({ id: checkedId });
      surfaces.set(handle, Object.freeze({ kind: "input", owner, id: checkedId }));
      surfaceNames.add(checkedId);
      return handle;
    },
    role(id, anchor, options) { return declareRole(id, anchor, options, true); },
    fixed(id, anchor) { return declareRole(id, anchor, undefined, false); },
    async compile(outputs) {
      const entries = keys(outputs, "compile.outputs").map((key) => {
        const checkedKey = name(key, "compile.output");
        const node = uses.get(read(outputs, key, `compile.outputs.${key}`));
        if (node === undefined || node.owner !== owner) authoringError(`compile.outputs.${key}`, "expected a local use", "color_foreign_reference");
        return Object.freeze({ name: checkedKey, node });
      });
      if (entries.length === 0 || entries.length > MAX_SECTION_ENTRIES_V1) authoringError("compile.outputs", "expected a bounded non-empty output map");
      const { compileColorSystem } = await import("./authoring-program.js");
      return compileColorSystem(Object.freeze(entries));
    },
  });
}
