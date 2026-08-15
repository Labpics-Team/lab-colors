//! sRGB ↔ XYZ(D65) colour space transforms.
//!
//! These are the official IEC 61966-2-1:1999 matrices as used by
//! W3C CSS Color Module Level 4 and published in
//! <https://github.com/w3c/csswg-drafts/issues/5922>.
//!
//! They are physical constants — they never change — so inlining them
//! avoids a heavy colour-management dependency (`palette` pulls ~20
//! transitive crates) and guarantees exact reproducibility with other
//! CSS-based pipelines.

use crate::Srgb8;
use crate::srgb8::hex_bytes;

/// D65 white point (normalized to Y = 1.0).
///
/// Derived from the 4-digit chromaticity (0.3127, 0.3290) of IEC 61966-2-1 /
/// CSS Color 4 via `X = x/y`, `Z = (1 − x − y)/y`. This is NOT the tabulated
/// CIE 015 / ISO 11664-2 D65 spectrum white, whose Y = 1 normalisation differs
/// by ≈2.3e-4 in Z. The chromaticity-derived white is used deliberately: it is
/// the exact white the sRGB matrices below are defined against, so the
/// transforms stay self-consistent with CSS-based pipelines.
pub const D65_WHITE: [f64; 3] = [
    0.950_455_927_051_671_6,
    1.000_000_000_000_000_0,
    1.089_057_750_759_878_4,
];

// ------------------------------------------------------------------
//  sRGB linear → XYZ(D65)
// ------------------------------------------------------------------
#[rustfmt::skip]
const SRGB_TO_XYZ_D65: [[f64; 3]; 3] = [
    [ 0.412_390_799_265_959_34,  0.357_584_339_383_878,     0.180_480_788_401_834_3  ],
    [ 0.212_639_005_871_510_27,  0.715_168_678_767_756,     0.072_192_315_360_733_71 ],
    [ 0.019_330_818_715_591_82,  0.119_194_779_794_625_98,  0.950_532_152_249_660_7  ],
];

/// Точные числовые владельцы, используемые artifact-ом contextual-region.
///
/// Test-only доступ связывает artifact с production-константами преобразования,
/// не добавляя символов или работы в release-сборку.
#[cfg(test)]
pub(crate) fn contextual_region_formula_literals_v1() -> &'static [(&'static str, f64)] {
    &[
        ("d65_x", D65_WHITE[0]),
        ("one", D65_WHITE[1]),
        ("d65_z", D65_WHITE[2]),
        ("srgb_m00", SRGB_TO_XYZ_D65[0][0]),
        ("srgb_m01", SRGB_TO_XYZ_D65[0][1]),
        ("srgb_m02", SRGB_TO_XYZ_D65[0][2]),
        ("srgb_m10", SRGB_TO_XYZ_D65[1][0]),
        ("srgb_m11", SRGB_TO_XYZ_D65[1][1]),
        ("srgb_m12", SRGB_TO_XYZ_D65[1][2]),
        ("srgb_m20", SRGB_TO_XYZ_D65[2][0]),
        ("srgb_m21", SRGB_TO_XYZ_D65[2][1]),
        ("srgb_m22", SRGB_TO_XYZ_D65[2][2]),
    ]
}

// ------------------------------------------------------------------
//  XYZ(D65) → sRGB linear
// ------------------------------------------------------------------
#[rustfmt::skip]
const XYZ_D65_TO_SRGB: [[f64; 3]; 3] = [
    [ 3.240_969_941_904_522_6,  -1.537_383_177_570_094,    -0.498_610_760_293_003_4  ],
    [-0.969_243_636_280_879_6,   1.875_967_501_507_720_2,   0.041_555_057_407_175_59 ],
    [ 0.055_630_079_696_993_66, -0.203_976_958_888_976_52,  1.056_971_514_242_878_6  ],
];

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

// ------------------------------------------------------------------
//  sRGB transfer functions (IEC 61966-2-1 § 6.4)
// ------------------------------------------------------------------

