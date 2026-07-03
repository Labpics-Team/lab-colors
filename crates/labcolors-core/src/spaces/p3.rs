//! CSS-эмиссия Display P3 — вторая поддерживаемая форма записи наружу.
//!
//! Закон формы тот же, что у [`super::oklch`]: **система координат записи, не
//! расширение гамута**. Значения остаются решёнными в sRGB-гамуте (sRGB ⊂ P3,
//! поэтому каждая решённая роль представима в P3 точно, с запасом от стен).
//! Расширение самого решателя на гамут P3 — поездными этапами:
//! **этап 1 (2026-07-03, сделан)** — геометрия стен ([`crate::scale`]::
//! `max_chroma_p3_bisect`) и 8-битная решётка эмиссии этого модуля
//! (`p3_bytes_from_linear` / `p3_css_from_bytes`, байт-точный round-trip);
//! **этап 2 (следующий)** — P3-кандидаты в солвере и перевод `Solved`/эмиссии
//! с hex-строки на типизированный цвет (hex непредставим вне sRGB).
//!
//! Матрицы — CSS Color Module Level 4 (та же деривация, что у sRGB-матриц в
//! [`super::srgb`]: <https://github.com/w3c/csswg-drafts/issues/5922>, значения
//! эталонной реализации colorjs.io). Передаточная функция Display P3 идентична
//! sRGB (IEC 61966-2-1 § 6.4) — переиспользуются [`super::srgb::srgb_gamma`] /
//! [`super::srgb::srgb_gamma_inv`], вторых копий кривой нет.
//!
//! Точность цифр подобрана под БАЙТ-ТОЧНЫЙ round-trip: `p3_css_from_hex` →
//! парс → XYZ → sRGB даёт исходные 8-битные байты на решётке всего куба
//! (доказательство — тест `round_trip_is_byte_exact_on_lattice`).

use super::srgb::{srgb_from_hex, srgb_gamma, srgb_to_xyz};

// ------------------------------------------------------------------
//  linear Display P3 → XYZ(D65)
// ------------------------------------------------------------------
#[rustfmt::skip]
const P3_TO_XYZ_D65: [[f64; 3]; 3] = [
    [ 0.486_570_948_648_216_15,  0.265_667_693_169_093_06,  0.198_217_285_234_362_5   ],
    [ 0.228_974_564_069_748_78,  0.691_738_521_836_506_4,   0.079_286_914_093_745     ],
    [ 0.0,                       0.045_113_381_858_902_64,  1.043_944_368_900_976     ],
];

// ------------------------------------------------------------------
//  XYZ(D65) → linear Display P3
// ------------------------------------------------------------------
#[rustfmt::skip]
const XYZ_D65_TO_P3: [[f64; 3]; 3] = [
    [ 2.493_496_911_941_425,    -0.931_383_617_919_123_9,  -0.402_710_784_450_716_84 ],
    [-0.829_488_969_561_574_7,   1.762_664_060_318_346_3,   0.023_624_685_841_943_577],
    [ 0.035_845_830_243_784_47, -0.076_172_389_268_041_82,  0.956_884_524_007_687_2  ],
];

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// XYZ(D65, Y∈[0,1]) → линейный Display P3.
pub(crate) fn xyz_to_p3_linear(xyz: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(XYZ_D65_TO_P3, xyz)
}

/// Линейный Display P3 → XYZ(D65, Y∈[0,1]).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn p3_linear_to_xyz(rgb: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(P3_TO_XYZ_D65, rgb)
}

/// Гашение вычислительного шума у краёв [0, 1].
///
/// Для sRGB-входа компоненты P3 математически лежат в [0, 1] (sRGB ⊂ P3);
/// за края может выйти только f64-шум цепочки матриц (≲1e-12, у белой точки —
/// две независимые деривации D65). Шум гасится, реальный выход за гамут —
/// честная ошибка вызывающего (сюда такие значения не приходят, пока солвер
/// работает в sRGB-гамуте; гард — на будущий gamut-aware этап).
const GAMUT_NOISE: f64 = 1e-9;

