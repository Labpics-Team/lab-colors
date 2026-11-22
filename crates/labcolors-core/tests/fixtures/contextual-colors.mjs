import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

const wire = new ProgramWireBuilderV3()
  .colorTarget(10, [0, 103, 244], { appearance: 5 })
  .colorTarget(20, [128, 128, 128], { appearance: 15, tone: {from: 2, curve: [[0, 0.15], [1, 0.96]]} })
  .colorTarget(30, [255, 255, 255], { appearance: 25 })
  .colorTarget(40, [0, 103, 244], { appearance: 35 })
  .assignment([10, 20, 30, 40])
  .surfaceInputPort(2)
  .solidPaint(3, 10).solidPaint(13, 20).solidPaint(23, 30).solidPaint(33, 40)
  .inputSurface(4, 2).occurrenceSurface(14, 5).occurrenceSurface(24, 15)
  .pointOccurrence(5, 3, 24).pointOccurrence(15, 13, 4)
  .pointOccurrence(25, 23, 14).pointOccurrence(35, 33, 24)
  .wcag22VisibleUnary(true, 6, 25, 1)
  .wcag22VisibleUnary(true, 16, 35, 1)
  .output(7, 3).output(17, 13).output(27, 23).output(37, 33).finish();
writeFileSync(new URL("contextual-colors.lcpw", import.meta.url), wire);

const fixed = new ProgramWireBuilderV3().source(1, [18, 52, 86]).fixedTarget(2, 1)
  .assignment([]).surfaceInputPort(3).solidPaint(4, 2).inputSurface(5, 3)
  .pointOccurrence(6, 4, 5).output(7, 4).finish();
writeFileSync(new URL("fixed-contextual.lcpw", import.meta.url), fixed);
