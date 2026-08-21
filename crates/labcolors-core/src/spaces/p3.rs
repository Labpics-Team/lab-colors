//! Internal Display P3 gamut geometry.
//!
//! This module is test-only until a complete output-profile release supplies
//! its own candidate domain, encoder, and final encoded verifier.  The sole
//! retained operation is the physical XYZ(D65) to linear Display P3 transform
//! used by the private gamut-boundary regression.  It is not an output
//! capability and exposes no public selector or CSS serializer.

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
}
