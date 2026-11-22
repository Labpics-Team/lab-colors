import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { ProgramWireBuilderV3, WCAG22_SC143_TEXT_DEFAULT_V1 } from "../program-wire/abi-v3.js";

const fixture = JSON.parse(readFileSync(new URL("../../../conformance/program-authoring-v3.json", import.meta.url), "utf8"));
const hex = (builder) => Buffer.from(builder.finish()).toString("hex");
function graph(builder, contrast = true) {
  builder.surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31).pointOccurrence(61, 41, 51);
  if (contrast) builder.wcag22VisibleUnary(true, 81, 61, WCAG22_SC143_TEXT_DEFAULT_V1);
  return builder.output(91, 41);
}
const contextual = {
  identity: { maxHueDeviation: 12, chroma: [.02, .35], lightness: [.2, .9] },
  maxChange: 1.5, appearance: 61,
  tone: { from: 31, curve: [[0, .25], [.5, .55], [1, .9]] },
};

test("V3 independent wire vectors cover native and JavaScript authoring", () => {
  assert.equal(fixture.schemaVersion, 1);
  assert.equal(hex(graph(new ProgramWireBuilderV3().computedTarget(21,
    { kind: "box", lower: [0, 0, 0], upper: [15, 15, 15] }, [8, 9, 10], 2)).assignment([21])), fixture.computedBox);
  assert.equal(hex(graph(new ProgramWireBuilderV3().computedTarget(21,
    { kind: "gray", lower: 0, upper: 255 }, [128, 128, 128], 2)).assignment([21])), fixture.computedGray);
  assert.equal(hex(graph(new ProgramWireBuilderV3().colorTarget(21, [0, 103, 244], contextual), false)
    .visibleSeparationHard(81, 61, .05, .8).assignment([21])), fixture.contextual);
  assert.equal(hex(graph(new ProgramWireBuilderV3().source(11, [0, 0, 0]).fixedTarget(21, 11))
    .assignment([])), fixture.fixedOnly);
  assert.equal(hex(new ProgramWireBuilderV3()
    .colorTarget(21, [5, 7, 9]).colorTarget(22, [80, 80, 80], { domain: "neutral" })
    .colorTarget(23, [90, 90, 90], { ...contextual, domain: "neutral" })
    .computedTarget(24, { kind: "box", lower: [1, 2, 3], upper: [4, 5, 6] }, [7, 8, 9], 0)
    .computedTarget(25, { kind: "gray", lower: 11, upper: 12 }, [13, 14, 15], 3)
    .pointOccurrence(26, 27, 28).visibleSeparationHard(29, 26, 0, 2).assignment([])), fixture.encodingOnly);
});
