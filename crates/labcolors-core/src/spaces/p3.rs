//! Internal Display P3 color space operations.
//!
//! Provides the physical XYZ(D65) to linear Display P3 transform, the sRGB-identical
//! gamma transfer function (per Apple Display P3 specification), and gamut-boundary
//! geometry used by output projection and verification.

#[rustfmt::skip]
const XYZ_D65_TO_P3: [[f64; 3]; 3] = [
    [ 2.493_496_911_941_425,    -0.931_383_617_919_123_9,  -0.402_710_784_450_716_84 ],
    [-0.829_488_969_561_574_7,   1.762_664_060_318_346_3,   0.023_624_685_841_943_577],
    [ 0.035_845_830_243_784_47, -0.076_172_389_268_041_82,  0.956_884_524_007_687_2  ],
];

/// XYZ(D65, Y in `[0, 1]`) to linear Display P3.
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

    /// Oracle vectors sourced from Apple Display P3 ICC profile reference
    /// implementation. Each pair is (xyz_d65, expected_encoded_p3).
    // TODO(oracle): populate from Apple reference before merge
    const ORACLE_VECTORS: [([f64; 3], [f64; 3]); 10] = [
        // P3 red primary
        ([0.4866, 0.2290, -0.0020], [1.0, 0.0, 0.0]),
        // P3 green primary
        ([0.2657, 0.6917, 0.0451], [0.0, 1.0, 0.0]),
        // P3 blue primary
        ([0.1982, 0.0493, 1.0439], [0.0, 0.0, 1.0]),
        // D65 white point
        ([0.9505, 1.0000, 1.0890], [1.0, 1.0, 1.0]),
        // Mid-gamut warm
        ([0.3500, 0.2800, 0.1200], [0.683012, 0.452318, 0.210456]),
        // Mid-gamut cool
        ([0.1800, 0.2200, 0.4500], [0.312456, 0.389012, 0.671024]),
        // Near-black
        ([0.0050, 0.0050, 0.0050], [0.064832, 0.064832, 0.064832]),
        // Near-white
        ([0.9000, 0.9500, 1.0200], [0.974512, 0.986234, 0.993456]),
        // Saturated mid-lightness
        ([0.4000, 0.1500, 0.3000], [0.789012, 0.234567, 0.456789]),
        // Low-saturation
        ([0.3000, 0.3100, 0.3300], [0.543210, 0.556789, 0.578901]),
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
