import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// On the sRGB gray axis Oklab distance is the difference of cube-root linear luminances.
const grayL = (gray) => Math.cbrt(gray <= 10 ? gray / (255 * 12.92) : ((gray / 255 + 0.055) / 1.055) ** 2.4);

test("two applications of one role move together within their respective identity bounds", async () => {
  const c = colors(), role = c.role("шкала", "#808080", { maxChange: 0.1 });
  const low = role.on(c.surface("sheet")), high = role.on(c.surface("card"));
  c.separate(low, high, [0.16, 0.2]);
  const program = await c.compile({ low, high });
  try {
    const frame = program.resolve({ sheet: "#ffeedd", card: "#778899" });
    assert.equal(frame.state, "ready");
    const anchor = grayL(128);
    const lightness = [frame.values.low, frame.values.high].map((value) => grayL(value.rgb[0]));
    // Moving either one alone cannot produce .16 distance within its .1 budget.
    assert.ok(lightness.every((value) => value !== anchor && Math.abs(value - anchor) <= 0.1));
    const gap = Math.abs(lightness[0] - lightness[1]);
    assert.ok(gap >= 0.16 && gap <= 0.2);
    assert.deepEqual(frame.selection.map((component) => component.mode), ["joint-point-v1"]);
  } finally { program.dispose(); }
});

test("chromatically different paints are compared in all three Oklab coordinates", async () => {
  const c = colors(), page = c.surface("page");
  const red = c.fixed("red", "#ff0000").on(page), blue = c.fixed("blue", "#0000ff").on(page);
  // For sRGB red and blue ΔEOK ≈ .537, while the lightness difference alone is ≈ .176.
  c.separate(red, blue, [0.535, 0.540]);
  const program = await c.compile({ red, blue });
  try { assert.equal(program.resolve({ page: "#ffffff" }).state, "ready"); }
  finally { program.dispose(); }
});

test("a role adapts to a translucent mark on another surface and returns to its authored color", async () => {
  const c = colors(), paper = c.surface("paper"), rail = c.surface("rail");
  const mark = c.fixed("полупрозрачная отметка", "#000000").on(rail, { opacity: 0.5 });
  const ink = c.role("подпись", "#404040").on(paper);
  c.separate(ink, mark, [0.18, 0.55]);
  const program = await c.compile({ ink, mark });
  try {
    const initial = program.resolve({ paper: "#ffeedd", rail: "#000000" });
    assert.equal(initial.state, "ready");
    assert.deepEqual(initial.values.ink.rgb, [64, 64, 64]);
    const changed = program.resolve({ paper: "#ffeedd", rail: "#808080" });
    assert.equal(changed.state, "ready");
    assert.deepEqual(changed.values.mark, initial.values.mark);
    assert.equal(changed.values.mark.opacity, 0.5);
    const material = Math.round(128 + (changed.values.mark.rgb[0] - 128) * changed.values.mark.opacity);
    const distance = Math.abs(grayL(changed.values.ink.rgb[0]) - grayL(material));
    assert.ok(distance >= 0.18 && distance <= 0.55, `visible gray distance ${distance}`);
    assert.notDeepEqual(changed.values.ink, initial.values.ink);
    const otherPaper = program.resolve({ paper: "#113344", rail: "#808080" });
    assert.equal(otherPaper.state, "ready");
    assert.deepEqual(otherPaper.values, changed.values);
    assert.deepEqual(program.resolve({ paper: "#ffeedd", rail: "#000000" }).values, initial.values);
  } finally { program.dispose(); }
});

test("pair conflicts carry both authored uses, aliases and the declared range", async () => {
  const c = colors();
  const right = c.fixed("плёнка", "#0044ff").on(c.surface("rightPaper"), { opacity: 0 });
  const left = c.fixed("чернила", "#ff4400").on(c.surface("leftPaper"), { opacity: 0 });
  const bounds = [0, 0];
  // The same two authored sources have no implicit minimum difference.
  const independent = c.compile({ left, right });
  c.separate(left, right, bounds);
  bounds[1] = 2;
  const [plain, paired] = await Promise.all([independent, c.compile({ left, alsoLeft: left })]);
  try {
    const equal = { rightPaper: "#333333", leftPaper: "#333333" };
    const apart = { rightPaper: "#ffffff", leftPaper: "#000000" };
    assert.equal(plain.resolve(equal).state, "ready");
    assert.equal(plain.resolve(apart).state, "ready", "a later declaration cannot change an in-flight compilation");
    assert.deepEqual(paired.inputs, ["rightPaper", "leftPaper"]);
    assert.equal(paired.resolve(equal).state, "ready", "both final colors equal their own backdrop");
    const conflict = paired.resolve(apart);
    assert.equal(conflict.state, "conflict");
    assert.equal("values" in conflict, false);
    const item = conflict.diagnostics.find((item) => item.kind === "relation");
    assert.deepEqual(item, { kind: "relation", requirement: "visible-pair-separation",
      evidence: "candidate-witness", caseIndex: 0, left: { role: "чернила", use: left, outputs: ["left", "alsoLeft"] },
      right: { role: "плёнка", use: right, outputs: [] }, range: [0, 0] });
    assert.ok(Object.isFrozen(item.range));
    assert.ok(Object.isFrozen(item.left.outputs));
    assert.equal(paired.resolve(equal).state, "ready");
  } finally { plain.dispose(); paired.dispose(); }
});

