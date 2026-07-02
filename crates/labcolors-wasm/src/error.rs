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

    /// The config JSON was rejected: parse error, unknown menu item, or a
    /// core validation/compile error (the full preflight message is carried).
    #[error("invalid config: {reason}")]
    InvalidConfig {
        /// The parse/validation reason, verbatim.
        reason: String,
    },

    /// A resolved colour failed to serialise into the oklch emission form.
    /// Unreachable by construction (solver/ladder hexes are valid) — carried
    /// as a structured code so JS branching stays uniform even for the
    /// impossible branch.
    #[error("internal error: {reason}")]
    Internal {
        /// What exactly failed to serialise.
        reason: String,
    },

    /// The theme string is not one of the public spellings.
    #[error("unknown theme: '{requested}' (expected light | dark | light-ic | dark-ic)")]
    UnknownTheme {
        /// The unrecognised theme string the caller passed.
        requested: String,
    },
}

impl BindingError {
    /// The stable, machine-readable code for this error — the contract a JS
    /// caller switches on. These never change without a versioned migration.
    pub fn code(&self) -> &'static str {
        match self {
            BindingError::InvalidBackground { .. } => "invalid_background",
            BindingError::InvalidConfig { .. } => "invalid_config",
            BindingError::UnknownTheme { .. } => "unknown_theme",
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
            BindingError::InvalidConfig { reason: "x".into() },
            BindingError::UnknownTheme {
                requested: "x".into(),
            },
            BindingError::Internal { reason: "x".into() },
        ];
        let codes: Vec<_> = errors.iter().map(BindingError::code).collect();
        assert_eq!(
            codes,
            [
                "invalid_background",
                "invalid_config",
                "unknown_theme",
                "internal_error"
            ]
        );
        // Distinctness, asserted directly so the test earns its name: a future
        // variant must not reuse an existing code.
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "error codes must be distinct");
    }
}
