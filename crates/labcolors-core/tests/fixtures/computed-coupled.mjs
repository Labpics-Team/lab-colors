import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

// Two independently authored colours, physically coupled by the composition.
const wire = new ProgramWireBuilderV3()
  .computedTarget(10, { kind: "gray", lower: 120, upper: 180 }, [150, 150, 150], 1)
  .computedTarget(20, { kind: "gray", lower: 0, upper: 255 }, [160, 160, 160], 1)
  .assignment([10, 20])
  .surfaceInputPort(2)
  .solidPaint(3, 10).solidPaint(13, 20)
  .inputSurface(4, 2).occurrenceSurface(14, 5)
  .sourceOverOccurrence(5, 3, 4, 64, 0.2, 1)
  .sourceOverOccurrence(15, 13, 14, 64, 0.2, 1)
  .wcag22VisibleUnary(true, 6, 15, 2)
  .output(7, 3).output(17, 13).finish();
writeFileSync(new URL("computed-coupled.lcpw", import.meta.url), wire);
