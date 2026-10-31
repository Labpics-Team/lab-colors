// Shared canonical scalar encoding for the two supported Program envelopes.
// Каноническая wire-грамматика авторского Draft-графа Program (v1) — JS-зеркало
// Rust-модуля crates/labcolors-core/src/program/wire.rs. Одни байты — одна
// декларация: заголовок LCPW + u16 version + u32 total_len, секции строго в
// порядке полей CoreProgramDraftV1, LE-скаляры, opacity как f64-bits.
//
// Слой 1 двухслойного контракта: байты <-> декларации. Семантику графа
// проверяет Rust-компилятор Program; этот модуль не выражает семантических
// отказов и не изобретает fallback.

export const PROGRAM_WIRE_MAGIC_V1 = Object.freeze([0x4c, 0x43, 0x50, 0x57]); // LCPW
export const PROGRAM_WIRE_VERSION_V1 = 1;
export const MAX_SECTION_ENTRIES_V1 = 4096;

export const SECTION_ORDER_V1 = Object.freeze([
  "sources",
  "targets",
  "families",
  "jointSelection",
  "surfaceInputPorts",
  "opacityInputs",
  "paints",
  "surfaces",
  "occurrences",
  "presentationRoots",
  "presentationTargets",
  "hardConstraints",
  "reportConstraints",
  "outputs",
]);

export const KIND_EXACT_VISIBLE_UNARY = 1;
export const KIND_EXACT_INTRINSIC_RELATION = 4;
export const KIND_WCAG22_VISIBLE_UNARY = 9;
export const KIND_CLEAN_SET = 10;

export const SURROUND_AVERAGE_V1 = 1;
export const SURROUND_DIM_V1 = 2;
export const SURROUND_DARK_V1 = 3;

export const WCAG22_SC143_TEXT_DEFAULT_V1 = 1;
export const WCAG22_SC143_TEXT_LARGE_SCALE_V1 = 2;
export const WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1 = 3;
export const WCAG22_SC1411_GRAPHICAL_OBJECT_V1 = 4;

/** Typed-отказ канонического builder-а — зеркало Rust ProgramWireEncodeErrorV1. */
export class ProgramWireError extends Error {
  /**
   * @param {string} code машинный код класса отказа
   * @param {string} message человекочитаемая причина
   */
  constructor(code, message) {
    super(message);
    this.name = "ProgramWireError";
    this.code = code;
  }
}

export const PROGRAM_WIRE_TOO_MANY_ENTRIES = "PROGRAM_WIRE_TOO_MANY_ENTRIES";
export const PROGRAM_WIRE_INVALID_DECLARATION = "PROGRAM_WIRE_INVALID_DECLARATION";

export function invalid(message) {
  throw new ProgramWireError(PROGRAM_WIRE_INVALID_DECLARATION, message);
}

// Диагностика не преобразует отклонённое значение и не вызывает его hooks.
export function u32Value(value, what) {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) {
    invalid(`${what} must be a u32 (received type ${typeof value})`);
  }
  return value >>> 0;
}

export function byteValue(value, what) {
  if (!Number.isInteger(value) || value < 0 || value > 0xff) {
    invalid(`${what} must be a byte (received type ${typeof value})`);
  }
  return value;
}

export function f64Value(value, what) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    invalid(`${what} must be a finite binary64 number (received type ${typeof value})`);
  }
  return value;
}

// Wire producers admit the same physical domains as Core, before emitting bytes.
export function opacityValue(value, what) {
  const admitted = f64Value(value, what);
  if (admitted < 0 || admitted > 1) invalid(`${what} must be in [0, 1]`);
  return admitted === 0 ? 0 : admitted;
}

export function positiveF64Value(value, what) {
  const admitted = f64Value(value, what);
  if (admitted <= 0) invalid(`${what} must be positive`);
  return admitted;
}

export function positiveUnitRatioValue(value, what) {
  const admitted = positiveF64Value(value, what);
  if (admitted > 1) invalid(`${what} must be in (0, 1]`);
  return admitted;
}

