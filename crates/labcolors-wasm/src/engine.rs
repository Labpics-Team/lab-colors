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
                    outcome: map_resolved(resolved, self.table.legal_floor(role)),
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

    /// Recheck the contrasts a set of foreground colours achieve against a
    /// (possibly changed) `bg_hex` under `theme` — the cheap per-frame primitive
    /// of the reactive runtime. One CAM16 forward for the background plus one per
    /// foreground, **no solve**: the controller keeps current colours while they
    /// still pass and re-solves only the rare role that stably fails.
    ///
    /// Returns a flat, interleaved buffer `[lc0, wcag0, lc1, wcag1, …]` (mapped to
    /// a JS `Float64Array`) — no per-call object allocation on the hot path. The
    /// values equal what the solver measured, so a freshly-resolved set rechecks
    /// to its own reported contrasts.
    pub fn recheck(
        &self,
        bg_hex: &str,
        fg_hexes: &[String],
        theme: Theme,
    ) -> Result<Vec<f64>, BindingError> {
        let vc = theme.viewing_conditions()?;
        let bg = normalise_hex(bg_hex)?;
        // Normalise foregrounds through the same parser as the background and
        // `resolveTheme`, so the three entry points agree on what a valid hex is
        // (`#RGB` shorthand, missing `#`, any case) instead of the core's
        // stricter 6-digit-only parse rejecting a shorthand a resolve accepted.
        let normalised: Vec<String> = fg_hexes
            .iter()
            .map(|h| normalise_hex(h))
            .collect::<Result<_, _>>()?;
        let refs: Vec<&str> = normalised.iter().map(String::as_str).collect();
        let pairs = labcolors_core::recheck_against(&bg, &refs, &vc)
            .map_err(|reason| BindingError::InvalidBackground { reason })?;
        let mut out = Vec::with_capacity(pairs.len() * 2);
        for (lc, wcag) in pairs {
            out.push(lc);
            out.push(wcag);
        }
        Ok(out)
    }
}

/// Map one core [`Resolved`] into the boundary [`RoleOutcome`]. `legal_floor` is
/// the role's WCAG clamp (from the role table), carried onto a solved colour.
fn map_resolved(resolved: Resolved, legal_floor: Option<f64>) -> RoleOutcome {
    match resolved {
        Resolved::Color { solved, compressed } => {
            RoleOutcome::Color(map_solved(solved, compressed, legal_floor))
        }
        Resolved::None => RoleOutcome::None,
        Resolved::Unreachable(reason) => RoleOutcome::Unreachable {
            code: unreachable_code(&reason),
            message: reason.to_string(),
        },
    }
}

fn map_solved(solved: Solved, compressed: bool, legal_floor: Option<f64>) -> SolvedColor {
    SolvedColor {
        hex: solved.hex().to_owned(),
        lc: solved.lc(),
        wcag_ratio: solved.wcag_ratio(),
        compressed,
        floor_override: solved.floor_override(),
        legal_floor,
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
        assert!(keys.contains(&"label-primary"));
        assert!(keys.contains(&"none"));
    }

    #[test]
    fn recheck_matches_resolve_theme_reported_contrasts() {
        // The WASM recheck end-to-end: resolve a set, then recheck each solved
        // colour against its OWN background — the returned interleaved (lc, wcag)
        // pairs must equal exactly what `resolve_theme` reported. This is the
        // identity the reactive controller stands on: "still passes?" means the
        // same thing as the original solve.
        let engine = Engine::new();
        for (bg, theme) in [
            ("#FFFFFF", Theme::Light),
            ("#3478F6", Theme::Light),
            ("#1C1C1E", Theme::Dark),
        ] {
            let result = engine.resolve_theme(bg, theme).unwrap();
            let mut fgs = Vec::new();
            let mut want = Vec::new();
            for r in &result.roles {
                if let RoleOutcome::Color(c) = &r.outcome {
                    fgs.push(c.hex.clone());
                    want.push((c.lc, c.wcag_ratio));
                }
            }
            let flat = engine.recheck(bg, &fgs, theme).unwrap();
            assert_eq!(flat.len(), want.len() * 2);
            for (i, (lc, wcag)) in want.iter().enumerate() {
                assert!((flat[2 * i] - lc).abs() < 1e-9, "{bg}: role {i} lc drift");
                assert!(
                    (flat[2 * i + 1] - wcag).abs() < 1e-9,
                    "{bg}: role {i} wcag drift"
                );
            }
        }
        // Invalid foreground hex surfaces a structured error, not a panic.
        assert!(
            Engine::new()
                .recheck("#FFFFFF", &["nothex".to_string()], Theme::Light)
                .is_err()
        );
    }

    #[test]
    fn recheck_accepts_the_same_hex_forms_as_resolve_theme() {
        // The three entry points share one hex contract: `#RGB` shorthand, a
        // missing `#`, and mixed case are all accepted by recheck exactly as by
        // resolve — and every spelling of a colour rechecks bit-identically.
        // `#123` and `#112233` are the SAME colour (each nibble is doubled), and
        // `#fff` is `#FFFFFF`, so all of these must agree with the canonical form.
        let engine = Engine::new();
        let canonical = engine
            .recheck("#FFFFFF", &["#112233".to_string()], Theme::Light)
            .unwrap();
        for bg in ["#fff", "FFFFFF", "#FFFFFF"] {
            for fg in ["#123", "112233", "#112233"] {
                let got = engine.recheck(bg, &[fg.to_string()], Theme::Light).unwrap();
                assert_eq!(got.len(), 2, "{bg}/{fg}: one (lc, wcag) pair");
                assert_eq!(got, canonical, "{bg}/{fg}: must match the canonical form");
            }
        }
    }

    #[test]
    fn none_role_resolves_to_none_outcome() {
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let none_entry = result.roles.iter().find(|r| r.role_key == "none").unwrap();
        assert_eq!(none_entry.outcome, RoleOutcome::None);
    }

    #[test]
    fn label_primary_on_white_is_a_dark_colour() {
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let tp = result
            .roles
            .iter()
            .find(|r| r.role_key == "label-primary")
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
    fn legal_floor_rides_along_on_solved_colours() {
        // The DTO carries each role's legal WCAG clamp so the runtime can hold
        // the floor while easing. Anchored roles report their conformance ratio;
        // decorative / zero roles report None.
        let engine = Engine::new();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let floor_of = |key: &str| {
            result
                .roles
                .iter()
                .find(|r| r.role_key == key)
                .map(|r| &r.outcome)
        };
        // AA text role → 4.5.
        match floor_of("label-primary") {
            Some(RoleOutcome::Color(c)) => assert_eq!(c.legal_floor, Some(4.5)),
            other => panic!("label-primary expected solved, got {other:?}"),
        }
        // AA UI role → 3.0.
        match floor_of("icon") {
            Some(RoleOutcome::Color(c)) => assert_eq!(c.legal_floor, Some(3.0)),
            other => panic!("icon expected solved, got {other:?}"),
        }
        // Decorative / JND roles carry no legal floor even when solved.
        if let Some(RoleOutcome::Color(c)) = floor_of("label-quaternary") {
            assert_eq!(c.legal_floor, None);
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
