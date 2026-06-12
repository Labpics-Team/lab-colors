#[rustfmt::skip]
const SRGB_TO_LMS: [[f64; 3]; 3] = [
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005],
];

#[rustfmt::skip]
const LMS_TO_OKLAB: [[f64; 3]; 3] = [
    [ 0.2104542553,  0.7936177850, -0.0040720468],
    [ 1.9779984951, -2.4285922050,  0.4505937099],
    [ 0.0259040371,  0.7827717662, -0.8086757660],
];

#[rustfmt::skip]
pub(crate) const OKLAB_TO_LMS: [[f64; 3]; 3] = [
    [1.0,  0.3963377774,  0.2158037573],
    [1.0, -0.1055613458, -0.0638541728],
    [1.0, -0.0894841775, -1.2914855480],
];

#[rustfmt::skip]
pub(crate) const LMS_TO_SRGB: [[f64; 3]; 3] = [
    [ 4.0767416621, -3.3077115913,  0.2309699292],
    [-1.2684380046,  2.6097574011, -0.3413193965],
    [-0.0041960863, -0.7034186147,  1.7076147010],
];

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

pub(crate) fn srgb_linear_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let lms = mat_vec_mul(SRGB_TO_LMS, rgb);
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    mat_vec_mul(LMS_TO_OKLAB, lms_)
}

pub(crate) fn oklab_to_srgb_linear(lab: [f64; 3]) -> [f64; 3] {
    let lms_ = mat_vec_mul(OKLAB_TO_LMS, lab);
    let lms = [lms_[0].powi(3), lms_[1].powi(3), lms_[2].powi(3)];
    mat_vec_mul(LMS_TO_SRGB, lms)
}

pub(crate) fn oklab_hue(rgb: [f64; 3]) -> f64 {
    let lab = srgb_linear_to_oklab(rgb);
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::srgb_from_hex;

    #[test]
    fn white_gives_l1_a0_b0() {
        let lab = srgb_linear_to_oklab([1.0, 1.0, 1.0]);
        assert!((lab[0] - 1.0).abs() < 1e-6, "L={}", lab[0]);
        assert!(lab[1].abs() < 1e-6, "a={}", lab[1]);
        assert!(lab[2].abs() < 1e-6, "b={}", lab[2]);
    }

    #[test]
    fn roundtrip_five_colors() {
        let hexes = ["#FF0000", "#00FF00", "#0000FF", "#787880", "#FFD700"];
        for hex in hexes {
            let lin = srgb_from_hex(hex).unwrap();
            let lab = srgb_linear_to_oklab(lin);
            let back = oklab_to_srgb_linear(lab);
            for i in 0..3 {
                assert!(
                    (lin[i] - back[i]).abs() < 1e-6,
                    "{hex} channel {i}: expected {}, got {}",
                    lin[i],
                    back[i]
                );
            }
        }
    }

    #[test]
    fn pure_red_has_positive_a() {
        let lin = srgb_from_hex("#FF0000").unwrap();
        let lab = srgb_linear_to_oklab(lin);
        assert!(lab[1] > 0.0, "a={} should be positive for red", lab[1]);
    }

    #[test]
    fn pure_blue_has_negative_b() {
        let lin = srgb_from_hex("#0000FF").unwrap();
        let lab = srgb_linear_to_oklab(lin);
        assert!(lab[2] < 0.0, "b={} should be negative for blue", lab[2]);
    }

    #[test]
    fn hue_returns_degrees_0_360() {
        // Red ≈ 24.5°, Green ≈ 142°, Blue ≈ 264° — Oklab canonical values
        let lin_r = srgb_from_hex("#FF0000").unwrap();
        let lin_g = srgb_from_hex("#00FF00").unwrap();
        let lin_b = srgb_from_hex("#0000FF").unwrap();

        let h_r = oklab_hue(lin_r);
        let h_g = oklab_hue(lin_g);
        let h_b = oklab_hue(lin_b);

        // All hues in [0, 360)
        for &h in &[h_r, h_g, h_b] {
            assert!((0.0..360.0).contains(&h), "hue {} not in [0, 360)", h);
        }

        // Red quadrant (≈29°)
        assert!(
            (h_r - 29.2).abs() < 1.0,
            "red hue = {}°, expected ≈29.2°",
            h_r
        );
        // Green quadrant (≈142°)
        assert!(
            (h_g - 142.0).abs() < 3.0,
            "green hue = {}°, expected ≈142°",
            h_g
        );
        // Blue quadrant (≈264°)
        assert!(
            (h_b - 264.0).abs() < 3.0,
            "blue hue = {}°, expected ≈264°",
            h_b
        );
    }

    #[test]
    fn hue_achromatic_is_not_nan() {
        let lin_w = srgb_from_hex("#FFFFFF").unwrap();
        let lin_k = srgb_from_hex("#000000").unwrap_or([0.0, 0.0, 0.0]);
        let h_w = oklab_hue(lin_w);
        let h_k = oklab_hue(lin_k);
        assert!(!h_w.is_nan(), "white hue should not be NaN");
        assert!(!h_k.is_nan(), "black hue should not be NaN");
        assert!((0.0..360.0).contains(&h_w), "white hue in range");
        assert!((0.0..360.0).contains(&h_k), "black hue in range");
    }
}
