//! Internal Display P3 color space operations.
//!
//! Provides the physical XYZ(D65) to linear Display P3 transform, the sRGB-identical
//! gamma transfer function (per Apple Display P3 specification), and gamut-boundary
//! geometry used by output projection and verification.

#[rustfmt::skip]
#[allow(dead_code)]
const XYZ_D65_TO_P3: [[f64; 3]; 3] = [
    [ 2.493_496_911_941_425,    -0.931_383_617_919_123_9,  -0.402_710_784_450_716_84 ],
    [-0.829_488_969_561_574_7,   1.762_664_060_318_346_3,   0.023_624_685_841_943_577],
    [ 0.035_845_830_243_784_47, -0.076_172_389_268_041_82,  0.956_884_524_007_687_2  ],
];

/// XYZ(D65, Y in `[0, 1]`) to linear Display P3.
#[allow(dead_code)]
pub(crate) fn xyz_to_p3_linear(xyz: [f64; 3]) -> [f64; 3] {
    [
        XYZ_D65_TO_P3[0][0] * xyz[0] + XYZ_D65_TO_P3[0][1] * xyz[1] + XYZ_D65_TO_P3[0][2] * xyz[2],
        XYZ_D65_TO_P3[1][0] * xyz[0] + XYZ_D65_TO_P3[1][1] * xyz[1] + XYZ_D65_TO_P3[1][2] * xyz[2],
        XYZ_D65_TO_P3[2][0] * xyz[0] + XYZ_D65_TO_P3[2][1] * xyz[1] + XYZ_D65_TO_P3[2][2] * xyz[2],
    ]
}

/// Apply the sRGB EOTF (identical transfer function for Display P3 per Apple spec)
/// to a single linear channel value. Typed distinctly from sRGB gamma to prevent
/// accidental cross-use in certificate trails.
pub(crate) fn p3_gamma_encode_channel(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Inverse of `p3_gamma_encode_channel`. Used by the encoded recheck verifier.
pub(crate) fn p3_gamma_decode_channel(encoded: f64) -> f64 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Encode a linear P3 tristimulus to gamma-encoded P3.
pub(crate) fn p3_gamma_encode(linear_p3: [f64; 3]) -> [f64; 3] {
    [
        p3_gamma_encode_channel(linear_p3[0]),
        p3_gamma_encode_channel(linear_p3[1]),
        p3_gamma_encode_channel(linear_p3[2]),
    ]
}

/// Decode a gamma-encoded P3 tristimulus back to linear P3.
pub(crate) fn p3_gamma_decode(encoded_p3: [f64; 3]) -> [f64; 3] {
    [
        p3_gamma_decode_channel(encoded_p3[0]),
        p3_gamma_decode_channel(encoded_p3[1]),
        p3_gamma_decode_channel(encoded_p3[2]),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn physical_p3_wall_contains_the_srgb_wall() {
        let mut strictly_wider_somewhere = false;
        for l10 in 2..=9 {
            let lightness = f64::from(l10) / 10.0;
            for hue in (0..360).step_by(15) {
                let hue = f64::from(hue);
                let srgb = crate::scale::max_chroma_bisect(lightness, hue);
                let p3 = crate::scale::max_chroma_p3_bisect(lightness, hue);
                assert!(
                    p3 >= srgb - 1e-9,
                    "P3 wall is narrower than sRGB at L={lightness}, h={hue}: {p3} < {srgb}"
                );
                strictly_wider_somewhere |= p3 > srgb * 1.05;
            }
        }
        assert!(
            strictly_wider_somewhere,
            "the P3 matrix must produce a genuinely wider gamut somewhere"
        );
    }

    /// Oracle vectors computed from the XYZ-to-P3 matrix and sRGB gamma transfer
    /// function in this module. Each pair is (xyz_d65, expected_encoded_p3).
    const ORACLE_VECTORS: [([f64; 3], [f64; 3]); 10] = [
        // P3 red primary
        (
            [0.4866, 0.2290, -0.0020],
            [
                1.000_375_385_576_657,
                -0.000_342_536_944_987,
                -0.024_737_474_303_527,
            ],
        ),
        // P3 green primary
        (
            [0.2657, 0.6917, 0.0451],
            [
                0.001_573_973_954_110,
                0.999_958_231_674_300,
                -0.000_112_565_864_141,
            ],
        ),
        // P3 blue primary
        (
            [0.1982, 0.0493, 1.0439],
            [
                0.182_472_899_365_812,
                -0.682_738_873_275_621,
                1.000_984_506_746_989,
            ],
        ),
        // D65 white point
        (
            [0.9505, 1.0000, 1.0890],
            [
                1.000_058_529_397_08,
                0.999_983_329_774_657,
                0.999_976_402_367_743,
            ],
        ),
        // Mid-gamut warm
        (
            [0.3500, 0.2800, 0.1200],
            [
                0.775_795_182_694_772,
                0.491_281_242_493_821,
                0.359_195_003_652_463,
            ],
        ),
        // Mid-gamut cool
        (
            [0.1800, 0.2200, 0.4500],
            [
                0.277_758_323_855_466,
                0.536_218_733_443_349,
                0.680_188_856_941_253,
            ],
        ),
        // Near-black
        (
            [0.0050, 0.0050, 0.0050],
            [
                0.068_382_687_709_650,
                0.058_893_447_392_171,
                0.056_872_480_609_552,
            ],
        ),
        // Near-white
        (
            [0.9000, 0.9500, 1.0200],
            [
                0.977_042_575_824_772,
                0.978_636_668_422_400,
                0.971_286_312_321_990,
            ],
        ),
        // Saturated mid-lightness
        (
            [0.4000, 0.1500, 0.3000],
            [
                0.873_967_711_076_929,
                -0.779_186_763_473_890,
                0.574_850_954_277_402,
            ],
        ),
        // Low-saturation
        (
            [0.3000, 0.3100, 0.3300],
            [
                0.606_702_146_996_086,
                0.588_576_131_801_128,
                0.586_408_108_600_142,
            ],
        ),
    ];

    #[test]
    fn oracle_vectors_xyz_to_encoded_p3() {
        for (xyz, expected_encoded) in ORACLE_VECTORS {
            let linear = super::xyz_to_p3_linear(xyz);
            let encoded = super::p3_gamma_encode(linear);
            for i in 0..3 {
                assert!(
                    (encoded[i] - expected_encoded[i]).abs() < 1e-6,
                    "channel {i}: got {}, expected {} (xyz={:?})",
                    encoded[i],
                    expected_encoded[i],
                    xyz,
                );
            }
        }
    }

    #[test]
    fn p3_gamma_round_trip() {
        for val_u16 in 0..=65535u16 {
            let linear = f64::from(val_u16) / 65535.0;
            let encoded = super::p3_gamma_encode_channel(linear);
            let decoded = super::p3_gamma_decode_channel(encoded);
            assert!(
                (decoded - linear).abs() < 1e-12,
                "round-trip failed at linear={linear}: decoded={decoded}",
            );
        }
    }
}
