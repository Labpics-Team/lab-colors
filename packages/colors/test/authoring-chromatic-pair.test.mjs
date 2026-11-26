import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const context = { sheet: "#ffeedd", card: "#778899" };
const source = [67, 100, 202];

function linear(channel) {
  const value = channel / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

function oklab([r, g, b]) {
  const red = linear(r), green = linear(g), blue = linear(b);
  const l = Math.cbrt(0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue);
  const m = Math.cbrt(0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue);
  const s = Math.cbrt(0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue);
  return [
    0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
  ];
}

function oklabDistance(left, right) {
  const a = oklab(left), b = oklab(right);
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

for (const [mode, options] of [
  ["joint-point-v1", {}], ["ordered-reference-v1", { reference: context }],
]) {
  test(`${mode} separates two uses of one blue source when both must move`, async () => {
    const c = colors(options), primary = c.role("primary", "#4364ca", { maxChange: 0.1 });
    const sheet = primary.on(c.surface("sheet")), card = primary.on(c.surface("card"));
    c.separate(sheet, card, [0.16, 0.2]);
    const program = await c.compile({ onSheet: sheet, onCard: card });
    try {
      const frame = program.resolve(context);
      assert.equal(frame.state, "ready");
      assert.deepEqual(frame.selection.map((component) => component.mode), [mode]);
      for (const { rgb } of Object.values(frame.values)) {
        assert.notDeepEqual(rgb, source);
        assert.ok(rgb[2] > rgb[1] && rgb[1] > rgb[0]);
        assert.ok(oklabDistance(rgb, source) <= 0.1, `${mode} result exceeds maxChange`);
      }
      const pairDistance = oklabDistance(frame.values.onSheet.rgb, frame.values.onCard.rgb);
      assert.ok(pairDistance >= 0.16 && pairDistance <= 0.2, `${mode} pair distance ${pairDistance}`);
      assert.notDeepEqual(frame.values.onSheet.rgb, frame.values.onCard.rgb);
    } finally { program.dispose(); }
  });
}

test("a wide pair range fits within unequal source movement limits", async () => {
  const c = colors(), sheet = c.surface("sheet"), card = c.surface("card");
  const a = c.role("restricted", "#4364ca", { maxChange: 0.04 }).on(sheet);
  const b = c.role("flexible", "#4364ca", { maxChange: 0.14 }).on(card);
  c.separate(a, b, [0.16, 2]);
  const program = await c.compile({ a, b });
  try {
    const frame = program.resolve(context);
    assert.equal(frame.state, "ready");
    assert.notDeepEqual(frame.values.a.rgb, source);
    assert.notDeepEqual(frame.values.b.rgb, source);
    const pairDistance = oklabDistance(frame.values.a.rgb, frame.values.b.rgb);
    assert.ok(pairDistance >= 0.16 && pairDistance <= 2, `wide pair distance ${pairDistance}`);
    assert.ok(oklabDistance(frame.values.a.rgb, source) <= 0.04, "restricted result exceeds maxChange");
    assert.ok(oklabDistance(frame.values.b.rgb, source) <= 0.14, "flexible result exceeds maxChange");
  } finally { program.dispose(); }
});
