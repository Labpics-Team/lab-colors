//! Oklab perceptual colour space (sRGB path).
//!
//! Source: Björn Ottosson, "A perceptual color space for image processing"
//! (2020), <https://bottosson.github.io/posts/oklab/>. The matrices below are
//! the 2021-01-25 revision (the values Ottosson pinned after the original post)
//! and were checked against that reference implementation to the last printed
//! digit (audit 2026-07-03).
//!
//! The published model does not send D65 white exactly to `L = 1`: the
//! `LMS_TO_OKLAB` first-row sum gives `L = 0.9999999935 ≠ 1` for `[1, 1, 1]`.
//! That ~6.5e-9 offset is a property of the published matrices themselves, not a
//! porting error, and is absorbed by the `< 1e-6` white-point test below.

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

/// Canonical degree domain of a hue angle.
pub(crate) const HUE_DEG_MIN_INCLUSIVE: f64 = 0.0;
pub(crate) const HUE_DEG_MAX_EXCLUSIVE: f64 = 360.0;

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

/// Exact linear-sRGB grey at an Oklab lightness coordinate.
///
/// On the neutral Oklab ray `a = b = 0`, the first column of `OKLAB_TO_LMS` is
/// exactly one, so all three cube-root LMS channels equal `L`; cubing gives
/// `L³`. Every row of `LMS_TO_SRGB` sums to one, hence every output channel is
/// the same `L³`. Sharing that scalar directly avoids row-specific round-off.
pub(crate) fn neutral_srgb_linear(l_ok: f64) -> [f64; 3] {
    let channel = (l_ok * l_ok * l_ok).clamp(0.0, 1.0);
    [channel; 3]
}

pub(crate) fn oklab_hue(rgb: [f64; 3]) -> f64 {
    // Хью зависит только от Oklab-компонент `a`, `b` (строки 1, 2 матрицы
    // `LMS_TO_OKLAB`) — светлота `L` (строка 0) не участвует. Считаем ровно эти
    // две проекции нелинейной `lms_`, не гоняя полный `srgb_linear_to_oklab` с
    // выбрасываемой строкой `L`. Значения `a`, `b` — те же независимые скалярные
    // произведения (те же операнды, тот же порядок слева-направо), что даёт полный
    // matmul, поэтому результат байт-идентичен; пинится golden/reference-векторами
    // (`h_ok`). Пропуск строки `L` убирает одно скалярное произведение и, главное,
    // одну ветку зависимостей, которую LLVM не всегда вычищает через границу
    // `mat_vec_mul` (массив `[f64; 3]` целиком).
    let lms = mat_vec_mul(SRGB_TO_LMS, rgb);
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let a =
        LMS_TO_OKLAB[1][0] * lms_[0] + LMS_TO_OKLAB[1][1] * lms_[1] + LMS_TO_OKLAB[1][2] * lms_[2];
    let b =
        LMS_TO_OKLAB[2][0] * lms_[0] + LMS_TO_OKLAB[2][1] * lms_[1] + LMS_TO_OKLAB[2][2] * lms_[2];
    // `atan2().to_degrees()` лежит в (−180, 180], поэтому `rem_euclid(360)` здесь
    // тождественно равен одной условной прибавке 360 (для отрицательного угла) —
    // байт-идентично, но без floating-modulo. Та же замена, что в `cam16::
    // forward_compute`; байт-гейтится golden/reference-векторами (`h_ok`).
    let deg = b.atan2(a).to_degrees();
    if deg < 0.0 { deg + 360.0 } else { deg }
}

/// Oklab hue identity carried by one exact encoded-sRGB8 stimulus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum OklabHue {
    /// Equal encoded channel bytes: there is no chromatic direction to amplify.
    Achromatic,
    /// Unequal encoded channel bytes with their Oklab angular coordinate.
    Chromatic { degrees: f64 },
}

/// Classify an exact emitted sRGB8 stimulus before deriving its Oklab angle.
///
/// The grey-axis branch is exact byte geometry. It prevents `atan2` of matrix
/// round-off from becoming an invented saturated colour without introducing a
/// tolerance or a perceptual claim.
pub(crate) fn hue_of_srgb8(source: crate::Srgb8) -> OklabHue {
    if source.is_achromatic() {
        return OklabHue::Achromatic;
    }
    let linear = super::srgb::srgb_linear_from_srgb8(source);
    OklabHue::Chromatic {
        degrees: oklab_hue(linear),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::{srgb_from_hex, srgb_gamma_inv};

    #[test]
    fn srgb8_hue_classification_is_exact_and_returns_chromatic_coordinates() {
        for base in 0_i16..=255 {
            for red_delta in -1_i16..=1 {
                for green_delta in -1_i16..=1 {
                    for blue_delta in -1_i16..=1 {
                        let channels = [base + red_delta, base + green_delta, base + blue_delta];
                        if channels.iter().any(|channel| !(0..=255).contains(channel)) {
                            continue;
                        }
                        let rgb = channels.map(|channel| channel as u8);
                        let source = crate::Srgb8::new(rgb);
                        if source.is_achromatic() {
                            assert_eq!(hue_of_srgb8(source), OklabHue::Achromatic);
                        } else {
                            let OklabHue::Chromatic { degrees } = hue_of_srgb8(source) else {
                                panic!(
                                    "unequal sRGB8 bytes must retain a chromatic direction: {rgb:?}"
                                );
                            };
                            let linear =
                                rgb.map(|channel| srgb_gamma_inv(f64::from(channel) / 255.0));
                            assert_eq!(degrees.to_bits(), oklab_hue(linear).to_bits());
                        }
                    }
                }
            }
        }
    }

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
}
