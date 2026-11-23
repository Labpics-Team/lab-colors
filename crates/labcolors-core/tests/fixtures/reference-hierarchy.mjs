import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

for (const fixed of [false, true]) {
  const wire = new ProgramWireBuilderV3();
  if (fixed) wire.source(1, [68, 68, 68]).source(2, [102, 102, 102])
    .fixedTarget(10, 1).fixedTarget(20, 2);
  else wire.colorTarget(10, [68, 68, 68], { referenceAppearance: 5 })
    .colorTarget(20, [102, 102, 102], { referenceAppearance: 15 });
  wire.assignment(fixed ? [] : [10, 20], { reference: [[2, [255, 255, 255]]] })
    .surfaceInputPort(2).solidPaint(3, 10).solidPaint(13, 20).inputSurface(4, 2)
    .pointOccurrence(5, 3, 4).pointOccurrence(15, 13, 4)
    .preserveTonalHierarchy(30, [5, 15]).output(7, 3).output(17, 13);
  if (!fixed) wire.wcag22VisibleUnary(true, 6, 5, 1).wcag22VisibleUnary(true, 16, 15, 1);
  writeFileSync(new URL(`reference-hierarchy${fixed ? "-fixed" : ""}.lcpw`, import.meta.url), wire.finish());
}
