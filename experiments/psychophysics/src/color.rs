//! Цвет: sRGB ↔ линейный свет, WCAG-люминанс Y, контраст и генерация свотча на
//! ЗАДАННЫЙ Y с сохранением оттенка семьи.
//!
//! Всё — публичные стандарты (IEC 61966-2-1 sRGB, WCAG 2 relative luminance),
//! никакой зависимости от `labcolors-core`: харнесс калибрует константу ядра, и
//! стимулы обязаны строиться независимой, самопроверяемой математикой, иначе
//! эксперимент был бы циркулярным. Формула Y байт-совпадает с
//! `labcolors_core::exposure_support::wcag_y` (Rec.709 на гамма-декодированных
//! 8-битных каналах) — именно против неё ядро сравнивает `PAIR_CROSSOVER_Y`.

/// Веса Rec.709 для относительной люминанс.
const WR: f64 = 0.2126;
const WG: f64 = 0.7152;
const WB: f64 = 0.0722;

/// Линеаризация одного sRGB-канала (encoded `[0,1]` → linear).
#[must_use]
pub fn gamma_inv(e: f64) -> f64 {
    if e <= 0.040_45 {
        e / 12.92
    } else {
        ((e + 0.055) / 1.055).powf(2.4)
    }
}

/// Гамма-кодирование одного канала (linear → encoded `[0,1]`).
#[must_use]
pub fn gamma_fwd(l: f64) -> f64 {
    if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Разобрать `#RRGGBB` (регистр любой) в 8-битные каналы.
///
/// # Errors
/// `Err`, если строка не `#` + 6 hex-цифр.
pub fn hex_to_rgb(hex: &str) -> Result<[u8; 3], String> {
    let s = hex.strip_prefix('#').unwrap_or(hex);
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("некорректный hex '{hex}'"));
    }
    let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())?;
    Ok([r, g, b])
}

/// 8-битные каналы → `#RRGGBB` (верхний регистр).
#[must_use]
pub fn rgb_to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Линейные компоненты (свет) из 8-битного цвета.
#[must_use]
pub fn linear_of(rgb: [u8; 3]) -> [f64; 3] {
    [
        gamma_inv(f64::from(rgb[0]) / 255.0),
        gamma_inv(f64::from(rgb[1]) / 255.0),
        gamma_inv(f64::from(rgb[2]) / 255.0),
    ]
}

/// WCAG относительная люминанс Y 8-битного цвета (Rec.709).
///
/// Совпадает с `labcolors_core::exposure_support::wcag_y`.
#[must_use]
pub fn wcag_y(rgb: [u8; 3]) -> f64 {
    let l = linear_of(rgb);
    WR * l[0] + WG * l[1] + WB * l[2]
}

/// Люминанс линейного триплета.
#[must_use]
pub fn luminance_lin(l: [f64; 3]) -> f64 {
    WR * l[0] + WG * l[1] + WB * l[2]
}

/// Контраст WCAG между двумя 8-битными цветами (симметрично, `1.0..=21.0`).
#[must_use]
pub fn wcag_contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let ya = wcag_y(a);
    let yb = wcag_y(b);
    let (hi, lo) = if ya >= yb { (ya, yb) } else { (yb, ya) };
    (hi + 0.05) / (lo + 0.05)
}

/// Свотч заданной люминанс, сохраняющий оттенок семьи, максимально хроматичный
/// в пределах гаммы sRGB.
///
/// Метод (всё в линейном свете, где Y аддитивна):
/// пусть `d` — линейный триплет якоря семьи, `Y(d)>0`. Вектор
/// `v = d/Y(d) − (1,1,1)` имеет НУЛЕВУЮ люминанс, поэтому
/// `c(t) = Y_t·((1,1,1) + t·v)` держит люминанс ровно `Y_t` при любом `t`, двигаясь
/// по прямой постоянного оттенка «серый → чистый оттенок» в линейном RGB. Берём
/// `t = chroma_frac · t_max`, где `t_max` — предел гаммы (канал упирается в `[0,1]`),
/// затем кодируем в sRGB и квантуем в 8 бит. Возвращаем `(hex, measured_y)`, где
/// `measured_y` пересчитана из КВАНТОВАННОГО hex — это истинная люминанс стимула
/// (округление до 8 бит слегка сдвигает `Y_t`, и анализ обязан использовать
/// показанное значение, а не номинал сетки).
///
/// `chroma_frac ∈ (0,1]`: 1.0 — впритык к ребру гаммы; меньшее держит запас от
/// численных краёв. `target_y` осмысленно в `(0,1)`.
#[must_use]
pub fn swatch_at_luminance(anchor: [u8; 3], target_y: f64, chroma_frac: f64) -> (String, f64) {
    let d = linear_of(anchor);
    let yd = luminance_lin(d);
    let yt = target_y.clamp(1e-6, 1.0 - 1e-9);
    let frac = chroma_frac.clamp(0.0, 1.0);

    // Ахроматичный якорь (или чёрный) → серый на нужной люминанс.
    if yd <= 1e-9 {
        return quantise([yt, yt, yt]);
    }

    // Вектор нулевой люминанс, задающий оттенок.
    let v = [d[0] / yd - 1.0, d[1] / yd - 1.0, d[2] / yd - 1.0];

    // Почти-ахроматический якорь: `v` тонет в плавучем шуме (для истинно серого
    // `d[i]/yd` = 1 лишь с точностью до ~1e-16, а веса Rec.709 суммируются не
    // ровно в 1.0). Тогда `−1/vc` раздувается до ~1e16 и `t·v` губит люминанс,
    // оставляя каналы равными (серый, но не на нужном Y). Порог 1e-6 надёжно выше
    // шума и ниже хромы любого реального оттенка — закрывает класс.
    let vmax = v[0].abs().max(v[1].abs()).max(v[2].abs());
    if vmax < 1e-6 {
        return quantise([yt, yt, yt]);
    }

    // Предел гаммы: c_c(t) = yt·(1 + t·v_c) ∈ [0,1] для каждого канала c.
    let mut t_max = f64::INFINITY;
    for &vc in &v {
        if vc > 0.0 {
            // Верхняя граница: 1 + t·vc ≤ 1/yt.
            t_max = t_max.min((1.0 / yt - 1.0) / vc);
        } else if vc < 0.0 {
            // Нижняя граница канала: 1 + t·vc ≥ 0.
            t_max = t_max.min(-1.0 / vc);
        }
    }
    if !t_max.is_finite() || t_max < 0.0 {
        t_max = 0.0;
    }

    let t = frac * t_max;
    let lin = [
        yt * (1.0 + t * v[0]),
        yt * (1.0 + t * v[1]),
        yt * (1.0 + t * v[2]),
    ];
    quantise(lin)
}

