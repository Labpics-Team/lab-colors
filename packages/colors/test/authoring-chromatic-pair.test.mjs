import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const context = { sheet: "#ffeedd", card: "#778899" };
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
        assert.notDeepEqual(rgb, [67, 100, 202]);
        assert.ok(rgb[2] > rgb[1] && rgb[1] > rgb[0]);
      }
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
    assert.notDeepEqual(frame.values.a.rgb, [67, 100, 202]);
    assert.notDeepEqual(frame.values.b.rgb, [67, 100, 202]);
  } finally { program.dispose(); }
});
