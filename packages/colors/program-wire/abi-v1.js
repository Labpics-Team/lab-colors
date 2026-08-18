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
export const KIND_EXACT_INTRINSIC_UNARY = 2;
export const KIND_FAMILY_MEMBERSHIP = 3;
export const KIND_EXACT_INTRINSIC_RELATION = 4;
export const KIND_EXACT_VISIBLE_RELATION = 5;
export const KIND_INTRINSIC_DISTINCTION = 6;
export const KIND_VISIBLE_DISTINCTION = 7;
export const KIND_FAMILY_CATEGORY_RELATION = 8;
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

function invalid(message) {
  throw new ProgramWireError(PROGRAM_WIRE_INVALID_DECLARATION, message);
}

function u32Value(value, what) {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) {
    invalid(`${what} must be a u32, got ${value}`);
  }
  return value >>> 0;
}

function byteValue(value, what) {
  if (!Number.isInteger(value) || value < 0 || value > 0xff) {
    invalid(`${what} must be a byte, got ${value}`);
  }
  return value;
}

function f64Value(value, what) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    invalid(`${what} must be a non-NaN number, got ${value}`);
  }
  return value;
}

function memberValue(value, allowed, what) {
  if (!allowed.includes(value)) {
    invalid(`${what} must be one of ${allowed.join(", ")}, got ${value}`);
  }
  return value;
}

const SURROUND_VALUES_V1 = Object.freeze([
  SURROUND_AVERAGE_V1,
  SURROUND_DIM_V1,
  SURROUND_DARK_V1,
]);

const WCAG22_CRITERIA_V1 = Object.freeze([
  WCAG22_SC143_TEXT_DEFAULT_V1,
  WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
  WCAG22_SC1411_GRAPHICAL_OBJECT_V1,
]);

function candidateList(candidates, what) {
  if (!Array.isArray(candidates) || candidates.length === 0) {
    invalid(`${what} must be a non-empty array`);
  }
  return candidates;
}

function rgbBytes(rgb, what) {
  if (!Array.isArray(rgb) || rgb.length !== 3) {
    invalid(`${what} must be an [r, g, b] triple`);
  }
  return rgb.map((channel, index) => byteValue(channel, `${what}[${index}]`));
}

/** Растущий LE-байтовый буфер: те же представления, что у Rust-стороны. */
class ByteSink {
  constructor() {
    /** @type {number[]} */
    this.bytes = [];
  }

  u8(value) {
    this.bytes.push(value & 0xff);
  }

  u16(value) {
    this.bytes.push(value & 0xff, (value >>> 8) & 0xff);
  }

  u32(value) {
    this.bytes.push(
      value & 0xff,
      (value >>> 8) & 0xff,
      (value >>> 16) & 0xff,
      (value >>> 24) & 0xff,
    );
  }

  f64Bits(value) {
    const view = new DataView(new ArrayBuffer(8));
    view.setFloat64(0, value, true);
    for (let index = 0; index < 8; index += 1) {
      this.bytes.push(view.getUint8(index));
    }
  }

  rgb(rgb) {
    this.bytes.push(rgb[0], rgb[1], rgb[2]);
  }
}

/**
 * Канонический builder байтов wire v1 — двойник Rust ProgramWireBuilderV1.
 * Клиент объявляет граф; builder эмитирует единственное каноническое
 * представление. Joint selection непредставим на wire v1 by design.
 */
export class ProgramWireBuilderV1 {
  constructor() {
    /** @type {Map<string, {count: number, sink: ByteSink}>} */
    this.sections = new Map(
      SECTION_ORDER_V1.map((name) => [name, { count: 0, sink: new ByteSink() }]),
    );
  }

  section(name) {
    const section = this.sections.get(name);
    if (section === undefined) invalid(`unknown section ${name}`);
    return section;
  }

  entry(name) {
    const section = this.section(name);
    section.count += 1;
    return section.sink;
  }

  source(id, rgb) {
    const sink = this.entry("sources");
    sink.u32(u32Value(id, "source id"));
    sink.rgb(rgbBytes(rgb, "source rgb"));
    return this;
  }

  fixedTarget(id, source) {
    const sink = this.entry("targets");
    sink.u32(u32Value(id, "target id"));
    sink.u8(1);
    sink.u32(u32Value(source, "target source"));
    return this;
  }

  finiteTarget(id, candidates) {
    const checkedId = u32Value(id, "target id");
    const checked = candidateList(candidates, "finite target candidates").map(
      (candidate, index) => ({
        id: u32Value(candidate.id, "candidate[" + index + "] id"),
        rgb: rgbBytes(candidate.rgb, "candidate[" + index + "] rgb"),
        opacity: f64Value(candidate.opacity, "candidate[" + index + "] opacity"),
      }),
    );
    const sink = this.entry("targets");
    sink.u32(checkedId);
    sink.u8(2);
    sink.u32(checked.length);
    for (const candidate of checked) {
      sink.u32(candidate.id);
      sink.rgb(candidate.rgb);
      sink.f64Bits(candidate.opacity);
    }
    return this;
  }

  family(id, releaseBytes) {
    const sink = this.entry("families");
    sink.u32(u32Value(id, "family id"));
    if (!Array.isArray(releaseBytes) || releaseBytes.length !== 32) {
      invalid("family release must be 32 bytes");
    }
    for (const byte of releaseBytes) {
      sink.u8(byteValue(byte, "family release byte"));
    }
    return this;
  }

  surfaceInputPort(id) {
    this.entry("surfaceInputPorts").u32(u32Value(id, "surface input port id"));
    return this;
  }

