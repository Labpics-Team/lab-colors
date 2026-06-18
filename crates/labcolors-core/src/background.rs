//! Theme background roles: the surfaces a UI is resolved *on*, not foreground
//! roles resolved *against* a surface.
//!
//! `semantic::resolve_set(bg, ...)` answers "what do I draw on this background?".
//! This module answers the layer below it: "which background hex should this part
//! of the theme use?". Keeping the two separate prevents background tokens from
//! leaking into the foreground role set and invalidating the contrast golden.

use crate::solve::{BgInput, Unreachable};

/// Light or dark colour scheme. Static/adaptive is intentionally not here: that is
/// a runtime inheritance decision. The core receives an already-resolved scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorScheme {
    Light,
    Dark,
}

impl ColorScheme {
    pub fn inverted(self) -> Self {
        match self {
            ColorScheme::Light => ColorScheme::Dark,
            ColorScheme::Dark => ColorScheme::Light,
        }
    }
}

/// Contrast axis. Increased contrast is represented independently from scheme so
/// callers can express Light+IC, Dark+IC, Static+IC, and inverted+IC without a
/// combinatorial enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContrastMode {
    Standard,
    Increased,
}

/// A resolved theme context for the core.
///
/// The web/runtime layer may decide whether a request is adaptive or static; once
/// that inheritance is resolved, the core only needs scheme + contrast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThemeContext {
    scheme: ColorScheme,
    contrast: ContrastMode,
}

impl ThemeContext {
    pub const fn new(scheme: ColorScheme, contrast: ContrastMode) -> Self {
        Self { scheme, contrast }
    }

    pub const fn light() -> Self {
        Self::new(ColorScheme::Light, ContrastMode::Standard)
    }

    pub const fn dark() -> Self {
        Self::new(ColorScheme::Dark, ContrastMode::Standard)
    }

    pub const fn light_increased_contrast() -> Self {
        Self::new(ColorScheme::Light, ContrastMode::Increased)
    }

    pub const fn dark_increased_contrast() -> Self {
        Self::new(ColorScheme::Dark, ContrastMode::Increased)
    }

    pub fn inverted(self) -> Self {
        Self {
            scheme: self.scheme.inverted(),
            contrast: self.contrast,
        }
    }

    pub fn scheme(self) -> ColorScheme {
        self.scheme
    }

    pub fn contrast(self) -> ContrastMode {
        self.contrast
    }
}

/// Neutral background roles from the Figma semantic collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BackgroundRole {
    Primary,
    Secondary,
    Tertiary,
    Inverted,
    GroupedPrimary,
    GroupedSecondary,
    GroupedTertiary,
}

impl BackgroundRole {
    pub const ALL: [BackgroundRole; 7] = [
        BackgroundRole::Primary,
        BackgroundRole::Secondary,
        BackgroundRole::Tertiary,
        BackgroundRole::Inverted,
        BackgroundRole::GroupedPrimary,
        BackgroundRole::GroupedSecondary,
        BackgroundRole::GroupedTertiary,
    ];

    /// CSS-variable stem without the `--lab-` prefix.
    pub fn key(self) -> &'static str {
        match self {
            BackgroundRole::Primary => "bg-primary",
            BackgroundRole::Secondary => "bg-secondary",
            BackgroundRole::Tertiary => "bg-tertiary",
            BackgroundRole::Inverted => "bg-inverted",
            BackgroundRole::GroupedPrimary => "bg-grouped-primary",
            BackgroundRole::GroupedSecondary => "bg-grouped-secondary",
            BackgroundRole::GroupedTertiary => "bg-grouped-tertiary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundError {
    IncreasedContrastNotCalibrated,
    InvalidBackgroundHex {
        role: BackgroundRole,
        reason: String,
    },
}

impl core::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BackgroundError::IncreasedContrastNotCalibrated => {
                write!(f, "increased-contrast backgrounds are not calibrated yet")
            }
            BackgroundError::InvalidBackgroundHex { role, reason } => {
                write!(f, "{} background is invalid: {reason}", role.key())
            }
        }
    }
}

/// One resolved background token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundEntry {
    pub role: BackgroundRole,
    pub key: &'static str,
    pub hex: &'static str,
}

