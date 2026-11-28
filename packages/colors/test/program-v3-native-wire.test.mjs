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
  assert.equal(hex(new ProgramWireBuilderV3().visiblePairSeparationHard(0x01020304, 0x10203040, 0xffffffff, -0, 0.75)),
    fixture.visiblePairSeparation);
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

test("reference declarations match independent native wire vectors", () => {
  for (const hierarchy of [false, true]) {
    const wire = new ProgramWireBuilderV3().colorTarget(10, [68, 68, 68], { referenceAppearance: 5 });
    if (hierarchy) wire.colorTarget(20, [102, 102, 102], { referenceAppearance: 15 });
    wire.assignment(hierarchy ? [10, 20] : [10], { reference: [[2, [255, 255, 255]]] })
      .surfaceInputPort(2).solidPaint(3, 10).inputSurface(4, 2).pointOccurrence(5, 3, 4);
    if (hierarchy) wire.solidPaint(13, 20).pointOccurrence(15, 13, 4).preserveTonalHierarchy(30, [5, 15]);
    wire.wcag22VisibleUnary(true, 6, 5, WCAG22_SC143_TEXT_DEFAULT_V1).output(7, 3);
    if (hierarchy) wire.wcag22VisibleUnary(true, 16, 15, WCAG22_SC143_TEXT_DEFAULT_V1).output(17, 13);
    assert.equal(hex(wire), hierarchy ? fixture.referenceHierarchy : fixture.referenceSingleton);
  }
  assert.equal(hex(new ProgramWireBuilderV3().assignment([], { reference: [] })), fixture.referenceEmpty);
});

test("backdrop tone uses tag 2 and carries only the curve after the occurrence", () => {
  const curve = [[0, 1], [1, 0]];
  const wire = graph(new ProgramWireBuilderV3().colorTarget(21, [68, 68, 68],
    { appearance: 61, tone: { curve } }), false).compose([21]);
  assert.equal(hex(wire), fixture.backdropTone);
  const empty = new ProgramWireBuilderV3();
  const before = empty.finish();
  assert.throws(() => empty.colorTarget(21, [0, 103, 244], { tone: { curve } }), /requires an appearance occurrence/);
  assert.deepEqual(empty.finish(), before);
});

test("mixed tone coordinates match both independent source encodings", () => {
  const points = [[0, [255, 255, 255]], [[255, 0, 0], [34, 34, 34]], [[255, 255, 255], 0]];
  for (const [from, expected] of [[undefined, fixture.sampledBackdropTone], [31, fixture.sampledInputTone]]) {
    const wire = graph(new ProgramWireBuilderV3().colorTarget(21, [68, 68, 68],
      { appearance: 61, tone: { from, curve: points } }), false).compose([21]);
    assert.equal(hex(wire), expected);
  }
});

test("composition uses distinct tags with the unchanged assignment field grammar", () => {
  const targets = [0x01020304, 0xffffffff, 0];
  const reference = [[0x10203040, [1, 128, 255]], [0xffffffff, [254, 129, 0]]];
  const peers = new ProgramWireBuilderV3().composeReference(targets, reference).finish();
  assert.equal(peers[26], 7);
  peers[26] = 6;
  assert.equal(Buffer.from(peers).toString("hex"), fixture.composeReference);
  assert.throws(() => new ProgramWireBuilderV3().composeReference(targets), /requires a reference/);
  for (const [members, options, name, tag] of [
    [targets, {}, "compose", 5],
    [targets, { reference }, "composeReference", 6],
    [[], {}, "composeEmpty", 5],
    [[], { reference: [] }, "composeReferenceEmpty", 6],
  ]) {
    const composition = new ProgramWireBuilderV3().compose(members, options).finish();
    assert.equal(Buffer.from(composition).toString("hex"), fixture[name]);
    // With no preceding sections, byte 26 is the selection tag. Everything
    // else, including declaration order and an explicitly empty reference, agrees.
    assert.equal(composition[26], tag);
    composition[26] -= 2;
    assert.deepEqual(composition, new ProgramWireBuilderV3().assignment(members, options).finish());
  }
});
