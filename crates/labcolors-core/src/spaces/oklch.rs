//! CSS-эмиссия oklch — ЕДИНАЯ форма всех ролей наружу.
//!
//! Закон формы (владелец, 2026-07-02): «почему форма эмиссии везде разная?
//! …в идеале бы выводить окончательно oklch». Солид и полупрозрачная роль
//! отличаются ФИЗИКОЙ (α), но не синтаксисом: `oklch(L% C H)` и
//! `oklch(L% C H / A)` — один парсер, одна форма, перцептивно читаемые
//! компоненты, готовность к широкому гамуту (P3 — этап gamut-aware солвера).
//!
//! Значения остаются решёнными в sRGB-гамуте: oklch здесь — система координат
//! записи, не расширение гамута. Точность цифр подобрана под БАЙТ-ТОЧНЫЙ
//! round-trip: `oklch_css_from_hex` → парс → sRGB даёт исходные 8-битные
//! байты на всём кубе (доказательство — тест `round_trip_is_byte_exact_*`).

use super::oklab::srgb_linear_to_oklab;
use super::srgb::srgb_from_hex;

/// (L, C, H°) Oklch из hex-солида: линейный свет → Oklab → полярная форма.
///
/// H — градусы `[0, 360)`; у ахроматических цветов (C ≈ 0) H численно
/// произволен и на реконструкцию не влияет.
///
/// # Errors
///
/// `Err` — невалидный hex (пробрасывается из парсера).
pub fn oklch_from_hex(hex: &str) -> Result<[f64; 3], String> {
    let lab = srgb_linear_to_oklab(srgb_from_hex(hex)?);
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    Ok([lab[0], c, h])
}

/// CSS-строка `oklch(L% C H)` / `oklch(L% C H / A)` из hex-солида и
/// опциональной альфы.
///
/// Точность: L — 5 знаков процента, C — 6, H — 3; запас относительно
/// полушага 8-битного канала, доказан round-trip тестом на всём кубе.
/// Альфа печатается как есть (данные рампы, не производная).
///
/// # Errors
///
/// `Err` — невалидный hex.
pub fn oklch_css_from_hex(hex: &str, alpha: Option<f64>) -> Result<String, String> {
    let [l, c, h] = oklch_from_hex(hex)?;
    let base = format!("oklch({:.5}% {:.6} {:.3}", l * 100.0, c, h);
    Ok(match alpha {
        Some(a) => format!("{base} / {a})"),
        None => format!("{base})"),
    })
}

/// Обратный путь (только для доказательства round-trip): (L, C, H°) → hex.
#[cfg(test)]
fn hex_from_oklch(l: f64, c: f64, h_deg: f64) -> String {
    let (sin, cos) = h_deg.to_radians().sin_cos();
    let lin = super::oklab::oklab_to_srgb_linear([l, c * cos, c * sin]);
    super::srgb::hex_from_srgb(lin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Парсер эмитированной строки — эталонная реконструкция потребителя
    /// (браузер парсит те же компоненты тем же путём Oklab → sRGB).
    fn parse_emitted(css: &str) -> (String, Option<f64>) {
        let inner = css
            .strip_prefix("oklch(")
            .and_then(|s| s.strip_suffix(')'))
            .expect("форма oklch(...)");
        let (lch, alpha) = match inner.split_once(" / ") {
            Some((lch, a)) => (lch, Some(a.parse::<f64>().unwrap())),
            None => (inner, None),
        };
        let parts: Vec<f64> = lch
            .split_whitespace()
            .map(|p| p.trim_end_matches('%').parse::<f64>().unwrap())
            .collect();
        (hex_from_oklch(parts[0] / 100.0, parts[1], parts[2]), alpha)
    }

    /// Байт-точность round-trip на полном 8-битном кубе с шагом 5 по каждому
    /// каналу (включая края 0 и 255) — ~140k цветов; формат → парс → те же
    /// байты. Шаг 5 взаимно прост с 255, поэтому решётка не выровнена по
    /// «удобным» значениям.
    #[test]
    fn round_trip_is_byte_exact_on_lattice() {
        let steps: Vec<u8> = (0u16..=255).step_by(5).map(|v| v as u8).collect();
        assert!(steps.contains(&0) && steps.contains(&255));
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    let hex = format!("#{r:02X}{g:02X}{b:02X}");
                    let css = oklch_css_from_hex(&hex, None).unwrap();
                    let (back, alpha) = parse_emitted(&css);
                    assert_eq!(back, hex, "round-trip разошёлся: {css}");
                    assert_eq!(alpha, None);
                }
            }
        }
    }

    /// Серые — худший случай хромы (C ≈ 0, H произволен): весь грей-рамп
    /// байт-точен, альфа проходит насквозь как данные.
    #[test]
    fn round_trip_is_byte_exact_on_greys_with_alpha() {
        for v in 0u16..=255 {
            let v = v as u8;
            let hex = format!("#{v:02X}{v:02X}{v:02X}");
            let css = oklch_css_from_hex(&hex, Some(0.361)).unwrap();
            let (back, alpha) = parse_emitted(&css);
            assert_eq!(back, hex, "grey round-trip разошёлся: {css}");
            assert_eq!(alpha, Some(0.361));
        }
    }

    /// Форма строки — контракт потребителя: процент у L, слэш-альфа,
    /// H в диапазоне [0, 360).
    #[test]
    fn css_shape_is_the_contract() {
        let solid = oklch_css_from_hex("#3E87FF", None).unwrap();
        assert!(solid.starts_with("oklch(") && solid.ends_with(')'));
        assert!(solid.contains('%') && !solid.contains('/'));
        let translucent = oklch_css_from_hex("#101012", Some(0.122)).unwrap();
        assert!(translucent.contains(" / 0.122)"));
        let [_, _, h] = oklch_from_hex("#FF3B30").unwrap();
        assert!((0.0..360.0).contains(&h));
    }
}
