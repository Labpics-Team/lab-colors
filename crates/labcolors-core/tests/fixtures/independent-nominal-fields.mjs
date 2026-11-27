import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

// Two independently evaluated targets, each constrained against its own input.
const wire = new ProgramWireBuilderV3()
  .computedTarget(10, { kind: "gray", lower: 0, upper: 255 }, [96, 96, 96], 1)
  .computedTarget(20, { kind: "gray", lower: 0, upper: 255 }, [160, 160, 160], 1)
  .assignment([10, 20])
  .surfaceInputPort(2).surfaceInputPort(12)
  .solidPaint(3, 10).solidPaint(13, 20)
  .inputSurface(4, 2).inputSurface(14, 12)
  .pointOccurrence(5, 3, 4).pointOccurrence(15, 13, 14)
  .nominalSrgb8Visible(false, 6, 5)
  .nominalSrgb8Visible(false, 16, 15)
  .output(7, 3).output(17, 13)
  .finish();

writeFileSync(new URL("independent-nominal-fields.lcpw", import.meta.url), wire);
