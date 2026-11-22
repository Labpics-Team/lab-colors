import { ProgramWireBuilderV1 } from "./abi-v1.js";
import { ByteSink, byteValue, candidateList, f64Value, inputProperty, invalid,
  rgb24, u32Value, inputArrayLength, indexedValues, MAX_SECTION_ENTRIES_V1 } from "./encoding.js";

export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

/** V3 передаёт области, опоры и предпочтения; назначение цветов принадлежит Core. */
export class ProgramWireBuilderV3 extends ProgramWireBuilderV1 {
  colorTarget(id, anchor, options = {}) {
    const checkedId = u32Value(id, "target id");
    const checkedAnchor = rgb24(anchor, "color anchor");
    const domain = inputProperty(options, "domain", "color options");
    if (domain !== undefined && domain !== "srgb" && domain !== "neutral") {
      invalid("color domain must be srgb or neutral");
    }
    const rawIdentity = inputProperty(options, "identity", "color options");
    const identity = rawIdentity === undefined ? {} : rawIdentity;
    const hue = inputProperty(identity, "maxHueDeviation", "color identity");
    const chroma = inputProperty(identity, "chroma", "color identity");
    const lightness = inputProperty(identity, "lightness", "color identity");
    const rawDelta = inputProperty(options, "maxChange", "color options");
    const delta = bounded(rawDelta === undefined ? 2 : rawDelta, 2, "color maxChange");
    const appearance = inputProperty(options, "appearance", "color options");
    const tone = inputProperty(options, "tone", "color options");
    const payload = new ByteSink();
    payload.u32(checkedId);
    payload.u8(4);
    payload.rgb24(checkedAnchor);
    payload.u8(domain === "neutral" ? 1 : 0);
    payload.u8((hue === undefined ? 0 : 1) | (chroma === undefined ? 0 : 2) | (lightness === undefined ? 0 : 4));
    if (hue !== undefined) payload.f64Bits(bounded(hue, 180, "identity maxHueDeviation"));
    for (const [name, range] of [["chroma", chroma], ["lightness", lightness]]) {
      if (range === undefined) continue;
      const values = checkedPair(range, "identity " + name);
      for (const value of values) payload.f64Bits(value);
    }
    payload.f64Bits(delta);
    payload.u8(appearance === undefined ? 0 : 1);
    if (appearance !== undefined) payload.u32(u32Value(appearance, "color appearance"));
    payload.u8(tone === undefined ? 0 : 1);
    if (tone !== undefined) {
      const from = u32Value(inputProperty(tone, "from", "tone"), "tone input");
      const curve = candidateList(inputProperty(tone, "curve", "tone"), "tone curve")
        .map((point) => {
          if (inputArrayLength(point, "tone point") !== 2) invalid("tone point must contain two numbers");
          return indexedValues(point, 2, "tone point").map((value) => bounded(value, 1, "tone point"));
        });
      if (curve.length < 2 || curve[0][0] !== 0 || curve.at(-1)[0] !== 1
        || curve.some((point, index) => index > 0 && point[0] <= curve[index - 1][0])) {
        invalid("tone curve must span 0..1 with increasing input coordinates");
      }
      payload.u32(from);
      payload.u32(curve.length);
      for (const point of curve) for (const value of point) payload.f64Bits(value);
    }
    const output = this.entry("targets");
    for (const byte of payload.bytes) output.u8(byte);
    return this;
  }

  pointOccurrence(id, paint, surface) {
    const payload = new ByteSink();
    payload.u32(u32Value(id, "occurrence id"));
    payload.u32(u32Value(paint, "occurrence paint"));
    payload.u32(u32Value(surface, "occurrence surface"));
    payload.f64Bits(0);
    payload.f64Bits(0);
    payload.u8(0);
    const output = this.entry("occurrences");
    for (const byte of payload.bytes) output.u8(byte);
    return this;
  }

