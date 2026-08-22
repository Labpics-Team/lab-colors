//! Linear Rec. 2020 color space operations for HDR output projection.
//!
//! Provides the XYZ(D65) to linear Rec. 2020 transform used by the HDR
//! PQ encoding pipeline. No gamma/EOTF is applied here — that is the
//! responsibility of the PQ module.

/// Linear Rec. 2020 tristimulus (R, G, B in [0, 1] nominal range).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LinearRec2020V1([f64; 3]);

#[rustfmt::skip]
const XYZ_D65_TO_REC2020: [[f64; 3]; 3] = [
    [ 1.716_651_187_994_398_4, -0.355_670_778_794_398_4, -0.253_366_281_794_398_4 ],
    [-0.666_684_351_794_398_4,  1.616_481_231_794_398_4,  0.015_767_881_794_398_4 ],
    [ 0.017_639_857_794_398_4, -0.042_770_613_794_398_4,  0.942_100_422_794_398_4 ],
];

impl LinearRec2020V1 {
    /// Convert XYZ(D65) tristimulus to linear Rec. 2020.
    pub(crate) fn from_xyz_d65(xyz: [f64; 3]) -> Self {
        let r = XYZ_D65_TO_REC2020[0][0] * xyz[0]
            + XYZ_D65_TO_REC2020[0][1] * xyz[1]
            + XYZ_D65_TO_REC2020[0][2] * xyz[2];
        let g = XYZ_D65_TO_REC2020[1][0] * xyz[0]
            + XYZ_D65_TO_REC2020[1][1] * xyz[1]
            + XYZ_D65_TO_REC2020[1][2] * xyz[2];
        let b = XYZ_D65_TO_REC2020[2][0] * xyz[0]
            + XYZ_D65_TO_REC2020[2][1] * xyz[1]
            + XYZ_D65_TO_REC2020[2][2] * xyz[2];
        Self([r, g, b])
    }

    /// Return the three linear channels as an array.
    pub(crate) fn channels(self) -> [f64; 3] {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d65_white_maps_to_equal_channels() {
        // D65 white point (Y=1): X=0.95047, Y=1.0, Z=1.08883
        let xyz = [0.95047, 1.0, 1.08883];
        let rec = LinearRec2020V1::from_xyz_d65(xyz);
        let [r, g, b] = rec.channels();
        assert!(
            (r - 1.0).abs() < 1e-6 && (g - 1.0).abs() < 1e-6 && (b - 1.0).abs() < 1e-6,
            "D65 white should map to (1,1,1) in Rec.2020, got ({r}, {g}, {b})"
        );
    }
}