// Regenerate after building packages/colors/pkg:
// node crates/labcolors-core/tests/fixtures/interface-contextual.mjs
// This is the complete public example's ten-output declaration. The internal
// source accessor only copies the already compiled wire for a Rust consumer test.
import { readFile, writeFile } from "node:fs/promises";
import init, { colors } from "../../../../packages/colors/index.js";
import { colorProgramSource } from "../../../../packages/colors/authoring-program.js";

await init({ module_or_path: await readFile(new URL("../../../../packages/colors/pkg/labcolors_bg.wasm", import.meta.url)) });
const c = colors(), canvas = c.surface("canvas");
const accent = c.role("accent", "#0067f4"), warm = c.role("warm-accent", "#b85d35");
const paper = c.role("paper", "#efe6d8"), ink = c.role("ink", "#242424");
const text = { contrast: "wcag22-text" };
const layer = { tone: { from: canvas, curve: [[0, 0.22], [0.5, 0.58], [1, 0.97]] }, separation: [0.02, 0.35] };
const panel = paper.on(canvas, layer), action = accent.on(panel, { separation: [0.1, 2] });
const actionLabel = ink.on(action, text), panelText = ink.on(panel, text), link = accent.on(panel, text);
const island = c.fixed("note-paper", "#f4eadb").on(canvas);
const islandText = ink.on(island, text), islandLink = accent.on(island, text);
const notice = warm.on(canvas, { ...layer, opacity: 0.72 }), noticeText = ink.on(notice, text);
const program = await c.compile({ panel, action, actionLabel, text: panelText, link, island, islandText, islandLink, notice, noticeText });
try {
  await writeFile(new URL("interface-contextual.lcpw", import.meta.url), colorProgramSource(program).graph.wire);
} finally {
  program.dispose();
}
