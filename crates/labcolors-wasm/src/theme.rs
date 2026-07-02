//! Тематический словарь границы — реэкспорт КАНОНИЧЕСКОГО [`Theme`] ядра.
//!
//! Раньше здесь жила вторая копия enum-а (31 ссылка) — два словаря одного
//! понятия расходились бы молча. Канон один, в ядре
//! (`labcolors_core::Theme`): kebab-контракт (`"light"` / `"dark"` /
//! `"light-ic"` / `"dark-ic"`), ключи и карта условий просмотра живут на нём.
//! Граница добавляет ТОЛЬКО свой тип ошибки: неизвестная тема — ошибка
//! вызывающего, оборачивается в [`BindingError::UnknownTheme`], никогда не
//! коэрсится в тему по умолчанию.
//!
//! «dim surround» — внутренний термин CIECAM16 для тёмной темы и наружу не
//! утекает: граница говорит темами, ядро — [`ViewingConditions`]
//! (labcolors_core::ViewingConditions).

pub use labcolors_core::Theme;

use crate::error::BindingError;

/// Разобрать kebab-строку границы в тему, с границевой ошибкой.
pub fn parse_theme(raw: &str) -> Result<Theme, BindingError> {
    Theme::parse(raw).map_err(|requested| BindingError::UnknownTheme { requested })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_public_spelling() {
        assert_eq!(parse_theme("light").unwrap(), Theme::Light);
        assert_eq!(parse_theme("dark").unwrap(), Theme::Dark);
        assert_eq!(parse_theme("light-ic").unwrap(), Theme::LightIc);
        assert_eq!(parse_theme("dark-ic").unwrap(), Theme::DarkIc);
    }

    #[test]
    fn rejects_unknown_theme_with_reason() {
        match parse_theme("solarized") {
            Err(BindingError::UnknownTheme { requested }) => assert_eq!(requested, "solarized"),
            other => panic!("expected UnknownTheme, got {other:?}"),
        }
    }

    #[test]
    fn key_round_trips_through_parse() {
        for theme in [Theme::Light, Theme::Dark, Theme::LightIc, Theme::DarkIc] {
            assert_eq!(parse_theme(theme.key()).unwrap(), theme);
        }
    }

    #[test]
    fn light_and_dark_map_to_distinct_viewing_conditions() {
        let light = Theme::Light.viewing_conditions();
        let dark = Theme::Dark.viewing_conditions();
        assert!(
            dark.aw < light.aw,
            "dim surround lowers the achromatic response"
        );
    }

    #[test]
    fn increased_contrast_themes_are_fully_calibrated() {
        for theme in [Theme::LightIc, Theme::DarkIc] {
            assert!(theme.viewing_conditions().high_contrast);
        }
    }
}
