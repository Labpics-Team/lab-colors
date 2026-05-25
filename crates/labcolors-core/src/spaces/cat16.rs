/**
 * Chromatic Adaptation Transform 16 (CAT16).
 *
 * Matrices from CIE 170-2:2015 §6.3.1.
 */

/// CAT16: CIE XYZ → cone responses (LMS).
#[rustfmt::skip]
const XYZ_TO_CONE: [[f64; 3]; 3] = [
    [ 0.401288,  0.650173, -0.051461],
    [-0.250268,  1.204414,  0.045854],
    [-0.002079,  0.048952,  0.953127],
];

/// Inverse CAT16: cone responses → CIE XYZ.
#[rustfmt::skip]
const CONE_TO_XYZ: [[f64; 3]; 3] = [
    [ 1.86206786, -1.01125463,  0.14918677],
    [ 0.38752654,  0.62144744, -0.00897398],
    [-0.01584150, -0.03412294,  1.04996444],
];

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// CIE XYZ → LMS cone responses.
pub(crate) fn xyz_to_cone(xyz: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(XYZ_TO_CONE, xyz)
}

/// LMS cone responses → CIE XYZ.
pub(crate) fn cone_to_xyz(lms: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(CONE_TO_XYZ, lms)
}
