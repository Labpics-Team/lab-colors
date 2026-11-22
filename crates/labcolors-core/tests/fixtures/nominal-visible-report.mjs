import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

// Public wire consumed unchanged by native bindings: one transparent material,
// report-only quality, and a complete V3 snapshot with no computed variables.
const wire = new ProgramWireBuilderV3()
  .source(1, [255, 255, 255]).fixedTarget(2, 1)
  .surfaceInputPort(6).opacityInput(5, 0.5)
  .solidPaint(3, 2).opacityPaint(4, 3, 5).inputSurface(7, 6)
  .pointOccurrence(8, 4, 7).nominalSrgb8Visible(false, 10, 8)
  .output(17, 4).assignment([]).finish();
writeFileSync(new URL("nominal-visible-report.lcpw", import.meta.url), wire);