/// Кодировать линейный триплет в sRGB, квантовать в 8 бит, вернуть hex и его Y.
fn quantise(lin: [f64; 3]) -> (String, f64) {
    let enc = [
        gamma_fwd(lin[0].clamp(0.0, 1.0)),
        gamma_fwd(lin[1].clamp(0.0, 1.0)),
        gamma_fwd(lin[2].clamp(0.0, 1.0)),
    ];
    let rgb = [
        (enc[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (enc[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (enc[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    ];
    (rgb_to_hex(rgb), wcag_y(rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_roundtrips() {
        for i in 0..=255 {
            let e = f64::from(i) / 255.0;
            let back = gamma_fwd(gamma_inv(e));
            assert!((back - e).abs() < 1e-9, "e={e} back={back}");
        }
    }

    #[test]
    fn luminance_extremes() {
        assert!((wcag_y([255, 255, 255]) - 1.0).abs() < 1e-9);
        assert!(wcag_y([0, 0, 0]).abs() < 1e-12);
    }

    #[test]
    fn contrast_white_black_is_21() {
        assert!((wcag_contrast([255, 255, 255], [0, 0, 0]) - 21.0).abs() < 1e-6);
    }

    #[test]
    fn hex_roundtrip() {
        for hex in ["#FF3B30", "#007AFF", "#101012", "#FFFFFF"] {
            let rgb = hex_to_rgb(hex).unwrap();
            assert_eq!(rgb_to_hex(rgb), hex);
        }
    }

    #[test]
    fn hex_rejects_malformed() {
        assert!(hex_to_rgb("#12345").is_err());
        assert!(hex_to_rgb("#GGGGGG").is_err());
        assert!(hex_to_rgb("007AFF").is_ok()); // без '#' допускаем
    }

    #[test]
    fn swatch_hits_target_luminance_within_quantisation() {
        // Для каждой семьи и каждого узла сетки measured_y близко к номиналу.
        let anchors = [[255, 59, 48], [0, 122, 255], [52, 199, 89], [191, 90, 242]];
        let mut y = 0.18;
        while y <= 0.45 + 1e-9 {
            for a in anchors {
                let (_hex, my) = swatch_at_luminance(a, y, 0.9);
                // 8-битное квантование сдвигает Y максимум на ~соседний код;
                // 0.01 — щедрый, но надёжный порог на всём диапазоне.
                assert!(
                    (my - y).abs() < 0.01,
                    "anchor={a:?} target={y} measured={my}"
                );
            }
            y += 0.02;
        }
    }

    #[test]
    fn swatch_is_in_gamut_and_preserves_hue_family() {
        // Оттенок семьи: доминирующий линейный канал якоря остаётся доминирующим.
        let anchor = [0, 122, 255]; // синий: доминирует B
        let (hex, _my) = swatch_at_luminance(anchor, 0.30, 0.9);
        let rgb = hex_to_rgb(&hex).unwrap();
        let lin = linear_of(rgb);
        let argmax = |l: [f64; 3]| {
            if l[0] >= l[1] && l[0] >= l[2] {
                0
            } else if l[1] >= l[2] {
                1
            } else {
                2
            }
        };
        assert_eq!(argmax(lin), 2, "синий свотч должен доминировать B: {hex}");
    }

    #[test]
    fn achromatic_anchor_gives_gray() {
        let (hex, my) = swatch_at_luminance([128, 128, 128], 0.25, 0.9);
        let rgb = hex_to_rgb(&hex).unwrap();
        assert_eq!(rgb[0], rgb[1]);
        assert_eq!(rgb[1], rgb[2]);
        assert!((my - 0.25).abs() < 0.01);
    }

    #[test]
    fn black_anchor_falls_back_to_gray() {
        let (hex, _my) = swatch_at_luminance([0, 0, 0], 0.3, 0.9);
        let rgb = hex_to_rgb(&hex).unwrap();
        assert_eq!(rgb[0], rgb[1]);
        assert_eq!(rgb[1], rgb[2]);
    }
}