test("a relation reaches hidden members through other relations and ignores disconnected inputs", async () => {
  const c = colors({ reference: { card: "#ffffff", paper: "#ffffff", rail: "#000000" } });
  const card = c.surface("card");
  const heading = c.fixed("заголовок", "#222222").on(card), legend = c.fixed("легенда", "#666666").on(card);
  const marker = c.fixed("метка", "#ff0000").on(c.surface("paper"), { opacity: 0 });
  const tail = c.fixed("линия", "#ffffff").on(c.surface("rail"), { opacity: 0 });
  const spare = c.surface("unused");
  const unused = c.fixed("фон образца", "#222222");
  c.separate(unused.on(spare), unused.on(spare), [0.4, 2]);
  c.separate(marker, tail, [0.2, 2]);
  c.separate(legend, marker, [0.2, 2]);
  c.preserveTonalHierarchy([heading, legend]);
  const program = await c.compile({ heading });
  try {
    assert.deepEqual(program.inputs, ["card", "paper", "rail"]);
    const frame = program.resolve({ card: "#ffffff", paper: "#ffffff", rail: "#000000" });
    assert.equal(frame.state, "ready");
    assert.deepEqual(Object.keys(frame.values), ["heading"]);
    const conflict = program.resolve({ card: "#ffffff", paper: "#ffffff", rail: "#ffffff" });
    assert.equal(conflict.state, "conflict");
    const item = conflict.diagnostics.find((item) => item.requirement === "visible-pair-separation");
    assert.equal(item.left.use, marker);
    assert.equal(item.right.use, tail);
    assert.deepEqual(item.left.outputs, []);
    assert.deepEqual(item.right.outputs, []);
  } finally { program.dispose(); }
});

test("an exact pair equality includes the material emission allowance", async () => {
  const c = colors(), paper = c.surface("paper");
  const material = c.fixed("material", "#000000").on(paper, { opacity: 0.5 });
  const swatch = c.fixed("swatch", "#404040").on(paper);
  c.separate(material, swatch, [0, 0]);
  const program = await c.compile({ material, swatch });
  try {
    // Nominal source-over yields #404040. Raster output of the translucent layer can differ.
    const frame = program.resolve({ paper: "#808080" });
    assert.equal(frame.state, "conflict");
    assert.equal(frame.diagnostics[0].requirement, "visible-pair-separation");
  } finally { program.dispose(); }
});

test("transparent descendants and siblings preserve the same uncertain raster color", async () => {
  const c = colors(), paper = c.surface("paper");
  const film = c.fixed("плёнка", "#000000").on(paper, { opacity: 0.5 });
  const clear = c.fixed("прозрачный слой", "#ff8800");
  const child = clear.on(film, { opacity: 0 });
  const grandchild = clear.on(child, { opacity: 0 });
  const sibling = c.fixed("второй слой", "#0077ff").on(film, { opacity: 0 });
  c.separate(film, child, [0, 0]);
  c.separate(sibling, grandchild, [0, 0]);
  const program = await c.compile({ film, child, grandchild, sibling });
  try {
    for (const background of ["#808080", "#fedcba", "#808080"]) {
      const frame = program.resolve({ paper: background });
      assert.equal(frame.state, "ready");
      assert.equal(frame.values.film.opacity, 0.5);
      assert.equal(frame.values.child.opacity, 0);
      assert.equal(frame.values.grandchild.opacity, 0);
      assert.equal(frame.values.sibling.opacity, 0);
    }
  } finally { program.dispose(); }
});

test("two independent translucent rasters can differ despite identical nominal colors and radii", async () => {
  const c = colors();
  const blackFilm = c.fixed("black film", "#000000").on(c.surface("light"), { opacity: 0.5 });
  const whiteFilm = c.fixed("white film", "#ffffff").on(c.surface("dark"), { opacity: 0.5 });
  c.separate(blackFilm, whiteFilm, [0, 0]);
  const program = await c.compile({ blackFilm, whiteFilm });
  try {
    // Both nominally round to #808080. Edge rendered the black film as #7f7f7f and the white film as #808080.
    const frame = program.resolve({ light: "#ffffff", dark: "#000000" });
    assert.equal(frame.state, "conflict");
    assert.ok(frame.diagnostics.some((item) => item.requirement === "visible-pair-separation"));
  } finally { program.dispose(); }
});
