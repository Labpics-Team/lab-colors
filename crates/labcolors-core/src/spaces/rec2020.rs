//! ITU-R BT.2020 (Rec.2020) color space primaries and XYZ conversion matrices.
//!
//! Reference: ITU-R BT.2020-2 Table 1 (primaries), D65 white point.

// HDR/Rec.2020 infrastructure staged for o08 release; not yet wired into production paths.
// All items carry #[allow(dead_code)] until the output-profile release connects them.

/// Rec.2020 primary chromaticities (CIE 1931 xy).
#[allow(dead_code)]
pub const REC2020_RED_XY: [f64; 2] = [0.708, 0.292];
#[allow(dead_code)]
pub const REC2020_GREEN_XY: [f64; 2] = [0.170, 0.797];
#[allow(dead_code)]
pub const REC2020_BLUE_XY: [f64; 2] = [0.131, 0.046];
#[allow(dead_code)]
pub const REC2020_WHITE_XY: [f64; 2] = [0.3127, 0.3290]; // D65

/// XYZ(D65) → Linear Rec.2020 matrix (3×3, row-major).
/// Derived from Rec.2020 primaries + D65 white point normalization.
#[allow(dead_code)]
pub const XYZ_TO_REC2020: [[f64; 3]; 3] = [
    [1.7166511880, -0.3556707838, -0.2533662814],
    [-0.6666843518, 1.6164812366, 0.0157685458],
    [0.0176398574, -0.0427706133, 0.9421031212],
];

/// Linear Rec.2020 → XYZ(D65) matrix (inverse of above).
#[allow(dead_code)]
pub const REC2020_TO_XYZ: [[f64; 3]; 3] = [
    [0.6369580483, 0.1446169036, 0.1688809752],
    [0.2627002120, 0.6779980715, 0.0593017165],
    [0.0000000000, 0.0280726930, 1.0609850577],
];

/// Linear Rec.2020 tristimulus values (unbounded, may be out-of-gamut).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRec2020V1([f64; 3]);

#[allow(dead_code)]
impl LinearRec2020V1 {
    pub fn from_xyz_d65(xyz: [f64; 3]) -> Self {
        let m = &XYZ_TO_REC2020;
        let r = m[0][0] * xyz[0] + m[0][1] * xyz[1] + m[0][2] * xyz[2];
        let g = m[1][0] * xyz[0] + m[1][1] * xyz[1] + m[1][2] * xyz[2];
        let b = m[2][0] * xyz[0] + m[2][1] * xyz[1] + m[2][2] * xyz[2];
        Self([r, g, b])
    }

    pub fn to_xyz_d65(self) -> [f64; 3] {
        let [r, g, b] = self.0;
        let m = &REC2020_TO_XYZ;
        let x = m[0][0] * r + m[0][1] * g + m[0][2] * b;
        let y = m[1][0] * r + m[1][1] * g + m[1][2] * b;
        let z = m[2][0] * r + m[2][1] * g + m[2][2] * b;
        [x, y, z]
    }

    pub fn channels(self) -> [f64; 3] {
        self.0
    }

    /// True if all channels ≥ 0 (within floating-point tolerance).
    pub fn is_in_gamut(self) -> bool {
        const EPSILON: f64 = -1e-12;
        self.0.iter().all(|&c| c >= EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_inverse_identity() {
        // XYZ_TO_REC2020 × REC2020_TO_XYZ ≈ I
        for (i, row) in XYZ_TO_REC2020.iter().enumerate() {
            for (j, _) in REC2020_TO_XYZ.iter().enumerate() {
                let mut sum = 0.0;
                for k in 0..3 {
                    sum += row[k] * REC2020_TO_XYZ[k][j];
                }
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (sum - expected).abs() < 1e-12,
                    "M*M^-1[{i}][{j}] = {sum}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn rec2020_primaries_roundtrip() {
        // Red primary should map to [1, 0, 0] in linear Rec.2020
        // (after XYZ conversion from chromaticity)
        // This is a structural check; exact values depend on Y normalization.
        let red_xyz = chromaticity_to_xyz(REC2020_RED_XY);
        let rec2020 = LinearRec2020V1::from_xyz_d65(red_xyz);
        let back = rec2020.to_xyz_d65();
        for i in 0..3 {
            assert!((back[i] - red_xyz[i]).abs() < 1e-10);
        }
    }

    fn chromaticity_to_xyz(xy: [f64; 2]) -> [f64; 3] {
        let x = xy[0];
        let y = xy[1];
        // Y = 1.0 for primaries
        [x / y, 1.0, (1.0 - x - y) / y]
    }
}
