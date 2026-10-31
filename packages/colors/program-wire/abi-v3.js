import { ProgramWireBuilderV1 } from "./abi-v1.js";
import { ByteSink, byteValue, candidateList, f64Value, inputProperty, invalid,
  rgbBytes, u32Value } from "./encoding.js";

export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

/** V3 передаёт области, опоры и предпочтения; назначение цветов принадлежит Core. */
export class ProgramWireBuilderV3 extends ProgramWireBuilderV1 {
  computedTarget(id, domain, anchor, maxDelta) {
    const checkedId = u32Value(id, "target id");
    const kind = inputProperty(domain, "kind", "computed domain");
    const payload = new ByteSink();
    payload.u32(checkedId);
    payload.u8(3);
    if (kind === "box") {
      const lower = rgbBytes(inputProperty(domain, "lower", "computed domain lower"), "computed domain lower");
      const upper = rgbBytes(inputProperty(domain, "upper", "computed domain upper"), "computed domain upper");
      if (lower.some((channel, index) => channel > upper[index])) {
        invalid("computed box lower bounds must not exceed upper bounds");
      }
      payload.u8(1);
      payload.rgb(lower);
      payload.rgb(upper);
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
    const checkedAnchor = rgbBytes(anchor, "computed anchor");
    const delta = f64Value(maxDelta, "computed maxDelta");
    if (delta < 0 || !Number.isFinite(delta * delta)) {
      invalid("computed maxDelta must be nonnegative with a finite square");
    }
    payload.rgb(checkedAnchor);
    payload.f64Bits(delta === 0 ? 0 : delta);
    // Ввод уже скопирован: дальнейшая запись не вызывает getters клиента.
    const output = this.entry("targets");
    for (const byte of payload.bytes) output.u8(byte);
    return this;
  }

  assignment(targetsInPreferenceOrder) {
    if (this.section("jointSelection").count !== 0) invalid("assignment is already declared");
    const targets = candidateList(targetsInPreferenceOrder, "assignment targets")
      .map((target) => u32Value(target, "assignment target"));
    const payload = new ByteSink();
    payload.u16(1);
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