/// Resolve a neutral background role to the exact Figma-calibrated hex.
///
/// These are exact theme surface tokens, not dJ' foreground contracts. The light
/// ladder intentionally alternates two surfaces; the dark ladder has three steps.
pub fn resolve_background(
    role: BackgroundRole,
    context: ThemeContext,
) -> Result<BackgroundEntry, BackgroundError> {
    if context.contrast == ContrastMode::Increased {
        return Err(BackgroundError::IncreasedContrastNotCalibrated);
    }

    let effective_context = if role == BackgroundRole::Inverted {
        context.inverted()
    } else {
        context
    };

    let hex = match effective_context.scheme {
        ColorScheme::Light => match role {
            BackgroundRole::Primary | BackgroundRole::Tertiary => "#FFFFFF",
            BackgroundRole::Secondary => "#F7F8FA",
            BackgroundRole::GroupedPrimary | BackgroundRole::GroupedTertiary => "#F7F8FA",
            BackgroundRole::GroupedSecondary => "#FFFFFF",
            // Inverted is resolved by flipping the context first, so this is the
            // primary of the opposite scheme.
            BackgroundRole::Inverted => "#FFFFFF",
        },
        ColorScheme::Dark => match role {
            BackgroundRole::Primary | BackgroundRole::GroupedPrimary => "#101012",
            BackgroundRole::Secondary | BackgroundRole::GroupedSecondary => "#1C1C1E",
            BackgroundRole::Tertiary | BackgroundRole::GroupedTertiary => "#242426",
            BackgroundRole::Inverted => "#101012",
        },
    };

    Ok(BackgroundEntry {
        role,
        key: role.key(),
        hex,
    })
}

pub fn resolve_background_set(
    context: ThemeContext,
) -> Result<Vec<BackgroundEntry>, BackgroundError> {
    BackgroundRole::ALL
        .iter()
        .map(|&role| resolve_background(role, context))
        .collect()
}

