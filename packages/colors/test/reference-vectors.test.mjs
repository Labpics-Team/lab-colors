// JS↔Rust byte-parity: `effective-bg.js`'s oklch parser (oklch → Oklab → sRGB
// bytes) must reproduce the CORE emitter byte-for-byte on ≥1000 seeded strings.
//
// The fixture `test/data/oklch-core-vectors.txt` is EMITTED by the Rust core's
// public `oklch_css_from_hex` (see crates/labcolors-core/tests/reference_vectors.rs)
// and kept in lock-step with the live emitter by the Rust anti-drift test
// `oklch_core_vectors_fixture_is_fresh`. Each line is `#RRGGBB|alpha|css`
// (`alpha` = `-` for solids). The core's own byte-exact round-trip proof
// (`oklch::round_trip_is_byte_exact_on_lattice`) guarantees the seed `#RRGGBB`
// IS the correct decode of `css`, so if `parseCssColor` reproduces those bytes,
// the JS duplicate is byte-identical to the Rust core across the whole set.
//
// This closes the class the module's own doc claims ("byte-exact to the core's
// round-trip proof") on ≥1000 seeded vectors instead of 16 hand-picked ones,
// including the edges L=0/1, the full C=0 grey axis, the hue lattice, out-of-gamut
// reconstruction, `none` components, percentages, and translucent alpha.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { parseCssColor } from "../effective-bg.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "data", "oklch-core-vectors.txt");

/** Parse a fixture line `#RRGGBB|alpha|css` → { hex, alpha, css, bytes }. */
function parseFixtureLine(line) {
  const [hex, aStr, css] = line.split("|");
  const bytes = [
    parseInt(hex.slice(1, 3), 16),
    parseInt(hex.slice(3, 5), 16),
    parseInt(hex.slice(5, 7), 16),
    aStr === "-" ? 1 : parseFloat(aStr),
  ];
  return { hex, alpha: aStr, css, bytes };
}

const VECTORS = readFileSync(FIXTURE, "utf8")
  .split("\n")
  .map((l) => l.trim())
  .filter((l) => l.length > 0)
  .map(parseFixtureLine);

test("fixture carries ≥1000 core-emitted vectors", () => {
  assert.ok(
    VECTORS.length >= 1000,
    `expected ≥1000 parity vectors, got ${VECTORS.length}. Regenerate with ` +
      `\`cargo test -p labcolors-core --test reference_vectors ` +
      `emit_oklch_core_vectors_fixture -- --ignored\`.`,
  );
});

test("byte-parity with the Rust core on every seeded oklch emission (zero tolerance)", () => {
  // Zero tolerance: the emitted string's digit precision carries byte precision
  // by the core's own round-trip proof, so any off-by-one is a real JS↔Rust
  // divergence in the shared Oklab↔sRGB math.
  let checked = 0;
  for (const { hex, css, bytes } of VECTORS) {
    const got = parseCssColor(css);
    assert.deepEqual(got, bytes, `core-emitted ${css} must decode to ${hex}'s bytes (+alpha)`);
    checked++;
  }
  // Guard the loop actually ran (a truncated fixture would make the suite pass
  // vacuously).
  assert.ok(checked >= 1000, `only ${checked} vectors checked`);
});