/// sRGB gamma decode: non-linear [0,1] → linear light [0,1].
///
/// The canonical decode math. Production no longer calls it directly — the
/// finite 8-bit decode is served by [`DECODE_8BIT`](gamma_data::DECODE_8BIT) —
/// but it remains the single source of truth that the table generator and the
/// `decode_table_matches_live_math` anti-drift gate regenerate from, so it is
/// never allowed to silently diverge from the shipped table.
#[cfg_attr(not(test), allow(dead_code))]
pub fn srgb_gamma_inv(v: f64) -> f64 {
    let sign = if v < 0.0 { -1.0 } else { 1.0 };
    let abs = v * sign;
    if abs <= 0.040_45 {
        v / 12.92
    } else {
        sign * ((abs + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB gamma encode: linear light [0,1] → non-linear [0,1].
pub fn srgb_gamma(v: f64) -> f64 {
    let sign = if v < 0.0 { -1.0 } else { 1.0 };
    let abs = v * sign;
    if abs > 0.003_130_8 {
        sign * (1.055 * abs.powf(1.0 / 2.4) - 0.055)
    } else {
        12.92 * v
    }
}

const RELATIVE_LUMINANCE_RED_WEIGHT: f64 = 0.2126;
const RELATIVE_LUMINANCE_GREEN_WEIGHT: f64 = 0.7152;
const RELATIVE_LUMINANCE_BLUE_WEIGHT: f64 = 0.0722;
const CONTINUOUS_ENCODED_CHANNEL_SPLIT: f64 = 0.039_28;
const CONTINUOUS_ENCODED_CHANNEL_SPLIT_RIGHT: f64 =
    f64::from_bits(CONTINUOUS_ENCODED_CHANNEL_SPLIT.to_bits() + 1);
const RELATIVE_LUMINANCE_RANGE_MARGIN: f64 = 8.0 * f64::EPSILON;

/// Frozen continuous encoded-sRGB channel transfer used by pre-cutover physical
/// proposal and reporting paths.
///
/// The `0.03928` split is preserved byte-for-byte from those paths. It is not
/// the canonical IEC transfer (`srgb_gamma_inv`) and cannot issue a WCAG verdict;
/// final sRGB8 conformance belongs to the proof-bound WCAG 2.2 evaluator.
fn continuous_encoded_channel_to_linear(channel: f64) -> f64 {
    if channel <= CONTINUOUS_ENCODED_CHANNEL_SPLIT {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn continuous_encoded_channel_linear_range(encoded_lo: f64, encoded_hi: f64) -> (f64, f64) {
    debug_assert!(
        encoded_lo.is_finite()
            && encoded_hi.is_finite()
            && (0.0..=1.0).contains(&encoded_lo)
            && (0.0..=1.0).contains(&encoded_hi)
            && encoded_lo <= encoded_hi
    );

    let mut lo = continuous_encoded_channel_to_linear(encoded_lo)
        .min(continuous_encoded_channel_to_linear(encoded_hi));
    let mut hi = continuous_encoded_channel_to_linear(encoded_lo)
        .max(continuous_encoded_channel_to_linear(encoded_hi));
    if encoded_lo <= CONTINUOUS_ENCODED_CHANNEL_SPLIT
        && encoded_hi > CONTINUOUS_ENCODED_CHANNEL_SPLIT
    {
        let at_split = continuous_encoded_channel_to_linear(CONTINUOUS_ENCODED_CHANNEL_SPLIT);
        let right_of_split =
            continuous_encoded_channel_to_linear(CONTINUOUS_ENCODED_CHANNEL_SPLIT_RIGHT);
        lo = lo.min(at_split).min(right_of_split);
        hi = hi.max(at_split).max(right_of_split);
    }
    (lo, hi)
}

/// Continuous relative-luminance coordinate for an encoded-sRGB colour.
///
/// This is a non-normative physical primitive for proposal search and frozen
/// report projections. Canonical WCAG decisions use the independent finite
/// sRGB8 Q55 evaluator.
pub(crate) fn encoded_srgb_relative_luminance(encoded: [f64; 3]) -> f64 {
    RELATIVE_LUMINANCE_RED_WEIGHT * continuous_encoded_channel_to_linear(encoded[0])
        + RELATIVE_LUMINANCE_GREEN_WEIGHT * continuous_encoded_channel_to_linear(encoded[1])
        + RELATIVE_LUMINANCE_BLUE_WEIGHT * continuous_encoded_channel_to_linear(encoded[2])
}

/// Continuous ratio of two already-derived relative-luminance coordinates.
///
/// This is not a criterion, applicability decision, or proof.
pub(crate) fn relative_luminance_ratio(first: f64, second: f64) -> f64 {
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Continuous encoded-sRGB ratio without criterion semantics.
pub(crate) fn encoded_srgb_contrast_ratio(first: [f64; 3], second: [f64; 3]) -> f64 {
    relative_luminance_ratio(
        encoded_srgb_relative_luminance(first),
        encoded_srgb_relative_luminance(second),
    )
}

/// Characterized binary64 enclosure of relative luminance over ordered encoded
/// channel intervals. Both sides of the frozen transfer seam participate in the
/// extrema; the final fixed-operation pad covers multiply/add rounding.
pub(crate) fn encoded_srgb_relative_luminance_range(
    encoded_lo: [f64; 3],
    encoded_hi: [f64; 3],
) -> (f64, f64) {
    let channels = core::array::from_fn::<_, 3, _>(|channel| {
        continuous_encoded_channel_linear_range(encoded_lo[channel], encoded_hi[channel])
    });
    let lower = RELATIVE_LUMINANCE_RED_WEIGHT * channels[0].0
        + RELATIVE_LUMINANCE_GREEN_WEIGHT * channels[1].0
        + RELATIVE_LUMINANCE_BLUE_WEIGHT * channels[2].0;
    let upper = RELATIVE_LUMINANCE_RED_WEIGHT * channels[0].1
        + RELATIVE_LUMINANCE_GREEN_WEIGHT * channels[1].1
        + RELATIVE_LUMINANCE_BLUE_WEIGHT * channels[2].1;
    (
        (lower - RELATIVE_LUMINANCE_RANGE_MARGIN).max(0.0),
        (upper + RELATIVE_LUMINANCE_RANGE_MARGIN).min(1.0),
    )
}

// ------------------------------------------------------------------
//  Exact 8-bit gamma tables (issue: discrete exactness)
// ------------------------------------------------------------------
//
// The system terminates on an 8-bit hex grid, so both gamma transforms on the
// hot path have a FINITE domain on one side and are tabulated EXACTLY — this is
// enumeration of every answer, not approximation, so no quality is lost by
// construction. Both tables are generated from the live `srgb_gamma`/
// `srgb_gamma_inv` math and gated bit-for-bit by anti-drift tests.

mod gamma_data;

/// Exact 8-bit decode: linear light for each of the 256 input codes.
///
/// `srgb_from_hex` always parses an 8-bit byte, so its decode domain is the
/// finite set `{0/255, …, 255/255}`. `DECODE_8BIT[b] = srgb_gamma_inv(b / 255)`
/// is therefore the *exact* decode for every reachable input — a table lookup
/// that replaces the per-channel `powf` with zero loss (gated by
/// `decode_table_matches_live_math` and `decode_reproduces_legacy_powf_path`).
pub(crate) fn decode_8bit(byte: u8) -> f64 {
    gamma_data::DECODE_8BIT[byte as usize]
}

// NOTE on the encode (quantisation) side — deliberately NOT tabulated.
//
// `hex_from_srgb` takes a *continuous* linear value (matrix / Oklab output), so
// unlike the decode it has no finite domain: it is a genuine continuous→discrete
// map. A boundary table (binary search over `srgb_gamma_inv((b+0.5)/255)`) was
// prototyped and measured bit-for-bit against the live
// `(srgb_gamma(x).clamp(0,1)*255).round()` path on a dense sweep including the
// half-step seams. It diverged by exactly one 8-bit code at ~10 high-range walls
// (e.g. x≈0.9088 → table 244 vs legacy 245): the round-trip
// `srgb_gamma(srgb_gamma_inv(e)) ≠ e` shifts the round-half tie across the wall.
// Reproducing the legacy bits would require evaluating `srgb_gamma(x)` anyway —
// the very `powf` the table was meant to remove — so an exact encode table is
// impossible here and an approximate one is forbidden by the discrete-exactness
// principle ("no quality loss at all"). The encode therefore keeps the live
// gamma path; only the finite-domain decode is tabulated. (See
// `encode_powf_table_is_not_bit_identical` for the pinned evidence.)

// ------------------------------------------------------------------
//  Public helpers
// ------------------------------------------------------------------

/// Parse `#RRGGBB` → linear sRGB `[r, g, b]` in `[0, 1]`.
pub fn srgb_from_hex(hex: &str) -> Result<[f64; 3], String> {
    Ok(srgb_linear_from_srgb8(Srgb8::new(hex_bytes(hex)?)))
}

/// Decode one exact encoded triplet to continuous linear sRGB.
///
/// Keeping this conversion typed lets callers that also need representation
/// facts parse the transport once without defining a second decode path.
pub(crate) fn srgb_linear_from_srgb8(rgb: Srgb8) -> [f64; 3] {
    let [r, g, b] = rgb.bytes();
    // The input is always an 8-bit byte, so the decode is an exact table lookup
    // (finite domain) — no per-channel powf.
    [decode_8bit(r), decode_8bit(g), decode_8bit(b)]
}

/// Derive one CIE 1931 XYZ(D65, relative Y = 1) point from exact encoded sRGB8.
///
/// This is the single operation-order owner for the registered finite-input
/// colourimetric transform. It is a deterministic model of a declared encoded
/// point signal; it does not claim that a renderer emitted or an
/// observer measured the resulting tristimulus.
#[inline]
pub(crate) fn xyz_d65_from_srgb8_v1(rgb: Srgb8) -> [f64; 3] {
    srgb_to_xyz(srgb_linear_from_srgb8(rgb))
}

/// Parse `#RRGGBB` → `(linear, display)` in one pass over the bytes: `linear`
/// is the exact 8-bit decode (as [`srgb_from_hex`]), `display` is the
/// **gamma-encoded** value WCAG measures — `[r/255, g/255, b/255]` — obtained
/// **without** the encode `powf`, because the byte *is* the display code.
///
/// For a hex input this `display` equals `quantised_display(linear)`
/// **bit-for-bit**: `quantised_display` computes `round(srgb_gamma(linear)·255)/255`,
/// and `round(srgb_gamma(decode_8bit(b))·255) == b` for every byte (the 8-bit
/// encode/decode round-trip, pinned exhaustively by
/// `display_equals_quantised_display_on_every_byte`). So a caller that needs both
/// the linear colour (for the CAM16/LPC forward) and the WCAG display value (for
/// the contrast ratio) gets the display value for free — no `srgb_gamma` on the
/// hot path — while staying byte-identical to the `quantised_display` path.
///
/// С главы #64 (level-3) путь читаемости стал полностью display-доменным
/// ([`crate::semantic::measure_contrast`] — ноль CAM16-форвардов), потому
/// продакшн-потребитель этого хелпера ушёл; остаётся якорь-тест байт-тождества
/// (`display_equals_quantised_display_on_every_byte`), потому `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn srgb_linear_and_display_from_hex(hex: &str) -> Result<([f64; 3], [f64; 3]), String> {
    let [r, g, b] = hex_bytes(hex)?;
    let linear = [decode_8bit(r), decode_8bit(g), decode_8bit(b)];
    let display = [
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    ];
    Ok((linear, display))
}

/// Quantise linear sRGB to the 8-bit display grid and back to linear, exactly as
/// `srgb_from_hex(hex_from_srgb(rgb))` would — same gamma encode, same per-channel
/// round to `[0, 255]`, same gamma decode — but without allocating the hex string.
///
/// This is the numeric identity of the hex round-trip: a caller that only needs
/// the quantised linear colour (e.g. to measure its `M'`) gets the byte-for-byte
/// same result the hex path produces, with no `format!`/parse on the hot path.
pub(crate) fn quantise_srgb(rgb: [f64; 3]) -> [f64; 3] {
    let q = |c: f64| {
        let byte = (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() / 255.0;
        srgb_gamma_inv(byte)
    };
    [q(rgb[0]), q(rgb[1]), q(rgb[2])]
}

/// Linear sRGB `[r, g, b]` in `[0, 1]` → `#RRGGBB` (clamped & rounded).
///
/// The input is continuous, so the gamma encode stays on the live transfer
/// function (see the encode note above for why a table cannot be bit-exact here).
pub(crate) fn hex_from_srgb(rgb: [f64; 3]) -> String {
    srgb8_from_linear(rgb).to_hex()
}

/// Quantise continuous linear sRGB to the one exact emitted byte triplet.
///
/// This is the typed form of [`hex_from_srgb`]; both share the same live gamma,
/// clamp and round operation, so string transport cannot define a second
/// output boundary.
pub(crate) fn srgb8_from_linear(rgb: [f64; 3]) -> crate::Srgb8 {
    let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() as u8;
    crate::Srgb8::new([q(rgb[0]), q(rgb[1]), q(rgb[2])])
}

/// Разбор `#RRGGBB` → ГАММА-КОДИРОВАННЫЙ sRGB `[r, g, b]` в `[0, 1]`
/// (byte/255, без декода в линейный свет).
///
/// Кодированное пространство — то, в котором Figma и браузер композитят
/// straight-alpha (см. [`crate::alpha`]); для колориметрии используй
/// `srgb_from_hex` (линейный свет).
pub fn srgb_encoded_from_hex(hex: &str) -> Result<[f64; 3], String> {
    Ok(Srgb8::new(hex_bytes(hex)?).encoded())
}

/// Внутренняя сериализация конечного gamma-encoded sRGB в `#RRGGBB`.
///
/// Это generated-finite boundary: публичный raw-`f64` formatter намеренно не
/// существует, потому `NaN`/бесконечность не должны превращаться в цвет. На
/// публичной границе exact-представление сериализуется через [`Srgb8::to_hex`].
pub(crate) fn hex_from_srgb_encoded(rgb: [f64; 3]) -> String {
    let q = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Srgb8::new([q(rgb[0]), q(rgb[1]), q(rgb[2])]).to_hex()
}

/// Linear sRGB → CIE XYZ under D65.
pub fn srgb_to_xyz(rgb: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(SRGB_TO_XYZ_D65, rgb)
}

/// CIE XYZ under D65 → linear sRGB.
pub fn xyz_to_srgb(xyz: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(XYZ_D65_TO_SRGB, xyz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_from_hex_rejects_non_ascii_without_panicking() {
        // Six bytes, but two 3-byte codepoints: a bare byte-length check passes,
        // and slicing on byte index 2 would cut mid-codepoint and panic. The
        // parser must return its declared Err instead (it is fallible, and is fed
        // untrusted strings through the public lpc/solve/LcsColor entry points).
        for bad in [
            "\u{20AC}\u{20AC}",
            "#\u{20AC}\u{20AC}",
            "\u{00E9}\u{00E9}\u{00E9}",
        ] {
            assert!(
                srgb_from_hex(bad).is_err(),
                "non-ASCII hex {bad:?} must return Err, not panic"
            );
        }
        // A valid #RRGGBB still parses.
        assert!(srgb_from_hex("#1A2B3C").is_ok());
    }

    /// Live decode table: the exact linear value of every 8-bit code.
    fn generate_decode() -> [f64; 256] {
        let mut t = [0.0_f64; 256];
        for (b, slot) in t.iter_mut().enumerate() {
            *slot = srgb_gamma_inv(b as f64 / 255.0);
        }
        t
    }

    #[test]
    #[ignore]
    fn _emit_gamma_data() {
        // GENERATOR (run once with --ignored): writes src/spaces/srgb/gamma_data.rs
        // from the live gamma math. The committed file is the artifact; the
        // anti-drift test guards it thereafter.
        use std::fmt::Write as _;
        let decode = generate_decode();
        let mut out = String::new();
        out.push_str("//! Precompiled exact 8-bit sRGB decode table — DO NOT EDIT BY HAND.\n");
        out.push_str("//!\n");
        out.push_str("//! `DECODE_8BIT[b] = srgb_gamma_inv(b / 255)`: the exact linear light of\n");
        out.push_str("//! every 8-bit code. Generated from the crate's own `srgb_gamma_inv` by\n");
        out.push_str("//! `srgb::tests::_emit_gamma_data`; regenerate with\n");
        out.push_str("//! `cargo test -p labcolors-core _emit_gamma_data -- --ignored`. The\n");
        out.push_str(
            "//! `decode_table_matches_live_math` test fails if this drifts from the math.\n\n",
        );
        writeln!(out, "#[rustfmt::skip]").ok();
        out.push_str("pub(super) static DECODE_8BIT: [f64; 256] = [\n");
        for chunk in decode.chunks(4) {
            out.push_str("    ");
            let line = chunk
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&line);
            out.push_str(",\n");
        }
        out.push_str("];\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/spaces/srgb/gamma_data.rs");
        std::fs::write(path, out).expect("write gamma_data.rs");
        eprintln!("wrote {path}");
    }

    #[test]
    fn decode_table_matches_live_math() {
        // ANTI-DRIFT: the committed decode table must equal a fresh generation
        // from the live gamma math, bit-for-bit (the decode is a pure finite
        // enumeration — no cross-platform powf-on-grid noise like the J_HK LUT,
        // because the same srgb_gamma_inv produces both). A changed transfer
        // function moves values wholesale and breaks this until regenerated.
        let live = generate_decode();
        for (b, (&l, &c)) in live.iter().zip(gamma_data::DECODE_8BIT.iter()).enumerate() {
            assert_eq!(
                l.to_bits(),
                c.to_bits(),
                "DECODE_8BIT[{b}] drifted: live {l} vs committed {c} — regenerate gamma_data.rs"
            );
        }
    }

    #[test]
    fn display_equals_quantised_display_on_every_byte() {
        // The identity `srgb_linear_and_display_from_hex` stands on: for every
        // 8-bit code, its `byte/255` display value equals `quantised_display` of
        // the linear decode, bit-for-bit — so the recheck primitive can take the
        // WCAG display value straight from the byte, skipping the encode `powf`,
        // and stay byte-identical to the `quantised_display` path. Exhaustive
        // over all 256 codes (the round in `quantised_display` snaps any 1-ULP
        // encode wobble back onto the exact grid).
        for byte in 0u16..=255 {
            let b = byte as u8;
            let hex = format!("#{b:02X}{b:02X}{b:02X}");
            let (linear, display) = srgb_linear_and_display_from_hex(&hex).unwrap();
            let quantised = crate::solve::quantised_display(linear);
            for ch in 0..3 {
                assert_eq!(
                    display[ch].to_bits(),
                    quantised[ch].to_bits(),
                    "byte {b}: display {} != quantised_display {}",
                    display[ch],
                    quantised[ch]
                );
            }
        }
    }

    #[test]
    fn decode_reproduces_legacy_powf_path_for_every_byte() {
        // BIT-IDENTITY: the table decode equals the pre-table powf decode
        // (srgb_gamma_inv(byte/255)) for all 256 codes, so srgb_from_hex is
        // numerically unchanged.
        for byte in 0u16..=255 {
            let b = byte as u8;
            let legacy = srgb_gamma_inv(b as f64 / 255.0);
            assert_eq!(
                decode_8bit(b).to_bits(),
                legacy.to_bits(),
                "decode_8bit({b}) != legacy powf decode"
            );
        }
    }

    #[test]
    fn encode_powf_table_is_not_bit_identical_near_walls() {
        // PINNED EVIDENCE for the design decision NOT to tabulate the encode.
        // A boundary table would compare a continuous linear `x` to
        // `srgb_gamma_inv((b+0.5)/255)` walls, but `srgb_gamma(srgb_gamma_inv(e))
        // != e` to the last ULP, so for `x` within a few ULPs of a high-range
        // wall the round-half tie lands on the wrong side: the table emits a
        // different 8-bit code than the live `(srgb_gamma(x).clamp*255).round()`.
        // A uniform grid usually misses these measure-zero seams, so this test
        // probes each wall deterministically with ULP-scale offsets. Finding a
        // disagreement proves an exact encode table is impossible (the round-trip
        // is not bit-stable), so the encode stays on the live gamma path — an
        // approximate table is forbidden by the discrete-exactness principle.
        let legacy = |x: f64| -> u8 { (srgb_gamma(x).clamp(0.0, 1.0) * 255.0).round() as u8 };
        let table = |x: f64| -> u8 {
            (0..255usize)
                .filter(|&b| srgb_gamma_inv((b as f64 + 0.5) / 255.0) <= x)
                .count() as u8
        };
        let mut disagreements = 0u32;
        for b in 0..255usize {
            let wall = srgb_gamma_inv((b as f64 + 0.5) / 255.0);
            for k in -8i64..=8 {
                let off = (k as f64) * f64::EPSILON * wall.max(1.0);
                let x = wall + off;
                if table(x) != legacy(x) {
                    disagreements += 1;
                }
            }
        }
        assert!(
            disagreements > 0,
            "encode table now matches legacy bit-for-bit even near walls; \
             the encode could be tabulated"
        );
        eprintln!("encode-table vs legacy near-wall disagreements: {disagreements}");
    }

    #[test]
    fn hex_round_trip_is_identity_for_all_grey_codes() {
        for byte in 0u16..=255 {
            let b = byte as u8;
            let hex = format!("#{b:02X}{b:02X}{b:02X}");
            let rgb = srgb_from_hex(&hex).expect("valid grey hex");
            let back = hex_from_srgb(rgb);
            assert!(
                back.eq_ignore_ascii_case(&hex),
                "grey round-trip drift: {hex} -> {back}"
            );
        }
    }
}
