//! Theme background roles: the surfaces a UI is resolved *on*, not foreground
//! roles resolved *against* a surface.
//!
//! `semantic::resolve_set(bg, ...)` answers "what do I draw on this background?".
//! This module answers the layer below it: "which background hex should this part
//! of the theme use?". Keeping the two separate prevents background tokens from
//! leaking into the foreground role set and invalidating the contrast golden.
//!
//! # Calibration
//!
//! Hex values in [`resolve_background`] were calibrated against Daniel's Figma
//! file `🧪Lab UI (v.1)` / `🔵 4.2 Semantic` collection / `Backgrounds/Neutral/*`
//! variables (2026-06-18, Daniel eye-sign-off). The light ladder alternates two
//! surfaces (`#FFFFFF` / `#F7F8FA`); the dark ladder uses three ascending steps
//! (`#101012` / `#1C1C1E` / `#242426`). Grouped variants alias the non-grouped
//! steps: they are the same physical colour, only a different semantic context.
//!
//! # Seams for future growth
//!
//! - **Static/adaptive** is a runtime inheritance decision. The core receives an
//!   already-resolved [`ThemeContext`]. A caller who needs static-light or
//!   static-dark backgrounds passes a fixed [`ColorScheme`] instead of inheriting
//!   the OS/user preference. No `Static*` role variants are needed.
//!
//! - **Inverted tiering** currently resolves to the primary of the opposite
//!   scheme (`BackgroundRole::Inverted`). The Figma semantic collection defines a
//!   single `Backgrounds/Neutral/Inverted`. If the design later requires separate
//!   `InvertedPrimary`/`InvertedSecondary`/`InvertedTertiary`, that will be added
//!   as new variants behind `#[non_exhaustive]` without breaking consumers.
//!
//! - **Sentiment backgrounds** (Brand/Danger/Warning/Success/Info) are out of
//!   scope for v1. When they land, they will reuse the same [`BackgroundRole`]
//!   enum with a separate `BackgroundIntent` (colour parameter), NOT a per-family
//!   variant explosion, keeping the role count constant regardless of how many
//!   colour families the system supports.

use crate::solve::{BgInput, Unreachable};

/// Light or dark colour scheme. Static/adaptive is intentionally not here: that is
/// a runtime inheritance decision. The core receives an already-resolved scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
pub enum BackgroundError {
    IncreasedContrastNotCalibrated,
    InvalidBackgroundHex {
        role: BackgroundRole,
        reason: String,
    },
}

impl core::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The current variant set is exhaustively matched above; the wildcard
        // arm covers future variants added behind `#[non_exhaustive]` from
        // outside this crate. The `allow` silences a false-positive warning
        // for the *internal* (in-crate) exhaustive case — outside the crate
        // the wildcard is reachable and mandatory.
        #[allow(unreachable_patterns)]
        match self {
            BackgroundError::IncreasedContrastNotCalibrated => {
                write!(f, "increased-contrast backgrounds are not calibrated yet")
            }
            BackgroundError::InvalidBackgroundHex { role, reason } => {
                write!(f, "{} background is invalid: {reason}", role.key())
            }
            _ => write!(f, "background resolution error"),
        }
    }
}
/// One resolved background token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundEntry {
    /// The role this entry resolves.
    pub role: BackgroundRole,
    /// The resolved surface colour. A `String` (not `&'static str`) so future
    /// sentiment backgrounds can carry runtime-derived values, not only
    /// compile-time literal hexes.
    pub hex: String,
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

    // Inverted flips the scheme: asking for the inverted surface on light gives
    // the dark primary, and vice versa. We normalise by rewriting the role
    // before the match, so the match sees a non-inverted role and stays
    // small and flat. If a future role variant is added, the `non_inverted`
    // assignment forces a deliberate decision about how it relates to
    // inversion.
    let (effective_scheme, effective_role) = if role == BackgroundRole::Inverted {
        (context.scheme().inverted(), BackgroundRole::Primary)
    } else {
        (context.scheme(), role)
    };

    let hex = match (effective_scheme, effective_role) {
        (ColorScheme::Light, BackgroundRole::Primary | BackgroundRole::Tertiary) => "#FFFFFF",
        (ColorScheme::Light, BackgroundRole::Secondary) => "#F7F8FA",
        (ColorScheme::Light, BackgroundRole::GroupedPrimary | BackgroundRole::GroupedTertiary) => {
            "#F7F8FA"
        }
        (ColorScheme::Light, BackgroundRole::GroupedSecondary) => "#FFFFFF",
        (ColorScheme::Dark, BackgroundRole::Primary | BackgroundRole::GroupedPrimary) => "#101012",
        (ColorScheme::Dark, BackgroundRole::Secondary | BackgroundRole::GroupedSecondary) => {
            "#1C1C1E"
        }
        (ColorScheme::Dark, BackgroundRole::Tertiary | BackgroundRole::GroupedTertiary) => {
            "#242426"
        }
        // #[non_exhaustive] — a future role variant (e.g. Quaternary) lands
        // here. It is NOT in the Figma v1 ladder, so the honest answer is
        // the closest visible surface: the primary of the effective scheme.
        _ => match effective_scheme {
            ColorScheme::Light => "#FFFFFF",
            ColorScheme::Dark => "#101012",
        },
    };

    Ok(BackgroundEntry {
        role,
        hex: hex.to_owned(),
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
    BgInput::solid(&entry.hex).map_err(|reason: Unreachable| {
        BackgroundError::InvalidBackgroundHex {
            role,
            reason: reason.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{Resolved, Role, RoleTable, resolve_set};
    use crate::spaces::vc::ViewingConditions;

    #[test]
    fn neutral_light_backgrounds_match_figma_exactly() {
        let set = resolve_background_set(ThemeContext::light()).unwrap();
        let pairs: Vec<(&str, &str)> = set.iter().map(|e| (e.role.key(), e.hex.as_str())).collect();
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
        let pairs: Vec<(&str, &str)> = set.iter().map(|e| (e.role.key(), e.hex.as_str())).collect();
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
        // The contrast axis is *preserved* through inversion: flipping scheme
        // must NOT change contrast. If this ever flips, Inverted backgrounds
        // would carry the wrong contrast regime.
        for (ctx, expected_inverse) in [
            (ThemeContext::light(), ThemeContext::dark()),
            (ThemeContext::dark(), ThemeContext::light()),
            (
                ThemeContext::light_increased_contrast(),
                ThemeContext::dark_increased_contrast(),
            ),
            (
                ThemeContext::dark_increased_contrast(),
                ThemeContext::light_increased_contrast(),
            ),
        ] {
            assert_eq!(ctx.inverted(), expected_inverse);
        }

        // The hex flips: the Inverted surface of the light scheme IS the
        // primary of the dark scheme, and vice versa.
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

        // Other roles are NOT affected by the inversion logic — Primary on
        // light is the same whether you ask for it via direct call or
        // implicit through some hypothetical path. Guards against future
        // refactors that move the Inverted rewrite up the stack.
        assert_eq!(
            resolve_background(BackgroundRole::Primary, ThemeContext::light())
                .unwrap()
                .hex,
            "#FFFFFF"
        );
        assert_eq!(
            resolve_background(BackgroundRole::Primary, ThemeContext::dark())
                .unwrap()
                .hex,
            "#101012"
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
                srgb_from_hex(&entry.hex).unwrap_or_else(|reason| {
                    panic!(
                        "{} hex {} failed to parse: {reason}",
                        entry.role.key(),
                        entry.hex
                    )
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
            let l_y = relative_luminance(&light.hex);
            let d_y = relative_luminance(&dark.hex);
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
