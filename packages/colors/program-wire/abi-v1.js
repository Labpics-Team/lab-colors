import {
  PROGRAM_WIRE_MAGIC_V1,
  PROGRAM_WIRE_VERSION_V1,
  MAX_SECTION_ENTRIES_V1,
  SECTION_ORDER_V1,
  KIND_EXACT_VISIBLE_UNARY,
  KIND_EXACT_INTRINSIC_RELATION,
  KIND_WCAG22_VISIBLE_UNARY,
  KIND_CLEAN_SET,
  ProgramWireError,
  PROGRAM_WIRE_TOO_MANY_ENTRIES,
  invalid,
  u32Value,
  byteValue,
  f64Value,
  memberValue,
  SURROUND_VALUES_V1,
  WCAG22_CRITERIA_V1,
  inputProperty,
  inputArrayLength,
  indexedValues,
  candidateList,
  rgbBytes,
  ByteSink
} from "./encoding.js";
export {
  SURROUND_AVERAGE_V1,
  SURROUND_DIM_V1,
  SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1,
  WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
  WCAG22_SC1411_GRAPHICAL_OBJECT_V1,
  ProgramWireError,
  PROGRAM_WIRE_TOO_MANY_ENTRIES,
  PROGRAM_WIRE_INVALID_DECLARATION
} from "./encoding.js";

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
    if (section === undefined) invalid("unknown section");
    return section;
  }

  entry(name) {
    const section = this.section(name);
    section.count += 1;
    return section.sink;
  }

  source(id, rgb) {
    const checkedId = u32Value(id, "source id");
    const checkedRgb = rgbBytes(rgb, "source rgb");
    const sink = this.entry("sources");
    sink.u32(checkedId);
    sink.rgb(checkedRgb);
    return this;
  }

  fixedTarget(id, source) {
    const checkedId = u32Value(id, "target id");
    const checkedSource = u32Value(source, "target source");
    const sink = this.entry("targets");
    sink.u32(checkedId);
    sink.u8(1);
    sink.u32(checkedSource);
    return this;
  }

  finiteTarget(id, candidates) {
    const checkedId = u32Value(id, "target id");
    const checked = candidateList(candidates, "finite target candidates").map(
      (candidate, index) => {
        if (candidate === null || typeof candidate !== "object") {
          invalid(`candidate[${index}] must be an object`);
        }
        return {
          id: u32Value(inputProperty(candidate, "id", "candidate id"), "candidate[" + index + "] id"),
          rgb: rgbBytes(inputProperty(candidate, "rgb", "candidate rgb"), "candidate[" + index + "] rgb"),
          opacity: f64Value(inputProperty(candidate, "opacity", "candidate opacity"), "candidate[" + index + "] opacity"),
        };
      },
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
    const checkedId = u32Value(id, "family id");
    if (inputArrayLength(releaseBytes, "family release") !== 32) {
      invalid("family release must be 32 bytes");
    }
    const checkedRelease = indexedValues(releaseBytes, 32, "family release").map((byte) => byteValue(byte, "family release byte"));
    const sink = this.entry("families");
    sink.u32(checkedId);
    for (const byte of checkedRelease) sink.u8(byte);
    return this;
  }

  surfaceInputPort(id) {
    const checkedId = u32Value(id, "surface input port id");
    this.entry("surfaceInputPorts").u32(checkedId);
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
    const checkedId = u32Value(id, "paint id");
    const checkedTarget = u32Value(target, "paint target");
    const sink = this.entry("paints");
    sink.u32(checkedId);
    sink.u8(1);
    sink.u32(checkedTarget);
    return this;
  }

  opacityPaint(id, source, opacity) {
    const checkedId = u32Value(id, "paint id");
    const checkedSource = u32Value(source, "paint source");
    const checkedOpacity = u32Value(opacity, "paint opacity input");
    const sink = this.entry("paints");
    sink.u32(checkedId);
    sink.u8(2);
    sink.u32(checkedSource);
    sink.u32(checkedOpacity);
    return this;
  }

  inputSurface(id, input) {
    const checkedId = u32Value(id, "surface id");
    const checkedInput = u32Value(input, "surface input port");
    const sink = this.entry("surfaces");
    sink.u32(checkedId);
    sink.u8(1);
    sink.u32(checkedInput);
    return this;
  }

  occurrenceSurface(id, occurrence) {
    const checkedId = u32Value(id, "surface id");
    const checkedOccurrence = u32Value(occurrence, "surface occurrence");
    const sink = this.entry("surfaces");
    sink.u32(checkedId);
    sink.u8(2);
    sink.u32(checkedOccurrence);
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
    const checkedId = u32Value(id, "presentation root id");
    const checkedTerminal = u32Value(terminal, "presentation terminal");
    const sink = this.entry("presentationRoots");
    sink.u32(checkedId);
    sink.u32(checkedTerminal);
    return this;
  }

  presentationTarget(root, occurrence) {
    const checkedRoot = u32Value(root, "presentation root");
    const checkedOccurrence = u32Value(occurrence, "presentation occurrence");
    const sink = this.entry("presentationTargets");
    sink.u32(checkedRoot);
    sink.u32(checkedOccurrence);
    return this;
  }

  constraintEntry(hard) {
    if (typeof hard !== "boolean") invalid("constraint hard flag must be a boolean");
    return this.entry(hard ? "hardConstraints" : "reportConstraints");
  }

  exactVisibleUnary(hard, id, occurrence, expectedRgb) {
    const checkedId = u32Value(id, "constraint id");
    const checkedOccurrence = u32Value(occurrence, "constraint occurrence");
    const checkedRgb = rgbBytes(expectedRgb, "expected rgb");
    const sink = this.constraintEntry(hard);
    sink.u32(checkedId);
    sink.u8(KIND_EXACT_VISIBLE_UNARY);
    sink.u32(checkedOccurrence);
    sink.rgb(checkedRgb);
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

  declaredSrgb8CleanSet(hard, id, root, occurrence) {
    const checkedId = u32Value(id, "constraint id");
    const checkedRoot = u32Value(root, "constraint presentation root");
    const checkedOccurrence = u32Value(occurrence, "constraint occurrence");
    const sink = this.constraintEntry(hard);
    sink.u32(checkedId);
    sink.u8(KIND_CLEAN_SET);
    sink.u32(checkedRoot);
    sink.u32(checkedOccurrence);
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
    const checkedSlot = u32Value(slot, "output slot");
    const checkedPaint = u32Value(paint, "output paint");
    const sink = this.entry("outputs");
    sink.u32(checkedSlot);
    sink.u32(checkedPaint);
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
