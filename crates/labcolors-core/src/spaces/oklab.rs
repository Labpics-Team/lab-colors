//! Перцептивное цветовое пространство Oklab (путь sRGB).
//!
//! Источник определения: Björn Ottosson, «A perceptual color space for image
//! processing» (2020), <https://bottosson.github.io/posts/oklab/>. Численный
//! reference — пересчитанные для binary64 матрицы XYZ D65 → Oklab из CSS Color
//! 4, <https://www.w3.org/TR/css-color-4/#color-conversion-code>.
//!
//! Oklab определяется прямым преобразованием XYZ → LMS → Oklab. Поэтому именно
//! опубликованные forward-матрицы являются источником истины, а обратные
//! выводятся из них при компиляции. Это сохраняет определение модели и убирает
//! ошибку около 2.6e-7 двух независимо округлённых направлений.

use super::srgb::{SRGB_TO_XYZ_D65, XYZ_D65_TO_SRGB, srgb_to_xyz};

/// Официальная binary64-матрица CSS Color 4: XYZ D65 → LMS Oklab.
// Десятичные записи скопированы из нормативного reference-кода без усечения:
// предупреждение Clippy здесь противоречило бы цели побитового воспроизведения.
#[allow(clippy::excessive_precision)]
#[rustfmt::skip]
const XYZ_TO_LMS: [[f64; 3]; 3] = [
    [0.8190224379967030, 0.3619062600528904, -0.1288737815209879],
    [0.0329836539323885, 0.9292868615863434,  0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
];

/// Официальная binary64-матрица CSS Color 4: LMS′ → Oklab.
#[allow(clippy::excessive_precision)]
#[rustfmt::skip]
const LMS_TO_OKLAB: [[f64; 3]; 3] = [
    [0.2104542683093140,  0.7936177747023054, -0.0040720430116193],
    [1.9779985324311684, -2.4285922420485799,  0.4505937096174110],
    [0.0259040424655478,  0.7827717124575296, -0.8086757549230774],
];

/// Обращает фиксированную невырожденную матрицу 3×3 при вычислении констант.
///
/// Канонические forward-матрицы Oklab невырождены по построению. Функция закрыта,
/// чтобы это предусловие не превратилось в обещание универсального matrix API.
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

    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);

    [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ]
}

/// Умножает две фиксированные матрицы при вычислении констант.
const fn mat_mul_const(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut product = [[0.0; 3]; 3];
    let mut row = 0;
    while row < 3 {
        let mut column = 0;
        while column < 3 {
            product[row][column] =
                a[row][0] * b[0][column] + a[row][1] * b[1][column] + a[row][2] * b[2][column];
            column += 1;
        }
        row += 1;
    }
    product
}

// Обратные и быстрые sRGB-матрицы выводятся из официального forward. Ни одно
// второе направление не получает независимо округлённую таблицу коэффициентов.
pub(crate) const OKLAB_TO_LMS: [[f64; 3]; 3] = inverse_3x3(LMS_TO_OKLAB);
const LMS_TO_XYZ: [[f64; 3]; 3] = inverse_3x3(XYZ_TO_LMS);
const SRGB_TO_LMS: [[f64; 3]; 3] = mat_mul_const(XYZ_TO_LMS, SRGB_TO_XYZ_D65);
pub(crate) const LMS_TO_SRGB: [[f64; 3]; 3] = mat_mul_const(XYZ_D65_TO_SRGB, LMS_TO_XYZ);

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

pub(crate) fn srgb_linear_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    // Равные sRGB-каналы задают ахроматическую ось до любой округлённой
    // матрицы. Эта аналитическая ветвь не даёт округлению придумать hue серого.
    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        return [rgb[0].cbrt(), 0.0, 0.0];
    }

    // В физическом sRGB предкомпозиция отличается от официального пошагового
    // XYZ-пути лишь ошибкой арифметики f64. Для extended RGB порядок округления
    // около LMS=0 может усиливаться cbrt, поэтому там сохраняется буквальный
    // путь CSS Color 4.
    let lms = if rgb
        .into_iter()
        .all(|channel| (0.0..=1.0).contains(&channel))
    {
        mat_vec_mul(SRGB_TO_LMS, rgb)
    } else {
        mat_vec_mul(XYZ_TO_LMS, srgb_to_xyz(rgb))
    };
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    mat_vec_mul(LMS_TO_OKLAB, lms_)
}

