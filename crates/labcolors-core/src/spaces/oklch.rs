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
//! round-trip: `oklch_css_from_hex` → независимый парс и обратные матрицы →
//! sRGB8. Полный release-гейт `oklch_full_cube` перечисляет все 16 777 216
//! входов; CI запускает его явно в фактической Linux-среде с закреплённым Rust.
//! PASS относится к target, toolchain и системной `libm` конкретного запуска, а
//! не объявляет переносимость без проверки. Быстрый debug-тест остаётся smoke.

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
/// Точность: L — 5 знаков процента, C — 6, H — 3. Байт-точность для конкретных
/// target, toolchain и системной `libm` проверяет полное перечисление в release-тесте
/// `oklch_full_cube`; CI вызывает его с `--ignored`, а debug-решётка служит
/// быстрым smoke-тестом.
/// Альфа печатается кратчайшей десятичной записью, которая восстанавливает
/// исходный `f64`. Вычисленная прозрачность участвует в сертификате композита,
/// поэтому произвольное число знаков после запятой здесь недопустимо.
///
/// # Errors
///
/// `Err` — невалидный hex, не-конечная альфа либо альфа вне `[0, 1]`.
pub fn oklch_css_from_hex(hex: &str, alpha: Option<f64>) -> Result<String, String> {
    let [l, c, h] = oklch_from_hex(hex)?;
    let base = format!("oklch({:.5}% {:.6} {:.3}", l * 100.0, c, h);
    let suffix = css_alpha_suffix(alpha)?;
    Ok(format!("{base}{suffix})"))
}

/// Общий CSS-суффикс альфы для всех форм эмиссии (`oklch(...)`, `color(display-p3 ...)`):
/// пустая строка для солида, `" / A"` для полупрозрачного. ЕДИНСТВЕННЫЙ дом
/// политики альфы — строгую границу не переобъявлять в других модулях.
///
/// Суффикс не клампит даже малое отклонение: вычисленная альфа участвует в
/// сертификате композита, поэтому значение за `[0, 1]` означает дефект
/// вызывающего кода, а не допустимый шум. Знаковый ноль канонизируется в
/// [`css_alpha_value`], не расширяя входной домен эпсилоном.
pub(crate) fn css_alpha_suffix(alpha: Option<f64>) -> Result<String, String> {
    match alpha {
        None => Ok(String::new()),
        Some(a) => Ok(format!(" / {}", css_alpha_value(a)?)),
    }
}

/// Каноническая CSS-запись альфы без потери binary64-точности.
///
/// Сериализатор не исправляет значения вызывающего: даже малое отклонение за
/// `[0, 1]` означает, что сертификат был построен в другом домене. Знаковый
/// ноль нормализуется, потому что `-0` и `0` задают одну прозрачность, но первая
/// форма создаёт ненужный платформенный дрейф строк.
///
/// # Errors
///
/// `Err`, если значение не конечно или лежит вне `[0, 1]`.
pub fn css_alpha_value(alpha: f64) -> Result<String, String> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("альфа вне [0, 1]: {alpha}"));
    }
    let canonical = if alpha == 0.0 { 0.0 } else { alpha };
    Ok(canonical.to_string())
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

    /// Независимая test-транскрипция CSS Color 4 Oklab → sRGB. Она проверяет
    /// численный контракт эмиттера; паритет конкретного браузера — отдельный
    /// WPT/headless gate.
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

    /// Вычисленная альфа является частью сертификата композита: CSS-строка
    /// обязана восстанавливать то же binary64-значение, а не соседнюю точку.
    #[test]
    fn computed_alpha_round_trips_without_precision_loss() {
        let alpha = 0.036_045_459_685_627_89_f64;
        let css = oklch_css_from_hex("#4A8FFF", Some(alpha)).unwrap();
        let (_, parsed) = parse_emitted(&css);
        assert_eq!(
            parsed.unwrap().to_bits(),
            alpha.to_bits(),
            "CSS потерял точность вычисленной альфы: {css}"
        );
    }

    /// Граница альфы не маскирует ошибку вычисления клампом; знаковый ноль
    /// канонизируется без изменения значения прозрачности.
    #[test]
    fn alpha_guard_rejects_every_out_of_domain_value() {
        assert!(oklch_css_from_hex("#101012", Some(f64::NAN)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(f64::INFINITY)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(f64::NEG_INFINITY)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(-10.0)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(2.0)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(-f64::EPSILON)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(1.0 + f64::EPSILON)).is_err());
        // Знаковый ноль H не просачивается в печать.
        assert!(
            !oklch_css_from_hex("#FFFFFF", None).unwrap().contains("-0."),
            "signed zero в компонентах запрещён"
        );
        let zero = oklch_css_from_hex("#101012", Some(-0.0)).unwrap();
        assert!(
            zero.ends_with(" / 0)"),
            "знаковый ноль нормализован: {zero}"
        );
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
