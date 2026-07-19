// Class-lock: the walker must parse EXACTLY the colour form the engine emits.
//
// Since @labpics/colors 0.4.0 the core emits every CSS variable as
// `oklch(L% C H)` / `oklch(L% C H / A)` (crates/labcolors-core/src/spaces/
// oklch.rs::oklch_css_from_hex). Per CSS Color 4, a browser then serialises the
// *computed* `background-color` of an oklch()-painted surface back in OKLCH form
// (Chrome ≥ M111 yields `oklch(<L 0..1> <C> <H>)`, optionally ` / <A>`). If
// `parseCssColor` cannot read that form, `effectiveBackground` silently drops
// the layer → a wrong effective background on the very surfaces the package
// paints. This suite closes that class: emission and walker must agree.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { initSync } from "../pkg/labcolors.js";
import { parseCssColor, effectiveBackground } from "../effective-bg.js";

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

// --- Live-emitted fixtures -------------------------------------------------
//
// `hex | alpha("-"=solid) | emitted-oklch-string`, produced by the ACTUAL core
// emitter (byte-exact round-trip proven in oklch.rs::round_trip_is_byte_exact_*).
// NOT hand-computed. Reproduce by recreating this throwaway example and running
// it against the live crate:
//
//   // crates/labcolors-core/examples/emit_oklch_fixtures.rs
//   fn main() {
//     for hex in ["#FF0000","#00FF00","#0000FF","#FFFFFF","#000000","#808080",
//                 "#787880","#3E87FF","#FF3B30","#FFD700","#2563EB","#1A1A1A"] {
//       println!("{hex}|-|{}", labcolors_core::oklch_css_from_hex(hex, None).unwrap());
//     }
//     for (hex, a) in [("#101012",0.122),("#3E87FF",0.8),("#FFFFFF",0.5),("#000000",0.361)] {
//       println!("{hex}|{a}|{}", labcolors_core::oklch_css_from_hex(hex, Some(a)).unwrap());
//     }
//   }
//
//   cargo run -p labcolors-core --example emit_oklch_fixtures
//
const FIXTURES = `
#FF0000|-|oklch(62.79554% 0.257683 29.234)
#00FF00|-|oklch(86.64396% 0.294827 142.495)
#0000FF|-|oklch(45.20137% 0.313214 264.052)
#FFFFFF|-|oklch(100.00000% 0.000000 89.876)
#000000|-|oklch(0.00000% 0.000000 0.000)
#808080|-|oklch(59.98708% 0.000000 89.876)
#787880|-|oklch(57.53363% 0.012136 286.012)
#3E87FF|-|oklch(64.04613% 0.193058 259.892)
#FF3B30|-|oklch(65.42146% 0.232135 28.659)
#FFD700|-|oklch(88.67711% 0.182186 95.330)
#2563EB|-|oklch(54.61497% 0.215208 262.881)
#1A1A1A|-|oklch(21.77865% 0.000000 89.876)
#101012|0.122|oklch(17.39406% 0.004094 285.967 / 0.122)
#3E87FF|0.8|oklch(64.04613% 0.193058 259.892 / 0.8)
#FFFFFF|0.5|oklch(100.00000% 0.000000 89.876 / 0.5)
#000000|0.361|oklch(0.00000% 0.000000 0.000 / 0.361)`
  .trim()
  .split("\n")
  .map((line) => {
    const [hex, aStr, css] = line.split("|");
    const bytes = [
      parseInt(hex.slice(1, 3), 16),
      parseInt(hex.slice(3, 5), 16),
      parseInt(hex.slice(5, 7), 16),
      aStr === "-" ? 1 : parseFloat(aStr),
    ];
    return { hex, css, bytes };
  });

test("self-consistency lock: parseCssColor decodes EXACTLY what the engine emits (byte-exact)", () => {
  // Zero tolerance: the string's digit precision carries byte precision by the
  // core's own round-trip proof, so an off-by-one is a real regression.
  for (const { hex, css, bytes } of FIXTURES) {
    assert.deepEqual(parseCssColor(css), bytes, `${css} must decode to ${hex}'s bytes`);
  }
});

test("the reported bug: an opaque oklch base is a real layer, not silently dropped", () => {
  // Pre-fix, parseCssColor(oklch(...)) → null, so effectiveBackground treated the
  // package's own painted surface as "no layer" and fell through to white.
  const opaque = "oklch(64.04613% 0.193058 259.892)"; // #3E87FF, opaque
  const tree = fakeTree([opaque]);
  assert.equal(effectiveBackground(tree.leaf, tree), "#3E87FF");
});

test("Chrome computed form: L as a 0..1 number parses, and equals the percentage form", () => {
  const numForm = "oklch(0.95123 0.011234 286.123)"; // what getComputedStyle yields
  const pctForm = "oklch(95.123% 0.011234 286.123)"; // the same colour, engine form
  const parsed = parseCssColor(numForm);
  assert.notEqual(parsed, null, "L-as-number form must parse");
  assert.deepEqual(parsed, parseCssColor(pctForm), "number and percentage L must agree");
  // Slash alpha on the Chrome number form.
  const withAlpha = parseCssColor("oklch(0.95123 0.011234 286.123 / 0.8)");
  assert.deepEqual(withAlpha, [...parsed.slice(0, 3), 0.8]);
});

test("effectiveBackground composites oklch layers (translucent over opaque)", () => {
  // A translucent white oklch panel over an opaque near-black oklch base — the
  // exact self-composed case the package produces. The known byte arithmetic
  // is `26 + .5 × (255 - 26) = 140.5`, round-half-up → 141 (`#8D8D8D`).
  const leaf = "oklch(100.00000% 0.000000 89.876 / 0.5)"; // #FFFFFF @ 0.5
  const base = "oklch(21.77865% 0.000000 89.876)"; // #1A1A1A opaque
  const tree = fakeTree([leaf, base]);
  assert.equal(effectiveBackground(tree.leaf, tree), "#8D8D8D");
});