export function memberValue(value, allowed, what) {
  if (!allowed.includes(value)) {
    invalid(`${what} must be one of ${allowed.join(", ")} (received type ${typeof value})`);
  }
  return value;
}

export const SURROUND_VALUES_V1 = Object.freeze([
  SURROUND_AVERAGE_V1,
  SURROUND_DIM_V1,
  SURROUND_DARK_V1,
]);

export const WCAG22_CRITERIA_V1 = Object.freeze([
  WCAG22_SC143_TEXT_DEFAULT_V1,
  WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
  WCAG22_SC1411_GRAPHICAL_OBJECT_V1,
]);

// Объявление задаёт индексные элементы массива, а не его переопределяемый
// iterator. Длина фиксируется до чтения элементов, чтобы snapshot и wire-count
// описывали один ограниченный набор даже при пользовательском iterator.
export function inputProperty(value, key, what) {
  try {
    return value[key];
  } catch {
    invalid(`${what} could not be read`);
  }
}

export function inputArrayLength(values, what) {
  // Array.isArray тоже бросает TypeError на отозванном Proxy: это отказ входа.
  let isArray;
  try {
    isArray = Array.isArray(values);
  } catch {
    invalid(`${what} must be a readable array`);
  }
  if (!isArray) invalid(`${what} must be an array`);
  return inputProperty(values, "length", what);
}

export function indexedValues(values, length, what) {
  return Array.from({ length }, (_, index) => inputProperty(values, index, what));
}

export function candidateList(candidates, what) {
  const length = inputArrayLength(candidates, what);
  if (!Number.isInteger(length) || length <= 0) {
    invalid(`${what} must be a non-empty array`);
  }
  // Вложенный счётчик имеет тот же wire-limit, что и секция. Проверка до
  // копирования и чтения элементов сохраняет builder при отказе.
  if (length > MAX_SECTION_ENTRIES_V1) {
    throw new ProgramWireError(
      PROGRAM_WIRE_TOO_MANY_ENTRIES,
      `${what} exceeds the wire v1 count limit ${MAX_SECTION_ENTRIES_V1}`,
    );
  }
  return indexedValues(candidates, length, what);
}

export function rgbBytes(rgb, what) {
  if (inputArrayLength(rgb, what) !== 3) {
    invalid(`${what} must be an [r, g, b] triple`);
  }
  return indexedValues(rgb, 3, what).map((channel, index) => byteValue(channel, `${what}[${index}]`));
}

/** Little-endian section payload. Only initialized bytes reach the wire. */
export class ByteSink {
  #data = new Uint8Array(0);
  #view = new DataView(this.#data.buffer);
  #length = 0;

  get bytes() { return this.#data.subarray(0, this.#length); }

  #reserve(width) {
    const at = this.#length;
    const end = at + width;
    if (end > 0xffff_ffff) {
      throw new ProgramWireError(PROGRAM_WIRE_TOO_MANY_ENTRIES, "section exceeds the u32 wire length");
    }
    if (end > this.#data.length) {
      const data = new Uint8Array(Math.max(end, Math.min(this.#data.length * 2, 0xffff_ffff)));
      data.set(this.#data);
      this.#data = data;
      this.#view = new DataView(data.buffer);
    }
    this.#length = end;
    return at;
  }

  u8(value) { const at = this.#reserve(1); this.#view.setUint8(at, value); }
  u16(value) { const at = this.#reserve(2); this.#view.setUint16(at, value, true); }
  u32(value) { const at = this.#reserve(4); this.#view.setUint32(at, value, true); }
  u64(value) { const at = this.#reserve(8); this.#view.setBigUint64(at, value, true); }
  f64Bits(value) { const at = this.#reserve(8); this.#view.setFloat64(at, value, true); }
  rgb(value) { this.append(value); }
  append(bytes) { const at = this.#reserve(bytes.length); this.#data.set(bytes, at); }
}
