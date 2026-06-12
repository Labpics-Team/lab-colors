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
pub enum BindingError {
    /// The background hex string was not a valid `#RRGGBB` colour. Carries the
    /// core's own parse reason so the caller learns exactly what was wrong.
    #[error("invalid background colour: {reason}")]
    InvalidBackground {
        /// The reason the core's hex parser rejected the input.
        reason: String,
    },

    /// The theme string is not one of the public spellings.
    #[error("unknown theme: '{requested}' (expected light | dark | light-ic | dark-ic)")]
    UnknownTheme {
        /// The unrecognised theme string the caller passed.
        requested: String,
    },

    /// A recognised theme whose contrast table is not calibrated yet — the
    /// honest "reserved but absent" signal for the increased-contrast themes.
    #[error("theme '{theme}' is not yet calibrated")]
    ThemeNotCalibrated {
        /// The stable key of the reserved theme.
        theme: &'static str,
    },
}

impl BindingError {
    /// The stable, machine-readable code for this error — the contract a JS
    /// caller switches on. These never change without a versioned migration.
    pub fn code(&self) -> &'static str {
        match self {
            BindingError::InvalidBackground { .. } => "invalid_background",
            BindingError::UnknownTheme { .. } => "unknown_theme",
            BindingError::ThemeNotCalibrated { .. } => "theme_not_calibrated",
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
            BindingError::UnknownTheme {
                requested: "x".into(),
            },
            BindingError::ThemeNotCalibrated { theme: "light-ic" },
        ];
        let codes: Vec<_> = errors.iter().map(BindingError::code).collect();
        assert_eq!(
            codes,
            [
                "invalid_background",
                "unknown_theme",
                "theme_not_calibrated"
            ]
        );
    }
}
