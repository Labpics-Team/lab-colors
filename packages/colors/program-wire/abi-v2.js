import { ProgramWireBuilderV1 } from "./abi-v1.js";
import { ByteSink, candidateList, inputProperty, invalid, MAX_SECTION_ENTRIES_V1,
  PROGRAM_WIRE_TOO_MANY_ENTRIES, ProgramWireError, u32Value } from "./encoding.js";

export { ProgramWireError, PROGRAM_WIRE_INVALID_DECLARATION, PROGRAM_WIRE_TOO_MANY_ENTRIES,
  SURROUND_AVERAGE_V1, SURROUND_DIM_V1, SURROUND_DARK_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1, WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1, WCAG22_SC1411_GRAPHICAL_OBJECT_V1 } from "./abi-v1.js";

const MAX_U64 = (1n << 64n) - 1n;

/** Explicit authored preferences over complete finite states; the Core selects. */
export class ProgramWireBuilderV2 extends ProgramWireBuilderV1 {
  selectionRelease(revision, rankGroups) {
    if (typeof revision !== "bigint" || revision < 0n || revision > MAX_U64) {
      invalid("selection revision must be a u64 bigint");
    }
    if (this.section("jointSelection").count !== 0) invalid("selection release is already declared");
    const groups = candidateList(rankGroups, "selection rank groups");
    const sink = new ByteSink();
    sink.u64(revision);
    sink.u32(groups.length);
    let states = 0, choices = 0;
    for (const group of groups) {
      const members = candidateList(group, "selection rank group");
      states += members.length;
      if (states > MAX_SECTION_ENTRIES_V1) {
        throw new ProgramWireError(PROGRAM_WIRE_TOO_MANY_ENTRIES, "selection state budget exceeded");
      }
      sink.u32(members.length);
      for (const member of members) {
        const id = u32Value(inputProperty(member, "id", "selection state"), "selection state id");
        const entries = candidateList(inputProperty(member, "choices", "selection state"), "selection choices");
        choices += entries.length;
        if (choices > MAX_SECTION_ENTRIES_V1) {
          throw new ProgramWireError(PROGRAM_WIRE_TOO_MANY_ENTRIES, "selection choice budget exceeded");
        }
        sink.u32(id);
        sink.u32(entries.length);
        for (const entry of entries) {
          const target = u32Value(inputProperty(entry, "target", "selection choice"), "selection target");
          const candidate = u32Value(inputProperty(entry, "candidate", "selection choice"), "selection candidate");
          sink.u32(target);
          sink.u32(candidate);
        }
      }
    }
    // Callers may execute getters while the declaration is copied. Commit only
    // after all nested values have passed admission, without replacing a release.
    if (this.section("jointSelection").count !== 0) invalid("selection release is already declared");
    const output = this.entry("jointSelection");
    for (const byte of sink.bytes) output.u8(byte);
    return this;
  }

  finish() {
    const bytes = super.finish();
    new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setUint16(4, 2, true);
    return bytes;
  }
}
