//! Structured, matchable errors for the binding boundary.
//!
//! This is a library crate, so errors are `thiserror` enums callers can match
//! on — not opaque strings. They cross into JS as a *structured* error object
//! (a `code` plus a human `message`), never as a thrown panic or an unwound
//! stack. The engine's hot path returns these as values; the only `throw` is at
//! the top-level wasm adapter for whole-call failures (bad hex, unknown theme),
//! which is the JS-idiomatic place for a rejected input.

use thiserror::Error;

/// A reason a binding call could not produce a result.
///
/// Per-role unreachability is *not* here — that is a successful resolve whose
/// individual entries carry their own reason (see [`crate::dto`]). This enum is
/// for failures of the call as a whole.
#[derive(Error, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BindingError {
    /// The background hex string was not a valid `#RRGGBB` colour. Carries the
    /// core's own parse reason so the caller learns exactly what was wrong.
    #[error("invalid background colour: {reason}")]
    InvalidBackground {
        /// The reason the core's hex parser rejected the input.
        reason: String,
    },

    /// A non-background colour argument was not valid encoded sRGB hex.
    #[error("invalid colour: {reason}")]
    InvalidColor {
        /// The core parse/domain reason.
        reason: String,
    },

    /// The config JSON was rejected: parse error, unknown menu item, or a
    /// core validation/compile error (the full preflight message is carried).
    #[error("invalid config: {reason}")]
    InvalidConfig {
        /// The parse/validation reason, verbatim.
        reason: String,
    },

    /// `resolve_theme` was called before any config was loaded. The engine is
    /// agnostic (ADR-0001 PR-c): it carries no built-in design system, so a
    /// resolve has nothing to emit until `load_config` supplies one. Honest,
    /// matchable failure — never a panic and never a silent built-in default.
    #[error("no config loaded: call load_config before resolve_theme")]
    ConfigRequired,

    /// A core-generated value violated an internal postcondition or the adapter
    /// could not represent a known/forward core variant without losing meaning.
    /// Includes projection/oklch serialization failures and stable-Glow recheck
    /// failures after public inputs were already validated. Never client blame.
    #[error("internal error: {reason}")]
    Internal {
        /// The internal postcondition, projection, or contract mismatch.
        reason: String,
    },

    /// The theme string is not one of the public spellings.
    #[error("unknown theme: '{requested}' (expected light | dark | light-ic | dark-ic)")]
    UnknownTheme {
        /// The unrecognised theme string the caller passed.
        requested: String,
    },

    /// WCAG 2.2 criterion transport is outside the closed public menu.
    #[error(
        "unknown WCAG22 criterion: '{requested}' (expected sc-1.4.3-text-default | sc-1.4.3-text-large-scale | sc-1.4.11-ui-component-or-state | sc-1.4.11-graphical-object)"
    )]
    UnknownWcag22Criterion {
        /// Unrecognised criterion key.
        requested: String,
    },
}

impl BindingError {
    /// The stable, machine-readable code for this error — the contract a JS
    /// caller switches on. These never change without a versioned migration.
    pub fn code(&self) -> &'static str {
        match self {
            BindingError::InvalidBackground { .. } => "invalid_background",
            BindingError::InvalidColor { .. } => "invalid_color",
            BindingError::InvalidConfig { .. } => "invalid_config",
            BindingError::ConfigRequired => "config_required",
            BindingError::UnknownTheme { .. } => "unknown_theme",
            BindingError::UnknownWcag22Criterion { .. } => "unknown_wcag22_criterion",
            BindingError::Internal { .. } => "internal_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let errors = [
            BindingError::InvalidBackground { reason: "x".into() },
            BindingError::InvalidColor { reason: "x".into() },
            BindingError::InvalidConfig { reason: "x".into() },
            BindingError::ConfigRequired,
            BindingError::UnknownTheme {
                requested: "x".into(),
            },
            BindingError::UnknownWcag22Criterion {
                requested: "x".into(),
            },
            BindingError::Internal { reason: "x".into() },
        ];
        let codes: Vec<_> = errors.iter().map(BindingError::code).collect();
        assert_eq!(
            codes,
            [
                "invalid_background",
                "invalid_color",
                "invalid_config",
                "config_required",
                "unknown_theme",
                "unknown_wcag22_criterion",
                "internal_error"
            ]
        );
        // Distinctness, asserted directly so the test earns its name: a future
        // variant must not reuse an existing code.
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "error codes must be distinct");
    }
}
