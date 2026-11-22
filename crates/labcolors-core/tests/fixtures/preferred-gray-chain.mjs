import { writeFileSync } from "node:fs";
import {
  ProgramWireBuilderV3,
  WCAG22_SC143_TEXT_DEFAULT_V1,
} from "../../../../packages/colors/program-wire/abi-v3.js";

// Three opaque local uses, with two coupled contrast constraints and an
// explicit surface-first preference. Output aliases do not choose the order.
const wire = new ProgramWireBuilderV3()
  .surfaceInputPort(1).inputSurface(2, 1)
  .colorTarget(10, [239, 239, 239], {
    domain: "neutral", appearance: 11,
    tone: { from: 1, curve: [[0, 0.25], [1, 0.25]] },
  })
  .solidPaint(12, 10).pointOccurrence(11, 12, 2).occurrenceSurface(13, 11)
  .colorTarget(20, [36, 36, 36], { domain: "neutral", appearance: 21 })
  .solidPaint(22, 20).pointOccurrence(21, 22, 13).occurrenceSurface(23, 21)
  .colorTarget(30, [36, 36, 36], { domain: "neutral", appearance: 31 })
  .solidPaint(32, 30).pointOccurrence(31, 32, 23)
  .wcag22VisibleUnary(true, 41, 21, WCAG22_SC143_TEXT_DEFAULT_V1)
  .wcag22VisibleUnary(true, 42, 31, WCAG22_SC143_TEXT_DEFAULT_V1)
  .assignment([10, 20, 30])
  .output(100, 12).output(101, 22).output(102, 32)
  .finish();

writeFileSync(new URL("preferred-gray-chain.lcpw", import.meta.url), wire);
