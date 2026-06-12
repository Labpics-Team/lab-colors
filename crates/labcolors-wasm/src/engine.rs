//! The application core of the bindings: resolve a background under a theme,
//! generically over whatever role set the core provides.
//!
//! This layer knows the core and the DTOs; it does NOT know wasm-bindgen. It
//! holds the role table and the contract cache, runs `resolve_set`, and maps
//! the core's `Vec<(Role, Resolved)>` into [`ResolvedTheme`]. The mapping never
//! enumerates roles — it walks the vector the core returns and keys each entry
//! by `Role::key()` — so issue #59's role growth flows through on a rebuild.

use std::rc::Rc;

use labcolors_core::{BgInput, Resolved, RoleTable, Solved, Unreachable};

use crate::cache::{CacheKey, ContractCache, DEFAULT_TABLE_FINGERPRINT};
use crate::dto::{ResolvedTheme, RoleEntry, RoleOutcome, SolvedColor};
use crate::error::BindingError;
use crate::theme::Theme;

/// How many distinct `(bg, theme, table)` resolves the cache holds before a
/// wholesale clear. A few thousand entries at well under 1 MB — generous for a
/// design tool sweeping backgrounds, bounded so memory cannot run away.
const CACHE_CAPACITY: usize = 4096;

/// A configured, caching contrast engine.
///
/// Construct once (`init`), call [`resolve_theme`](Self::resolve_theme) many
/// times. Zero-config: the default role table and the per-theme default viewing
/// conditions are baked in. The result is cached behind an `Rc` so a cache hit
/// is a cheap reference-count bump, not a re-clone of the whole set.
pub struct Engine {
    table: RoleTable,
    table_fingerprint: u64,
    cache: ContractCache<Rc<ResolvedTheme>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// A zero-config engine on the default role table.
    pub fn new() -> Self {
        Self {
            table: RoleTable::default(),
            table_fingerprint: DEFAULT_TABLE_FINGERPRINT,
            cache: ContractCache::new(CACHE_CAPACITY),
        }
    }

    /// Resolve every role for `bg_hex` under `theme`, returning the shared
    /// result. Repeated identical calls hit the contract cache.
    ///
    /// Errors (bad hex, unknown/uncalibrated theme) are returned, never
    /// panicked. Per-role unreachability is part of a *successful* result.
    pub fn resolve_theme(
        &self,
        bg_hex: &str,
        theme: Theme,
    ) -> Result<Rc<ResolvedTheme>, BindingError> {
        let vc = theme.viewing_conditions()?;
        // Validate and normalise the background once, before the cache lookup,
        // so an invalid hex fails fast and the cache key is canonical.
        let normalised = normalise_hex(bg_hex)?;
        let bg = BgInput::solid(&normalised).map_err(|u| BindingError::InvalidBackground {
            reason: u.to_string(),
        })?;

        let key = CacheKey::new(normalised.clone(), theme, self.table_fingerprint);
        let result = self.cache.get_or_insert_with(key, || {
            let set = labcolors_core::resolve_set(&bg, &self.table, &vc);
            let roles = set
                .into_iter()
                .map(|(role, resolved)| RoleEntry {
                    role_key: role.key(),
                    outcome: map_resolved(resolved),
                })
                .collect();
            Rc::new(ResolvedTheme {
                theme: theme.key(),
                background: normalised.clone(),
                roles,
            })
        });
        Ok(result)
    }
}

/// Map one core [`Resolved`] into the boundary [`RoleOutcome`].
fn map_resolved(resolved: Resolved) -> RoleOutcome {
    match resolved {
        Resolved::Color { solved, compressed } => {
            RoleOutcome::Color(map_solved(solved, compressed))
        }
        Resolved::None => RoleOutcome::None,
        Resolved::Unreachable(reason) => RoleOutcome::Unreachable {
            code: unreachable_code(&reason),
            message: reason.to_string(),
        },
    }
}

fn map_solved(solved: Solved, compressed: bool) -> SolvedColor {
    SolvedColor {
        hex: solved.hex().to_owned(),
        lc: solved.lc(),
        wcag_ratio: solved.wcag_ratio(),
        compressed,
        floor_override: solved.floor_override(),
    }
}

