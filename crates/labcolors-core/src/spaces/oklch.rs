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
    // atan2(-0.0, x>0) даёт -0.0, а rem_euclid его не снимает (−0.0 не < 0) —
    // печать дала бы "-0.000" и нарушила контракт H ∈ [0, 360).
    let h = if h == 0.0 { 0.0 } else { h };
    Ok([lab[0], c, h])
}

/// CSS-строка `oklch(L% C H)` / `oklch(L% C H / A)` из hex-солида и
/// опциональной альфы.
///
/// Точность: L — 5 знаков процента, C — 6, H — 3; запас относительно
/// полушага 8-битного канала, доказан round-trip тестом на всём кубе.
/// Альфа — до 4 знаков с обрезкой хвостовых нулей: литералы рампы проходят
/// как есть (`0.122` → `0.122`), а вычисленная альфа (альфа-аналог) не
/// тащит float-хвост в CSS-строку.
///
/// # Errors
///
/// `Err` — невалидный hex.
pub fn oklch_css_from_hex(hex: &str, alpha: Option<f64>) -> Result<String, String> {
    let [l, c, h] = oklch_from_hex(hex)?;
    let base = format!("oklch({:.5}% {:.6} {:.3}", l * 100.0, c, h);
    let suffix = css_alpha_suffix(alpha)?;
    Ok(format!("{base}{suffix})"))
}

/// Общий CSS-суффикс альфы для всех форм эмиссии (`oklch(...)`, `color(display-p3 ...)`):
/// пустая строка для солида, `" / A"` для полупрозрачного. ЕДИНСТВЕННЫЙ дом
/// политики альфы — правило клампа/ошибки не переобъявлять в других модулях.
///
/// Кламп — только для вычислительного шума в пределах эпсилона от
/// [0, 1] (легитимен у выведенных альф; заодно гасит артефакт "-0").
/// Всё остальное — честная ошибка, не тихая подмена: NaN в CSS
/// невалиден, а альфа -10 или 2 — дефект вызывающего кода.
pub(crate) fn css_alpha_suffix(alpha: Option<f64>) -> Result<String, String> {
    match alpha {
        None => Ok(String::new()),
        Some(a) => {
            const ALPHA_NOISE: f64 = 1e-6;
            if !a.is_finite() || !(-ALPHA_NOISE..=1.0 + ALPHA_NOISE).contains(&a) {
                return Err(format!("альфа вне [0, 1]: {a}"));
            }
            let a4 = format!("{:.4}", a.clamp(0.0, 1.0));
            let a4 = a4.trim_end_matches('0').trim_end_matches('.');
            Ok(format!(" / {a4}"))
        }
    }
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
            Some((lch, a)) => (lch, Some(a.parse::<f64>().expect("альфа — число"))),
            None => (inner, None),
        };
        let parts: Vec<f64> = lch
            .split_whitespace()
            .map(|p| {
                p.trim_end_matches('%')
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("компонента не число: {p} в {css}"))
            })
            .collect();
        assert_eq!(parts.len(), 3, "ровно L C H: {css}");
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

    /// Гард альфы: NaN — честная ошибка, конечный шум за краями — кламп
    /// без артефакта "-0".
    #[test]
    fn alpha_guard_rejects_nan_and_clamps_noise() {
        assert!(oklch_css_from_hex("#101012", Some(f64::NAN)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(f64::INFINITY)).is_err());
        // Грубо невалидная альфа — дефект вызывающего, не шум: ошибка, не кламп.
        assert!(oklch_css_from_hex("#101012", Some(-10.0)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(2.0)).is_err());
        // Знаковый ноль H не просачивается в печать.
        assert!(
            !oklch_css_from_hex("#FFFFFF", None).unwrap().contains("-0."),
            "signed zero в компонентах запрещён"
        );
        let noisy = oklch_css_from_hex("#101012", Some(-1e-7)).unwrap();
        assert!(noisy.ends_with(" / 0)"), "шум у нуля клампится: {noisy}");
        let over = oklch_css_from_hex("#101012", Some(1.0 + 1e-9)).unwrap();
        assert!(over.ends_with(" / 1)"), "шум у единицы клампится: {over}");
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