  /** Inclusive client-owned Oklab distance from the final visible colour to its actual backdrop. */
  visibleSeparationHard(id, occurrence, minimum, maximum) {
    const checkedId = u32Value(id, "constraint id");
    const checkedOccurrence = u32Value(occurrence, "constraint occurrence");
    const lower = f64Value(minimum, "visible separation minimum");
    const upper = f64Value(maximum, "visible separation maximum");
    if (lower < 0 || upper > 2 || lower > upper) {
      invalid("visible separation bounds must satisfy 0 <= minimum <= maximum <= 2");
    }
    const sink = this.constraintEntry(true);
    sink.u32(checkedId);
    sink.u8(11);
    sink.u32(checkedOccurrence);
    sink.f64Bits(lower === 0 ? 0 : lower);
    sink.f64Bits(upper === 0 ? 0 : upper);
    return this;
  }

  computedTarget(id, domain, anchor, maxDelta) {
    const checkedId = u32Value(id, "target id");
    const kind = inputProperty(domain, "kind", "computed domain");
    const payload = new ByteSink();
    payload.u32(checkedId);
    payload.u8(3);
    if (kind === "box") {
      const lower = rgb24(inputProperty(domain, "lower", "computed domain lower"), "computed domain lower");
      const upper = rgb24(inputProperty(domain, "upper", "computed domain upper"), "computed domain upper");
      for (let shift = 0; shift < 24; shift += 8) {
        if (((lower >>> shift) & 255) > ((upper >>> shift) & 255)) {
          invalid("computed box lower bounds must not exceed upper bounds");
        }
      }
      payload.u8(1);
      payload.rgb24(lower);
      payload.rgb24(upper);
    } else if (kind === "gray") {
      const lower = byteValue(inputProperty(domain, "lower", "computed domain lower"), "computed domain lower");
      const upper = byteValue(inputProperty(domain, "upper", "computed domain upper"), "computed domain upper");
      if (lower > upper) invalid("computed gray lower bound must not exceed upper bound");
      payload.u8(2);
      payload.u8(lower);
      payload.u8(upper);
    } else {
      invalid("computed domain kind must be box or gray");
    }
    const checkedAnchor = rgb24(anchor, "computed anchor");
    const delta = f64Value(maxDelta, "computed maxDelta");
    if (delta < 0 || !Number.isFinite(delta * delta)) {
      invalid("computed maxDelta must be nonnegative with a finite square");
    }
    payload.rgb24(checkedAnchor);
    payload.f64Bits(delta === 0 ? 0 : delta);
    // Ввод уже скопирован: дальнейшая запись не вызывает getters клиента.
    const output = this.entry("targets");
    for (const byte of payload.bytes) output.u8(byte);
    return this;
  }

  assignment(targetsInPreferenceOrder) {
    if (this.section("jointSelection").count !== 0) invalid("assignment is already declared");
    const length = inputArrayLength(targetsInPreferenceOrder, "assignment targets");
    if (length > MAX_SECTION_ENTRIES_V1) invalid("too many assignment targets");
    const targets = indexedValues(targetsInPreferenceOrder, length, "assignment targets")
      .map((target) => u32Value(target, "assignment target"));
    const payload = new ByteSink();
    payload.u8(3);
    payload.u8(0);
    payload.u32(targets.length);
    for (const target of targets) payload.u32(target);
    // Getter мог принять вложенное assignment; внешнее не заменяет его.
    if (this.section("jointSelection").count !== 0) invalid("assignment is already declared");
    const output = this.entry("jointSelection");
    for (const byte of payload.bytes) output.u8(byte);
    return this;
  }

  finish() {
    const bytes = super.finish();
    new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setUint16(4, 3, true);
    return bytes;
  }
}

function bounded(value, maximum, what) {
  const checked = f64Value(value, what);
  if (checked < 0 || checked > maximum) invalid(`${what} must be in [0, ${maximum}]`);
  return checked === 0 ? 0 : checked;
}

function checkedPair(range, what) {
  if (inputArrayLength(range, what) !== 2) invalid(what + " must contain two bounds");
  const pair = indexedValues(range, 2, what).map((v) => bounded(v, 1, what));
  if (pair[0] > pair[1]) invalid(what + " bounds must be increasing");
  return pair;
}