pub(crate) fn oklab_to_srgb_linear(lab: [f64; 3]) -> [f64; 3] {
    // На ахроматической оси точное уравнение равно RGB=L³. Явная форма
    // сохраняет равенство каналов вместо трёх разных ошибок матричных сумм.
    if lab[1] == 0.0 && lab[2] == 0.0 {
        let value = lab[0] * lab[0] * lab[0];
        return [value, value, value];
    }
    let lms_ = mat_vec_mul(OKLAB_TO_LMS, lab);
    let lms = [lms_[0].powi(3), lms_[1].powi(3), lms_[2].powi(3)];
    mat_vec_mul(LMS_TO_SRGB, lms)
}

pub(crate) fn oklab_hue(rgb: [f64; 3]) -> f64 {
    // Совместимый числовой placeholder: у ахромата hue отсутствует, а ноль не
    // должен интерпретироваться как измеренное красное направление. Публичный
    // полярный API несёт это состояние через `Option`.
    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        return 0.0;
    }
    // L не влияет на hue; явные проекции a и b сохраняют тот же порядок операций,
    // но не создают бесполезную ветвь вычисления светлоты.
    let lms = if rgb
        .into_iter()
        .all(|channel| (0.0..=1.0).contains(&channel))
    {
        mat_vec_mul(SRGB_TO_LMS, rgb)
    } else {
        mat_vec_mul(XYZ_TO_LMS, srgb_to_xyz(rgb))
    };
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let a =
        LMS_TO_OKLAB[1][0] * lms_[0] + LMS_TO_OKLAB[1][1] * lms_[1] + LMS_TO_OKLAB[1][2] * lms_[2];
    let b =
        LMS_TO_OKLAB[2][0] * lms_[0] + LMS_TO_OKLAB[2][1] * lms_[1] + LMS_TO_OKLAB[2][2] * lms_[2];

    let deg = b.atan2(a).to_degrees();
    if deg < 0.0 { deg + 360.0 } else { deg }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::srgb_from_hex;

    fn mat_mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let mut product = [[0.0; 3]; 3];
        for (row, a_row) in a.iter().enumerate() {
            for column in 0..3 {
                product[row][column] =
                    a_row[0] * b[0][column] + a_row[1] * b[1][column] + a_row[2] * b[2][column];
            }
        }
        product
    }

    fn identity_residual(product: [[f64; 3]; 3]) -> f64 {
        let mut residual: f64 = 0.0;
        for (row, values) in product.iter().enumerate() {
            for (column, &value) in values.iter().enumerate() {
                let expected = if row == column { 1.0 } else { 0.0 };
                residual = residual.max((value - expected).abs());
            }
        }
        residual
    }

    fn max_roundtrip_error(
        forward_lms: [[f64; 3]; 3],
        forward_lab: [[f64; 3]; 3],
        inverse_lms: [[f64; 3]; 3],
        inverse_rgb: [[f64; 3]; 3],
    ) -> f64 {
        let mut maximum: f64 = 0.0;
        for red in 0..=32 {
            for green in 0..=32 {
                for blue in 0..=32 {
                    let rgb = [red as f64 / 32.0, green as f64 / 32.0, blue as f64 / 32.0];
                    let lms = mat_vec_mul(forward_lms, rgb);
                    let lab =
                        mat_vec_mul(forward_lab, [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()]);
                    let lms_ = mat_vec_mul(inverse_lms, lab);
                    let back = mat_vec_mul(
                        inverse_rgb,
                        [lms_[0].powi(3), lms_[1].powi(3), lms_[2].powi(3)],
                    );
                    for channel in 0..3 {
                        maximum = maximum.max((rgb[channel] - back[channel]).abs());
                    }
                }
            }
        }
        maximum
    }

    #[test]
    fn analytic_neutral_branch_preserves_the_axis_exactly() {
        for l in [-1.0, -0.25, 0.0, 0.125, 0.5, 1.0, 1.5] {
            let rgb = oklab_to_srgb_linear([l, 0.0, 0.0]);
            assert_eq!(rgb[0], rgb[1], "L={l}: красный канал не равен зелёному");
            assert_eq!(rgb[1], rgb[2], "L={l}: зелёный канал не равен синему");
            let back = srgb_linear_to_oklab(rgb);
            assert!((back[0] - l).abs() <= f64::EPSILON * l.abs().max(1.0));
            assert_eq!([back[1], back[2]], [0.0, 0.0]);
        }
    }

    #[test]
    fn matrices_derived_from_forward_are_numerical_inverses() {
        let residuals = [
            identity_residual(mat_mul(OKLAB_TO_LMS, LMS_TO_OKLAB)),
            identity_residual(mat_mul(LMS_TO_OKLAB, OKLAB_TO_LMS)),
            identity_residual(mat_mul(LMS_TO_SRGB, SRGB_TO_LMS)),
            identity_residual(mat_mul(SRGB_TO_LMS, LMS_TO_SRGB)),
        ];
        for residual in residuals {
            assert!(
                residual <= 8.0 * f64::EPSILON,
                "остаток обратной матрицы {residual:e} больше ошибки арифметики f64"
            );
        }
    }

    #[test]
    fn dense_roundtrip_eliminates_independent_rounding_error() {
        #[rustfmt::skip]
        const LEGACY_SRGB_TO_LMS: [[f64; 3]; 3] = [
            [0.4122214708, 0.5363325363, 0.0514459929],
            [0.2119034982, 0.6806995451, 0.1073969566],
            [0.0883024619, 0.2817188376, 0.6299787005],
        ];
        #[rustfmt::skip]
        const LEGACY_LMS_TO_OKLAB: [[f64; 3]; 3] = [
            [ 0.2104542553,  0.7936177850, -0.0040720468],
            [ 1.9779984951, -2.4285922050,  0.4505937099],
            [ 0.0259040371,  0.7827717662, -0.8086757660],
        ];
        #[rustfmt::skip]
        const LEGACY_OKLAB_TO_LMS: [[f64; 3]; 3] = [
            [1.0,  0.3963377774,  0.2158037573],
            [1.0, -0.1055613458, -0.0638541728],
            [1.0, -0.0894841775, -1.2914855480],
        ];
        #[rustfmt::skip]
        const LEGACY_LMS_TO_SRGB: [[f64; 3]; 3] = [
            [ 4.0767416621, -3.3077115913,  0.2309699292],
            [-1.2684380046,  2.6097574011, -0.3413193965],
            [-0.0041960863, -0.7034186147,  1.7076147010],
        ];

        let legacy = max_roundtrip_error(
            LEGACY_SRGB_TO_LMS,
            LEGACY_LMS_TO_OKLAB,
            LEGACY_OKLAB_TO_LMS,
            LEGACY_LMS_TO_SRGB,
        );
        let coherent = max_roundtrip_error(SRGB_TO_LMS, LMS_TO_OKLAB, OKLAB_TO_LMS, LMS_TO_SRGB);

        eprintln!("Oklab 33^3 round-trip: старая={legacy:.17e}, согласованная={coherent:.17e}");
        assert!(
            legacy > 1.0e-7,
            "аудитная выборка больше не обнаруживает прежний дефект"
        );
        assert!(
            coherent <= 64.0 * f64::EPSILON,
            "ошибка согласованного round-trip {coherent:e} вышла за границу f64"
        );
    }

    #[test]
    fn fast_physical_srgb_path_matches_official_xyz_path() {
        let mut maximum: f64 = 0.0;
        for red in 0..=64 {
            for green in 0..=64 {
                for blue in 0..=64 {
                    let rgb = [red as f64 / 64.0, green as f64 / 64.0, blue as f64 / 64.0];
                    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
                        continue;
                    }
                    let fast = srgb_linear_to_oklab(rgb);
                    let lms = mat_vec_mul(XYZ_TO_LMS, srgb_to_xyz(rgb));
                    let official =
                        mat_vec_mul(LMS_TO_OKLAB, [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()]);
                    maximum = maximum.max(
                        (fast[0] - official[0])
                            .hypot(fast[1] - official[1])
                            .hypot(fast[2] - official[2]),
                    );
                }
            }
        }
        assert!(
            maximum <= 16.0 * f64::EPSILON,
            "быстрый sRGB-путь разошёлся с CSS XYZ reference на ΔEOK={maximum:e}"
        );
    }

    #[test]
    fn extended_linear_rgb_roundtrips_without_clamping() {
        let samples: [[f64; 3]; 4] = [
            [-0.25, 0.4, 1.5],
            [2.0, -1.0, 0.5],
            [-1.0, -0.5, -0.25],
            [4.0, 2.0, -3.0],
        ];
        for rgb in samples {
            let back = oklab_to_srgb_linear(srgb_linear_to_oklab(rgb));
            let scale = rgb.into_iter().map(f64::abs).fold(1.0, f64::max);
            for channel in 0..3 {
                assert!(
                    (rgb[channel] - back[channel]).abs() <= 32.0 * f64::EPSILON * scale,
                    "расширенный RGB {rgb:?}, канал {channel}: round-trip дал {back:?}"
                );
            }
        }
    }

    #[test]
    fn white_gives_l1_a0_b0() {
        let lab = srgb_linear_to_oklab([1.0, 1.0, 1.0]);
        for (name, got, expected) in [("L", lab[0], 1.0), ("a", lab[1], 0.0), ("b", lab[2], 0.0)] {
            assert!(
                (got - expected).abs() <= 8.0 * f64::EPSILON,
                "{name}={got}, ожидалось {expected}"
            );
        }
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
                    (lin[i] - back[i]).abs() <= 64.0 * f64::EPSILON,
                    "{hex}, канал {i}: ожидалось {}, получено {}",
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
        assert!(
            lab[1] > 0.0,
            "для красного a={} должна быть положительной",
            lab[1]
        );
    }

    #[test]
    fn pure_blue_has_negative_b() {
        let lin = srgb_from_hex("#0000FF").unwrap();
        let lab = srgb_linear_to_oklab(lin);
        assert!(
            lab[2] < 0.0,
            "для синего b={} должна быть отрицательной",
            lab[2]
        );
    }

    #[test]
    fn hue_returns_degrees_0_360() {
        let h_r = oklab_hue(srgb_from_hex("#FF0000").unwrap());
        let h_g = oklab_hue(srgb_from_hex("#00FF00").unwrap());
        let h_b = oklab_hue(srgb_from_hex("#0000FF").unwrap());

        for &hue in &[h_r, h_g, h_b] {
            assert!((0.0..360.0).contains(&hue), "hue {hue} вне [0, 360)");
        }
        assert!(
            (h_r - 29.2).abs() < 1.0,
            "hue красного = {h_r}°, ожидалось около 29.2°"
        );
        assert!(
            (h_g - 142.0).abs() < 3.0,
            "hue зелёного = {h_g}°, ожидалось около 142°"
        );
        assert!(
            (h_b - 264.0).abs() < 3.0,
            "hue синего = {h_b}°, ожидалось около 264°"
        );
    }

    #[test]
    fn achromatic_numeric_hue_is_only_a_compatibility_placeholder() {
        let h_w = oklab_hue(srgb_from_hex("#FFFFFF").unwrap());
        let h_k = oklab_hue(srgb_from_hex("#000000").unwrap_or([0.0, 0.0, 0.0]));
        assert_eq!(h_w, 0.0);
        assert_eq!(h_k, 0.0);
    }
}