fn clamp_gamut_noise(v: f64) -> Result<f64, String> {
    if !(-GAMUT_NOISE..=1.0 + GAMUT_NOISE).contains(&v) {
        return Err(format!("компонента P3 вне гамута: {v}"));
    }
    Ok(v.clamp(0.0, 1.0))
}

/// Гамма-кодированные компоненты Display P3 `[r, g, b]` (каждая в [0, 1])
/// из sRGB-hex-солида.
///
/// Путь: hex → линейный sRGB → XYZ(D65) → линейный P3 → передаточная кривая
/// (общая с sRGB, IEC 61966-2-1 § 6.4).
///
/// # Errors
///
/// `Err` — невалидный hex (пробрасывается из парсера) либо компонента вне
/// гамута сверх шумового эпсилона (недостижимо для валидного sRGB-входа).
pub fn p3_from_hex(hex: &str) -> Result<[f64; 3], String> {
    let xyz = srgb_to_xyz(srgb_from_hex(hex)?);
    let lin = xyz_to_p3_linear(xyz);
    Ok([
        srgb_gamma(clamp_gamut_noise(lin[0])?),
        srgb_gamma(clamp_gamut_noise(lin[1])?),
        srgb_gamma(clamp_gamut_noise(lin[2])?),
    ])
}

/// CSS-строка `color(display-p3 R G B)` / `color(display-p3 R G B / A)` из
/// sRGB-hex-солида и опциональной альфы.
///
/// Точность: 6 знаков на компоненту — ошибка квантования печати ≤ 5·10⁻⁷ при
/// полушаге 8-битного канала ≈ 2·10⁻³, запас > 3 порядков; байт-точность
/// round-trip доказана тестом на решётке всего куба. Политика альфы — единая
/// ([`super::oklch::css_alpha_suffix`]): та же, что у oklch-эмиссии.
///
/// # Errors
///
/// `Err` — невалидный hex либо альфа вне [0, 1] сверх шумового эпсилона.
pub fn p3_css_from_hex(hex: &str, alpha: Option<f64>) -> Result<String, String> {
    let [r, g, b] = p3_from_hex(hex)?;
    let suffix = super::oklch::css_alpha_suffix(alpha)?;
    Ok(format!("color(display-p3 {r:.6} {g:.6} {b:.6}{suffix})"))
}

// ------------------------------------------------------------------
//  8-битная решётка эмиссии P3 (этап 1 gamut-aware солвера, 2026-07-03)
// ------------------------------------------------------------------

/// 8-битное квантование линейного P3: передаточная кривая (общая с sRGB) →
/// байты. Решётка кандидатов будущего P3-солвера — зеркало sRGB-пути
/// (quantise + измерение на отданном значении).
///
/// # Errors
///
/// `Err` — компонента вне гамута P3 сверх шумового эпсилона: квантовать
/// не-цвет молча нельзя (честная граница, ADR-0002 закон 3).
#[cfg_attr(not(test), allow(dead_code))] // прод-потребитель — этап 2
pub(crate) fn p3_bytes_from_linear(lin: [f64; 3]) -> Result<[u8; 3], String> {
    let mut out = [0_u8; 3];
    for (i, &v) in lin.iter().enumerate() {
        let encoded = srgb_gamma(clamp_gamut_noise(v)?);
        out[i] = (encoded * 255.0).round() as u8;
    }
    Ok(out)
}

/// Линейный P3 из 8-битных байтов решётки (обратный путь квантования).
#[cfg_attr(not(test), allow(dead_code))] // прод-потребитель — этап 2
pub(crate) fn p3_linear_from_bytes(bytes: [u8; 3]) -> [f64; 3] {
    [
        super::srgb::srgb_gamma_inv(f64::from(bytes[0]) / 255.0),
        super::srgb::srgb_gamma_inv(f64::from(bytes[1]) / 255.0),
        super::srgb::srgb_gamma_inv(f64::from(bytes[2]) / 255.0),
    ]
}

