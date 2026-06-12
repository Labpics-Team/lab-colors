//! The public theme vocabulary and its mapping to core viewing conditions.
//!
//! The owner's HIG naming (2026-06-12) is the contract the web sees:
//! `Light` / `Dark` / `Light-IC` / `Dark-IC`. "dim surround" is the *internal*
//! CIECAM16 term for the dark theme's viewing conditions and never leaks out —
//! the boundary speaks themes, the core speaks [`ViewingConditions`].
//!
//! The `-IC` ("increased contrast") themes are reserved in the type so the
//! public surface is complete from day one, but they are honestly *not yet
//! calibrated*: asking for one is a real error with a real reason, not a
//! silent substitution of a Light/Dark result. That keeps the type correct
//! while the implementation is honestly absent.

use labcolors_core::ViewingConditions;

use crate::error::BindingError;

/// A theme the engine can resolve a background against.
///
/// Parsed from the stable lowercase-kebab string contract at the boundary
/// (`"light"`, `"dark"`, `"light-ic"`, `"dark-ic"`); the increased-contrast
/// variants are part of the contract but resolve to a calibration error until
/// the IC tables land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// Light theme — sRGB average-surround viewing conditions.
    Light,
    /// Dark theme — dim-surround viewing conditions internally.
    Dark,
    /// Increased-contrast light theme. Reserved; not yet calibrated.
    LightIncreasedContrast,
    /// Increased-contrast dark theme. Reserved; not yet calibrated.
    DarkIncreasedContrast,
}

impl Theme {
    /// Parse the stable string contract into a theme.
    ///
    /// The accepted spellings are the public contract; an unknown string is a
    /// caller error, surfaced — never coerced to a default theme.
    pub fn parse(raw: &str) -> Result<Self, BindingError> {
        match raw {
            "light" => Ok(Theme::Light),
            "dark" => Ok(Theme::Dark),
            "light-ic" => Ok(Theme::LightIncreasedContrast),
            "dark-ic" => Ok(Theme::DarkIncreasedContrast),
            other => Err(BindingError::UnknownTheme {
                requested: other.to_owned(),
            }),
        }
    }

    /// The stable string key for this theme — the inverse of [`parse`](Self::parse).
    pub fn key(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::LightIncreasedContrast => "light-ic",
            Theme::DarkIncreasedContrast => "dark-ic",
        }
    }

    /// The viewing conditions the core resolves under for this theme.
    ///
    /// Light → `srgb` (average surround); Dark → `dim_surround` (the internal
    /// term). The `-IC` themes have no calibrated mapping yet, so this returns
    /// [`BindingError::ThemeNotCalibrated`] rather than aliasing a Light/Dark VC.
    pub fn viewing_conditions(self) -> Result<ViewingConditions, BindingError> {
        match self {
            Theme::Light => Ok(ViewingConditions::srgb()),
            Theme::Dark => Ok(ViewingConditions::dim_surround()),
            Theme::LightIncreasedContrast | Theme::DarkIncreasedContrast => {
                Err(BindingError::ThemeNotCalibrated { theme: self.key() })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_public_spelling() {
        assert_eq!(Theme::parse("light").unwrap(), Theme::Light);
        assert_eq!(Theme::parse("dark").unwrap(), Theme::Dark);
        assert_eq!(
            Theme::parse("light-ic").unwrap(),
            Theme::LightIncreasedContrast
        );
        assert_eq!(
            Theme::parse("dark-ic").unwrap(),
            Theme::DarkIncreasedContrast
        );
    }

    #[test]
    fn rejects_unknown_theme_with_reason() {
        match Theme::parse("solarized") {
            Err(BindingError::UnknownTheme { requested }) => assert_eq!(requested, "solarized"),
            other => panic!("expected UnknownTheme, got {other:?}"),
        }
    }

    #[test]
    fn key_round_trips_through_parse() {
        for theme in [
            Theme::Light,
            Theme::Dark,
            Theme::LightIncreasedContrast,
            Theme::DarkIncreasedContrast,
        ] {
            assert_eq!(Theme::parse(theme.key()).unwrap(), theme);
        }
    }

    #[test]
    fn light_and_dark_map_to_distinct_viewing_conditions() {
        let light = Theme::Light.viewing_conditions().unwrap();
        let dark = Theme::Dark.viewing_conditions().unwrap();
        assert!(
            dark.aw < light.aw,
            "dim surround lowers the achromatic response"
        );
    }

    #[test]
    fn increased_contrast_themes_are_honestly_uncalibrated() {
        for theme in [Theme::LightIncreasedContrast, Theme::DarkIncreasedContrast] {
            match theme.viewing_conditions() {
                Err(BindingError::ThemeNotCalibrated { .. }) => {}
                other => panic!("expected ThemeNotCalibrated, got {other:?}"),
            }
        }
    }
}
