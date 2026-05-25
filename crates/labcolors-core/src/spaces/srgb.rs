/**
 * sRGB ↔ XYZ(D65) colour space transforms.
 *
 * These are the official IEC 61966-2-1:1999 matrices as used by
 * W3C CSS Color Module Level 4 and published in
 * <https://github.com/w3c/csswg-drafts/issues/5922>.
 *
 * They are physical constants — they never change — so inlining them
 * avoids a heavy colour-management dependency (`palette` pulls ~20
 * transitive crates) and guarantees exact reproducibility with other
 * CSS-based pipelines.
 */

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
    [ 0.412_390_799_265_959_34,  0.357_584_339_383_878_0,   0.180_480_788_401_834_3  ],
    [ 0.212_639_005_871_510_27,  0.715_168_678_767_756_0,   0.072_192_315_360_733_71 ],
    [ 0.019_330_818_715_591_82,  0.119_194_779_794_625_98,  0.950_532_152_249_660_7  ],
];

// ------------------------------------------------------------------
//  XYZ(D65) → sRGB linear
// ------------------------------------------------------------------
#[rustfmt::skip]
const XYZ_D65_TO_SRGB: [[f64; 3]; 3] = [
    [ 3.240_969_941_904_522_6,  -1.537_383_177_570_094_0,  -0.498_610_760_293_003_4  ],
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
//  Public helpers
// ------------------------------------------------------------------

/// Parse `#RRGGBB` → linear sRGB `[r, g, b]` in `[0, 1]`.
pub fn srgb_from_hex(hex: &str) -> Result<[f64; 3], String> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(format!("expected #RRGGBB, got #{}", hex));
    }
    let parse = |s: &str| u8::from_str_radix(s, 16).map_err(|e| format!("invalid hex '{}': {}", s, e));
    let r = parse(&hex[0..2])? as f64 / 255.0;
    let g = parse(&hex[2..4])? as f64 / 255.0;
    let b = parse(&hex[4..6])? as f64 / 255.0;
    Ok([srgb_gamma_inv(r), srgb_gamma_inv(g), srgb_gamma_inv(b)])
}

/// Linear sRGB `[r, g, b]` in `[0, 1]` → `#RRGGBB` (clamped & rounded).
pub fn hex_from_srgb(rgb: [f64; 3]) -> String {
    let r = (srgb_gamma(rgb[0]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (srgb_gamma(rgb[1]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (srgb_gamma(rgb[2]).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Linear sRGB → CIE XYZ under D65.
pub fn srgb_to_xyz(rgb: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(SRGB_TO_XYZ_D65, rgb)
}

/// CIE XYZ under D65 → linear sRGB.
pub fn xyz_to_srgb(xyz: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(XYZ_D65_TO_SRGB, xyz)
}