/// CSS-строка `color(display-p3 R G B [/ A])` из 8-битных байтов решётки.
/// Точность печати и политика альфы — те же, что у [`p3_css_from_hex`]
/// (6 знаков: полушаг канала ≈ 2·10⁻³, запас > 3 порядков; байт-точность
/// round-trip доказана тестом на решётке).
#[cfg_attr(not(test), allow(dead_code))] // прод-потребитель — этап 2
pub(crate) fn p3_css_from_bytes(bytes: [u8; 3], alpha: Option<f64>) -> Result<String, String> {
    let r = f64::from(bytes[0]) / 255.0;
    let g = f64::from(bytes[1]) / 255.0;
    let b = f64::from(bytes[2]) / 255.0;
    let suffix = super::oklch::css_alpha_suffix(alpha)?;
    Ok(format!("color(display-p3 {r:.6} {g:.6} {b:.6}{suffix})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::{hex_from_srgb, srgb_gamma_inv, xyz_to_srgb};

    /// Парсер эмитированной строки — эталонная реконструкция потребителя:
    /// браузер декодирует компоненты той же передаточной кривой и тем же
    /// матричным путём P3 → XYZ → sRGB.
    fn parse_emitted(css: &str) -> (String, Option<f64>) {
        let inner = css
            .strip_prefix("color(display-p3 ")
            .and_then(|s| s.strip_suffix(')'))
            .expect("форма color(display-p3 ...)");
        let (rgb_str, alpha) = match inner.split_once(" / ") {
            Some((rgb, a)) => (rgb, Some(a.parse::<f64>().expect("альфа — число"))),
            None => (inner, None),
        };
        let parts: Vec<f64> = rgb_str
            .split_whitespace()
            .map(|p| {
                p.parse::<f64>()
                    .unwrap_or_else(|_| panic!("компонента не число: {p} в {css}"))
            })
            .collect();
        assert_eq!(parts.len(), 3, "ровно R G B: {css}");
        let lin_p3 = [
            srgb_gamma_inv(parts[0]),
            srgb_gamma_inv(parts[1]),
            srgb_gamma_inv(parts[2]),
        ];
        let xyz = p3_linear_to_xyz(lin_p3);
        (hex_from_srgb(xyz_to_srgb(xyz)), alpha)
    }

    /// Матрицы — взаимные обратные: P3 → XYZ → P3 тождественно до f64-шума.
    #[test]
    fn matrices_are_mutual_inverses() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.25, 0.5, 0.75],
        ] {
            let back = xyz_to_p3_linear(p3_linear_to_xyz(rgb));
            for i in 0..3 {
                assert!(
                    (back[i] - rgb[i]).abs() < 1e-12,
                    "P3 roundtrip канал {i}: {} vs {}",
                    back[i],
                    rgb[i]
                );
            }
        }
    }

    /// Белые точки согласованы: sRGB-белый → P3 (1, 1, 1) до шума двух
    /// независимых D65-дериваций (обе цепочки CSS Color 4).
    #[test]
    fn srgb_white_maps_to_p3_white() {
        let [r, g, b] = p3_from_hex("#FFFFFF").unwrap();
        for (ch, v) in [("r", r), ("g", g), ("b", b)] {
            assert!((v - 1.0).abs() < 1e-6, "белый канал {ch}: {v}");
        }
    }

    /// sRGB ⊂ P3: каждая точка решётки куба конвертируется без выхода за
    /// гамут (кламп только шумового эпсилона — иначе p3_from_hex вернул бы Err).
    /// Шаг 17 взаимно прост с 255 — решётка не выровнена по «удобным» байтам.
    #[test]
    fn srgb_cube_is_inside_p3_gamut() {
        let steps: Vec<u8> = (0u16..=255).step_by(17).map(|v| v as u8).collect();
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    let hex = format!("#{r:02X}{g:02X}{b:02X}");
                    p3_from_hex(&hex).unwrap_or_else(|e| panic!("{hex} вне P3: {e}"));
                }
            }
        }
    }

    /// Байт-точность round-trip на решётке 8-битного куба с шагом 5 (включая
    /// края 0 и 255): формат → парс → P3 → XYZ → sRGB → те же байты.
    #[test]
    fn round_trip_is_byte_exact_on_lattice() {
        let steps: Vec<u8> = (0u16..=255).step_by(5).map(|v| v as u8).collect();
        assert!(steps.contains(&0) && steps.contains(&255));
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    let hex = format!("#{r:02X}{g:02X}{b:02X}");
                    let css = p3_css_from_hex(&hex, None).unwrap();
                    let (back, alpha) = parse_emitted(&css);
                    assert_eq!(back, hex, "round-trip разошёлся: {css}");
                    assert_eq!(alpha, None);
                }
            }
        }
    }

    /// Серые с альфой: полный грей-рамп байт-точен, альфа проходит как данные.
    #[test]
    fn round_trip_is_byte_exact_on_greys_with_alpha() {
        for v in 0u16..=255 {
            let v = v as u8;
            let hex = format!("#{v:02X}{v:02X}{v:02X}");
            let css = p3_css_from_hex(&hex, Some(0.361)).unwrap();
            let (back, alpha) = parse_emitted(&css);
            assert_eq!(back, hex, "grey round-trip разошёлся: {css}");
            assert_eq!(alpha, Some(0.361));
        }
    }

    /// Политика альфы — единая с oklch-эмиссией (общий дом css_alpha_suffix):
    /// NaN/грубый выход — ошибка, шум у краёв — кламп.
    #[test]
    fn alpha_guard_shared_with_oklch() {
        assert!(p3_css_from_hex("#101012", Some(f64::NAN)).is_err());
        assert!(p3_css_from_hex("#101012", Some(-10.0)).is_err());
        assert!(p3_css_from_hex("#101012", Some(2.0)).is_err());
        let noisy = p3_css_from_hex("#101012", Some(-1e-7)).unwrap();
        assert!(noisy.ends_with(" / 0)"), "шум у нуля клампится: {noisy}");
        let over = p3_css_from_hex("#101012", Some(1.0 + 1e-9)).unwrap();
        assert!(over.ends_with(" / 1)"), "шум у единицы клампится: {over}");
    }

    /// Форма строки — контракт потребителя: `color(display-p3 R G B [/ A])`,
    /// компоненты в [0, 1], без знакового нуля.
    #[test]
    fn css_shape_is_the_contract() {
        let solid = p3_css_from_hex("#3E87FF", None).unwrap();
        assert!(solid.starts_with("color(display-p3 ") && solid.ends_with(')'));
        assert!(!solid.contains('/'));
        assert!(!solid.contains("-0."), "signed zero запрещён: {solid}");
        let translucent = p3_css_from_hex("#101012", Some(0.122)).unwrap();
        assert!(translucent.contains(" / 0.122)"));
        // Чистый sRGB-красный внутри P3 — менее насыщен, чем P3-красный:
        // r < 1, g/b > 0 (иначе матрицы перепутаны).
        let [r, g, b] = p3_from_hex("#FF0000").unwrap();
        assert!(r > 0.9 && r < 1.0, "P3 r красного: {r}");
        assert!(g > 0.0 && b > 0.0, "P3 g/b красного: {g}/{b}");
    }

    /// Этап 1 gamut-aware: стена P3 не уже sRGB-стены НИГДЕ (sRGB ⊂ P3) и
    /// СТРОГО шире на насыщенных срединных светлотах (зелёная зона P3 —
    /// самое сильное расширение). Сетка L × h покрывает обе ветки бисекции.
    #[test]
    fn p3_wall_dominates_srgb_wall() {
        let mut strictly_wider_somewhere = false;
        for l10 in 2..=9 {
            let l = f64::from(l10) / 10.0;
            for h in (0..360).step_by(15) {
                let h = f64::from(h);
                let srgb = crate::scale::max_chroma_bisect(l, h);
                let p3 = crate::scale::max_chroma_p3_bisect(l, h);
                assert!(
                    p3 >= srgb - 1e-9,
                    "P3-стена уже sRGB при L={l}, h={h}: {p3} < {srgb}"
                );
                if p3 > srgb * 1.05 {
                    strictly_wider_somewhere = true;
                }
            }
        }
        assert!(
            strictly_wider_somewhere,
            "P3 обязан быть строго шире sRGB хоть где-то (иначе матрицы выродились)"
        );
    }

    /// Достижимость за sRGB-стеной: цвет с хромой между стенами (вне sRGB,
    /// внутри P3) представим на 8-битной P3-решётке И ПЕРЕЖИВАЕТ квантование —
    /// перечитанный с решётки цвет остаётся за sRGB-стеной. Это ровно то,
    /// что этап 2 отдаст наружу.
    #[test]
    fn beyond_srgb_chroma_survives_the_p3_lattice() {
        use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
        // Зелёная срединная зона — максимальный разрыв стен.
        let (l, h) = (0.75, 145.0);
        let srgb_wall = crate::scale::max_chroma_bisect(l, h);
        let p3_wall = crate::scale::max_chroma_p3_bisect(l, h);
        assert!(
            p3_wall > srgb_wall * 1.1,
            "в зелёной зоне разрыв стен обязан быть ощутимым: {p3_wall} vs {srgb_wall}"
        );
        let c = (srgb_wall + p3_wall) / 2.0;
        let h_rad = h.to_radians();
        let lab = [l, c * h_rad.cos(), c * h_rad.sin()];
        let lin_p3 = xyz_to_p3_linear(srgb_to_xyz(oklab_to_srgb_linear(lab)));
        let bytes = p3_bytes_from_linear(lin_p3).expect("между стенами — внутри P3");
        // Перечитываем с решётки и меряем хрому честно (на отданном значении).
        let back = p3_linear_to_xyz(p3_linear_from_bytes(bytes));
        let back_lab = srgb_linear_to_oklab(crate::spaces::srgb::xyz_to_srgb(back));
        let back_c = (back_lab[1] * back_lab[1] + back_lab[2] * back_lab[2]).sqrt();
        assert!(
            back_c > srgb_wall,
            "квантование не должно ронять хрому обратно в sRGB: {back_c} <= {srgb_wall}"
        );
    }

    /// Байт-точный round-trip решётки: байты → css-строка → парс компонент →
    /// байты. Шаг 7 взаимно прост с 255 — решётка пробегает все классы вычетов.
    #[test]
    fn p3_lattice_css_round_trip_is_byte_exact() {
        for r in (0..=255).step_by(7) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let bytes = [r as u8, g as u8, b as u8];
                    let css = p3_css_from_bytes(bytes, None).expect("байты валидны");
                    let inner = css
                        .strip_prefix("color(display-p3 ")
                        .and_then(|s| s.strip_suffix(')'))
                        .expect("форма color(display-p3 ...)");
                    let parts: Vec<f64> = inner
                        .split_whitespace()
                        .map(|p| p.parse::<f64>().expect("компонента — число"))
                        .collect();
                    let parsed = [
                        (parts[0] * 255.0).round() as u8,
                        (parts[1] * 255.0).round() as u8,
                        (parts[2] * 255.0).round() as u8,
                    ];
                    assert_eq!(parsed, bytes, "byte round-trip сломан: {css}");
                }
            }
        }
    }

    /// Гард решётки: не-цвет (за гамутом P3) не квантуется молча.
    #[test]
    fn out_of_gamut_linear_is_an_error_not_a_clamp() {
        assert!(p3_bytes_from_linear([1.2, 0.5, 0.5]).is_err());
        assert!(p3_bytes_from_linear([-0.2, 0.5, 0.5]).is_err());
    }
}
