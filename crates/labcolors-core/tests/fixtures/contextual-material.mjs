import { writeFileSync } from "node:fs";
import { ProgramWireBuilderV3 } from "../../../../packages/colors/program-wire/abi-v3.js";

// Same declaration as a translucent color use, its text, and an independent
// text use on the observed surface. Names do not enter the Core declaration.
function declaration(witness) {
  const builder = new ProgramWireBuilderV3()
    .colorTarget(10, [206, 115, 50], { appearance: 51 })
    .colorTarget(20, [36, 36, 36], { appearance: 52 })
    .colorTarget(30, [0, 103, 244], { appearance: 53 })
    .assignment([10, 20, 30])
    .surfaceInputPort(2).inputSurface(3, 2)
    .opacityInput(4, .72)
    .solidPaint(41, 10).opacityPaint(42, 41, 4)
    .solidPaint(43, 20).solidPaint(44, 30)
    .pointOccurrence(51, 42, 3).occurrenceSurface(5, 51)
    .pointOccurrence(52, 43, 5).pointOccurrence(53, 44, 3)
    .visibleSeparationHard(61, 51, .02, .6)
    .wcag22VisibleUnary(true, 62, 52, 1)
    .wcag22VisibleUnary(true, 63, 53, 1)
    .output(100, 42).output(101, 43).output(102, 44);
  if (witness) {
    // These extra predicates restrict the original color identities and hard
    // constraints. A Ready result is therefore a feasible subset witness.
    builder.exactVisibleUnary(true, 71, 51, [148, 83, 36])
      .exactVisibleUnary(true, 72, 52, [255, 255, 255])
      .exactVisibleUnary(true, 73, 53, [0, 108, 248]);
  }
  return builder.finish();
}
writeFileSync(new URL("contextual-material.lcpw", import.meta.url), declaration(false));
writeFileSync(new URL("contextual-material-witness.lcpw", import.meta.url), declaration(true));
