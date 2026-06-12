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

/// CIE D65 standard illuminant (normalized to Y = 1.0).
///
/// Source: ISO 11664-2:2007 / CIE 015:2018.
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
fn decode_8bit(byte: u8) -> f64 {
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
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(format!("expected #RRGGBB, got #{}", hex));
    }
    let parse =
        |s: &str| u8::from_str_radix(s, 16).map_err(|e| format!("invalid hex '{}': {}", s, e));
    let r = parse(&hex[0..2])?;
    let g = parse(&hex[2..4])?;
    let b = parse(&hex[4..6])?;
    // The input is always an 8-bit byte, so the decode is an exact table lookup
    // (finite domain) — no per-channel powf.
    Ok([decode_8bit(r), decode_8bit(g), decode_8bit(b)])
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
pub fn hex_from_srgb(rgb: [f64; 3]) -> String {
    let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", q(rgb[0]), q(rgb[1]), q(rgb[2]))
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
