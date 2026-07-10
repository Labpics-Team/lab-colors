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
//! байты на проверяемой sRGB8-решётке и всей серой оси. Полный куб оставлен
//! отдельному длительному аудиту: CI не выдаёт выборку за исчерпывающий перебор.

use super::oklab::srgb_linear_to_oklab;
use super::srgb::srgb_from_hex;

/// Порог powerless-hue Oklch из CSS Color 4.
///
/// Это часть синтаксического контракта пространства, а не психофизический JND:
/// при `C ≤ ε` hue не влияет на цвет и после конвертации обязан стать missing.
pub const OKLCH_POWERLESS_CHROMA: f64 = 0.000_004;

/// Полярные координаты Oklch без выдуманного hue на ахроматической оси.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchCoordinates {
    pub l: f64,
    pub c: f64,
    pub h_deg: Option<f64>,
}

/// Oklch из hex-солида: линейный свет → Oklab → полярная форма.
///
/// Hue присутствует в градусах `[0, 360)` только при `C` выше нормативного
/// powerless-порога CSS Color 4. У ахромата возвращается `None`, потому что
/// числовой угол там не является свойством стимула.
///
/// # Errors
///
/// `Err` — невалидный hex (пробрасывается из парсера).
pub fn oklch_from_hex(hex: &str) -> Result<OklchCoordinates, String> {
    let lab = srgb_linear_to_oklab(srgb_from_hex(hex)?);
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h_deg = if c <= OKLCH_POWERLESS_CHROMA {
        None
    } else {
        let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
        // `atan2(-0, x>0)` сохраняет знаковый ноль; каноническая запись угла — +0.
        Some(if h == 0.0 { 0.0 } else { h })
    };
    Ok(OklchCoordinates {
        l: lab[0],
        c,
        h_deg,
    })
}

/// CSS-строка `oklch(L% C H)` / `oklch(L% C H / A)` из hex-солида и
/// опциональной альфы.
///
/// Точность: L — 5 знаков процента, C — 6, H — 3; запас относительно
/// полушага 8-битного канала, проверен round-trip на sRGB8-решётке и серой оси.
/// Альфа — до 4 знаков с обрезкой хвостовых нулей: литералы рампы проходят
/// как есть (`0.122` → `0.122`), а вычисленная альфа (альфа-аналог) не
/// тащит float-хвост в CSS-строку.
///
/// # Errors
///
/// `Err` — невалидный hex.
pub fn oklch_css_from_hex(hex: &str, alpha: Option<f64>) -> Result<String, String> {
    let OklchCoordinates { l, c, h_deg } = oklch_from_hex(hex)?;
    let hue = h_deg
        .map(|h| format!("{h:.3}"))
        .unwrap_or_else(|| "none".to_string());
    let base = format!("oklch({:.5}% {:.6} {hue}", l * 100.0, c);
    let suffix = css_alpha_suffix(alpha)?;
    Ok(format!("{base}{suffix})"))
}

/// Общий CSS-суффикс альфы для всех форм эмиссии (`oklch(...)`, `color(display-p3 ...)`):
/// пустая строка для солида, `" / A"` для полупрозрачного. ЕДИНСТВЕННЫЙ дом
/// политики альфы — правило клампа/ошибки не переобъявлять в других модулях.
///
/// Любое значение вне `[0, 1]` отвергается: у универсального эмиттера нет
/// доказанной границы «допустимого вычислительного шума». Знаковый `−0`
/// нормализуется отдельной точной веткой, не числовым допуском.
pub(crate) fn css_alpha_suffix(alpha: Option<f64>) -> Result<String, String> {
    match alpha {
        None => Ok(String::new()),
        Some(a) => {
            if !a.is_finite() || !(0.0..=1.0).contains(&a) {
                return Err(format!("альфа вне [0, 1]: {a}"));
            }
            let canonical = if a == 0.0 { 0.0 } else { a };
            let a4 = format!("{canonical:.4}");
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
        let parts: Vec<&str> = lch.split_whitespace().collect();
        assert_eq!(parts.len(), 3, "ровно L C H: {css}");
        let l = parts[0]
            .trim_end_matches('%')
            .parse::<f64>()
            .expect("L — число");
        let c = parts[1].parse::<f64>().expect("C — число");
        let h = if parts[2] == "none" {
            0.0
        } else {
            parts[2].parse::<f64>().expect("H — число либо none")
        };
        (hex_from_oklch(l / 100.0, c, h), alpha)
    }

    /// Байт-точность round-trip на 8-битной решётке с шагом 5 по каждому
    /// каналу (включая края 0 и 255) — ~140k цветов; формат → парс → те же
    /// байты. Шаг 5 взаимно прост с 255, поэтому выборка не выровнена по
    /// «удобным» значениям. Это плотная регрессия, а не полный перебор куба.
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

    /// Гард альфы: нечисловые и внедоменные значения — честная ошибка;
    /// знаковый ноль канонизируется без числового допуска.
    #[test]
    fn alpha_guard_rejects_every_out_of_domain_value() {
        assert!(oklch_css_from_hex("#101012", Some(f64::NAN)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(f64::INFINITY)).is_err());
        // Размер выхода за границу не меняет контракт: недоказанного epsilon нет.
        assert!(oklch_css_from_hex("#101012", Some(-10.0)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(2.0)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(-1e-7)).is_err());
        assert!(oklch_css_from_hex("#101012", Some(1.0 + 1e-9)).is_err());
        // Знаковый ноль H не просачивается в печать.
        assert!(
            !oklch_css_from_hex("#FFFFFF", None).unwrap().contains("-0."),
            "signed zero в компонентах запрещён"
        );
        let negative_zero = oklch_css_from_hex("#101012", Some(-0.0)).unwrap();
        assert!(
            negative_zero.ends_with(" / 0)"),
            "знаковый ноль канонизируется: {negative_zero}"
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
        let h = oklch_from_hex("#FF3B30").unwrap().h_deg.unwrap();
        assert!((0.0..360.0).contains(&h));
    }

    #[test]
    fn every_encoded_gray_has_missing_hue() {
        for value in 0_u8..=u8::MAX {
            let hex = format!("#{value:02X}{value:02X}{value:02X}");
            let coordinates = oklch_from_hex(&hex).unwrap();
            assert_eq!(coordinates.h_deg, None, "{hex}");
            assert!(
                oklch_css_from_hex(&hex, None).unwrap().contains(" none)"),
                "{hex} обязан сериализовать powerless hue как none"
            );
        }
    }
}