/// A stable machine code for each unreachability reason.
///
/// `Unreachable` is `#[non_exhaustive]`, so the catch-all is mandatory and
/// honest: a core variant we have not mapped yet reports `"unreachable"` rather
/// than failing to compile against a future core. Known variants get a specific
/// code so a JS caller can branch on the cause.
fn unreachable_code(reason: &Unreachable) -> &'static str {
    match reason {
        Unreachable::BelowContrastFloor { .. } => "below_contrast_floor",
        Unreachable::ExceedsRange { .. } => "exceeds_range",
        Unreachable::QuantizationGap { .. } => "quantization_gap",
        Unreachable::FloorUnreachable { .. } => "floor_unreachable",
        Unreachable::PolarityMismatch { .. } => "polarity_mismatch",
        Unreachable::GamutUnsupported => "gamut_unsupported",
        Unreachable::UnsupportedBackground => "unsupported_background",
        Unreachable::InvalidInput(_) => "invalid_input",
        _ => "unreachable",
    }
}

/// Normalise a background hex to the canonical `#RRGGBB` upper-case form the
/// cache keys on. Accepts `#`-led or bare, 3- or 6-digit; rejects anything else
/// with the core's own parse vocabulary so the message matches `BgInput::solid`.
fn normalise_hex(raw: &str) -> Result<String, BindingError> {
    let body = raw.strip_prefix('#').unwrap_or(raw);
    let expanded = match body.len() {
        3 => body.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => body.to_owned(),
        _ => {
            return Err(BindingError::InvalidBackground {
                reason: format!("expected #RGB or #RRGGBB, got '{raw}'"),
            });
        }
    };
    if !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BindingError::InvalidBackground {
            reason: format!("non-hex digit in '{raw}'"),
        });
    }
    Ok(format!("#{}", expanded.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_short_and_cased_hex() {
        assert_eq!(normalise_hex("#fff").unwrap(), "#FFFFFF");
        assert_eq!(normalise_hex("abcdef").unwrap(), "#ABCDEF");
        assert_eq!(normalise_hex("#1A2B3C").unwrap(), "#1A2B3C");
    }

    #[test]
    fn rejects_malformed_hex_with_reason() {
        assert!(matches!(
            normalise_hex("#12"),
            Err(BindingError::InvalidBackground { .. })
        ));
        assert!(matches!(
            normalise_hex("#gggggg"),
            Err(BindingError::InvalidBackground { .. })
        ));
    }

    #[test]
    fn resolves_white_light_to_keyed_entries() {
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert_eq!(result.theme, "light");
        assert_eq!(result.background, "#FFFFFF");
        // Generic over the role set: at least the v1 roles are present, each
        // keyed by Role::key(). We assert the keys exist, not their count, so
        // issue #59's growth does not break this test.
        let keys: Vec<_> = result.roles.iter().map(|r| r.role_key).collect();
        assert!(keys.contains(&"text-primary"));
        assert!(keys.contains(&"none"));
    }

    #[test]
    fn none_role_resolves_to_none_outcome() {
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let none_entry = result.roles.iter().find(|r| r.role_key == "none").unwrap();
        assert_eq!(none_entry.outcome, RoleOutcome::None);
    }

    #[test]
    fn text_primary_on_white_is_a_dark_colour() {
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let tp = result
            .roles
            .iter()
            .find(|r| r.role_key == "text-primary")
            .unwrap();
        match &tp.outcome {
            RoleOutcome::Color(c) => {
                assert!(c.wcag_ratio >= 4.5, "primary text must clear AA on white");
                assert!(c.lc.abs() > 50.0, "primary text should be strong contrast");
            }
            other => panic!("expected a solved colour, got {other:?}"),
        }
    }

    #[test]
    fn cache_returns_identical_shared_result() {
        let engine = Engine::new();
        let first = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let second = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert!(
            Rc::ptr_eq(&first, &second),
            "second call must be a cache hit"
        );
    }

    #[test]
    fn cache_key_is_hex_normalised() {
        let engine = Engine::new();
        let canonical = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let shorthand = engine.resolve_theme("#fff", Theme::Light).unwrap();
        assert!(
            Rc::ptr_eq(&canonical, &shorthand),
            "equivalent hex spellings must share a cache entry"
        );
    }

    #[test]
    fn uncalibrated_theme_is_an_error_not_a_panic() {
        let engine = Engine::new();
        assert!(matches!(
            engine.resolve_theme("#FFFFFF", Theme::LightIncreasedContrast),
            Err(BindingError::ThemeNotCalibrated { .. })
        ));
    }
}