/// Build the `BgInput` a role can feed into `semantic::resolve_set`.
pub fn background_input(
    role: BackgroundRole,
    context: ThemeContext,
) -> Result<BgInput, BackgroundError> {
    let entry = resolve_background(role, context)?;
    BgInput::solid(entry.hex).map_err(
        |reason: Unreachable| BackgroundError::InvalidBackgroundHex {
            role,
            reason: reason.to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{Resolved, Role, RoleTable, resolve_set};
    use crate::spaces::vc::ViewingConditions;

    #[test]
    fn neutral_light_backgrounds_match_figma_exactly() {
        let set = resolve_background_set(ThemeContext::light()).unwrap();
        let pairs: Vec<(&str, &str)> = set.iter().map(|e| (e.key, e.hex)).collect();
        assert_eq!(
            pairs,
            vec![
                ("bg-primary", "#FFFFFF"),
                ("bg-secondary", "#F7F8FA"),
                ("bg-tertiary", "#FFFFFF"),
                ("bg-inverted", "#101012"),
                ("bg-grouped-primary", "#F7F8FA"),
                ("bg-grouped-secondary", "#FFFFFF"),
                ("bg-grouped-tertiary", "#F7F8FA"),
            ]
        );
    }

    #[test]
    fn neutral_dark_backgrounds_match_figma_exactly() {
        let set = resolve_background_set(ThemeContext::dark()).unwrap();
        let pairs: Vec<(&str, &str)> = set.iter().map(|e| (e.key, e.hex)).collect();
        assert_eq!(
            pairs,
            vec![
                ("bg-primary", "#101012"),
                ("bg-secondary", "#1C1C1E"),
                ("bg-tertiary", "#242426"),
                ("bg-inverted", "#FFFFFF"),
                ("bg-grouped-primary", "#101012"),
                ("bg-grouped-secondary", "#1C1C1E"),
                ("bg-grouped-tertiary", "#242426"),
            ]
        );
    }

    #[test]
    fn inverted_preserves_contrast_axis_and_flips_scheme() {
        let light_ic = ThemeContext::light_increased_contrast();
        assert_eq!(light_ic.inverted(), ThemeContext::dark_increased_contrast());

        assert_eq!(
            resolve_background(BackgroundRole::Inverted, ThemeContext::light())
                .unwrap()
                .hex,
            "#101012"
        );
        assert_eq!(
            resolve_background(BackgroundRole::Inverted, ThemeContext::dark())
                .unwrap()
                .hex,
            "#FFFFFF"
        );
    }

    #[test]
    fn increased_contrast_is_explicitly_reserved_not_aliased() {
        let err = resolve_background(
            BackgroundRole::Primary,
            ThemeContext::light_increased_contrast(),
        )
        .unwrap_err();
        assert_eq!(err, BackgroundError::IncreasedContrastNotCalibrated);
    }

    #[test]
    fn resolve_background_set_errors_on_increased_contrast() {
        let err = resolve_background_set(ThemeContext::dark_increased_contrast()).unwrap_err();
        assert_eq!(err, BackgroundError::IncreasedContrastNotCalibrated);
    }

    #[test]
    fn all_background_keys_are_unique_and_use_bg_prefix() {
        let mut seen = std::collections::HashSet::new();
        for role in BackgroundRole::ALL {
            let key = role.key();
            assert!(
                key.starts_with("bg-"),
                "{role:?} key must use bg-* prefix: {key}"
            );
            assert!(seen.insert(key), "duplicate background key {key}");
        }
        assert_eq!(seen.len(), BackgroundRole::ALL.len());
    }

    #[test]
    fn all_background_hexes_parse_as_valid_srgb() {
        use crate::spaces::srgb::srgb_from_hex;
        for context in [ThemeContext::light(), ThemeContext::dark()] {
            for entry in resolve_background_set(context).unwrap() {
                srgb_from_hex(entry.hex).unwrap_or_else(|reason| {
                    panic!("{} hex {} failed to parse: {reason}", entry.key, entry.hex)
                });
            }
        }
    }

    #[test]
    fn non_inverted_dark_backgrounds_are_darker_than_light_counterparts() {
        fn relative_luminance(hex: &str) -> f64 {
            let rgb = crate::spaces::srgb::srgb_from_hex(hex).unwrap();
            let channel = |linear: f64| {
                let c = crate::spaces::srgb::srgb_gamma(linear);
                if c <= 0.039_28 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
        }

        for role in BackgroundRole::ALL {
            if role == BackgroundRole::Inverted {
                continue;
            }
            let light = resolve_background(role, ThemeContext::light()).unwrap();
            let dark = resolve_background(role, ThemeContext::dark()).unwrap();
            let l_y = relative_luminance(light.hex);
            let d_y = relative_luminance(dark.hex);
            assert!(
                l_y > d_y,
                "{role:?}: light {} (Y={l_y}) must be lighter than dark {} (Y={d_y})",
                light.hex,
                dark.hex
            );
        }
    }

    #[test]
    fn backgrounds_are_inputs_to_the_foreground_role_solver() {
        let table = RoleTable::default();
        for context in [ThemeContext::light(), ThemeContext::dark()] {
            let vc = match context.scheme() {
                ColorScheme::Light => ViewingConditions::srgb(),
                ColorScheme::Dark => ViewingConditions::dim_surround(),
            };
            for role in BackgroundRole::ALL {
                let bg = background_input(role, context).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                for (fg_role, resolved) in &set {
                    match resolved {
                        Resolved::Color { solved, .. } => {
                            if let Some(floor) = table.legal_floor(*fg_role) {
                                assert!(
                                    solved.wcag_ratio() >= floor - 1e-9,
                                    "{} in {:?}: {} ratio {} below floor {floor}",
                                    role.key(),
                                    context,
                                    fg_role.key(),
                                    solved.wcag_ratio()
                                );
                            }
                        }
                        Resolved::None => {}
                        Resolved::Unreachable(reason) => panic!(
                            "{} in {:?}: {} unexpectedly unreachable: {reason}",
                            role.key(),
                            context,
                            fg_role.key()
                        ),
                    }
                }
            }
        }
    }

    #[test]
    fn foreground_role_set_does_not_include_background_tokens() {
        let keys: std::collections::HashSet<&str> = Role::ALL.iter().map(|r| r.key()).collect();
        for bg in BackgroundRole::ALL {
            assert!(
                !keys.contains(bg.key()),
                "{} belongs to background resolver, not semantic::Role::ALL",
                bg.key()
            );
        }
    }
}