test("edge coverage present in the fixture (L=0/1, C=0 greys, alpha, out-of-gamut hues)", () => {
  // The parity loop already asserts these decode correctly; this pins that the
  // set actually INCLUDES the edges, so the guarantee is not over a soft middle.
  const has = (pred) => VECTORS.some(pred);
  assert.ok(has((v) => v.hex === "#000000" && v.alpha === "-"), "L=0 edge (#000000)");
  assert.ok(has((v) => v.hex === "#FFFFFF" && v.alpha === "-"), "L=1 edge (#FFFFFF)");
  // C=0 grey axis: a run of pure greys must be present.
  const greys = VECTORS.filter((v) => v.alpha === "-" && /^#(..)\1\1$/.test(v.hex));
  assert.ok(greys.length >= 200, `expected the full grey axis, got ${greys.length}`);
  // Translucent emissions (alpha ≠ solid) with the `/ A` form.
  const translucent = VECTORS.filter((v) => v.alpha !== "-");
  assert.ok(translucent.length >= 20, `expected alpha vectors, got ${translucent.length}`);
  for (const v of translucent) assert.ok(v.css.includes(" / "), `alpha css must carry / : ${v.css}`);
});

test("Chrome computed form (L as 0..1) decodes to the same core bytes", () => {
  // A browser serialises a computed oklch `background-color` with L in 0..1
  // (Chrome ≥ M111), not as a percentage. effectiveBackground reads exactly that
  // form off the DOM. For every core emission we rebuild the L-as-number spelling
  // — using String(parseFloat(Lpct)/100) so the double is bit-identical to the
  // percent path — and assert it decodes to the same bytes. Solids only (the
  // transform is on L; alpha is orthogonal and covered above).
  let checked = 0;
  for (const { css, bytes } of VECTORS) {
    if (!css.startsWith("oklch(") || css.includes(" / ")) continue;
    const inner = css.slice(6, -1); // "L% C H"
    const [lPct, c, h] = inner.split(/\s+/);
    const lNum = String(parseFloat(lPct) / 100); // bit-identical to Lpct/100
    const chromeForm = `oklch(${lNum} ${c} ${h})`;
    assert.deepEqual(parseCssColor(chromeForm), bytes, `${chromeForm} must equal ${css}`);
    checked++;
  }
  // Solids only (translucent alpha is excluded above), so the bar is below the
  // full-set 1000: the grey axis (256) + hue lattice + random body clear 900.
  assert.ok(checked >= 900, `only ${checked} Chrome-form vectors checked`);
});

test("`none` components decode to the core's achromatic bytes (CSS Color 4)", () => {
  // Ядро само эмитирует missing hue как `none`: числовой угол на оси C=0 не
  // является свойством цвета. Подмена также и C на `none` обязана сохранить
  // байты, поскольку CSS Color 4 вычисляет обе missing-компоненты как ноль.
  const greys = VECTORS.filter((v) => v.alpha === "-" && /^#(..)\1\1$/.test(v.hex));
  assert.ok(greys.length >= 200, "need the grey axis for the none check");
  for (const { css, bytes } of greys) {
    const lPct = css.slice(6, -1).split(/\s+/)[0];
    const noneForm = `oklch(${lPct} none none)`;
    assert.deepEqual(parseCssColor(noneForm), bytes, `${noneForm} must equal ${css}`);
  }
});

test("percentage chroma (100% = 0.4) agrees with the numeric form at the gamut edge and mid-gamut", () => {
  // CSS Color 4: an oklch chroma percentage is a fraction of 0.4. Locked from
  // BOTH sides — a clamped gamut edge (#FF0000) and an un-clamped mid-gamut
  // sample (#787880) where a wrong 0.4 factor cannot hide behind a byte clamp.
  // (Values are core emissions; see oklch-parse.test.mjs for provenance.)
  assert.deepEqual(
    parseCssColor("oklch(62.79554% 64.42075% 29.234)"),
    parseCssColor("oklch(62.79554% 0.257683 29.234)"),
    "gamut-edge chroma % must equal the numeric form",
  );
  assert.deepEqual(parseCssColor("oklch(62.79554% 64.42075% 29.234)"), [255, 0, 0, 1]);
  assert.deepEqual(
    parseCssColor("oklch(57.53363% 3.034% 286.012)"),
    parseCssColor("oklch(57.53363% 0.012136 286.012)"),
    "mid-gamut chroma % must equal the numeric form",
  );
});

test("out-of-gamut oklch clamps per channel to a valid #RRGGBB (CSS Color 4)", () => {
  // The core only emits in-gamut oklch, so out-of-gamut decoding is a JS-parser
  // robustness contract, checked against CSS Color 4 semantics: L clamps to
  // [0,1], C clamps to ≥0, out-of-gamut channels clamp per channel — the result
  // is always a finite [r,g,b] in 0..255, never null/NaN.
  const oog = [
    "oklch(70% 0.5 30)", // chroma far past the gamut wall
    "oklch(50% 0.4 250)",
    "oklch(90% 0.37 140)",
    "oklch(30% 0.6 0)",
  ];
  for (const css of oog) {
    const c = parseCssColor(css);
    assert.notEqual(c, null, `${css} must not be null`);
    for (let i = 0; i < 3; i++) {
      assert.ok(Number.isInteger(c[i]) && c[i] >= 0 && c[i] <= 255, `${css} ch${i}=${c[i]} must be a 0..255 byte`);
    }
    assert.equal(c[3], 1, `${css} alpha default 1`);
  }
  // Negative L and C clamp (not pass-through to a different colour), high chroma
  // keeps the two distinct pre-clamp so this locks the clamp, not the byte round.
  assert.deepEqual(parseCssColor("oklch(-10% 0.3 30)"), parseCssColor("oklch(0% 0.3 30)"));
  assert.deepEqual(parseCssColor("oklch(150% 0.3 30)"), parseCssColor("oklch(100% 0.3 30)"));
  assert.deepEqual(parseCssColor("oklch(50% -0.1 120)"), parseCssColor("oklch(50% 0 120)"));
});

test("hue is periodic: 360° ≡ 0°, and wrapping is by sin/cos (CSS Color 4)", () => {
  assert.deepEqual(parseCssColor("oklch(60% 0.1 360)"), parseCssColor("oklch(60% 0.1 0)"));
  assert.deepEqual(parseCssColor("oklch(60% 0.1 420)"), parseCssColor("oklch(60% 0.1 60)"));
  assert.deepEqual(parseCssColor("oklch(60% 0.1 -60)"), parseCssColor("oklch(60% 0.1 300)"));
});
