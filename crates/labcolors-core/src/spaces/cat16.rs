//! Chromatic Adaptation Transform 16 (CAT16).
//!
//! Matrices from Li, Li, Wang, Xu, Luo, Cui, Melgosa, Brill & Pointer (2017),
//! "Comprehensive color solutions: CAM16, CAT16, and CAM16-UCS",
//! Color Res. Appl. 42(6), DOI 10.1002/col.22131 — the paper that introduced
//! CAT16, later formalised as CIE 248:2022. (CIE 170-2:2015 is the
//! physiological cone-fundamental standard and does not define CAT16; the
//! earlier citation to it was incorrect.)

/// CAT16: CIE XYZ → cone responses (LMS).
#[rustfmt::skip]
const XYZ_TO_CONE: [[f64; 3]; 3] = [
    [ 0.401288,  0.650173, -0.051461],
    [-0.250268,  1.204414,  0.045854],
    [-0.002079,  0.048952,  0.953127],
];

/// Обращает опубликованную невырожденную матрицу CAT16 при компиляции.
///
/// Отдельно напечатанный восьмизначный inverse оставлял остаток около 5.4e-9.
/// На грани гамута CAM16 round-trip усиливал его до ложного выхода канала за
/// `[0,1]`. Математическое определение обратного преобразования — именно inverse
/// forward-матрицы, поэтому второй независимо округлённый набор не нужен.
const fn inverse_3x3(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let a = m[0][0];
    let b = m[0][1];
    let c = m[0][2];
    let d = m[1][0];
    let e = m[1][1];
    let f = m[1][2];
    let g = m[2][0];
    let h = m[2][1];
    let i = m[2][2];
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [
            (e * i - f * h) / determinant,
            (c * h - b * i) / determinant,
            (b * f - c * e) / determinant,
        ],
        [
            (f * g - d * i) / determinant,
            (a * i - c * g) / determinant,
            (c * d - a * f) / determinant,
        ],
        [
            (d * h - e * g) / determinant,
            (b * g - a * h) / determinant,
            (a * e - b * d) / determinant,
        ],
    ]
}

const CONE_TO_XYZ: [[f64; 3]; 3] = inverse_3x3(XYZ_TO_CONE);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_inverse_closes_cat16_to_f64_precision() {
        let mut maximum: f64 = 0.0;
        for (row_index, inverse_row) in CONE_TO_XYZ.iter().enumerate() {
            for (column_index, _) in XYZ_TO_CONE[0].iter().enumerate() {
                let product = inverse_row
                    .iter()
                    .zip(XYZ_TO_CONE.iter())
                    .map(|(left, forward_row)| left * forward_row[column_index])
                    .sum::<f64>();
                let expected = if row_index == column_index { 1.0 } else { 0.0 };
                maximum = maximum.max((product - expected).abs());
            }
        }
        assert!(
            maximum <= 4.0 * f64::EPSILON,
            "остаток CAT16 inverse равен {maximum:e}"
        );
    }
}