  opacityInput(id, value) {
    const checkedId = u32Value(id, "opacity input id");
    const checkedValue = f64Value(value, "opacity input value");
    const sink = this.entry("opacityInputs");
    sink.u32(checkedId);
    sink.f64Bits(checkedValue);
    return this;
  }

  solidPaint(id, target) {
    const sink = this.entry("paints");
    sink.u32(u32Value(id, "paint id"));
    sink.u8(1);
    sink.u32(u32Value(target, "paint target"));
    return this;
  }

  opacityPaint(id, source, opacity) {
    const sink = this.entry("paints");
    sink.u32(u32Value(id, "paint id"));
    sink.u8(2);
    sink.u32(u32Value(source, "paint source"));
    sink.u32(u32Value(opacity, "paint opacity input"));
    return this;
  }

  inputSurface(id, input) {
    const sink = this.entry("surfaces");
    sink.u32(u32Value(id, "surface id"));
    sink.u8(1);
    sink.u32(u32Value(input, "surface input port"));
    return this;
  }

  occurrenceSurface(id, occurrence) {
    const sink = this.entry("surfaces");
    sink.u32(u32Value(id, "surface id"));
    sink.u8(2);
    sink.u32(u32Value(occurrence, "surface occurrence"));
    return this;
  }

  sourceOverOccurrence(id, subject, against, adaptingLuminance, backgroundRatio, surround) {
    const checkedId = u32Value(id, "occurrence id");
    const checkedSubject = u32Value(subject, "occurrence subject");
    const checkedAgainst = u32Value(against, "occurrence surface");
    const checkedLuminance = f64Value(adaptingLuminance, "occurrence adapting luminance");
    const checkedRatio = f64Value(backgroundRatio, "occurrence background ratio");
    const checkedSurround = memberValue(surround, SURROUND_VALUES_V1, "occurrence surround");
    const sink = this.entry("occurrences");
    sink.u32(checkedId);
    sink.u32(checkedSubject);
    sink.u32(checkedAgainst);
    sink.f64Bits(checkedLuminance);
    sink.f64Bits(checkedRatio);
    sink.u8(checkedSurround);
    return this;
  }

  presentationRoot(id, terminal) {
    const sink = this.entry("presentationRoots");
    sink.u32(u32Value(id, "presentation root id"));
    sink.u32(u32Value(terminal, "presentation terminal"));
    return this;
  }

  presentationTarget(root, occurrence) {
    const sink = this.entry("presentationTargets");
    sink.u32(u32Value(root, "presentation root"));
    sink.u32(u32Value(occurrence, "presentation occurrence"));
    return this;
  }

  constraintEntry(hard) {
    return this.entry(hard ? "hardConstraints" : "reportConstraints");
  }

  exactVisibleUnary(hard, id, occurrence, expectedRgb) {
    const sink = this.constraintEntry(hard);
    sink.u32(u32Value(id, "constraint id"));
    sink.u8(KIND_EXACT_VISIBLE_UNARY);
    sink.u32(u32Value(occurrence, "constraint occurrence"));
    sink.rgb(rgbBytes(expectedRgb, "expected rgb"));
    return this;
  }

  wcag22VisibleUnary(hard, id, occurrence, criterion) {
    const checkedId = u32Value(id, "constraint id");
    const checkedOccurrence = u32Value(occurrence, "constraint occurrence");
    const checkedCriterion = memberValue(criterion, WCAG22_CRITERIA_V1, "wcag22 criterion");
    const sink = this.constraintEntry(hard);
    sink.u32(checkedId);
    sink.u8(KIND_WCAG22_VISIBLE_UNARY);
    sink.u32(checkedOccurrence);
    sink.u8(checkedCriterion);
    return this;
  }

  exactIntrinsicRelationHard(id, reference, candidates) {
    const checkedId = u32Value(id, "constraint id");
    const checkedReference = u32Value(reference, "relation reference");
    const checked = candidateList(candidates, "relation candidates").map(
      (candidate, index) => u32Value(candidate, "relation candidate[" + index + "]"),
    );
    const sink = this.constraintEntry(true);
    sink.u32(checkedId);
    sink.u8(KIND_EXACT_INTRINSIC_RELATION);
    sink.u32(checkedReference);
    sink.u32(checked.length);
    for (const candidate of checked) {
      sink.u32(candidate);
    }
    return this;
  }

  output(slot, paint) {
    const sink = this.entry("outputs");
    sink.u32(u32Value(slot, "output slot"));
    sink.u32(u32Value(paint, "output paint"));
    return this;
  }

  /** Эмитирует единственные канонические байты объявленного графа. */
  finish() {
    const header = new ByteSink();
    for (const byte of PROGRAM_WIRE_MAGIC_V1) header.u8(byte);
    header.u16(PROGRAM_WIRE_VERSION_V1);
    header.u32(0); // patched below

    const body = new ByteSink();
    for (const name of SECTION_ORDER_V1) {
      const { count, sink } = this.section(name);
      if (count > MAX_SECTION_ENTRIES_V1) {
        throw new ProgramWireError(
          PROGRAM_WIRE_TOO_MANY_ENTRIES,
          `section ${name} exceeds the wire limit: ${count}`,
        );
      }
      body.u32(count);
      // Поэлементно: spread разворачивает массив в аргументы вызова и падает
      // на больших секциях (лимит аргументов V8), а не по нашему typed-отказу.
      for (const byte of sink.bytes) {
        body.bytes.push(byte);
      }
    }

    const total = header.bytes.length + body.bytes.length;
    const bytes = new Uint8Array(total);
    bytes.set(header.bytes, 0);
    bytes.set(body.bytes, header.bytes.length);
    const view = new DataView(bytes.buffer);
    view.setUint32(6, total, true);
    return bytes;
  }
}
