import { colors } from "@labpics/colors";

export async function declareInterface({ blue, warm, opacity }) {
  const c = colors();
  const canvas = c.surface("canvas");
  const accent = c.role("accent", blue);
  const warmAccent = c.role("warm-accent", warm);
  const paper = c.role("paper", "#efe6d8");
  const ink = c.role("ink", "#242424");
  const text = { contrast: "wcag22-text" };
  // One client-owned law reused by different roles and different compositions.
  const layer = { tone: { from: canvas, curve: [[0, 0.22], [0.5, 0.58], [1, 0.97]] }, separation: [0.02, 0.35] };

  // Topology A: canvas → panel → action → label; the text follows the final fill.
  const panel = paper.on(canvas, layer);
  const action = accent.on(panel, { separation: [0.1, 2] });
  const actionLabel = ink.on(action, text);
  const panelText = ink.on(panel, text);
  const link = accent.on(panel, text);

  // A fixed local surface is independent of the outer canvas; it reuses the accent identity.
  const island = c.fixed("note-paper", "#f4eadb").on(canvas);
  const islandText = ink.on(island, text);
  const islandLink = accent.on(island, text);

  // Topology B: canvas → translucent material → label; alpha belongs only to its paint.
  const notice = warmAccent.on(canvas, { ...layer, opacity });
  const noticeText = ink.on(notice, text);
  return c.compile({ panel, action, actionLabel, text: panelText, link, island, islandText, islandLink, notice, noticeText });
}