test("component forms: none = 0, chroma as a percentage (100% = 0.4), deg suffix on hue", () => {
  // `none` per CSS: a missing component is 0.
  assert.deepEqual(parseCssColor("oklch(0% none none)"), [0, 0, 0, 1]);
  assert.deepEqual(parseCssColor("oklch(100% none none)"), [255, 255, 255, 1]);
  assert.deepEqual(parseCssColor("oklch(59.98708% 0 none)"), [128, 128, 128, 1]); // grey, hue irrelevant
  // Chroma percentage: 100% ≡ 0.4. #FF0000's C=0.257683 → 64.42075% (gamut edge).
  assert.deepEqual(parseCssColor("oklch(62.79554% 64.42075% 29.234)"), [255, 0, 0, 1]);
  // …and a MID-gamut sample (not a clamped extreme) locks the 0.4 factor from
  // BOTH sides: #787880's C=0.012136 → 3.034%. A wrong factor (e.g. 0.5) lands
  // off #787880 here, where no per-channel clamp can hide it.
  assert.deepEqual(parseCssColor("oklch(57.53363% 3.034% 286.012)"), [120, 120, 128, 1]);
  assert.deepEqual(
    parseCssColor("oklch(57.53363% 3.034% 286.012)"),
    parseCssColor("oklch(57.53363% 0.012136 286.012)"),
    "chroma % and number forms agree at mid-gamut",
  );
  // Hue may carry a `deg` suffix.
  assert.deepEqual(parseCssColor("oklch(64.04613% 0.193058 259.892deg)"), [62, 135, 255, 1]);
  // Alpha as a percentage.
  assert.deepEqual(parseCssColor("oklch(64.04613% 0.193058 259.892 / 80%)"), [62, 135, 255, 0.8]);
});

test("out-of-range components clamp per CSS Color 4 (L≥0, C≥0, alpha∈[0,1])", () => {
  // F3 — negative L clamps to 0. A per-channel byte clamp alone masks this at
  // low chroma, so use HIGH chroma where an unclamped negative L lands on
  // different bytes than L=0; asserting the two are equal locks the L clamp.
  assert.deepEqual(
    parseCssColor("oklch(-10% 0.3 30)"),
    parseCssColor("oklch(0% 0.3 30)"),
    "negative L must clamp to 0 (not pass through to a different colour)",
  );
  // …and above 100% clamps to 1 (symmetric upper bound). High chroma keeps the
  // two distinct pre-clamp, so this locks the upper clamp, not the byte clamp.
  assert.deepEqual(
    parseCssColor("oklch(150% 0.3 30)"),
    parseCssColor("oklch(100% 0.3 30)"),
    "L > 100% must clamp to 1",
  );
  // F4 — negative chroma clamps to 0 → achromatic, hue irrelevant.
  assert.deepEqual(
    parseCssColor("oklch(50% -0.1 120)"),
    parseCssColor("oklch(50% 0 120)"),
    "negative chroma must clamp to 0 (grey)",
  );
  // F5 — alpha clamps into [0,1] from both sides.
  assert.equal(parseCssColor("oklch(50% 0 0 / 1.5)")[3], 1, "alpha > 1 clamps to 1");
  assert.equal(parseCssColor("oklch(50% 0 0 / -0.5)")[3], 0, "alpha < 0 clamps to 0");
  assert.equal(parseCssColor("oklch(50% 0 0 / 150%)")[3], 1, "alpha % > 100 clamps to 1");
});

test("hue is periodic and chroma 0 is grey regardless of hue", () => {
  // F7 — 360° ≡ 0°, and hue wraps (periodicity of sin/cos), no special-casing.
  assert.deepEqual(parseCssColor("oklch(60% 0.1 360)"), parseCssColor("oklch(60% 0.1 0)"));
  assert.deepEqual(parseCssColor("oklch(60% 0.1 420)"), parseCssColor("oklch(60% 0.1 60)"));
  assert.deepEqual(parseCssColor("oklch(60% 0.1 -60)"), parseCssColor("oklch(60% 0.1 300)"));
  // Chroma 0 → achromatic for ANY hue value (a real number, not only `none`).
  assert.deepEqual(parseCssColor("oklch(60% 0 120)"), parseCssColor("oklch(60% 0 0)"));
  assert.deepEqual(parseCssColor("oklch(60% 0 120)"), [128, 128, 128, 1]);
});

test("garbage inside oklch(...) yields null, never throws", () => {
  const bad = [
    "oklch(foo bar baz)",
    "oklch(50%)", // too few components
    "oklch(50% 0.1)", // two components
    "oklch(50% 0.1 120 30)", // four components (no slash)
    "oklch(1abc 2 3)", // non-numeric junk (parseFloat would leniently accept — must not)
    "oklch(50% 0.1 120", // missing close paren
    "oklch()", // empty
  ];
  assert.doesNotThrow(() => bad.forEach(parseCssColor));
  for (const s of bad) assert.equal(parseCssColor(s), null, `${s} must be null`);
});

// A leaf→root element tree for effectiveBackground: each node carries a
// background-color string. index 0 = leaf. Returns { leaf, getStyle, parentOf }.
function fakeTree(chain) {
  const nodes = chain.map((bg) => ({ bg, parent: null }));
  for (let i = 0; i < nodes.length - 1; i++) nodes[i].parent = nodes[i + 1];
  return {
    leaf: nodes[0],
    getStyle: (el) => ({ getPropertyValue: () => el.bg }),
    parentOf: (el) => el.parent,
  };
}
