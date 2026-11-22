import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

const wire = new ProgramWireBuilderV3()
  .computedTarget(10, { kind: "gray", lower: 0, upper: 255 }, [128, 128, 128], 2)
  .assignment([10])
  .surfaceInputPort(2)
  .solidPaint(3, 10)
  .opacityInput(8, 0.5)
  .opacityPaint(13, 3, 8)
  .inputSurface(4, 2)
  .sourceOverOccurrence(5, 13, 4, 64, 0.2, 1)
  .visibleSeparationHard(6, 5, 0.18, 0.21)
  .output(7, 13).finish();
writeFileSync(new URL("visible-separation.lcpw", import.meta.url), wire);
